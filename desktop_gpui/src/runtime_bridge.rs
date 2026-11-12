//! Optional bridge to the *existing* Alera CLI Runtime Host.
//! No RPC token parsing, direct SQLite access, host startup, or PTY spawning.
//! The CLI owns authentication, version negotiation and request semantics.
use crate::host_tab_pins::{parse_pin_readback, parse_pin_snapshot, HostTabPinSnapshot};
use alera_core::child_process::windowless_command;
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: u64 = 128 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const QUOTA_TIMEOUT: Duration = Duration::from_secs(70);
const MAX_QUOTA_RESPONSE_BYTES: u64 = 512 * 1024;

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
pub struct ProjectRemovalPreview {
    pub workspace_count: u64,
    pub tab_count: u64,
    pub active_session_count: u64,
    pub has_config_override: bool,
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

    /// Read Classic's Host quota snapshot. Native never starts a Host, reads
    /// credentials, or invokes an agent CLI directly.
    pub fn agent_quota_snapshot(
        &self,
        force_refresh: bool,
        provider: Option<&str>,
        account_id: Option<&str>,
    ) -> io::Result<Value> {
        if provider.is_none() && account_id.is_some() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Quota account requires provider",
            ));
        }
        if provider
            .is_some_and(|value| value.len() > 32 || !value.bytes().all(|b| b.is_ascii_lowercase()))
            || account_id.is_some_and(|value| value.is_empty() || value.len() > 256)
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Invalid quota filter",
            ));
        }
        if !self.runtime_dir.join("host.json").is_file()
            && !self.runtime_dir.join("runtime-host.json").is_file()
        {
            return Err(io::Error::new(
                ErrorKind::NotConnected,
                "Classic Runtime Host is not running",
            ));
        }
        let mut args = vec![
            OsString::from("quota"),
            OsString::from("--no-start"),
            OsString::from("--json"),
            OsString::from("--runtime-dir"),
            self.runtime_dir.as_os_str().to_os_string(),
            OsString::from("snapshot"),
        ];
        if force_refresh {
            args.push(OsString::from("--force-refresh"));
        }
        if let Some(provider) = provider {
            args.extend([OsString::from("--provider"), OsString::from(provider)]);
        }
        if let Some(account_id) = account_id {
            args.extend([OsString::from("--account-id"), OsString::from(account_id)]);
        }
        let mut child = windowless_command(&self.executable)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Quota CLI stdout missing"))?;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout
                .take(MAX_QUOTA_RESPONSE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let status = match wait_with_timeout(&mut child, QUOTA_TIMEOUT) {
            Ok(status) => status,
            Err(error) => {
                let _ = reader.join();
                return Err(error);
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("Quota CLI reader panicked"))??;
        if !status.success() {
            return Err(io::Error::other(
                "Quota unavailable: existing Runtime Host/CLI is offline or incompatible",
            ));
        }
        if bytes.len() as u64 > MAX_QUOTA_RESPONSE_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Quota response exceeds 512 KiB",
            ));
        }
        let result: Value = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        if !result.get("snapshots").is_some_and(Value::is_array) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Invalid quota snapshot payload",
            ));
        }
        Ok(result)
    }

    /// Return only the Host-configured enabled provider order. No environment
    /// keys, credentials, profile command lines or other Host fields.
    pub fn agent_quota_enabled_order(&self) -> io::Result<Vec<String>> {
        if !self.runtime_dir.join("host.json").is_file()
            && !self.runtime_dir.join("runtime-host.json").is_file()
        {
            return Err(io::Error::new(
                ErrorKind::NotConnected,
                "Classic Runtime Host is offline",
            ));
        }
        let mut child = windowless_command(&self.executable)
            .arg("quota")
            .arg("--no-start")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("config")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Quota config stdout missing"))?;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout
                .take(MAX_QUOTA_RESPONSE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let status = match wait_with_timeout(&mut child, QUOTA_TIMEOUT) {
            Ok(status) => status,
            Err(error) => {
                let _ = reader.join();
                return Err(error);
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("Quota config reader panicked"))??;
        if !status.success() || bytes.len() as u64 > MAX_QUOTA_RESPONSE_BYTES {
            return Err(io::Error::other(
                "Quota config unavailable: existing Host or CLI is incompatible",
            ));
        }
        let response: Value = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        let order = response["enabledProviders"]
            .as_array()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Missing quota display order"))?;
        if order.len() > 10 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Too many quota providers",
            ));
        }
        let allowed = [
            "claude", "codex", "kimi", "grok", "cursor", "agy", "minimax", "zai", "devin",
            "opencode",
        ];
        let mut safe = Vec::<String>::new();
        for value in order {
            let id = value
                .as_str()
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Invalid quota ID"))?;
            if !allowed.contains(&id) || safe.iter().any(|existing| existing == id) {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Unsupported quota ID",
                ));
            }
            safe.push(id.to_owned());
        }
        Ok(safe)
    }

    /// Internal subprocess parameters; the Agent adapter still checks for an
    /// existing Host and can invoke only strict no-start CLI verbs.
    pub(crate) fn agent_cli_paths(&self) -> (&Path, &Path) {
        (&self.executable, &self.runtime_dir)
    }

    /// Only allow listing, reading, workspace listing, explicit creation and
    /// input to an existing Host. --no-start is mandatory; older CLI builds
    /// fail closed, rather than silently starting a new runtime host or
    /// owning a PTY.
    pub(crate) fn terminal_json(&self, args: &[OsString]) -> io::Result<Value> {
        self.terminal_json_io(args, None)
    }

    /// Explicit user input takes the child process's stdin pipe so command
    /// text cannot leak through the operating system process argument list.
    pub(crate) fn terminal_json_with_input(
        &self,
        args: &[OsString],
        input: &[u8],
    ) -> io::Result<Value> {
        self.terminal_json_io(args, Some(input))
    }

    fn terminal_json_io(&self, args: &[OsString], input: Option<&[u8]>) -> io::Result<Value> {
        let verb = args.first().and_then(|arg| arg.to_str());
        let allowed = match (verb, input.is_some()) {
            (Some("list" | "read"), false) | (Some("write"), true) => true,
            // Workspace listing and terminal creation are Host requests with
            // no stdin payload; both still require --no-start below.
            (Some("workspaces"), false) => args.len() == 1,
            (Some("create"), false) => is_create_terminal_args(args),
            (Some("resize"), false) => is_resize_terminal_args(args),
            _ => false,
        };
        if !allowed {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "GPUI supports terminal list/read/workspaces/create/explicit write only",
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
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if let Some(bytes) = input {
            let result = child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("CLI stdin pipe unavailable"))
                .and_then(|mut stdin| stdin.write_all(bytes));
            if let Err(error) = result {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
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

    /// Read exactly six non-secret Classic/Native shared settings through the
    /// already running Host; this does not read RuntimeStore SQLite directly.
    pub fn desktop_settings_snapshot(&self) -> io::Result<Value> {
        self.desktop_settings_command(&[OsString::from("get")])
    }

    /// Read-only Host Hook flags, through a secret-free CLI projection.
    /// An older CLI/Host or malformed reply fails closed; no inference from
    /// a running Agent process may silently enable native notifications.
    pub fn agent_hook_settings_snapshot(&self) -> io::Result<HashMap<String, bool>> {
        Ok(self.agent_hook_settings_versioned_snapshot()?.switches)
    }

    pub fn agent_hook_settings_versioned_snapshot(&self) -> io::Result<AgentHookSettingsSnapshot> {
        let response = self.desktop_settings_command(&[OsString::from("agent-hooks-get")])?;
        parse_versioned_agent_hook_settings(&response)
    }

    /// Writes exactly one Agent Hook via Host's atomic expectedRevision CAS.
    /// Caller must supply an observed, unmodified Host snapshot. CLI also
    /// compares the old Provider value before sending its two-field update.
    pub fn agent_hook_settings_compare_and_set(
        &self,
        observed: &AgentHookSettingsSnapshot,
        agent: &str,
        enabled: bool,
    ) -> io::Result<AgentHookSettingsSnapshot> {
        if agent.is_empty()
            || agent.len() > 64
            || !agent.bytes().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'_' | b'.')
            })
            || observed.revision < 0
            || observed.revision == i64::MAX
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Invalid Agent Hook CAS target",
            ));
        }
        let response = self.desktop_settings_command(&[
            OsString::from("agent-hooks-cas"),
            OsString::from("--agent-id"),
            OsString::from(agent),
            OsString::from("--expected-revision"),
            OsString::from(observed.revision.to_string()),
            OsString::from("--expected-enabled"),
            OsString::from(
                observed
                    .switches
                    .get(agent)
                    .copied()
                    .unwrap_or(false)
                    .to_string(),
            ),
            OsString::from("--enabled"),
            OsString::from(enabled.to_string()),
        ])?;
        validate_agent_hook_cas_readback(observed, agent, enabled, &response)
    }

    pub fn git_favorites_for_workspace(&self, root: &Path) -> io::Result<Value> {
        self.desktop_settings_command(&[
            OsString::from("favorites-get"),
            OsString::from("--root-path"),
            root.as_os_str().to_owned(),
        ])
    }

    /// Seed only when an old Native file *actually existed* for this root.
    /// The Host will never overwrite an existing Classic or GPUI scope.
    pub fn git_favorites_seed_native(&self, root: &Path, branches: &[String]) -> io::Result<Value> {
        self.desktop_settings_command(&[
            OsString::from("favorites-seed"),
            OsString::from("--root-path"),
            root.as_os_str().to_owned(),
            OsString::from("--branches"),
            OsString::from(serde_json::to_string(branches)?),
        ])
    }

    pub fn git_favorites_compare_and_set(
        &self,
        root: &Path,
        expected: &[String],
        value: &[String],
    ) -> io::Result<Value> {
        self.desktop_settings_command(&[
            OsString::from("favorites-set"),
            OsString::from("--root-path"),
            root.as_os_str().to_owned(),
            OsString::from("--expected"),
            OsString::from(serde_json::to_string(expected)?),
            OsString::from("--value"),
            OsString::from(serde_json::to_string(value)?),
        ])
    }

    /// Older GPUI builds saved the default Agent Profile in Native JSON.
    /// Migrate at most once in the Host; the Host checks the existing catalog
    /// and prefers an already saved Classic selection.
    pub fn desktop_settings_migrate_profile(&self, profile: Option<&str>) -> io::Result<Value> {
        let mut args = vec![OsString::from("migrate-profile")];
        if let Some(profile) = profile {
            args.push(OsString::from("--profile-id"));
            args.push(OsString::from(profile));
        }
        self.desktop_settings_command(&args)
    }

    pub fn desktop_settings_migrate_explorer(
        &self,
        show_hidden: bool,
        show_ignored: bool,
    ) -> io::Result<Value> {
        self.desktop_settings_command(&[
            OsString::from("migrate-explorer"),
            OsString::from("--show-hidden"),
            OsString::from(show_hidden.to_string()),
            OsString::from("--show-ignored"),
            OsString::from(show_ignored.to_string()),
        ])
    }

    /// Seed a persisted Native-only choice without overwriting Classic's
    /// existing Rust value. Never writes the legacy Native JSON.
    pub fn desktop_settings_migrate_two_row(&self, enabled: bool) -> io::Result<Value> {
        self.desktop_settings_command(&[
            OsString::from("migrate-two-row"),
            OsString::from("--enabled"),
            OsString::from(enabled.to_string()),
        ])
    }

    /// Read secret-free pin/revision metadata through the authenticated Host.
    pub fn host_tab_pin_list(&self, workspace_id: &str) -> io::Result<Vec<HostTabPinSnapshot>> {
        if workspace_id.is_empty() || workspace_id.len() > 256 {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Invalid Host workspace",
            ));
        }
        let value = self.desktop_settings_command(&[
            OsString::from("pin-list"),
            OsString::from("--workspace-id"),
            OsString::from(workspace_id),
        ])?;
        parse_pin_snapshot(&value, workspace_id)
    }

    /// Never accept a stale/recycled Terminal or persist an optimistic pin.
    pub fn host_tab_pin_compare_and_set(
        &self,
        observed: &HostTabPinSnapshot,
        pinned: bool,
    ) -> io::Result<HostTabPinSnapshot> {
        let session = observed.terminal_session_id.as_deref().ok_or_else(|| {
            io::Error::new(ErrorKind::InvalidInput, "Tab has no Host Terminal session")
        })?;
        let value = self.desktop_settings_command(&[
            OsString::from("pin-set"),
            OsString::from("--workspace-id"),
            OsString::from(&observed.workspace_id),
            OsString::from("--id"),
            OsString::from(&observed.id),
            OsString::from("--expected-updated-at"),
            OsString::from(&observed.updated_at),
            OsString::from("--expected-pinned"),
            OsString::from(observed.pinned.to_string()),
            OsString::from("--pinned"),
            OsString::from(pinned.to_string()),
            OsString::from("--expected-terminal-session-id"),
            OsString::from(session),
        ])?;
        parse_pin_readback(&value, observed, pinned)
    }

    pub fn desktop_settings_compare_and_set(
        &self,
        path: &str,
        expected: &Value,
        next: &Value,
    ) -> io::Result<Value> {
        if !alera_core::desktop_settings_contract::cli_writable(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown shared Settings path",
            ));
        }
        self.desktop_settings_command(&[
            OsString::from("set"),
            OsString::from("--path"),
            OsString::from(path),
            OsString::from("--expected"),
            OsString::from(expected.to_string()),
            OsString::from("--value"),
            OsString::from(next.to_string()),
        ])
    }

    fn desktop_settings_command(&self, args: &[OsString]) -> io::Result<Value> {
        if !self.runtime_dir.join("host.json").is_file()
            && !self.runtime_dir.join("runtime-host.json").is_file()
        {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Classic Runtime Host is not running",
            ));
        }
        let mut command = windowless_command(&self.executable);
        command
            .arg("settings")
            .arg("--no-start")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Settings CLI stdout missing"))?;
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
            .map_err(|_| io::Error::other("Settings CLI reader panicked"))??;
        if !status.success() {
            return Err(io::Error::other("Shared settings unavailable: Host/CLI missing feature, offline or conflicting edit"));
        }
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Settings response exceeded limit",
            ));
        }
        let response: Value = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        validate_desktop_settings_response(args.first().and_then(|arg| arg.to_str()), &response)?;
        Ok(response)
    }

    /// Query Classic's persistent RuntimeStore using the existing CLI.
    /// Read-only; does not create an RPC session or start any Host.
    pub fn list_runtime_catalog(&self) -> io::Result<crate::runtime_catalog::RuntimeCatalog> {
        let projects = self.catalog_list_json("project")?;
        let workspaces = self.catalog_list_json("workspace")?;
        let tags = self.catalog_list_json("tag")?;
        let sections = self.catalog_list_json("workspace-section")?;
        let mut catalog = crate::runtime_catalog::parse_runtime_catalog(&projects, &workspaces)?;
        crate::runtime_catalog::attach_runtime_tags(&mut catalog, &tags)?;
        crate::runtime_catalog::attach_runtime_sections(&mut catalog, &sections)?;
        Ok(catalog)
    }

    fn catalog_list_json(&self, noun: &str) -> io::Result<Value> {
        if !matches!(noun, "project" | "workspace" | "tag" | "workspace-section") {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Only read-only project/workspace listings are allowed",
            ));
        }
        let mut command = windowless_command(&self.executable);
        command
            .arg(if noun == "workspace-section" {
                "workspace"
            } else {
                noun
            })
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg(if noun == "workspace-section" {
                "section-list"
            } else {
                "list"
            });
        if noun == "workspace" {
            command.arg("--all");
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("CLI catalog stdout pipe unavailable"))?;
        const LIMIT: u64 = 512 * 1024;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout.take(LIMIT + 1).read_to_end(&mut bytes)?;
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
            .map_err(|_| io::Error::other("Catalog reader panicked"))??;
        if bytes.len() as u64 > LIMIT {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "CLI catalog output too large",
            ));
        }
        if !status.success() {
            return Err(io::Error::other("Alera CLI catalog query failed"));
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Use Classic's Host-managed create flow for registered projects.
    /// The Host owns Git, workspace registration and setup; GPUI never falls
    /// back to an unregistered Git checkout after a Host error.
    pub fn create_managed_workspace(
        &self,
        project_id: &str,
        project_root: &Path,
        branch: &str,
        reuse_existing_branch: bool,
    ) -> io::Result<PathBuf> {
        if project_id.is_empty()
            || project_id.starts_with('-')
            || project_id.len() > 128
            || !project_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Invalid project ID",
            ));
        }
        crate::worktree_manager::validate_branch(branch)?;

        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("add")
            .arg("--project-id")
            .arg(project_id)
            .arg("--branch")
            .arg(branch);
        if reuse_existing_branch {
            command.arg("--reuse-existing-branch");
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Managed workspace create output unavailable"))?;
        const LIMIT: u64 = 512 * 1024;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout.take(LIMIT + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let status = match wait_with_timeout(&mut child, Duration::from_secs(180)) {
            Ok(status) => status,
            Err(error) => {
                let _ = reader.join();
                return Err(error);
            }
        };
        let bytes = reader
            .join()
            .map_err(|_| io::Error::other("Managed workspace output reader panicked"))??;
        if !status.success() {
            return Err(io::Error::other(
                "Classic Host rejected managed workspace creation",
            ));
        }
        if bytes.len() as u64 > LIMIT {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Managed workspace response too large",
            ));
        }
        let result: Value = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        let path = result
            .pointer("/workspace/path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                io::Error::new(ErrorKind::InvalidData, "Host omitted new workspace path")
            })?;
        let path = dunce::canonicalize(path)?;
        let manager = crate::worktree_manager::WorktreeManager::discover(project_root)?;
        if !manager
            .list()?
            .iter()
            .any(|entry| entry.path == path && !entry.is_primary)
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Host response is not a registered linked Git worktree",
            ));
        }
        Ok(path)
    }

    /// Sleep an inactive Classic workspace, closing Host-managed tabs/sessions
    /// without deleting its branch, worktree or files. UI must confirm first.
    pub fn sleep_registered_workspace(&self, workspace_id: &str) -> io::Result<()> {
        validate_catalog_identifier(workspace_id)?;
        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("sleep")
            .arg("--id")
            .arg(workspace_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, Duration::from_secs(60))?.success() {
            return Err(io::Error::other("Classic Host workspace sleep refused"));
        }
        Ok(())
    }

    /// Archive/restore only the registered metadata; Git branch and files
    /// are kept intact by Classic RuntimeStore.
    pub fn set_workspace_archived(&self, workspace_id: &str, archived: bool) -> io::Result<()> {
        validate_catalog_identifier(workspace_id)?;
        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg(if archived { "archive" } else { "restore" })
            .arg("--id")
            .arg(workspace_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, PROBE_TIMEOUT)?.success() {
            return Err(io::Error::other("Classic workspace archive/restore failed"));
        }
        Ok(())
    }

    /// Move a registered workspace into an existing Classic section, or into
    /// Others when section_id is None. Never create a duplicate local section.
    pub fn set_workspace_section(
        &self,
        workspace_id: &str,
        section_id: Option<&str>,
    ) -> io::Result<()> {
        validate_catalog_identifier(workspace_id)?;
        if let Some(id) = section_id {
            validate_catalog_identifier(id)?;
        }
        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("section-set")
            .arg("--workspace-id")
            .arg(workspace_id);
        if let Some(id) = section_id {
            command.arg("--section-id").arg(id);
        } else {
            command.arg("--clear");
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, PROBE_TIMEOUT)?.success() {
            return Err(io::Error::other("Classic workspace section update failed"));
        }
        Ok(())
    }

    pub fn remove_workspace_section(&self, section_id: &str) -> io::Result<()> {
        validate_catalog_identifier(section_id)?;
        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("section-remove")
            .arg("--id")
            .arg(section_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, PROBE_TIMEOUT)?.success() {
            return Err(io::Error::other("Classic section removal refused"));
        }
        Ok(())
    }

    pub fn create_workspace_section(&self, workspace_id: &str, name: &str) -> io::Result<()> {
        validate_catalog_identifier(workspace_id)?;
        validate_display_name(name)?;
        if name.eq_ignore_ascii_case("Others") {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Others is a reserved section",
            ));
        }
        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("section-create")
            .arg("--workspace-id")
            .arg(workspace_id)
            .arg("--name")
            .arg(name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, PROBE_TIMEOUT)?.success() {
            return Err(io::Error::other(
                "Classic workspace section creation failed",
            ));
        }
        Ok(())
    }

    /// Change one registered RuntimeStore workspace only after a user action.
    /// No direct SQLite writes, no Host startup, no Git worktree mutation.
    pub fn set_workspace_pinned(&self, workspace_id: &str, pinned: bool) -> io::Result<()> {
        if workspace_id.is_empty()
            || workspace_id.starts_with('-')
            || workspace_id.len() > 128
            || !workspace_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Invalid workspace ID",
            ));
        }
        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg(if pinned { "pin" } else { "unpin" })
            .arg("--id")
            .arg(workspace_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let status = wait_with_timeout(&mut child, PROBE_TIMEOUT)?;
        if !status.success() {
            return Err(io::Error::other("Alera CLI workspace pin/unpin failed"));
        }
        Ok(())
    }

    /// Link or unlink existing Classic RuntimeStore workspaces without
    /// moving their Git paths, Git branches, or native PTY ownership.
    pub fn set_workspace_parent(
        &self,
        parent_id: &str,
        child_id: &str,
        link: bool,
    ) -> io::Result<()> {
        validate_catalog_identifier(parent_id)?;
        validate_catalog_identifier(child_id)?;
        if parent_id == child_id {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "A workspace cannot be its own parent",
            ));
        }
        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg(if link { "link" } else { "unlink" })
            .arg("--parent-workspace-id")
            .arg(parent_id)
            .arg("--child-workspace-id")
            .arg(child_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, PROBE_TIMEOUT)?.success() {
            return Err(io::Error::other(
                "Classic workspace relation change rejected",
            ));
        }
        Ok(())
    }

    /// Create a globally shared Classic tag using Host uniqueness checks.
    pub fn create_workspace_tag(&self, name: &str) -> io::Result<String> {
        validate_display_name(name)?;
        if name.len() > 64 {
            return Err(io::Error::new(ErrorKind::InvalidInput, "Tag name too long"));
        }
        let mut command = windowless_command(&self.executable);
        command
            .arg("tag")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("create")
            .arg("--name")
            .arg(name)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Tag creation output unavailable"))?;
        const LIMIT: u64 = 16 * 1024;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout.take(LIMIT + 1).read_to_end(&mut bytes)?;
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
            .map_err(|_| io::Error::other("Tag reader panicked"))??;
        if !status.success() || bytes.len() as u64 > LIMIT {
            return Err(io::Error::other("Classic tag creation refused"));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Created tag missing ID"))?;
        validate_catalog_identifier(id)?;
        Ok(id.to_string())
    }

    /// Remove a globally shared Classic tag only after native UI double confirmation.
    /// Classic Host clears all workspace assignments in the same registry.
    pub fn remove_global_tag(&self, tag_id: &str) -> io::Result<()> {
        validate_catalog_identifier(tag_id)?;
        let mut command = windowless_command(&self.executable);
        command
            .arg("tag")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("remove")
            .arg("--id")
            .arg(tag_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, PROBE_TIMEOUT)?.success() {
            return Err(io::Error::other("Classic Host refused global tag removal"));
        }
        Ok(())
    }

    /// Assign or remove a Classic RuntimeStore workspace tag.
    pub fn set_workspace_tag(
        &self,
        workspace_id: &str,
        tag_id: &str,
        assign: bool,
    ) -> io::Result<()> {
        validate_catalog_identifier(workspace_id)?;
        validate_catalog_identifier(tag_id)?;
        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg(if assign { "tag" } else { "untag" })
            .arg("--workspace-id")
            .arg(workspace_id)
            .arg("--tag-id")
            .arg(tag_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, PROBE_TIMEOUT)?.success() {
            return Err(io::Error::other("Classic workspace tag change rejected"));
        }
        Ok(())
    }

    /// Query Classic Host for removal impact, without modifying any project.
    pub fn preview_project_removal(&self, project_id: &str) -> io::Result<ProjectRemovalPreview> {
        validate_catalog_identifier(project_id)?;
        let mut command = windowless_command(&self.executable);
        command
            .arg("project")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("remove-preview")
            .arg("--id")
            .arg(project_id)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Remove preview stdout unavailable"))?;
        const LIMIT: u64 = 16 * 1024;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout.take(LIMIT + 1).read_to_end(&mut bytes)?;
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
            .map_err(|_| io::Error::other("Remove preview reader panicked"))??;
        if !status.success() || bytes.len() as u64 > LIMIT {
            return Err(io::Error::other(
                "Classic project remove preview unavailable",
            ));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        if value.get("projectId").and_then(Value::as_str) != Some(project_id) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Removal preview identity mismatch",
            ));
        }
        Ok(ProjectRemovalPreview {
            workspace_count: value
                .get("workspaceCount")
                .and_then(Value::as_u64)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Missing workspace count"))?,
            tab_count: value
                .get("tabCount")
                .and_then(Value::as_u64)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Missing tab count"))?,
            active_session_count: value
                .get("activeSessionCount")
                .and_then(Value::as_u64)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Missing session count"))?,
            has_config_override: value
                .get("hasConfigOverride")
                .and_then(Value::as_bool)
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Missing config flag"))?,
        })
    }

    /// Forget an inactive Classic project record only, after preview has
    /// been shown and repeated. Never remove project files or Git worktrees.
    pub fn remove_project_record(
        &self,
        project_id: &str,
        project_root: &Path,
        preview: &ProjectRemovalPreview,
    ) -> io::Result<()> {
        validate_catalog_identifier(project_id)?;
        if preview.active_session_count > 0 {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "Project has live Host sessions",
            ));
        }
        if let Ok(manager) = crate::worktree_manager::WorktreeManager::discover(project_root) {
            if manager.list()?.iter().any(|entry| !entry.is_primary) {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "Project has linked Git worktrees; remove these before forgetting the project",
                ));
            }
        }
        if &self.preview_project_removal(project_id)? != preview {
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                "Project impact changed; review removal preview again",
            ));
        }
        let mut command = windowless_command(&self.executable);
        command
            .arg("project")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("remove")
            .arg("--id")
            .arg(project_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, PROBE_TIMEOUT)?.success() {
            return Err(io::Error::other("Classic Host refused project removal"));
        }
        Ok(())
    }

    /// Fetch Classic's entire project config for a first-class shared editor.
    pub fn read_project_settings(&self, project_id: &str) -> io::Result<Value> {
        validate_catalog_identifier(project_id)?;
        let mut command = windowless_command(&self.executable);
        command
            .arg("project")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("config-show")
            .arg("--id")
            .arg(project_id)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Config stdout unavailable"))?;
        const LIMIT: u64 = 64 * 1024;
        let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout.take(LIMIT + 1).read_to_end(&mut bytes)?;
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
            .map_err(|_| io::Error::other("Config reader panicked"))??;
        if !status.success() {
            return Err(io::Error::other("Classic project settings unavailable"));
        }
        if bytes.len() as u64 > LIMIT {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Project config too large",
            ));
        }
        let response: Value = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        if response.get("kind").and_then(Value::as_str) != Some("projectConfig")
            || response.get("projectId").and_then(Value::as_str) != Some(project_id)
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Project config response mismatched",
            ));
        }
        response
            .get("config")
            .cloned()
            .filter(Value::is_object)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Project config JSON missing"))
    }

    pub fn write_project_settings(&self, project_id: &str, json: &str) -> io::Result<()> {
        validate_catalog_identifier(project_id)?;
        if json.is_empty() || json.len() > 32 * 1024 {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Invalid project config length",
            ));
        }
        let parsed: Value = serde_json::from_str(json)
            .map_err(|error| io::Error::new(ErrorKind::InvalidInput, error))?;
        if !parsed.is_object() {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Project settings must be a JSON object",
            ));
        }
        // The Alera CLI validates against the complete ProjectConfig type.
        // Avoid enabling RuntimeStore/SQLx in the standalone GPUI binary.
        let mut command = windowless_command(&self.executable);
        command
            .arg("project")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("config-set")
            .arg("--id")
            .arg(project_id)
            .arg("--config-json")
            .arg(json)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        if !wait_with_timeout(&mut child, PROBE_TIMEOUT)?.success() {
            return Err(io::Error::other("Classic project config update refused"));
        }
        Ok(())
    }

    /// Rename an existing RuntimeStore record. Neither Git path nor branch is changed.
    pub fn rename_catalog_record(&self, noun: &str, record_id: &str, name: &str) -> io::Result<()> {
        if !matches!(noun, "project" | "workspace") {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Invalid catalog record kind",
            ));
        }
        validate_catalog_identifier(record_id)?;
        validate_display_name(name)?;
        let mut command = windowless_command(&self.executable);
        command
            .arg(noun)
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("rename")
            .arg("--id")
            .arg(record_id)
            .arg("--name")
            .arg(name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let status = wait_with_timeout(&mut child, PROBE_TIMEOUT)?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "Classic {noun} rename was rejected"
            )));
        }
        Ok(())
    }

    /// Host-managed cleanup of a *registered* worktree, never a direct Git
    /// prune. A local clean check is mandatory because Host removal uses a
    /// force-capable Git API internally. Branches are always kept.
    pub fn remove_registered_worktree(
        &self,
        workspace_id: &str,
        project_root: &Path,
        target: &Path,
    ) -> io::Result<()> {
        validate_catalog_identifier(workspace_id)?;
        let manager = crate::worktree_manager::WorktreeManager::discover(project_root)?;
        // Require the exact managed checkout under primary/.worktrees, a clean
        // status, and no locked/stale checkout, without deleting anything yet.
        manager.preflight_remove(target)?;
        let mut command = windowless_command(&self.executable);
        command
            .arg("workspace")
            .arg("--json")
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("remove")
            .arg("--id")
            .arg(workspace_id)
            .arg("--keep-branch")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let status = wait_with_timeout(&mut child, Duration::from_secs(120))?;
        if !status.success() {
            return Err(io::Error::other(
                "Classic Host refused managed Worktree removal",
            ));
        }
        Ok(())
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

fn validate_catalog_identifier(id: &str) -> io::Result<()> {
    if id.is_empty()
        || id.starts_with('-')
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Invalid catalog record ID",
        ));
    }
    Ok(())
}

