pub(crate) mod agent_profile_capabilities;
pub(crate) mod ai_assist_capabilities;
pub(crate) mod ai_dictation_capabilities;
#[path = "../alera_account/mod.rs"]
pub(crate) mod alera_account;
pub mod buffer;
pub mod client;
// Kept for re-enabling cloud services; the portable privacy build does not call it.
#[allow(dead_code)]
mod client_budget;
pub mod control_file;
pub mod demand_driven_ticker;
pub mod diagnostics;
pub mod frame_codec;
pub(crate) mod history_repository;
mod history_store;
pub mod host_error;
pub mod mobile_gateway;
pub mod orchestration;
pub mod protocol;
#[path = "../push_notifications/mod.rs"]
pub(crate) mod push_notifications;
// The relay modules are kept for re-enabling cloud services; the portable
// privacy build does not start the Internet relay.
#[allow(dead_code)]
mod relay_connection;
#[allow(dead_code)]
pub mod relay_crypto;
#[allow(dead_code)]
mod relay_runtime;
#[allow(dead_code)]
mod relay_runtime_auth;
#[allow(dead_code)]
pub mod relay_wire;
pub mod resources;
pub(crate) mod restart;
pub(crate) mod runtime_build_info;
pub(crate) mod runtime_owner;
pub mod server;
pub mod session;
pub mod sleep_detector;
