//! What the mobile gateway advertises to a phone and what it admits from one.
//!
//! Separate from the terminal requests themselves: this is the surface a phone
//! is allowed to see, and it grows with every mobile feature, while the
//! terminal verbs below it do not.

use crate::terminal_host::agent_profile_capabilities::RUNTIME_HOST_AGENT_PROFILE_LAUNCH_IDEMPOTENCY_CAPABILITY;
use crate::terminal_host::ai_assist_capabilities::{
    RUNTIME_HOST_AI_ASSIST_AGENT_TITLE_CAPABILITY,
    RUNTIME_HOST_AI_ASSIST_SPEECH_MESSAGE_CAPABILITY,
    RUNTIME_HOST_AI_ASSIST_WORKSPACE_IDENTITY_CAPABILITY,
};
use crate::terminal_host::ai_dictation_capabilities::RUNTIME_HOST_REMOTE_AI_DICTATION_CAPABILITY;
use crate::terminal_host::protocol::{
    RUNTIME_HOST_AGENT_PROFILES_CAPABILITY, RUNTIME_HOST_AGENT_PROFILE_PROMPT_LAUNCH_CAPABILITY,
    RUNTIME_HOST_AGENT_QUOTA_CLAUDE_TUI_CAPABILITY, RUNTIME_HOST_AGENT_STATUS_CAPABILITY,
    RUNTIME_HOST_AI_DICTATION_BACKENDS_CAPABILITY, RUNTIME_HOST_AI_DICTATION_CAPABILITY,
    RUNTIME_HOST_AI_DICTATION_MODELS_CAPABILITY, RUNTIME_HOST_AUTOMATIONS_CAPABILITY,
    RUNTIME_HOST_BINARY_FRAMES_CAPABILITY, RUNTIME_HOST_CAPABILITY,
    RUNTIME_HOST_CODEX_RESET_CREDITS_CAPABILITY, RUNTIME_HOST_LIFECYCLE_CAPABILITY,
    RUNTIME_HOST_MANAGED_WORKSPACE_CAPABILITY, RUNTIME_HOST_MOBILE_AGENT_QUOTA_CAPABILITY,
    RUNTIME_HOST_MOBILE_CAPABILITY, RUNTIME_HOST_MOBILE_CLOUD_ENROLLMENT_CAPABILITY,
    RUNTIME_HOST_MOBILE_CODEX_WORKSPACE_FILES_CAPABILITY,
    RUNTIME_HOST_MOBILE_HOST_TOOLS_CAPABILITY, RUNTIME_HOST_MOBILE_MUTATIONS_CAPABILITY,
    RUNTIME_HOST_MOBILE_PORTABLE_SETTINGS_CAPABILITY,
    RUNTIME_HOST_MOBILE_PROJECT_MANAGEMENT_CAPABILITY,
    RUNTIME_HOST_MOBILE_PROMPT_ATTACHMENT_READ_CAPABILITY,
    RUNTIME_HOST_MOBILE_PROMPT_FILE_UPLOAD_CAPABILITY,
    RUNTIME_HOST_MOBILE_PROMPT_IMAGE_UPLOAD_CAPABILITY,
    RUNTIME_HOST_MOBILE_SIDEBAR_PARITY_CAPABILITY, RUNTIME_HOST_MOBILE_TAB_RENAME_CAPABILITY,
    RUNTIME_HOST_MOBILE_TERMINAL_TITLES_CAPABILITY, RUNTIME_HOST_RESTART_CAPABILITY,
    RUNTIME_HOST_TERMINAL_DEFERRED_INPUT_CAPABILITY, RUNTIME_HOST_TERMINAL_DRIVER_CAPABILITY,
    RUNTIME_HOST_TERMINAL_RESTART_CAPABILITY, RUNTIME_HOST_WORKSPACE_SECTIONS_CAPABILITY,
};

use super::request_route_policy::request_route_policy;

