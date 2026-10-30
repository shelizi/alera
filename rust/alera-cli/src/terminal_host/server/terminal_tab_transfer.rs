//! Runtime-owned live PTY reparenting. A terminal tab transfer is *never* a
//! tab.upsert followed by layout.upsert: all three records must commit
//! together and only then may the actor mutate its in-memory Session owner.

use std::sync::OnceLock;

use alera_core::runtime::WorkspaceTabRecord;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::event;

use super::terminal_startup_commands::terminal_session_id;
use super::ServerActor;

// A Session ID may be reminted after a crash; the numeric instance counter
// also resets after a process restart. Combine the per-process random epoch
// and the Session's per-PTY instance number for a conservative proof.
static TRANSFER_HOST_EPOCH: OnceLock<Uuid> = OnceLock::new();

pub(super) fn pty_instance_token(instance_id: u64) -> String {
    format!(
        "{}:{instance_id}",
        TRANSFER_HOST_EPOCH.get_or_init(Uuid::new_v4)
    )
}

/// Durable evidence of an *observed original generation's normal exit*.
/// A Host crash, missing Session, stale token or newer transfer receipt must
/// never be promoted into a confirmed exit. This evidence does not authorize
/// deleting the transferred Tab or claiming an acknowledged live move.
fn observed_transferred_pty_exit(
    tab: &WorkspaceTabRecord,
    session_id: &str,
    target_workspace_id: &str,
    mutation_id: &str,
) -> Option<i64> {
    let receipt = &tab.payload["lastTerminalTabTransfer"];
    let evidence = &tab.payload["lastTerminalTabExit"];
    if evidence["transferMutationId"].as_str() != Some(mutation_id)
        || evidence["sessionId"].as_str() != Some(session_id)
        || evidence["workspaceId"].as_str() != Some(target_workspace_id)
        || evidence["ptyInstanceToken"].as_str() != receipt["ptyInstanceToken"].as_str()
        || receipt["ptyInstanceToken"].as_str().is_none()
    {
        return None;
    }
    evidence["exitCode"].as_i64()
}

fn string_field<'a>(payload: &'a Value, name: &str) -> HostResult<&'a str> {
    payload
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| HostError::format(format!("tab.transfer requires {name}.")))
}

