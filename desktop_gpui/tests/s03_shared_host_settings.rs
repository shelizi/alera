//! S03: two independent client paths to a disposable real Alera Runtime Host.
//! One acts as Classic through standalone CLI, one as GPUI via RuntimeCliBridge.
//! Opt-in only: do not start or write the user's normal Runtime Host.
#![cfg(windows)]

use alera_core::child_process::windowless_command;
use alera_gpui_desktop::runtime_bridge::RuntimeCliBridge;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct IsolatedHost {
    child: Child,
    control: PathBuf,
    port: u16,
    token: String,
}

impl IsolatedHost {
    fn start(cli: &Path, runtime: &Path, home: &Path) -> Self {
        let control = runtime.join("runtime-host.json");
        let token = format!(
            "s03-isolated-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let child = windowless_command(cli)
            .arg("runtime-host")
            .arg("--runtime-dir")
            .arg(runtime)
            .arg("--control-file")
            .arg(&control)
            .arg("--token")
            .arg(&token)
            .args([
                "--empty-shutdown-delay-seconds",
                "120",
                "--detached-session-shutdown-delay-seconds",
                "120",
            ])
            .env("HOME", home)
            .env("USERPROFILE", home)
            .env("APPDATA", home.join("AppData").join("Roaming"))
            .env("LOCALAPPDATA", home.join("AppData").join("Local"))
            .env("ALERA_TERMINAL_WORKER_PROCESS", "0")
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start disposable real Runtime Host");
        let mut host = Self {
            child,
            control,
            port: 0,
            token,
        };
        let start = Instant::now();
        loop {
            if let Ok(raw) = std::fs::read_to_string(&host.control) {
                if let Ok(record) = serde_json::from_str::<Value>(&raw) {
                    if record["pid"].as_u64() == Some(host.child.id() as u64) {
                        host.port = record["port"].as_u64().unwrap() as u16;
                        assert_ne!(host.port, 0);
                        break;
                    }
                }
            }
            assert!(
                start.elapsed() < Duration::from_secs(25)
                    && host.child.try_wait().unwrap().is_none(),
                "disposable S03 Runtime Host never became ready"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let reader = RuntimeCliBridge::new(cli, runtime);
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if reader.desktop_settings_snapshot().is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "S03 settings Host did not become readable"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        host
    }

    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.control);
    }
}

impl Drop for IsolatedHost {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn classic_cli(
    cli: &Path,
    runtime: &Path,
    path: Option<&str>,
    expected: Value,
    next: Value,
) -> Result<Value, String> {
    let mut cmd = windowless_command(cli);
    cmd.args(["settings", "--no-start", "--json", "--runtime-dir"])
        .arg(runtime);
    match path {
        None => {
            cmd.arg("get");
        }
        Some(path) => {
            cmd.arg("set")
                .arg("--path")
                .arg(path)
                .arg("--expected")
                .arg(expected.to_string())
                .arg("--value")
                .arg(next.to_string());
        }
    }
    let reply = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Classic CLI failed to start: {error}"))?;
    if !reply.status.success() {
        return Err(format!(
            "Classic Host rejected operation ({}): {}",
            reply.status,
            String::from_utf8_lossy(&reply.stderr)
        ));
    }
    serde_json::from_slice(&reply.stdout)
        .map_err(|error| format!("Classic settings response is not JSON: {error}"))
}

/// Classic Flutter uses authenticated desktopSettings.get and
/// desktopSettings.compareAndSetBatch RPCs, not the CLI's single-path Set.
/// Exercise that exact protocol against the separate GPUI CLI bridge.
struct ClassicRpc {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    next_id: u64,
    events: Vec<String>,
}

impl ClassicRpc {
    fn connect(host: &IsolatedHost) -> Self {
        let writer = TcpStream::connect(("127.0.0.1", host.port)).unwrap();
        writer
            .set_read_timeout(Some(Duration::from_secs(12)))
            .unwrap();
        let reader = BufReader::new(writer.try_clone().unwrap());
        let mut client = Self {
            writer,
            reader,
            next_id: 1,
            events: Vec::new(),
        };
        let hello = client.call("hello", json!({"protocolVersion":4,"token":host.token}));
        assert_eq!(
            hello["ok"], true,
            "isolated Classic RPC authentication failed"
        );
        client
    }