fn validate_display_name(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name.len() > 256
        || name.trim() != name
        || name.chars().any(char::is_control)
    {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Invalid catalog display name",
        ));
    }
    Ok(())
}

/// `create` reaches the CLI only in the exact two-argument form produced by
/// `terminal::create_terminal_verb`; every other shape is rejected before a
/// process is spawned.
fn is_create_terminal_args(args: &[OsString]) -> bool {
    const FLAG: &str = "--workspace-id=";
    args.len() == 2
        && args[1]
            .to_str()
            .is_some_and(|arg| arg.len() > FLAG.len() && arg.starts_with(FLAG))
}

fn is_resize_terminal_args(args: &[OsString]) -> bool {
    if args.len() != 4 {
        return false;
    }
    let Some(handle) = args[1].to_str().and_then(|s| s.strip_prefix("--handle=")) else {
        return false;
    };
    let Some(cols) = args[2].to_str().and_then(|s| s.strip_prefix("--cols=")) else {
        return false;
    };
    let Some(rows) = args[3].to_str().and_then(|s| s.strip_prefix("--rows=")) else {
        return false;
    };
    !handle.is_empty()
        && handle.len() <= 128
        && handle
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
        && cols.parse::<u16>().is_ok_and(|n| (20..=512).contains(&n))
        && rows.parse::<u16>().is_ok_and(|n| (5..=200).contains(&n))
}

