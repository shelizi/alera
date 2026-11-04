//! Scoped, in-memory Host → Flutter Workbench command bridge.
//!
//! An accepted request is NOT evidence of a visible UI operation. The
//! registered local App must send a request-specific `uiControl.complete`
//! before the originating MCP client can observe `applied`.
//!
//! The per-PTY capability is minted by the Host during shell creation and is
//! never stored in a tab record or accepted from a caller-supplied claim.
//! Recovered sessions with an older capability fail closed after Host restart.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use alera_core::runtime::WorkspaceStatus;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::event;
use crate::terminal_host::session::Session;

use super::client_delivery::LocalClientRole;
use super::{ClientKind, ServerActor};

const REQUEST_TTL: Duration = Duration::from_secs(12);
const RECEIPT_TTL: Duration = Duration::from_secs(60);
const MAX_PENDING_REQUESTS: usize = 32;

#[derive(Default)]
pub(super) struct McpUiControlState {
    windows: HashMap<u64, String>,
    /// Ephemeral capabilities are bound to live PTYs, never database rows.
    tokens: HashMap<String, String>,
    requests: HashMap<String, McpUiReceipt>,
}

struct McpUiReceipt {
    session_handle: String,
    target_workspace_id: String,
    expected_tab_id: Option<String>,
    target_client_id: u64,
    window_id: String,
    created: Instant,
    status: &'static str,
    tab_id: Option<String>,
    setting_path: Option<String>,
    setting_value: Option<Value>,
    error_code: Option<&'static str>,
}

impl McpUiReceipt {
    fn report(&mut self) -> Value {
        if self.status == "accepted" && self.created.elapsed() >= REQUEST_TTL {
            self.status = "failed";
            self.error_code = Some("ui_timeout");
        }
        json!({
            "status": self.status,
            "applied": self.status == "applied",
            "windowId": self.window_id,
            "workspaceId": self.target_workspace_id,
            "tabId": self.tab_id,
            "settingPath": self.setting_path,
            "settingValue": self.setting_value,
            "errorCode": self.error_code,
        })
    }
}

impl McpUiControlState {
    pub(super) fn mint_token(&mut self, handle: &str) -> String {
        // Two independent UUIDv4 values form a 64-character per-PTY proof.
        // Replacing the session invalidates the previous proof.
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        self.tokens.insert(handle.to_owned(), token.clone());
        token
    }

    pub(super) fn revoke(&mut self, handle: &str) {
        self.tokens.remove(handle);
        for request in self.requests.values_mut() {
            if request.session_handle == handle && request.status == "accepted" {
                request.status = "failed";
                request.error_code = Some("session_ended");
            }
        }
    }

    pub(super) fn disconnect(&mut self, client_id: u64) {
        self.windows.remove(&client_id);
        for receipt in self.requests.values_mut() {
            if receipt.target_client_id == client_id && receipt.status == "accepted" {
                receipt.status = "failed";
                receipt.error_code = Some("ui_disconnected");
            }
        }
    }

    fn compact(&mut self) {
        // Abandoned requests must release their queue slot at the 12-second
        // deadline even when their original MCP client never polls status.
        for receipt in self.requests.values_mut() {
            receipt.report();
        }
        self.requests
            .retain(|_, receipt| receipt.created.elapsed() < RECEIPT_TTL);
    }
}

