//! Declarative per-agent metadata shared by the terminal host and (via FRB)
//! the Dart UI layer.
//!
//! This table is the single source of truth for what each agent adapter is and
//! which strategies it uses. It deliberately stays free of runtime state and
//! feature gates so both `alera-cli` and `alera_native` can read it. Consumers
//! dispatch on the strategy fields instead of matching on the agent id.

/// How a freshly launched agent CLI receives its opening prompt.
///
/// Most spawnable agents get their prompt at launch. `fx` is the exception: it
/// has no interactive initial-prompt argument and its built-in Herdr integration
/// emits an idle event after the TUI is ready, so the host can paste safely then.
/// Print/execute flags (`-p`, `--print`, `-x`) are deliberately unused, since
/// they answer once and exit instead of leaving an agent in the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStartupPrompt {
    /// Positional, preceded by the standard option terminator so a prompt
    /// starting with a dash is not read as another option.
    PositionalAfterTerminator,
    /// Positional, with no terminator available. `pi` rejects `--` outright
    /// (`Error: Unknown option: --`), so the terminator cannot be used, and a
    /// dash-prefixed prompt has to be defused another way.
    Positional,
    /// A long option carrying the prompt as a single `--flag=<prompt>` token,
    /// which keeps a dash-prefixed prompt out of the parser's way without a
    /// terminator.
    LongOption(&'static str),
    /// The CLI has no interactive initial-prompt argument at all. `amp` only
    /// accepts an opening message on stdin, so the prompt is fed from a file
    /// through a generated launcher script.
    StdinScript,
    /// The CLI starts only without a prompt. A semantic ready event tells the
    /// host when it is safe to paste and submit the opening prompt in the PTY.
    TerminalAfterReady,
}

/// How the app installs status-reporting hooks for an agent CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentHookStrategy {
    /// No hook installation at all; the agent has no status integration files.
    None,
    /// Hooks are merged into the agent's own JSON config file, e.g.
    /// `~/.gemini/config/hooks.json` or Devin's `config.json`.
    ConfigJson,
    /// A managed plugin/extension file carries the hooks (opencode, opencode2,
    /// pi, amp).
    PluginScript,
    /// An isolated `agent-runtime-homes/<agent>/home` with merged config
    /// (codex, claude).
    RuntimeHome,
    /// A per-session overlay directory passed via launch flags (cursor's
    /// `--plugin-dir`).
    SessionOverlay,
    /// Status flows over `fx-herdr.sock`; no files are installed (fx).
    HerdrSocket,
}

/// How the host learns an agent's working/waiting state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatusStrategy {
    /// Loopback HTTP hook events normalized into canonical states.
    HookEvents,
    /// Hook events plus a session-transcript watcher that catches exits that
    /// never fire a `Stop` hook (codex).
    HookEventsWithTranscriptWatch,
    /// Status arrives over the fx Herdr unix socket (Unix only).
    HerdrSocket,
}

/// Whether a managed profile's `model` field reaches the launch command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentModelOverride {
    /// `--model <id>` is appended to the launch arguments.
    Supported,
    /// The CLI has no model flag; the profile model field is rejected.
    Unsupported,
    /// The profile schema accepts `model` but interactive launch suppresses it
    /// (opencode2 accepts the key for schema symmetry).
    ProfileOnly,
}

/// One declarative risk rule evaluated against a managed profile's config map.
/// Exactly one of `expected_bool` / `expected_str` is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentRiskRule {
    /// Profile config key evaluated, e.g. `permissionMode`.
    pub key: &'static str,
    /// The rule hits when the config value equals this boolean.
    pub expected_bool: Option<bool>,
    /// The rule hits when the config value equals this string.
    pub expected_str: Option<&'static str>,
    /// Marker reported when the rule hits; drives UI badges.
    pub marker: &'static str,
    /// Score contribution when the rule hits.
    pub score: u32,
}

