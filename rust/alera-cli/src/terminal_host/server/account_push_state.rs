use std::sync::Arc;
use std::time::Instant;

use alera_core::runtime::RuntimeStore;
use anyhow::Result;
use serde_json::{json, Value};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::mobile_access::host_name;
use crate::terminal_host::alera_account::{
    bind_callback_listener, AleraAccountService, AuthProvider, Pkce,
};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::push_notifications::{PushDamper, PushEvent};

pub(super) struct AccountPushState {
    pub(super) service: Arc<AleraAccountService>,
    pub(super) sign_in_cancel: Option<oneshot::Sender<()>>,
    pub(super) cloud_jobs: usize,
    pub(super) subscription_sync_in_flight: bool,
    pub(super) subscription_sync_waiters: Vec<(u64, i64)>,
    pub(super) push_enabled: bool,
    pub(super) active_subscriptions: usize,
    pub(super) damper: PushDamper,
    pub(super) pending_events: Vec<PushEvent>,
    pub(super) batch_started: Option<Instant>,
    pub(super) flush_generation: u64,
    pub(super) relay_task: Option<JoinHandle<()>>,
    pub(super) relay_stop: Option<oneshot::Sender<()>>,
    pub(super) relay_generation: u64,
    pub(super) relay_status: serde_json::Value,
    pub(super) relay_presence: std::collections::HashMap<
        u64,
        (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>),
    >,
}

impl AccountPushState {
    pub(super) async fn new(runtime_dir: std::path::PathBuf, store: RuntimeStore) -> Result<Self> {
        let service = Arc::new(AleraAccountService::new(runtime_dir, store.clone()).await?);
        // Privacy portable build: cloud push is hard-disabled even if an older
        // runtime database previously opted in.
        let enabled = false;
        let active_subscriptions = 0;
        Ok(Self {
            service,
            sign_in_cancel: None,
            cloud_jobs: 0,
            subscription_sync_in_flight: false,
            subscription_sync_waiters: Vec::new(),
            push_enabled: enabled,
            active_subscriptions,
            damper: PushDamper::default(),
            pending_events: Vec::new(),
            batch_started: None,
            flush_generation: 0,
            relay_task: None,
            relay_stop: None,
            relay_generation: 0,
            relay_status: serde_json::json!({ "state": "disabled" }),
            relay_presence: Default::default(),
        })
    }
}

pub(super) async fn prepare_sign_in(
    service: &AleraAccountService,
    provider: AuthProvider,
    linking: bool,
) -> anyhow::Result<(
    tokio::net::TcpListener,
    String,
    Pkce,
    crate::terminal_host::alera_account::AuthTransaction,
)> {
    let (listener, redirect_uri) = bind_callback_listener().await?;
    let pkce = Pkce::generate();
    let transaction = if linking {
        service
            .create_link_transaction(provider, &redirect_uri, &pkce.challenge)
            .await?
    } else {
        service
            .create_auth_transaction(provider, &redirect_uri, &pkce.challenge, &host_name())
            .await?
    };
    Ok((listener, redirect_uri, pkce, transaction))
}

pub(super) async fn async_account_sign_out(service: Arc<AleraAccountService>) -> HostResult<Value> {
    service.sign_out().await.map_err(account_error)?;
    Ok(json!({ "connected": false }))
}

pub(super) async fn async_account_delete(service: Arc<AleraAccountService>) -> HostResult<Value> {
    service.delete_account().await.map_err(account_error)?;
    Ok(json!({ "deleted": true }))
}

pub(super) async fn async_account_transfer(
    service: Arc<AleraAccountService>,
    target_account_id: String,
) -> HostResult<Value> {
    service
        .transfer_runtime(&target_account_id)
        .await
        .map_err(account_error)?;
    Ok(json!({
        "transferred": true,
        "reauthenticationRequired": true,
    }))
}

pub(super) async fn async_mobile_enrollment(
    service: Arc<AleraAccountService>,
    device_id: String,
    device_name: String,
) -> HostResult<Value> {
    service
        .create_mobile_enrollment(&device_id, &device_name)
        .await
        .map(|enrollment| json!(enrollment))
        .map_err(account_error)
}

pub(super) fn account_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}
