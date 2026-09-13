use alera_core::agent_descriptor::{
    canonical_agent_id as core_canonical_agent_id, AgentDescriptor, AgentHookStrategy,
    AgentModelOverride, AgentStartupPrompt, AgentStatusStrategy, AGENT_DESCRIPTORS,
};

pub struct AgentRiskRuleDto {
    pub key: String,
    pub expected_bool: Option<bool>,
    pub expected_str: Option<String>,
    pub marker: String,
    pub score: u32,
}

pub struct AgentDescriptorDto {
    pub id: String,
    pub aliases: Vec<String>,
    pub display_name: String,
    pub default_command: String,
    pub force_submit: bool,
    pub interrupt_bytes: Vec<u8>,
    pub startup_prompt: String,
    pub startup_prompt_flag: Option<String>,
    pub hook_strategy: String,
    pub status_strategy: String,
    pub quota_provider_id: Option<String>,
    pub transcript_usage: bool,
    pub model_override: String,
    pub supports_persona: bool,
    pub supports_ccs_profile: bool,
    pub risk_warning: String,
    pub risk_warning_severe: Option<String>,
    pub risk_rules: Vec<AgentRiskRuleDto>,
}

pub struct ManagedAgentLaunchDto {
    pub executable: String,
    pub arguments: Vec<String>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn agent_descriptors() -> Vec<AgentDescriptorDto> {
    AGENT_DESCRIPTORS.iter().map(agent_descriptor_dto).collect()
}

#[flutter_rust_bridge::frb(sync)]
pub fn canonical_agent_id(id: String) -> Option<String> {
    core_canonical_agent_id(&id).map(|canonical| canonical.to_string())
}

#[flutter_rust_bridge::frb(sync)]
pub fn preview_managed_agent_launch(
    agent_type: String,
    config_json: String,
) -> Result<ManagedAgentLaunchDto, String> {
    let config = serde_json::from_str::<serde_json::Value>(&config_json)
        .map_err(|error| error.to_string())?;
    alera_core::managed_agent_launch::build_managed_agent_launch(&agent_type, &config).map(
        |launch| ManagedAgentLaunchDto {
            executable: launch.executable,
            arguments: launch.arguments,
        },
    )
}

fn agent_descriptor_dto(descriptor: &AgentDescriptor) -> AgentDescriptorDto {
    let (startup_prompt, startup_prompt_flag) = match descriptor.startup_prompt {
        AgentStartupPrompt::PositionalAfterTerminator => ("positional_after_terminator", None),
        AgentStartupPrompt::Positional => ("positional", None),
        AgentStartupPrompt::LongOption(flag) => ("long_option", Some(flag.to_string())),
        AgentStartupPrompt::StdinScript => ("stdin_script", None),
        AgentStartupPrompt::TerminalAfterReady => ("terminal_after_ready", None),
    };
    let hook_strategy = match descriptor.hook_strategy {
        AgentHookStrategy::None => "none",
        AgentHookStrategy::ConfigJson => "config_json",
        AgentHookStrategy::PluginScript => "plugin_script",
        AgentHookStrategy::RuntimeHome => "runtime_home",
        AgentHookStrategy::SessionOverlay => "session_overlay",
        AgentHookStrategy::HerdrSocket => "herdr_socket",
    };
    let status_strategy = match descriptor.status_strategy {
        AgentStatusStrategy::HookEvents => "hook_events",
        AgentStatusStrategy::HookEventsWithTranscriptWatch => "hook_events_with_transcript_watch",
        AgentStatusStrategy::HerdrSocket => "herdr_socket",
    };
    let model_override = match descriptor.model_override {
        AgentModelOverride::Supported => "supported",
        AgentModelOverride::Unsupported => "unsupported",
        AgentModelOverride::ProfileOnly => "profile_only",
    };

    AgentDescriptorDto {
        id: descriptor.id.to_string(),
        aliases: descriptor
            .aliases
            .iter()
            .map(|alias| (*alias).to_string())
            .collect(),
        display_name: descriptor.display_name.to_string(),
        default_command: descriptor.default_command.to_string(),
        force_submit: descriptor.force_submit,
        interrupt_bytes: descriptor.interrupt_bytes.to_vec(),
        startup_prompt: startup_prompt.to_string(),
        startup_prompt_flag,
        hook_strategy: hook_strategy.to_string(),
        status_strategy: status_strategy.to_string(),
        quota_provider_id: descriptor
            .quota_provider_id
            .map(|provider| provider.to_string()),
        transcript_usage: descriptor.transcript_usage,
        model_override: model_override.to_string(),
        supports_persona: descriptor.supports_persona,
        supports_ccs_profile: descriptor.supports_ccs_profile,
        risk_warning: descriptor.risk_warning.to_string(),
        risk_warning_severe: descriptor
            .risk_warning_severe
            .map(|warning| warning.to_string()),
        risk_rules: descriptor
            .risk_rules
            .iter()
            .map(|rule| AgentRiskRuleDto {
                key: rule.key.to_string(),
                expected_bool: rule.expected_bool,
                expected_str: rule.expected_str.map(|value| value.to_string()),
                marker: rule.marker.to_string(),
                score: rule.score,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_the_static_descriptor_order() {
        let descriptors = agent_descriptors();
        assert_eq!(descriptors.len(), 12);
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.id.as_str())
                .collect::<Vec<_>>(),
            [
                "codex",
                "claude",
                "copilot",
                "cursor",
                "agy",
                "opencode",
                "opencode2",
                "pi",
                "amp",
                "grok",
                "devin",
                "fx",
            ]
        );
    }

    #[test]
    fn canonicalizes_agent_ids_and_aliases() {
        assert_eq!(
            canonical_agent_id("antigravity".to_string()),
            Some("agy".to_string())
        );
        assert_eq!(
            canonical_agent_id("codex".to_string()),
            Some("codex".to_string())
        );
        assert_eq!(canonical_agent_id("unknown".to_string()), None);
    }

    #[test]
    fn previews_managed_launch_arguments() {
        let launch =
            preview_managed_agent_launch("codex".to_string(), r#"{"model":"x"}"#.to_string())
                .unwrap();
        let model_index = launch
            .arguments
            .iter()
            .position(|argument| argument == "--model")
            .unwrap();
        assert_eq!(
            launch.arguments.get(model_index + 1),
            Some(&"x".to_string())
        );
        assert_eq!(launch.executable, "codex");
    }

    #[test]
    fn rejects_invalid_preview_input() {
        assert!(preview_managed_agent_launch("codex".to_string(), "{".to_string()).is_err());
    }
}
