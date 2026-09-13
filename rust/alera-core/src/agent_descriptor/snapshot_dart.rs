use std::fmt::Write;

use super::{
    AgentDescriptor, AgentHookStrategy, AgentLaunchRule, AgentLaunchRuleKind, AgentModelOverride,
    AgentRiskRule, AgentStartupPrompt, AgentStatusStrategy, AGENT_DESCRIPTORS,
};

// `dart format off` keeps the emitted output byte-stable: the freshness test
// compares this string to the committed file, which dart format would otherwise
// rewrap and desync.
const HEADER: &str = "// GENERATED FILE. Do not edit by hand.\n// Regenerate with: cargo run -p alera-cli -- export-agent-descriptors\n// dart format off\n\n";

/// Renders the Rust descriptor table as the checked-in Dart snapshot used by
/// Dart tests and code that cannot load the native library.
pub fn emit() -> String {
    let mut output = String::from(HEADER);
    output.push_str(DART_TYPES);
    output.push_str("\nconst List<AgentDescriptorSnapshot> agentDescriptorSnapshots = <AgentDescriptorSnapshot>[\n");
    for descriptor in AGENT_DESCRIPTORS {
        emit_descriptor(&mut output, descriptor);
    }
    output.push_str("];\n");
    output
}

const DART_TYPES: &str = r#"enum AgentStartupPromptKindSnapshot {
  positionalAfterTerminator,
  positional,
  longOption,
  stdinScript,
  terminalAfterReady,
}

enum AgentHookStrategySnapshot {
  none,
  configJson,
  pluginScript,
  runtimeHome,
  sessionOverlay,
  herdrSocket,
}

enum AgentStatusStrategySnapshot {
  hookEvents,
  hookEventsWithTranscriptWatch,
  herdrSocket,
}

enum AgentModelOverrideSnapshot {
  supported,
  unsupported,
  profileOnly,
}

enum AgentLaunchRuleKindSnapshot {
  stringOption,
  enumOption,
  boolFlag,
  numberOption,
  configKv,
  enumToFlag,
  exclusiveToggle,
}

class AgentStartupPromptSnapshot {
  const AgentStartupPromptSnapshot({
    required this.kind,
    required this.option,
  });

  final AgentStartupPromptKindSnapshot kind;
  final String? option;
}

class AgentRiskRuleSnapshot {
  const AgentRiskRuleSnapshot({
    required this.key,
    required this.expectedBool,
    required this.expectedString,
    required this.marker,
    required this.score,
  });

  final String key;
  final bool? expectedBool;
  final String? expectedString;
  final String marker;
  final int score;
}

class AgentProfileLauncherSnapshot {
  const AgentProfileLauncherSnapshot({
    required this.key,
    required this.executable,
  });

  final String key;
  final String executable;
}

class AgentLaunchRuleSpec {
  const AgentLaunchRuleSpec({
    required this.key,
    required this.suppressedBy,
    required this.kind,
    required this.flag,
    required this.allowed,
    required this.positiveOnly,
    required this.rustKey,
    required this.flags,
    required this.conflicts,
  });

  final String key;
  final String? suppressedBy;
  final AgentLaunchRuleKindSnapshot kind;
  final String? flag;
  final List<String> allowed;
  final bool positiveOnly;
  final String? rustKey;
  final Map<String, String> flags;
  final List<String> conflicts;
}

class AgentLaunchSpecSnapshot {
  const AgentLaunchSpecSnapshot({
    required this.allowedKeys,
    required this.profileLauncher,
    required this.rules,
  });

  final List<String> allowedKeys;
  final AgentProfileLauncherSnapshot? profileLauncher;
  final List<AgentLaunchRuleSpec> rules;
}

class AgentDescriptorSnapshot {
  const AgentDescriptorSnapshot({
    required this.id,
    required this.aliases,
    required this.displayName,
    required this.defaultCommand,
    required this.forceSubmit,
    required this.interruptBytes,
    required this.startupPrompt,
    required this.hookStrategy,
    required this.statusStrategy,
    required this.quotaProviderId,
    required this.transcriptUsage,
    required this.modelOverride,
    required this.supportsPersona,
    required this.supportsCcsProfile,
    required this.riskWarning,
    required this.riskWarningSevere,
    required this.riskRules,
    required this.launchSpec,
  });

