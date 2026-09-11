//! End-to-end protocol conformance test. Spawns the real `alera terminal-host`
//! binary, connects over the loopback socket it advertises, and drives the full
//! request/event sequence the Dart app relies on.

mod agent_integration_home_isolation;

mod terminal_host_test_platform;

#[path = "terminal_host_conformance/cli.rs"]
mod cli;
#[path = "terminal_host_conformance/lifecycle.rs"]
mod lifecycle;
#[path = "terminal_host_conformance/orchestration.rs"]
mod orchestration;
#[path = "terminal_host_conformance/resources.rs"]
mod resources;
#[path = "terminal_host_conformance/ssh_bootstrap.rs"]
mod ssh_bootstrap;
#[path = "terminal_host_conformance/terminal_sessions.rs"]
mod terminal_sessions;
#[path = "terminal_host_conformance/workspace.rs"]
mod workspace;

use agent_integration_home_isolation::alera_command_with_isolated_home;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::Child;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};

const PROTOCOL_VERSION: i64 = 4;

/// Kills the host process when the test ends, regardless of assertions.
struct HostGuard(Child);

impl Drop for HostGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn read_control(path: &std::path::Path) -> Option<(u16, String)> {
    let contents = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&contents).ok()?;
    let port = value.get("port")?.as_u64()? as u16;
    let token = value.get("token")?.as_str()?.to_string();
    Some((port, token))
}

fn send(writer: &mut TcpStream, message: Value) {
    let mut line = serde_json::to_vec(&message).unwrap();
    line.push(b'\n');
    writer.write_all(&line).unwrap();
    writer.flush().unwrap();
}

fn read_message(reader: &mut BufReader<TcpStream>) -> Value {
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .expect("timed out or failed reading from host");
    assert!(read > 0, "host closed the connection unexpectedly");
    serde_json::from_str(line.trim_end()).expect("host sent invalid JSON")
}

fn read_message_with_timeout(
    reader: &mut BufReader<TcpStream>,
    timeout: Duration,
) -> Option<Value> {
    reader.get_mut().set_read_timeout(Some(timeout)).unwrap();
    let mut line = String::new();
    let result = reader.read_line(&mut line);
    reader
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    match result {
        Ok(0) => panic!("host closed the connection unexpectedly"),
        Ok(_) => Some(serde_json::from_str(line.trim_end()).expect("host sent invalid JSON")),
        Err(error)
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.kind() == std::io::ErrorKind::TimedOut =>
        {
            None
        }
        Err(error) => panic!("timed out or failed reading from host: {error}"),
    }
}

fn connect(port: u16) -> (TcpStream, BufReader<TcpStream>) {
    let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let writer = stream.try_clone().unwrap();
    (writer, BufReader::new(stream))
}

fn read_response(reader: &mut BufReader<TcpStream>, id: i64) -> Value {
    loop {
        let message = read_message(reader);
        if message.get("id") == Some(&json!(id)) {
            return message;
        }
    }
}

/// Appends an `output` event's bytes to `sink`. Returns whether it was one.
fn collect_output(message: &Value, sink: &mut Vec<u8>) -> bool {
    if message.get("event").and_then(Value::as_str) != Some("output") {
        return false;
    }
    let encoded = message["payload"]["dataBase64"].as_str().unwrap();
    sink.extend_from_slice(&STANDARD.decode(encoded).unwrap());
    true
}

fn read_output_until(reader: &mut BufReader<TcpStream>, sink: &mut Vec<u8>, needle: &str) {
    while !String::from_utf8_lossy(sink).contains(needle) {
        let message = read_message(reader);
        collect_output(&message, sink);
    }
}

fn handshake(writer: &mut TcpStream, reader: &mut BufReader<TcpStream>, token: &str) {
    send(
        writer,
        json!({"id": 0, "type": "hello", "payload": {"protocolVersion": PROTOCOL_VERSION, "token": token}}),
    );
    let hello = read_message(reader);
    assert_eq!(hello["id"], json!(0));
    assert_eq!(hello["ok"], json!(true), "handshake rejected: {hello}");
}

