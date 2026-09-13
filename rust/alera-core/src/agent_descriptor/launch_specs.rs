use super::{AgentLaunchRule, AgentLaunchSpec, AgentProfileLauncher};

const CODEX_EFFORTS: &[&str] = &["minimal", "low", "medium", "high", "xhigh", "max", "ultra"];
const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
const COPILOT_EFFORTS: &[&str] = &["none", "minimal", "low", "medium", "high", "xhigh", "max"];
const GROK_EFFORTS: &[&str] = &["none", "minimal", "low", "medium", "high", "xhigh", "max"];

const CODEX_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::config_kv("effort", "model_reasoning_effort", CODEX_EFFORTS),
    AgentLaunchRule::config_kv(
        "planModeEffort",
        "plan_mode_reasoning_effort",
        CODEX_EFFORTS,
    ),
    AgentLaunchRule::exclusive(
        "bypassApprovalsAndSandbox",
        "--dangerously-bypass-approvals-and-sandbox",
        &["sandbox", "approvalPolicy"],
    ),
    AgentLaunchRule::enumeration(
        "sandbox",
        "--sandbox",
        &["read-only", "workspace-write", "danger-full-access"],
    )
    .suppressed_by("bypassApprovalsAndSandbox"),
    AgentLaunchRule::enumeration(
        "approvalPolicy",
        "--ask-for-approval",
        &["untrusted", "on-request", "never"],
    )
    .suppressed_by("bypassApprovalsAndSandbox"),
    AgentLaunchRule::boolean("webSearch", "--search"),
];

pub(super) const CODEX_LAUNCH_SPEC: AgentLaunchSpec = AgentLaunchSpec::new(
    &[
        "model",
        "effort",
        "planModeEffort",
        "sandbox",
        "approvalPolicy",
        "webSearch",
        "bypassApprovalsAndSandbox",
    ],
    None,
    CODEX_RULES,
);

const CLAUDE_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::enumeration("effort", "--effort", CLAUDE_EFFORTS),
    AgentLaunchRule::string("agent", "--agent"),
    AgentLaunchRule::enumeration(
        "permissionMode",
        "--permission-mode",
        &[
            "acceptEdits",
            "auto",
            "bypassPermissions",
            "manual",
            "dontAsk",
            "plan",
        ],
    ),
    AgentLaunchRule::boolean(
        "allowSkipPermissions",
        "--allow-dangerously-skip-permissions",
    ),
];

pub(super) const CLAUDE_LAUNCH_SPEC: AgentLaunchSpec = AgentLaunchSpec::new(
    &[
        "model",
        "effort",
        "agent",
        "permissionMode",
        "allowSkipPermissions",
        "ccsProfile",
    ],
    Some(AgentProfileLauncher::new("ccsProfile", "ccs")),
    CLAUDE_RULES,
);

const COPILOT_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::enumeration("effort", "--effort", COPILOT_EFFORTS),
    AgentLaunchRule::string("agent", "--agent"),
    AgentLaunchRule::enumeration("mode", "--mode", &["interactive", "plan", "autopilot"]),
    AgentLaunchRule::enumeration("context", "--context", &["default", "long_context"]),
    AgentLaunchRule::boolean("allowAll", "--allow-all"),
    AgentLaunchRule::number("maxAiCredits", "--max-ai-credits", true),
    AgentLaunchRule::number("maxAutopilotContinues", "--max-autopilot-continues", false),
    AgentLaunchRule::boolean("noAskUser", "--no-ask-user"),
];

pub(super) const COPILOT_LAUNCH_SPEC: AgentLaunchSpec = AgentLaunchSpec::new(
    &[
        "model",
        "effort",
        "agent",
        "mode",
        "context",
        "allowAll",
        "maxAiCredits",
        "maxAutopilotContinues",
        "noAskUser",
    ],
    None,
    COPILOT_RULES,
);

const CURSOR_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::enumeration("mode", "--mode", &["plan", "ask"]),
    AgentLaunchRule::enum_to_flag(
        "permissionMode",
        &["autoReview", "force"],
        &[("autoReview", "--auto-review"), ("force", "--force")],
    ),
    AgentLaunchRule::enumeration("sandbox", "--sandbox", &["enabled", "disabled"]),
    AgentLaunchRule::boolean("trustWorkspace", "--trust"),
];

