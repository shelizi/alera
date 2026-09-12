use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use alera_core::runtime::{
    NewOrchestrationTask, OrchestrationDispatchStatus, OrchestrationTaskStatus,
};
use serde_json::json;

use super::actor_test_harness::{local_client, test_actor};
use super::deferred_admission::DeferredAdmission;
use super::runtime_mutations::RuntimeMutationRequest;
use super::{ServerActor, ServerCommand};
use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;
use crate::terminal_host::session::Session;

struct CoordinatorFixture {
    actor: ServerActor,
    responses: tokio::sync::mpsc::UnboundedReceiver<crate::terminal_host::client::ClientFrame>,
    commands: tokio::sync::mpsc::UnboundedReceiver<ServerCommand>,
    run_id: String,
    _dir: tempfile::TempDir,
}

impl CoordinatorFixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (handle, responses) = ClientHandle::test_channels();
        let mut actor = test_actor(
            &dir,
            HashMap::from([(1, local_client(handle))]),
            HashMap::new(),
        )
        .await;
        actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(
            usize::MAX,
            usize::MAX,
            0,
        ));
        let (inbox, commands) = tokio::sync::mpsc::unbounded_channel();
        actor.inbox = inbox;

        actor
            .runtime_store
            .create_orchestration_task(NewOrchestrationTask {
                spec: "do the work".to_string(),
                task_title: None,
                display_name: None,
                deps: Vec::new(),
                parent_id: None,
                created_by_terminal_handle: Some("coord".to_string()),
                run_id: None,
                workspace_id: "workspace-1".to_string(),
                coordinator_handle: "coord".to_string(),
                result_schema: None,
            })
            .await
            .unwrap();
        let mut worker = Session::driver_test_stub("worker-1", 80, 24);
        worker.workspace_id = "workspace-1".to_string();
        actor.sessions.insert("worker-1".to_string(), worker);
        actor
            .agent_presence
            .update("worker-1", "codex".to_string(), AgentPresenceState::Done);

        let run = actor
            .orchestration_run(&json!({
                "spec": "coordinate",
                "from": "coord",
                "workspace": "workspace-1",
                "agent": "codex",
                "pollIntervalMs": 600_000,
            }))
            .await
            .unwrap();
        let run_id = run["runId"].as_str().unwrap().to_string();
        Self {
            actor,
            responses,
            commands,
            run_id,
            _dir: dir,
        }
    }

    async fn tick(&mut self) {
        self.actor
            .handle(ServerCommand::CoordinatorTick {
                run_id: self.run_id.clone(),
            })
            .await;
    }

    async fn status_is_answered(&mut self) {
        self.actor
            .handle_line(
                1,
                json!({"id": 7, "type": "status.get", "payload": {}}).to_string(),
            )
            .await;
        let status = tokio::time::timeout(Duration::from_secs(1), self.responses.recv())
            .await
            .expect("status.get must answer while the drift probe is parked")
            .unwrap()
            .as_json()
            .unwrap();
        assert_eq!(status["id"], 7);
        assert_eq!(status["ok"], true);
    }

    async fn release_probe(&mut self) {
        self.actor.deferred_admission.add_test_permits(1);
        let command = tokio::time::timeout(Duration::from_secs(1), self.commands.recv())
            .await
            .expect("the drift probe should report completion off the actor")
            .unwrap();
        assert!(
            matches!(command, ServerCommand::CoordinatorDriftProbed { .. }),
            "expected a drift probe completion command"
        );
        self.actor.handle(command).await;
    }

    async fn active_dispatch(&self) -> Option<OrchestrationDispatchStatus> {
        self.actor
            .runtime_store
            .active_orchestration_dispatch_for_handle("worker-1")
            .await
            .unwrap()
            .map(|dispatch| dispatch.status)
    }
}

#[tokio::test]
async fn coordinator_drift_probe_defers_dispatch_without_stalling_the_mailbox() {
    let mut fixture = CoordinatorFixture::new().await;

    fixture.tick().await;
    assert_eq!(
        fixture.active_dispatch().await,
        None,
        "the dispatch round must park behind the off-actor drift probe"
    );
    fixture.status_is_answered().await;

    fixture.release_probe().await;
    assert_eq!(
        fixture.active_dispatch().await,
        Some(OrchestrationDispatchStatus::AwaitingAcceptance),
        "the dispatch round resumes once the probe completion lands"
    );
}

#[tokio::test]
async fn coordinator_drift_probe_completion_is_dropped_when_the_run_stopped() {
    let mut fixture = CoordinatorFixture::new().await;

    fixture.tick().await;
    assert_eq!(fixture.active_dispatch().await, None);

    fixture.actor.coordinators.remove(&fixture.run_id);
    fixture.release_probe().await;

    assert_eq!(fixture.active_dispatch().await, None);
    let tasks = fixture
        .actor
        .runtime_store
        .list_scoped_orchestration_tasks(None, Some(&fixture.run_id), None)
        .await
        .unwrap();
    assert!(tasks
        .iter()
        .all(|task| task.status == OrchestrationTaskStatus::Ready));
}

#[tokio::test]
async fn coordinator_drift_probe_completion_yields_to_runtime_mutations() {
    let mut fixture = CoordinatorFixture::new().await;

    fixture.tick().await;
    assert_eq!(fixture.active_dispatch().await, None);

    fixture.actor.park_runtime_mutations();
    fixture.actor.start_runtime_mutation(
        1,
        9,
        RuntimeMutationRequest::RemoveWorkspace {
            workspace_id: "workspace-1".to_string(),
            cascade_tabs: true,
        },
    );
    fixture.release_probe().await;

    assert_eq!(
        fixture.active_dispatch().await,
        None,
        "a probe landing mid-mutation must not interleave dispatch writes"
    );
    let tasks = fixture
        .actor
        .runtime_store
        .list_scoped_orchestration_tasks(None, Some(&fixture.run_id), None)
        .await
        .unwrap();
    assert!(tasks
        .iter()
        .all(|task| task.status == OrchestrationTaskStatus::Ready));
}
