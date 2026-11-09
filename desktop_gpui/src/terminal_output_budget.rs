//! Frame-time-aware terminal output budget, shared by Native's Host read path.
//!
//! Mirrors the behavior of Classic Dart's terminal_runtime_output_batching:
//! short interactive bursts are cheap, while sustained ANSI-heavy output
//! must not monopolize the GPUI thread.

use std::time::{Duration, Instant};

const MIN_BYTES: u64 = 4 * 1024;
const INITIAL_BYTES: u64 = 16 * 1024;
const MAX_BYTES: u64 = 64 * 1024;
const GROWTH_STEP: u64 = 8 * 1024;
const QUANTUM: u64 = 1024;
const TARGET_PARSE_MICROS: u64 = 6_000;
const IDLE_RESET: Duration = Duration::from_millis(500);

/// Per-session read budget, trained only by full/saturated output chunks.
///
/// Classic's adaptive output pump targets ~6ms for ANSI-heavy parsing, grows
/// slowly for cheap frames, and shrinks at most 2x on slow frames. Native
/// measures its own VT parser rather than copying any Dart timing sample.
#[derive(Clone, Debug)]
pub struct TerminalParseBudget {
    bytes: u64,
    last_output_at: Option<Instant>,
}

impl Default for TerminalParseBudget {
    fn default() -> Self {
        Self {
            bytes: INITIAL_BYTES,
            last_output_at: None,
        }
    }
}

impl TerminalParseBudget {
    pub fn max_read_bytes(&self) -> u64 {
        self.bytes
    }

    pub fn observe(&mut self, now: Instant, read_bytes: usize, parse_time: Duration) {
        if self
            .last_output_at
            .is_some_and(|last| now.saturating_duration_since(last) >= IDLE_RESET)
        {
            self.bytes = INITIAL_BYTES;
        }
        self.last_output_at = Some(now);

        // Partial/quiet output can be misleading: only a saturated read
        // provides a useful full-budget parse-time measurement.
        if (read_bytes as u64).saturating_add(1) < self.bytes {
            return;
        }
        let micros = parse_time.as_micros().min(u64::MAX as u128) as u64;
        if micros < TARGET_PARSE_MICROS / 2 {
            self.bytes = (self.bytes + GROWTH_STEP).min(MAX_BYTES);
        } else if micros > TARGET_PARSE_MICROS {
            let proportional = self.bytes.saturating_mul(TARGET_PARSE_MICROS) / micros;
            let not_faster_than_half = self.bytes / 2;
            let next = proportional
                .max(not_faster_than_half)
                .clamp(MIN_BYTES, self.bytes);
            self.bytes = ((next / QUANTUM) * QUANTUM).clamp(MIN_BYTES, self.bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_conservatively_then_earns_larger_chunks_on_fast_full_reads() {
        let mut policy = TerminalParseBudget::default();
        assert_eq!(policy.max_read_bytes(), 16 * 1024);
        let started = Instant::now();
        policy.observe(started, 16 * 1024, Duration::from_millis(2));
        assert_eq!(policy.max_read_bytes(), 24 * 1024);
        policy.observe(
            started + Duration::from_millis(100),
            24 * 1024,
            Duration::from_millis(2),
        );
        assert_eq!(policy.max_read_bytes(), 32 * 1024);
    }

    #[test]
    fn tui_slowdown_shrinks_budget_at_most_by_half_and_never_below_floor() {
        let mut policy = TerminalParseBudget::default();
        let started = Instant::now();
        policy.observe(started, 16 * 1024, Duration::from_millis(50));
        assert_eq!(policy.max_read_bytes(), 8 * 1024);
        policy.observe(
            started + Duration::from_millis(100),
            8 * 1024,
            Duration::from_millis(50),
        );
        assert_eq!(policy.max_read_bytes(), 4 * 1024);
        policy.observe(
            started + Duration::from_millis(200),
            4 * 1024,
            Duration::from_millis(50),
        );
        assert_eq!(policy.max_read_bytes(), 4 * 1024);
    }

    #[test]
    fn partial_reads_do_not_train_adaptive_budget_and_idle_resets_it() {
        let mut policy = TerminalParseBudget::default();
        let started = Instant::now();
        policy.observe(started, 800, Duration::from_millis(80));
        assert_eq!(policy.max_read_bytes(), 16 * 1024);
        policy.observe(
            started + Duration::from_millis(100),
            16 * 1024,
            Duration::from_millis(2),
        );
        assert_eq!(policy.max_read_bytes(), 24 * 1024);
        policy.observe(
            started + Duration::from_millis(800),
            800,
            Duration::from_millis(50),
        );
        assert_eq!(policy.max_read_bytes(), 16 * 1024);
    }

    #[test]
    fn long_quick_stream_respects_host_limit() {
        let mut policy = TerminalParseBudget::default();
        let started = Instant::now();
        for sample in 0..30 {
            let full = policy.max_read_bytes() as usize;
            policy.observe(
                started + Duration::from_millis(sample * 100),
                full,
                Duration::from_millis(1),
            );
        }
        assert_eq!(policy.max_read_bytes(), 64 * 1024);
    }
}
