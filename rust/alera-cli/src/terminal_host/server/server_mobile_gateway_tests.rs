use super::mobile_gateway_replacement::MobileGatewayReplacement;
use super::server_test_support::account_push_for_test;
use super::*;

use std::net::{Ipv4Addr, Ipv6Addr};

use alera_core::runtime::MobileAccessSettings;
use tokio::net::TcpListener;

#[tokio::test]
async fn mobile_gateway_rebinds_same_port_after_releasing_old_listener() {
    let port_probe = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = port_probe.local_addr().unwrap().port();
    drop(port_probe);
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path())
        .await
        .unwrap();
    let runtime_store = RuntimeStore::open(dir.path()).await.unwrap();
    let (inbox, _rx) = mpsc::unbounded_channel();
    let current = MobileAccessSettings {
        enabled: true,
        bind_host: "127.0.0.1".to_string(),
        port: i64::from(port),
        ..MobileAccessSettings::default()
    };
    let next = MobileAccessSettings {
        enabled: true,
        bind_host: "0.0.0.0".to_string(),
        port: i64::from(port),
        ..MobileAccessSettings::default()
    };
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

    actor
        .apply_mobile_gateway_settings(MobileAccessSettings::default(), current.clone())
        .await
        .unwrap();
    let saved = actor
        .apply_mobile_gateway_settings(current, next)
        .await
        .unwrap();

    assert_eq!(saved.bind_host, "0.0.0.0");
    assert!(actor.mobile_gateway.is_some());
    actor.dispose().await;
}

#[tokio::test]
async fn mobile_gateway_binds_ipv6_loopback() {
    let port_probe = match TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).await {
        Ok(listener) => listener,
        Err(_) => return,
    };
    let port = port_probe.local_addr().unwrap().port();
    drop(port_probe);
    let dir = tempfile::tempdir().unwrap();
    let actor = actor_test_harness::test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let settings = MobileAccessSettings {
        enabled: true,
        bind_host: "::1".to_string(),
        port: i64::from(port),
        ..MobileAccessSettings::default()
    };

    let replacement = actor
        .prepare_mobile_gateway_replacement(&settings)
        .await
        .unwrap();

    match replacement {
        MobileGatewayReplacement::Bound { bind_address, .. } => {
            assert!(bind_address.starts_with("[::1]:"));
        }
        MobileGatewayReplacement::Disabled => panic!("expected bound mobile gateway"),
        MobileGatewayReplacement::Keep => panic!("expected bound mobile gateway"),
    }
}