/// What `mobile.hello` tells a phone this host can do.
///
/// A different list from the one `status.get` answers with, and the one the app
/// feature-detects against. Named rather than inlined so a test can assert a
/// capability is in it: the runtime enforces an exact `MOBILE_PROTOCOL_VERSION`
/// match, so an omission here is invisible and silently leaves every phone on
/// the older code path with no version to blame.
pub(super) const MOBILE_HELLO_CAPABILITIES: &[&str] = &[
    "configurationSyncV1",
    "relayAuthorizationRenewalV1",
    RUNTIME_HOST_CAPABILITY,
    RUNTIME_HOST_MANAGED_WORKSPACE_CAPABILITY,
    RUNTIME_HOST_MOBILE_CAPABILITY,
    RUNTIME_HOST_MOBILE_CLOUD_ENROLLMENT_CAPABILITY,
    RUNTIME_HOST_MOBILE_MUTATIONS_CAPABILITY,
    RUNTIME_HOST_MOBILE_PROJECT_MANAGEMENT_CAPABILITY,
    RUNTIME_HOST_WORKSPACE_SECTIONS_CAPABILITY,
    RUNTIME_HOST_MOBILE_SIDEBAR_PARITY_CAPABILITY,
    RUNTIME_HOST_MOBILE_TAB_RENAME_CAPABILITY,
    RUNTIME_HOST_MOBILE_TERMINAL_TITLES_CAPABILITY,
    RUNTIME_HOST_MOBILE_PORTABLE_SETTINGS_CAPABILITY,
    RUNTIME_HOST_MOBILE_AGENT_QUOTA_CAPABILITY,
    RUNTIME_HOST_AGENT_QUOTA_CLAUDE_TUI_CAPABILITY,
    RUNTIME_HOST_CODEX_RESET_CREDITS_CAPABILITY,
    RUNTIME_HOST_MOBILE_HOST_TOOLS_CAPABILITY,
    RUNTIME_HOST_TERMINAL_DEFERRED_INPUT_CAPABILITY,
    RUNTIME_HOST_TERMINAL_DRIVER_CAPABILITY,
    RUNTIME_HOST_TERMINAL_RESTART_CAPABILITY,
    RUNTIME_HOST_LIFECYCLE_CAPABILITY,
    RUNTIME_HOST_RESTART_CAPABILITY,
    RUNTIME_HOST_AGENT_STATUS_CAPABILITY,
    RUNTIME_HOST_AGENT_PROFILES_CAPABILITY,
    RUNTIME_HOST_AI_ASSIST_WORKSPACE_IDENTITY_CAPABILITY,
    RUNTIME_HOST_AI_ASSIST_AGENT_TITLE_CAPABILITY,
    RUNTIME_HOST_AI_ASSIST_SPEECH_MESSAGE_CAPABILITY,
    RUNTIME_HOST_AGENT_PROFILE_PROMPT_LAUNCH_CAPABILITY,
    RUNTIME_HOST_AGENT_PROFILE_LAUNCH_IDEMPOTENCY_CAPABILITY,
    RUNTIME_HOST_BINARY_FRAMES_CAPABILITY,
    RUNTIME_HOST_MOBILE_PROMPT_IMAGE_UPLOAD_CAPABILITY,
    RUNTIME_HOST_MOBILE_PROMPT_FILE_UPLOAD_CAPABILITY,
    RUNTIME_HOST_MOBILE_PROMPT_ATTACHMENT_READ_CAPABILITY,
    RUNTIME_HOST_MOBILE_CODEX_WORKSPACE_FILES_CAPABILITY,
    RUNTIME_HOST_AUTOMATIONS_CAPABILITY,
    RUNTIME_HOST_AI_DICTATION_CAPABILITY,
    RUNTIME_HOST_AI_DICTATION_MODELS_CAPABILITY,
    RUNTIME_HOST_AI_DICTATION_BACKENDS_CAPABILITY,
    RUNTIME_HOST_REMOTE_AI_DICTATION_CAPABILITY,
];
pub(super) fn mobile_hello_capabilities(renewal_enabled: bool) -> Vec<&'static str> {
    MOBILE_HELLO_CAPABILITIES
        .iter()
        .copied()
        .filter(|capability| renewal_enabled || *capability != "relayAuthorizationRenewalV1")
        .collect()
}

pub(super) fn mobile_request_allowed(request_type: &str) -> bool {
    request_route_policy(request_type).mobile_allowed
}

#[cfg(test)]
mod mobile_codex_file_surface_tests {
    use super::*;

    #[test]
    fn advertises_and_allows_mobile_codex_file_surfaces() {
        assert!(MOBILE_HELLO_CAPABILITIES
            .contains(&RUNTIME_HOST_MOBILE_CODEX_WORKSPACE_FILES_CAPABILITY));
        assert!(
            MOBILE_HELLO_CAPABILITIES.contains(&RUNTIME_HOST_MOBILE_PROMPT_FILE_UPLOAD_CAPABILITY)
        );
        assert!(MOBILE_HELLO_CAPABILITIES
            .contains(&RUNTIME_HOST_MOBILE_PROMPT_ATTACHMENT_READ_CAPABILITY));
        for request in [
            "mobile.workspaceQuickOpen.start",
            "mobile.workspaceQuickOpen.search",
            "mobile.workspaceQuickOpen.stop",
            "mobile.workspaceFile.read",
            "mobile.promptFile.start",
            "mobile.promptFile.chunk",
            "mobile.promptFile.complete",
            "mobile.promptFile.cancel",
            "mobile.promptAttachment.read",
        ] {
            assert!(mobile_request_allowed(request), "{request}");
        }
    }

    #[test]
    fn advertises_and_allows_speech_capabilities() {
        assert!(MOBILE_HELLO_CAPABILITIES.contains(&RUNTIME_HOST_AI_DICTATION_BACKENDS_CAPABILITY));
        assert!(MOBILE_HELLO_CAPABILITIES.contains(&RUNTIME_HOST_REMOTE_AI_DICTATION_CAPABILITY));
        assert!(mobile_request_allowed("mobile.aiDictation.capabilities"));
    }
}

#[cfg(test)]
mod relay_renewal_tests {
    #[test]
    fn disabling_renewal_preserves_all_other_mobile_capabilities() {
        let enabled = super::mobile_hello_capabilities(true);
        let disabled = super::mobile_hello_capabilities(false);
        assert!(enabled.contains(&"relayAuthorizationRenewalV1"));
        assert!(!disabled.contains(&"relayAuthorizationRenewalV1"));
        assert_eq!(enabled.len(), disabled.len() + 1);
    }
}