impl ServerActor {
    pub(super) async fn handle_mcp_ui_request(
        &mut self,
        client_id: u64,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        self.mcp_ui.compact();
        match request_type {
            "uiControl.register" => {
                self.require_mcp_ui_app(client_id)?;
                let window_id = required_short(payload, "windowId", 128)?;
                if self
                    .mcp_ui
                    .windows
                    .iter()
                    .any(|(existing_id, existing_window)| {
                        *existing_id != client_id && existing_window == &window_id
                    })
                {
                    return Err(HostError::state(
                        "Workbench window ID is already registered.",
                    ));
                }
                self.mcp_ui.windows.insert(client_id, window_id.clone());
                Ok(json!({"registered": true, "windowId": window_id}))
            }
            "uiControl.unregister" => {
                self.require_mcp_ui_app(client_id)?;
                let window_id = required_short(payload, "windowId", 128)?;
                if self.mcp_ui.windows.get(&client_id) != Some(&window_id) {
                    return Err(HostError::unauthorized(
                        "Only the registered Workbench window may unregister itself.",
                    ));
                }
                self.mcp_ui.disconnect(client_id);
                Ok(json!({"registered": false}))
            }
            "uiControl.windows" => {
                let session = self.require_mcp_ui_session(payload)?;
                let windows = self.mcp_ui.windows.values().cloned().collect::<Vec<_>>();
                Ok(json!({"workspaceId": session.workspace_id, "windows": windows}))
            }
            "uiControl.request" => {
                let session = self.require_mcp_ui_session(payload)?;
                let session_handle = session.id.clone();
                let workspace_id = session.workspace_id.clone();
                if self
                    .mcp_ui
                    .requests
                    .values()
                    .filter(|value| value.status == "accepted")
                    .count()
                    >= MAX_PENDING_REQUESTS
                {
                    return Err(HostError::state("Workbench UI control queue is full."));
                }
                let action = required_short(payload, "action", 40)?;
                let mut arguments = validate_ui_arguments(&action, payload.get("arguments"))?;
                let target_workspace_id = self
                    .validate_navigation_target(&action, &arguments, &workspace_id)
                    .await?;
                let expected_tab_id = if action == "select_tab" {
                    Some(required_short(&arguments, "tabId", 128)?)
                } else {
                    None
                };
                let setting_path = if matches!(action.as_str(), "get_setting" | "update_setting") {
                    Some(required_short(&arguments, "path", 128)?)
                } else {
                    None
                };
                if action == "focus_agent" {
                    // Resolve from the live Host, not an MCP-supplied Tab ID.
                    // A caller may only focus a running Agent in its own
                    // workspace, never guess a foreign Tab or stale PTY.
                    let handle = required_short(&arguments, "handle", 128)?;
                    let target = self
                        .sessions
                        .get(&handle)
                        .filter(|target| {
                            target.running()
                                && target.workspace_id == workspace_id
                                && self.agent_presence.get(&handle).is_some()
                        })
                        .ok_or_else(|| {
                            HostError::unauthorized(
                                "Agent is not running in the authorized workspace.",
                            )
                        })?;
                    arguments["tabId"] = json!(target.tab_id);
                }
                let requested_window = optional_short(payload, "windowId", 128)?;
                let (target_client_id, window_id) = match requested_window {
                    Some(target) => self
                        .mcp_ui
                        .windows
                        .iter()
                        .find(|(_, id)| *id == &target)
                        .map(|(client_id, id)| (*client_id, id.clone()))
                        .ok_or_else(|| {
                            HostError::state("Requested Workbench window is not connected.")
                        })?,
                    None if self.mcp_ui.windows.len() == 1 => {
                        let (client_id, id) = self.mcp_ui.windows.iter().next().unwrap();
                        (*client_id, id.clone())
                    }
                    None => {
                        return Err(HostError::state(
                            "No unique Workbench window is connected. Specify windowId.",
                        ));
                    }
                };
                let request_id = Uuid::new_v4().to_string();
                let command = event(
                    "mcpUiCommand",
                    json!({
                        "requestId": request_id,
                        "windowId": window_id,
                        "workspaceId": target_workspace_id,
                        "action": action,
                        "arguments": arguments,
                    }),
                );
                if !self.try_client_write(target_client_id, command) {
                    return Err(HostError::state(
                        "Workbench UI command could not be delivered.",
                    ));
                }
                self.mcp_ui.requests.insert(
                    request_id.clone(),
                    McpUiReceipt {
                        session_handle,
                        target_workspace_id,
                        expected_tab_id,
                        target_client_id,
                        window_id,
                        created: Instant::now(),
                        status: "accepted",
                        tab_id: None,
                        setting_path,
                        setting_value: None,
                        error_code: None,
                    },
                );
                Ok(json!({"requestId": request_id, "status": "accepted", "applied": false}))
            }
            "uiControl.status" => {
                let session = self.require_mcp_ui_session(payload)?;
                let handle = session.id.clone();
                let request_id = required_short(payload, "requestId", 128)?;
                let receipt = self
                    .mcp_ui
                    .requests
                    .get_mut(&request_id)
                    .filter(|receipt| receipt.session_handle == handle)
                    .ok_or_else(|| {
                        HostError::state("UI control request does not exist in this session.")
                    })?;
                Ok(receipt.report())
            }
            "uiControl.complete" => {
                self.require_mcp_ui_app(client_id)?;
                let request_id = required_short(payload, "requestId", 128)?;
                let applied = payload
                    .get("applied")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| HostError::format("applied must be boolean."))?;
                let tab_id = optional_short(payload, "tabId", 128)?;
                let error_code = optional_short(payload, "errorCode", 60)?;
                let receipt = self
                    .mcp_ui
                    .requests
                    .get_mut(&request_id)
                    .filter(|receipt| receipt.target_client_id == client_id)
                    .ok_or_else(|| {
                        HostError::unauthorized("UI command is not assigned to this window.")
                    })?;
                if receipt.status != "accepted" || receipt.created.elapsed() >= REQUEST_TTL {
                    return Err(HostError::state("UI command is no longer pending."));
                }
                let setting_value = match receipt.setting_path.as_deref() {
                    Some(path) if applied => {
                        let value = payload.get("settingValue").ok_or_else(|| {
                            HostError::format("Settings response must include a typed value.")
                        })?;
                        if !mcp_setting_value_valid(path, value) {
                            return Err(HostError::format("Invalid MCP settings reply value."));
                        }
                        Some(value.clone())
                    }
                    _ => None,
                };
                if applied
                    && receipt.expected_tab_id.is_some()
                    && tab_id.as_deref() != receipt.expected_tab_id.as_deref()
                {
                    return Err(HostError::format(
                        "Selected tab does not match the authorized target.",
                    ));
                }
                receipt.status = if applied { "applied" } else { "failed" };
                receipt.tab_id = if applied { tab_id } else { None };
                receipt.setting_value = setting_value;
                receipt.error_code = if applied {
                    None
                } else {
                    Some(public_ui_error_code(error_code.as_deref()))
                };
                Ok(json!({"acknowledged": true, "status": receipt.status}))
            }
            _ => Err(HostError::state("Unknown Workbench UI operation.")),
        }
    }

    /// Distinguish the Host-proven caller PTY from a *navigation-only* target.
    /// A forged target ID never grants agent input, shell launch or a foreign
    /// capability. The destination must remain a live, nonarchived workspace
    /// attached to the explicitly named project before any Flutter event.
    async fn validate_navigation_target(
        &self,
        action: &str,
        args: &Value,
        caller_workspace_id: &str,
    ) -> HostResult<String> {
        let navigation = matches!(
            action,
            "open_file" | "reveal_file" | "focus_workspace" | "select_tab"
        );
        let target_workspace_id = if navigation {
            args.get("workspaceId")
                .and_then(Value::as_str)
                .unwrap_or(caller_workspace_id)
        } else {
            caller_workspace_id
        };
        let project_id = args.get("projectId").and_then(Value::as_str);
        if target_workspace_id != caller_workspace_id && project_id.is_none() {
            return Err(HostError::unauthorized(
                "Cross-workspace navigation requires an explicit projectId.",
            ));
        }
        if navigation && (target_workspace_id != caller_workspace_id || project_id.is_some()) {
            let project_id = project_id.ok_or_else(|| {
                HostError::unauthorized("Target project identity was not provided.")
            })?;
            let project_exists = self
                .runtime_store
                .find_project(project_id)
                .await
                .map_err(|error| HostError::state(error.to_string()))?
                .is_some();
            let workspace = self
                .runtime_store
                .find_workspace(target_workspace_id)
                .await
                .map_err(|error| HostError::state(error.to_string()))?;
            if !project_exists
                || !workspace.as_ref().is_some_and(|workspace| {
                    workspace.project_id == project_id
                        && workspace.status == WorkspaceStatus::Active
                        && workspace.archived_at.is_none()
                })
            {
                return Err(HostError::unauthorized(
                    "Navigation target is unavailable or does not belong to the project.",
                ));
            }
        }
        if action == "select_tab" {
            let tab_id = required_short(args, "tabId", 128)?;
            let tab = self
                .runtime_store
                .find_workspace_tab(&tab_id)
                .await
                .map_err(|error| HostError::state(error.to_string()))?;
            if !tab
                .as_ref()
                .is_some_and(|tab| tab.workspace_id == target_workspace_id)
            {
                return Err(HostError::unauthorized(
                    "The selected Tab is not in the authorized target Workspace.",
                ));
            }
        }
        Ok(target_workspace_id.to_owned())
    }

    fn require_mcp_ui_app(&self, client_id: u64) -> HostResult<()> {
        let client = self.clients.get(&client_id);
        if client.is_some_and(|client| {
            client.authenticated
                && client.kind == ClientKind::Local
                && client.local_role == LocalClientRole::App
        }) {
            return Ok(());
        }
        Err(HostError::unauthorized(
            "Only a local Alera App may acknowledge UI commands.",
        ))
    }

    pub(super) fn require_mcp_ui_session(&self, payload: &Value) -> HostResult<&Session> {
        let handle = required_short(payload, "sessionId", 128)?;
        let workspace_id = required_short(payload, "workspaceId", 128)?;
        let token = required_short(payload, "capability", 128)?;
        let session = self
            .sessions
            .get(&handle)
            .filter(|session| session.running() && session.workspace_id == workspace_id);
        let stored_token = self.mcp_ui.tokens.get(&handle);
        if stored_token.map(String::as_str) != Some(token.as_str()) || session.is_none() {
            return Err(HostError::unauthorized(
                "MCP UI control requires a live Host-minted Terminal capability.",
            ));
        }
        Ok(session.expect("session existence checked"))
    }
}

