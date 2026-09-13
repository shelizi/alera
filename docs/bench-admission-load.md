# Deferred admission load benchmark

## Purpose

This harness measures queue wait, class-priority draining, and delayed-timer reservation behavior in the terminal host `DeferredAdmission` queue. It is a repeatable load observation, not a performance gate: the tests report machine-dependent timings and assert only structural behavior.

All benchmark tests are marked `#[ignore]`, so the default test suite does not run them.

## Run command

Run the command from the Rust workspace directory `rust/`:

```powershell
cargo test -p alera-cli --release deferred_admission_load_bench -- --ignored --nocapture
```

`--ignored` enables the three benchmark tests and `--nocapture` keeps their tables on stdout.

## Scenarios

### Saturated submit queue-wait distribution

The harness creates a paused admission with an active limit of 4 and submits 150 jobs in a fixed round-robin class pattern. All enqueue timestamps are recorded immediately after `schedule` returns while the test gate prevents starts. Four permits then admit fixed 2 ms work, allowing a real queue to drain. The table reports per-class and overall enqueue-to-task-start latency, followed by wall-clock throughput. The cross-check compares the locally measured sum and maximum with the corresponding `snapshot()` fields.

### Three-class fairness observation

With one active slot and no permits, the harness submits three bulk jobs, then three maintenance jobs, then three dispatch-critical jobs. It releases each started job one at a time and records the actual start order. The assertion checks that the later dispatch-critical job `#6` starts before earlier bulk job `#0`; the expected complete order also demonstrates class priority and FIFO order within each class.

### Delayed timers and slot reservation

Two active blocker jobs hold all active slots. Six 100 ms `schedule_delayed` timers are armed while the active and pending snapshot values remain 2 and 0. The timer request types are absent from metrics, and all six deliveries are observed while the blockers still occupy both active slots. With active plus delayed occupancy at the total limit of 8, a seventh timer is rejected synchronously. The timer table reports armed, delivered, rejected, and enqueue-to-delivery latency counts.

## Field and column definitions

- `class`: the admission class, in drain priority order: `dispatchCritical`, `maintenance`, `bulk`.
- `count`: locally recorded jobs in the row.
- `min ms`, `median ms`, `p90 ms`, `max ms`: locally measured enqueue-to-start wait in milliseconds. `p90` uses the nearest-rank value at the 90th percentile. Median and maximum are labeled explicitly because they are the most useful comparison points for repeated runs.
- `started`, `queued`: the per-request-type counters from `snapshot()`.
- `measured sum ms`, `measured max`: sums and maxima of the local per-job samples.
- `snapshot sum ms`, `snapshot max`: `queueWaitMs` and `maxQueueWaitMs` from the matching `snapshot()` request-type entry. These counters are integer milliseconds measured when admission drains a queued job, so small scheduler and timestamp-placement differences from the local task-start samples are expected.
- `elapsed ms`: wall-clock time from before submission through all fixed-work completions.
- `jobs/sec`: submitted job count divided by `elapsed ms`.
- `armed`, `delivered`, `rejected`: successful delayed-timer submissions, observed task deliveries, and the synchronous over-capacity rejection.
- `before` and `during`: active-slot, pending-job, and timer-metric-entry values before delivery and while deliveries are being observed.

## Baseline from this machine

Captured with the release command above on Windows 11 x64, Rust 1.98.0, CPU `Intel(R) Core(TM) i5-14500`. These values are a single baseline run and will vary with scheduler load.

```text
=== Three-Class Fairness Observation ===
submitted order: bulk -> maintenance -> dispatchCritical
actual start order: dispatchCritical#6 -> dispatchCritical#7 -> dispatchCritical#8 -> maintenance#3 -> maintenance#4 -> maintenance#5 -> bulk#0 -> bulk#1 -> bulk#2
=== Three-Class Fairness Waits ===
class                 count       min ms    median ms       p90 ms       max ms
--------------------------------------------------------------------------------
dispatchCritical          3         0.15         0.22         0.22         0.22
maintenance               3         0.23         0.23         0.23         0.23
bulk                      3         0.24         0.24         0.24         0.24
overall                   9         0.15         0.23         0.24         0.24

assertion: dispatchCritical#6 rank 0 < bulk#0 rank 6 -> PASS

=== Delayed Timer Delivery ===
lane                    armed  delivered   rejected       min ms    median ms       max ms
----------------------------------------------------------------------------------------
schedule_delayed            6          6          1       101.21       101.22       101.23

=== Delayed Timer Slot Observation ===
measurement                    before       during         expected
--------------------------------------------------------------------
active slots                        2            2        unchanged
pending jobs                        0            0        unchanged
timer metric entries                0            0           absent

=== Saturated Submit Queue-Wait Distribution ===
class                 count       min ms    median ms       p90 ms       max ms
--------------------------------------------------------------------------------
dispatchCritical         50         0.25        20.72        37.67        40.89
maintenance              50        40.97        62.08        77.77        80.54
bulk                     50        83.71       102.81       118.08       121.60
overall                 150         0.25        62.08       108.88       121.60

=== Wall-Clock Throughput ===
workload               jobs       elapsed ms         jobs/sec
--------------------------------------------------------------
fixed 2 ms work         150           125.12          1198.83

=== Snapshot Queue-Wait Cross-Check ===
class               started   queued    measured sum ms    snapshot sum ms     measured max     snapshot max
----------------------------------------------------------------------------------------------------------------
dispatchCritical         50       50             986.90                956            40.89               40
maintenance              50       50            3111.51               3092            80.54               80
bulk                     50       50            5090.24               5062           121.60              121
```