/// Static capability and strategy metadata for one spawnable agent adapter.
///
/// Quota-only providers (kimi, minimax, zai) are not listed here; they live in
/// the quota registry. `quota_provider_id` links a spawnable agent to that
/// registry when usage polling exists for it.
#[derive(Debug)]
pub struct AgentDescriptor {
    /// Canonical identifier used on the wire and in settings keys.
    pub id: &'static str,
    /// Legacy/alternate identifiers that resolve to `id` (e.g. `antigravity`).
    pub aliases: &'static [&'static str],
    /// Display name shown in menus, status dots, and settings labels.
    pub display_name: &'static str,
    /// CLI binary name used when a profile does not override the command.
    pub default_command: &'static str,
    /// Whether the host force-submits the initial prompt after pasting.
    pub force_submit: bool,
    /// Bytes sent to interrupt a running agent (Ctrl-C for all current agents).
    pub interrupt_bytes: &'static [u8],
    pub startup_prompt: AgentStartupPrompt,
    pub hook_strategy: AgentHookStrategy,
    pub status_strategy: AgentStatusStrategy,
    /// Canonical quota registry id when usage polling exists for this agent.
    pub quota_provider_id: Option<&'static str>,
    /// Whether JSONL transcript usage/cost aggregation exists (claude, codex,
    /// grok).
    pub transcript_usage: bool,
    pub model_override: AgentModelOverride,
    /// Whether managed profiles support the persona field for this agent.
    pub supports_persona: bool,
    /// Whether the profile may launch through a `ccs`-style profile switcher
    /// (claude only).
    pub supports_ccs_profile: bool,
    /// Copy shown when any risk rule hits. Empty when the agent has no risky
    /// profile options.
    pub risk_warning: &'static str,
    /// Copy shown instead of `risk_warning` once the total score reaches 100
    /// (codex full-bypass tier).
    pub risk_warning_severe: Option<&'static str>,
    pub risk_rules: &'static [AgentRiskRule],
}

const CTRL_C: &[u8] = b"\x03";

const CODEX_RISK_RULES: &[AgentRiskRule] = &[
    AgentRiskRule {
        key: "bypassApprovalsAndSandbox",
        expected_bool: Some(true),
        expected_str: None,
        marker: "bypassApprovalsAndSandbox",
        score: 100,
    },
    AgentRiskRule {
        key: "sandbox",
        expected_bool: None,
        expected_str: Some("danger-full-access"),
        marker: "dangerFullAccess",
        score: 40,
    },
    AgentRiskRule {
        key: "approvalPolicy",
        expected_bool: None,
        expected_str: Some("never"),
        marker: "neverAsk",
        score: 30,
    },
];

const CLAUDE_RISK_RULES: &[AgentRiskRule] = &[
    AgentRiskRule {
        key: "permissionMode",
        expected_bool: None,
        expected_str: Some("bypassPermissions"),
        marker: "bypassPermissions",
        score: 100,
    },
    AgentRiskRule {
        key: "permissionMode",
        expected_bool: None,
        expected_str: Some("dontAsk"),
        marker: "dontAsk",
        score: 40,
    },
    AgentRiskRule {
        key: "allowSkipPermissions",
        expected_bool: Some(true),
        expected_str: None,
        marker: "allowSkipPermissions",
        score: 30,
    },
];

const COPILOT_RISK_RULES: &[AgentRiskRule] = &[
    AgentRiskRule {
        key: "allowAll",
        expected_bool: Some(true),
        expected_str: None,
        marker: "allowAll",
        score: 70,
    },
    AgentRiskRule {
        key: "mode",
        expected_bool: None,
        expected_str: Some("autopilot"),
        marker: "autopilot",
        score: 40,
    },
    AgentRiskRule {
        key: "noAskUser",
        expected_bool: Some(true),
        expected_str: None,
        marker: "noAskUser",
        score: 30,
    },
];

const CURSOR_RISK_RULES: &[AgentRiskRule] = &[
    AgentRiskRule {
        key: "permissionMode",
        expected_bool: None,
        expected_str: Some("force"),
        marker: "force",
        score: 70,
    },
    AgentRiskRule {
        key: "sandbox",
        expected_bool: None,
        expected_str: Some("disabled"),
        marker: "sandboxDisabled",
        score: 50,
    },
    AgentRiskRule {
        key: "trustWorkspace",
        expected_bool: Some(true),
        expected_str: None,
        marker: "trustWorkspace",
        score: 20,
    },
];

const AGY_RISK_RULES: &[AgentRiskRule] = &[AgentRiskRule {
    key: "skipPermissions",
    expected_bool: Some(true),
    expected_str: None,
    marker: "skipPermissions",
    score: 100,
}];

const OPENCODE_RISK_RULES: &[AgentRiskRule] = &[AgentRiskRule {
    key: "autoApprove",
    expected_bool: Some(true),
    expected_str: None,
    marker: "autoApprove",
    score: 60,
}];

const PI_RISK_RULES: &[AgentRiskRule] = &[AgentRiskRule {
    key: "projectTrust",
    expected_bool: None,
    expected_str: Some("approve"),
    marker: "projectTrust",
    score: 30,
}];

const GROK_RISK_RULES: &[AgentRiskRule] = &[
    AgentRiskRule {
        key: "permissionMode",
        expected_bool: None,
        expected_str: Some("bypassPermissions"),
        marker: "bypassPermissions",
        score: 100,
    },
    AgentRiskRule {
        key: "permissionMode",
        expected_bool: None,
        expected_str: Some("dontAsk"),
        marker: "dontAsk",
        score: 40,
    },
];

