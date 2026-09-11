use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};

use alera_core::runtime::RuntimeAgentStatusHookSettings;

use crate::agent_status::reconcile_agent_integrations;

use super::ServerActor;

type AgentIntegrationReconcileRegistry =
    StdMutex<HashMap<String, Weak<AgentIntegrationReconcileState>>>;

static AGENT_INTEGRATION_RECONCILE_STATES: OnceLock<AgentIntegrationReconcileRegistry> =
    OnceLock::new();

struct AgentIntegrationReconcileState {
    generation: AtomicU64,
    serial: Arc<tokio::sync::Mutex<()>>,
}

impl Default for AgentIntegrationReconcileState {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            serial: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}

impl AgentIntegrationReconcileState {
    fn next_generation(&self) -> u64 {
        self.generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }

    fn is_latest(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Acquire) == generation
    }

    async fn acquire_if_latest(&self, generation: u64) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        let guard = self.serial.clone().lock_owned().await;
        self.is_latest(generation).then_some(guard)
    }
}

fn agent_integration_reconcile_state(runtime_dir: &Path) -> Arc<AgentIntegrationReconcileState> {
    let key = runtime_dir.to_string_lossy().into_owned();
    let registry = AGENT_INTEGRATION_RECONCILE_STATES.get_or_init(|| StdMutex::new(HashMap::new()));
    let Ok(mut states) = registry.lock() else {
        return Arc::new(AgentIntegrationReconcileState::default());
    };
    states.retain(|_, state| state.strong_count() > 0);
    if let Some(state) = states.get(&key).and_then(Weak::upgrade) {
        return state;
    }
    let state = Arc::new(AgentIntegrationReconcileState::default());
    states.insert(key, Arc::downgrade(&state));
    state
}

impl ServerActor {
    pub(super) fn schedule_agent_integration_reconcile(
        &self,
        settings: RuntimeAgentStatusHookSettings,
    ) {
        let runtime_dir = self.runtime_dir.clone();
        let state = agent_integration_reconcile_state(&runtime_dir);
        let generation = state.next_generation();
        let slots = self.deferred_request_slots.clone();
        tokio::spawn(async move {
            let Some(_serial) = state.acquire_if_latest(generation).await else {
                return;
            };
            let _permit = match slots.acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => return,
            };
            if !state.is_latest(generation) {
                return;
            }
            match tokio::task::spawn_blocking(move || {
                reconcile_agent_integrations(&runtime_dir, &settings)
            })
            .await
            {
                Ok(warnings) => {
                    for warning in warnings {
                        tracing::warn!("alera agent integration warning: {warning}");
                    }
                }
                Err(error) => tracing::warn!("alera agent integration reconcile failed: {error}"),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::AgentIntegrationReconcileState;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn agent_integration_reconcile_skips_stale_work_before_it_starts() {
        let state = AgentIntegrationReconcileState::default();
        let stale = state.next_generation();
        let latest = state.next_generation();

        assert!(state.acquire_if_latest(stale).await.is_none());
        assert!(state.acquire_if_latest(latest).await.is_some());
    }

    #[tokio::test]
    async fn latest_agent_integration_reconcile_finishes_after_in_flight_stale_work() {
        let state = Arc::new(AgentIntegrationReconcileState::default());
        let order = Arc::new(Mutex::new(Vec::new()));
        let first = state.next_generation();
        let first_guard = state.acquire_if_latest(first).await.unwrap();
        let latest = state.next_generation();
        let latest_state = state.clone();
        let latest_order = order.clone();
        let latest_worker = tokio::spawn(async move {
            let _guard = latest_state.acquire_if_latest(latest).await.unwrap();
            latest_order.lock().unwrap().push(2);
        });
        tokio::task::yield_now().await;

        order.lock().unwrap().push(1);
        drop(first_guard);
        latest_worker.await.unwrap();

        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }
}
