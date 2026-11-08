//! Opt-in Windows end-to-end test for the *actual* Alera CLI / Runtime Host.
//! Run only with ALERA_GPUI_TEST_CLI=<absolute-path-to-this-branch-alera.exe>
//! and -- --ignored. Never launches or reads the user's production runtime.
#![cfg(windows)]

use alera_core::child_process::windowless_command;
use alera_gpui_desktop::mcp_ui_bridge::NativeMcpUiEvent;
use alera_gpui_desktop::runtime_bridge::{RuntimeCliBridge, RuntimeStatus};
use alera_gpui_desktop::terminal::{TerminalCreateFlow, TerminalDock};
use alera_gpui_desktop::terminal_panes::TerminalPaneLayout;
use alera_gpui_desktop::vt_screen::VtScreen;
use alera_gpui_desktop::workbench_panes::{PaneSlot, WorkbenchPaneLayout};
use alera_gpui_desktop::workbench_tabs::WorkbenchTabIdentity;
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

    // A separately named Native window registers to the *same* isolated
    // Host as an App, receives the CLI's ready event, and disconnects without
    // terminating Classic's/Host's sessions or claiming a PTY.
    let mut mcp_ui = bridge
        .start_native_mcp_ui(format!("native-host-smoke-{}", std::process::id()))
        .expect("Native MCP bridge should start against existing isolated Host");
    let mcp_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let events = mcp_ui.take_events();
        if events.contains(&NativeMcpUiEvent::Ready) {
            break;
        }
        assert!(
            !events.contains(&NativeMcpUiEvent::Disconnected) && Instant::now() < mcp_deadline,
            "Native MCP bridge did not register with the isolated real Host"
        );
        std::thread::sleep(Duration::from_millis(40));
    }
    drop(mcp_ui);
    assert!(matches!(
        bridge.probe().unwrap(),
        RuntimeStatus::Running { .. }
    ));

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

    // Exercise the *real* Host creation contract via GPUI's strict CLI
    // adapter, not a synthetic reply. In particular, this Host omits
    // attachment.workspaceId/tabId and instead reports the session identity
    // in tab.payload.terminalSessionId. A previously passing unit fixture
    // failed in the native GUI because it incorrectly required those fields.
    let project = windowless_command(&cli)
        .args(["project", "--runtime-dir"])
        .arg(&runtime)
        .args([
            "--json",
            "add",
            "--id",
            "gpui-registered-test-project",
            "--name",
            "GPUI Host test",
            "--repo-path",
        ])
        .arg(&home)
        .args(["--kind", "folder"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        project.status.success(),
        "isolated project registration failed: {}",
        String::from_utf8_lossy(&project.stderr)
    );
    let workspace = windowless_command(&cli)
        .args(["workspace", "--runtime-dir"])
        .arg(&runtime)
        .args([
            "--json",
            "register",
            "--id",
            "gpui-registered-test-workspace",
            "--project-id",
            "gpui-registered-test-project",
            "--name",
            "GPUI Host test Workspace",
            "--path",
        ])
        .arg(&home)
        .args(["--kind", "main"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        workspace.status.success(),
        "isolated workspace registration failed: {}",
        String::from_utf8_lossy(&workspace.stderr)
    );
    let registered = bridge.list_registered_workspaces().unwrap();
    assert!(registered
        .iter()
        .any(|ws| ws.id == "gpui-registered-test-workspace" && ws.path == home));

    let native_created = bridge
        .create_terminal("gpui-registered-test-workspace")
        .expect("GPUI CLI adapter must parse the Host's real terminal.create response");
    let sessions = bridge.list_terminals().unwrap();
    assert!(
        sessions
            .iter()
            .any(|session| native_created.matches_live_session(session)),
        "created Host session must match the authenticated list before attachment"
    );
    let mut create_flow = TerminalCreateFlow::default();
    let request = create_flow.start_request().unwrap();
    assert!(create_flow.finish_request(request, native_created.clone()));
    assert_eq!(
        create_flow
            .confirm_sessions(&sessions)
            .expect("Host identities must pass GPUI's create-confirmation gate"),
        native_created
    );
    let new_marker = format!("GPUI_BRIDGE_EXEC_{}", std::process::id());
    // A real filesystem side-effect in the temporary workspace proves
    // command execution; echoed keystrokes alone would be misleading.
    let command_output = home.join("gpui-bridge-exec.txt");
    bridge
        .write_terminal(
            &native_created.handle,
            &format!("echo {new_marker} > gpui-bridge-exec.txt"),
            true,
        )
        .unwrap();
    bridge
        .resize_terminal(&native_created.handle, 120, 36)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut created_cursor = None;
    let mut created_transcript = String::new();
    loop {
        let chunk = bridge
            .read_terminal(&native_created.handle, created_cursor)
            .unwrap();
        assert!(created_cursor.is_none_or(|value| chunk.next_cursor >= value));
        created_cursor = Some(chunk.next_cursor);
        created_transcript.push_str(&chunk.text);
        if created_transcript.contains(&new_marker) && command_output.is_file() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "created Host PTY never echoed test command: {created_transcript:?}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(
        std::fs::read_to_string(&command_output).unwrap().trim(),
        new_marker,
        "temporary workspace command output did not match the Host input"
    );

    // Two independently Host-owned PTYs must be enough for a split local
    // preview without changing, duplicating or terminating either session.
    let second_created = bridge
        .create_terminal("gpui-registered-test-workspace")
        .unwrap();
    let sessions = bridge.list_terminals().unwrap();
    assert!(sessions
        .iter()
        .any(|session| second_created.matches_live_session(session)));
    assert_ne!(native_created.handle, second_created.handle);

    // A split view must not just list two Host tickets: prove that each
    // authenticated PTY independently accepts commands and returns its OWN
    // output. The filesystem effects verify actual execution, while the
    // separate output cursors detect a wrong-pane read or keyboard target.
    let first_marker = format!("GPUI_PANE_ONE_{}", std::process::id());
    let second_marker = format!("GPUI_PANE_TWO_{}", std::process::id());
    let first_file = home.join("gpui-pane-one.txt");
    let second_file = home.join("gpui-pane-two.txt");
    bridge
        .write_terminal(
            &native_created.handle,
            &format!("echo {first_marker} > gpui-pane-one.txt"),
            true,
        )
        .unwrap();
    bridge
        .write_terminal(
            &second_created.handle,
            &format!("echo {second_marker} > gpui-pane-two.txt"),
            true,
        )
        .unwrap();
    let mut first_cursor = created_cursor;
    let mut second_cursor = None;
    let mut first_transcript = String::new();
    let mut second_transcript = String::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let first = bridge
            .read_terminal(&native_created.handle, first_cursor)
            .unwrap();
        assert!(first_cursor.is_none_or(|old| first.next_cursor >= old));
        first_cursor = Some(first.next_cursor);
        first_transcript.push_str(&first.text);
        let second = bridge
            .read_terminal(&second_created.handle, second_cursor)
            .unwrap();
        assert!(second_cursor.is_none_or(|old| second.next_cursor >= old));
        second_cursor = Some(second.next_cursor);
        second_transcript.push_str(&second.text);
        if first_transcript.contains(&first_marker)
            && second_transcript.contains(&second_marker)
            && first_file.is_file()
            && second_file.is_file()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "two independent Host PTYs did not complete: first={first_transcript:?} second={second_transcript:?}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        !first_transcript.contains(&second_marker),
        "second pane leaked into first PTY"
    );
    assert!(
        !second_transcript.contains(&first_marker),
        "first pane leaked into second PTY"
    );
    assert_eq!(
        std::fs::read_to_string(first_file).unwrap().trim(),
        first_marker
    );
    assert_eq!(
        std::fs::read_to_string(second_file).unwrap().trim(),
        second_marker
    );

    let mut dock = TerminalDock::default();
    dock.update_sessions(sessions);
    assert!(dock.open(&native_created.handle));
    assert!(dock.open(&second_created.handle));
    assert!(dock.open(&native_created.handle));
    let mut panes = TerminalPaneLayout::default();
    assert!(panes.split(&dock));
    assert_eq!(
        panes.secondary.as_ref().map(|item| item.handle.as_str()),
        Some(second_created.handle.as_str())
    );
    assert!(panes.switch_focus(&mut dock));
    assert_eq!(dock.active().unwrap().handle, second_created.handle);
    panes.close_secondary();
    assert!(panes.secondary.is_none());
    // Feed real authenticated Host tickets through the *new* mixed
    // WorkbenchPaneLayout, not just the legacy TerminalPaneLayout. Each
    // pane must keep its own exact Host identity while focus swaps.
    let first_target = dock.context_target_at(0).unwrap();
    let second_target = dock.context_target_at(1).unwrap();
    assert_eq!(first_target.handle, native_created.handle);
    assert_eq!(second_target.handle, second_created.handle);
    assert!(dock.context_target_is_live(&first_target));
    assert!(dock.context_target_is_live(&second_target));
    let first_id = WorkbenchTabIdentity::Host(first_target.clone());
    let second_id = WorkbenchTabIdentity::Host(second_target.clone());
    let mut mixed_panes = WorkbenchPaneLayout::new();
    mixed_panes.reconcile(&[first_id.clone(), second_id.clone()]);
    assert!(mixed_panes.split_with(&second_id));
    assert_eq!(mixed_panes.active_in(PaneSlot::Primary), Some(&first_id));
    assert_eq!(mixed_panes.active_in(PaneSlot::Secondary), Some(&second_id));
    assert_eq!(mixed_panes.background_host_target(), Some(&first_target));
    assert_eq!(
        mixed_panes.focused_host_target_for_input(PaneSlot::Secondary),
        Some(&second_target)
    );
    assert_eq!(
        mixed_panes.focused_host_target_for_input(PaneSlot::Primary),
        None
    );
    assert!(mixed_panes.focus_pane(PaneSlot::Primary));
    assert_eq!(mixed_panes.background_host_target(), Some(&second_target));
    assert_eq!(
        mixed_panes.focused_host_target_for_input(PaneSlot::Primary),
        Some(&first_target)
    );
    assert_eq!(
        mixed_panes.focused_host_target_for_input(PaneSlot::Secondary),
        None
    );
    mixed_panes.close_secondary();
    assert!(!mixed_panes.has_secondary());
    assert!(mixed_panes.tabs_in(PaneSlot::Primary).contains(&first_id));
    assert!(mixed_panes.tabs_in(PaneSlot::Primary).contains(&second_id));
    let running_after_preview_close = bridge.list_terminals().unwrap();
    assert!(running_after_preview_close
        .iter()
        .any(|s| native_created.matches_live_session(s)));
    assert!(running_after_preview_close
        .iter()
        .any(|s| second_created.matches_live_session(s)));

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
