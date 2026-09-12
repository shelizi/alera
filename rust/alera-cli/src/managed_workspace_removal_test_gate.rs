use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::Notify;

struct GateState {
    entered: Notify,
    release: Notify,
}

static GATES: OnceLock<Mutex<HashMap<String, Arc<GateState>>>> = OnceLock::new();

pub(crate) struct ManagedWorkspaceRemovalTestGate {
    state: Arc<GateState>,
}

impl ManagedWorkspaceRemovalTestGate {
    pub(crate) async fn wait_until_entered(&self) {
        self.state.entered.notified().await;
    }

    pub(crate) fn release(&self) {
        self.state.release.notify_one();
    }
}

pub(crate) fn install(workspace_id: &str) -> ManagedWorkspaceRemovalTestGate {
    let state = Arc::new(GateState {
        entered: Notify::new(),
        release: Notify::new(),
    });
    let mut gates = GATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    gates.insert(workspace_id.to_string(), Arc::clone(&state));
    ManagedWorkspaceRemovalTestGate { state }
}

pub(crate) async fn wait_if_installed(workspace_id: &str) {
    let gate = {
        let Some(gates) = GATES.get() else {
            return;
        };
        let mut gates = gates.lock().unwrap_or_else(|error| error.into_inner());
        gates.remove(workspace_id)
    };
    if let Some(gate) = gate {
        gate.entered.notify_one();
        gate.release.notified().await;
    }
}
