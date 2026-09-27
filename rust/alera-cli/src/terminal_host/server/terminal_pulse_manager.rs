use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{TerminalPulseConfiguration, WorkspacePulseWatcher};

pub(in crate::terminal_host::server) struct TerminalPulseRule {
    pub(in crate::terminal_host::server) workspace_id: String,
    pub(in crate::terminal_host::server) session_instance_id: u64,
    pub(in crate::terminal_host::server) configuration: TerminalPulseConfiguration,
    pub(in crate::terminal_host::server) generation: u64,
    pub(in crate::terminal_host::server) pending: bool,
    pub(in crate::terminal_host::server) activated_after_event_sequence: u64,
    pub(in crate::terminal_host::server) active: Arc<AtomicBool>,
}

pub(in crate::terminal_host::server) struct PendingTerminalPulseConfiguration {
    pub(in crate::terminal_host::server) client_id: u64,
    pub(in crate::terminal_host::server) request_id: i64,
    pub(in crate::terminal_host::server) session_id: String,
    pub(in crate::terminal_host::server) workspace_id: String,
    pub(in crate::terminal_host::server) tab_id: String,
    pub(in crate::terminal_host::server) session_instance_id: u64,
    pub(in crate::terminal_host::server) configuration: TerminalPulseConfiguration,
}

#[derive(Default)]
pub(in crate::terminal_host::server) struct TerminalPulseManager {
    pub(in crate::terminal_host::server) rules: HashMap<String, TerminalPulseRule>,
    pub(in crate::terminal_host::server) watchers: HashMap<String, WorkspacePulseWatcher>,
    pub(in crate::terminal_host::server) watcher_starts: HashMap<String, u64>,
    pub(in crate::terminal_host::server) pending_configurations:
        HashMap<String, PendingTerminalPulseConfiguration>,
    pub(in crate::terminal_host::server) next_generation: u64,
}

impl TerminalPulseManager {
    pub(in crate::terminal_host::server) fn reserve_watcher_start(
        &mut self,
        workspace_id: &str,
    ) -> Option<u64> {
        if self.watchers.contains_key(workspace_id)
            || self.watcher_starts.contains_key(workspace_id)
        {
            return None;
        }
        let generation = self.issue_generation();
        self.watcher_starts
            .insert(workspace_id.to_string(), generation);
        Some(generation)
    }

    pub(in crate::terminal_host::server) fn queue_configuration(
        &mut self,
        pending: PendingTerminalPulseConfiguration,
    ) -> Option<PendingTerminalPulseConfiguration> {
        self.pending_configurations
            .insert(pending.session_id.clone(), pending)
    }

    pub(in crate::terminal_host::server) fn cancel_pending_configuration(
        &mut self,
        session_id: &str,
    ) -> Option<PendingTerminalPulseConfiguration> {
        self.pending_configurations.remove(session_id)
    }

    pub(in crate::terminal_host::server) fn finish_watcher_start(
        &mut self,
        workspace_id: &str,
        generation: u64,
        watcher: WorkspacePulseWatcher,
    ) -> Option<Vec<PendingTerminalPulseConfiguration>> {
        if self.watcher_starts.get(workspace_id) != Some(&generation)
            || watcher.generation() != generation
        {
            return None;
        }
        self.watcher_starts.remove(workspace_id);
        self.watchers.insert(workspace_id.to_string(), watcher);
        Some(self.take_pending_configurations(workspace_id))
    }

    pub(in crate::terminal_host::server) fn fail_watcher_start(
        &mut self,
        workspace_id: &str,
        generation: u64,
    ) -> Option<Vec<PendingTerminalPulseConfiguration>> {
        if self.watcher_starts.get(workspace_id) != Some(&generation) {
            return None;
        }
        self.watcher_starts.remove(workspace_id);
        Some(self.take_pending_configurations(workspace_id))
    }

