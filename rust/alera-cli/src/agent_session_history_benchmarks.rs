//! Opt-in synthetic workload. Run alone to avoid unrelated test-process I/O.
use super::*;
use serde_json::json;
use std::fs;
use std::io::{BufWriter, Write};

const FILES: usize = 256;

struct Fixture {
    _home: tempfile::TempDir,
    roots: Vec<ProviderHistoryRoot>,
    workspaces: Vec<RegisteredHistoryWorkspace>,
    changed_file: PathBuf,
    original: String,
    file_bytes: u64,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let workspaces = (0..8)
            .map(|i| {
                let path = home.path().join(format!("workspace-{i}"));
                fs::create_dir_all(&path).unwrap();
                RegisteredHistoryWorkspace {
                    runtime_instance_id: "benchmark-runtime".into(),
                    project_id: format!("project-{}", i / 2),
                    workspace_id: format!("workspace-{i}"),
                    path,
                }
            })
            .collect::<Vec<_>>();
        let roots = [
            ("codex", "codex-test", "codex/sessions/2026/11/24"),
            ("claude", "claude-test", "claude/projects/synthetic"),
        ]
        .into_iter()
        .map(|(provider, account, path)| {
            let path = home.path().join(path);
            fs::create_dir_all(&path).unwrap();
            ProviderHistoryRoot {
                provider,
                account_id: account.into(),
                path,
            }
        })
        .collect::<Vec<_>>();
        let mut changed_file = PathBuf::new();
        let mut file_bytes = 0;
        let filler = json!({"type":"assistant","text":"x".repeat(500)});
        for (root_index, root) in roots.iter().enumerate() {
            for i in 0..FILES / 2 {
                let n = root_index * (FILES / 2) + i;
                let id = format!("550e8400-e29b-41d4-a716-{n:012}");
                let cwd = &workspaces[n % workspaces.len()].path;
                let message = format!("historybenchmark keyboard workspace {} session {n}", n % 8);
                let name = if root.provider == "codex" {
                    format!("rollout-2026-11-24T12-00-00-{id}.jsonl")
                } else {
                    format!("{id}.jsonl")
                };
                let path = root.path.join(name);
                let mut writer = BufWriter::new(fs::File::create(&path).unwrap());
                if root.provider == "codex" {
                    writeln!(
                        writer,
                        "{}",
                        json!({"type":"session_meta","payload":{"id":id,"cwd":cwd}})
                    )
                    .unwrap();
                }
                for _ in 0..6 {
                    writeln!(writer, "{filler}").unwrap();
                }
                if root.provider == "codex" {
                    writeln!(writer, "{}", json!({"type":"event_msg","payload":{"type":"user_message","message":message}})).unwrap();
                } else {
                    writeln!(writer, "{}", json!({"type":"user","sessionId":id,"cwd":cwd,"message":{"role":"user","content":message}})).unwrap();
                }
                // The remainder is intentionally beyond the parsed preview.
                for _ in 0..64 {
                    writeln!(writer, "{filler}").unwrap();
                }
                writer.flush().unwrap();
                file_bytes += fs::metadata(&path).unwrap().len();
                if n == 0 {
                    changed_file = path;
                }
            }
            for i in 0..64 {
                fs::write(root.path.join(format!("zz-ignored-{i:03}.txt")), b"ignored").unwrap();
            }
        }
        let original = fs::read_to_string(&changed_file).unwrap();
        Self {
            _home: home,
            roots,
            workspaces,
            changed_file,
            original,
            file_bytes,
        }
    }

    fn change(&self, iteration: usize) {
        let changed = self.original.replace(
            "historybenchmark keyboard",
            &format!(
                "historybenchmark keyboard {}",
                "revision".repeat(iteration + 1)
            ),
        );
        fs::write(&self.changed_file, changed).unwrap();
    }
}

#[cfg(windows)]
fn process_counters() -> [u64; 6] {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetProcessIoCounters, GetProcessTimes, IO_COUNTERS,
    };
    let mut io = IO_COUNTERS::default();
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // The pseudo-handle belongs to this process and all outputs are initialized.
    unsafe {
        let process = GetCurrentProcess();
        GetProcessIoCounters(process, &mut io).unwrap();
        GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user).unwrap();
    }
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    [
        io.ReadOperationCount,
        io.WriteOperationCount,
        io.OtherOperationCount,
        io.ReadTransferCount,
        io.WriteTransferCount,
        (ticks(kernel) + ticks(user)) / 10,
    ]
}

struct Sample {
    total: Duration,
    response: HistorySearchResponse,
    #[cfg(windows)]
    io: [u64; 6],
}

async fn sample(fixture: &Fixture, cache: &Path, state: &HistorySearchState) -> Sample {
    let query = AgentSessionSearchQuery {
        text: "historybenchmark".into(),
        max_results: 20,
        ..Default::default()
    };
    #[cfg(windows)]
    let before = process_counters();
    let start = Instant::now();
    let response = search_history_on_demand(
        cache,
        state,
        &fixture.roots,
        &fixture.workspaces,
        &query,
        true,
    )
    .await
    .unwrap();
    let total = start.elapsed();
    #[cfg(windows)]
    let io = {
        let after = process_counters();
        std::array::from_fn(|i| after[i] - before[i])
    };
    assert_eq!(response.visited_files, FILES);
    assert_eq!(response.hits.len(), 20);
    assert_eq!(response.parsed_files + response.reused_files, FILES);
    assert!(response
        .hits
        .iter()
        .all(|hit| !hit.session.provider_identity_verified));
    Sample {
        total,
        response,
        #[cfg(windows)]
        io,
    }
}

