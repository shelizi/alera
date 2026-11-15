//! Bounded Alacritty Terminal ANSI/VT state for GPUI.
//!
//! The native GPUI frontend renders the mature alacritty_terminal grid through
//! the existing StyledRow boundary. Classic Flutter, Host/PTY ownership, and
//! authenticated Session management are deliberately left unchanged. The
//! preview cannot create, attach, resize or control the Host by itself.

use std::ops::Range;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{Config, Osc52, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Rgb};

/// Match Classic's default; the Native preference remains local, and Host PTY
/// retention in bytes is separately controlled by the Runtime Host.
pub const DEFAULT_SCROLLBACK_LINES: usize = 10_000;
pub const MIN_SCROLLBACK_LINES: usize = 100;
pub const MAX_SCROLLBACK_LINES: usize = 200_000;
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;
const MAX_RESIZE_CSI_BYTES: usize = 32;
pub const TERMINAL_DEFAULT_FG: u32 = 0xe1e4e8;
pub const TERMINAL_DEFAULT_BG: u32 = 0x15171b;
const CURSOR_FOREGROUND: u32 = 0x15171b;
const CURSOR_BACKGROUND: u32 = 0x93c5fd;

/// Materialized terminal attributes, independent of GPUI. Colors are RGB,
/// not escape sequences. Each style includes a cursor flag for testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellStyle {
    pub fg: u32,
    pub bg: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub cursor: bool,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: TERMINAL_DEFAULT_FG,
            bg: TERMINAL_DEFAULT_BG,
            bold: false,
            italic: false,
            underline: false,
            cursor: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSpan {
    /// UTF-8 byte range into the row text, never a character index.
    pub range: Range<usize>,
    pub style: CellStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledRow {
    pub text: String,
    pub spans: Vec<StyleSpan>,
}

fn indexed_rgb(index: u8) -> u32 {
    const ANSI: [u32; 16] = [
        0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5, 0x666666,
        0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff,
    ];
    match index {
        0..=15 => ANSI[index as usize],
        16..=231 => {
            let n = index - 16;
            let rgb_component = |v: u8| if v == 0 { 0 } else { 55 + 40 * u32::from(v) };
            (rgb_component(n / 36) << 16) | (rgb_component((n / 6) % 6) << 8) | rgb_component(n % 6)
        }
        232..=255 => {
            let g = 8 + 10 * u32::from(index - 232);
            (g << 16) | (g << 8) | g
        }
    }
}

fn rgb_value(rgb: Rgb) -> u32 {
    (u32::from(rgb.r) << 16) | (u32::from(rgb.g) << 8) | u32::from(rgb.b)
}

fn color_rgb(color: Color, colors: &Colors, default: u32) -> u32 {
    match color {
        Color::Spec(rgb) => rgb_value(rgb),
        Color::Indexed(index) => colors[usize::from(index)]
            .map(rgb_value)
            .unwrap_or_else(|| indexed_rgb(index)),
        Color::Named(named) => {
            if let Some(value) = colors[named] {
                return rgb_value(value);
            }
            match named {
                NamedColor::Foreground
                | NamedColor::BrightForeground
                | NamedColor::DimForeground => TERMINAL_DEFAULT_FG,
                NamedColor::Background => TERMINAL_DEFAULT_BG,
                NamedColor::Cursor => CURSOR_BACKGROUND,
                _ => {
                    let index = named as usize;
                    if index < 16 {
                        indexed_rgb(index as u8)
                    } else if (259..=266).contains(&index) {
                        indexed_rgb((index - 259) as u8)
                    } else {
                        default
                    }
                }
            }
        }
    }
}

fn cell_style(cell: &Cell, colors: &Colors, cursor: bool) -> CellStyle {
    let mut fg = color_rgb(cell.fg, colors, TERMINAL_DEFAULT_FG);
    let mut bg = color_rgb(cell.bg, colors, TERMINAL_DEFAULT_BG);
    if cell.flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cursor {
        fg = CURSOR_FOREGROUND;
        bg = CURSOR_BACKGROUND;
    }
    CellStyle {
        fg,
        bg,
        bold: cell.flags.contains(Flags::BOLD),
        italic: cell.flags.contains(Flags::ITALIC),
        underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
        cursor,
    }
}

/// Tracks only the bounded ConPTY/xterm window resize report
/// CSI 8;rows;cols t across incremental Host reads. Other control sequences
/// stay the responsibility of the Alacritty parser; this probe never consumes or
/// rewrites them and cannot trigger a Host resize.
#[derive(Default)]
struct ConptyResizeProbe {
    candidate: Vec<u8>,
}

impl ConptyResizeProbe {
    fn observe(&mut self, byte: u8) -> Option<(u16, u16)> {
        if byte == b'\x1b' {
            self.candidate.clear();
            self.candidate.push(byte);
            return None;
        }
        if self.candidate.is_empty() {
            return None;
        }
        self.candidate.push(byte);
        let candidate = self.candidate.as_slice();
        const PREFIX: &[u8] = b"\x1b[8;";
        if candidate.len() > MAX_RESIZE_CSI_BYTES
            || (candidate.len() <= PREFIX.len() && !PREFIX.starts_with(candidate))
        {
            self.candidate.clear();
            return None;
        }
        if candidate.len() <= PREFIX.len() {
            return None;
        }
        let args = &candidate[PREFIX.len()..];
        if args.last() == Some(&b't') {
            let payload = &args[..args.len() - 1];
            let mut values = payload.split(|byte| *byte == b';');
            let rows = values.next().and_then(parse_resize_number);
            let cols = values.next().and_then(parse_resize_number);
            let result = if values.next().is_none() {
                rows.zip(cols)
            } else {
                None
            };
            self.candidate.clear();
            return result;
        }
        if args
            .iter()
            .any(|byte| !byte.is_ascii_digit() && *byte != b';')
            || args.iter().filter(|byte| **byte == b';').count() > 1
        {
            self.candidate.clear();
        }
        None
    }
}

fn parse_resize_number(bytes: &[u8]) -> Option<u16> {
    (!bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit))
        .then(|| std::str::from_utf8(bytes).ok()?.parse::<u16>().ok())
        .flatten()
        .filter(|value| *value > 0)
}