fn public_ui_error_code(code: Option<&str>) -> &'static str {
    match code {
        Some("setting_conflict") => "setting_conflict",
        Some("setting_invalid") => "setting_invalid",
        Some("invalid_path") => "invalid_path",
        Some("file_missing") => "file_missing",
        Some("tab_missing") => "tab_missing",
        Some("invalid_position") => "invalid_position",
        Some("invalid_editor") => "invalid_editor",
        Some("external_editor_unavailable") => "external_editor_unavailable",
        Some("external_editor_failed") => "external_editor_failed",
        Some("workspace_missing") => "workspace_missing",
        Some("git_compare_failed") => "git_compare_failed",
        Some("ui_not_ready") => "ui_not_ready",
        Some("not_visible") => "not_visible",
        _ => "ui_operation_failed",
    }
}

fn required_short(payload: &Value, key: &str, max_length: usize) -> HostResult<String> {
    optional_short(payload, key, max_length)?
        .ok_or_else(|| HostError::format(format!("{key} must be a non-empty string.")))
}

fn optional_short(payload: &Value, key: &str, max_length: usize) -> HostResult<Option<String>> {
    let Some(value) = payload.get(key) else {
        return Ok(None);
    };
    let text = value
        .as_str()
        .filter(|value| !value.is_empty() && value.len() <= max_length)
        .ok_or_else(|| HostError::format(format!("{key} is invalid.")))?;
    Ok(Some(text.to_owned()))
}

