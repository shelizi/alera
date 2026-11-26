//! Conservative, incremental literal-query evidence for verified cached text.
//! Neither a filter hit nor a cached result authorizes replacement.
//! A negative proof can skip matching; every other case runs the exact matcher.
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const MAX_QUERY_BYTES: usize = 256;
const NEGATIVE_SLOTS: usize = 3;
const TRIGRAM_MIN_BYTES: usize = 64 * 1024;
const TRIGRAM_PROBES: u8 = 3;
const TRIGRAM_WORDS: usize = 512;
const TRIGRAM_BITS: usize = TRIGRAM_WORDS * 64;
const MAX_QUERY_GRAMS: usize = 16;

pub(super) struct LiteralQuery {
    text: String,
    case_sensitive: bool,
    ascii: bool,
    grams: Vec<u16>,
}

impl LiteralQuery {
    pub(super) fn new(query: &str, case_sensitive: bool) -> Option<Arc<Self>> {
        if query.is_empty() || query.len() > MAX_QUERY_BYTES || query.contains(['\r', '\n']) {
            return None;
        }
        let mut grams = Vec::with_capacity(MAX_QUERY_GRAMS);
        for gram in query.as_bytes().windows(3) {
            let bit = gram_bit(gram) as u16;
            if !grams.contains(&bit) {
                grams.push(bit);
                if grams.len() == MAX_QUERY_GRAMS {
                    break;
                }
            }
        }
        Some(Arc::new(Self {
            text: query.to_owned(),
            case_sensitive,
            ascii: query.is_ascii(),
            grams,
        }))
    }

    fn extends(&self, previous: &Self) -> bool {
        self.case_sensitive == previous.case_sensitive && self.text.contains(&previous.text)
    }
}

// One-bit hashed byte trigrams. ASCII folding gives a conservative superset
// for case-sensitive queries; Unicode-insensitive matching uses the exact
// matcher unless both the text and query are ASCII.
fn gram_bit(bytes: &[u8]) -> usize {
    let value = u32::from(bytes[0].to_ascii_lowercase()) << 16
        | u32::from(bytes[1].to_ascii_lowercase()) << 8
        | u32::from(bytes[2].to_ascii_lowercase());
    ((value.wrapping_mul(0x9e37_79b1)) >> 17) as usize
}

struct TrigramFilter {
    bits: Box<[u64; TRIGRAM_WORDS]>,
}

impl TrigramFilter {
    fn new(text: &str) -> Self {
        let mut bits = Box::new([0_u64; TRIGRAM_WORDS]);
        // Compute a conservative fingerprint once, only after warm probes.
        // Hash collisions can cause extra exact scans, never lost matches.
        for gram in text.as_bytes().windows(3) {
            let bit = gram_bit(gram);
            bits[bit / 64] |= 1_u64 << (bit % 64);
        }
        Self { bits }
    }

