//! Opt-in Windows integration: real, disposable Runtime Host -> authenticated
//! Agent PTY -> MCP stdio -> Native CLI App relay -> Native settings store.
//! Unlike GUI acceptance, this test explicitly does NOT fabricate a visible
//! open_settings success: the headless Native consumer must refuse it.
#![cfg(windows)]

use alera_core::child_process::windowless_command;
use alera_gpui_desktop::mcp_ui_bridge::NativeMcpUiEvent;
use alera_gpui_desktop::native_settings::NativeSettingsStore;
use alera_gpui_desktop::runtime_bridge::RuntimeCliBridge;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct DisposableHost(Child);
impl Drop for DisposableHost {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn request(writer: &mut TcpStream, id: u64, kind: &str, payload: Value) {
    let bytes = serde_json::to_vec(&json!({"id":id,"type":kind,"payload":payload})).unwrap();
    writer.write_all(&bytes).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
}

fn response(reader: &mut BufReader<TcpStream>, id: u64) -> Value {
    for _ in 0..100 {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .expect("Host response timed out");
        assert!(read > 0, "disposable Host disconnected");
        let frame: Value = serde_json::from_str(&line).expect("invalid Host frame");
        if frame["id"] == id {
            return frame;
        }
    }
    panic!("too many unrelated Host events");
}

fn call_cli(cli: &Path, args: &[&str]) {
    let result = windowless_command(cli)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("isolated Alera CLI failed to start");
    assert!(
        result.status.success(),
        "isolated Alera CLI command failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

const AGENT_SCRIPT: &str = r#"
import json, os, pathlib, subprocess, sys
cli, runtime, report, agent, workspace = sys.argv[1:]
assert os.environ.get("ALERA_TERMINAL_HANDLE") == agent
assert os.environ.get("ALERA_WORKSPACE_ID") == workspace
assert len(os.environ.get("ALERA_MCP_UI_TOKEN","")) == 64

def call(name, args, want_ok=True):
    req = {"jsonrpc":"2.0", "id":1, "method":"tools/call",
           "params":{"name":"alera."+name,"arguments":args}}
    child = subprocess.run([cli, "mcp", "--runtime-dir", runtime, "serve"],
                           input=json.dumps(req)+"\n", capture_output=True,
                           text=True, encoding="utf-8", errors="replace",
                           env=os.environ.copy(), timeout=17)
    assert child.returncode == 0, "MCP stdio did not exit cleanly"
    frame = json.loads(child.stdout.strip())
    result = frame["result"]
    assert (result.get("isError") is False) == want_ok, "Unexpected MCP result: "+name
    return result.get("structuredContent") or {}

original = None
cases = []
try:
    value = call("get_setting",{"path":"editor.tabSize"})
    assert value.get("kind")=="setting" and value.get("path")=="editor.tabSize"
    assert isinstance(value.get("value"),int) and not isinstance(value["value"],bool)
    original=value["value"]
    cases.append("actual_agent_host_get")

    newer = 5 if original != 5 else 6
    updated = call("update_setting",{"path":"editor.tabSize",
                                     "expectedValue":original, "value":newer})
    assert updated.get("status")=="applied" and updated.get("value")==newer
    cases.append("actual_agent_host_update")
    readback = call("get_setting",{"path":"editor.tabSize"})
    assert readback.get("value")==newer
    cases.append("actual_agent_host_readback")

    call("update_setting",{"path":"editor.tabSize",
                           "expectedValue":original,"value":newer},False)
    cases.append("stale_expected_denied")
    call("get_setting",{"path":"secrets.apiKey"},False)
    cases.append("secret_denied")
    call("update_setting",{"path":"editor.tabSize",
                           "expectedValue":newer,"value":100},False)
    cases.append("unsafe_range_denied")
    # No real GPUI window is presented by the test consumer; a successful
    # navigation receipt here would be fabricated and must fail closed.
    call("open_settings",{"sectionId":"editor"},False)
    cases.append("headless_navigation_denied")
finally:
    if original is not None:
        now=call("get_setting",{"path":"editor.tabSize"}).get("value")
        if now != original:
            undo=call("update_setting",{"path":"editor.tabSize",
                    "expectedValue":now,"value":original})
            assert undo.get("value")==original
        assert call("get_setting",{"path":"editor.tabSize"}).get("value")==original
pathlib.Path(report).write_text(json.dumps({"passed":len(cases), "cases":cases,
        "restored":True,"mode":"real isolated Host/Agent PTY + Native settings owner; no GUI"}),encoding="utf-8")
print("NATIVE_MCP_SETTINGS_HOST_PASS",flush=True)
"#;

#[test]
#[ignore = "requires ALERA_GPUI_TEST_CLI pointing at this branch's disposable Windows alera.exe"]
fn real_agent_mcp_settings_roundtrip_through_native_app_bridge() {
    let cli = std::env::var_os("ALERA_GPUI_TEST_CLI")
        .map(std::path::PathBuf::from)
        .expect("set ALERA_GPUI_TEST_CLI");
    assert!(cli.is_absolute() && cli.is_file());
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let runtime = root.join("runtime");
    let home = root.join("home");
    let state = root.join("native-state");
    let control = runtime.join("runtime-host.json");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let token = format!(
        "native-settings-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut host = DisposableHost(
        windowless_command(&cli)
            .arg("runtime-host")
            .arg("--runtime-dir")
            .arg(&runtime)
            .arg("--control-file")
            .arg(&control)
            .arg("--token")
            .arg(&token)
            .args([
                "--empty-shutdown-delay-seconds",
                "60",
                "--detached-session-shutdown-delay-seconds",
                "60",
            ])
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("APPDATA", home.join("AppData").join("Roaming"))
            .env("LOCALAPPDATA", home.join("AppData").join("Local"))
            .env("ALERA_TERMINAL_WORKER_PROCESS", "0")
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    let port = loop {
        if let Ok(contents) = std::fs::read_to_string(&control) {
            if let Ok(value) = serde_json::from_str::<Value>(&contents) {
                if value["pid"].as_u64() == Some(u64::from(host.0.id())) {
                    break value["port"].as_u64().unwrap() as u16;
                }
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(25),
            "Host never started"
        );
        assert!(
            host.0.try_wait().unwrap().is_none(),
            "Host process exited early"
        );
        std::thread::sleep(Duration::from_millis(50));
    };

    let runtime_arg = runtime.to_str().unwrap();
    let home_arg = home.to_str().unwrap();
    let project_id = "native-settings-isolated-project";
    let workspace_id = "native-settings-isolated-workspace";
    call_cli(
        &cli,
        &[
            "project",
            "--runtime-dir",
            runtime_arg,
            "--json",
            "add",
            "--id",
            project_id,
            "--name",
            "Native settings test",
            "--repo-path",
            home_arg,
            "--kind",
            "folder",
        ],
    );
    call_cli(
        &cli,
        &[
            "workspace",
            "--runtime-dir",
            runtime_arg,
            "--json",
            "register",
            "--id",
            workspace_id,
            "--project-id",
            project_id,
            "--name",
            "Native settings workspace",
            "--path",
            home_arg,
            "--kind",
            "main",
        ],
    );

    let bridge = RuntimeCliBridge::new(&cli, &runtime);
    let mut native = bridge
        .start_native_mcp_ui(format!("native-settings-test-{}", std::process::id()))
        .expect("Native App bridge registration");
    let ready_at = Instant::now();
    loop {
        let events = native.take_events();
        if events.contains(&NativeMcpUiEvent::Ready) {
            break;
        }
        assert!(
            !events.contains(&NativeMcpUiEvent::Disconnected)
                && ready_at.elapsed() < Duration::from_secs(10),
            "Native window could not register with isolated real Host"
        );
        std::thread::sleep(Duration::from_millis(40));
    }

    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    request(
        &mut writer,
        1,
        "hello",
        json!({"protocolVersion":4,"token":token}),
    );
    assert_eq!(response(&mut reader, 1)["ok"], true);
    let session_id = "native-settings-agent-pty";
    request(
        &mut writer,
        2,
        "tab.upsert",
        json!({
            "id":"native-settings-agent-tab","workspaceId":workspace_id,
            "kind":"terminal","title":"Native MCP isolated Agent",
            "createdAt":"2026-11-08T00:00:00.000Z",
            "updatedAt":"2026-11-08T00:00:00.000Z",
            "payload":{"terminalSessionId":session_id,"agentType":"codex","spawnOnCreate":true}
        }),
    );
    let spawned = response(&mut reader, 2);
    assert_eq!(
        spawned["ok"], true,
        "Host denied Agent session registration"
    );
    let began = Instant::now();
    loop {
        let sessions = bridge.list_terminals().unwrap();
        if sessions.iter().any(|s| s.handle == session_id && s.running) {
            break;
        }
        assert!(
            began.elapsed() < Duration::from_secs(25),
            "Host never spawned Agent PTY"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let script = home.join("agent_native_settings_probe.py");
    let report = home.join("native_mcp_settings_result.json");
    std::fs::write(&script, AGENT_SCRIPT).unwrap();
    let command = format!(
        "python \"{}\" \"{}\" \"{}\" \"{}\" \"{}\"",
        script.display(),
        cli.display(),
        runtime.display(),
        report.display(),
        session_id
    );
    // Last argument is the verified Host Workspace identity.
    let command = format!("{command} \"{workspace_id}\"");
    bridge.write_terminal(session_id, &command, true).unwrap();

    let store = NativeSettingsStore::new(&state);
    let deadline = Instant::now() + Duration::from_secs(110);
    let mut accepted = 0usize;
    let mut completed = 0usize;
    let mut cursor = None;
    let mut last_read = Instant::now();
    loop {
        for event in native.take_events() {
            match event {
                NativeMcpUiEvent::Command(command) => {
                    accepted += 1;
                    assert_eq!(command.workspace_id, workspace_id);
                    match command.action.as_str() {
                        "get_setting" => {
                            let path = command.arguments["path"].as_str().unwrap();
                            match store.get(path) {
                                Ok(value) => native
                                    .complete_setting(&command.request_id, &value)
                                    .unwrap(),
                                Err(_) => native
                                    .complete(
                                        &command.request_id,
                                        false,
                                        None,
                                        Some("ui_operation_failed"),
                                    )
                                    .unwrap(),
                            }
                        }
                        "update_setting" => {
                            let args = &command.arguments;
                            let path = args["path"].as_str().unwrap();
                            match store.compare_and_set(
                                path,
                                &args["expectedValue"],
                                &args["value"],
                            ) {
                                Ok(saved) => {
                                    let readback = saved.get(path).unwrap();
                                    native
                                        .complete_setting(&command.request_id, &readback)
                                        .unwrap();
                                }
                                Err(_) => native
                                    .complete(
                                        &command.request_id,
                                        false,
                                        None,
                                        Some("ui_operation_failed"),
                                    )
                                    .unwrap(),
                            }
                        }
                        "open_settings" => {
                            // This test has NO GUI. A setting navigation ACK
                            // cannot be asserted by a headless substitute.
                            native
                                .complete(&command.request_id, false, None, Some("ui_not_ready"))
                                .unwrap();
                        }
                        other => panic!("Unexpected Host-validated action: {other}"),
                    }
                    completed += 1;
                }
                NativeMcpUiEvent::Disconnected => panic!("Native App disconnected"),
                NativeMcpUiEvent::Ready => {}
            }
        }
        if report.exists() {
            break;
        }
        if last_read.elapsed() >= Duration::from_millis(750) {
            let chunk = bridge.read_terminal(session_id, cursor).unwrap();
            cursor = Some(chunk.next_cursor);
            if chunk.text.contains("Traceback (most recent call last)")
                || chunk.text.contains("is not recognized as an internal")
            {
                panic!("Disposable Agent PTY failed before completing MCP settings probe");
            }
            last_read = Instant::now();
        }
        assert!(
            Instant::now() < deadline,
            "disposable Agent MCP settings probe timed out"
        );
        std::thread::sleep(Duration::from_millis(35));
    }
    let evidence: Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    assert_eq!(evidence["passed"], 7);
    assert_eq!(evidence["restored"], true);
    assert!(
        accepted >= 8 && completed == accepted,
        "Expected Host-driven settings requests and failed navigation receipt"
    );
    assert_eq!(store.get("editor.tabSize").unwrap(), json!(4));
    assert!(matches!(
        bridge.probe().unwrap(),
        alera_gpui_desktop::runtime_bridge::RuntimeStatus::Running { .. }
    ));
    drop(native);
    assert!(bridge
        .list_terminals()
        .unwrap()
        .iter()
        .any(|s| s.handle == session_id));
    host.0.kill().unwrap();
    host.0.wait().unwrap();
}