fn validate_relative_path(value: &str) -> HostResult<String> {
    if value.is_empty() || value.len() > 1024 || value.contains('\\') || value.contains(':') {
        return Err(HostError::format("Invalid workspace-relative file path."));
    }
    if value.split('/').any(|segment| {
        segment.is_empty() || segment == "." || segment == ".." || segment.contains('\0')
    }) {
        return Err(HostError::format("Unsafe workspace-relative file path."));
    }
    Ok(value.to_string())
}

fn validate_git_ref(value: &str) -> HostResult<String> {
    if value.is_empty()
        || value.len() > 128
        || value.starts_with('-')
        || value.contains("..")
        || value.contains("@{")
        || value.ends_with('.')
        || value.ends_with('/')
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/._-".contains(&c))
    {
        return Err(HostError::format("Invalid Git ref."));
    }
    Ok(value.to_string())
}

fn validate_ui_arguments(action: &str, raw: Option<&Value>) -> HostResult<Value> {
    let args = raw
        .and_then(Value::as_object)
        .ok_or_else(|| HostError::format("UI command arguments must be an object."))?;
    let allowed: &[&str] = match action {
        "open_file" => &[
            "relativePath",
            "workspaceId",
            "projectId",
            "openMode",
            "line",
            "column",
            "editor",
        ],
        "reveal_file" => &["relativePath", "workspaceId", "projectId"],
        "focus_workspace" => &["workspaceId", "projectId"],
        "select_tab" => &["workspaceId", "projectId", "tabId"],
        "focus_agent" => &["handle"],
        "get_setting" => &["path"],
        "update_setting" => &["path", "expectedValue", "value"],
        "open_settings" => &["sectionId"],
        "open_git_graph" => &[
            "filePath",
            "gitDiffRoot",
            "allBranches",
            "favoritesOnly",
            "selectedRef",
        ],
        "compare_branches" => &["baseRef", "headRef", "gitDiffRoot"],
        _ => return Err(HostError::format("Unsupported Workbench UI action.")),
    };
    if args.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(HostError::format("Unsupported Workbench UI argument."));
    }
    let mut validated = serde_json::Map::new();
    if matches!(
        action,
        "open_file" | "reveal_file" | "focus_workspace" | "select_tab"
    ) {
        for key in ["workspaceId", "projectId"] {
            if let Some(value) = optional_short(raw.unwrap(), key, 128)? {
                validated.insert(key.to_string(), json!(value));
            }
        }
        if matches!(action, "focus_workspace" | "select_tab")
            && !validated.contains_key("workspaceId")
        {
            return Err(HostError::format("Navigation workspaceId is required."));
        }
    }
    if action == "select_tab" {
        validated.insert(
            "tabId".into(),
            json!(required_short(raw.unwrap(), "tabId", 128)?),
        );
    }
    if action == "open_file" {
        let editor =
            optional_short(raw.unwrap(), "editor", 16)?.unwrap_or_else(|| "alera".to_owned());
        if !matches!(editor.as_str(), "alera" | "zed" | "vscode") {
            return Err(HostError::format("editor must be alera, zed or vscode."));
        }
        validated.insert("editor".into(), json!(editor));
        let mode =
            optional_short(raw.unwrap(), "openMode", 16)?.unwrap_or_else(|| "reuse".to_owned());
        if !matches!(mode.as_str(), "reuse" | "new") {
            return Err(HostError::format("openMode must be reuse or new."));
        }
        if editor != "alera" && mode != "reuse" {
            return Err(HostError::format(
                "openMode new is only supported for Alera's built-in editor.",
            ));
        }
        validated.insert("openMode".into(), json!(mode));
        for key in ["line", "column"] {
            if let Some(value) = args.get(key) {
                let position = value
                    .as_u64()
                    .filter(|position| (1..=1_000_000_000).contains(position))
                    .ok_or_else(|| {
                        HostError::format(format!("{key} must be a positive 1-based integer."))
                    })?;
                validated.insert(key.into(), json!(position));
            }
        }
        if validated.contains_key("column") && !validated.contains_key("line") {
            return Err(HostError::format("column requires a 1-based line."));
        }
    }
    if matches!(action, "open_file" | "reveal_file") {
        let path = required_short(raw.unwrap(), "relativePath", 1024)?;
        validated.insert("relativePath".into(), json!(validate_relative_path(&path)?));
    }
    if action == "focus_agent" {
        let handle = required_short(raw.unwrap(), "handle", 128)?;
        validated.insert("handle".into(), json!(handle));
    }
    if matches!(action, "get_setting" | "update_setting") {
        let path = required_short(raw.unwrap(), "path", 128)?;
        let writable = mcp_setting_writable(&path);
        if writable.is_none() || (action == "update_setting" && writable != Some(true)) {
            return Err(HostError::format("MCP setting path is not permitted."));
        }
        validated.insert("path".into(), json!(path));
        if action == "update_setting" {
            let value = args
                .get("value")
                .ok_or_else(|| HostError::format("MCP setting value is required."))?;
            let expected = args
                .get("expectedValue")
                .ok_or_else(|| HostError::format("MCP setting expectedValue is required."))?;
            if !mcp_setting_value_valid(&path, value) || !mcp_setting_value_valid(&path, expected) {
                return Err(HostError::format("MCP setting value is invalid."));
            }
            validated.insert("value".into(), value.clone());
            validated.insert("expectedValue".into(), expected.clone());
        }
    }
    if action == "open_settings" {
        let section = optional_short(raw.unwrap(), "sectionId", 64)?
            .unwrap_or_else(|| "application".to_string());
        if !matches!(
            section.as_str(),
            "application" | "terminal" | "editor" | "agents" | "agentProfiles"
        ) {
            return Err(HostError::format("Unknown Alera settings section."));
        }
        validated.insert("sectionId".into(), json!(section));
    }
    if action == "compare_branches" {
        for key in ["baseRef", "headRef"] {
            let git_ref = required_short(raw.unwrap(), key, 128)?;
            validated.insert(key.into(), json!(validate_git_ref(&git_ref)?));
        }
    }
    if action == "open_git_graph" {
        for key in ["allBranches", "favoritesOnly"] {
            if let Some(value) = args.get(key) {
                if !value.is_boolean() {
                    return Err(HostError::format(format!("{key} must be boolean.")));
                }
                validated.insert(key.to_owned(), value.clone());
            }
        }
        if let Some(value) = optional_short(raw.unwrap(), "selectedRef", 128)? {
            validated.insert("selectedRef".into(), json!(validate_git_ref(&value)?));
        }
        if let Some(value) = optional_short(raw.unwrap(), "filePath", 1024)? {
            validated.insert("filePath".into(), json!(validate_relative_path(&value)?));
        }
    }
    if let Some(value) = optional_short(raw.unwrap(), "gitDiffRoot", 1024)? {
        validated.insert("gitDiffRoot".into(), json!(validate_relative_path(&value)?));
    }
    Ok(Value::Object(validated))
}