pub(crate) fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
) -> io::Result<std::process::ExitStatus> {
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
    // Classic Flutter stores its active Host under the app support directory,
    // NOT at the standalone CLI fallback ~/.alera/runtime. Prefer that
    // existing Host when no explicit runtime was requested. GPUI still never
    // reads the Host's authentication token or starts a second Host.
    let home: Option<OsString> =
        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    let standalone_cli_dir = PathBuf::from(home.unwrap_or_else(|| OsString::from(".")))
        .join(".alera")
        .join("runtime");
    #[cfg(windows)]
    let classic_dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|appdata| {
            appdata
                .join("dev.leynier")
                .join("Alera")
                .join("terminal_host")
        });
    #[cfg(not(windows))]
    let classic_dir: Option<PathBuf> = None;
    choose_runtime_dir(
        std::env::var_os("ALERA_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        classic_dir,
        standalone_cli_dir,
    )
}

fn choose_runtime_dir(
    explicit: Option<PathBuf>,
    classic: Option<PathBuf>,
    standalone_cli: PathBuf,
) -> PathBuf {
    if let Some(path) = explicit {
        return path;
    }
    // Only inspect control-file *presence*. The authenticated CLI decides
    // whether the Host is live/compatible. Reading host.json in GPUI would
    // expose a credential that belongs exclusively to the Host and CLI.
    if let Some(path) = classic {
        if path.join("host.json").is_file() {
            return path;
        }
    }
    standalone_cli
}