    fn call(&mut self, method: &str, payload: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let mut encoded = serde_json::to_vec(&json!({
            "id": id,
            "type": method,
            "payload": payload
        }))
        .unwrap();
        encoded.push(b'\n');
        self.writer.write_all(&encoded).unwrap();
        self.writer.flush().unwrap();
        for _ in 0..128 {
            let mut line = String::new();
            let count = self
                .reader
                .read_line(&mut line)
                .expect("Classic Host response timeout");
            assert!(count > 0, "disposable Classic Host disconnected");
            let frame: Value = serde_json::from_str(&line).unwrap();
            if frame["id"].as_u64() == Some(id) {
                return frame;
            }
            if let Some(name) = frame["event"].as_str() {
                self.events.push(name.to_owned());
            }
        }
        panic!("too many unrelated Host events while reading Classic RPC reply");
    }

    fn get(&mut self) -> Value {
        let response = self.call("desktopSettings.get", json!({}));
        assert_eq!(response["ok"], true, "Classic Host settings Get failed");
        response["payload"].clone()
    }

    fn batch(&mut self, changes: Value) -> Value {
        self.call(
            "desktopSettings.compareAndSetBatch",
            json!({"changes":changes}),
        )
    }
}

fn classic_get(cli: &Path, runtime: &Path) -> Value {
    classic_cli(cli, runtime, None, Value::Null, Value::Null).unwrap()
}

fn classic_set(cli: &Path, runtime: &Path, path: &str, before: Value, after: Value) -> Value {
    classic_cli(cli, runtime, Some(path), before, after).unwrap()
}

#[test]
#[ignore = "opt in with ALERA_GPUI_TEST_CLI pointing at the experimental Windows alera.exe"]
fn s03_classic_cli_and_gpui_bridge_share_cas_and_persist_after_host_restart() {
    let cli = PathBuf::from(
        std::env::var_os("ALERA_GPUI_TEST_CLI")
            .expect("Set ALERA_GPUI_TEST_CLI to the absolute tested Windows CLI"),
    );
    assert!(cli.is_absolute() && cli.is_file());
    let temp = tempfile::tempdir().unwrap();
    let runtime = temp.path().join("runtime");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let native = RuntimeCliBridge::new(&cli, &runtime);

    // Neither client may fall back to the user's real Runtime on startup.
    assert!(native.desktop_settings_snapshot().is_err());
    assert!(classic_cli(&cli, &runtime, None, Value::Null, Value::Null).is_err());
    assert!(!runtime.join("runtime-host.json").exists());

    let host = IsolatedHost::start(&cli, &runtime, &home);
    let classic_before = classic_get(&cli, &runtime);
    let native_before = native.desktop_settings_snapshot().unwrap();
    assert_eq!(classic_before["values"], native_before["values"]);

    // Classic writes a portable setting, GPUI independently reads it, then
    // GPUI writes it back. A cached Classic/GPUI expected value must fail CAS.
    assert_eq!(native_before["values"]["editor.tabSize"], json!(4));
    classic_set(&cli, &runtime, "editor.tabSize", json!(4), json!(6));
    assert_eq!(
        native.desktop_settings_snapshot().unwrap()["values"]["editor.tabSize"],
        6
    );
    assert!(native
        .desktop_settings_compare_and_set("editor.tabSize", &json!(4), &json!(8))
        .is_err());
    native
        .desktop_settings_compare_and_set("editor.tabSize", &json!(6), &json!(8))
        .unwrap();
    assert_eq!(classic_get(&cli, &runtime)["values"]["editor.tabSize"], 8);

    // The UI-local field migrated to Host must also share one authoritative
    // value across the two binaries, not the former Native JSON preference.
    assert_eq!(native_before["values"]["editor.twoRowTabs"], false);
    classic_set(
        &cli,
        &runtime,
        "editor.twoRowTabs",
        json!(false),
        json!(true),
    );
    assert_eq!(
        native.desktop_settings_snapshot().unwrap()["values"]["editor.twoRowTabs"],
        true
    );
    assert!(native
        .desktop_settings_compare_and_set("editor.twoRowTabs", &json!(false), &json!(false))
        .is_err());
    native
        .desktop_settings_compare_and_set("editor.twoRowTabs", &json!(true), &json!(false))
        .unwrap();
    assert_eq!(
        classic_get(&cli, &runtime)["values"]["editor.twoRowTabs"],
        false
    );

    // Local language and portable terminal selection are likewise visible
    // bidirectionally, without overwriting unrelated settings.
    native
        .desktop_settings_compare_and_set(
            "general.language",
            &json!("system"),
            &json!("traditionalChinese"),
        )
        .unwrap();
    assert_eq!(
        classic_get(&cli, &runtime)["values"]["general.language"],
        "traditionalChinese"
    );
    classic_set(
        &cli,
        &runtime,
        "general.language",
        json!("traditionalChinese"),
        json!("english"),
    );
    assert_eq!(
        native.desktop_settings_snapshot().unwrap()["values"]["general.language"],
        "english"
    );
    classic_set(
        &cli,
        &runtime,
        "terminal.cursorShape",
        json!("block"),
        json!("bar"),
    );
    assert_eq!(
        native.desktop_settings_snapshot().unwrap()["values"]["terminal.cursorShape"],
        "bar"
    );

    // Agent-executable CLI cannot switch off destructive confirmation flags.
    assert!(classic_cli(
        &cli,
        &runtime,
        Some("general.confirmWorkspaceRemoval"),
        json!(true),
        json!(false)
    )
    .is_err());
    assert_eq!(
        native.desktop_settings_snapshot().unwrap()["values"]["general.confirmWorkspaceRemoval"],
        true
    );

    // Exact Classic Flutter path: authenticated Host batch RPC, including
    // the protected confirmation flag that is intentionally *not* CLI-writable.
    // Native must observe the whole successful transaction together.
    let mut classic = ClassicRpc::connect(&host);
    assert_eq!(
        classic.get()["values"],
        native.desktop_settings_snapshot().unwrap()["values"]
    );
    let batch = classic.batch(json!([
        {"path":"editor.tabSize","expectedValue":8,"value":7},
        {"path":"general.autoGitFetchIntervalMinutes","expectedValue":5,"value":9},
        {"path":"general.confirmWorkspaceRemoval","expectedValue":true,"value":false}
    ]));
    assert_eq!(
        batch["ok"], true,
        "Classic 3-field Host transaction failed: {batch}"
    );
    let native_after_classic_batch = native.desktop_settings_snapshot().unwrap();
    assert_eq!(native_after_classic_batch["values"]["editor.tabSize"], 7);
    assert_eq!(
        native_after_classic_batch["values"]["general.autoGitFetchIntervalMinutes"],
        9
    );
    assert_eq!(
        native_after_classic_batch["values"]["general.confirmWorkspaceRemoval"],
        false
    );

    // A stale Classic batch cannot partially overwrite another client's
    // value, even when one of the remaining fields has a valid expected value.
    let stale = classic.batch(json!([
        {"path":"editor.tabSize","expectedValue":8,"value":11},
        {"path":"general.language","expectedValue":"english","value":"traditionalChinese"}
    ]));
    assert_eq!(stale["ok"], false);
    assert_eq!(stale["errorCode"], "desktop_settings_revision_conflict");
    assert_eq!(
        native.desktop_settings_snapshot().unwrap()["values"]["editor.tabSize"],
        7
    );
    assert_eq!(
        native.desktop_settings_snapshot().unwrap()["values"]["general.language"],
        "english"
    );

    // GPUI's separate CLI CAS must broadcast the exact change event that
    // Classic's SettingsController listens to before it reloads.
    native
        .desktop_settings_compare_and_set("editor.tabSize", &json!(7), &json!(10))
        .unwrap();
    assert_eq!(classic.get()["values"]["editor.tabSize"], 10);
    assert!(
        classic
            .events
            .iter()
            .any(|event| event == "runtimeSettingsChanged"),
        "Classic connected client did not receive Host settings-change event"
    );
    assert!(
        classic_cli(
            &cli,
            &runtime,
            Some("general.confirmWorkspaceRemoval"),
            json!(false),
            json!(true)
        )
        .is_err(),
        "CLI Agent must not bypass Classic-only safety switch"
    );

    // Validate persistence at the real process boundary, not only by
    // rebuilding in-memory Rust structs or rereading from the running Host.
    drop(classic);
    host.stop();
    assert!(native.desktop_settings_snapshot().is_err());
    let restarted = IsolatedHost::start(&cli, &runtime, &home);
    let after = native.desktop_settings_snapshot().unwrap();
    assert_eq!(after["values"]["editor.tabSize"], 10);
    assert_eq!(after["values"]["general.autoGitFetchIntervalMinutes"], 9);
    assert_eq!(after["values"]["general.confirmWorkspaceRemoval"], false);
    assert_eq!(after["values"]["editor.twoRowTabs"], false);
    assert_eq!(after["values"]["general.language"], "english");
    assert_eq!(after["values"]["terminal.cursorShape"], "bar");
    assert_eq!(classic_get(&cli, &runtime)["values"], after["values"]);
    restarted.stop();
    assert!(!runtime.join("runtime-host.json").exists());
}
