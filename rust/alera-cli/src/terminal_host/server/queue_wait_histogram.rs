/// Constant-memory queue wait histogram. Bounds correspond to upper inclusive
/// milliseconds; buckets, not per-request samples, are retained.
const BUCKET_UPPER_MS: [u64; 19] = [
    0,
    1,
    2,
    4,
    8,
    16,
    32,
    64,
    128,
    256,
    512,
    1024,
    2048,
    4096,
    8192,
    16384,
    32768,
    65536,
    u64::MAX,
];

#[derive(Default)]
pub(super) struct QueueWaitHistogram {
    samples: u64,
    counts: [u64; BUCKET_UPPER_MS.len()],
    max_observed_ms: u64,
}

impl QueueWaitHistogram {
    pub(super) fn record(&mut self, wait_ms: u64) {
        self.samples = self.samples.saturating_add(1);
        self.max_observed_ms = self.max_observed_ms.max(wait_ms);
        let bucket = BUCKET_UPPER_MS.partition_point(|upper| *upper < wait_ms);
        self.counts[bucket] = self.counts[bucket].saturating_add(1);
    }

    pub(super) fn samples(&self) -> u64 {
        self.samples
    }

    pub(super) fn percentile_upper_ms(&self, percent: u64) -> u64 {
        if self.samples == 0 {
            return 0;
        }
        // Use u128 for the multiplication so the percentile target itself
        // cannot wrap after many days of runtime. The fixed array stays O(1).
        let percentage = percent.clamp(1, 100) as u128;
        let target = ((self.samples as u128 * percentage + 99) / 100) as u64;
        let mut observed = 0u64;
        for (upper, count) in BUCKET_UPPER_MS.iter().zip(&self.counts) {
            observed = observed.saturating_add(*count);
            if observed >= target {
                // The overflow bucket is open-ended; report the exact maximum
                // instead of a meaningless u64::MAX latency.
                return if *upper == u64::MAX {
                    self.max_observed_ms
                } else {
                    *upper
                };
            }
        }
        self.max_observed_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p95_and_p99_are_bounded_bucket_estimates_without_per_request_storage() {
        let mut histogram = QueueWaitHistogram::default();
        for _ in 0..98 {
            histogram.record(0);
        }
        histogram.record(7);
        histogram.record(500);
        assert_eq!(histogram.samples(), 100);
        assert_eq!(histogram.percentile_upper_ms(95), 0);
        assert_eq!(histogram.percentile_upper_ms(99), 8);
        assert_eq!(histogram.percentile_upper_ms(100), 512);
        assert!(std::mem::size_of::<QueueWaitHistogram>() <= 256);
    }

    #[test]
    fn empty_histogram_and_slow_overflow_bucket_are_defined() {
        let mut histogram = QueueWaitHistogram::default();
        assert_eq!(histogram.percentile_upper_ms(95), 0);
        histogram.record(70_000);
        assert_eq!(histogram.percentile_upper_ms(99), 70_000);
    }
}
