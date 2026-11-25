//! Shapes the `resources.snapshot` wire payload.
//!
//! Kept apart from the sampler so the JSON field names and the arithmetic that
//! feeds them stay readable side by side. Every byte field is named `*Bytes` and
//! carries bytes; CPU is percent of a single core, so it can exceed 100.

use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::process_tree::SubtreeUsage;
use super::SessionPidRoot;

/// History keys for the two rows that are not sessions.
pub const HOST_HISTORY_KEY: &str = "__host__";
pub const APP_HISTORY_KEY: &str = "__app__";

/// The payload returned before the first sweep lands, so a client that asks the
/// instant the ticker starts gets a well-formed answer instead of an error.
pub fn warming_snapshot() -> Value {
    json!({
        "collectedAt": Utc::now().timestamp_millis(),
        "warming": true,
        "host": {
            "totalMemoryBytes": 0,
            "availableMemoryBytes": 0,
            "usedMemoryBytes": 0,
            "memoryUsagePercent": 0.0,
            "cpuCoreCount": 0,
            "loadAverage1m": 0.0,
        },
        "processes": { "host": Value::Null, "app": Value::Null },
        "sessions": [],
        "totals": { "cpuPercent": 0.0, "memoryBytes": 0 },
    })
}

pub fn process_json(pid: u32, usage: SubtreeUsage, history: impl Serialize) -> Value {
    json!({
        "pid": pid,
        "cpuPercent": usage.cpu_percent,
        "memoryBytes": usage.memory_bytes,
        "processCount": usage.process_count,
        "history": history,
    })
}

pub(super) fn session_json(
    root: &SessionPidRoot,
    measured: bool,
    usage: SubtreeUsage,
    history: impl Serialize,
) -> Value {
    json!({
        "sessionId": root.session_id,
        "workspaceId": root.workspace_id,
        "tabId": root.tab_id,
        "running": root.running,
        "shellPid": root.shell.map(|shell| shell.pid),
        "measured": measured,
        "cpuPercent": usage.cpu_percent,
        "memoryBytes": usage.memory_bytes,
        "processCount": usage.process_count,
        "history": history,
    })
}

/// The rows are already JSON Values. The json! expression arm would
/// serialize them into a second tree; transfer their ownership instead.
pub(super) fn attach_rows(
    mut snapshot: Value,
    host: Value,
    app: Option<Value>,
    sessions: Vec<Value>,
) -> Value {
    snapshot["processes"] = Value::Object(Map::from_iter([
        ("host".to_owned(), host),
        ("app".to_owned(), app.unwrap_or(Value::Null)),
    ]));
    snapshot["sessions"] = Value::Array(sessions);
    snapshot
}

pub fn accumulate(totals: &mut SubtreeUsage, usage: SubtreeUsage) {
    totals.cpu_percent += usage.cpu_percent;
    totals.memory_bytes = totals.memory_bytes.saturating_add(usage.memory_bytes);
    totals.process_count += usage.process_count;
}