    fn definitely_absent(&self, query: &LiteralQuery) -> bool {
        query.grams.iter().any(|&bit| {
            let bit = usize::from(bit);
            self.bits[bit / 64] & (1_u64 << (bit % 64)) == 0
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Probe {
    Scan,
    RefinementAbsent,
    TrigramAbsent,
}

pub(super) struct IndexedText {
    text: String,
    ascii: OnceLock<bool>,
    absent: Mutex<[Option<Arc<LiteralQuery>>; NEGATIVE_SLOTS]>,
    filter: OnceLock<TrigramFilter>,
    probes: AtomicU8,
    #[cfg(test)]
    refinement_skips: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    trigram_skips: std::sync::atomic::AtomicUsize,
}

impl IndexedText {
    pub(super) fn new(text: String) -> Self {
        Self {
            text,
            ascii: OnceLock::new(),
            absent: Mutex::new(std::array::from_fn(|_| None)),
            filter: OnceLock::new(),
            probes: AtomicU8::new(0),
            #[cfg(test)]
            refinement_skips: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            trigram_skips: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub(super) fn as_str(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub(super) fn index_stats(&self) -> (usize, usize, usize) {
        (
            usize::from(self.filter.get().is_some()),
            self.refinement_skips.load(Ordering::Relaxed),
            self.trigram_skips.load(Ordering::Relaxed),
        )
    }

    // Reserve the maximum potential index charge at admission, so lazy
    // construction and remembered literals cannot grow past the LRU budget.
    pub(super) fn charged_capacity(&self) -> usize {
        self.text
            .capacity()
            .saturating_add(std::mem::size_of::<Self>())
            .saturating_add(NEGATIVE_SLOTS * (MAX_QUERY_BYTES + MAX_QUERY_GRAMS * 2 + 96))
            .saturating_add(if self.text.len() >= TRIGRAM_MIN_BYTES {
                TRIGRAM_BITS / 8
            } else {
                0
            })
    }

    pub(super) fn probe(&self, query: &LiteralQuery) -> Probe {
        if self.absent.lock().is_ok_and(|absent| {
            absent
                .iter()
                .flatten()
                .any(|previous| query.extends(previous))
        }) {
            #[cfg(test)]
            self.refinement_skips.fetch_add(1, Ordering::Relaxed);
            return Probe::RefinementAbsent;
        }
        if self.text.len() < TRIGRAM_MIN_BYTES || query.grams.is_empty() {
            return Probe::Scan;
        }
        if !query.case_sensitive
            && (!query.ascii || !*self.ascii.get_or_init(|| self.text.is_ascii()))
        {
            return Probe::Scan;
        }
        let filter = if let Some(filter) = self.filter.get() {
            filter
        } else {
            // Do not build an index during first load or early refinement.
            // Only unrelated, eligible repeated probes pay its construction.
            let prior = self
                .probes
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                    Some(count.saturating_add(1))
                })
                .unwrap_or(TRIGRAM_PROBES);
            if prior < TRIGRAM_PROBES - 1 {
                return Probe::Scan;
            }
            self.filter.get_or_init(|| TrigramFilter::new(&self.text))
        };
        if filter.definitely_absent(query) {
            #[cfg(test)]
            self.trigram_skips.fetch_add(1, Ordering::Relaxed);
            Probe::TrigramAbsent
        } else {
            Probe::Scan
        }
    }

    // Only call after an exact complete scan returned no matches. Keep
    // shortest proofs where possible; a shorter absent literal also proves
    // absence of any query containing it under the same literal semantics.
    pub(super) fn remember_absent(&self, query: &Arc<LiteralQuery>) {
        let Ok(mut absent) = self.absent.lock() else {
            return;
        };
        if absent
            .iter()
            .flatten()
            .any(|previous| query.extends(previous))
        {
            return;
        }
        for slot in absent.iter_mut() {
            if slot
                .as_ref()
                .is_some_and(|previous| previous.extends(query))
            {
                *slot = None;
            }
        }
        if let Some(slot) = absent.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(query.clone());
        } else {
            absent.rotate_left(1);
            absent[NEGATIVE_SLOTS - 1] = Some(query.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_refinements_require_same_literal_semantics() {
        let text = IndexedText::new("actual content".into());
        let absent = LiteralQuery::new("needle", false).unwrap();
        text.remember_absent(&absent);
        for query in ["needle", "needle2", "my_needle", "prefixneedlepostfix"] {
            assert_eq!(
                text.probe(&LiteralQuery::new(query, false).unwrap()),
                Probe::RefinementAbsent
            );
        }
        assert_eq!(
            text.probe(&LiteralQuery::new("needle", true).unwrap()),
            Probe::Scan
        );
        assert_eq!(
            text.probe(&LiteralQuery::new("need", false).unwrap()),
            Probe::Scan
        );
        assert_eq!(
            text.probe(&LiteralQuery::new("actual", false).unwrap()),
            Probe::Scan
        );
        assert!(LiteralQuery::new(&"x".repeat(257), false).is_none());
    }

    #[test]
    fn trigram_filter_is_lazy_and_never_rejects_present_substrings() {
        let text = IndexedText::new("Hello NeedLE 世界 ABCD\r\n".repeat(4096));
        let absent = LiteralQuery::new("zzz_not_in_text", true).unwrap();
        assert_eq!(text.probe(&absent), Probe::Scan);
        assert_eq!(text.probe(&absent), Probe::Scan);
        assert!(text.filter.get().is_none());
        assert_eq!(text.probe(&absent), Probe::TrigramAbsent);
        assert!(text.filter.get().is_some());
        for query in ["Hello", "NeedLE", "世界", "ABCD", "NeedLE 世界"] {
            assert_eq!(
                text.probe(&LiteralQuery::new(query, true).unwrap()),
                Probe::Scan
            );
        }
        // Kelvin sign and long s can fold to ASCII K/S. Never apply the
        // ASCII-folded filter to Unicode-insensitive text or queries.
        let mixed = IndexedText::new("\u{212a}\u{017f}x\n".repeat(18000));
        let query = LiteralQuery::new("ksx", false).unwrap();
        for _ in 0..5 {
            assert_eq!(mixed.probe(&query), Probe::Scan);
        }
        let ascii = IndexedText::new("KSX example\n".repeat(9000));
        let query = LiteralQuery::new("\u{212a}\u{017f}x", false).unwrap();
        for _ in 0..5 {
            assert_eq!(ascii.probe(&query), Probe::Scan);
        }
    }
}
