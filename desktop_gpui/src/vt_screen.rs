//! Bounded VT100/ANSI screen state for the GPUI frontend.
//!
//! This uses the maintained vt100 parser for terminal control sequences,
//! rather than interpreting TUI cursor/erase codes as plain text. Host-owned
//! PTY dimensions are NOT changed by resizing this local preview.
use std::num::NonZeroU16;

const MAX_SCROLLBACK_LINES: usize = 240;
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;

pub struct VtScreen {
    parser: vt100::Parser,
    rows: u16,
    cols: u16,
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
        Self { parser, rows, cols }
    }

    pub fn feed(&mut self, text: &str) {
        self.parser.process(text.as_bytes());
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
}