/// Settings CLI shares a transport but different verbs have intentionally
/// different secret-free result contracts. Never interpret a pin-list array
/// or pin-set projection as a full DesktopSettings snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookSettingsSnapshot {
    pub revision: i64,
    pub switches: HashMap<String, bool>,
}

fn parse_versioned_agent_hook_settings(response: &Value) -> io::Result<AgentHookSettingsSnapshot> {
    let fields = response.as_object().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidData,
            "Host Agent Hooks reply is not an object",
        )
    })?;
    if fields.len() != 3
        || response.get("kind").and_then(Value::as_str) != Some("agentHookSettings")
    {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Unexpected Host Hook response",
        ));
    }
    let revision = response
        .get("revision")
        .and_then(Value::as_i64)
        .filter(|revision| *revision >= 0 && *revision < i64::MAX)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Host Hook revision missing"))?;
    let hooks = response
        .get("agentStatusHooks")
        .and_then(Value::as_object)
        .filter(|map| map.len() <= 64)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Invalid Host Hook switches"))?;
    let mut parsed = HashMap::with_capacity(hooks.len());
    for (agent, enabled) in hooks {
        if agent.is_empty()
            || agent.len() > 64
            || !agent.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            })
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Invalid Host Agent identity",
            ));
        }
        let enabled = enabled
            .as_bool()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Invalid Host Hook boolean"))?;
        parsed.insert(agent.clone(), enabled);
    }
    Ok(AgentHookSettingsSnapshot {
        revision,
        switches: parsed,
    })
}

