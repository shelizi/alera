use alera_core::runtime::{AutomationActor, RuntimeStore, WorkspaceTabRecord};
use serde_json::Value;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{int_or, require_object, TerminalHostLaunch};
use crate::terminal_host::session::Session;

use super::requests::require_string;
use super::workspace_tab_requests::WorkspaceTabStoreHandler;
use super::ServerActor;

struct TerminalAttachPersistence<'a> {
    runtime_store: &'a RuntimeStore,
    tabs: WorkspaceTabStoreHandler<'a>,
}

impl<'a> TerminalAttachPersistence<'a> {
    const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self {
            runtime_store,
            tabs: WorkspaceTabStoreHandler::new(runtime_store),
        }
    }

    async fn find_tab(&self, tab_id: &str) -> HostResult<Option<WorkspaceTabRecord>> {
        self.tabs.find(tab_id).await
    }

    async fn touch_tab(&self, mut tab: WorkspaceTabRecord) -> HostResult<WorkspaceTabRecord> {
        tab.updated_at = chrono::Utc::now();
        self.tabs.upsert(tab).await
    }

    async fn mark_run_taken_over(&self, run_id: &str, actor: AutomationActor) -> HostResult<()> {
        self.runtime_store
            .mark_automation_run_taken_over(run_id, actor)
            .await
            .map(|_| ())
            .map_err(|error| HostError::state(error.to_string()))
    }
}

impl ServerActor {
    pub(super) async fn create_or_attach(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        let session_id = require_string(payload, "sessionId")?;
        let workspace_id = require_string(payload, "workspaceId")?;
        let tab_id = require_string(payload, "tabId")?;
        let working_directory = require_string(payload, "workingDirectory")?;
        let resume_cursor = payload.get("resumeCursor").and_then(Value::as_u64);

        // Attaching a user client to a tab created for an automation is the
        // durable takeover signal. It prevents a later successful completion
        // from deleting a tab the user has started using.
        let persistence = TerminalAttachPersistence::new(&self.runtime_store);
        if let Ok(Some(tab)) = persistence.find_tab(&tab_id).await {
            if tab
                .payload
                .get("automationOwned")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let run_id = tab
                    .payload
                    .get("automationRunId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let _ = persistence.touch_tab(tab).await;
                if let Some(run_id) = run_id.as_deref() {
                    let (mobile, local_role, id, human_client) = self
                        .clients
                        .get(&client_id)
                        .map(|client| {
                            (
                                client.kind == super::ClientKind::Mobile,
                                client.local_role,
                                client
                                    .mobile_device_id
                                    .clone()
                                    .or_else(|| Some(client_id.to_string())),
                                client.kind == super::ClientKind::Mobile
                                    || (client.kind == super::ClientKind::Local
                                        && client.local_role
                                            == super::client_delivery::LocalClientRole::App),
                            )
                        })
                        .unwrap_or((
                            false,
                            super::client_delivery::LocalClientRole::Cli,
                            None,
                            false,
                        ));
                    if human_client {
                        let actor =
                            super::automation_actor::actor_for_client(mobile, local_role, id);
                        // Only a desktop or authenticated mobile client counts
                        // as a user takeover. The automation CLI is a local
                        // client too, but it must not preserve its own cleanup
                        // target.
                        let _ = persistence.mark_run_taken_over(run_id, actor).await;
                    }
                }
            }
        }

        let max_bytes = self.config.scrollback_bytes as usize;
        let restore_bytes = self.config.restore_snapshot_bytes as usize;

        // Live session: attach only. Dead session: remint with the same handle so
        // ALERA_TERMINAL_HANDLE / orchestration dispatch targets stay valid.
        if self.sessions.contains_key(&session_id) {
            let running = self.sessions.get(&session_id).is_some_and(Session::running);
            if running {
                self.flush_all_output(&session_id);
                if let Some(cursor) = resume_cursor {
                    let retained = self
                        .sessions
                        .get_mut(&session_id)
                        .expect("just checked")
                        .attach_at_output_cursor(client_id, cursor);
                    if retained {
                        let data = {
                            let session = self.sessions.get(&session_id).expect("just checked");
                            let (base_cursor, _) = session.output_stream_range();
                            session.buffer.slice_from((cursor - base_cursor) as usize)
                        };
                        if data.is_empty()
                            || self.send_terminal_output(
                                &session_id,
                                client_id,
                                crate::terminal_host::client::ClientFrame::Output {
                                    session_id: session_id.clone(),
                                    data,
                                },
                            )
                        {
                            let session = self.sessions.get(&session_id).expect("just checked");
                            return Ok(session.delta_attachment_payload(true));
                        }

                        // The terminal lane is backpressured. Do not acknowledge
                        // a delta that never reached the client: reset the
                        // delivery cursor to the current end and send the
                        // scrollback through the control reply instead.
                        let session = self.sessions.get_mut(&session_id).expect("just checked");
                        session.attach(client_id);
                        return Ok(session.attachment_payload(false, restore_bytes));
                    }
                }
                let session = self.sessions.get_mut(&session_id).expect("just checked");
                session.attach(client_id);
                return Ok(session.attachment_payload(false, restore_bytes));
            }
        }
        let (initial_scrollback, initial_output_stream_bytes) = self
            .take_terminal_restart_state(&session_id, &workspace_id, &tab_id, max_bytes)
            .await;

        let launch = TerminalHostLaunch::from_json(&Value::Object(
            require_object(payload.get("launch"), "launch")?.clone(),
        ))?;
        let cols = int_or(payload, "cols", 80) as u16;
        let rows = int_or(payload, "rows", 24) as u16;
        self.start_new_terminal_session(
            session_id.clone(),
            workspace_id,
            tab_id,
            working_directory,
            launch,
            cols,
            rows,
            initial_scrollback,
            initial_output_stream_bytes,
            None,
        )
        .await?;
        let session = self.sessions.get_mut(&session_id).expect("just inserted");
        session.attach(client_id);
        Ok(session.attachment_payload(true, restore_bytes))
    }

