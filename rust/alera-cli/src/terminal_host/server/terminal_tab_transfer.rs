//! Runtime-owned live PTY reparenting. A terminal tab transfer is *never* a
//! tab.upsert followed by layout.upsert: all three records must commit
//! together and only then may the actor mutate its in-memory Session owner.

use alera_core::runtime::WorkspaceTabRecord;
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::event;

use super::terminal_startup_commands::terminal_session_id;
use super::ServerActor;

fn string_field<'a>(payload: &'a Value, name: &str) -> HostResult<&'a str> {
    payload
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| HostError::format(format!("tab.transfer requires {name}.")))
}

impl ServerActor {
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
        Ok(json!(self.workspace_tab_for_client(client_id, moved)))
    }
}
