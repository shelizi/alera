use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use alera_core::runtime::{prepare_private_runtime_directory, RuntimeStore, SshBootstrapStatus};
use anyhow::Result;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::agent_status::{start_agent_integrations, start_fx_herdr_receiver, start_hook_receiver};
use crate::terminal_host::client::{
    connection_loop, ClientFrame, ClientHandle, CLIENT_TERMINAL_OUT_QUEUE_CAPACITY,
};
use crate::terminal_host::control_file;
use crate::terminal_host::history_repository::TerminalHostHistoryRepository;
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::agent_presence::AgentPresenceRegistry;
use crate::terminal_host::orchestration::coordinator_loop::CoordinatorHandle;
use crate::terminal_host::orchestration::message_delivery::skips_auto_enter;
use crate::terminal_host::orchestration::message_waiters::MessageWaiterRegistry;
use crate::terminal_host::protocol::{event, TerminalHostConfig};
use crate::terminal_host::runtime_owner;
use crate::terminal_host::session::{PtyWriteCompletion, Session};
use crate::terminal_host::sleep_detector::SleepDetector;

use client_accept_loop::spawn_accept_loop;

use resource_requests::ResourceMonitorState;

mod account_push_state;
mod account_requests;
#[cfg(test)]
mod account_requests_tests;
#[cfg(test)]
mod actor_test_harness;
mod agent_hook_events;
mod agent_profile_launch_requests;
mod agent_prompt_composition;
#[cfg(test)]
mod agent_settings_contract_tests;
#[cfg(test)]
mod agent_settings_contract_tests_runtime_settings;
mod agent_spawn_install;
mod agent_title_context;
mod agent_title_events;
mod agent_title_generation;
#[cfg(all(test, unix))]
mod agent_title_provider_tests;
mod agent_title_state;
#[cfg(test)]
mod agent_title_tests;
mod ai_assist_command_execution;
mod ai_assist_failure_detail;
mod ai_assist_fx_plan;
mod ai_assist_grok_plan;
mod ai_assist_model_defaults;
mod ai_assist_open_code;
mod ai_assist_requests;
mod ai_assist_speech_message;
mod ai_assist_workspace_identity;
mod ai_dictation_credentials;
mod ai_dictation_openai;
mod ai_dictation_remote_requests;
mod ai_dictation_requests;
mod automation_actor;
mod automation_catalog_requests;
mod automation_definition_requests;
mod automation_dispatch;
mod automation_policy_requests;
mod automation_request_authorization;
mod automation_request_routes;
mod automation_requests;
mod automation_run_target_requests;
mod automation_scheduler;
mod checkpoint_timer;
mod client_accept_loop;
mod client_delivery;
mod client_dispose;
mod codex_app_server;
mod codex_app_server_session_state;
mod codex_dictation;
mod codex_server_startup;
mod configuration_requests;
mod configuration_transfers;
mod coordinator_dispatch;
#[cfg(test)]
mod coordinator_drift_probe_tests;
mod coordinator_requests;
mod coordinator_stall_policy;
mod declared_catalog_requests;
mod deferred_admission;
#[cfg(test)]
mod deferred_admission_load_bench;
mod deferred_admission_metrics;
#[cfg(test)]
mod deferred_admission_tests;
#[cfg(test)]
mod deferred_admission_tests_delayed_timers;
#[cfg(test)]
mod deferred_project_requests_tests;
mod deferred_requests;
#[cfg(test)]
mod deferred_requests_tests;
mod disconnect_reason;
mod dispatch_context_continuations;
mod dispatch_context_install;
#[cfg(test)]
mod dispatch_context_install_tests;
#[cfg(test)]
mod dispatch_context_install_tests_startup_recovery;
mod host_service_agent_integrations;
mod host_service_agent_quota;
mod host_service_agent_quota_codex_reset;
#[cfg(test)]
mod host_service_autostart_tests;
mod host_service_requests;
mod host_status;
mod lifecycle;
#[cfg(test)]
mod managed_workspace_cleanup_tests;
mod managed_workspace_requests;
mod mobile_gateway_replacement;
mod mobile_gateway_surface;
mod mobile_hello_requests;
#[cfg(test)]
mod mobile_relay_presence_tests;
mod mobile_terminal_requests;
#[cfg(test)]
mod mobile_terminal_viewport_tests;
mod mobile_workspace_file_paths;
mod mobile_workspace_file_requests;
mod orchestration_agent_spawn_requests;
mod orchestration_completion_requests;
#[cfg(test)]
mod orchestration_contract_tests;
#[cfg(test)]
mod orchestration_contract_tests_mutation_ids;
mod orchestration_delivery;
mod orchestration_dispatch_requests;
mod orchestration_gate_requests;
mod orchestration_message_requests;
mod orchestration_mutation_requests;
mod orchestration_owned_spawn;
mod orchestration_policy_requests;
mod orchestration_profile_spawn;
mod orchestration_requests;
mod orchestration_run_requests;
mod orchestration_task_requests;
mod orchestration_terminal_requests;
mod orchestration_validation;
mod orchestration_wait_requests;
mod output_delivery;
#[cfg(test)]
mod output_resume_tests;
mod project_clone_requests;
#[cfg(test)]
mod project_contract_tests;
mod project_requests;
mod prompt_file_requests;
mod prompt_file_store;
mod prompt_image_requests;
mod prompt_image_store;
mod pty_event_forwarder;
mod pty_events;
mod pty_events_session_lifecycle;
mod push_delivery;
mod remote_relay;
mod request_payloads;
mod request_route_policy;
mod requests;
mod resource_requests;
mod runtime_change_broadcasts;
mod runtime_mutation_barrier;
mod runtime_mutation_queue;
mod runtime_mutations;
mod server_command;
#[cfg(test)]
mod server_lifecycle_tests;
#[cfg(test)]
mod server_mobile_gateway_tests;
#[cfg(test)]
mod server_orchestration_tests;
#[path = "server_runner.rs"]
mod server_runner;
mod server_shutdown;
#[cfg(test)]
mod server_shutdown_tests;
#[cfg(test)]
mod server_ssh_bootstrap_tests;
#[cfg(test)]
mod server_test_support;
mod session_termination;
#[cfg(test)]
mod session_termination_tests;
mod ssh_bootstrap_jobs;
mod tab_compatibility;
#[cfg(test)]
mod tab_compatibility_tests;
#[cfg(test)]
mod tab_layout_contract_tests;
#[cfg(test)]
mod tab_profile_launch_compatibility_tests;
#[cfg(test)]
mod terminal_contract_tests;
mod terminal_driver;
mod terminal_input_requests;
mod terminal_launch_defaults;
mod terminal_prompt_rearm;
mod terminal_pulse;
mod terminal_session_requests;
mod terminal_spawn;
mod terminal_spawn_startup_delivery;
mod terminal_startup_commands;
#[cfg(test)]
mod wire_fixture_tests;
#[cfg(test)]
mod wire_fixture_tests_lifecycle;
#[cfg(test)]
mod wire_fixture_tests_lifecycle_hello;
#[cfg(test)]
mod wire_fixture_tests_requests;
#[cfg(test)]
mod wire_fixture_tests_requests_workspace;
#[cfg(test)]
mod wire_fixture_tests_stream;
#[cfg(test)]
mod workspace_contract_tests;
mod workspace_pinning;
mod workspace_section_requests;
#[cfg(test)]
mod workspace_section_requests_tests;
mod workspace_sidebar_requests;
#[cfg(test)]
mod workspace_sidebar_requests_tests;

