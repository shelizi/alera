use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::deferred_admission::{DeferredAdmission, DeferredRequestClass};
use super::deferred_admission_tests::request_type_entry;
use crate::terminal_host::host_error::HostError;
const BENCH_CLASSES: [DeferredRequestClass; 3] = [
    DeferredRequestClass::DispatchCritical,
    DeferredRequestClass::Maintenance,
    DeferredRequestClass::Bulk,
];

#[derive(Clone, Copy)]
struct WaitSample {
    class: DeferredRequestClass,
    wait_ms: f64,
}

struct WaitStats {
    min_ms: f64,
    median_ms: f64,
    p90_ms: f64,
    max_ms: f64,
}
fn median(sorted_values: &[f64]) -> f64 {
    let middle = sorted_values.len() / 2;
    if sorted_values.len().is_multiple_of(2) {
        (sorted_values[middle - 1] + sorted_values[middle]) / 2.0
    } else {
        sorted_values[middle]
    }
}

fn load_request_type(class: DeferredRequestClass) -> &'static str {
    match class {
        DeferredRequestClass::DispatchCritical => "bench.load.dispatchCritical",
        DeferredRequestClass::Maintenance => "bench.load.maintenance",
        DeferredRequestClass::Bulk => "bench.load.bulk",
    }
}

fn fairness_request_type(class: DeferredRequestClass) -> &'static str {
    match class {
        DeferredRequestClass::DispatchCritical => "bench.fairness.dispatchCritical",
        DeferredRequestClass::Maintenance => "bench.fairness.maintenance",
        DeferredRequestClass::Bulk => "bench.fairness.bulk",
    }
}

fn fairness_class(index: usize) -> DeferredRequestClass {
    if index < 3 {
        DeferredRequestClass::Bulk
    } else if index < 6 {
        DeferredRequestClass::Maintenance
    } else {
        DeferredRequestClass::DispatchCritical
    }
}

async fn enqueue_time_at(
    enqueue_times: &Arc<Mutex<Vec<Option<Instant>>>>,
    index: usize,
) -> Instant {
    loop {
        if let Some(enqueued_at) = enqueue_times.lock().unwrap()[index] {
            return enqueued_at;
        }
        tokio::task::yield_now().await;
    }
}

fn wait_stats(samples: &[WaitSample]) -> WaitStats {
    assert!(!samples.is_empty());
    let mut values: Vec<f64> = samples.iter().map(|sample| sample.wait_ms).collect();
    values.sort_by(f64::total_cmp);
    let p90_index = (values.len() * 90 + 99) / 100 - 1;
    WaitStats {
        min_ms: values[0],
        median_ms: median(&values),
        p90_ms: values[p90_index],
        max_ms: values[values.len() - 1],
    }
}

fn print_wait_row(label: &str, samples: &[WaitSample]) {
    let stats = wait_stats(samples);
    println!(
        "{label:<18} {count:>8} {min:>12.2} {median:>12.2} {p90:>12.2} {max:>12.2}",
        count = samples.len(),
        min = stats.min_ms,
        median = stats.median_ms,
        p90 = stats.p90_ms,
        max = stats.max_ms
    );
}
fn samples_for_class(samples: &[WaitSample], class: DeferredRequestClass) -> Vec<WaitSample> {
    samples
        .iter()
        .copied()
        .filter(|sample| sample.class == class)
        .collect()
}
fn print_wait_distribution(title: &str, samples: &[WaitSample]) {
    println!("=== {title} ===");
    println!(
        "{:<18} {:>8} {:>12} {:>12} {:>12} {:>12}",
        "class", "count", "min ms", "median ms", "p90 ms", "max ms"
    );
    println!("{}", "-".repeat(80));
    for class in BENCH_CLASSES {
        let class_samples = samples_for_class(samples, class);
        print_wait_row(class.as_str(), &class_samples);
    }
    print_wait_row("overall", samples);
    println!();
}
fn print_snapshot_cross_check(snapshot: &serde_json::Value, samples: &[WaitSample]) {
    println!("=== Snapshot Queue-Wait Cross-Check ===");
    println!(
        "{:<18} {:>8} {:>8} {:>18} {:>18} {:>16} {:>16}",
        "class",
        "started",
        "queued",
        "measured sum ms",
        "snapshot sum ms",
        "measured max",
        "snapshot max"
    );
    println!("{}", "-".repeat(112));
    for class in BENCH_CLASSES {
        let class_samples = samples_for_class(samples, class);
        let stats = wait_stats(&class_samples);
        let entry = request_type_entry(snapshot, load_request_type(class));
        println!(
            "{:<18} {:>8} {:>8} {:>18.2} {:>18} {:>16.2} {:>16}",
            class.as_str(),
            entry["started"].as_u64().unwrap(),
            entry["queued"].as_u64().unwrap(),
            class_samples
                .iter()
                .map(|sample| sample.wait_ms)
                .sum::<f64>(),
            entry["queueWaitMs"].as_u64().unwrap(),
            stats.max_ms,
            entry["maxQueueWaitMs"].as_u64().unwrap()
        );
    }
    println!();
}
fn assert_no_timer_metrics(snapshot: &serde_json::Value) {
    assert!(snapshot["requestTypes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| {
            !entry["requestType"]
                .as_str()
                .unwrap()
                .starts_with("bench.delayed.timer.")
        }));
}