    pub(in crate::terminal_host::server) fn take_pending_configurations(
        &mut self,
        workspace_id: &str,
    ) -> Vec<PendingTerminalPulseConfiguration> {
        let session_ids = self
            .pending_configurations
            .iter()
            .filter(|(_, pending)| pending.workspace_id == workspace_id)
            .map(|(session_id, _)| session_id.clone())
            .collect::<Vec<_>>();
        session_ids
            .into_iter()
            .filter_map(|session_id| self.pending_configurations.remove(&session_id))
            .collect()
    }

    pub(in crate::terminal_host::server) fn accepts_watcher_command(
        &self,
        workspace_id: &str,
        watcher_generation: u64,
    ) -> bool {
        self.watchers
            .get(workspace_id)
            .is_some_and(|watcher| watcher.generation() == watcher_generation)
    }

    pub(in crate::terminal_host::server) fn arm(
        &mut self,
        session_id: String,
        workspace_id: String,
        session_instance_id: u64,
        configuration: TerminalPulseConfiguration,
    ) {
        let generation = self.issue_generation();
        let activated_after_event_sequence = self
            .watchers
            .get(&workspace_id)
            .map_or(0, WorkspacePulseWatcher::current_event_sequence);
        let previous = self.rules.insert(
            session_id,
            TerminalPulseRule {
                workspace_id,
                session_instance_id,
                configuration,
                generation,
                pending: false,
                activated_after_event_sequence,
                active: Arc::new(AtomicBool::new(true)),
            },
        );
        if let Some(previous) = previous {
            previous.active.store(false, Ordering::Release);
        }
    }

    pub(in crate::terminal_host::server) fn remove_unused_watcher(&mut self, workspace_id: &str) {
        if !self
            .rules
            .values()
            .any(|rule| rule.workspace_id == workspace_id)
            && !self
                .pending_configurations
                .values()
                .any(|pending| pending.workspace_id == workspace_id)
        {
            self.watchers.remove(workspace_id);
        }
    }

    pub(in crate::terminal_host::server) fn disarm(
        &mut self,
        session_id: &str,
    ) -> Option<TerminalPulseConfiguration> {
        let rule = self.rules.remove(session_id)?;
        rule.active.store(false, Ordering::Release);
        if !self
            .rules
            .values()
            .any(|candidate| candidate.workspace_id == rule.workspace_id)
        {
            self.watchers.remove(&rule.workspace_id);
        }
        Some(rule.configuration)
    }

    pub(in crate::terminal_host::server) fn fail_workspace(
        &mut self,
        workspace_id: &str,
    ) -> Vec<TerminalPulseStateChange> {
        self.watchers.remove(workspace_id);
        let changes = self
            .rules
            .iter()
            .filter(|(_, rule)| rule.workspace_id == workspace_id)
            .map(|(session_id, rule)| TerminalPulseStateChange {
                session_id: session_id.clone(),
                configuration: rule.configuration.clone(),
            })
            .collect::<Vec<_>>();
        for change in &changes {
            if let Some(rule) = self.rules.remove(&change.session_id) {
                rule.active.store(false, Ordering::Release);
            }
        }
        changes
    }

    pub(in crate::terminal_host::server) fn is_armed(
        &self,
        session_id: &str,
        session_instance_id: u64,
    ) -> bool {
        self.rules
            .get(session_id)
            .is_some_and(|rule| rule.session_instance_id == session_instance_id)
    }

    pub(in crate::terminal_host::server) fn schedule(
        &mut self,
        workspace_id: &str,
        event_sequence: u64,
    ) -> Vec<TerminalPulseSchedule> {
        let mut schedules = Vec::new();
        let session_ids = self
            .rules
            .iter_mut()
            .filter_map(|(session_id, rule)| {
                if rule.workspace_id != workspace_id
                    || event_sequence <= rule.activated_after_event_sequence
                {
                    return None;
                }
                rule.activated_after_event_sequence = event_sequence;
                if rule.pending {
                    return None;
                }
                Some(session_id.clone())
            })
            .collect::<Vec<_>>();
        for session_id in session_ids {
            let generation = self.issue_generation();
            let rule = self
                .rules
                .get_mut(&session_id)
                .expect("scheduled Terminal Pulse rule exists");
            rule.pending = true;
            rule.generation = generation;
            schedules.push(TerminalPulseSchedule {
                session_id,
                session_instance_id: rule.session_instance_id,
                generation: rule.generation,
                delay: Duration::from_millis(rule.configuration.delay_ms),
            });
        }
        schedules
    }