pub(crate) use disconnect_reason::DisconnectReason;
pub use server_command::ServerCommand;

/// Delay before a debounced checkpoint write fires.
const CHECKPOINT_DELAY: Duration = Duration::from_secs(5);

const OUTPUT_BATCH_DELAY: Duration = Duration::from_millis(8);
const OUTPUT_RESYNC_RETRY_DELAY: Duration = Duration::from_millis(16);
const DURABLE_OUTPUT_BATCH_DELAY: Duration = Duration::from_millis(100);
pub(crate) const TERMINAL_INPUT_BACKPRESSURE_CODE: &str = "terminal_input_backpressure";
/// Cap coalesced PTY→client batches so a verbose agent/build cannot grow an
/// unbounded `output_batch` between timer flushes (early flush when exceeded).
const OUTPUT_BATCH_MAX_BYTES: usize = 64 * 1024;
const ORCHESTRATION_ACTIVITY_WRITE_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientKind {
    Local,
    Mobile,
}

struct ClientState {
    handle: ClientHandle,
    authenticated: bool,
    binary_frames: bool,
    kind: ClientKind,
    local_role: client_delivery::LocalClientRole,
    mobile_device_id: Option<String>,
    mobile_device_name: Option<String>,
    cloud_device_id: Option<String>,
    relay_client_id: Option<String>,
}

