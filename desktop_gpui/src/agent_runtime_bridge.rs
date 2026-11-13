//! Narrow Agent Profile CLI bridge. The existing Alera CLI owns all Host
//! authentication, Agent launch semantics, MCP injection and PTY lifetimes.
//! No generic process runner is exposed to GPUI views.
use crate::agent_profiles::{
    parse_launched_agent, parse_profile_catalog, parse_profile_removal_impact, safe_runtime_id,
    validate_profile_name, AgentProfileRemovalImpact, AgentProfileRow, LaunchedAgentTab,
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
    CreateCommand {
        name: &'a str,
        agent_type: &'a str,
        command: &'a str,
    },
    Rename {
        id: &'a str,
        expected_revision: i64,
        name: &'a str,
    },
    Reorder {
        profiles: &'a [AgentProfileRow],
    },
    RemovalImpact {
        id: &'a str,
        expected_revision: i64,
    },
    RemoveConfirmed {
        id: &'a str,
        expected_revision: i64,
    },
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
        AllowedAgentRequest::CreateCommand {
            name,
            agent_type,
            command,
        } => {
            args.push(OsString::from("create"));
            args.push(OsString::from(format!("--name={name}")));
            args.push(OsString::from(format!("--agent-type={agent_type}")));
            args.push(OsString::from("--launch-mode=command"));
            args.push(OsString::from(format!("--command={command}")));
        }
        AllowedAgentRequest::Rename {
            id,
            expected_revision,
            name,
        } => {
            args.push(OsString::from("update"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
            args.push(OsString::from(format!("--name={name}")));
        }
        AllowedAgentRequest::Reorder { profiles } => {
            args.push(OsString::from("reorder"));
            for profile in *profiles {
                args.push(OsString::from(format!("--id={}", profile.id)));
            }
            for profile in *profiles {
                args.push(OsString::from(format!(
                    "--expected-revision={}={}",
                    profile.id,
                    profile.revision.unwrap_or(-1)
                )));
            }
        }
        AllowedAgentRequest::RemovalImpact {
            id,
            expected_revision,
        } => {
            args.push(OsString::from("removal-impact"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
        }
        AllowedAgentRequest::RemoveConfirmed {
            id,
            expected_revision,
        } => {
            args.push(OsString::from("remove"));
            args.push(OsString::from(format!("--profile-id={id}")));
            args.push(OsString::from(format!(
                "--expected-revision={expected_revision}"
            )));
            args.push(OsString::from("--confirm"));
        }
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

fn parse_saved_profile(value: Value) -> io::Result<AgentProfileRow> {
    let mut profiles = parse_profile_catalog(&serde_json::json!({"items":[value]}))?;
    profiles.pop().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Host returned no saved Agent Profile",
        )
    })
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

    /// Create only a normal command-mode profile with the canonical default
    /// executable. Advanced managed settings remain Host/Classic owned.
    pub fn create_command_agent_profile(
        &self,
        raw_name: &str,
        agent_type: &str,
    ) -> io::Result<AgentProfileRow> {
        let name = validate_profile_name(raw_name)?;
        let descriptor = alera_core::agent_descriptor::agent_descriptor(agent_type)
            .filter(|d| d.id == agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown Agent adapter"))?;
        let saved = self.invoke_existing_host_agent(AllowedAgentRequest::CreateCommand {
            name: &name,
            agent_type,
            command: descriptor.default_command,
        })?;
        let row = parse_saved_profile(saved)?;
        if row.name != name || row.agent_type != agent_type || row.managed || row.revision.is_none()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm created Agent Profile",
            ));
        }
        Ok(row)
    }

    /// Rename only: leave Host-managed command/config/prompt/credentials
    /// untouched. Refuse overwriting a revision the user never observed.
    pub fn rename_agent_profile(
        &self,
        profile_id: &str,
        expected_revision: i64,
        raw_name: &str,
    ) -> io::Result<AgentProfileRow> {
        if !safe_runtime_id(profile_id, 128) || expected_revision < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Agent Profile revision or ID",
            ));
        }
        let name = validate_profile_name(raw_name)?;
        let saved = self.invoke_existing_host_agent(AllowedAgentRequest::Rename {
            id: profile_id,
            expected_revision,
            name: &name,
        })?;
        let row = parse_saved_profile(saved)?;
        if row.id != profile_id
            || row.name != name
            || row
                .revision
                .is_none_or(|revision| revision <= expected_revision)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm revision-guarded Agent Profile rename",
            ));
        }
        Ok(row)
    }

    /// Change exactly one adjacent position using the complete Host revision
    /// snapshot. Never reorder silently when another view changed the catalog.
    pub fn move_agent_profile(
        &self,
        observed: &[AgentProfileRow],
        profile_id: &str,
        toward_start: bool,
    ) -> io::Result<Vec<AgentProfileRow>> {
        if observed.len() < 2 || observed.len() > 96 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Profile reorder needs at least two Host Profiles",
            ));
        }
        for (index, profile) in observed.iter().enumerate() {
            if !safe_runtime_id(&profile.id, 128)
                || profile.revision.is_none_or(|revision| revision < 0)
                || observed[..index]
                    .iter()
                    .any(|previous| previous.id == profile.id)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Profile reorder requires unique IDs and all Host revisions",
                ));
            }
        }
        let Some(from) = observed.iter().position(|profile| profile.id == profile_id) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Selected Agent Profile is no longer listed",
            ));
        };
        let target = if toward_start {
            from.checked_sub(1)
        } else {
            (from + 1 < observed.len()).then_some(from + 1)
        };
        let Some(to) = target else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Agent Profile is already at this end",
            ));
        };
        let mut ordered = observed.to_vec();
        ordered.swap(from, to);
        let response =
            self.invoke_existing_host_agent(AllowedAgentRequest::Reorder { profiles: &ordered })?;
        let readback = parse_profile_catalog(&response)?;
        if readback.len() != ordered.len()
            || readback
                .iter()
                .zip(&ordered)
                .any(|(actual, requested)| actual.id != requested.id)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm requested Agent Profile order",
            ));
        }
        Ok(readback)
    }

    /// Preview every Host reference before presenting an explicit deletion confirmation.
    pub fn preview_agent_profile_removal(
        &self,
        profile_id: &str,
        expected_revision: i64,
    ) -> io::Result<AgentProfileRemovalImpact> {
        if !safe_runtime_id(profile_id, 128) || expected_revision < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Agent Profile removal ID or revision",
            ));
        }
        let result = self.invoke_existing_host_agent(AllowedAgentRequest::RemovalImpact {
            id: profile_id,
            expected_revision,
        })?;
        parse_profile_removal_impact(&result, profile_id, expected_revision)
    }

    /// Only after an explicit UI confirmation: refetch the exact impact before
    /// issuing Host's revision-guarded --confirm. Host remains authoritative.
    pub fn remove_agent_profile(&self, observed: &AgentProfileRemovalImpact) -> io::Result<()> {
        if observed.has_blocking_references() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Profile is still used by active automation, policies or tabs",
            ));
        }
        let fresh = self.preview_agent_profile_removal(&observed.profile_id, observed.revision)?;
        if fresh != *observed {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Host Profile references changed since deletion preview; preview again",
            ));
        }
        let reply = self.invoke_existing_host_agent(AllowedAgentRequest::RemoveConfirmed {
            id: &observed.profile_id,
            expected_revision: observed.revision,
        })?;
        if reply.get("removed").and_then(Value::as_bool) != Some(true) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host did not confirm Agent Profile removal",
            ));
        }
        if self
            .list_agent_profiles()?
            .iter()
            .any(|profile| profile.id == observed.profile_id)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Deleted Agent Profile still appears in Host catalog",
            ));
        }
        Ok(())
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
            AllowedAgentRequest::List
            | AllowedAgentRequest::CreateCommand { .. }
            | AllowedAgentRequest::Rename { .. }
            | AllowedAgentRequest::Reorder { .. }
            | AllowedAgentRequest::RemovalImpact { .. }
            | AllowedAgentRequest::RemoveConfirmed { .. } => None,
            AllowedAgentRequest::Launch { prompt, .. } => Some(prompt.as_bytes()),
        };
        let timeout = if matches!(request, AllowedAgentRequest::List) {
            LIST_TIMEOUT
        } else {
            LAUNCH_TIMEOUT
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
    fn profile_create_and_rename_are_explicit_no_start_and_revision_guarded() {
        let root = Path::new("C:/isolated/runtime");
        let create = args_for(
            root,
            &AllowedAgentRequest::CreateCommand {
                name: "Reviewer",
                agent_type: "codex",
                command: "codex",
            },
        );
        assert_eq!(create[1], "--no-start");
        assert!(create.iter().any(|arg| arg == "create"));
        assert!(create.iter().any(|arg| arg == "--launch-mode=command"));
        assert!(create.iter().any(|arg| arg == "--name=Reviewer"));
        assert!(create.iter().any(|arg| arg == "--agent-type=codex"));
        let rename = args_for(
            root,
            &AllowedAgentRequest::Rename {
                id: "profile_1",
                expected_revision: 3,
                name: "Review Updated",
            },
        );
        assert_eq!(rename[1], "--no-start");
        assert!(rename.iter().any(|arg| arg == "update"));
        assert!(rename.iter().any(|arg| arg == "--profile-id=profile_1"));
        assert!(rename.iter().any(|arg| arg == "--expected-revision=3"));
        assert!(rename.iter().any(|arg| arg == "--name=Review Updated"));
    }

    #[test]
    fn profile_reorder_sends_full_snapshot_and_each_expected_revision() {
        let rows = [
            AgentProfileRow {
                id: "profile_2".into(),
                name: "B".into(),
                agent_type: "claude".into(),
                managed: false,
                revision: Some(0),
            },
            AgentProfileRow {
                id: "profile_1".into(),
                name: "A".into(),
                agent_type: "codex".into(),
                managed: false,
                revision: Some(3),
            },
        ];
        let args = args_for(
            Path::new("C:/isolated/runtime"),
            &AllowedAgentRequest::Reorder { profiles: &rows },
        );
        assert_eq!(args[1], "--no-start");
        assert!(args.iter().any(|part| part == "reorder"));
        assert!(args.iter().any(|part| part == "--id=profile_2"));
        assert!(args.iter().any(|part| part == "--id=profile_1"));
        assert!(args
            .iter()
            .any(|part| part == "--expected-revision=profile_2=0"));
        assert!(args
            .iter()
            .any(|part| part == "--expected-revision=profile_1=3"));
    }

    #[test]
    fn profile_removal_requires_revision_no_start_and_explicit_confirmation() {
        let runtime = Path::new("C:/isolated/runtime");
        let impact = args_for(
            runtime,
            &AllowedAgentRequest::RemovalImpact {
                id: "prof_1",
                expected_revision: 4,
            },
        );
        assert_eq!(impact[1], "--no-start");
        assert!(impact.iter().any(|arg| arg == "removal-impact"));
        assert!(impact.iter().any(|arg| arg == "--expected-revision=4"));
        assert!(!impact.iter().any(|arg| arg == "--confirm"));
        let remove = args_for(
            runtime,
            &AllowedAgentRequest::RemoveConfirmed {
                id: "prof_1",
                expected_revision: 4,
            },
        );
        assert_eq!(remove[1], "--no-start");
        assert!(remove.iter().any(|arg| arg == "remove"));
        assert!(remove.iter().any(|arg| arg == "--profile-id=prof_1"));
        assert!(remove.iter().any(|arg| arg == "--expected-revision=4"));
        assert!(remove.iter().any(|arg| arg == "--confirm"));
    }

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
        assert_eq!(
            bridge
                .create_command_agent_profile("Review", "codex")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            bridge
                .rename_agent_profile("p_1", 0, "Review Updated")
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            bridge
                .create_command_agent_profile("  ", "codex")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            bridge
                .create_command_agent_profile("Review", "unknown-adapter")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            bridge
                .rename_agent_profile("p_1", -1, "Review")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
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
