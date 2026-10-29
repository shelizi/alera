use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::deferred_admission::DeferredRequestClass;
use super::queue_wait_histogram::QueueWaitHistogram;

#[derive(Default)]
struct RequestTypeMetrics {
    active: usize,
    pending: usize,
    queued: u64,
    started: u64,
    completed: u64,
    rejected: u64,
    disconnected: u64,
    queue_wait_ms: u64,
    max_queue_wait_ms: u64,
    queue_wait_histogram: QueueWaitHistogram,
}

#[derive(Default)]
pub(super) struct AdmissionMetrics {
    request_types: BTreeMap<(String, DeferredRequestClass), RequestTypeMetrics>,
    rejected: u64,
    disconnected: u64,
    stale_completions: u64,
}

impl AdmissionMetrics {
    fn for_type(
        &mut self,
        request_type: &str,
        class: DeferredRequestClass,
    ) -> &mut RequestTypeMetrics {
        self.request_types
            .entry((request_type.to_string(), class))
            .or_default()
    }

    pub(super) fn queued(&mut self, request_type: &str, class: DeferredRequestClass) {
        let metrics = self.for_type(request_type, class);
        metrics.queued += 1;
        metrics.pending += 1;
    }

    pub(super) fn started(
        &mut self,
        request_type: &str,
        class: DeferredRequestClass,
        was_queued: bool,
        queue_wait_ms: u64,
    ) {
        let metrics = self.for_type(request_type, class);
        metrics.started += 1;
        metrics.active += 1;
        if was_queued {
            metrics.pending = metrics.pending.saturating_sub(1);
        }
        metrics.queue_wait_ms = metrics.queue_wait_ms.saturating_add(queue_wait_ms);
        metrics.max_queue_wait_ms = metrics.max_queue_wait_ms.max(queue_wait_ms);
        metrics.queue_wait_histogram.record(queue_wait_ms);
    }

    pub(super) fn completed(&mut self, request_type: &str, class: DeferredRequestClass) {
        let metrics = self.for_type(request_type, class);
        metrics.completed += 1;
        metrics.active = metrics.active.saturating_sub(1);
    }

    pub(super) fn rejected(&mut self, request_type: &str, class: DeferredRequestClass) {
        self.for_type(request_type, class).rejected += 1;
        self.rejected += 1;
    }

    pub(super) fn disconnected(
        &mut self,
        request_type: &str,
        class: DeferredRequestClass,
        was_pending: bool,
    ) {
        let metrics = self.for_type(request_type, class);
        metrics.disconnected += 1;
        if was_pending {
            metrics.pending = metrics.pending.saturating_sub(1);
        }
        self.disconnected += 1;
    }

    pub(super) fn stale_completion(&mut self) {
        self.stale_completions += 1;
    }

    pub(super) fn snapshot(
        &self,
        active: usize,
        pending: usize,
        active_limit: usize,
        capacity: usize,
    ) -> Value {
        let request_types: Vec<Value> = self
            .request_types
            .iter()
            .map(|((request_type, class), metrics)| {
                json!({
                    "requestType": request_type,
                    "requestClass": class.as_str(),
                    "active": metrics.active,
                    "pending": metrics.pending,
                    "queued": metrics.queued,
                    "started": metrics.started,
                    "completed": metrics.completed,
                    "rejected": metrics.rejected,
                    "disconnected": metrics.disconnected,
                    "queueWaitMs": metrics.queue_wait_ms,
                    "maxQueueWaitMs": metrics.max_queue_wait_ms,
                    "queueWaitSamples": metrics.queue_wait_histogram.samples(),
                    "queueWaitP95Ms": metrics.queue_wait_histogram.percentile_upper_ms(95),
                    "queueWaitP99Ms": metrics.queue_wait_histogram.percentile_upper_ms(99),
                })
            })
            .collect();
        json!({
            "active": active,
            "pending": pending,
            "activeLimit": active_limit,
            "capacity": capacity,
            "rejected": self.rejected,
            "disconnected": self.disconnected,
            "staleCompletions": self.stale_completions,
            "requestTypes": request_types,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_type<'a>(snapshot: &'a Value, name: &str) -> &'a Value {
        snapshot["requestTypes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["requestType"] == name)
            .expect("request type metrics should be present")
    }

    #[test]
    fn queue_wait_percentiles_use_bounded_buckets_without_distorting_the_existing_sum() {
        let mut metrics = AdmissionMetrics::default();
        for _ in 0..98 {
            metrics.started("test.bulk", DeferredRequestClass::Bulk, false, 0);
        }
        metrics.started("test.bulk", DeferredRequestClass::Bulk, true, 7);
        metrics.started("test.bulk", DeferredRequestClass::Bulk, true, 500);
        let snapshot = metrics.snapshot(0, 0, 8, 64);
        let bulk = request_type(&snapshot, "test.bulk");
        assert_eq!(bulk["queueWaitSamples"], 100);
        assert_eq!(bulk["queueWaitMs"], 507);
        assert_eq!(bulk["maxQueueWaitMs"], 500);
        assert_eq!(bulk["queueWaitP95Ms"], 0);
        assert_eq!(bulk["queueWaitP99Ms"], 8);
    }

    #[test]
    fn unstarted_queued_work_is_not_a_sample_and_classes_stay_isolated() {
        let mut metrics = AdmissionMetrics::default();
        metrics.queued("test.bulk", DeferredRequestClass::Bulk);
        metrics.started(
            "test.critical",
            DeferredRequestClass::DispatchCritical,
            false,
            5,
        );
        let snapshot = metrics.snapshot(1, 1, 8, 64);
        let bulk = request_type(&snapshot, "test.bulk");
        assert_eq!(bulk["queueWaitSamples"], 0);
        assert_eq!(bulk["queueWaitP95Ms"], 0);
        assert_eq!(bulk["queueWaitP99Ms"], 0);
        let critical = request_type(&snapshot, "test.critical");
        assert_eq!(critical["queueWaitSamples"], 1);
        assert_eq!(critical["queueWaitP95Ms"], 8);
        assert_eq!(critical["queueWaitP99Ms"], 8);
    }
}
