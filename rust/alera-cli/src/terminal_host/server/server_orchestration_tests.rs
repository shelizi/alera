use super::server_test_support::account_push_for_test;
use super::*;

use crate::terminal_host::history_repository::TerminalHostCheckpoint;
use alera_core::runtime::{
    NewOrchestrationTask, OrchestrationDispatchStatus, OrchestrationTaskStatus,
};

#[tokio::test]
async fn run_stop_clears_persisted_run_without_in_memory_ticker() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path()).await.unwrap();
    let runtime_store = RuntimeStore::open(dir.path()).await.unwrap();
    let run = runtime_store
        .create_orchestration_coordinator_run("coordinate", Some("coord"), 1000)
        .await
        .unwrap();
    let (inbox, _rx) = mpsc::unbounded_channel();
    let mut actor = ServerActor {
        runtime_dir: dir.path().to_path_buf(),
        control_file_path: dir.path().join("runtime-host.json"),
        token: "token".to_string(),
        config: TerminalHostConfig::default(),
        history,
        runtime_store: runtime_store.clone(),
        automation_wake: Arc::new(Notify::new()),
        automations_active: false,
        sessions: HashMap::new(),
        ssh_bootstrap_jobs: HashMap::new(),
        project_clone_jobs: HashMap::new(),
        agent_title_jobs: HashMap::new(),
        managed_workspace_jobs: 0,
        mutation_queue: Default::default(),
        agent_quota_cache: None,
        configuration_transfers: Default::default(),
        account_push: account_push_for_test(&dir, &runtime_store).await,
        clients: HashMap::new(),
        mobile_prompt_file_uploads: HashMap::new(),
        mobile_prompt_image_uploads: HashMap::new(),
        agent_presence: AgentPresenceRegistry::default(),
        orchestration_waiters: MessageWaiterRegistry::default(),
        orchestration_delivery_in_flight: HashSet::new(),
        orchestration_delivery_backpressured: HashSet::new(),
        orchestration_activity_last_recorded: HashMap::new(),
        coordinators: HashMap::new(),
        pending_drift_probes: HashSet::new(),
        pending_dispatch_installs: HashMap::new(),
        resources: ResourceMonitorState::default(),
        terminal_pulses: Default::default(),
        codex: None,
        codex_starting: None,
        deferred_admission: Arc::new(deferred_admission::DeferredAdmission::default()),
        workspace_sidebar_snapshots: Default::default(),
        inbox,
        next_client_id: Arc::new(AtomicU64::new(1)),
        mobile_gateway: None,
        shutdown_gen: 0,
        disposed: false,
    };

    let response = actor
        .orchestration_run_stop(&json!({
            "id": run.id,
            "actor": "coord",
            "reason": "maintenance"
        }))
        .await
        .unwrap();
    assert_eq!(response["runId"], json!(run.id));
    assert_eq!(response["status"], json!("stopped"));
    assert!(actor
        .runtime_store
        .active_orchestration_coordinator_run()
        .await
        .unwrap()
        .is_none());
    let stopped = actor
        .runtime_store
        .orchestration_coordinator_run_by_id(&run.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stopped.stop_reason.as_deref(), Some("maintenance"));
}