struct SshBootstrapJobState {
    job_id: String,
    target_id: String,
    status: SshBootstrapStatus,
    cancel: Arc<AtomicBool>,
    cancel_notify: Arc<Notify>,
}

async fn wait_for_ssh_bootstrap_cancellation(cancel: Arc<AtomicBool>, cancel_notify: Arc<Notify>) {
    loop {
        if cancel.load(Ordering::Acquire) {
            return;
        }
        cancel_notify.notified().await;
    }
}

pub use server_runner::{run_terminal_host_server, TerminalHostExit};

struct ServerActor {
    runtime_dir: PathBuf,
    control_file_path: PathBuf,
    token: String,
    config: TerminalHostConfig,
    history: TerminalHostHistoryRepository,
    runtime_store: RuntimeStore,
    automation_wake: Arc<Notify>,
    automations_active: bool,
    sessions: HashMap<String, Session>,
    ssh_bootstrap_jobs: HashMap<String, SshBootstrapJobState>,
    project_clone_jobs: HashMap<String, tokio::sync::oneshot::Sender<()>>,
    agent_title_jobs: HashMap<String, agent_title_generation::AgentTitleJob>,
    managed_workspace_jobs: usize,
    mutation_queue: runtime_mutation_queue::RuntimeMutationQueue,
    agent_quota_cache: Option<(Instant, u64, Value)>,
    configuration_transfers: configuration_transfers::ConfigurationTransfers,
    account_push: account_push_state::AccountPushState,
    clients: HashMap<u64, ClientState>,
    mobile_prompt_file_uploads: HashMap<u64, HashSet<String>>,
    mobile_prompt_image_uploads: HashMap<u64, HashSet<String>>,
    agent_presence: AgentPresenceRegistry,
    orchestration_waiters: MessageWaiterRegistry,
    orchestration_delivery_in_flight: HashSet<String>,
    orchestration_delivery_backpressured: HashSet<String>,
    orchestration_activity_last_recorded: HashMap<String, Instant>,
    pending_dispatch_installs: HashMap<String, dispatch_context_install::PendingDispatchContext>,
    /// Runs with a drift probe parked off the actor; the probe completion
    /// resumes the dispatch round. One in-flight probe per run.
    pending_drift_probes: HashSet<String>,
    coordinators: HashMap<String, CoordinatorHandle>,
    resources: ResourceMonitorState,
    terminal_pulses: terminal_pulse::TerminalPulseManager,
    codex: Option<codex_app_server::CodexAppServer>,
    codex_starting: Option<codex_server_startup::CodexServerStartup>,
    inbox: UnboundedSender<ServerCommand>,
    deferred_admission: Arc<deferred_admission::DeferredAdmission>,
    workspace_sidebar_snapshots: workspace_sidebar_requests::WorkspaceSidebarSnapshotState,
    next_client_id: Arc<AtomicU64>,
    mobile_gateway: Option<JoinHandle<()>>,
    shutdown_gen: u64,
    disposed: bool,
}