pub fn memory_usage_percent(total: u64, available: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (total.saturating_sub(available) as f64) * 100.0 / (total as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_usage_percent_handles_an_unknown_total() {
        assert_eq!(memory_usage_percent(0, 0), 0.0);
    }

    #[test]
    fn memory_usage_percent_reports_the_used_share() {
        assert!((memory_usage_percent(1000, 250) - 75.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_warming_snapshot_is_shaped_like_a_real_one() {
        let snapshot = warming_snapshot();

        assert_eq!(snapshot["warming"], json!(true));
        assert!(snapshot["sessions"].is_array());
        assert!(snapshot["host"]["totalMemoryBytes"].is_number());
        assert!(snapshot["totals"]["memoryBytes"].is_number());
    }

    #[test]
    fn accumulating_sums_every_field() {
        let mut totals = SubtreeUsage {
            cpu_percent: 1.0,
            memory_bytes: 100,
            process_count: 1,
        };

        accumulate(
            &mut totals,
            SubtreeUsage {
                cpu_percent: 2.0,
                memory_bytes: 200,
                process_count: 2,
            },
        );

        assert_eq!(totals.memory_bytes, 300);
        assert_eq!(totals.process_count, 3);
        assert!((totals.cpu_percent - 3.0).abs() < f64::EPSILON);
    }

    fn fixture_roots(count: usize) -> Vec<SessionPidRoot> {
        use super::super::ShellProcess;
        (0..count)
            .map(|i| SessionPidRoot {
                session_id: format!("resource-session-{i:04}"),
                workspace_id: format!("workspace-{:03}", i / 4),
                tab_id: format!("terminal-{i:04}"),
                running: i % 5 != 0,
                shell: (i % 7 != 0).then_some(ShellProcess {
                    pid: i as u32 + 100,
                    start_time: 1000,
                }),
            })
            .collect()
    }

    fn project_fixture(
        history: &mut super::super::history::ResourceHistory,
        roots: &[SessionPidRoot],
        step: u64,
        now: std::time::Instant,
        app_pid: Option<u32>,
        legacy: bool,
    ) -> Value {
        let mut totals = SubtreeUsage::default();
        let mut sessions = Vec::with_capacity(roots.len());
        for (i, root) in roots.iter().enumerate() {
            let measured = root.shell.is_some() && i % 11 != 0;
            let usage = if measured && root.running {
                SubtreeUsage {
                    cpu_percent: (i % 8) as f64 + 0.25,
                    memory_bytes: 1024 * (i as u64 + step),
                    process_count: 2,
                }
            } else {
                SubtreeUsage::default()
            };
            accumulate(&mut totals, usage);
            sessions.push(if legacy {
                session_json(
                    root,
                    measured,
                    usage,
                    history.record_legacy(&root.session_id, usage.memory_bytes, now),
                )
            } else {
                session_json(
                    root,
                    measured,
                    usage,
                    history.record(&root.session_id, usage.memory_bytes, now),
                )
            });
        }
        let host_usage = SubtreeUsage {
            cpu_percent: 1.0,
            memory_bytes: 4096 + step,
            process_count: 1,
        };
        let app_usage = if app_pid.is_some() {
            SubtreeUsage {
                cpu_percent: 2.0,
                memory_bytes: 8192 + step,
                process_count: 1,
            }
        } else {
            SubtreeUsage::default()
        };
        accumulate(&mut totals, host_usage);
        accumulate(&mut totals, app_usage);
        let (host, app) = if legacy {
            let host = process_json(
                42,
                host_usage,
                history.record_legacy(HOST_HISTORY_KEY, host_usage.memory_bytes, now),
            );
            let app_history = history.record_legacy(APP_HISTORY_KEY, app_usage.memory_bytes, now);
            (
                host,
                app_pid.map(|pid| process_json(pid, app_usage, app_history)),
            )
        } else {
            let host = process_json(
                42,
                host_usage,
                history.record(HOST_HISTORY_KEY, host_usage.memory_bytes, now),
            );
            let app_history = history.record(APP_HISTORY_KEY, app_usage.memory_bytes, now);
            (
                host,
                app_pid.map(|pid| process_json(pid, app_usage, app_history)),
            )
        };
        history.evict_stale(now);
        let mut snapshot = json!({
            "collectedAt": step,
            "warming": step < 3,
            "host": {
                "totalMemoryBytes": 65536,
                "availableMemoryBytes": 16384,
                "usedMemoryBytes": 49152,
                "memoryUsagePercent": 75.0,
                "cpuCoreCount": 20,
                "loadAverage1m": 0.0,
            },
            "totals": {
                "cpuPercent": totals.cpu_percent,
                "memoryBytes": totals.memory_bytes,
            },
        });
        if legacy {
            // The original expression arms serialized the existing Values.
            snapshot["processes"] = json!({"host": host, "app": app});
            snapshot["sessions"] = json!(sessions);
            snapshot
        } else {
            attach_rows(snapshot, host, app, sessions)
        }
    }

    #[test]
    fn moved_snapshot_matches_original_through_history_wrap_and_eviction() {
        use super::super::history::{ResourceHistory, HISTORY_CAPACITY, HISTORY_STALE};
        use std::time::{Duration, Instant};
        let roots = fixture_roots(24);
        let now = Instant::now();
        let mut original = ResourceHistory::default();
        let mut optimized = ResourceHistory::default();
        let mut retained = None;
        for step in 0..(HISTORY_CAPACITY as u64 + 8) {
            let app_pid = (step % 3 != 0).then_some(43);
            let time = now + Duration::from_secs(step);
            let before = project_fixture(&mut original, &roots, step, time, app_pid, true);
            let after = project_fixture(&mut optimized, &roots, step, time, app_pid, false);
            assert_eq!(after, before);
            if step == 4 {
                retained = Some(after);
            }
        }
        let time = now + HISTORY_STALE + Duration::from_secs(HISTORY_CAPACITY as u64 + 9);
        let before = project_fixture(&mut original, &roots[..2], 1000, time, None, true);
        let after = project_fixture(&mut optimized, &roots[..2], 1000, time, None, false);
        assert_eq!(after, before);
        let before = project_fixture(&mut original, &roots, 1001, time, Some(43), true);
        let after = project_fixture(&mut optimized, &roots, 1001, time, Some(43), false);
        assert_eq!(after, before);
        assert_eq!(after["sessions"][2]["history"].as_array().unwrap().len(), 1);
        assert_eq!(
            retained.unwrap()["sessions"][2]["history"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
    }

    #[test]
    fn attaching_rows_transfers_existing_session_and_history_allocations() {
        let sessions = vec![json!({"sessionId":"session-1","history":[1,2,3]})];
        let rows_ptr = sessions.as_ptr();
        let history_ptr = sessions[0]["history"].as_array().unwrap().as_ptr();
        let host = json!({"pid":42,"history":[4,5,6]});
        let host_ptr = host["history"].as_array().unwrap().as_ptr();
        let snapshot = attach_rows(json!({"collectedAt":123}), host, None, sessions);
        assert_eq!(snapshot["sessions"].as_array().unwrap().as_ptr(), rows_ptr);
        assert_eq!(
            snapshot["sessions"][0]["history"]
                .as_array()
                .unwrap()
                .as_ptr(),
            history_ptr
        );
        assert_eq!(
            snapshot["processes"]["host"]["history"]
                .as_array()
                .unwrap()
                .as_ptr(),
            host_ptr
        );
        assert_eq!(snapshot["processes"]["app"], Value::Null);
    }

    #[test]
    #[ignore = "opt-in synthetic resource snapshot projection benchmark"]
    fn resource_snapshot_projection_benchmark() {
        use super::super::history::{ResourceHistory, HISTORY_CAPACITY};
        use std::hint::black_box;
        use std::time::{Duration, Instant};
        const SAMPLES: u64 = 100;
        for count in [16, 100, 256] {
            let roots = fixture_roots(count);
            let run = |legacy| {
                let mut history = ResourceHistory::default();
                let now = Instant::now();
                for step in 0..HISTORY_CAPACITY as u64 {
                    black_box(project_fixture(
                        &mut history,
                        &roots,
                        step,
                        now + Duration::from_secs(step),
                        Some(43),
                        legacy,
                    ));
                }
                let started = Instant::now();
                for step in HISTORY_CAPACITY as u64..HISTORY_CAPACITY as u64 + SAMPLES {
                    black_box(project_fixture(
                        &mut history,
                        &roots,
                        step,
                        now + Duration::from_secs(step),
                        Some(43),
                        legacy,
                    ));
                }
                println!(
                    "RESOURCE_BENCH sessions={count} samples={SAMPLES} version={} elapsed_us={}",
                    if legacy { "before" } else { "after" },
                    started.elapsed().as_micros()
                );
            };
            for legacy in [true, false, false, true, true, false] {
                run(legacy);
            }
        }
    }
}