    pub(in crate::terminal_host::server) fn issue_generation(&mut self) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1);
        if self.next_generation == 0 {
            self.next_generation = 1;
        }
        self.next_generation
    }

    pub(in crate::terminal_host::server) fn due_write(
        &self,
        session_id: &str,
        session_instance_id: u64,
        generation: u64,
    ) -> Option<TerminalPulseWrite> {
        let rule = self.rules.get(session_id)?;
        if !rule.pending
            || rule.session_instance_id != session_instance_id
            || rule.generation != generation
        {
            return None;
        }
        Some(TerminalPulseWrite {
            bytes: rule.configuration.input_bytes(),
            active: Arc::clone(&rule.active),
        })
    }

    #[cfg(test)]
    pub(in crate::terminal_host::server) fn due_bytes(
        &self,
        session_id: &str,
        session_instance_id: u64,
        generation: u64,
    ) -> Option<Vec<u8>> {
        self.due_write(session_id, session_instance_id, generation)
            .map(|write| write.bytes)
    }

    pub(in crate::terminal_host::server) fn complete_due(
        &mut self,
        session_id: &str,
        session_instance_id: u64,
        generation: u64,
    ) {
        let observed_event_sequence = self
            .rules
            .get(session_id)
            .and_then(|rule| self.watchers.get(&rule.workspace_id))
            .map_or(0, WorkspacePulseWatcher::current_event_sequence);
        self.complete_due_at_sequence(
            session_id,
            session_instance_id,
            generation,
            observed_event_sequence,
        );
    }

    pub(in crate::terminal_host::server) fn complete_due_at_sequence(
        &mut self,
        session_id: &str,
        session_instance_id: u64,
        generation: u64,
        observed_event_sequence: u64,
    ) {
        let Some(rule) = self.rules.get_mut(session_id) else {
            return;
        };
        if rule.pending
            && rule.session_instance_id == session_instance_id
            && rule.generation == generation
        {
            rule.activated_after_event_sequence = rule
                .activated_after_event_sequence
                .max(observed_event_sequence);
            rule.pending = false;
        }
    }

    pub(in crate::terminal_host::server) fn retry_due(
        &mut self,
        session_id: &str,
        session_instance_id: u64,
        generation: u64,
    ) -> Option<u64> {
        let rule = self.rules.get(session_id)?;
        if !rule.pending
            || rule.session_instance_id != session_instance_id
            || rule.generation != generation
        {
            return None;
        }
        let next_generation = self.issue_generation();
        self.rules
            .get_mut(session_id)
            .expect("retried Terminal Pulse rule exists")
            .generation = next_generation;
        Some(next_generation)
    }

    pub(in crate::terminal_host::server) fn cancel_due(
        &mut self,
        session_id: &str,
        session_instance_id: u64,
        generation: u64,
    ) {
        let Some(rule) = self.rules.get_mut(session_id) else {
            return;
        };
        if rule.pending
            && rule.session_instance_id == session_instance_id
            && rule.generation == generation
        {
            rule.pending = false;
        }
    }
}

pub(in crate::terminal_host::server) struct TerminalPulseStateChange {
    pub(in crate::terminal_host::server) session_id: String,
    pub(in crate::terminal_host::server) configuration: TerminalPulseConfiguration,
}

pub(in crate::terminal_host::server) struct TerminalPulseSchedule {
    pub(in crate::terminal_host::server) session_id: String,
    pub(in crate::terminal_host::server) session_instance_id: u64,
    pub(in crate::terminal_host::server) generation: u64,
    pub(in crate::terminal_host::server) delay: Duration,
}

pub(in crate::terminal_host::server) struct TerminalPulseWrite {
    pub(in crate::terminal_host::server) bytes: Vec<u8>,
    pub(in crate::terminal_host::server) active: Arc<AtomicBool>,
}
