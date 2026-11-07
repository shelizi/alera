//! Optional bridge to the *existing* Alera CLI Runtime Host.
//! No RPC token parsing, direct SQLite access, host startup, or PTY spawning.
//! The CLI owns authentication, version negotiation and request semantics.
use alera_core::child_process::windowless_command;
use serde_json::Value;
use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: u64 = 128 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// `alera terminal` invocation that never auto-starts an unknown Host.
pub fn terminal_cli_arguments(runtime_dir: &Path, args: &[OsString]) -> Vec<OsString> {
    let mut command = Vec::with_capacity(5 + args.len());
    command.push(OsString::from("terminal"));
    command.push(OsString::from("--no-start"));
    command.push(OsString::from("--json"));
    command.push(OsString::from("--runtime-dir"));
    command.push(runtime_dir.as_os_str().to_os_string());
    command.extend(args.iter().cloned());
    command
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeStatus {
    Stopped,
    Running { active_sessions: u64 },
}

impl RuntimeStatus {
    pub fn label(&self) -> String {
        match self {
            RuntimeStatus::Stopped => "Runtime Host: offline".into(),
            RuntimeStatus::Running { active_sessions } => {
                format!("Runtime Host: online · {active_sessions} active terminal(s)")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeCliBridge {
    executable: PathBuf,
    runtime_dir: PathBuf,
}

impl RuntimeCliBridge {
    pub fn discover() -> io::Result<Self> {
        let configured = std::env::var_os("ALERA_CLI_PATH").filter(|value| !value.is_empty());
        let executable = match configured {
            Some(value) => {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "ALERA_CLI_PATH must be an absolute executable path",
                    ));
                }
                path
            }
            None => {
                let current = std::env::current_exe()?;
                current
                    .parent()
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotFound, "no executable directory")
                    })?
                    .join(if cfg!(windows) { "alera.exe" } else { "alera" })
            }
        };
        let executable = dunce::canonicalize(executable)?;
        if !executable.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Alera CLI is not a file",
            ));
        }
        Ok(Self::new(executable, default_runtime_dir()))
    }

    pub fn new(executable: impl Into<PathBuf>, runtime_dir: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            runtime_dir: runtime_dir.into(),
        }
    }

    /// Only allow listing, reading and explicit input to an existing Host.
    /// --no-start is mandatory; older CLI builds fail closed, rather than
    /// silently starting a new runtime host or owning a PTY.
    pub(crate) fn terminal_json(&self, args: &[OsString]) -> io::Result<Value> {
        if !matches!(
            args.first().and_then(|arg| arg.to_str()),
            Some("list" | "read" | "write")
        ) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "GPUI supports terminal list/read/explicit write only",
            ));
        }
        if !self.runtime_dir.join("host.json").is_file()
            && !self.runtime_dir.join("runtime-host.json").is_file()
        {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "No existing runtime host control file",
            ));
        }
        let mut command = windowless_command(&self.executable);
        command
            .args(terminal_cli_arguments(&self.runtime_dir, args))
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("CLI stdout pipe unavailable"))?;
        const TERMINAL_OUTPUT_LIMIT: u64 = 512 * 1024;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout
                .take(TERMINAL_OUTPUT_LIMIT + 1)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let status = match wait_with_timeout(&mut child, PROBE_TIMEOUT) {
            Ok(status) => status,
            Err(error) => {
                let _ = reader.join();
                return Err(error);
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("CLI response reader panicked"))??;
        if !status.success() {
            return Err(io::Error::other(format!(
                "Alera CLI failed ({status}): Host may be offline, incompatible, or CLI lacks --no-start"
            )));
        }
        if bytes.len() as u64 > TERMINAL_OUTPUT_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Terminal CLI output exceeded 512 KiB",
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Never start a host on behalf of the GPUI preview, and never read its
    /// control file's token. The CLI itself performs authenticated requests.
    pub fn probe(&self) -> io::Result<RuntimeStatus> {
        if !self.runtime_dir.join("host.json").is_file()
            && !self.runtime_dir.join("runtime-host.json").is_file()
        {
            return Ok(RuntimeStatus::Stopped);
        }
        let mut command = windowless_command(&self.executable);
        command
            .arg("runtime")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("status")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("CLI stdout pipe unavailable"))?;

        // Drain while the process runs: even unexpectedly large CLI output
        // cannot fill a pipe and hang the UI or the subprocess indefinitely.
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout
                .take(MAX_RESPONSE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let status = match wait_with_timeout(&mut child, PROBE_TIMEOUT) {
            Ok(status) => status,
            Err(error) => {
                let _ = reader.join();
                return Err(error);
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("CLI response reader panicked"))??;
        if !status.success() {
            return Err(io::Error::other(format!("Alera CLI returned {status}")));
        }
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CLI output exceeded 128 KiB",
            ));
        }
        parse_status(&bytes)
    }
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> io::Result<std::process::ExitStatus> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Alera CLI status timed out",
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
}

fn parse_status(bytes: &[u8]) -> io::Result<RuntimeStatus> {
    let payload: Value = serde_json::from_slice(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    match payload.get("running").and_then(Value::as_bool) {
        Some(true) => Ok(RuntimeStatus::Running {
            active_sessions: payload
                .get("host")
                .and_then(|host| host.get("activeSessions"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
        }),
        Some(false) => Ok(RuntimeStatus::Stopped),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "CLI status response is missing running boolean",
        )),
    }
}

fn default_runtime_dir() -> PathBuf {
    // Match rust/alera-cli/src/main.rs:runtime_dir without depending on
    // Alera's private CLI implementation or duplicating its auth mechanism.
    if let Some(value) = std::env::var_os("ALERA_RUNTIME_DIR").filter(|value| !value.is_empty()) {
        return PathBuf::from(value);
    }
    let home: Option<OsString> =
        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    PathBuf::from(home.unwrap_or_else(|| OsString::from(".")))
        .join(".alera")
        .join("runtime")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interprets_existing_cli_runtime_status_contract() {
        assert_eq!(
            parse_status(br#"{"status":"ok","running":true,"host":{"activeSessions":4}}"#).unwrap(),
            RuntimeStatus::Running { active_sessions: 4 }
        );
        assert_eq!(
            parse_status(br#"{"status":"ok","running":false,"host":null}"#).unwrap(),
            RuntimeStatus::Stopped
        );
        assert!(RuntimeStatus::Running { active_sessions: 2 }
            .label()
            .contains("2 active"));
    }

    #[test]
    fn rejects_malformed_or_unrecognized_host_payloads() {
        for content in ["{}", r#"{"running":"true"}"#, "not json"] {
            assert_eq!(
                parse_status(content.as_bytes()).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn missing_control_file_does_not_spawn_or_create_runtime() {
        let runtime = tempfile::tempdir().unwrap();
        let fake_cli = runtime.path().join("missing-alera-cli");
        let bridge = RuntimeCliBridge::new(fake_cli, runtime.path());
        assert_eq!(bridge.probe().unwrap(), RuntimeStatus::Stopped);
        assert!(!runtime.path().join("runtime-host.json").exists());
        assert!(!runtime.path().join("host.json").exists());
    }
}