fn quantile(mut values: Vec<u64>, percent: usize) -> u64 {
    assert!(!values.is_empty());
    values.sort_unstable();
    values[(values.len() * percent).div_ceil(100) - 1]
}

fn report(name: &str, samples: &[Sample]) {
    let phases: [(&str, fn(&Sample) -> Duration); 7] = [
        ("total", |s| s.total),
        ("scan", |s| s.response.timings.scan),
        ("index_open_or_validate", |s| {
            s.response.timings.index_open_or_validate
        }),
        ("prepare_keys", |s| s.response.timings.prepare_keys),
        ("upsert", |s| s.response.timings.upsert),
        ("fts", |s| s.response.timings.fts),
        ("rank", |s| s.response.timings.rank),
    ];
    println!("BENCH scenario={name} samples={}", samples.len());
    let show = |metric: &str, values: Vec<u64>| {
        println!(
            "  {metric} p50={} p95={}",
            quantile(values.clone(), 50),
            quantile(values, 95)
        );
    };
    for (phase, select) in phases {
        show(
            &format!("{phase}_us"),
            samples
                .iter()
                .map(|s| select(s).as_micros() as u64)
                .collect(),
        );
    }
    let counts: [(&str, fn(&Sample) -> usize); 12] = [
        ("parsed", |s| s.response.parsed_files),
        ("reused", |s| s.response.reused_files),
        ("added", |s| s.response.writes.added),
        ("updated", |s| s.response.writes.updated),
        ("unchanged", |s| s.response.writes.unchanged),
        ("sql_write_statements", |s| {
            s.response.writes.write_statements
        }),
        ("json_buffer_starts", |s| {
            s.response.writes.json_buffer_starts
        }),
        ("warm_check_queries", |s| {
            s.response.writes.warm_check_queries
        }),
        ("warm_check_hits", |s| s.response.writes.warm_check_hits),
        ("existing_rows_loaded", |s| {
            s.response.writes.existing_rows_loaded
        }),
        ("selective_lookup_queries", |s| {
            s.response.writes.selective_lookup_queries
        }),
        ("selective_mismatches", |s| {
            s.response.writes.selective_mismatches
        }),
    ];
    for (metric, select) in counts {
        show(metric, samples.iter().map(|s| select(s) as u64).collect());
    }
    #[cfg(windows)]
    for (i, name) in [
        "process_read_ops",
        "process_write_ops",
        "process_other_ops",
        "process_read_bytes",
        "process_write_bytes",
        "process_cpu_us",
    ]
    .into_iter()
    .enumerate()
    {
        show(name, samples.iter().map(|s| s.io[i]).collect());
    }
}

#[tokio::test]
#[ignore = "opt-in synthetic performance benchmark; run alone with --ignored --nocapture"]
async fn history_search_phase_benchmark() {
    let fixture = Fixture::new();
    println!(
        "BENCH fixture files={FILES} transcript_bytes={} workspaces={} accounts={} build={}",
        fixture.file_bytes,
        fixture.workspaces.len(),
        fixture.roots.len(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    let mut cold = Vec::new();
    // Cold refers to application previews and database creation, not an OS
    // cache flush. Do not interfere with other applications or disk caches.
    for _ in 0..7 {
        let cache = tempfile::tempdir().unwrap();
        let state = HistorySearchState::default();
        let s = sample(&fixture, cache.path(), &state).await;
        assert_eq!(s.response.parsed_files, FILES);
        assert_eq!(s.response.writes.added, FILES);
        assert!(!s.response.index_reused);
        cold.push(s);
        state.lock().await.close_index().await;
    }
    report("cold", &cold);

    let cache = tempfile::tempdir().unwrap();
    let state = HistorySearchState::default();
    let _ = sample(&fixture, cache.path(), &state).await;
    let mut warm = Vec::new();
    for _ in 0..21 {
        let s = sample(&fixture, cache.path(), &state).await;
        assert!(s.response.index_reused);
        assert_eq!(s.response.reused_files, FILES);
        assert_eq!(s.response.writes.unchanged, FILES);
        assert_eq!(s.response.writes.write_statements, 0);
        assert_eq!(s.response.writes.warm_check_hits, 1);
        assert_eq!(s.response.writes.existing_rows_loaded, 0);
        warm.push(s);
    }
    report("warm", &warm);

    let mut reopen = Vec::new();
    for _ in 0..7 {
        state.lock().await.close_index().await;
        let s = sample(&fixture, cache.path(), &state).await;
        assert!(!s.response.index_reused);
        assert_eq!(s.response.reused_files, FILES);
        assert_eq!(s.response.writes.unchanged, FILES);
        reopen.push(s);
    }
    report("index_reopen", &reopen);

    let mut changed = Vec::new();
    for i in 0..7 {
        fixture.change(i);
        let s = sample(&fixture, cache.path(), &state).await;
        assert_eq!(s.response.parsed_files, 1);
        assert_eq!(s.response.reused_files, FILES - 1);
        assert_eq!(s.response.writes.updated, 1);
        assert_eq!(s.response.writes.unchanged, FILES - 1);
        assert_eq!(s.response.writes.selective_lookup_queries, 1);
        assert_eq!(s.response.writes.selective_mismatches, 1);
        assert_eq!(s.response.writes.existing_rows_loaded, 1);
        changed.push(s);
    }
    report("one_file_changed", &changed);
    state.lock().await.close_index().await;
}