async fn wait_for_completions(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<usize>,
    count: usize,
    timeout: Duration,
    message: &'static str,
) {
    tokio::time::timeout(timeout, async {
        for _ in 0..count {
            receiver.recv().await.expect(message);
        }
    })
    .await
    .expect(message);
}
#[tokio::test]
#[ignore]
async fn saturated_submit_queue_wait_distribution() {
    const ACTIVE_LIMIT: usize = 4;
    const JOB_COUNT: usize = 150;
    const WORK_DURATION: Duration = Duration::from_millis(2);

    let admission = Arc::new(DeferredAdmission::paused_with_limits(
        ACTIVE_LIMIT,
        JOB_COUNT,
        0,
    ));
    let enqueue_times = Arc::new(Mutex::new(vec![None; JOB_COUNT]));
    let samples = Arc::new(Mutex::new(Vec::with_capacity(JOB_COUNT)));
    let (completed_tx, mut completed_rx) = tokio::sync::mpsc::unbounded_channel();
    let wall_start = Instant::now();

    for index in 0..JOB_COUNT {
        let class = BENCH_CLASSES[index % BENCH_CLASSES.len()];
        let enqueue_times_for_task = Arc::clone(&enqueue_times);
        let samples_for_task = Arc::clone(&samples);
        let completed_tx_for_task = completed_tx.clone();
        admission
            .schedule(class, load_request_type(class), None, async move {
                let started_at = Instant::now();
                let enqueued_at = enqueue_time_at(&enqueue_times_for_task, index).await;
                samples_for_task.lock().unwrap().push(WaitSample {
                    class,
                    wait_ms: started_at
                        .saturating_duration_since(enqueued_at)
                        .as_secs_f64()
                        * 1000.0,
                });
                tokio::time::sleep(WORK_DURATION).await;
                completed_tx_for_task.send(index).unwrap();
            })
            .expect("the fixed load must fit within admission capacity");
        enqueue_times.lock().unwrap()[index] = Some(Instant::now());
    }

    let queued_snapshot = admission.snapshot();
    assert_eq!(queued_snapshot["pending"], JOB_COUNT);

    admission.add_test_permits(ACTIVE_LIMIT);
    wait_for_completions(
        &mut completed_rx,
        JOB_COUNT,
        Duration::from_secs(10),
        "fixed load did not complete within the benchmark guard",
    )
    .await;
    tokio::task::yield_now().await;
    let elapsed = wall_start.elapsed();

    let snapshot = admission.snapshot();
    assert_eq!(snapshot["active"], 0);
    assert_eq!(snapshot["pending"], 0);
    let samples = samples.lock().unwrap().clone();
    assert_eq!(samples.len(), JOB_COUNT);

    print_wait_distribution("Saturated Submit Queue-Wait Distribution", &samples);
    println!("=== Wall-Clock Throughput ===");
    println!(
        "{:<18} {:>8} {:>16} {:>16}",
        "workload", "jobs", "elapsed ms", "jobs/sec"
    );
    println!("{}", "-".repeat(62));
    println!(
        "{:<18} {:>8} {:>16.2} {:>16.2}",
        "fixed 2 ms work",
        JOB_COUNT,
        elapsed.as_secs_f64() * 1000.0,
        JOB_COUNT as f64 / elapsed.as_secs_f64()
    );
    println!();
    print_snapshot_cross_check(&snapshot, &samples);
}
#[tokio::test]
#[ignore]
async fn three_class_fairness_observation() {
    const ACTIVE_LIMIT: usize = 1;
    const JOB_COUNT: usize = 9;

    let admission = Arc::new(DeferredAdmission::paused_with_limits(
        ACTIVE_LIMIT,
        JOB_COUNT,
        1,
    ));
    let enqueue_times = Arc::new(Mutex::new(vec![None; JOB_COUNT]));
    let samples = Arc::new(Mutex::new(Vec::with_capacity(JOB_COUNT)));
    let (start_tx, mut start_rx) = tokio::sync::mpsc::unbounded_channel();
    let (completed_tx, mut completed_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut release_txs = Vec::with_capacity(JOB_COUNT);

    for index in 0..JOB_COUNT {
        let class = fairness_class(index);
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        release_txs.push(Some(release_tx));
        let enqueue_times_for_task = Arc::clone(&enqueue_times);
        let samples_for_task = Arc::clone(&samples);
        let start_tx_for_task = start_tx.clone();
        let completed_tx_for_task = completed_tx.clone();
        admission
            .schedule(class, fairness_request_type(class), None, async move {
                let started_at = Instant::now();
                let enqueued_at = enqueue_time_at(&enqueue_times_for_task, index).await;
                samples_for_task.lock().unwrap().push(WaitSample {
                    class,
                    wait_ms: started_at
                        .saturating_duration_since(enqueued_at)
                        .as_secs_f64()
                        * 1000.0,
                });
                start_tx_for_task.send((index, class)).unwrap();
                let _ = release_rx.await;
                completed_tx_for_task.send(index).unwrap();
            })
            .expect("the fixed fairness load must fit within admission capacity");
        enqueue_times.lock().unwrap()[index] = Some(Instant::now());
    }

    let queued_snapshot = admission.snapshot();
    assert_eq!(queued_snapshot["pending"], JOB_COUNT);

    admission.add_test_permits(ACTIVE_LIMIT);
    let mut start_order = Vec::with_capacity(JOB_COUNT);
    for _ in 0..JOB_COUNT {
        let (index, class) = tokio::time::timeout(Duration::from_secs(2), start_rx.recv())
            .await
            .expect("each queued job must start")
            .expect("the start channel must remain open");
        assert_eq!(class, fairness_class(index));
        start_order.push(index);
        release_txs[index]
            .take()
            .expect("each fairness job starts once")
            .send(())
            .expect("each fairness job must be released");
    }
    wait_for_completions(
        &mut completed_rx,
        JOB_COUNT,
        Duration::from_secs(2),
        "fairness jobs did not complete within the benchmark guard",
    )
    .await;
    let samples = samples.lock().unwrap().clone();
    assert_eq!(samples.len(), JOB_COUNT);

    assert_eq!(start_order, [6, 7, 8, 3, 4, 5, 0, 1, 2]);
    let critical_position = start_order.iter().position(|index| *index == 6).unwrap();
    let earlier_bulk_position = start_order.iter().position(|index| *index == 0).unwrap();
    assert!(critical_position < earlier_bulk_position);

    println!("=== Three-Class Fairness Observation ===");
    println!("submitted order: bulk -> maintenance -> dispatchCritical");
    let labels: Vec<String> = start_order
        .iter()
        .map(|index| format!("{}#{index}", fairness_class(*index).as_str()))
        .collect();
    println!("actual start order: {}", labels.join(" -> "));
    print_wait_distribution("Three-Class Fairness Waits", &samples);
    println!("assertion: dispatchCritical#6 rank {critical_position} < bulk#0 rank {earlier_bulk_position} -> PASS");
    println!();
}

fn print_timer_latency(samples: &[f64], armed: usize, rejected: usize) {
    let mut latencies = samples.to_vec();
    latencies.sort_by(f64::total_cmp);
    let median_ms = median(&latencies);
    println!("=== Delayed Timer Delivery ===");
    println!(
        "{:<18} {:>10} {:>10} {:>10} {:>12} {:>12} {:>12}",
        "lane", "armed", "delivered", "rejected", "min ms", "median ms", "max ms"
    );
    println!("{}", "-".repeat(88));
    println!(
        "{:<18} {:>10} {:>10} {:>10} {:>12.2} {:>12.2} {:>12.2}",
        "schedule_delayed",
        armed,
        samples.len(),
        rejected,
        latencies[0],
        median_ms,
        latencies[latencies.len() - 1]
    );
    println!();
}
#[tokio::test]
#[ignore]
async fn delayed_timers_do_not_occupy_slots_load() {
    const ACTIVE_LIMIT: usize = 2;
    const TIMER_COUNT: usize = 6;
    const TOTAL_LIMIT: usize = ACTIVE_LIMIT + TIMER_COUNT;
    const TIMER_DELAY: Duration = Duration::from_millis(100);

    let admission = Arc::new(DeferredAdmission::paused_with_limits(
        ACTIVE_LIMIT,
        TOTAL_LIMIT,
        0,
    ));
    admission.add_test_permits(ACTIVE_LIMIT);

    let mut blocker_release_txs = Vec::with_capacity(ACTIVE_LIMIT);
    let mut blocker_finished_rxs = Vec::with_capacity(ACTIVE_LIMIT);
    for index in 0..ACTIVE_LIMIT {
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
        blocker_release_txs.push(release_tx);
        blocker_finished_rxs.push(finished_rx);
        admission
            .schedule(
                DeferredRequestClass::Bulk,
                format!("bench.delayed.blocker.{index}"),
                None,
                async move {
                    let _ = release_rx.await;
                    let _ = finished_tx.send(());
                },
            )
            .expect("the active blocker must be admitted");
    }

    let enqueue_times = Arc::new(Mutex::new(vec![None; TIMER_COUNT]));
    let samples = Arc::new(Mutex::new(Vec::with_capacity(TIMER_COUNT)));
    let (delivered_tx, mut delivered_rx) = tokio::sync::mpsc::unbounded_channel();
    for index in 0..TIMER_COUNT {
        let enqueue_times_for_task = Arc::clone(&enqueue_times);
        let samples_for_task = Arc::clone(&samples);
        let delivered_tx_for_task = delivered_tx.clone();
        admission
            .schedule_delayed(
                TIMER_DELAY,
                DeferredRequestClass::Maintenance,
                format!("bench.delayed.timer.{index}"),
                None,
                async move {
                    let delivered_at = Instant::now();
                    let enqueued_at = enqueue_time_at(&enqueue_times_for_task, index).await;
                    samples_for_task.lock().unwrap().push(
                        delivered_at
                            .saturating_duration_since(enqueued_at)
                            .as_secs_f64()
                            * 1000.0,
                    );
                    delivered_tx_for_task.send(index).unwrap();
                },
            )
            .expect("the fixed timer load must fit within reservation capacity");
        enqueue_times.lock().unwrap()[index] = Some(Instant::now());
    }

    let saturated_snapshot = admission.snapshot();
    assert_eq!(saturated_snapshot["active"], ACTIVE_LIMIT);
    assert_eq!(saturated_snapshot["pending"], 0);

    let error = admission
        .schedule_delayed(
            Duration::from_secs(60),
            DeferredRequestClass::Maintenance,
            "bench.delayed.rejected",
            None,
            async {},
        )
        .expect_err("a timer past total reservation capacity must reject synchronously");
    let HostError::Conflict { code, details, .. } = error else {
        panic!("expected a typed backpressure conflict");
    };
    assert_eq!(
        code,
        super::deferred_admission::DEFERRED_REQUEST_BACKPRESSURE_CODE
    );
    assert_eq!(details["active"], ACTIVE_LIMIT);
    assert_eq!(details["capacity"], TOTAL_LIMIT);

    assert_no_timer_metrics(&saturated_snapshot);

    let mut delivered_indices = Vec::with_capacity(TIMER_COUNT);
    tokio::time::timeout(Duration::from_secs(3), async {
        for _ in 0..TIMER_COUNT {
            let index = delivered_rx
                .recv()
                .await
                .expect("every short delayed timer must deliver");
            assert_eq!(admission.snapshot()["active"], ACTIVE_LIMIT);
            assert_eq!(admission.snapshot()["pending"], 0);
            delivered_indices.push(index);
        }
    })
    .await
    .expect("short delayed timers did not deliver within the benchmark guard");
    delivered_indices.sort_unstable();
    assert_eq!(delivered_indices, (0..TIMER_COUNT).collect::<Vec<_>>());

    let samples = samples.lock().unwrap().clone();
    assert_eq!(samples.len(), TIMER_COUNT);

    print_timer_latency(&samples, TIMER_COUNT, 1);
    println!("=== Delayed Timer Slot Observation ===");
    println!(
        "{:<24} {:>12} {:>12} {:>16}",
        "measurement", "before", "during", "expected"
    );
    println!("{}", "-".repeat(68));
    for (measurement, before, during, expected) in [
        ("active slots", ACTIVE_LIMIT, ACTIVE_LIMIT, "unchanged"),
        ("pending jobs", 0, 0, "unchanged"),
        ("timer metric entries", 0, 0, "absent"),
    ] {
        println!("{measurement:<24} {before:>12} {during:>12} {expected:>16}");
    }
    println!();

    for release_tx in blocker_release_txs {
        release_tx.send(()).expect("each blocker must be released");
    }
    for finished_rx in blocker_finished_rxs {
        tokio::time::timeout(Duration::from_secs(2), finished_rx)
            .await
            .expect("each blocker must finish")
            .expect("each blocker completion must arrive");
    }
}
