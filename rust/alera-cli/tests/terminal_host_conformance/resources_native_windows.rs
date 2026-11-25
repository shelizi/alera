//! Opt-in real executable RPC regression with private runtime/home roots.
//! Run serially, with the isolated-host-conformance feature. The normal CLI
//! never honors the test isolation environment flag.

use super::*;
use std::path::Path;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

fn own_creation() -> u64 {
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    }
    .expect("read private test client's own creation time");
    (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime)
}

struct Client {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    id: i64,
}

impl Client {
    fn new(port: u16, token: &str) -> Self {
        let (mut writer, mut reader) = connect(port);
        handshake(&mut writer, &mut reader, token);
        Self {
            writer,
            reader,
            id: 0,
        }
    }

    fn call(&mut self, kind: &str, payload: Value) -> Value {
        self.id += 1;
        send(
            &mut self.writer,
            json!({"id":self.id,"type":kind,"payload":payload}),
        );
        let response = read_response(&mut self.reader, self.id);
        assert_eq!(
            response["ok"], true,
            "private conformance RPC {kind} failed"
        );
        response["payload"].clone()
    }

    fn wait_snapshot(&mut self, request: &Value, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(16);
        loop {
            let snapshot = self.call("resources.snapshot", request.clone());
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "no matching private resource snapshot"
            );
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

fn isolated_host(root: &Path, token: &str, native: bool) -> (HostGuard, u16) {
    let control = root.join("runtime-host.json");
    let home = root.join("agent-integration-home");
    let roaming = home.join("AppData/Roaming");
    let local = home.join("AppData/Local");
    for path in [&home, &roaming, &local] {
        std::fs::create_dir_all(path).unwrap();
    }
    let mut command = alera_command_with_isolated_home(root);
    command.args([
        "terminal-host",
        "--runtime-dir",
        root.to_str().unwrap(),
        "--control-file",
        control.to_str().unwrap(),
        "--token",
        token,
        "--empty-shutdown-delay-seconds",
        "60",
        "--detached-session-shutdown-delay-seconds",
        "60",
    ]);
    command
        .env("ALERA_TEST_ISOLATED_HOST", "1")
        .env(
            "ALERA_RESOURCE_NATIVE_WINDOWS",
            if native { "1" } else { "0" },
        )
        .env(
            "ALERA_HOST_LOG",
            "warn,resource_conformance=debug,alera::terminal_host::resources::windows_native=debug",
        )
        .env("ALERA_TERMINAL_WORKER_PROCESS", "0")
        .env("APPDATA", &roaming)
        .env("LOCALAPPDATA", &local)
        .env("XDG_CONFIG_HOME", &roaming)
        .env("ALERA_CLOUD_URL", "http://127.0.0.1:9")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for name in [
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "COPILOT_HOME",
        "GROK_HOME",
        "OPENCODE_CONFIG_DIR",
        "PI_CODING_AGENT_DIR",
        "AMP_CONFIG_DIR",
    ] {
        command.env_remove(name);
    }
    let guard = HostGuard(command.spawn().expect("private conformance Host"));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some((port, returned_token)) = read_control(&control) {
            assert_eq!(returned_token, token);
            return (guard, port);
        }
        assert!(Instant::now() < deadline, "private Host did not start");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn records(root: &Path) -> Vec<Value> {
    let logs = std::fs::read_to_string(root.join("logs/runtime.log")).expect("private Host log");
    logs.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn sample_record(root: &Path, snapshot: &Value) -> Value {
    let at = snapshot["collectedAt"].as_i64().unwrap().to_string();
    records(root)
        .into_iter()
        .rev()
        .find(|record| {
            record["msg"] == "resource snapshot completed" && record["collected_at"] == at
        })
        .expect("sample phase record from actual Host")
}

fn check_snapshot(snapshot: &Value, count: usize) {
    let sessions = snapshot["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), count);
    let mut memory = 0u64;
    for session in sessions {
        assert_eq!(session["measured"], true);
        assert_eq!(session["running"], true);
        assert!(session["shellPid"].as_u64().unwrap() > 0);
        assert!(session["processCount"].as_u64().unwrap() >= 1);
        let bytes = session["memoryBytes"].as_u64().unwrap();
        assert!(bytes > 0);
        memory += bytes;
    }
    for name in ["host", "app"] {
        let row = &snapshot["processes"][name];
        assert!(row["pid"].as_u64().unwrap() > 0);
        assert!(row["cpuPercent"].as_f64().unwrap() >= 0.0);
        memory += row["memoryBytes"].as_u64().unwrap();
    }
    assert_eq!(snapshot["totals"]["memoryBytes"].as_u64().unwrap(), memory);
    assert!(snapshot["host"]["totalMemoryBytes"].as_u64().unwrap() > 0);
}

#[test]
#[ignore = "real isolated Windows Host and PTYs; run explicitly with the isolated-host-conformance feature"]
fn native_backend_real_rpc_and_legacy_fallback() {
    // This test runs on the actual non-test Host binary, not the cfg(test)
    // ResourceSampler. The feature/flag combination only suppresses global
    // integration writes and enables aggregate timing diagnostics.
    let creation = own_creation();
    let request = json!({"appPid":std::process::id(),"appCreationFiletime":creation.to_string(),"intervalMs":2000});
    let count = 4;
    for native in [false, true] {
        let sandbox = tempfile::tempdir().unwrap();
        let root = sandbox.path();
        let token = uuid::Uuid::new_v4().to_string();
        let (mut host, port) = isolated_host(root, &token, native);
        let mut client = Client::new(port, &token);
        let status = client.call("status.get", json!({}));
        assert_eq!(status["protocolVersion"], PROTOCOL_VERSION);
        assert!(status["runtimeCapabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("resourceMonitorV1")));
        assert!(records(root)
            .iter()
            .any(|r| r["msg"] == "isolated host skipped global integration writes"));
        for i in 0..count {
            let mut launch = terminal_host_test_platform::long_running_launch();
            launch["arguments"][3] = json!("Start-Sleep -Seconds 120");
            let reply = client.call(
                "createOrAttach",
                json!({
                    "sessionId":format!("native-resource-{i}"),"workspaceId":"resource-workspace",
                    "tabId":format!("tab-{i}"),"workingDirectory":root.to_str().unwrap(),
                    "launch":launch,"cols":80,"rows":24
                }),
            );
            assert_eq!(reply["running"], true);
        }
        let mut last = 0;
        let mut timings = Vec::new();
        let mut fallback_frames = 0;
        let deadline = Instant::now() + Duration::from_secs(if native { 75 } else { 20 });
        // The collector intentionally falls back during process churn. Test
        // the real recovery contract, not an unrealistic assertion that
        // the very first settled frame always comes from native.
        while timings.len() < 4 {
            if Instant::now() >= deadline {
                for diagnostic in records(root)
                    .iter()
                    .filter(|r| r["msg"] == "resource sampler falling back to sysinfo")
                {
                    println!("HOST_FALLBACK_REASON {}", diagnostic["reason"]);
                }
                panic!("no four consecutive settled frames from the requested backend; fallback_frames={fallback_frames}");
            }
            let snapshot = client.wait_snapshot(&request, |s| {
                s["sessions"]
                    .as_array()
                    .is_some_and(|rows| rows.len() == count)
                    && s["collectedAt"].as_i64().is_some_and(|at| at > last)
            });
            check_snapshot(&snapshot, count);
            assert!(
                snapshot["processes"]["app"]["memoryBytes"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
            let record = sample_record(root, &snapshot);
            assert_eq!(record["sessions"], count.to_string());
            last = snapshot["collectedAt"].as_i64().unwrap();
            let backend = record["backend"].as_str().unwrap();
            if native && backend == "sysinfo" {
                fallback_frames += 1;
                timings.clear();
                continue;
            }
            assert_eq!(backend, if native { "native" } else { "sysinfo" });
            if snapshot["warming"] == true {
                timings.clear();
                continue;
            }
            timings.push(
                record["elapsed_us"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap(),
            );
        }
        for (round, elapsed) in timings.into_iter().enumerate() {
            println!(
                "HOST_RPC_BENCH backend={} sessions={count} round={round} elapsed_us={elapsed}",
                if native { "native" } else { "sysinfo" }
            );
        }
        println!(
            "HOST_RPC_STABLE backend={} fallback_frames={fallback_frames}",
            if native { "native" } else { "sysinfo" }
        );
        if native {
            for diagnostic in records(root)
                .iter()
                .filter(|r| r["msg"] == "resource sampler falling back to sysinfo")
            {
                println!("HOST_FALLBACK_REASON {}", diagnostic["reason"]);
            }
        }
        if native {
            // A forged/stale marker within the same second must not measure
            // the current App as that generation, even on sysinfo fallback.
            let wrong = if creation % 10_000_000 == 9_999_999 {
                creation - 1
            } else {
                creation + 1
            };
            let stale = json!({"appPid":std::process::id(),"appCreationFiletime":wrong.to_string(),"intervalMs":2000});
            let snapshot = client.wait_snapshot(&stale, |s| {
                s["sessions"]
                    .as_array()
                    .is_some_and(|rows| rows.len() == count)
                    && s["collectedAt"].as_i64().is_some_and(|at| at > last)
                    && s["processes"]["app"]["memoryBytes"] == 0
            });
            check_snapshot(&snapshot, count);
            assert_eq!(snapshot["processes"]["app"]["processCount"], 0);
            assert_eq!(sample_record(root, &snapshot)["backend"], "sysinfo");
            last = snapshot["collectedAt"].as_i64().unwrap();
            // A caller that predates the additive marker remains usable and
            // must not inherit the previous stale marker.
            let old = json!({"appPid":std::process::id(),"intervalMs":2000});
            let snapshot = client.wait_snapshot(&old, |s| {
                s["warming"] == false
                    && s["collectedAt"].as_i64().is_some_and(|at| at > last)
                    && s["processes"]["app"]["memoryBytes"]
                        .as_u64()
                        .is_some_and(|bytes| bytes > 0)
            });
            check_snapshot(&snapshot, count);
            assert_eq!(sample_record(root, &snapshot)["backend"], "sysinfo");
            println!("HOST_RPC_FALLBACK stale_marker_rejected=1 old_client_measured=1");
        }
        for i in 0..count {
            client.call(
                "terminate",
                json!({"sessionId":format!("native-resource-{i}")}),
            );
        }
        client.call("host.shutdown", json!({"force":true}));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = host.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "private Host did not shut down");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
