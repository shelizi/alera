use alera_core::runtime::{
    RuntimeAgentQuotaSettings, RuntimeAgentStatusHookSettings, RuntimeAiAssistSettings,
    RuntimeAutomationSettings, RuntimeMobilePushSettings, RuntimeTextActionsSettings,
};
use serde_json::Value;

use crate::terminal_host::host_error::{HostError, HostResult};

const RUNTIME_SETTINGS_KEYS: [&str; 11] = [
    "workspaceDirectory",
    "confirmProjectRemoval",
    "confirmWorkspaceRemoval",
    "autoArchiveWorkspacesAfterDays",
    "defaultAgentProfileId",
    "agentStatusHooks",
    "agentQuotas",
    "mobilePushNotifications",
    "aiTextGeneration",
    "textActions",
    "automation",
];
const MOBILE_RUNTIME_SETTINGS_KEYS: [&str; 9] = [
    "workspaceDirectory",
    "confirmProjectRemoval",
    "confirmWorkspaceRemoval",
    "autoArchiveWorkspacesAfterDays",
    "defaultAgentProfileId",
    "agentStatusHooks",
    "agentQuotas",
    "mobilePushNotifications",
    "automation",
];

pub(super) struct RuntimeSettingsUpdate {
    pub(super) expected_revision: Option<i64>,
    pub(super) workspace_directory: Option<Option<String>>,
    pub(super) confirm_project_removal: Option<bool>,
    pub(super) confirm_workspace_removal: Option<bool>,
    pub(super) auto_archive_workspaces_after_days: Option<i64>,
    pub(super) default_agent_profile_id: Option<Option<String>>,
    pub(super) agent_status_hooks: Option<RuntimeAgentStatusHookSettings>,
    pub(super) agent_quotas: Option<RuntimeAgentQuotaSettings>,
    pub(super) mobile_push_notifications: Option<RuntimeMobilePushSettings>,
    pub(super) ai_assist: Option<RuntimeAiAssistSettings>,
    pub(super) text_actions: Option<RuntimeTextActionsSettings>,
    pub(super) automation: Option<RuntimeAutomationSettings>,
}

impl RuntimeSettingsUpdate {
    pub(super) fn has_settings(&self) -> bool {
        self.workspace_directory.is_some()
            || self.confirm_project_removal.is_some()
            || self.confirm_workspace_removal.is_some()
            || self.auto_archive_workspaces_after_days.is_some()
            || self.default_agent_profile_id.is_some()
            || self.agent_status_hooks.is_some()
            || self.agent_quotas.is_some()
            || self.mobile_push_notifications.is_some()
            || self.ai_assist.is_some()
            || self.text_actions.is_some()
            || self.automation.is_some()
    }
}

pub(crate) fn validate_mobile_runtime_settings_payload(payload: &Value) -> HostResult<()> {
    let object = payload
        .as_object()
        .ok_or_else(|| HostError::format("Runtime settings payload must be a JSON object."))?;
    if let Some(key) = object.keys().find(|key| {
        *key != "expectedRevision" && !MOBILE_RUNTIME_SETTINGS_KEYS.contains(&key.as_str())
    }) {
        return Err(HostError::format(format!(
            "Unsupported mobile setting: {key}."
        )));
    }
    Ok(())
}

