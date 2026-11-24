use memchr::memmem;

const QUERY: &[u8] = b"\x1b[6n";

/// A one-shot startup query matcher. The only state required across reads is
/// how much of the four-byte query matched the suffix of the preceding read.
/// A complete match doubles as the permanently answered state.
#[derive(Default)]
pub(super) struct StartupCursorQuery {
    matched: u8,
}

impl StartupCursorQuery {
    pub(super) fn answered() -> Self {
        Self {
            matched: QUERY.len() as u8,
        }
    }

    pub(super) fn take(&mut self, data: &[u8]) -> bool {
        let matched = self.matched as usize;
        if matched == QUERY.len() || data.is_empty() {
            return false;
        }

        if matched != 0 {
            let remaining = &QUERY[matched..];
            let count = remaining.len().min(data.len());
            if data[..count] == remaining[..count] {
                self.matched += count as u8;
                return self.matched as usize == QUERY.len();
            }
            // A partial query contains no other ESC, so on mismatch only
            // candidates beginning in the new chunk can still match.
            self.matched = 0;
        }

        if memmem::find(data, QUERY).is_some() {
            *self = Self::answered();
            return true;
        }
        // Only an incomplete query at the end can affect the next read.
        // The pattern has no internal ESC, so these suffixes cannot overlap.
        for count in (1..=data.len().min(QUERY.len() - 1)).rev() {
            if data.ends_with(&QUERY[..count]) {
                self.matched = count as u8;
                break;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct LegacyMatcher {
        tail: Vec<u8>,
        answered: bool,
    }

    impl LegacyMatcher {
        fn take(&mut self, data: &[u8]) -> bool {
            if self.answered {
                return false;
            }
            let mut observed = Vec::with_capacity(self.tail.len() + data.len());
            observed.extend_from_slice(&self.tail);
            observed.extend_from_slice(data);
            if observed.windows(QUERY.len()).any(|window| window == QUERY) {
                self.answered = true;
                self.tail.clear();
                return true;
            }
            let keep = (QUERY.len() - 1).min(observed.len());
            self.tail = observed[observed.len() - keep..].to_vec();
            false
        }
    }

    #[test]
    fn matches_every_split_and_answers_exactly_once() {
        for split in 0..=QUERY.len() {
            let mut matcher = StartupCursorQuery::default();
            let first = matcher.take(&QUERY[..split]);
            assert!(!matcher.take(b""));
            let second = matcher.take(&QUERY[split..]);
            assert_ne!(first, second, "split {split}");
            assert!(!matcher.take(QUERY));
        }
        let mut answered = StartupCursorQuery::answered();
        assert!(!answered.take(QUERY));
    }

    #[test]
    fn overlapping_escape_and_wrong_partial_candidates_match_legacy() {
        let data = b"\x1b\x1b[6x\x1b[\x1b[6\x1b[6n\x1b[6n";
        for size in 1..=data.len() {
            let mut matcher = StartupCursorQuery::default();
            let mut legacy = LegacyMatcher::default();
            for chunk in data.chunks(size) {
                assert_eq!(matcher.take(chunk), legacy.take(chunk));
                assert!(!matcher.take(b""));
            }
            assert_eq!(matcher.matched as usize, QUERY.len());
        }
    }

    #[test]
    fn arbitrary_binary_streams_match_legacy_across_fragmentation() {
        let mut random = 0xface_1234_u32;
        for trial in 0..96 {
            let mut data = Vec::new();
            for index in 0..4096 {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                data.push(random as u8);
                if index % 127 == 0 {
                    data.extend_from_slice(b"\x1b[6x\x1b[");
                }
                if trial % 2 == 0 && index == trial * 31 {
                    data.extend_from_slice(QUERY);
                }
            }
            let mut matcher = StartupCursorQuery::default();
            let mut legacy = LegacyMatcher::default();
            for chunk in data.chunks(trial % 29 + 1) {
                assert_eq!(matcher.take(chunk), legacy.take(chunk), "trial {trial}");
            }
            assert_eq!(matcher.take(QUERY), legacy.take(QUERY));
        }
    }
}
