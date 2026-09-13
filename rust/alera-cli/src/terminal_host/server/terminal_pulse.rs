use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{ServerActor, ServerCommand};
use crate::terminal_host::host_error::{HostError, HostResult};

#[path = "terminal_pulse_delivery.rs"]
mod delivery;

#[path = "terminal_pulse_configuration.rs"]
mod configuration;

#[path = "terminal_pulse_watcher.rs"]
mod watcher;

#[path = "terminal_pulse_path_identities.rs"]
mod path_identities;

#[path = "terminal_pulse_manager.rs"]
mod manager;

#[cfg(test)]
use watcher::event_is_relevant;
pub(super) use watcher::WorkspacePulseWatcher;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::sync::Arc;
pub(super) use manager::{
    PendingTerminalPulseConfiguration, TerminalPulseManager,
};
#[cfg(test)]
pub(super) use manager::TerminalPulseRule;

pub(super) const TERMINAL_PULSE_PAYLOAD_KEY: &str = "terminalPulse";
const DEFAULT_DELAY_MS: u64 = 2_000;
const MIN_DELAY_MS: u64 = 100;
const MAX_DELAY_MS: u64 = 3_600_000;
const MAX_INPUT_BYTES: usize = 4_096;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TerminalPulseConfiguration {
    command: String,
    #[serde(default = "default_append_enter")]
    append_enter: bool,
    #[serde(default = "default_delay_ms")]
    delay_ms: u64,
}

impl Default for TerminalPulseConfiguration {
    fn default() -> Self {
        Self {
            command: "r".to_string(),
            append_enter: true,
            delay_ms: DEFAULT_DELAY_MS,
        }
    }
}

impl TerminalPulseConfiguration {
    fn validate(&self) -> HostResult<()> {
        if self.command.is_empty() {
            return Err(HostError::format("Terminal Pulse input is required."));
        }
        if self.command.len() > MAX_INPUT_BYTES {
            return Err(HostError::format(format!(
                "Terminal Pulse input cannot exceed {MAX_INPUT_BYTES} bytes."
            )));
        }
        if !(MIN_DELAY_MS..=MAX_DELAY_MS).contains(&self.delay_ms) {
            return Err(HostError::format(format!(
                "Terminal Pulse delay must be between {MIN_DELAY_MS} and {MAX_DELAY_MS} ms."
            )));
        }
        Ok(())
    }

    fn input_bytes(&self) -> Vec<u8> {
        let mut bytes = self.command.as_bytes().to_vec();
        if self.append_enter {
            bytes.push(b'\r');
        }
        bytes
    }
}

fn default_append_enter() -> bool {
    true
}

fn default_delay_ms() -> u64 {
    DEFAULT_DELAY_MS
}

impl ServerActor {
    pub(super) async fn terminal_pulse_status(&self, payload: &Value) -> HostResult<Value> {
        let session_id = self.require_session(payload)?;
        let session = self
            .sessions
            .get(&session_id)
            .expect("required session exists");
        let configuration = self.terminal_pulse_configuration(&session.tab_id).await?;
        Ok(terminal_pulse_state(
            configuration,
            self.terminal_pulses
                .is_armed(&session_id, session.instance_id()),
        ))
    }

    async fn terminal_pulse_configuration(
        &self,
        tab_id: &str,
    ) -> HostResult<TerminalPulseConfiguration> {
        let tab = self
            .runtime_store
            .find_workspace_tab(tab_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        let Some(value) = tab.and_then(|tab| tab.payload.get(TERMINAL_PULSE_PAYLOAD_KEY).cloned())
        else {
            return Ok(TerminalPulseConfiguration::default());
        };
        serde_json::from_value(value).map_err(|error| HostError::format(error.to_string()))
    }

    async fn persist_terminal_pulse_configuration(
        &self,
        tab_id: &str,
        configuration: &TerminalPulseConfiguration,
    ) -> HostResult<()> {
        let mut tab = self
            .runtime_store
            .find_workspace_tab(tab_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            .ok_or_else(|| HostError::state(format!("terminal tab not found: {tab_id}")))?;
        let payload = tab
            .payload
            .as_object_mut()
            .ok_or_else(|| HostError::format("Terminal tab payload must be an object."))?;
        payload.insert(
            TERMINAL_PULSE_PAYLOAD_KEY.to_string(),
            serde_json::to_value(configuration)
                .map_err(|error| HostError::format(error.to_string()))?,
        );
        tab.updated_at = chrono::Utc::now();
        self.runtime_store
            .upsert_workspace_tab(tab)
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        Ok(())
    }
}

fn terminal_pulse_state(configuration: TerminalPulseConfiguration, armed: bool) -> Value {
    json!({
        "configuration": configuration,
        "armed": armed,
    })
}

#[cfg(test)]
#[path = "terminal_pulse_manager_cases.rs"]
mod manager_cases;
#[cfg(test)]
#[path = "terminal_pulse_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "terminal_pulse_watcher_cases.rs"]
mod watcher_cases;