#[tokio::test]
async fn terminal_exit_fails_active_orchestration_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path()).await.unwrap();
    let runtime_store = RuntimeStore::open(dir.path()).await.unwrap();
    let task = runtime_store
        .create_orchestration_task(NewOrchestrationTask {
            spec: "do work".to_string(),
            task_title: None,
            display_name: None,
            deps: Vec::new(),
            parent_id: None,
            created_by_terminal_handle: None,
            run_id: None,
            workspace_id: "workspace-1".to_string(),
            coordinator_handle: "coord".to_string(),
            result_schema: None,
        })
        .await
        .unwrap();
    let dispatch = runtime_store
        .create_orchestration_dispatch(&task.id, "term-1")
        .await
        .unwrap();
    let (inbox, _rx) = mpsc::unbounded_channel();
    let mut actor = ServerActor {
        runtime_dir: dir.path().to_path_buf(),
        control_file_path: dir.path().join("runtime-host.json"),
        token: "token".to_string(),
        config: TerminalHostConfig::default(),
        history,
        runtime_store: runtime_store.clone(),
        automation_wake: Arc::new(Notify::new()),
        automations_active: false,
        sessions: HashMap::new(),
        ssh_bootstrap_jobs: HashMap::new(),
        project_clone_jobs: HashMap::new(),
        agent_title_jobs: HashMap::new(),
        managed_workspace_jobs: 0,
        mutation_queue: Default::default(),
        agent_quota_cache: None,
        configuration_transfers: Default::default(),
        account_push: account_push_for_test(&dir, &runtime_store).await,
        clients: HashMap::new(),
        mobile_prompt_file_uploads: HashMap::new(),
        mobile_prompt_image_uploads: HashMap::new(),
        agent_presence: AgentPresenceRegistry::default(),
        orchestration_waiters: MessageWaiterRegistry::default(),
        orchestration_delivery_in_flight: HashSet::new(),
        orchestration_delivery_backpressured: HashSet::new(),
        orchestration_activity_last_recorded: HashMap::new(),
        coordinators: HashMap::new(),
        pending_drift_probes: HashSet::new(),
        pending_dispatch_installs: HashMap::new(),
        resources: ResourceMonitorState::default(),
        terminal_pulses: Default::default(),
        codex: None,
        codex_starting: None,
        deferred_admission: Arc::new(deferred_admission::DeferredAdmission::default()),
        workspace_sidebar_snapshots: Default::default(),
        inbox,
        next_client_id: Arc::new(AtomicU64::new(1)),
        mobile_gateway: None,
        shutdown_gen: 0,
        disposed: false,
    };

    actor.handle_session_exit("term-1".to_string(), 9).await;

    let updated_dispatch = actor
        .runtime_store
        .orchestration_dispatch_by_id(&dispatch.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_dispatch.status, OrchestrationDispatchStatus::Failed);
    assert_eq!(updated_dispatch.failure_count, 1);
    assert_eq!(
        updated_dispatch.last_failure.as_deref(),
        Some("terminal exited with code 9")
    );
    let updated_task = actor
        .runtime_store
        .orchestration_task_by_id(&task.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_task.status, OrchestrationTaskStatus::Ready);
}

#[tokio::test]
async fn host_dispose_fails_active_orchestration_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path()).await.unwrap();
    let runtime_store = RuntimeStore::open(dir.path()).await.unwrap();
    let task = runtime_store
        .create_orchestration_task(NewOrchestrationTask {
            spec: "do work".to_string(),
            task_title: None,
            display_name: None,
            deps: Vec::new(),
            parent_id: None,
            created_by_terminal_handle: None,
            run_id: None,
            workspace_id: "workspace-1".to_string(),
            coordinator_handle: "coord".to_string(),
            result_schema: None,
        })
        .await
        .unwrap();
    let dispatch = runtime_store
        .create_orchestration_dispatch(&task.id, "term-1")
        .await
        .unwrap();
    history.queue_checkpoint(
        TerminalHostCheckpoint {
            session_id: "term-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            tab_id: "tab-1".to_string(),
            working_directory: "/tmp".to_string(),
            running: false,
            exit_code: None,
            ended_at: None,
            output_stream_bytes: 0,
            updated_at: chrono::Utc::now(),
            buffer: Vec::new(),
        },
        1024,
    );
    let session = Session::restore_exited(
        "term-1".to_string(),
        "workspace-1".to_string(),
        "tab-1".to_string(),
        &history,
        1024,
    )
    .await
    .unwrap();
    let (inbox, _rx) = mpsc::unbounded_channel();
    let mut actor = ServerActor {
        runtime_dir: dir.path().to_path_buf(),
        control_file_path: dir.path().join("runtime-host.json"),
        token: "token".to_string(),
        config: TerminalHostConfig::default(),
        history,
        runtime_store: runtime_store.clone(),
        automation_wake: Arc::new(Notify::new()),
        automations_active: false,
        sessions: HashMap::from([("term-1".to_string(), session)]),
        ssh_bootstrap_jobs: HashMap::new(),
        project_clone_jobs: HashMap::new(),
        agent_title_jobs: HashMap::new(),
        managed_workspace_jobs: 0,
        mutation_queue: Default::default(),
        agent_quota_cache: None,
        configuration_transfers: Default::default(),
        account_push: account_push_for_test(&dir, &runtime_store).await,
        clients: HashMap::new(),
        mobile_prompt_file_uploads: HashMap::new(),
        mobile_prompt_image_uploads: HashMap::new(),
        agent_presence: AgentPresenceRegistry::default(),
        orchestration_waiters: MessageWaiterRegistry::default(),
        orchestration_delivery_in_flight: HashSet::new(),
        orchestration_delivery_backpressured: HashSet::new(),
        orchestration_activity_last_recorded: HashMap::new(),
        coordinators: HashMap::new(),
        pending_drift_probes: HashSet::new(),
        pending_dispatch_installs: HashMap::new(),
        resources: ResourceMonitorState::default(),
        terminal_pulses: Default::default(),
        codex: None,
        codex_starting: None,
        deferred_admission: Arc::new(deferred_admission::DeferredAdmission::default()),
        workspace_sidebar_snapshots: Default::default(),
        inbox,
        next_client_id: Arc::new(AtomicU64::new(1)),
        mobile_gateway: None,
        shutdown_gen: 0,
        disposed: false,
    };

    actor.dispose().await;

    let updated_dispatch = actor
        .runtime_store
        .orchestration_dispatch_by_id(&dispatch.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_dispatch.status, OrchestrationDispatchStatus::Failed);
    assert_eq!(updated_dispatch.failure_count, 1);
    assert_eq!(
        updated_dispatch.last_failure.as_deref(),
        Some("terminal host shut down")
    );
    let updated_task = actor
        .runtime_store
        .orchestration_task_by_id(&task.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_task.status, OrchestrationTaskStatus::Ready);
}

