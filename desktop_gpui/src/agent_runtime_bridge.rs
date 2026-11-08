//! Narrow Agent Profile CLI bridge. The existing Alera CLI owns all Host
//! authentication, Agent launch semantics, MCP injection and PTY lifetimes.
//! No generic process runner is exposed to GPUI views.
use crate::agent_profiles::{
    parse_launched_agent, parse_profile_catalog, safe_runtime_id, AgentProfileRow, LaunchedAgentTab,
};
use crate::runtime_bridge::RuntimeCliBridge;
use alera_core::child_process::windowless_command;
use serde_json::Value;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: u64 = 512 * 1024;
const MAX_PROMPT_BYTES: usize = 8 * 1024;
const LIST_TIMEOUT: Duration = Duration::from_secs(5);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);

enum AllowedAgentRequest<'a> {
    List,
    Launch {
        profile_id: &'a str,
        workspace_id: &'a str,
        prompt: &'a str,
    },
}

fn args_for(runtime: &Path, request: &AllowedAgentRequest<'_>) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("agent-profile"),
        OsString::from("--no-start"),
        OsString::from("--json"),
        OsString::from("--runtime-dir"),
        runtime.as_os_str().to_owned(),
    ];
    match request {
        AllowedAgentRequest::List => args.push(OsString::from("list")),
        AllowedAgentRequest::Launch {
            profile_id,
            workspace_id,
            ..
        } => {
            args.push(OsString::from("launch"));
            args.push(OsString::from(format!("--profile-id={profile_id}")));
            args.push(OsString::from(format!("--workspace={workspace_id}")));
            args.push(OsString::from("--prompt-stdin"));
        }
    }
    args
}

fn wait_bounded(child: &mut Child, timeout: Duration) -> io::Result<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(code)) => return Ok(code),
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Agent CLI timed out; its Host request may already have been applied",
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

impl RuntimeCliBridge {
    pub fn list_agent_profiles(&self) -> io::Result<Vec<AgentProfileRow>> {
        let response = self.invoke_existing_host_agent(AllowedAgentRequest::List)?;
        parse_profile_catalog(&response)
    }

    pub fn launch_agent_profile(
        &self,
        profile_id: &str,
        workspace_id: &str,
        prompt: &str,
    ) -> io::Result<LaunchedAgentTab> {
        if !safe_runtime_id(profile_id, 128) || !safe_runtime_id(workspace_id, 256) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Agent Profile or Host Workspace id",
            ));
        }
        let prompt = prompt.trim();
        if prompt.is_empty() || prompt.len() > MAX_PROMPT_BYTES || prompt.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Agent prompt must be nonempty, NUL-free, and at most 8 KiB",
            ));
        }
        let response = self.invoke_existing_host_agent(AllowedAgentRequest::Launch {
            profile_id,
            workspace_id,
            prompt,
        })?;
        parse_launched_agent(&response, profile_id, workspace_id)
    }

    fn invoke_existing_host_agent(&self, request: AllowedAgentRequest<'_>) -> io::Result<Value> {
        let (exe, runtime) = self.agent_cli_paths();
        // No accidental launch of a Host when Classic is closed. Older CLI
        // versions without --no-start fail closed instead of starting one.
        if !runtime.join("host.json").is_file() && !runtime.join("runtime-host.json").is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "No running Runtime Host; Native will not start one automatically",
            ));
        }
        let prompt = match &request {
            AllowedAgentRequest::List => None,
            AllowedAgentRequest::Launch { prompt, .. } => Some(prompt.as_bytes()),
        };
        let timeout = if prompt.is_some() {
            LAUNCH_TIMEOUT
        } else {
            LIST_TIMEOUT
        };
        let mut command = windowless_command(exe);
        command
            .args(args_for(runtime, &request))
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(if prompt.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if let Some(input) = prompt {
            let sent = child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("Agent stdin unavailable"))?
                .write_all(input);
            if let Err(error) = sent {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Agent stdout unavailable"))?;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout
                .take(MAX_RESPONSE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let exit = match wait_bounded(&mut child, timeout) {
            Ok(exit) => exit,
            Err(error) => {
                let _ = reader.join();
                return Err(error);
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("Agent stdout reader failed"))??;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Agent Profile CLI response exceeds 512 KiB",
            ));
        }
        if !exit.success() {
            return Err(io::Error::other(
                "Agent Profile CLI failed; verify the existing Host and the CLI supports --no-start",
            ));
        }
        serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_cli_uses_only_explicit_no_start_json_verbs_and_never_exposes_prompt_in_argv() {
        let runtime = Path::new("C:/isolated/runtime");
        let list = args_for(runtime, &AllowedAgentRequest::List);
        assert_eq!(list[0], "agent-profile");
        assert_eq!(list[1], "--no-start");
        assert_eq!(list.last().unwrap(), "list");
        let launch = args_for(
            runtime,
            &AllowedAgentRequest::Launch {
                profile_id: "profile_1",
                workspace_id: "workspace_2",
                prompt: "SECRET QUESTION",
            },
        );
        assert!(launch.iter().any(|part| part == "--prompt-stdin"));
        assert!(launch.iter().any(|part| part == "--profile-id=profile_1"));
        assert!(launch.iter().any(|part| part == "--workspace=workspace_2"));
        assert!(!launch
            .iter()
            .any(|part| part.to_string_lossy().contains("SECRET")));
    }

    #[test]
    fn no_host_never_executes_agent_or_creates_runtime_directory() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("runtime");
        let bridge = RuntimeCliBridge::new(dir.path().join("not-an-executable.exe"), &runtime);
        assert_eq!(
            bridge.list_agent_profiles().unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            bridge
                .launch_agent_profile("p_1", "w_1", "Check work")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotConnected
        );
        assert!(!runtime.exists());
        assert_eq!(
            bridge
                .launch_agent_profile("../escape", "w_1", "Check work")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            bridge
                .launch_agent_profile("p_1", "w_1", " ")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
