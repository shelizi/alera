//! Opt-in Windows end-to-end test for the *actual* Alera CLI / Runtime Host.
//! Run only with ALERA_GPUI_TEST_CLI=<absolute-path-to-this-branch-alera.exe>
//! and -- --ignored. Never launches or reads the user's production runtime.
#![cfg(windows)]

use alera_core::child_process::windowless_command;
use alera_gpui_desktop::runtime_bridge::{RuntimeCliBridge, RuntimeStatus};
use alera_gpui_desktop::vt_screen::VtScreen;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct TestHost(Child);

impl Drop for TestHost {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn send(stream: &mut TcpStream, value: Value) {
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.push(b'\n');
    stream.write_all(&bytes).unwrap();
    stream.flush().unwrap();
}

fn response(reader: &mut BufReader<TcpStream>, request_id: u64) -> Value {
    for _ in 0..200 {
        let mut line = String::new();
        let n = reader.read_line(&mut line).expect("Host reply timed out");
        assert!(n > 0, "isolated Host closed its socket");
        let frame: Value = serde_json::from_str(&line).unwrap();
        if frame.get("id").and_then(Value::as_u64) == Some(request_id) {
            return frame;
        }
    }
    panic!("Host produced too many unrelated events");
}

#[test]
#[ignore = "requires ALERA_GPUI_TEST_CLI pointing at feature-branch alera.exe"]
fn real_host_terminal_list_and_incremental_read_never_autostart() {
    let cli = std::env::var_os("ALERA_GPUI_TEST_CLI")
        .map(std::path::PathBuf::from)
        .expect("set ALERA_GPUI_TEST_CLI to the absolute experimental alera.exe");
    assert!(cli.is_absolute() && cli.is_file());
    let temp = tempfile::tempdir().unwrap();
    let runtime = temp.path().join("runtime");
    let home = temp.path().join("home");
    let control_path = runtime.join("runtime-host.json");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::create_dir_all(&home).unwrap();

    // This CLI flag must fail closed when no Host is running. Assert no
    // control file appeared; do not call ordinary terminal list without it.
    let preflight = windowless_command(&cli)
        .args(["terminal", "--no-start", "--json", "--runtime-dir"])
        .arg(&runtime)
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!preflight.status.success());
    assert!(!control_path.exists());

