//! Bounded VT100/ANSI screen state for the GPUI frontend.
//!
//! This uses the maintained vt100 parser for terminal control sequences,
//! rather than interpreting TUI cursor/erase codes as plain text. Host-owned
//! PTY dimensions are NOT changed by resizing this local preview.
use std::num::NonZeroU16;
use std::ops::Range;

const MAX_SCROLLBACK_LINES: usize = 240;
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

fn color_rgb(color: vt100::Color, default: u32) -> u32 {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Idx(index) => indexed_rgb(index),
        vt100::Color::Rgb(r, g, b) => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
    }
}

fn cell_style(cell: &vt100::Cell, cursor: bool) -> CellStyle {
    let mut fg = color_rgb(cell.fgcolor(), TERMINAL_DEFAULT_FG);
    let mut bg = color_rgb(cell.bgcolor(), TERMINAL_DEFAULT_BG);
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cursor {
        fg = CURSOR_FOREGROUND;
        bg = CURSOR_BACKGROUND;
    }
    CellStyle {
        fg,
        bg,
        bold: cell.bold(),
        italic: cell.italic(),
        underline: cell.underline(),
        cursor,
    }
}

/// Tracks only the bounded ConPTY/xterm window resize report
/// CSI 8;rows;cols t across incremental Host reads. Other control sequences
/// stay the responsibility of the vt100 parser; this probe never consumes or
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

pub struct VtScreen {
    parser: vt100::Parser,
    rows: u16,
    cols: u16,
    resize_probe: ConptyResizeProbe,
}

impl std::fmt::Debug for VtScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VtScreen")
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
        let rows = rows.clamp(5, 100);
        let cols = cols.clamp(20, 240);
        let parser = vt100::Parser::new(
            NonZeroU16::new(rows).expect("clamped rows"),
            NonZeroU16::new(cols).expect("clamped cols"),
            MAX_SCROLLBACK_LINES,
        );
        Self {
            parser,
            rows,
            cols,
            resize_probe: ConptyResizeProbe::default(),
        }
    }

    pub fn feed(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut start = 0;
        for (offset, byte) in bytes.iter().copied().enumerate() {
            if let Some((rows, cols)) = self.resize_probe.observe(byte) {
                // Process the CSI itself at the old size, then adjust the
                // bounded local preview BEFORE the following Host redraw.
                // This also works when CSI starts in a previous read.
                self.parser.process(&bytes[start..=offset]);
                self.set_preview_size(rows, cols);
                start = offset + 1;
            }
        }
        self.parser.process(&bytes[start..]);
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.rows, self.cols);
    }

    /// A visible screen snapshot (not a full terminal transcript). Escape
    /// commands, hyperlinks, clipboard OSC52 and control bytes are not
    /// forwarded into UI text, only rendered into local terminal state.
    pub fn contents(&self) -> String {
        self.parser.screen().contents()
    }

    /// The exact visible grid, grouped into contiguous styled text runs.
    /// O(rows * cols), capped at 100 x 240, independent of scrollback size.
    /// A CJK/emoji wide character occupies one text run with double-width
    /// font layout; its trailing VT continuation cell is not duplicated.
    pub fn styled_rows(&self) -> Vec<StyledRow> {
        let screen = self.parser.screen();
        let (cursor_row, cursor_col) = screen.cursor_position();
        let show_cursor = !screen.hide_cursor();
        (0..self.rows)
            .map(|row| {
                let mut text = String::with_capacity(usize::from(self.cols) * 2);
                let mut spans: Vec<StyleSpan> = Vec::new();
                for col in 0..self.cols {
                    let cell = screen.cell(row, col);
                    if cell.is_some_and(vt100::Cell::is_wide_continuation) {
                        continue;
                    }
                    let is_cursor = show_cursor
                        && row == cursor_row
                        && (col == cursor_col
                            || (col + 1 == cursor_col && cell.is_some_and(vt100::Cell::is_wide)));
                    let style = cell.map_or_else(CellStyle::default, |c| cell_style(c, is_cursor));
                    let start = text.len();
                    match cell {
                        Some(c) if c.has_contents() => text.push_str(c.contents()),
                        _ => text.push(' '),
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
        self.parser.screen().cursor_position()
    }

    pub fn alternate_screen(&self) -> bool {
        self.parser.screen().alternate_screen()
    }

    pub fn dimensions(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// Local preview size only. Never assert to Host that it was resized.
    pub fn set_preview_size(&mut self, rows: u16, cols: u16) {
        let rows = rows.clamp(5, 100);
        let cols = cols.clamp(20, 240);
        if self.rows != rows || self.cols != cols {
            self.parser.set_size(
                NonZeroU16::new(rows).expect("clamped rows"),
                NonZeroU16::new(cols).expect("clamped cols"),
            );
            self.rows = rows;
            self.cols = cols;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style_at(row: &StyledRow, byte: usize) -> CellStyle {
        row.spans
            .iter()
            .find(|span| span.range.contains(&byte))
            .expect("each displayed character must have a style")
            .style
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
    fn terminal_rows_are_bounded_and_osc_clipboard_does_not_leak() {
        let mut screen = VtScreen::new(u16::MAX, u16::MAX);
        screen.feed("visible\x1b]52;c;dG9rZW4=\x07\x1b[32mOK");
        let rows = screen.styled_rows();
        assert_eq!(rows.len(), 100);
        assert!(rows.iter().all(|row| row.spans.len() <= 240));
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
        assert_eq!(screen.dimensions(), (5, 240));
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
        assert_eq!(screen.dimensions(), (100, 240));
        screen.feed("\x1b[8;24;80t");
        assert_eq!(screen.dimensions(), (24, 80));
    }
}