pub(super) const CURSOR_LAUNCH_SPEC: AgentLaunchSpec = AgentLaunchSpec::new(
    &[
        "model",
        "mode",
        "permissionMode",
        "sandbox",
        "trustWorkspace",
    ],
    None,
    CURSOR_RULES,
);

const AGY_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::enumeration("effort", "--effort", &["low", "medium", "high"]),
    AgentLaunchRule::string("agent", "--agent"),
    AgentLaunchRule::enumeration("mode", "--mode", &["accept-edits", "plan"]),
    AgentLaunchRule::boolean("skipPermissions", "--dangerously-skip-permissions"),
    AgentLaunchRule::boolean("sandbox", "--sandbox"),
];

pub(super) const AGY_LAUNCH_SPEC: AgentLaunchSpec = AgentLaunchSpec::new(
    &[
        "model",
        "effort",
        "agent",
        "mode",
        "skipPermissions",
        "sandbox",
    ],
    None,
    AGY_RULES,
);

const OPENCODE_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::string("agent", "--agent"),
    AgentLaunchRule::boolean("autoApprove", "--auto"),
];

pub(super) const OPENCODE_LAUNCH_SPEC: AgentLaunchSpec =
    AgentLaunchSpec::new(&["model", "agent", "autoApprove"], None, OPENCODE_RULES);

const OPENCODE2_RULES: &[AgentLaunchRule] = &[AgentLaunchRule::boolean("autoApprove", "--auto")];

pub(super) const OPENCODE2_LAUNCH_SPEC: AgentLaunchSpec =
    AgentLaunchSpec::new(&["model", "agent", "autoApprove"], None, OPENCODE2_RULES);

const PI_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::enumeration(
        "thinking",
        "--thinking",
        &["off", "minimal", "low", "medium", "high", "xhigh", "max"],
    ),
    AgentLaunchRule::enum_to_flag(
        "projectTrust",
        &["approve", "ignore"],
        &[("approve", "--approve"), ("ignore", "--no-approve")],
    ),
];

pub(super) const PI_LAUNCH_SPEC: AgentLaunchSpec =
    AgentLaunchSpec::new(&["model", "thinking", "projectTrust"], None, PI_RULES);

const AMP_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::enumeration("mode", "--mode", &["low", "medium", "high", "ultra"]),
    AgentLaunchRule::boolean("fast", "--fast"),
];

pub(super) const AMP_LAUNCH_SPEC: AgentLaunchSpec =
    AgentLaunchSpec::new(&["mode", "fast"], None, AMP_RULES);

const GROK_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::enumeration("effort", "--effort", GROK_EFFORTS),
    AgentLaunchRule::string("agent", "--agent"),
    AgentLaunchRule::enumeration(
        "permissionMode",
        "--permission-mode",
        &[
            "default",
            "acceptEdits",
            "auto",
            "dontAsk",
            "bypassPermissions",
            "plan",
        ],
    ),
    AgentLaunchRule::enumeration(
        "sandbox",
        "--sandbox",
        &["off", "workspace", "devbox", "read-only", "strict"],
    ),
    AgentLaunchRule::boolean("disableWebSearch", "--disable-web-search"),
];

pub(super) const GROK_LAUNCH_SPEC: AgentLaunchSpec = AgentLaunchSpec::new(
    &[
        "model",
        "effort",
        "agent",
        "permissionMode",
        "sandbox",
        "disableWebSearch",
    ],
    None,
    GROK_RULES,
);

const DEVIN_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::enumeration(
        "permissionMode",
        "--permission-mode",
        &["auto", "accept-edits", "smart", "dangerous"],
    ),
    AgentLaunchRule::boolean("sandbox", "--sandbox"),
];

pub(super) const DEVIN_LAUNCH_SPEC: AgentLaunchSpec =
    AgentLaunchSpec::new(&["model", "permissionMode", "sandbox"], None, DEVIN_RULES);

const FX_RULES: &[AgentLaunchRule] = &[
    AgentLaunchRule::boolean("resumeLast", "--continue"),
    AgentLaunchRule::boolean("noAdditionalDirs", "--no-additional-dirs"),
    AgentLaunchRule::boolean("record", "--record"),
];

pub(super) const FX_LAUNCH_SPEC: AgentLaunchSpec = AgentLaunchSpec::new(
    &["resumeLast", "noAdditionalDirs", "record"],
    None,
    FX_RULES,
);