    let token = format!(
        "gpui-test-token-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut host = TestHost(
        windowless_command(&cli)
            .arg("runtime-host")
            .arg("--runtime-dir")
            .arg(&runtime)
            .arg("--control-file")
            .arg(&control_path)
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

    let deadline = Instant::now() + Duration::from_secs(25);
    let port = loop {
        if let Ok(data) = std::fs::read_to_string(&control_path) {
            if let Ok(value) = serde_json::from_str::<Value>(&data) {
                if value["pid"].as_u64() == Some(u64::from(host.0.id())) {
                    break value["port"].as_u64().unwrap() as u16;
                }
            }
        }
        assert!(
            host.0.try_wait().unwrap().is_none(),
            "test Host exited unexpectedly"
        );
        assert!(
            Instant::now() < deadline,
            "test Host never published control file"
        );
        std::thread::sleep(Duration::from_millis(50));
    };

    let bridge = RuntimeCliBridge::new(&cli, &runtime);
    assert!(matches!(
        bridge.probe().unwrap(),
        RuntimeStatus::Running { .. }
    ));
    assert!(bridge.list_terminals().unwrap().is_empty());

    // Use authenticated loopback solely to construct this test fixture. The
    // GPUI application under test still uses only the normal CLI adapter.
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    send(
        &mut writer,
        json!({"id":1,"type":"hello","payload":{"protocolVersion":4,"token":token}}),
    );
    let hello = response(&mut reader, 1);
    assert_eq!(hello["ok"], true, "Host handshake failed");

    let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
    let marker = format!("GPUI_HOST_SMOKE_{}", std::process::id());
    let command = format!("echo {marker}& ping -n 3 127.0.0.1 >NUL& echo GPUI_DONE");
    let environment = json!({
        "TERM": "xterm",
        "PATH": std::env::var("PATH").unwrap_or_default(),
        "SystemRoot": std::env::var("SystemRoot").unwrap_or_default(),
        "WINDIR": std::env::var("WINDIR").unwrap_or_default(),
        "TEMP": std::env::var("TEMP").unwrap_or_default()
    });
    let session_id = "gpui-test-session";
    send(
        &mut writer,
        json!({
            "id":2,
            "type":"createOrAttach",
            "payload":{
                "sessionId": session_id,
                "workspaceId": "gpui-test-workspace",
                "tabId": "gpui-test-tab",
                "workingDirectory": home.to_string_lossy(),
                "launch":{"shell":shell,"arguments":["/d","/s","/c",command],"environment":environment},
                "cols":80,
                "rows":24
            }
        }),
    );
    let created = response(&mut reader, 2);
    assert_eq!(
        created["ok"], true,
        "test Session creation failed: {created}"
    );

    let sessions = bridge.list_terminals().unwrap();
    assert!(sessions.iter().any(|s| s.handle == session_id));

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut cursor = None;
    let mut transcript = String::new();
    loop {
        let chunk = bridge.read_terminal(session_id, cursor).unwrap();
        assert!(cursor.is_none_or(|value| chunk.next_cursor >= value));
        transcript.push_str(&chunk.text);
        cursor = Some(chunk.next_cursor);
        if transcript.contains(&marker) && transcript.contains("GPUI_DONE") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "live CLI output incomplete: {transcript:?}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    let empty = bridge.read_terminal(session_id, cursor).unwrap();
    assert_eq!(empty.next_cursor, cursor.unwrap());
    assert!(empty.text.is_empty(), "cursor replayed old bytes");
    let mut screen = VtScreen::default();
    screen.feed(&transcript);
    assert!(
        screen.contents().contains(&marker),
        "VT screen did not render marker from actual Host PTY output"
    );
    let styled = screen.styled_rows();
    assert!(
        styled.iter().any(|row| row.text.contains(&marker)),
        "Styled GPUI VT rows lost actual Host PTY output"
    );
    assert!(styled.iter().all(|row| {
        row.spans.iter().all(|span| {
            row.text.is_char_boundary(span.range.start) && row.text.is_char_boundary(span.range.end)
        })
    }));

    // Optional native GUI process smoke against the SAME isolated Host.
    // It observes the existing test session through its window-owned
    // polling loop. No direct test token is passed to GPUI.
    if let Some(app_path) = std::env::var_os("ALERA_GPUI_TEST_APP") {
        let app_path = std::path::PathBuf::from(app_path);
        assert!(app_path.is_absolute() && app_path.is_file());
        let app = windowless_command(&app_path)
            .arg(&home)
            .env("ALERA_CLI_PATH", &cli)
            .env("ALERA_RUNTIME_DIR", &runtime)
            .env("ALERA_GPUI_STATE_DIR", temp.path().join("gpui-state"))
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("APPDATA", home.join("AppData").join("Roaming"))
            .env("LOCALAPPDATA", home.join("AppData").join("Local"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("unable to launch native GPUI process");
        let mut app = TestHost(app);
        std::thread::sleep(Duration::from_secs(5));
        assert!(
            app.0.try_wait().unwrap().is_none(),
            "GPUI exited during live-host smoke"
        );
        assert!(matches!(
            bridge.probe().unwrap(),
            RuntimeStatus::Running { .. }
        ));
        assert!(bridge
            .list_terminals()
            .unwrap()
            .iter()
            .any(|s| s.handle == session_id));
    }

    // After the Host is gone, --no-start refuses to create a replacement.
    host.0.kill().unwrap();
    host.0.wait().unwrap();
    // Runtime control data can remain briefly after abrupt exit.
    // The CLI must still reject the dead process and never spawn a new one.
    let post = windowless_command(&cli)
        .args(["terminal", "--no-start", "--json", "--runtime-dir"])
        .arg(&runtime)
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!post.status.success());
}