#[derive(Debug, Clone, Copy)]
struct PreviewDimensions {
    rows: usize,
    cols: usize,
}

impl Dimensions for PreviewDimensions {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// GPUI-compatible screen facade over the Alacritty grid and VTE processor.
///
/// This layer materializes visible RGB spans but never owns or starts PTYs.
pub struct VtScreen {
    terminal: Term<VoidListener>,
    processor: Processor,
    rows: u16,
    cols: u16,
    scrollback_lines: usize,
    resize_probe: ConptyResizeProbe,
}

impl std::fmt::Debug for VtScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VtScreen")
            .field("backend", &"alacritty_terminal")
            .field("rows", &self.rows)
            .field("cols", &self.cols)
            .finish_non_exhaustive()
    }
}

impl Default for VtScreen {
    fn default() -> Self {
        Self::new(DEFAULT_ROWS, DEFAULT_COLS)
    }
}

impl VtScreen {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self::with_scrollback(rows, cols, DEFAULT_SCROLLBACK_LINES)
    }

    pub fn with_scrollback(rows: u16, cols: u16, scrollback_lines: usize) -> Self {
        let scrollback_lines = scrollback_lines.clamp(MIN_SCROLLBACK_LINES, MAX_SCROLLBACK_LINES);
        let rows = rows.clamp(5, 200);
        let cols = cols.clamp(20, 512);
        let config = Config {
            scrolling_history: scrollback_lines,
            // A Host transcript must never trigger a GPUI clipboard read/write.
            osc52: Osc52::Disabled,
            ..Config::default()
        };
        let dims = PreviewDimensions {
            rows: usize::from(rows),
            cols: usize::from(cols),
        };
        let terminal = Term::new(config, &dims, VoidListener);
        Self {
            terminal,
            processor: Processor::new(),
            rows,
            cols,
            scrollback_lines,
            resize_probe: ConptyResizeProbe::default(),
        }
    }

    pub fn feed(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut start = 0;
        for (offset, byte) in bytes.iter().copied().enumerate() {
            if let Some((rows, cols)) = self.resize_probe.observe(byte) {
                // Parse ConPTY's resize notification at the old size, then
                // adjust the local grid before processing the following redraw.
                // Incremental reads may split the CSI across several chunks.
                self.processor
                    .advance(&mut self.terminal, &bytes[start..=offset]);
                self.set_preview_size(rows, cols);
                start = offset + 1;
            }
        }
        self.processor.advance(&mut self.terminal, &bytes[start..]);
    }

    pub fn reset(&mut self) {
        *self = Self::with_scrollback(self.rows, self.cols, self.scrollback_lines);
    }

    pub fn scrollback_lines(&self) -> usize {
        self.scrollback_lines
    }

    /// Update the Alacritty ring in place. This applies to existing sessions
    /// without dropping their screen, dirty input, active PTY or ANSI state.
    pub fn set_scrollback_lines(&mut self, lines: usize) {
        let lines = lines.clamp(MIN_SCROLLBACK_LINES, MAX_SCROLLBACK_LINES);
        if lines == self.scrollback_lines {
            return;
        }
        self.scrollback_lines = lines;
        self.terminal.set_options(Config {
            scrolling_history: lines,
            osc52: Osc52::Disabled,
            ..Config::default()
        });
    }

    pub fn scrollback_offset(&self) -> usize {
        self.terminal.grid().display_offset()
    }

    pub fn scrollback_available(&self) -> usize {
        self.terminal.grid().history_size()
    }

    /// Scroll only the standard terminal buffer, never a full-screen TUI's
    /// alternate screen. Returning false lets the normal PTY key path work.
    pub fn scroll_history_page(&mut self, toward_older: bool) -> bool {
        if self.alternate_screen() {
            return false;
        }
        let before = self.scrollback_offset();
        self.terminal.scroll_display(if toward_older {
            Scroll::PageUp
        } else {
            Scroll::PageDown
        });
        self.scrollback_offset() != before
    }

    pub fn scroll_to_live(&mut self) -> bool {
        if self.alternate_screen() {
            return false;
        }
        let before = self.scrollback_offset();
        self.terminal.scroll_display(Scroll::Bottom);
        self.scrollback_offset() != before
    }

    /// Visible text snapshot only. Escape and OSC control sequences cannot
    /// become text; OSC52 is disabled at the engine configuration boundary.
    pub fn contents(&self) -> String {
        let rows = self.styled_rows();
        let mut lines: Vec<&str> = rows
            .iter()
            .map(|row| row.text.trim_end_matches(' '))
            .collect();
        while lines.last().is_some_and(|row| row.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// Materialize the Alacritty visible grid as GPUI styled UTF-8 spans.
    /// Skip wide continuation cells, retaining any combining marks in the
    /// leading cell. Row/span work is bounded by the local 200 x 512 preview.
    pub fn styled_rows(&self) -> Vec<StyledRow> {
        let grid = self.terminal.grid();
        let colors = self.terminal.colors();
        let cursor = grid.cursor.point;
        let offset = self.terminal.grid().display_offset() as i32;
        let show_cursor = offset == 0 && self.terminal.mode().contains(TermMode::SHOW_CURSOR);
        (0..self.rows)
            .map(|row| {
                let mut text = String::with_capacity(usize::from(self.cols) * 2);
                let mut spans: Vec<StyleSpan> = Vec::new();
                for col in 0..self.cols {
                    let cell = &grid[Line(i32::from(row) - offset)][Column(usize::from(col))];
                    if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                        continue;
                    }
                    let is_cursor = show_cursor
                        && cursor.line.0 == i32::from(row)
                        && (cursor.column.0 == usize::from(col)
                            || (cursor.column.0 == usize::from(col) + 1
                                && cell.flags.contains(Flags::WIDE_CHAR)));
                    let style = cell_style(cell, colors, is_cursor);
                    let start = text.len();
                    if cell.flags.contains(Flags::HIDDEN) {
                        text.push(' ');
                    } else {
                        text.push(cell.c);
                        if let Some(zerowidth) = cell.zerowidth() {
                            text.extend(zerowidth.iter().copied());
                        }
                    }
                    let end = text.len();
                    if let Some(last) = spans.last_mut() {
                        if last.style == style {
                            last.range.end = end;
                            continue;
                        }
                    }
                    spans.push(StyleSpan {
                        range: start..end,
                        style,
                    });
                }
                StyledRow { text, spans }
            })
            .collect()
    }

    pub fn cursor(&self) -> (u16, u16) {
        let point = self.terminal.grid().cursor.point;
        (point.line.0.max(0) as u16, point.column.0 as u16)
    }

    pub fn alternate_screen(&self) -> bool {
        self.terminal.mode().contains(TermMode::ALT_SCREEN)
    }

    pub fn bracketed_paste(&self) -> bool {
        self.terminal.mode().contains(TermMode::BRACKETED_PASTE)
    }

    pub fn application_cursor_keys(&self) -> bool {
        self.terminal.mode().contains(TermMode::APP_CURSOR)
    }

    pub fn dimensions(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// Local preview size only. Never assert to Host that it was resized.
    pub fn set_preview_size(&mut self, rows: u16, cols: u16) {
        let rows = rows.clamp(5, 200);
        let cols = cols.clamp(20, 512);
        if self.rows != rows || self.cols != cols {
            self.terminal.resize(PreviewDimensions {
                rows: usize::from(rows),
                cols: usize::from(cols),
            });
            self.rows = rows;
            self.cols = cols;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollback_lines_are_real_bounded_history_not_just_a_ui_preference() {
        let mut screen = VtScreen::with_scrollback(5, 20, 100);
        assert_eq!(screen.scrollback_lines(), 100);
        for i in 0..140 {
            screen.feed(&format!("line{i:03}\r\n"));
        }
        assert_eq!(screen.scrollback_available(), 100);
        assert_eq!(screen.scrollback_offset(), 0);
        let live = screen.contents();
        assert!(live.contains("line139"));
        assert!(screen.scroll_history_page(true));
        assert!(screen.scrollback_offset() > 0);
        assert_ne!(
            screen.contents(),
            live,
            "rendering must follow Alacritty display offset"
        );
        assert!(screen.scroll_to_live());
        assert_eq!(screen.contents(), live);
        screen.set_scrollback_lines(200);
        assert_eq!(screen.scrollback_lines(), 200);
        screen.feed("after-change\r\n");
        assert!(screen.contents().contains("after-change"));
        screen.reset();
        assert_eq!(
            screen.scrollback_lines(),
            200,
            "Host replay/reset must preserve chosen cap"
        );
    }

    #[test]
    fn alternate_screen_does_not_hijack_full_screen_tui_scrollback_keys() {
        let mut screen = VtScreen::with_scrollback(5, 20, 100);
        screen.feed("\u{1b}[?1049hfull screen TUI");
        assert!(screen.alternate_screen());
        assert!(!screen.scroll_history_page(true));
        assert_eq!(screen.scrollback_offset(), 0);
    }

    fn style_at(row: &StyledRow, byte: usize) -> CellStyle {
        row.spans
            .iter()
            .find(|span| span.range.contains(&byte))
            .expect("each displayed character must have a style")
            .style
    }

    #[test]
    fn terminal_mode_switches_are_backed_by_live_alacritty_state() {
        let mut screen = VtScreen::default();
        assert!(!screen.bracketed_paste());
        assert!(!screen.application_cursor_keys());
        screen.feed("\x1b[?2004h\x1b[?1h");
        assert!(screen.bracketed_paste());
        assert!(screen.application_cursor_keys());
        screen.feed("\x1b[?2004l\x1b[?1l");
        assert!(!screen.bracketed_paste());
        assert!(!screen.application_cursor_keys());
    }

    #[test]
    fn ansi_16_256_truecolor_foreground_and_background_are_preserved() {
        let mut screen = VtScreen::default();
        screen.feed("a\x1b[31mR\x1b[38;5;196mX\x1b[48;5;244mG\x1b[38;2;17;34;51mT");
        let row = &screen.styled_rows()[0];
        assert!(row.text.starts_with("aRXGT"));
        assert_eq!(style_at(row, 0).fg, TERMINAL_DEFAULT_FG);
        assert_eq!(style_at(row, 1).fg, 0xcd3131);
        assert_eq!(style_at(row, 2).fg, 0xff0000);
        assert_eq!(style_at(row, 3).bg, 0x808080);
        assert_eq!(style_at(row, 4).fg, 0x112233);
        assert_eq!(style_at(row, 4).bg, 0x808080);
    }

    #[test]
    fn inverse_bold_italic_underline_and_cursor_have_distinct_styles() {
        let mut screen = VtScreen::new(5, 20);
        screen.feed("\x1b[31;44;1;3;4;7mX\x1b[0m ");
        let row = &screen.styled_rows()[0];
        assert!(row.text.starts_with("X "));
        let x = style_at(row, 0);
        assert_eq!(x.fg, 0x2472c8);
        assert_eq!(x.bg, 0xcd3131);
        assert!(x.bold);
        assert!(x.italic);
        assert!(x.underline);
        let cursor = style_at(row, 2);
        assert!(cursor.cursor);
        assert_eq!(cursor.fg, CURSOR_FOREGROUND);
        assert_eq!(cursor.bg, CURSOR_BACKGROUND);
        screen.feed("\x1b[?25l");
        assert!(!screen.styled_rows()[0]
            .spans
            .iter()
            .any(|span| span.style.cursor));
    }

    #[test]
    fn unicode_wide_graphemes_keep_byte_boundaries_and_cell_alignment() {
        let mut screen = VtScreen::new(5, 20);
        screen.feed("你😀e\u{301}");
        let row = &screen.styled_rows()[0];
        assert!(row.text.starts_with("你😀e\u{301}"));
        assert!(!row.text.starts_with("你 😀"));
        assert_eq!(screen.cursor(), (0, 5));
        assert_eq!(row.text.chars().filter(|ch| *ch == '你').count(), 1);
        assert!(row.spans.iter().all(|span| {
            row.text.is_char_boundary(span.range.start) && row.text.is_char_boundary(span.range.end)
        }));
        let mut next = 0;
        for span in &row.spans {
            assert_eq!(span.range.start, next);
            next = span.range.end;
        }
        assert_eq!(next, row.text.len());
        assert!(row.spans.len() <= 20);
    }

    #[test]
    fn wide_terminal_preview_uses_the_host_supported_512_column_limit() {
        let mut screen = VtScreen::new(120, 350);
        assert_eq!(screen.dimensions(), (120, 350));
        assert_eq!(screen.styled_rows().len(), 120);
        screen.set_preview_size(160, 384);
        assert_eq!(screen.dimensions(), (160, 384));
        assert_eq!(screen.styled_rows()[0].text.chars().count(), 384);
    }

    #[test]
    fn terminal_rows_are_bounded_and_osc_clipboard_does_not_leak() {
        let mut screen = VtScreen::new(u16::MAX, u16::MAX);
        screen.feed("visible\x1b]52;c;dG9rZW4=\x07\x1b[32mOK");
        let rows = screen.styled_rows();
        assert_eq!(rows.len(), 200);
        assert!(rows.iter().all(|row| row.spans.len() <= 512));
        assert!(rows[0].text.starts_with("visibleOK"));
        assert!(!rows[0].text.contains("dG9rZW4"));
    }

    #[test]
    fn renders_cursor_reposition_and_erase_not_escape_noise() {
        let mut screen = VtScreen::new(24, 80);
        screen.feed("hello\x1b[2DXY\x1b[K");
        let text = screen.contents();
        assert!(text.starts_with("helXY"), "{text:?}");
        assert!(!text.contains("\x1b"));
        screen.feed("\x1b[H\x1b[2Jfresh");
        let text = screen.contents();
        assert!(text.starts_with("fresh"), "{text:?}");
        assert!(!text.contains("helXY"));
    }

    #[test]
    fn supports_alternate_screen_and_restores_main_screen() {
        let mut screen = VtScreen::default();
        screen.feed("shell output");
        screen.feed("\x1b[?1049h\x1b[Htop");
        assert!(screen.alternate_screen());
        assert!(screen.contents().starts_with("top"));
        assert!(!screen.contents().contains("shell output"));
        screen.feed("\x1b[?1049l");
        assert!(!screen.alternate_screen());
        assert!(screen.contents().contains("shell output"));
    }

    #[test]
    fn unicode_and_split_escape_sequence_survive_incremental_reads() {
        let mut screen = VtScreen::default();
        screen.feed("繁體中文 e\u{301} 🚀 \x1b[");
        screen.feed("31m紅\x1b[0m");
        let content = screen.contents();
        assert!(content.contains("繁體中文"));
        assert!(content.contains("🚀"));
        assert!(content.contains("紅"));
        assert!(!content.contains("\x1b"));
    }

    #[test]
    fn osc_clipboard_data_is_not_exposed_as_terminal_text() {
        let mut screen = VtScreen::default();
        screen.feed("safe\x1b]52;c;c2VjcmV0\x07visible");
        let contents = screen.contents();
        assert!(contents.contains("safe"));
        assert!(contents.contains("visible"));
        assert!(!contents.contains("c2VjcmV0"));
    }

    #[test]
    fn preview_dimension_limits_and_reset_are_bounded() {
        let mut screen = VtScreen::new(0, u16::MAX);
        assert_eq!(screen.dimensions(), (5, 512));
        screen.set_preview_size(40, 120);
        assert_eq!(screen.dimensions(), (40, 120));
        screen.feed("hello");
        screen.reset();
        assert!(!screen.contents().contains("hello"));
        assert_eq!(screen.dimensions(), (40, 120));
    }

    #[test]
    fn conpty_resize_notification_updates_preview_before_redraw_across_reads() {
        let mut screen = VtScreen::default();
        screen.feed("old output");
        // Windows ConPTY redraw begins with the host's actual viewport size,
        // sometimes split between two incremental CLI reads. 80x24 must not
        // scroll away a 36x120 redraw into an apparently blank GPUI screen.
        screen.feed("\x1b[?25l\x1b[8;36;");
        screen.feed("120t\x1b[HMicrosoft Windows [Version test]\x1b[K\r\n");
        // Leave the last grid row available for the cursor; a CRLF on the
        // bottom row would intentionally scroll the first line away.
        for row in 1..35 {
            screen.feed(&format!("row{row}\x1b[K\r\n"));
        }
        screen.feed("\x1b[12;2H\x1b[?25h");
        assert_eq!(screen.dimensions(), (36, 120));
        assert_eq!(screen.styled_rows().len(), 36);
        assert!(
            screen
                .contents()
                .contains("Microsoft Windows [Version test]"),
            "ConPTY redraw disappeared after resize: {:?}",
            screen.contents()
        );
    }

    #[test]
    fn conpty_resize_notification_rejects_invalid_dimensions_and_clamps_sizes() {
        let mut screen = VtScreen::default();
        screen.feed("\x1b[8;0;120t");
        screen.feed("\x1b[8;not-a-number;120t");
        screen.feed("\x1b[8;99999;99999t");
        assert_eq!(screen.dimensions(), (24, 80));
        screen.feed("\x1b[8;120;512t");
        assert_eq!(screen.dimensions(), (120, 512));
        screen.feed("\x1b[8;24;80t");
        assert_eq!(screen.dimensions(), (24, 80));
    }
}