fn create_long_running_session(
    writer: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
    id: i64,
    session_id: &str,
    workspace_id: &str,
    tab_id: &str,
) {
    let working_directory = terminal_host_test_platform::working_directory();
    send(
        writer,
        json!({
            "id": id,
            "type": "createOrAttach",
            "payload": {
                "sessionId": session_id,
                "workspaceId": workspace_id,
                "tabId": tab_id,
                "workingDirectory": working_directory,
                "launch": terminal_host_test_platform::long_running_launch(),
                "cols": 80,
                "rows": 24
            }
        }),
    );
    let created = read_response(reader, id);
    assert_eq!(
        created["ok"],
        json!(true),
        "createOrAttach failed: {created}"
    );
    assert_eq!(created["payload"]["running"], json!(true));
}

fn assert_session_is_not_attached(
    writer: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
    id: i64,
    session_id: &str,
) {
    send(
        writer,
        json!({
            "id": id,
            "type": "write",
            "payload": {
                "sessionId": session_id,
                "dataBase64": STANDARD.encode(b"ignored")
            }
        }),
    );
    let write = read_response(reader, id);
    assert_eq!(write["ok"], json!(false), "write succeeded: {write}");
    assert!(
        write["error"]
            .as_str()
            .is_some_and(|error| error.contains("Terminal session is not attached")),
        "unexpected write error: {write}"
    );
}

/// Spawn a host against the given runtime dir / control file and wait until it
/// publishes its loopback port.
fn spawn_host(
    runtime_dir: &std::path::Path,
    control_path: &std::path::Path,
    token: &str,
) -> (HostGuard, u16) {
    spawn_host_with_env(runtime_dir, control_path, token, &[])
}

fn spawn_host_with_env(
    runtime_dir: &std::path::Path,
    control_path: &std::path::Path,
    token: &str,
    env: &[(&str, &str)],
) -> (HostGuard, u16) {
    let mut command = alera_command_with_isolated_home(runtime_dir);
    command.args([
        "terminal-host",
        "--runtime-dir",
        runtime_dir.to_str().unwrap(),
        "--control-file",
        control_path.to_str().unwrap(),
        "--token",
        token,
        "--empty-shutdown-delay-seconds",
        "60",
        "--detached-session-shutdown-delay-seconds",
        "60",
    ]);
    for (key, value) in env {
        command.env(key, value);
    }
    let child = command
        .spawn()
        .expect("failed to spawn alera terminal-host");
    let guard = HostGuard(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some((port, _)) = read_control(control_path) {
            return (guard, port);
        }
        assert!(Instant::now() < deadline, "control file was never written");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn ssh_target_payload(id: &str, bootstrap_status: &str) -> Value {
    json!({
        "id": id,
        "alias": "Test Remote",
        "host": "example.invalid",
        "port": 22,
        "username": "tester",
        "platform": "linux",
        "arch": "x64",
        "authKind": "agent",
        "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z",
        "lastStatus": null,
        "installDir": null,
        "runtimeVersion": null,
        "runtimePlatform": null,
        "runtimeArch": null,
        "bootstrapStatus": bootstrap_status,
        "lastBootstrapAt": null,
        "lastCheckedAt": null,
        "lastError": null,
    })
}

fn fake_blocking_ssh_path(root: &std::path::Path) -> String {
    let bin_dir = root.join("fake-bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let ssh_path = bin_dir.join("ssh");
    std::fs::write(&ssh_path, "#!/bin/sh\nsleep 30\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = std::fs::metadata(&ssh_path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&ssh_path, permissions).unwrap();
    }
    let current_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{current_path}", bin_dir.display())
}

fn fake_ready_ssh_path(root: &std::path::Path) -> String {
    let bin_dir = root.join("fake-ready-bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let ssh_path = bin_dir.join("ssh");
    std::fs::write(&ssh_path, "#!/bin/sh\nprintf '/tmp/alera-runtime\\n'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = std::fs::metadata(&ssh_path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&ssh_path, permissions).unwrap();
    }
    let current_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{current_path}", bin_dir.display())
}
