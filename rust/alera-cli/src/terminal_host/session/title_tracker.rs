use memchr::{memchr, memchr2};

const MAX_TITLE_BYTES: usize = 4096;
const MAX_OSC_CODE_BYTES: usize = 16;

#[derive(Default)]
pub(super) struct TerminalTitleTracker {
    state: ParserState,
    code: Vec<u8>,
    title: Vec<u8>,
    title_valid: bool,
    current_title: Option<String>,
}

#[derive(Default, Debug, PartialEq, Eq)]
enum ParserState {
    #[default]
    Ground,
    Escape,
    OscCode,
    OscTitle,
    OscTitleTail,
    OscDiscard,
    OscEscape,
}

impl TerminalTitleTracker {
    pub(super) fn current_title(&self) -> Option<&str> {
        self.current_title.as_deref()
    }

    pub(super) fn feed(&mut self, data: &[u8]) -> Option<String> {
        // Remember the original title only on the first real mutation.
        // Ordinary PTY output and unchanged OSC titles allocate nothing.
        let mut previous = None;
        self.scan(data, &mut previous);
        previous
            .filter(|original| original != &self.current_title)
            .and_then(|_| self.current_title.clone())
    }

    /// Consume restored output without creating an unused notification.
    pub(super) fn seed(&mut self, data: &[u8]) {
        self.scan(data, &mut None);
    }

    fn scan(&mut self, mut data: &[u8], previous: &mut Option<Option<String>>) {
        while !data.is_empty() {
            // These states ignore every byte except the listed delimiters.
            // Leave partial escape/title sequences in the scalar state
            // machine so behavior is identical across PTY chunk boundaries.
            let offset = match self.state {
                ParserState::Ground => memchr(0x1b, data),
                ParserState::OscTitleTail | ParserState::OscDiscard => memchr2(0x07, 0x1b, data),
                _ => Some(0),
            };
            let Some(offset) = offset else { break };
            self.feed_byte(data[offset], previous);
            data = &data[offset + 1..];
        }
    }

    fn feed_byte(&mut self, byte: u8, previous: &mut Option<Option<String>>) {
        match self.state {
            ParserState::Ground => {
                if byte == 0x1b {
                    self.state = ParserState::Escape;
                }
            }
            ParserState::Escape => {
                if byte == b']' {
                    self.code.clear();
                    self.title.clear();
                    self.title_valid = false;
                    self.state = ParserState::OscCode;
                } else if byte != 0x1b {
                    self.state = ParserState::Ground;
                }
            }
            ParserState::OscCode => match byte {
                0x07 => self.finish_osc(previous),
                0x1b => self.state = ParserState::OscEscape,
                b';' => {
                    self.title_valid = self.code == b"0" || self.code == b"2";
                    self.state = if self.title_valid {
                        ParserState::OscTitle
                    } else {
                        ParserState::OscDiscard
                    };
                }
                _ if self.code.len() < MAX_OSC_CODE_BYTES => self.code.push(byte),
                _ => {
                    self.title_valid = false;
                    self.state = ParserState::OscDiscard;
                }
            },
            ParserState::OscTitle => match byte {
                0x07 => self.finish_osc(previous),
                0x1b => self.state = ParserState::OscEscape,
                b';' => self.state = ParserState::OscTitleTail,
                _ if self.title.len() < MAX_TITLE_BYTES => self.title.push(byte),
                _ => {
                    self.title_valid = false;
                    self.state = ParserState::OscDiscard;
                }
            },
            ParserState::OscTitleTail => match byte {
                0x07 => self.finish_osc(previous),
                0x1b => self.state = ParserState::OscEscape,
                _ => {}
            },
            ParserState::OscDiscard => match byte {
                0x07 => self.reset_osc(),
                0x1b => self.state = ParserState::OscEscape,
                _ => {}
            },
            ParserState::OscEscape => {
                if byte == b'\\' {
                    self.finish_osc(previous);
                } else {
                    self.reset_osc();
                }
            }
        }
    }