impl ServerActor {
    async fn handle(&mut self, command: ServerCommand) {
        match command {
            command @ (ServerCommand::RelayActivity { .. }
            | ServerCommand::RelayStatus { .. }
            | ServerCommand::RelayClientConnected { .. }
            | ServerCommand::RelayClientLine { .. }) => self.handle_relay_command(command).await,
            ServerCommand::ClientConnected { id, handle, kind } => {
                self.clients.insert(
                    id,
                    ClientState {
                        handle,
                        authenticated: false,
                        binary_frames: false,
                        kind,
                        local_role: client_delivery::LocalClientRole::Cli,
                        mobile_device_id: None,
                        mobile_device_name: None,
                        cloud_device_id: None,
                        relay_client_id: None,
                    },
                );
            }
            ServerCommand::ClientLine { id, line } => self.handle_line(id, line).await,
            ServerCommand::ClientDisconnected { id, reason } => {
                self.account_push.relay_presence.remove(&id);
                self.dispose_client_with_reason(id, reason).await;
            }
            ServerCommand::MobileStatusFinished {
                client_id,
                request_id,
                payload,
            } => {
                self.finish_mobile_network_snapshot(client_id, request_id, payload);
            }
            ServerCommand::WorkspaceSidebarSnapshotFinished { result } => {
                self.finish_workspace_sidebar_snapshot(result);
            }
            ServerCommand::ProjectRegistrationPrepared {
                client_id,
                request_id,
                result,
            } => {
                self.finish_project_registration(client_id, request_id, result)
                    .await;
            }
            ServerCommand::DeferredRequestFinished {
                client_id,
                request_id,
                result,
            } => {
                self.finish_deferred_request(client_id, request_id, result);
            }
            ServerCommand::DispatchContextInstalled {
                dispatch_id,
                generation,
                result,
            } => {
                self.finish_dispatch_context_install(dispatch_id, generation, result)
                    .await;
            }
            ServerCommand::Pty {
                session_id,
                event,
                handled,
            } => {
                self.handle_pty_event(session_id, event).await;
                let _ = handled.send(());
            }
            ServerCommand::OutputBatchTick {
                session_id,
                generation,
            } => self.handle_output_batch_tick(session_id, generation),
            ServerCommand::OutputResyncTick {
                session_id,
                client_id,
            } => self.handle_output_resync_tick(session_id, client_id),
            ServerCommand::DurableOutputBatchTick {
                session_id,
                generation,
            } => self.handle_durable_output_batch_tick(session_id, generation),
            ServerCommand::CheckpointTick {
                session_id,
                generation,
            } => self.handle_checkpoint_tick(session_id, generation).await,
            ServerCommand::ShutdownTick { generation } => {
                self.handle_shutdown_tick(generation).await
            }
            ServerCommand::RequestedShutdown => self.dispose().await,
            ServerCommand::RequestedRestart => self.dispose().await,
            ServerCommand::AgentHookEvent { event } => self.handle_agent_hook_event(event).await,
            ServerCommand::SshBootstrapProgress { progress } => {
                self.handle_ssh_bootstrap_progress(progress)
            }
            ServerCommand::SshBootstrapFinished {
                target_id,
                job_id,
                status,
            } => self.handle_ssh_bootstrap_finished(target_id, job_id, status),
            ServerCommand::ManagedWorkspaceCreated {
                client_id,
                request_id,
                result,
            } => {
                self.handle_managed_workspace_created(client_id, request_id, result)
                    .await
            }
            ServerCommand::WorkspaceStorageMeasured {
                client_id,
                request_id,
                result,
            } => self.handle_workspace_storage_measured(client_id, request_id, result),
            ServerCommand::WorkspaceSetupFinished {
                client_id,
                request_id,
                result,
            } => {
                self.handle_workspace_setup_finished(client_id, request_id, result)
                    .await
            }
            ServerCommand::AgentTitleReady { tab_id, id } => {
                self.start_agent_title(tab_id, id).await
            }
            ServerCommand::AgentTitleFinished { tab_id, id, result } => {
                self.finish_agent_title(tab_id, id, result).await
            }
            ServerCommand::AiAssistFinished {
                client_id,
                request_id,
                result,
            } => self.handle_ai_assist_finished(client_id, request_id, result),
            ServerCommand::AiDictationFinished {
                client_id,
                request_id,
                result,
            } => self.handle_ai_dictation_finished(client_id, request_id, result),
            ServerCommand::MobileWorkspaceFileFinished {
                client_id,
                request_id,
                request_type,
                result,
            } => self.handle_mobile_workspace_file_finished(
                client_id,
                request_id,
                &request_type,
                result,
            ),
            ServerCommand::MobilePromptFileFinished {
                client_id,
                request_id,
                request_type,
                upload_id,
                result,
            } => self.handle_mobile_prompt_file_finished(
                client_id,
                request_id,
                &request_type,
                upload_id.as_deref(),
                result,
            ),
            ServerCommand::MobilePromptImageFinished {
                client_id,
                request_id,
                request_type,
                upload_id,
                result,
            } => self.handle_mobile_prompt_image_finished(
                client_id,
                request_id,
                &request_type,
                upload_id.as_deref(),
                result,
            ),
            ServerCommand::AgentQuotaFinished {
                client_id,
                request_id,
                environment_signature,
                result,
            } => self.handle_agent_quota_finished(
                client_id,
                request_id,
                environment_signature,
                result,
            ),
            ServerCommand::AgentQuotaClaudeTuiFinished {
                client_id,
                request_id,
                environment_signature,
                result,
            } => self.handle_agent_quota_claude_tui_finished(
                client_id,
                request_id,
                environment_signature,
                result,
            ),
            ServerCommand::AgentQuotaCodexResetFinished {
                client_id,
                request_id,
                environment_signature,
                result,
            } => self.handle_agent_quota_codex_reset_finished(
                client_id,
                request_id,
                environment_signature,
                result,
            ),
            ServerCommand::HostToolFinished {
                client_id,
                request_id,
                result,
                operation_id,
                skill,
            } => self.handle_host_tool_finished(client_id, request_id, result, operation_id, skill),
            ServerCommand::AutostartReconcileFinished {
                client_id,
                request_id,
                value,
                result,
            } => self.handle_autostart_reconcile_finished(client_id, request_id, value, result),
            ServerCommand::RuntimeMutationFinished(finished) => {
                self.handle_runtime_mutation_finished(finished).await
            }
            ServerCommand::PrepareRuntimeMutation {
                request,
                completion,
            } => {
                let result = self.prepare_runtime_mutation(&request).await;
                let _ = completion.send(result);
            }
            ServerCommand::OrchestrationWaitTimeout {
                waiter_id,
                effective_timeout_ms,
            } => {
                self.handle_orchestration_wait_timeout(waiter_id, effective_timeout_ms)
                    .await
            }
            ServerCommand::OrchestrationStateWaitPoll(waiter_id) => {
                self.handle_orchestration_state_wait_poll(waiter_id).await
            }
            ServerCommand::OrchestrationDeferredEnter {
                session_id,
                session_instance_id,
                message_ids,
                force_submit,
            } => {
                self.handle_orchestration_deferred_enter(
                    session_id,
                    session_instance_id,
                    message_ids,
                    force_submit,
                )
                .await
            }
            ServerCommand::TerminalStartupInput {
                session_id,
                session_instance_id,
                interactive_shell,
                command,
            } => self.handle_terminal_startup_input(
                session_id,
                session_instance_id,
                interactive_shell,
                command,
            ),
            ServerCommand::TerminalStartupSubmit {
                session_id,
                session_instance_id,
            } => self.handle_terminal_startup_submit(session_id, session_instance_id),
            ServerCommand::TerminalPulseFileChanged {
                workspace_id,
                watcher_generation,
                event_sequence,
            } => self.handle_terminal_pulse_file_changed(
                &workspace_id,
                watcher_generation,
                event_sequence,
            ),
            ServerCommand::TerminalPulseWatcherStarted {
                workspace_id,
                generation,
                result,
            } => {
                self.handle_terminal_pulse_watcher_started(workspace_id, generation, result)
                    .await
            }
            ServerCommand::TerminalPulseWatcherFailed {
                workspace_id,
                watcher_generation,
                error,
            } => {
                self.handle_terminal_pulse_watcher_failed(&workspace_id, watcher_generation, &error)
            }
            ServerCommand::TerminalPulseDue {
                session_id,
                session_instance_id,
                generation,
            } => self.handle_terminal_pulse_due(session_id, session_instance_id, generation),
            ServerCommand::ProjectCloneChanged { job_id } => {
                self.handle_project_clone_changed(job_id)
            }
            ServerCommand::ProjectCloneFinished { job_id } => {
                self.handle_project_clone_finished(job_id).await
            }
            ServerCommand::CoordinatorTick { run_id } => self.handle_coordinator_tick(run_id).await,
            ServerCommand::CoordinatorDriftProbed { run_id, drift } => {
                self.handle_coordinator_drift_probed(run_id, drift).await
            }
            ServerCommand::ResourceSampleTick => self.handle_resource_sample_tick(),
            ServerCommand::ResourceSampleReady { snapshot } => {
                self.handle_resource_sample_ready(snapshot)
            }
            ServerCommand::AutomationTick => self.handle_automation_tick().await,
            ServerCommand::CodexMessage { .. } => {}
            ServerCommand::CodexMalformed { reason } => {
                tracing::warn!(reason, "Codex app-server returned malformed JSON");
            }
            ServerCommand::CodexProcessExited { reason } => {
                tracing::warn!(reason, "Codex app-server exited");
                self.codex = None;
                self.codex_starting = None;
            }
            ServerCommand::Account(command) => self.handle_account_command(command).await,
            ServerCommand::Push(command) => self.handle_push_command(command),
        }
    }
}