const DEVIN_RISK_RULES: &[AgentRiskRule] = &[
    AgentRiskRule {
        key: "permissionMode",
        expected_bool: None,
        expected_str: Some("dangerous"),
        marker: "dangerous",
        score: 100,
    },
    AgentRiskRule {
        key: "permissionMode",
        expected_bool: None,
        expected_str: Some("smart"),
        marker: "smart",
        score: 30,
    },
];

const NO_RISK_RULES: &[AgentRiskRule] = &[];

/// Spawnable agent adapters, one entry per `AgentType`. Order is significant:
/// UI lists and tests rely on it.
pub const AGENT_DESCRIPTORS: &[AgentDescriptor] = &[
    AgentDescriptor {
        id: "codex",
        aliases: &[],
        display_name: "Codex",
        default_command: "codex",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::PositionalAfterTerminator,
        hook_strategy: AgentHookStrategy::RuntimeHome,
        status_strategy: AgentStatusStrategy::HookEventsWithTranscriptWatch,
        quota_provider_id: Some("codex"),
        transcript_usage: true,
        model_override: AgentModelOverride::Supported,
        supports_persona: false,
        supports_ccs_profile: false,
        risk_warning: "This profile reduces Codex approval or sandbox protections.",
        risk_warning_severe: Some(
            "This profile will bypass Codex approvals and sandbox protections.",
        ),
        risk_rules: CODEX_RISK_RULES,
    },
    AgentDescriptor {
        id: "claude",
        aliases: &[],
        display_name: "Claude Code",
        default_command: "claude",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::PositionalAfterTerminator,
        hook_strategy: AgentHookStrategy::RuntimeHome,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: Some("claude"),
        transcript_usage: true,
        model_override: AgentModelOverride::Supported,
        supports_persona: true,
        supports_ccs_profile: true,
        risk_warning: "This profile lets Claude continue with reduced permission prompts.",
        risk_warning_severe: None,
        risk_rules: CLAUDE_RISK_RULES,
    },
    AgentDescriptor {
        id: "copilot",
        aliases: &[],
        display_name: "GitHub Copilot",
        default_command: "copilot",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::LongOption("--interactive"),
        hook_strategy: AgentHookStrategy::ConfigJson,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: None,
        transcript_usage: false,
        model_override: AgentModelOverride::Supported,
        supports_persona: true,
        supports_ccs_profile: false,
        risk_warning:
            "This profile lets Copilot take broader actions with less supervision.",
        risk_warning_severe: None,
        risk_rules: COPILOT_RISK_RULES,
    },
    AgentDescriptor {
        id: "cursor",
        aliases: &[],
        display_name: "Cursor",
        default_command: "cursor-agent",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::PositionalAfterTerminator,
        hook_strategy: AgentHookStrategy::SessionOverlay,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: Some("cursor"),
        transcript_usage: false,
        model_override: AgentModelOverride::Supported,
        supports_persona: false,
        supports_ccs_profile: false,
        risk_warning:
            "This profile reduces Cursor review, sandbox, or trust protections.",
        risk_warning_severe: None,
        risk_rules: CURSOR_RISK_RULES,
    },
    AgentDescriptor {
        id: "agy",
        aliases: &["antigravity"],
        display_name: "Antigravity",
        default_command: "agy",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::LongOption("--prompt-interactive"),
        hook_strategy: AgentHookStrategy::ConfigJson,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: Some("agy"),
        transcript_usage: false,
        model_override: AgentModelOverride::Supported,
        supports_persona: true,
        supports_ccs_profile: false,
        risk_warning: "This profile lets Antigravity skip permission checks.",
        risk_warning_severe: None,
        risk_rules: AGY_RISK_RULES,
    },
    AgentDescriptor {
        id: "opencode",
        aliases: &[],
        display_name: "OpenCode",
        default_command: "opencode",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::LongOption("--prompt"),
        hook_strategy: AgentHookStrategy::PluginScript,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: Some("opencode"),
        transcript_usage: false,
        model_override: AgentModelOverride::Supported,
        supports_persona: true,
        supports_ccs_profile: false,
        risk_warning: "This profile lets OpenCode approve actions automatically.",
        risk_warning_severe: None,
        risk_rules: OPENCODE_RISK_RULES,
    },
    AgentDescriptor {
        // OpenCode 2 installs as `opencode2` beside v1's `opencode`.
        id: "opencode2",
        aliases: &[],
        display_name: "OpenCode 2",
        default_command: "opencode2",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::LongOption("--prompt"),
        hook_strategy: AgentHookStrategy::PluginScript,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: None,
        transcript_usage: false,
        model_override: AgentModelOverride::ProfileOnly,
        supports_persona: true,
        supports_ccs_profile: false,
        risk_warning: "This profile lets OpenCode approve actions automatically.",
        risk_warning_severe: None,
        risk_rules: OPENCODE_RISK_RULES,
    },
    AgentDescriptor {
        id: "pi",
        aliases: &[],
        display_name: "Pi",
        default_command: "pi",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::Positional,
        hook_strategy: AgentHookStrategy::PluginScript,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: None,
        transcript_usage: false,
        model_override: AgentModelOverride::Supported,
        supports_persona: false,
        supports_ccs_profile: false,
        risk_warning: "This profile pre-approves project trust for Pi.",
        risk_warning_severe: None,
        risk_rules: PI_RISK_RULES,
    },
    AgentDescriptor {
        id: "amp",
        aliases: &[],
        display_name: "Amp",
        default_command: "amp",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::StdinScript,
        hook_strategy: AgentHookStrategy::PluginScript,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: None,
        transcript_usage: false,
        model_override: AgentModelOverride::Unsupported,
        supports_persona: false,
        supports_ccs_profile: false,
        risk_warning: "",
        risk_warning_severe: None,
        risk_rules: NO_RISK_RULES,
    },
    AgentDescriptor {
        id: "grok",
        aliases: &[],
        display_name: "Grok Build",
        default_command: "grok",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::PositionalAfterTerminator,
        hook_strategy: AgentHookStrategy::ConfigJson,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: Some("grok"),
        transcript_usage: true,
        model_override: AgentModelOverride::Supported,
        supports_persona: true,
        supports_ccs_profile: false,
        risk_warning:
            "This profile lets Grok Build continue with reduced permission prompts.",
        risk_warning_severe: None,
        risk_rules: GROK_RISK_RULES,
    },
    AgentDescriptor {
        id: "devin",
        aliases: &[],
        display_name: "Devin",
        default_command: "devin",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::PositionalAfterTerminator,
        hook_strategy: AgentHookStrategy::ConfigJson,
        status_strategy: AgentStatusStrategy::HookEvents,
        quota_provider_id: Some("devin"),
        transcript_usage: false,
        model_override: AgentModelOverride::Supported,
        supports_persona: false,
        supports_ccs_profile: false,
        risk_warning:
            "This profile lets Devin take broader actions with less supervision.",
        risk_warning_severe: None,
        risk_rules: DEVIN_RISK_RULES,
    },
    AgentDescriptor {
        id: "fx",
        aliases: &[],
        display_name: "fx",
        default_command: "fx",
        force_submit: true,
        interrupt_bytes: CTRL_C,
        startup_prompt: AgentStartupPrompt::TerminalAfterReady,
        hook_strategy: AgentHookStrategy::HerdrSocket,
        status_strategy: AgentStatusStrategy::HerdrSocket,
        quota_provider_id: None,
        transcript_usage: false,
        model_override: AgentModelOverride::Unsupported,
        supports_persona: false,
        supports_ccs_profile: false,
        risk_warning: "",
        risk_warning_severe: None,
        risk_rules: NO_RISK_RULES,
    },
];