#[tokio::test]
async fn coordinator_does_not_spawn_worker_tab_for_cli_only_client() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path()).await.unwrap();
    let runtime_store = RuntimeStore::open(dir.path()).await.unwrap();
    runtime_store
        .create_orchestration_task(NewOrchestrationTask {
            spec: "do work".to_string(),
            task_title: None,
            display_name: None,
            deps: Vec::new(),
            parent_id: None,
            created_by_terminal_handle: None,
            run_id: None,
            workspace_id: "workspace-1".to_string(),
            coordinator_handle: "coord".to_string(),
            result_schema: None,
        })
        .await
        .unwrap();
    let (inbox, _rx) = mpsc::unbounded_channel();
    let (handle, _control_out_rx) = ClientHandle::test_channels();
    let mut actor = ServerActor {
        runtime_dir: dir.path().to_path_buf(),
        control_file_path: dir.path().join("runtime-host.json"),
        token: "token".to_string(),
        config: TerminalHostConfig::default(),
        history,
        runtime_store: runtime_store.clone(),
        automation_wake: Arc::new(Notify::new()),
        automations_active: false,
        sessions: HashMap::new(),
        ssh_bootstrap_jobs: HashMap::new(),
        project_clone_jobs: HashMap::new(),
        agent_title_jobs: HashMap::new(),
        managed_workspace_jobs: 0,
        mutation_queue: Default::default(),
        agent_quota_cache: None,
        configuration_transfers: Default::default(),
        account_push: account_push_for_test(&dir, &runtime_store).await,
        clients: HashMap::from([(1, ClientState::local(handle, false))]),
        mobile_prompt_file_uploads: HashMap::new(),
        mobile_prompt_image_uploads: HashMap::new(),
        agent_presence: AgentPresenceRegistry::default(),
        orchestration_waiters: MessageWaiterRegistry::default(),
        orchestration_delivery_in_flight: HashSet::new(),
        orchestration_delivery_backpressured: HashSet::new(),
        orchestration_activity_last_recorded: HashMap::new(),
        coordinators: HashMap::new(),
        pending_drift_probes: HashSet::new(),
        pending_dispatch_installs: HashMap::new(),
        resources: ResourceMonitorState::default(),
        terminal_pulses: Default::default(),
        codex: None,
        codex_starting: None,
        deferred_admission: Arc::new(deferred_admission::DeferredAdmission::default()),
        workspace_sidebar_snapshots: Default::default(),
        inbox,
        next_client_id: Arc::new(AtomicU64::new(2)),
        mobile_gateway: None,
        shutdown_gen: 0,
        disposed: false,
    };
    let response = actor
        .orchestration_run(&json!({
            "spec": "coordinate",
            "from": "coord",
            "workspace": "workspace-1",
            "pollIntervalMs": 600_000,
        }))
        .await
        .unwrap();

    actor
        .handle(ServerCommand::CoordinatorTick {
            run_id: response["runId"].as_str().unwrap().to_string(),
        })
        .await;

    assert!(actor
        .runtime_store
        .list_workspace_tabs("workspace-1")
        .await
        .unwrap()
        .is_empty());
}
