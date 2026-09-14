use crate::agent_descriptor::{
    agent_descriptor, AgentDescriptor, AgentLaunchRule, AgentLaunchRuleKind, AgentModelOverride,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[path = "managed_agent_launch_args.rs"]
mod managed_agent_launch_args;
use managed_agent_launch_args::{
    bool_value, enum_value, push_enum, push_flag, push_non_negative_integer, push_positive_number,
    push_string, push_string_option, require_known_keys, string_value,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedAgentLaunch {
    pub executable: String,
    pub arguments: Vec<String>,
}

fn push_model(
    descriptor: &AgentDescriptor,
    values: &Map<String, Value>,
    arguments: &mut Vec<String>,
) -> Result<(), String> {
    match descriptor.model_override {
        AgentModelOverride::Supported => push_string(values, "model", "--model", arguments),
        AgentModelOverride::ProfileOnly | AgentModelOverride::Unsupported => Ok(()),
    }
}

fn apply_launch_rule(
    descriptor: &AgentDescriptor,
    values: &Map<String, Value>,
    rule: &AgentLaunchRule,
    arguments: &mut Vec<String>,
) -> Result<(), String> {
    if let Some(suppressed_by) = rule.suppressed_by {
        if bool_value(values, suppressed_by)? == Some(true) {
            return Ok(());
        }
    }

    match rule.kind {
        AgentLaunchRuleKind::StringOption { flag } => {
            push_string_option(values, rule.key, flag, arguments)
        }
        AgentLaunchRuleKind::EnumOption { flag, allowed } => {
            push_enum(values, rule.key, flag, allowed, arguments)
        }
        AgentLaunchRuleKind::BoolFlag { flag } => push_flag(values, rule.key, flag, arguments),
        AgentLaunchRuleKind::NumberOption {
            flag,
            positive_only,
        } => {
            if positive_only {
                push_positive_number(values, rule.key, flag, arguments)
            } else {
                push_non_negative_integer(values, rule.key, flag, arguments)
            }
        }
        AgentLaunchRuleKind::ConfigKv { rust_key, allowed } => {
            if let Some(value) = enum_value(values, rule.key, allowed)? {
                arguments.extend(["--config".to_string(), format!("{rust_key}={value}")]);
            }
            Ok(())
        }
        AgentLaunchRuleKind::EnumToFlag { allowed, flags } => {
            if let Some(value) = enum_value(values, rule.key, allowed)? {
                let flag = flags
                    .iter()
                    .find(|(allowed_value, _)| *allowed_value == value)
                    .map(|(_, flag)| *flag)
                    .ok_or_else(|| format!("unsupported {}: {value}", rule.key))?;
                arguments.push(flag.to_string());
            }
            Ok(())
        }
        AgentLaunchRuleKind::ExclusiveToggle { flag, conflicts } => {
            if bool_value(values, rule.key)? == Some(true) {
                if conflicts
                    .iter()
                    .any(|conflict| values.contains_key(*conflict))
                {
                    return Err(format!(
                        "{} {} conflicts with {}.",
                        descriptor.display_name,
                        rule.key,
                        conflicts.join(" and ")
                    ));
                }
                arguments.push(flag.to_string());
            }
            Ok(())
        }
    }
}

pub fn build_managed_agent_launch(
    agent_type: &str,
    config: &Value,
) -> Result<ManagedAgentLaunch, String> {
    let descriptor = agent_descriptor(agent_type)
        .ok_or_else(|| format!("unsupported agent type: {agent_type}"))?;
    build_managed_agent_launch_for_descriptor(descriptor, config)
}

fn build_managed_agent_launch_for_descriptor(
    descriptor: &AgentDescriptor,
    config: &Value,
) -> Result<ManagedAgentLaunch, String> {
    let values = config
        .as_object()
        .ok_or_else(|| "managedConfig must be an object.".to_string())?;
    let spec = descriptor.launch_spec;
    require_known_keys(values, spec.allowed_keys)?;

    let mut executable = descriptor.default_command.to_string();
    let mut arguments = Vec::new();
    if let Some(launcher) = spec.profile_launcher {
        if let Some(profile) = string_value(values, launcher.key)? {
            // The profile is positional, so reject values that the switcher
            // would interpret as an option or multiple arguments.
            if profile.starts_with('-') {
                return Err("ccsProfile must not start with a dash.".to_string());
            }
            if profile.split_whitespace().count() > 1 {
                return Err("ccsProfile must be a single profile name.".to_string());
            }
            arguments.push(profile.to_string());
            executable = launcher.executable.to_string();
        }
    }
    push_model(descriptor, values, &mut arguments)?;
    for rule in spec.rules {
        apply_launch_rule(descriptor, values, rule, &mut arguments)?;
    }

    Ok(ManagedAgentLaunch {
        executable,
        arguments,
    })
}

#[cfg(test)]
#[path = "managed_agent_launch_tests.rs"]
mod tests;