/// Flat settings whitelist. No freeform nested objects, executable paths,
/// credentials, quota provider configs, or arbitrary storage operations.
fn mcp_setting_writable(path: &str) -> Option<bool> {
    match path {
        "general.autoGitFetchEnabled"
        | "general.autoGitFetchIntervalMinutes"
        | "terminal.fontSize"
        | "editor.autosaveEnabled"
        | "editor.tabSize" => Some(true),
        "general.confirmProjectRemoval"
        | "general.confirmWorkspaceRemoval"
        | "general.autoArchiveWorkspacesAfterDays" => Some(false),
        _ => None,
    }
}

fn mcp_setting_value_valid(path: &str, value: &Value) -> bool {
    match path {
        "general.autoGitFetchEnabled"
        | "editor.autosaveEnabled"
        | "general.confirmProjectRemoval"
        | "general.confirmWorkspaceRemoval" => value.is_boolean(),
        "general.autoGitFetchIntervalMinutes" => {
            value.as_u64().is_some_and(|v| (1..=60).contains(&v))
        }
        "editor.tabSize" => value.as_u64().is_some_and(|v| (1..=16).contains(&v)),
        "general.autoArchiveWorkspacesAfterDays" => value.as_u64().is_some_and(|v| v <= 36_500),
        "terminal.fontSize" => value
            .as_f64()
            .is_some_and(|v| v.is_finite() && (8.0..=48.0).contains(&v)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_host::client::ClientHandle;
    use crate::terminal_host::server::actor_test_harness::test_actor;
    use crate::terminal_host::server::ClientState;
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    #[test]
    fn validates_ui_command_paths_refs_and_types() {
        for editor in ["alera", "zed", "vscode"] {
            let valid = validate_ui_arguments(
                "open_file",
                Some(
                    &json!({"relativePath":"lib/main.dart","editor":editor,"line":120,"column":8}),
                ),
            )
            .expect("supported external editor and position should be valid");
            assert_eq!(valid["editor"], editor);
            assert_eq!(valid["column"], 8);
        }
        for invalid in [
            json!("cursor"),
            json!("shell"),
            json!("Zed"),
            json!(""),
            json!(true),
            json!(null),
            json!("/tmp/cmd"),
        ] {
            assert!(
                validate_ui_arguments(
                    "open_file",
                    Some(&json!({"relativePath":"main.dart","editor":invalid}))
                )
                .is_err(),
                "unknown/editor executable input must be rejected: {invalid}"
            );
        }
        for invalid in [
            json!(0),
            json!(-2),
            json!(1.5),
            json!("8"),
            json!(true),
            json!(1_000_000_001_u64),
        ] {
            assert!(
                validate_ui_arguments(
                    "open_file",
                    Some(&json!({"relativePath":"main.dart","line":120,"column":invalid}))
                )
                .is_err(),
                "column must be a bounded positive integer: {invalid}"
            );
        }
        assert!(
            validate_ui_arguments(
                "open_file",
                Some(&json!({"relativePath":"main.dart","column":8}))
            )
            .is_err(),
            "column without line is ambiguous"
        );
        assert!(
            validate_ui_arguments(
                "open_file",
                Some(&json!({"relativePath":"main.dart","editor":"zed","openMode":"new"}))
            )
            .is_err(),
            "internal-only new Tab mode must not be silently applied to external editor"
        );
        for line in [1_u64, 42, 1_000_000_000] {
            let command = validate_ui_arguments(
                "open_file",
                Some(&json!({"relativePath":"src/main.rs","line":line})),
            )
            .expect("valid 1-based line should be accepted");
            assert_eq!(command["line"], line);
        }
        for invalid in [
            json!(0),
            json!(-1),
            json!(1.2),
            json!("42"),
            json!(true),
            json!(1_000_000_001_u64),
        ] {
            assert!(
                validate_ui_arguments(
                    "open_file",
                    Some(&json!({"relativePath":"src/main.rs","line":invalid}))
                )
                .is_err(),
                "invalid line must be rejected: {invalid}"
            );
        }
        for (action, args) in [
            (
                "focus_workspace",
                json!({"workspaceId":"target","projectId":"project-b"}),
            ),
            (
                "select_tab",
                json!({"workspaceId":"target","projectId":"project-b","tabId":"tab-b"}),
            ),
            (
                "open_file",
                json!({"workspaceId":"target","projectId":"project-b","relativePath":"src/main.rs","openMode":"new"}),
            ),
            (
                "reveal_file",
                json!({"workspaceId":"target","projectId":"project-b","relativePath":"src/main.rs"}),
            ),
        ] {
            assert!(
                validate_ui_arguments(action, Some(&args)).is_ok(),
                "{action}: {args}"
            );
        }
        assert!(validate_ui_arguments(
            "open_file",
            Some(&json!({"relativePath":"a.rs","openMode":"execute"}))
        )
        .is_err());
        assert!(validate_ui_arguments(
            "select_tab",
            Some(&json!({"workspaceId":"other","tabId":"tab","stdin":"echo unsafe"}))
        )
        .is_err());
        assert!(
            validate_ui_arguments("open_file", Some(&json!({"relativePath":"src/main.rs"})))
                .is_ok()
        );
        assert!(
            validate_ui_arguments("reveal_file", Some(&json!({"relativePath":"../secret"})))
                .is_err()
        );
        assert!(
            validate_ui_arguments("open_file", Some(&json!({"relativePath":"C:/secret"}))).is_err()
        );
        assert!(validate_ui_arguments(
            "compare_branches",
            Some(&json!({"baseRef":"main","headRef":"feature/one"}))
        )
        .is_ok());
        assert!(validate_ui_arguments(
            "compare_branches",
            Some(&json!({"baseRef":"--exec","headRef":"main"}))
        )
        .is_err());
        assert!(
            validate_ui_arguments("open_git_graph", Some(&json!({"allBranches":"yes"}))).is_err()
        );
        assert!(validate_ui_arguments("untrusted", Some(&json!({}))).is_err());
        assert!(validate_ui_arguments(
            "open_file",
            Some(&json!({"relativePath":"src/main.rs","shell":"cmd"}))
        )
        .is_err());
        assert!(validate_ui_arguments(
            "get_setting",
            Some(&json!({
                "path":"terminal.fontSize",
            }))
        )
        .is_ok());
        assert!(validate_ui_arguments(
            "update_setting",
            Some(&json!({
                "path":"terminal.fontSize", "value":16, "expectedValue":13,
            }))
        )
        .is_ok());
        assert!(validate_ui_arguments(
            "update_setting",
            Some(&json!({
                "path":"terminal.fontSize", "value":100, "expectedValue":13,
            }))
        )
        .is_err());
        assert!(validate_ui_arguments(
            "update_setting",
            Some(&json!({
                "path":"general.autoGitFetchIntervalMinutes", "value":0, "expectedValue":5,
            }))
        )
        .is_err());
        assert!(validate_ui_arguments(
            "update_setting",
            Some(&json!({
                "path":"general.confirmProjectRemoval", "value":false,"expectedValue":true,
            }))
        )
        .is_err());
        assert!(validate_ui_arguments(
            "get_setting",
            Some(&json!({
                "path":"agents.quotas",
            }))
        )
        .is_err());
        assert!(validate_ui_arguments(
            "update_setting",
            Some(&json!({
                "path":"terminal.fontSize","value":14,"expectedValue":13,"danger":"shell",
            }))
        )
        .is_err());
        assert!(validate_ui_arguments(
            "open_settings",
            Some(&json!({
                "sectionId":"terminal",
            }))
        )
        .is_ok());
        assert!(validate_ui_arguments(
            "open_settings",
            Some(&json!({
                "sectionId":"secrets",
            }))
        )
        .is_err());
    }

    #[test]
    fn receipts_require_real_ui_ack_and_expire() {
        let mut receipt = McpUiReceipt {
            session_handle: "session".into(),
            target_workspace_id: "workspace".into(),
            expected_tab_id: None,
            target_client_id: 1,
            window_id: "main".into(),
            created: Instant::now(),
            status: "accepted",
            tab_id: None,
            setting_path: None,
            setting_value: None,
            error_code: None,
        };
        assert_eq!(receipt.report()["applied"], false);
        receipt.created = Instant::now() - REQUEST_TTL;
        assert_eq!(receipt.report()["errorCode"], "ui_timeout");
        assert_eq!(receipt.report()["applied"], false);
    }

    #[test]
    fn capability_tokens_rotate_and_disconnection_fails_pending() {
        let mut state = McpUiControlState::default();
        let first = state.mint_token("handle");
        assert_eq!(first.len(), 64);
        assert_ne!(first, state.mint_token("handle"));
        state.windows.insert(5, "window".into());
        state.requests.insert(
            "request".into(),
            McpUiReceipt {
                session_handle: "handle".into(),
                target_workspace_id: "workspace".into(),
                expected_tab_id: None,
                target_client_id: 5,
                window_id: "window".into(),
                created: Instant::now(),
                status: "accepted",
                tab_id: None,
                setting_path: None,
                setting_value: None,
                error_code: None,
            },
        );
        state.disconnect(5);
        assert_eq!(
            state.requests.get_mut("request").unwrap().report()["errorCode"],
            "ui_disconnected"
        );
        state.revoke("handle");
        assert!(!state.tokens.contains_key("handle"));
    }

    #[tokio::test]
    async fn host_accepts_only_proven_pty_and_reports_applied_only_after_correct_app_ack() {
        let dir = tempfile::tempdir().unwrap();
        let (app_events, mut app_output) = mpsc::unbounded_channel();
        let (app_pty, _) = mpsc::channel(4);
        let (cli_events, _cli_output) = mpsc::unbounded_channel();
        let (cli_pty, _) = mpsc::channel(4);
        let mut actor = test_actor(
            &dir,
            HashMap::from([
                (
                    1,
                    ClientState::local(ClientHandle::new(app_events, app_pty), true),
                ),
                (
                    2,
                    ClientState::local(ClientHandle::new(cli_events, cli_pty), false),
                ),
            ]),
            HashMap::from([(
                "handle".to_string(),
                Session::driver_test_stub("handle", 80, 24),
            )]),
        )
        .await;
        assert!(actor
            .handle_mcp_ui_request(2, "uiControl.register", &json!({"windowId":"main"}),)
            .await
            .is_err());
        actor
            .handle_mcp_ui_request(1, "uiControl.register", &json!({"windowId":"main"}))
            .await
            .unwrap();
        let token = actor.mcp_ui.mint_token("handle");
        let request = json!({
            "sessionId":"handle",
            "workspaceId":"workspace",
            "action":"reveal_file",
            "arguments":{"relativePath":"src/main.dart"},
            "capability": token,
        });
        let mut forged = request.clone();
        forged["capability"] = json!("not-host-minted");
        assert!(actor
            .handle_mcp_ui_request(2, "uiControl.request", &forged)
            .await
            .is_err());
        forged["capability"] = request["capability"].clone();
        forged["workspaceId"] = json!("foreign-workspace");
        assert!(actor
            .handle_mcp_ui_request(2, "uiControl.request", &forged)
            .await
            .is_err());
        let accepted = actor
            .handle_mcp_ui_request(2, "uiControl.request", &request)
            .await
            .unwrap();
        assert_eq!(accepted["status"], "accepted");
        assert_eq!(accepted["applied"], false);
        assert!(app_output.try_recv().is_ok());
        let request_id = accepted["requestId"].as_str().unwrap();
        let status = json!({
            "sessionId":"handle",
            "workspaceId":"workspace",
            "capability":token,
            "requestId":request_id,
        });
        let waiting = actor
            .handle_mcp_ui_request(2, "uiControl.status", &status)
            .await
            .unwrap();
        assert_eq!(waiting["applied"], false);
        assert_eq!(waiting["status"], "accepted");
        assert!(actor
            .handle_mcp_ui_request(
                2,
                "uiControl.complete",
                &json!({
                    "requestId":request_id, "applied":true
                })
            )
            .await
            .is_err());
        actor
            .handle_mcp_ui_request(
                1,
                "uiControl.complete",
                &json!({
                    "requestId":request_id, "applied":true, "tabId":"file-tab"
                }),
            )
            .await
            .unwrap();
        let applied = actor
            .handle_mcp_ui_request(2, "uiControl.status", &status)
            .await
            .unwrap();
        assert_eq!(applied["applied"], true);
        assert_eq!(applied["tabId"], "file-tab");
        assert!(actor
            .handle_mcp_ui_request(
                1,
                "uiControl.complete",
                &json!({
                    "requestId":request_id, "applied":true
                })
            )
            .await
            .is_err());
        actor.mcp_ui.revoke("handle");
        assert!(actor
            .handle_mcp_ui_request(2, "uiControl.status", &status)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn explicit_window_target_delivers_only_to_that_app_and_ambiguity_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let (first_events, mut first_output) = mpsc::unbounded_channel();
        let (first_pty, _) = mpsc::channel(4);
        let (second_events, mut second_output) = mpsc::unbounded_channel();
        let (second_pty, _) = mpsc::channel(4);
        let (cli_events, _cli_output) = mpsc::unbounded_channel();
        let (cli_pty, _) = mpsc::channel(4);
        let mut actor = test_actor(
            &dir,
            HashMap::from([
                (
                    10,
                    ClientState::local(ClientHandle::new(first_events, first_pty), true),
                ),
                (
                    20,
                    ClientState::local(ClientHandle::new(second_events, second_pty), true),
                ),
                (
                    30,
                    ClientState::local(ClientHandle::new(cli_events, cli_pty), false),
                ),
            ]),
            HashMap::from([(
                "handle".to_string(),
                Session::driver_test_stub("handle", 80, 24),
            )]),
        )
        .await;
        let token = actor.mcp_ui.mint_token("handle");
        let mut request = json!({
            "sessionId":"handle",
            "workspaceId":"workspace",
            "capability": token,
            "action":"open_git_graph",
            "arguments":{},
        });
        assert!(actor
            .handle_mcp_ui_request(30, "uiControl.request", &request)
            .await
            .is_err());
        actor
            .handle_mcp_ui_request(10, "uiControl.register", &json!({"windowId":"first"}))
            .await
            .unwrap();
        actor
            .handle_mcp_ui_request(20, "uiControl.register", &json!({"windowId":"second"}))
            .await
            .unwrap();
        assert!(actor
            .handle_mcp_ui_request(30, "uiControl.request", &request)
            .await
            .is_err());
        request["windowId"] = json!("missing");
        assert!(actor
            .handle_mcp_ui_request(30, "uiControl.request", &request)
            .await
            .is_err());
        request["windowId"] = json!("second");
        let accepted = actor
            .handle_mcp_ui_request(30, "uiControl.request", &request)
            .await
            .unwrap();
        assert_eq!(accepted["applied"], false);
        assert!(
            first_output.try_recv().is_err(),
            "untargeted Flutter App received a command"
        );
        assert!(
            second_output.try_recv().is_ok(),
            "targeted Flutter App did not receive a command"
        );
        let id = accepted["requestId"].as_str().unwrap();
        assert!(
            actor
                .handle_mcp_ui_request(
                    10,
                    "uiControl.complete",
                    &json!({
                        "requestId":id, "applied":true
                    })
                )
                .await
                .is_err(),
            "wrong App must not ACK other window's command"
        );
        actor
            .handle_mcp_ui_request(
                20,
                "uiControl.complete",
                &json!({
                    "requestId":id, "applied":true, "tabId":"git-history"
                }),
            )
            .await
            .unwrap();
        let mut status = request.clone();
        status["requestId"] = json!(id);
        assert_eq!(
            actor
                .handle_mcp_ui_request(30, "uiControl.status", &status)
                .await
                .unwrap()["tabId"],
            "git-history"
        );
        assert!(
            actor
                .handle_mcp_ui_request(
                    10,
                    "uiControl.unregister",
                    &json!({
                        "windowId":"second"
                    })
                )
                .await
                .is_err(),
            "cannot unregister another window"
        );
    }
}