  final String id;
  final List<String> aliases;
  final String displayName;
  final String defaultCommand;
  final bool forceSubmit;
  final List<int> interruptBytes;
  final AgentStartupPromptSnapshot startupPrompt;
  final AgentHookStrategySnapshot hookStrategy;
  final AgentStatusStrategySnapshot statusStrategy;
  final String? quotaProviderId;
  final bool transcriptUsage;
  final AgentModelOverrideSnapshot modelOverride;
  final bool supportsPersona;
  final bool supportsCcsProfile;
  final String riskWarning;
  final String? riskWarningSevere;
  final List<AgentRiskRuleSnapshot> riskRules;
  final AgentLaunchSpecSnapshot launchSpec;
}
"#;

fn emit_descriptor(output: &mut String, descriptor: &AgentDescriptor) {
    let (startup_kind, startup_option) = startup_prompt(descriptor.startup_prompt);
    let startup_prompt = format!(
        "AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.{startup_kind}, option: {})",
        dart_optional_string(startup_option)
    );
    writeln!(
        output,
        "  AgentDescriptorSnapshot(id: {}, aliases: {}, displayName: {}, defaultCommand: {}, forceSubmit: {}, interruptBytes: {}, startupPrompt: {startup_prompt}, hookStrategy: AgentHookStrategySnapshot.{}, statusStrategy: AgentStatusStrategySnapshot.{}, quotaProviderId: {}, transcriptUsage: {}, modelOverride: AgentModelOverrideSnapshot.{}, supportsPersona: {}, supportsCcsProfile: {}, riskWarning: {}, riskWarningSevere: {}, riskRules: {}, launchSpec: {}),",
        dart_string(descriptor.id),
        dart_string_list(descriptor.aliases),
        dart_string(descriptor.display_name),
        dart_string(descriptor.default_command),
        dart_bool(descriptor.force_submit),
        dart_byte_list(descriptor.interrupt_bytes),
        hook_strategy(descriptor.hook_strategy),
        status_strategy(descriptor.status_strategy),
        dart_optional_string(descriptor.quota_provider_id),
        dart_bool(descriptor.transcript_usage),
        model_override(descriptor.model_override),
        dart_bool(descriptor.supports_persona),
        dart_bool(descriptor.supports_ccs_profile),
        dart_string(descriptor.risk_warning),
        dart_optional_string(descriptor.risk_warning_severe),
        dart_risk_rules(descriptor.risk_rules),
        dart_launch_spec(descriptor.launch_spec),
    )
    .unwrap();
}