pub(super) fn parse_runtime_settings_update(payload: &Value) -> HostResult<RuntimeSettingsUpdate> {
    let object = payload
        .as_object()
        .ok_or_else(|| HostError::format("Runtime settings payload must be a JSON object."))?;
    if let Some(key) = object
        .keys()
        .find(|key| *key != "expectedRevision" && !RUNTIME_SETTINGS_KEYS.contains(&key.as_str()))
    {
        return Err(HostError::format(format!(
            "Unsupported runtime setting: {key}."
        )));
    }

    let expected_revision = match object.get("expectedRevision") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_i64()
                .filter(|revision| *revision >= 0)
                .ok_or_else(|| {
                    HostError::format("expectedRevision must be a non-negative integer.")
                })?,
        ),
    };
    let workspace_directory = object
        .get("workspaceDirectory")
        .map(|value| match value {
            Value::String(value) => Ok(Some(value.clone())),
            Value::Null => Ok(None),
            _ => Err(HostError::format(
                "workspaceDirectory must be a string or null.",
            )),
        })
        .transpose()?;
    let confirm_project_removal = object
        .get("confirmProjectRemoval")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| HostError::format("confirmProjectRemoval must be a boolean."))
        })
        .transpose()?;
    let confirm_workspace_removal = object
        .get("confirmWorkspaceRemoval")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| HostError::format("confirmWorkspaceRemoval must be a boolean."))
        })
        .transpose()?;
    let auto_archive_workspaces_after_days = object
        .get("autoArchiveWorkspacesAfterDays")
        .map(|value| {
            let days = value.as_i64().ok_or_else(|| {
                HostError::format("autoArchiveWorkspacesAfterDays must be an integer.")
            })?;
            if days < 0 {
                return Err(HostError::format(
                    "autoArchiveWorkspacesAfterDays must be zero or greater.",
                ));
            }
            Ok(days)
        })
        .transpose()?;
    let default_agent_profile_id = object
        .get("defaultAgentProfileId")
        .map(|value| match value {
            Value::String(value) => Ok(Some(value.clone())),
            Value::Null => Ok(None),
            _ => Err(HostError::format(
                "defaultAgentProfileId must be a string or null.",
            )),
        })
        .transpose()?;
    let agent_status_hooks = object
        .get("agentStatusHooks")
        .map(|value| {
            serde_json::from_value::<RuntimeAgentStatusHookSettings>(value.clone()).map_err(|_| {
                HostError::format("agentStatusHooks must contain boolean agent switches.")
            })
        })
        .transpose()?;
    let agent_quotas = object
        .get("agentQuotas")
        .map(|value| {
            let settings: RuntimeAgentQuotaSettings = serde_json::from_value(value.clone())
                .map_err(|_| HostError::format("agentQuotas is invalid."))?;
            validate_agent_quota_settings(&settings)?;
            Ok(settings)
        })
        .transpose()?;
    let mobile_push_notifications = object
        .get("mobilePushNotifications")
        .map(|value| {
            let mut settings: RuntimeMobilePushSettings = serde_json::from_value(value.clone())
                .map_err(|_| HostError::format("mobilePushNotifications is invalid."))?;
            // Privacy portable build: preserve the local setting shape but never
            // allow cloud push to become active.
            settings.enabled = false;
            Ok(settings)
        })
        .transpose()?;
    let ai_assist = object
        .get("aiTextGeneration")
        .map(|value| {
            let settings: RuntimeAiAssistSettings = serde_json::from_value(value.clone())
                .map_err(|_| HostError::format("AI Assist settings are invalid."))?;
            validate_ai_assist_settings(&settings)?;
            Ok(settings)
        })
        .transpose()?;
    let text_actions = object
        .get("textActions")
        .map(|value| {
            let settings: RuntimeTextActionsSettings = serde_json::from_value(value.clone())
                .map_err(|_| HostError::format("textActions is invalid."))?;
            validate_text_actions_settings(&settings)?;
            Ok(settings)
        })
        .transpose()?;
    let automation = object
        .get("automation")
        .map(|value| {
            serde_json::from_value::<RuntimeAutomationSettings>(value.clone())
                .map_err(|_| HostError::format("automation settings are invalid."))
        })
        .transpose()?;

    Ok(RuntimeSettingsUpdate {
        expected_revision,
        workspace_directory,
        confirm_project_removal,
        confirm_workspace_removal,
        auto_archive_workspaces_after_days,
        default_agent_profile_id,
        agent_status_hooks,
        agent_quotas,
        mobile_push_notifications,
        ai_assist,
        text_actions,
        automation,
    })
}

fn validate_ai_assist_settings(settings: &RuntimeAiAssistSettings) -> HostResult<()> {
    alera_core::runtime::validate_ai_assist_settings(settings)
        .map_err(|error| HostError::format(error.to_string()))
}

pub(crate) fn validate_text_actions_settings(
    settings: &RuntimeTextActionsSettings,
) -> HostResult<()> {
    alera_core::runtime::validate_text_actions_settings(settings)
        .map_err(|error| HostError::format(error.to_string()))
}

fn validate_agent_quota_settings(settings: &RuntimeAgentQuotaSettings) -> HostResult<()> {
    let mut aliases = std::collections::HashSet::new();
    let mut profiles = std::collections::HashSet::new();
    for profile in &settings.claude_profiles {
        let alias = profile.alias.trim();
        let profile_name = profile.profile.trim();
        if alias.is_empty() || profile_name.is_empty() {
            return Err(HostError::format(
                "Claude aliases and profiles are required.",
            ));
        }
        if !aliases.insert(alias) || !profiles.insert(profile_name) {
            return Err(HostError::format(
                "Claude aliases and profiles must be unique.",
            ));
        }
    }
    for name in [
        &settings.environment.kimi_api_key,
        &settings.environment.zai_api_key,
        &settings.environment.zai_base_url,
        &settings.environment.minimax_api_key,
        &settings.environment.minimax_api_host,
    ] {
        if !valid_environment_name(name) {
            return Err(HostError::format(format!(
                "Invalid environment variable name: {name}."
            )));
        }
    }
    Ok(())
}

fn valid_environment_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|char| char == '_' || char.is_ascii_alphanumeric())
}