impl ServerActor {
    /// A stale view in another window must not terminate a PTY whose Tab has
    /// since moved. Unscoped legacy termination remains available only for
    /// sessions without a persisted cross-workspace transfer receipt.
    pub(super) async fn verify_terminal_termination_owner(
        &self,
        client_id: u64,
        session_id: &str,
        payload: &Value,
    ) -> HostResult<()> {
        let expected_workspace = payload.get("expectedWorkspaceId").and_then(Value::as_str);
        let expected_tab = payload.get("expectedTabId").and_then(Value::as_str);
        let expected_transfer_receipt = payload
            .get("expectedTransferMutationId")
            .and_then(Value::as_str);
        let expected_view_lease_id = payload.get("expectedViewLeaseId").and_then(Value::as_str);
        let Some(session) = self.sessions.get(session_id) else {
            return Ok(());
        };
        if expected_workspace.is_some_and(|value| value != session.workspace_id.as_str())
            || expected_tab.is_some_and(|value| value != session.tab_id.as_str())
        {
            return Err(HostError::state(
                "Terminal ownership changed while this window was closing it. Refresh the tab instead.",
            ));
        }
        // A background tab.remove may already have deleted the SQLite row
        // while its owning actor has not yet retired the live process. Fence
        // both sides of that gap, even if find_workspace_tab now returns None.
        if self.mutation_queue.is_removing_tab(&session.tab_id) {
            return Err(HostError::state(
                "Terminal removal is in progress. Wait for its result before terminating the PTY.",
            ));
        }
        if let Some(stored) = self
            .runtime_store
            .find_workspace_tab(&session.tab_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
        {
            // A -> B -> A gives the original view matching workspace/tab ids
            // again (ABA). Only the *latest* observed transfer receipt permits
            // terminating the live process. Legacy never-moved tabs have no
            // receipt on either side and continue to work.
            let last_receipt = stored.payload["lastTerminalTabTransfer"]["id"].as_str();
            if expected_transfer_receipt != last_receipt {
                return Err(HostError::state(
                    "Terminal transfer revision changed. Refresh this tab before closing its PTY.",
                ));
            }
            if stored.payload["lastTerminalTabTransfer"].is_object()
                && (expected_workspace != Some(session.workspace_id.as_str())
                    || expected_tab != Some(session.tab_id.as_str())
                    || stored.workspace_id != session.workspace_id)
            {
                return Err(HostError::state(
                    "Moved Terminal requires current Workspace and Tab identity before termination.",
                ));
            }
            if stored.payload["lastTerminalTabTransfer"].is_object()
                && session
                    .clients
                    .iter()
                    .any(|attached| *attached != client_id)
            {
                // A moved PTY may be displayed in another process or device.
                // Reject explicit termination rather than killing a terminal
                // that still has another attached viewer.
                return Err(HostError::state(
                    "Moved Terminal is attached in another window or device. Detach other views before terminating its PTY.",
                ));
            }
            if stored.payload["lastTerminalTabTransfer"].is_object() {
                if !expected_view_lease_id
                    .is_some_and(|view| session.is_only_attached_view(client_id, view))
                {
                    return Err(HostError::state(
                        "Transferred Terminal termination requires its requester's only live expectedViewLeaseId.",
                    ));
                }
                let actual_token = pty_instance_token(session.instance_id());
                if stored.payload["lastTerminalTabTransfer"]["ptyInstanceToken"].as_str()
                    != Some(actual_token.as_str())
                    || !session.running()
                    || stored.workspace_id != session.workspace_id
                {
                    return Err(HostError::state(
                        "Transferred Terminal PTY instance could not be verified. Resolve the original transfer before terminating.",
                    ));
                }
            }
        }
        Ok(())
    }

    /// An obsolete workspace view cannot attach to a moved PTY or resurrect
    /// its process in the old project after host recovery.
    pub(super) async fn verify_terminal_attachment_owner(
        &self,
        session_id: &str,
        workspace_id: &str,
        tab_id: &str,
    ) -> HostResult<()> {
        // Check before reading Store. A queued/active removal may already
        // have committed its row deletion but not retired the PTY, and a new
        // process must not attach during that actor handoff.
        if self.mutation_queue.is_removing_tab(tab_id) {
            return Err(HostError::state(
                "Terminal removal is in progress. Wait for the Host result before attaching another view.",
            ));
        }
        if let Some(session) = self.sessions.get(session_id) {
            if session.running()
                && (session.workspace_id != workspace_id || session.tab_id != tab_id)
            {
                return Err(HostError::state(
                    "Terminal moved to a different workspace. Refresh before attaching.",
                ));
            }
        }
        if let Some(stored) = self
            .runtime_store
            .find_workspace_tab(tab_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
        {
            if stored.payload["lastTerminalTabTransfer"].is_object()
                && (stored.workspace_id != workspace_id
                    || terminal_session_id(&stored) != session_id)
            {
                return Err(HostError::state(
                    "Transferred Terminal metadata is stale. Refresh workspace before restarting.",
                ));
            }
            if stored.payload["lastTerminalTabTransfer"].is_object() {
                let original = self.sessions.get(session_id).filter(|session| {
                    session.running()
                        && session.tab_id == tab_id
                        && session.workspace_id == workspace_id
                });
                let Some(original) = original else {
                    return Err(HostError::state(
                        "Transferred Terminal original PTY is unavailable. Automatic restart is blocked; keep the Tab and resolve its transfer before starting a new process.",
                    ));
                };
                let actual_token = pty_instance_token(original.instance_id());
                if stored.payload["lastTerminalTabTransfer"]["ptyInstanceToken"].as_str()
                    != Some(actual_token.as_str())
                {
                    return Err(HostError::state(
                        "Transferred Terminal live PTY instance does not match its persisted receipt. Refusing to attach a reminted process.",
                    ));
                }
            }
        }
        Ok(())
    }

    /// A read-only, authoritative lost-reply check. A persisted receipt alone
    /// is insufficient: the process may have exited or the host may have
    /// crashed between SQLite commit and the in-memory Session reparenting.
    /// Callers may only treat the transfer as acknowledged when both agree.
    pub(super) async fn terminal_tab_transfer_status(
        &self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        let tab_id = string_field(payload, "tabId")?;
        let session_id = string_field(payload, "sessionId")?;
        let source = string_field(payload, "sourceWorkspaceId")?;
        let target = string_field(payload, "targetWorkspaceId")?;
        let mutation_id = string_field(payload, "clientMutationId")?;
        let Some(tab) = self
            .runtime_store
            .find_workspace_tab(tab_id)
            .await
            .map_err(|err| HostError::state(err.to_string()))?
        else {
            return Ok(json!({"applied": false}));
        };
        let receipt = &tab.payload["lastTerminalTabTransfer"];
        if tab.kind != "terminal" {
            return Ok(json!({"applied": false}));
        }
        if receipt.is_object()
            && (receipt["id"].as_str() != Some(mutation_id)
                || receipt["from"].as_str() != Some(source)
                || receipt["to"].as_str() != Some(target))
        {
            // A newer transfer could have committed after the lost reply.
            // This is a conflict, not proof the original operation failed.
            return Ok(json!({
                "applied": false,
                "recoveryRequired": true,
                "reason": "receipt_conflict"
            }));
        }
        if receipt.is_object()
            && (tab.workspace_id != target || terminal_session_id(&tab) != session_id)
        {
            return Ok(json!({
                "applied": false,
                "recoveryRequired": true,
                "reason": "persisted_owner_inconsistent"
            }));
        }
        if !receipt.is_object() {
            // Missing receipt is still unknown after Host crashes; do not
            // claim definitive rejection from this read-only query.
            return Ok(json!({"applied": false}));
        }
        let Some(live) = self.sessions.get(session_id) else {
            if let Some(exit_code) =
                observed_transferred_pty_exit(&tab, session_id, target, mutation_id)
            {
                return Ok(json!({
                    "applied": false,
                    "recoveryRequired": true,
                    "reason": "pty_exit_observed",
                    "exitCode": exit_code
                }));
            }
            // The exact SQLite receipt exists but the Host no longer owns a
            // live process. After a crash (or exit) this is not a confirmed
            // success or a safe rollback. The client must keep its journal.
            return Ok(json!({
                "applied": false,
                "recoveryRequired": true,
                "reason": "pty_missing"
            }));
        };
        if !live.running() {
            if let Some(exit_code) =
                observed_transferred_pty_exit(&tab, session_id, target, mutation_id)
            {
                return Ok(json!({
                    "applied": false,
                    "recoveryRequired": true,
                    "reason": "pty_exit_observed",
                    "exitCode": exit_code
                }));
            }
        }
        if !live.running() || live.tab_id != tab_id || live.workspace_id != target {
            return Ok(json!({
                "applied": false,
                "recoveryRequired": true,
                "reason": "pty_owner_mismatch"
            }));
        }
        let current_pty_instance = pty_instance_token(live.instance_id());
        if receipt["ptyInstanceToken"].as_str() != Some(current_pty_instance.as_str()) {
            return Ok(json!({
                "applied": false,
                "recoveryRequired": true,
                "reason": "pty_instance_unverified"
            }));
        }
        Ok(json!({
            "applied": true,
            "tab": self.workspace_tab_for_client(client_id, tab)
        }))
    }

    pub(super) async fn transfer_terminal_tab(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        let tab_id = string_field(payload, "tabId")?;
        let source = string_field(payload, "sourceWorkspaceId")?;
        let target = string_field(payload, "targetWorkspaceId")?;
        let session_id = string_field(payload, "sessionId")?;
        let mutation_id = string_field(payload, "clientMutationId")?;
        // Workspace and project removal may have been queued before this
        // request. Their asynchronous file/PTY shutdown is not a safe window
        // in which to change the display owner of a running terminal.
        if self.mutation_queue.has_runtime_mutations()
            || self
                .mutation_queue
                .pending_workspace_shutdowns
                .contains_key(source)
            || self
                .mutation_queue
                .pending_workspace_shutdowns
                .contains_key(target)
        {
            return Err(HostError::state(
                "A workspace mutation or shutdown is still in progress. Retry the Terminal move after it completes.",
            ));
        }
        let source_layout = payload
            .get("sourceLayout")
            .filter(|value| value.is_object())
            .ok_or_else(|| HostError::format("tab.transfer requires sourceLayout."))?;
        let target_layout = payload
            .get("targetLayout")
            .filter(|value| value.is_object())
            .ok_or_else(|| HostError::format("tab.transfer requires targetLayout."))?;
        let expected_source = payload
            .get("expectedSourceLayout")
            .filter(|value| !value.is_null());
        let expected_target = payload
            .get("expectedTargetLayout")
            .filter(|value| !value.is_null());

        // The actor owns the process and session. A currently unattached
        // terminal is still live; a missing/dead Session cannot honestly
        // promise to preserve the existing process.
        let live = self
            .sessions
            .get(session_id)
            .ok_or_else(|| HostError::state("Terminal PTY is not running."))?;
        if !live.running()
            || live.tab_id != tab_id
            || (live.workspace_id != source && live.workspace_id != target)
        {
            return Err(HostError::state(
                "Terminal PTY identity or current workspace does not match.",
            ));
        }
        let current_pty_instance = pty_instance_token(live.instance_id());
        if !live.every_attached_client_has_view_lease() {
            // An old Host client could multiplex several unknown windows.
            // Never move its PTY until every attached connection supplies
            // explicit per-view identity.
            return Err(HostError::state(
                "All attached Terminal views must support viewLeaseId before this PTY may move.",
            ));
        }
        let launch_directory = live.working_directory.clone();
        if self.sessions.values().any(|session| {
            session.tab_id == tab_id && session.id != session_id && session.running()
        }) {
            return Err(HostError::state(
                "Terminal tab has multiple active PTYs; transfer cannot pick an owner safely.",
            ));
        }
        if self.terminal_pulses.rules.contains_key(session_id)
            || self
                .terminal_pulses
                .pending_configurations
                .contains_key(session_id)
        {
            // Pulses watch a specific repository. Moving their watcher with
            // a running process would cause input to be injected from a
            // different project; keep the entire transfer rejected.
            return Err(HostError::state(
                "Disable the terminal's repository pulse before moving it.",
            ));
        }

        let stored: WorkspaceTabRecord = self
            .runtime_store
            .find_workspace_tab(tab_id)
            .await
            .map_err(|err| HostError::state(err.to_string()))?
            .ok_or_else(|| HostError::state("Terminal tab no longer exists."))?;
        if stored.kind != "terminal" || terminal_session_id(&stored) != session_id {
            return Err(HostError::state(
                "Terminal tab and live Session do not match.",
            ));
        }
        let previous_receipt = &stored.payload["lastTerminalTabTransfer"];
        if stored.workspace_id == target
            && previous_receipt["id"].as_str() == Some(mutation_id)
            && previous_receipt["from"].as_str() == Some(source)
            && previous_receipt["to"].as_str() == Some(target)
            && previous_receipt["ptyInstanceToken"].as_str() != Some(current_pty_instance.as_str())
        {
            return Err(HostError::state(
                "Terminal transfer was persisted, but the original PTY instance cannot be verified after recovery.",
            ));
        }
        if stored
            .payload
            .get("automationOwned")
            .and_then(Value::as_bool)
            == Some(true)
        {
            // Run-owned terminals are tied to their automation's Workspace
            // until explicit takeover; moving one would confuse completion
            // and deletion of the original execution target.
            return Err(HostError::state(
                "Take ownership of this automation terminal before moving it.",
            ));
        }
        if stored.payload.get("pendingAgentPrompt").is_some() {
            return Err(HostError::state(
                "Finish the pending agent launch before moving this Terminal.",
            ));
        }
        if self
            .runtime_store
            .active_orchestration_dispatch_for_handle(session_id)
            .await
            .map_err(|err| HostError::state(err.to_string()))?
            .is_some()
        {
            return Err(HostError::state(
                "Active orchestration dispatches cannot change execution workspaces.",
            ));
        }

        let moved = self
            .runtime_store
            .transfer_terminal_tab_storage(
                tab_id,
                source,
                target,
                mutation_id,
                expected_source,
                expected_target,
                source_layout,
                target_layout,
                &launch_directory,
                &current_pty_instance,
            )
            .await
            .map_err(|err| HostError::state(err.to_string()))?;
        // In the actor, there is no intervening command between persistence
        // commit and this synchronous mutation. Process, PTY, output stream,
        // scrollback, input queue and original working directory stay intact.
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.workspace_id = target.to_owned();
        }
        // Scoped watchers must see *both* sides; the layout event is global.
        self.broadcast_workspace_tabs_changed(Some(source));
        self.broadcast_workspace_tabs_changed(Some(target));
        self.broadcast_authenticated(event("workbenchLayoutsChanged", json!({})));
        // The current Agent Presence payload derives workspaceId from the
        // live Session. Refetch once so other windows replace the old source
        // badge immediately, even before the next Claude/Codex hook event.
        if self.agent_presence.get(session_id).is_some() {
            self.broadcast_agent_presence_changed();
        }
        Ok(json!(self.workspace_tab_for_client(client_id, moved)))
    }
}