/// Resolves an agent id or alias to its descriptor.
pub fn agent_descriptor(agent_type: &str) -> Option<&'static AgentDescriptor> {
    AGENT_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.matches(agent_type))
}

/// Maps an id or alias to the canonical descriptor id, e.g. `antigravity`
/// resolves to `agy` so quota-side naming converges on one identifier.
pub fn canonical_agent_id(agent_type: &str) -> Option<&'static str> {
    agent_descriptor(agent_type).map(|descriptor| descriptor.id)
}

impl AgentDescriptor {
    fn matches(&self, agent_type: &str) -> bool {
        self.id == agent_type || self.aliases.contains(&agent_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_ids_are_canonical_and_unique() {
        let ids: Vec<_> = AGENT_DESCRIPTORS
            .iter()
            .map(|descriptor| descriptor.id)
            .collect();
        assert_eq!(
            ids,
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
        assert!(AGENT_DESCRIPTORS
            .iter()
            .all(|d| !d.default_command.is_empty() && !d.display_name.is_empty()));
    }

    #[test]
    fn aliases_resolve_to_canonical_ids() {
        assert_eq!(canonical_agent_id("antigravity"), Some("agy"));
        assert_eq!(canonical_agent_id("agy"), Some("agy"));
        assert_eq!(canonical_agent_id("unknown-agent"), None);
    }

    #[test]
    fn severe_risk_warning_only_where_a_rule_scores_100() {
        for descriptor in AGENT_DESCRIPTORS {
            let has_full_score = descriptor.risk_rules.iter().any(|r| r.score >= 100);
            assert_eq!(descriptor.risk_warning_severe.is_some(), descriptor.id == "codex" && has_full_score);
        }
    }
}