fn dart_risk_rules(rules: &[AgentRiskRule]) -> String {
    let rules = rules
        .iter()
        .map(|rule| {
            format!(
                "AgentRiskRuleSnapshot(key: {}, expectedBool: {}, expectedString: {}, marker: {}, score: {})",
                dart_string(rule.key),
                dart_optional_bool(rule.expected_bool),
                dart_optional_string(rule.expected_str),
                dart_string(rule.marker),
                rule.score,
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("<AgentRiskRuleSnapshot>[{rules}]")
}

fn dart_launch_spec(spec: super::AgentLaunchSpec) -> String {
    let profile_launcher = spec
        .profile_launcher
        .map(|launcher| {
            format!(
                "AgentProfileLauncherSnapshot(key: {}, executable: {})",
                dart_string(launcher.key),
                dart_string(launcher.executable),
            )
        })
        .unwrap_or_else(|| "null".to_string());
    let rules = spec
        .rules
        .iter()
        .map(dart_launch_rule)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "AgentLaunchSpecSnapshot(allowedKeys: {}, profileLauncher: {profile_launcher}, rules: <AgentLaunchRuleSpec>[{rules}])",
        dart_string_list(spec.allowed_keys),
    )
}

fn dart_launch_rule(rule: &AgentLaunchRule) -> String {
    let (kind, flag, allowed, positive_only, rust_key, flags, conflicts) = match rule.kind {
        AgentLaunchRuleKind::StringOption { flag } => (
            "stringOption",
            Some(flag),
            &[][..],
            false,
            None,
            &[][..],
            &[][..],
        ),
        AgentLaunchRuleKind::EnumOption { flag, allowed } => (
            "enumOption",
            Some(flag),
            allowed,
            false,
            None,
            &[][..],
            &[][..],
        ),
        AgentLaunchRuleKind::BoolFlag { flag } => (
            "boolFlag",
            Some(flag),
            &[][..],
            false,
            None,
            &[][..],
            &[][..],
        ),
        AgentLaunchRuleKind::NumberOption {
            flag,
            positive_only,
        } => (
            "numberOption",
            Some(flag),
            &[][..],
            positive_only,
            None,
            &[][..],
            &[][..],
        ),
        AgentLaunchRuleKind::ConfigKv { rust_key, allowed } => (
            "configKv",
            None,
            allowed,
            false,
            Some(rust_key),
            &[][..],
            &[][..],
        ),
        AgentLaunchRuleKind::EnumToFlag { allowed, flags } => {
            ("enumToFlag", None, allowed, false, None, flags, &[][..])
        }
        AgentLaunchRuleKind::ExclusiveToggle { flag, conflicts } => (
            "exclusiveToggle",
            Some(flag),
            &[][..],
            false,
            None,
            &[][..],
            conflicts,
        ),
    };
    format!(
        "AgentLaunchRuleSpec(key: {}, suppressedBy: {}, kind: AgentLaunchRuleKindSnapshot.{kind}, flag: {}, allowed: {}, positiveOnly: {}, rustKey: {}, flags: {}, conflicts: {})",
        dart_string(rule.key),
        dart_optional_string(rule.suppressed_by),
        dart_optional_string(flag),
        dart_string_list(allowed),
        dart_bool(positive_only),
        dart_optional_string(rust_key),
        dart_flags(flags),
        dart_string_list(conflicts),
    )
}

fn startup_prompt(prompt: AgentStartupPrompt) -> (&'static str, Option<&'static str>) {
    match prompt {
        AgentStartupPrompt::PositionalAfterTerminator => ("positionalAfterTerminator", None),
        AgentStartupPrompt::Positional => ("positional", None),
        AgentStartupPrompt::LongOption(option) => ("longOption", Some(option)),
        AgentStartupPrompt::StdinScript => ("stdinScript", None),
        AgentStartupPrompt::TerminalAfterReady => ("terminalAfterReady", None),
    }
}

fn hook_strategy(strategy: AgentHookStrategy) -> &'static str {
    match strategy {
        AgentHookStrategy::None => "none",
        AgentHookStrategy::ConfigJson => "configJson",
        AgentHookStrategy::PluginScript => "pluginScript",
        AgentHookStrategy::RuntimeHome => "runtimeHome",
        AgentHookStrategy::SessionOverlay => "sessionOverlay",
        AgentHookStrategy::HerdrSocket => "herdrSocket",
    }
}

fn status_strategy(strategy: AgentStatusStrategy) -> &'static str {
    match strategy {
        AgentStatusStrategy::HookEvents => "hookEvents",
        AgentStatusStrategy::HookEventsWithTranscriptWatch => "hookEventsWithTranscriptWatch",
        AgentStatusStrategy::HerdrSocket => "herdrSocket",
    }
}

fn model_override(override_kind: AgentModelOverride) -> &'static str {
    match override_kind {
        AgentModelOverride::Supported => "supported",
        AgentModelOverride::Unsupported => "unsupported",
        AgentModelOverride::ProfileOnly => "profileOnly",
    }
}

fn dart_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('\'');
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            '$' => escaped.push_str("\\$"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character => escaped.push(character),
        }
    }
    escaped.push('\'');
    escaped
}

fn dart_optional_string(value: Option<&str>) -> String {
    value.map(dart_string).unwrap_or_else(|| "null".to_string())
}

fn dart_string_list(values: &[&str]) -> String {
    let values = values
        .iter()
        .map(|value| dart_string(value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("<String>[{values}]")
}

fn dart_byte_list(values: &[u8]) -> String {
    let values = values
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!("<int>[{values}]")
}

fn dart_flags(values: &[(&str, &str)]) -> String {
    let values = values
        .iter()
        .map(|(key, value)| format!("{}: {}", dart_string(key), dart_string(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("<String, String>{{{values}}}")
}

fn dart_bool(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn dart_optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(value) => dart_bool(value),
        None => "null",
    }
}