fn parse_agent_hook_settings_snapshot(response: &Value) -> io::Result<HashMap<String, bool>> {
    Ok(parse_versioned_agent_hook_settings(response)?.switches)
}

fn validate_agent_hook_cas_readback(
    before: &AgentHookSettingsSnapshot,
    agent: &str,
    enabled: bool,
    response: &Value,
) -> io::Result<AgentHookSettingsSnapshot> {
    let after = parse_versioned_agent_hook_settings(response)?;
    let mut expected = before.switches.clone();
    if !expected.contains_key(agent) && expected.len() >= 64 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Too many Agent Hooks",
        ));
    }
    let is_noop = before.switches.get(agent).copied().unwrap_or(false) == enabled;
    if !is_noop {
        expected.insert(agent.to_owned(), enabled);
    }
    let next_revision = if is_noop {
        before.revision
    } else {
        before
            .revision
            .checked_add(1)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Host Hook revision overflow"))?
    };
    if after.revision != next_revision || after.switches != expected {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Host Agent Hook CAS readback mismatch",
        ));
    }
    Ok(after)
}

fn validate_desktop_settings_response(verb: Option<&str>, response: &Value) -> io::Result<()> {
    let valid = match verb {
        Some("agent-hooks-get") => parse_agent_hook_settings_snapshot(response).is_ok(),
        Some("agent-hooks-cas") => parse_versioned_agent_hook_settings(response).is_ok(),
        Some("pin-list") => response.is_array(),
        Some("pin-set") => response.is_object(),
        Some(verb) if verb.starts_with("favorites-") => {
            response["branches"].is_array() && response["scopeKey"].is_string()
        }
        _ => response["values"].is_object(),
    };
    if !valid {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Host returned invalid Settings/Tab pin response for requested verb",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_classic_host_before_stale_standalone_cli_runtime() {
        let root = tempfile::tempdir().unwrap();
        let classic = root
            .path()
            .join("dev.leynier")
            .join("Alera")
            .join("terminal_host");
        let standalone = root.path().join(".alera").join("runtime");
        std::fs::create_dir_all(&classic).unwrap();
        std::fs::create_dir_all(&standalone).unwrap();
        std::fs::write(standalone.join("runtime-host.json"), "{}").unwrap();
        assert_eq!(
            choose_runtime_dir(None, Some(classic.clone()), standalone.clone()),
            standalone,
            "missing Classic host.json keeps standalone CLI fallback",
        );
        std::fs::write(classic.join("host.json"), "{}").unwrap();
        assert_eq!(
            choose_runtime_dir(None, Some(classic.clone()), standalone),
            classic,
            "Classic Host takes precedence over stale standalone control files",
        );
    }

    #[test]
    fn explicit_runtime_dir_wins_over_classic_autodiscovery() {
        let root = tempfile::tempdir().unwrap();
        let explicit = root.path().join("custom");
        let classic = root.path().join("Classic").join("terminal_host");
        std::fs::create_dir_all(&classic).unwrap();
        std::fs::write(classic.join("host.json"), "{}").unwrap();
        assert_eq!(
            choose_runtime_dir(
                Some(explicit.clone()),
                Some(classic),
                root.path().join("standalone"),
            ),
            explicit
        );
    }

    #[test]
    fn desktop_settings_bridge_accepts_both_host_pin_reply_shapes() {
        let pin = serde_json::json!({
            "workspaceId": "ws-1",
            "id": "tab-1",
            "updatedAt": "2026-11-10T01:23:45.001Z",
            "pinned": true,
            "terminalSessionId": "host-pty-1"
        });
        let list = serde_json::json!([pin.clone()]);
        assert!(validate_desktop_settings_response(Some("pin-list"), &list).is_ok());
        assert!(validate_desktop_settings_response(Some("pin-set"), &pin).is_ok());
        assert!(validate_desktop_settings_response(Some("pin-list"), &pin).is_err());
        assert!(validate_desktop_settings_response(Some("pin-set"), &list).is_err());
        // The secret-free pin projection must never satisfy the existing
        // Settings snapshot contract, nor the Git favorites contract.
        assert!(validate_desktop_settings_response(Some("get"), &pin).is_err());
        assert!(validate_desktop_settings_response(Some("favorites-get"), &pin).is_err());
        assert!(validate_desktop_settings_response(
            Some("get"),
            &serde_json::json!({"values": {"editor.twoRowTabs": false}})
        )
        .is_ok());
        assert!(validate_desktop_settings_response(
            Some("favorites-get"),
            &serde_json::json!({"scopeKey": "project:1", "branches": []})
        )
        .is_ok());
        // The strict parsers still reject injected Agent secrets or an
        // incorrect workspace/terminal identity after shape dispatch.
        assert!(crate::host_tab_pins::parse_pin_snapshot(&list, "ws-other").is_err());
        let unsafe_reply = serde_json::json!([{
            "workspaceId": "ws-1",
            "id": "tab-1",
            "updatedAt": "2026-11-10T01:23:45.001Z",
            "pinned": true,
            "terminalSessionId": "host-pty-1",
            "initialCommand": "secret"
        }]);
        assert!(crate::host_tab_pins::parse_pin_snapshot(&unsafe_reply, "ws-1").is_err());
    }

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
    fn native_hook_settings_snapshot_is_strict_boolean_read_only_and_fail_closed() {
        let safe = serde_json::json!({
            "kind":"agentHookSettings",
            "revision": 7,
            "agentStatusHooks":{"codex":true,"claude":false,"grok":true}
        });
        let parsed = parse_agent_hook_settings_snapshot(&safe).unwrap();
        assert_eq!(parsed.get("codex"), Some(&true));
        assert_eq!(parsed.get("claude"), Some(&false));
        let versioned = parse_versioned_agent_hook_settings(&safe).unwrap();
        assert_eq!(versioned.revision, 7);
        assert_eq!(versioned.switches.get("codex"), Some(&true));
        assert!(!parsed.contains_key("unknown"));
        assert!(
            parse_agent_hook_settings_snapshot(&serde_json::json!({
                "kind":"agentHookSettings","revision":0,"agentStatusHooks":{}
            }))
            .unwrap()
            .is_empty(),
            "default Host Hook settings are all disabled"
        );
        assert!(validate_desktop_settings_response(Some("agent-hooks-get"), &safe).is_ok());
        for unsafe_reply in [
            serde_json::json!({"kind":"agentHookSettings","agentStatusHooks":{"codex":"true"}}),
            serde_json::json!({"kind":"agentHookSettings","agentStatusHooks":{"codex":true},"apiKey":"SECRET"}),
            serde_json::json!({"kind":"desktopSettings","agentStatusHooks":{"codex":true}}),
            serde_json::json!({"kind":"agentHookSettings","agentStatusHooks":{"BAD\nKEY":true}}),
        ] {
            assert!(
                parse_agent_hook_settings_snapshot(&unsafe_reply).is_err(),
                "unexpected Host response must not turn on Native Hook notifications"
            );
        }
    }

    #[test]
    fn agent_hook_cas_readback_cannot_confirm_stale_or_other_provider_changes() {
        let before = AgentHookSettingsSnapshot {
            revision: 17,
            switches: HashMap::from([("codex".into(), false), ("claude".into(), true)]),
        };
        let confirmed = serde_json::json!({
            "kind":"agentHookSettings","revision":18,
            "agentStatusHooks":{"codex":true,"claude":true}
        });
        assert!(validate_agent_hook_cas_readback(&before, "codex", true, &confirmed).is_ok());
        for bad in [
            serde_json::json!({"kind":"agentHookSettings","revision":17,"agentStatusHooks":{"codex":true,"claude":true}}),
            serde_json::json!({"kind":"agentHookSettings","revision":19,"agentStatusHooks":{"codex":true,"claude":true}}),
            serde_json::json!({"kind":"agentHookSettings","revision":18,"agentStatusHooks":{"codex":false,"claude":true}}),
            serde_json::json!({"kind":"agentHookSettings","revision":18,"agentStatusHooks":{"codex":true,"claude":false}}),
            serde_json::json!({"kind":"agentHookSettings","revision":18,"agentStatusHooks":{"codex":true,"claude":true},"secret":"NOPE"}),
        ] {
            assert!(validate_agent_hook_cas_readback(&before, "codex", true, &bad).is_err());
        }
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

    #[test]
    fn terminal_bridge_rejects_unapproved_verbs_or_wrong_input_channel() {
        let dir = tempfile::tempdir().unwrap();
        let bridge = RuntimeCliBridge::new(dir.path().join("missing-cli"), dir.path());
        // A bare 'workspaces' is a valid read-only Host query. Its rejection
        // for missing Host control files is verified in the separate test.
        for verb in ["attach", "create", "prune", "write"] {
            let err = bridge.terminal_json(&[OsString::from(verb)]).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        }
        for verb in ["list", "read"] {
            let err = bridge
                .terminal_json_with_input(&[OsString::from(verb)], b"private input")
                .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        }
    }

    #[test]
    fn terminal_bridge_allows_only_exact_workspace_and_create_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let bridge = RuntimeCliBridge::new(dir.path().join("missing-cli"), dir.path());
        // Malformed create/workspaces shapes never reach the missing-CLI
        // spawn path: they are denied by the allowlist itself.
        let malformed: &[&[&str]] = &[
            &["create"],
            &["create", "--workspace-id="],
            &["create", "--workspace-id", "ws-1"],
            &["create", "--workspace-id=ws-1", "extra"],
            &["workspaces", "extra"],
        ];
        for args in malformed {
            let args: Vec<OsString> = args.iter().map(|arg| OsString::from(*arg)).collect();
            let err = bridge.terminal_json(&args).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        }
        // Neither new verb accepts a stdin payload.
        for args in [
            vec![OsString::from("workspaces")],
            vec![
                OsString::from("create"),
                OsString::from("--workspace-id=ws-1"),
            ],
        ] {
            let err = bridge
                .terminal_json_with_input(&args, b"private input")
                .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        }
        // Well-formed verbs pass the allowlist and stop at the missing Host
        // control file, proving no process was spawned.
        for args in [
            vec![OsString::from("workspaces")],
            vec![
                OsString::from("create"),
                OsString::from("--workspace-id=ws-1"),
            ],
        ] {
            let err = bridge.terminal_json(&args).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::NotConnected);
        }
        assert!(!dir.path().join("host.json").exists());
        assert!(!dir.path().join("runtime-host.json").exists());
    }
}