    pub(super) async fn restart_terminal(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        let session_id = require_string(payload, "sessionId")?;
        let workspace_id = require_string(payload, "workspaceId")?;
        let tab_id = require_string(payload, "tabId")?;
        let working_directory = require_string(payload, "workingDirectory")?;
        let launch = TerminalHostLaunch::from_json(&Value::Object(
            require_object(payload.get("launch"), "launch")?.clone(),
        ))?;
        let cols = int_or(payload, "cols", 80) as u16;
        let rows = int_or(payload, "rows", 24) as u16;

        if let Some(session) = self.sessions.get(&session_id) {
            if session.workspace_id != workspace_id || session.tab_id != tab_id {
                return Err(HostError::state(
                    "Terminal restart metadata does not match the live session.",
                ));
            }
        }

        let attached_clients = self
            .sessions
            .get(&session_id)
            .map(|session| session.clients.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        self.queue_terminal_exit_push(&session_id, None).await;
        self.cleanup_orchestration_for_closed_session(
            &session_id,
            "terminal was explicitly restarted",
        )
        .await;
        self.flush_all_output(&session_id);
        let max_bytes = self.config.scrollback_bytes as usize;
        let restore_bytes = self.config.restore_snapshot_bytes as usize;
        let (initial_scrollback, initial_output_stream_bytes) = self
            .take_terminal_restart_state(&session_id, &workspace_id, &tab_id, max_bytes)
            .await;
        self.start_new_terminal_session(
            session_id.clone(),
            workspace_id,
            tab_id,
            working_directory,
            launch,
            cols,
            rows,
            initial_scrollback,
            initial_output_stream_bytes,
            None,
        )
        .await?;

        let resync_clients = attached_clients
            .into_iter()
            .filter(|attached_client_id| {
                *attached_client_id != client_id && self.clients.contains_key(attached_client_id)
            })
            .collect::<Vec<_>>();
        let session = self.sessions.get_mut(&session_id).expect("just inserted");
        session.attach(client_id);
        for attached_client_id in &resync_clients {
            session.attach_for_resync(*attached_client_id);
        }
        let attachment = session.attachment_payload(true, restore_bytes);
        for attached_client_id in resync_clients {
            self.spawn_output_resync_timer(session_id.clone(), attached_client_id);
        }
        Ok(attachment)
    }
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{
        AutomationActor, AutomationActorKind, AutomationRun, RuntimeStore, WorkspaceTabRecord,
    };
    use chrono::{Duration, Utc};
    use serde_json::json;

    use super::TerminalAttachPersistence;

    fn automation_run(id: &str) -> AutomationRun {
        let now = Utc::now();
        serde_json::from_value(json!({
            "id": id,
            "automationId": "automation",
            "number": 1,
            "occurrenceKey": format!("manual|{id}"),
            "scheduledAt": now,
            "trigger": "manual",
            "actorKind": "managedAgent",
            "actorId": "profile",
            "status": "pending",
            "attemptCount": 0,
            "createdAt": now,
            "updatedAt": now,
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn automation_takeover_persistence_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let old_updated_at = Utc::now() - Duration::minutes(1);
        store
            .upsert_workspace_tab(WorkspaceTabRecord {
                id: "tab".into(),
                workspace_id: "workspace".into(),
                kind: "terminal".into(),
                title: "Automation".into(),
                created_at: old_updated_at,
                updated_at: old_updated_at,
                payload: json!({
                    "automationOwned": true,
                    "automationRunId": "run",
                    "keep": "value",
                }),
            })
            .await
            .unwrap();
        store
            .insert_automation_run(&automation_run("run"))
            .await
            .unwrap();
        let persistence = TerminalAttachPersistence::new(&store);

        let tab = persistence.find_tab("tab").await.unwrap().unwrap();
        let touched = persistence.touch_tab(tab).await.unwrap();
        assert!(touched.updated_at > old_updated_at);
        assert_eq!(touched.payload["keep"], "value");

        persistence
            .mark_run_taken_over(
                "run",
                AutomationActor {
                    kind: AutomationActorKind::HumanDesktop,
                    id: Some("desktop".into()),
                    label: None,
                },
            )
            .await
            .unwrap();
        assert!(
            store
                .find_automation_run("run")
                .await
                .unwrap()
                .unwrap()
                .taken_over
        );
    }
}
