use super::server_test_support::account_push_for_test;
use super::*;

use crate::ssh_bootstrap::SshTargetBootstrapProgress;

#[tokio::test]
async fn stale_ssh_bootstrap_progress_is_not_broadcast() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path()).await.unwrap();
    let runtime_store = RuntimeStore::open(dir.path()).await.unwrap();
    let (inbox, _rx) = mpsc::unbounded_channel();
    let (handle, mut out_rx) = ClientHandle::test_channels();
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
        ssh_bootstrap_jobs: HashMap::from([(
            "remote".to_string(),
            SshBootstrapJobState {
                job_id: "active-job".to_string(),
                target_id: "remote".to_string(),
                status: SshBootstrapStatus::Installing,
                cancel: Arc::new(AtomicBool::new(false)),
                cancel_notify: Arc::new(Notify::new()),
            },
        )]),
        project_clone_jobs: HashMap::new(),
        agent_title_jobs: HashMap::new(),
        managed_workspace_jobs: 0,
        mutation_queue: Default::default(),
        agent_quota_cache: None,
        configuration_transfers: Default::default(),
        account_push: account_push_for_test(&dir, &runtime_store).await,
        clients: HashMap::from([(1, ClientState::local(handle, true))]),
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

    actor.handle_ssh_bootstrap_progress(SshTargetBootstrapProgress {
        job_id: "stale-job".to_string(),
        target_id: "remote".to_string(),
        status: SshBootstrapStatus::Failed,
        stage: "failed".to_string(),
        message: "Stale failure".to_string(),
        error: Some("stale".to_string()),
    });

    assert!(out_rx.try_recv().is_err());
    assert_eq!(
        actor.ssh_bootstrap_jobs["remote"].status,
        SshBootstrapStatus::Installing
    );
}

#[tokio::test]
async fn saturated_admission_rejects_ssh_bootstrap_without_job_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = actor_test_harness::test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor
        .ssh_target_upsert(&json!({
            "id": "remote-saturated",
            "alias": "Saturated Remote",
            "host": "example.invalid",
            "port": 22,
            "username": "tester",
            "platform": "linux",
            "arch": "x64",
            "authKind": "agent",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z",
            "lastStatus": null,
            "installDir": null,
            "runtimeVersion": null,
            "runtimePlatform": null,
            "runtimeArch": null,
            "bootstrapStatus": "notInstalled",
            "lastBootstrapAt": null,
            "lastCheckedAt": null,
            "lastError": null,
        }))
        .await
        .unwrap();
    actor.deferred_admission = Arc::new(deferred_admission::DeferredAdmission::paused_with_limits(
        1, 2, 0,
    ));
    actor.deferred_admission.add_test_permits(1);
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    actor
        .deferred_admission
        .schedule(
            deferred_admission::DeferredRequestClass::Bulk,
            "test.fill",
            Some(1),
            async move {
                let _ = release_rx.await;
            },
        )
        .unwrap();
    actor
        .deferred_admission
        .schedule(
            deferred_admission::DeferredRequestClass::Bulk,
            "test.queued",
            Some(1),
            async {},
        )
        .unwrap();

    let request = serde_json::from_value(json!({
        "targetId": "remote-saturated",
    }))
    .unwrap();
    let error = actor
        .start_ssh_bootstrap_job(1, request)
        .await
        .expect_err("a saturated admission budget must reject SSH bootstrap");
    let HostError::Conflict { code, details, .. } = error else {
        panic!("expected a typed backpressure conflict");
    };
    assert_eq!(code, deferred_admission::DEFERRED_REQUEST_BACKPRESSURE_CODE);
    assert_eq!(details["requestType"], "sshTarget.bootstrap");
    assert_eq!(details["requestClass"], "bulk");
    assert!(actor.ssh_bootstrap_jobs.is_empty());
    let target = actor
        .runtime_store
        .find_ssh_target("remote-saturated")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(target.bootstrap_status, SshBootstrapStatus::Failed);
    assert!(actor.shutdown_gen > 0);

    release_tx.send(()).unwrap();
}