    fn finish_osc(&mut self, previous: &mut Option<Option<String>>) {
        if self.title_valid && self.title.len() <= MAX_TITLE_BYTES {
            let title = String::from_utf8_lossy(&self.title);
            if self.current_title.as_deref() != Some(title.as_ref()) {
                let old = self.current_title.replace(title.into_owned());
                if previous.is_none() {
                    *previous = Some(old);
                }
            }
        }
        self.reset_osc();
    }

    fn reset_osc(&mut self) {
        self.code.clear();
        self.title.clear();
        self.title_valid = false;
        self.state = ParserState::Ground;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_osc_zero_and_two_with_both_terminators() {
        let mut tracker = TerminalTitleTracker::default();

        assert_eq!(tracker.feed(b"\x1b]0;Build\x07"), Some("Build".into()));
        assert_eq!(tracker.current_title(), Some("Build"));
        assert_eq!(tracker.feed(b"\x1b]2;Plan\x1b\\"), Some("Plan".into()));
        assert_eq!(tracker.current_title(), Some("Plan"));
    }

    #[test]
    fn tracks_sequences_split_across_output_chunks() {
        let mut tracker = TerminalTitleTracker::default();

        assert_eq!(tracker.feed(b"before\x1b]2;Review"), None);
        assert_eq!(tracker.feed(b" Tests\x1b"), None);
        assert_eq!(tracker.feed(b"\\after"), Some("Review Tests".into()));
    }

    #[test]
    fn reports_only_the_final_change_from_one_chunk() {
        let mut tracker = TerminalTitleTracker::default();

        assert_eq!(
            tracker.feed(b"\x1b]0;First\x07\x1b]2;Second\x07"),
            Some("Second".into()),
        );
        assert_eq!(tracker.feed(b"\x1b]2;Second\x07"), None);
    }

    #[test]
    fn matches_xterm_title_parameter_handling() {
        let mut tracker = TerminalTitleTracker::default();

        assert_eq!(
            tracker.feed(b"\x1b]2;First;Ignored\x07"),
            Some("First".into()),
        );
    }

    #[test]
    fn empty_title_clears_and_non_title_osc_is_ignored() {
        let mut tracker = TerminalTitleTracker::default();

        tracker.feed(b"\x1b]2;Active\x07");
        assert_eq!(tracker.feed(b"\x1b]1;Icon\x07"), None);
        assert_eq!(tracker.current_title(), Some("Active"));
        assert_eq!(tracker.feed(b"\x1b]2;\x07"), Some(String::new()));
        assert_eq!(tracker.current_title(), Some(""));
    }

    fn scalar_feed(tracker: &mut TerminalTitleTracker, data: &[u8]) -> Option<String> {
        // Reference: original byte-at-a-time scanner and net-change reporting.
        let original = tracker.current_title.clone();
        let mut unused = None;
        for &byte in data {
            tracker.feed_byte(byte, &mut unused);
        }
        (tracker.current_title != original)
            .then(|| tracker.current_title.clone())
            .flatten()
    }

    fn assert_equivalent(fast: &TerminalTitleTracker, scalar: &TerminalTitleTracker) {
        assert_eq!(fast.state, scalar.state);
        assert_eq!(fast.code, scalar.code);
        assert_eq!(fast.title, scalar.title);
        assert_eq!(fast.title_valid, scalar.title_valid);
        assert_eq!(fast.current_title, scalar.current_title);
    }

    #[test]
    fn fast_scan_matches_scalar_across_all_fragment_boundaries() {
        let mut input = b"start\x1b[31m\x1b]2;A\x07\x1b]2;A\x1b\\\x1b]0;B;ignored".to_vec();
        input.extend(std::iter::repeat_n(b'i', 8192));
        input.extend_from_slice(b"\x1b\\\x1b]52;ignored\x07\x1b]2;\xff\x07\x1b]2;A\x07\x1b]");
        input.extend(std::iter::repeat_n(b'9', MAX_OSC_CODE_BYTES + 1));
        input.extend_from_slice(b";not a title\x1b\\\x1b]2;");
        input.extend(std::iter::repeat_n(b'x', MAX_TITLE_BYTES + 1));
        input.extend_from_slice(b"\x07\x1b]2;Final\x1b\\\x1b]2;unfinished\x1b");
        for size in [1, 2, 3, 7, 16, 31, 64, 127, 4096, input.len()] {
            let mut fast = TerminalTitleTracker::default();
            let mut scalar = TerminalTitleTracker::default();
            for chunk in input.chunks(size) {
                assert_eq!(
                    fast.feed(chunk),
                    scalar_feed(&mut scalar, chunk),
                    "size={size}"
                );
                assert_equivalent(&fast, &scalar);
            }
            // An incomplete ST terminator must survive the last chunk.
            assert_eq!(fast.feed(b"\\"), scalar_feed(&mut scalar, b"\\"));
            assert_equivalent(&fast, &scalar);
            assert_eq!(fast.current_title(), Some("unfinished"));
        }
    }

    #[test]
    fn fast_scan_matches_scalar_for_arbitrary_binary_input() {
        let mut bytes = Vec::new();
        let mut rng = 0x12345678_u32;
        for i in 0..20000 {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            bytes.push(rng as u8);
            if i % 127 == 0 {
                bytes.extend_from_slice(b"\x1b]2;random\x07\x1b]8;https://example.invalid\x1b\\");
            }
        }
        let mut fast = TerminalTitleTracker::default();
        let mut scalar = TerminalTitleTracker::default();
        for chunk in bytes.chunks(37) {
            assert_eq!(fast.feed(chunk), scalar_feed(&mut scalar, chunk));
            assert_equivalent(&fast, &scalar);
        }
    }

    #[test]
    fn repeated_titles_and_net_zero_changes_preserve_notification_contract() {
        let mut tracker = TerminalTitleTracker::default();
        assert_eq!(tracker.feed(b"\x1b]2;Stable\x07"), Some("Stable".into()));
        let original = tracker.current_title().unwrap().as_ptr();
        assert_eq!(tracker.feed(&vec![b'x'; 1024 * 1024]), None);
        assert_eq!(tracker.feed(b"\x1b[31mtext\x1b[0m\x1b]2;Stable\x07"), None);
        assert_eq!(tracker.current_title().unwrap().as_ptr(), original);
        assert_eq!(tracker.feed(b"\x1b]2;Temporary\x07\x1b]0;Stable\x07"), None);
        assert_eq!(tracker.current_title(), Some("Stable"));
        assert_eq!(tracker.feed(b"\x1b]2;\x07"), Some(String::new()));
    }

    #[test]
    fn seed_retains_partial_parser_state_without_emitting_notification() {
        let mut tracker = TerminalTitleTracker::default();
        tracker.seed(b"\x1b]0;First\x07");
        tracker.seed(&vec![b'x'; 1024 * 1024]);
        tracker.seed(b"\x1b]2;Second\x1b");
        assert_eq!(tracker.current_title(), Some("First"));
        assert_eq!(tracker.feed(b"\\"), Some("Second".into()));
    }

    #[test]
    fn oversized_title_is_discarded() {
        let mut tracker = TerminalTitleTracker::default();
        tracker.feed(b"\x1b]2;Stable\x07");
        let mut sequence = b"\x1b]2;".to_vec();
        sequence.extend(std::iter::repeat_n(b'x', MAX_TITLE_BYTES + 1));
        sequence.push(0x07);

        assert_eq!(tracker.feed(&sequence), None);
        assert_eq!(tracker.current_title(), Some("Stable"));
    }
}
