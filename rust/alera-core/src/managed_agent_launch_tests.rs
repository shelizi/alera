use std::{fs, path::Path};

use serde_json::json;

use super::*;
use crate::agent_descriptor::{
    AgentHookStrategy, AgentLaunchSpec, AgentStartupPrompt, AgentStatusNormalizationRule,
    AgentStatusNormalizationSpec, AgentStatusState, AgentStatusStrategy,
};

#[test]
fn synthetic_agent_descriptor_uses_shared_launch_and_status_paths() {
    const MODES: &[&str] = &["safe", "fast"];
    const RULES: &[AgentLaunchRule] = &[
        AgentLaunchRule::enumeration("mode", "--mode", MODES),
        AgentLaunchRule::boolean("unsafe", "--unsafe"),
    ];
    const STATUS_RULES: &[AgentStatusNormalizationRule] = &[
        AgentStatusNormalizationRule::new("Run", AgentStatusState::Working),
        AgentStatusNormalizationRule::new("Stop", AgentStatusState::Done),
    ];
    let descriptor = AgentDescriptor {
        id: "test-agent",
        aliases: &["test-agent-alias"],
        display_name: "Test Agent",
        default_command: "test-agent",
        force_submit: true,
        interrupt_bytes: b"\x03",
        startup_prompt: AgentStartupPrompt::PositionalAfterTerminator,
        hook_strategy: AgentHookStrategy::PluginScript,
        status_strategy: AgentStatusStrategy::HookEvents,
        status_normalization: AgentStatusNormalizationSpec::new(STATUS_RULES),
        quota_provider_id: None,
        transcript_usage: false,
        model_override: AgentModelOverride::Supported,
        launch_spec: AgentLaunchSpec::new(&["model", "mode", "unsafe"], None, RULES),
        supports_persona: false,
        supports_ccs_profile: false,
        risk_warning: "",
        risk_warning_severe: None,
        risk_rules: &[],
    };

    let launch = build_managed_agent_launch_for_descriptor(
        &descriptor,
        &json!({"model": "synthetic-v1", "mode": "fast", "unsafe": true}),
    )
    .expect("synthetic descriptor should use shared launch rules");
    assert_eq!(launch.executable, "test-agent");
    assert_eq!(
        launch.arguments,
        ["--model", "synthetic-v1", "--mode", "fast", "--unsafe"]
    );
    assert_eq!(
        descriptor.status_normalization.state_for("Run", false),
        Some(AgentStatusState::Working)
    );
    assert_eq!(
        descriptor.status_normalization.state_for("Stop", false),
        Some(AgentStatusState::Done)
    );
}

#[test]
fn codex_builds_structured_arguments_and_rejects_conflicting_bypass() {
    let launch = build_managed_agent_launch(
        "codex",
        &json!({
            "model": "gpt-5.6-sol",
            "effort": "high",
            "sandbox": "workspace-write",
            "approvalPolicy": "on-request",
            "webSearch": true
        }),
    )
    .unwrap();
    assert_eq!(
        launch.arguments,
        [
            "--model",
            "gpt-5.6-sol",
            "--config",
            "model_reasoning_effort=high",
            "--sandbox",
            "workspace-write",
            "--ask-for-approval",
            "on-request",
            "--search"
        ]
    );
    assert!(build_managed_agent_launch(
        "codex",
        &json!({
            "bypassApprovalsAndSandbox": true,
            "sandbox": "danger-full-access"
        })
    )
    .is_err());
}

#[test]
fn every_adapter_builds_its_native_session_flags() {
    let cases = [
        (
            "claude",
            json!({"permissionMode": "bypassPermissions"}),
            vec!["--permission-mode", "bypassPermissions"],
        ),
        (
            "copilot",
            json!({"allowAll": true, "mode": "autopilot"}),
            vec!["--mode", "autopilot", "--allow-all"],
        ),
        (
            "cursor",
            json!({"permissionMode": "autoReview", "sandbox": "enabled"}),
            vec!["--auto-review", "--sandbox", "enabled"],
        ),
        (
            "agy",
            json!({"skipPermissions": true, "sandbox": true}),
            vec!["--dangerously-skip-permissions", "--sandbox"],
        ),
        (
            "opencode",
            json!({"agent": "build", "autoApprove": true}),
            vec!["--agent", "build", "--auto"],
        ),
        (
            "opencode2",
            json!({"agent": "build", "model": "opencode/deepseek", "autoApprove": true}),
            vec!["--auto"],
        ),
        (
            "pi",
            json!({"thinking": "xhigh", "projectTrust": "ignore"}),
            vec!["--thinking", "xhigh", "--no-approve"],
        ),
        (
            "amp",
            json!({"mode": "ultra", "fast": true}),
            vec!["--mode", "ultra", "--fast"],
        ),
        (
            "grok",
            json!({
                "permissionMode": "acceptEdits",
                "sandbox": "workspace",
                "disableWebSearch": true
            }),
            vec![
                "--permission-mode",
                "acceptEdits",
                "--sandbox",
                "workspace",
                "--disable-web-search",
            ],
        ),
        (
            "devin",
            json!({
                "model": "opus",
                "permissionMode": "smart",
                "sandbox": true
            }),
            vec!["--model", "opus", "--permission-mode", "smart", "--sandbox"],
        ),
        (
            "fx",
            json!({
                "resumeLast": true,
                "noAdditionalDirs": true,
                "record": true
            }),
            vec!["--continue", "--no-additional-dirs", "--record"],
        ),
    ];
    for (agent, config, expected) in cases {
        assert_eq!(
            build_managed_agent_launch(agent, &config)
                .unwrap()
                .arguments,
            expected
        );
    }
}

#[test]
fn a_claude_ccs_profile_replaces_the_executable_and_leads_the_arguments() {
    let launch = build_managed_agent_launch(
        "claude",
        &json!({
            "ccsProfile": "work",
            "model": "opus",
            "permissionMode": "acceptEdits"
        }),
    )
    .unwrap();
    assert_eq!(launch.executable, "ccs");
    assert_eq!(
        launch.arguments,
        [
            "work",
            "--model",
            "opus",
            "--permission-mode",
            "acceptEdits"
        ]
    );

    let direct = build_managed_agent_launch("claude", &json!({"model": "opus"})).unwrap();
    assert_eq!(direct.executable, "claude");
    assert_eq!(direct.arguments, ["--model", "opus"]);
}

#[test]
fn codex_carries_a_separate_plan_mode_effort() {
    let launch = build_managed_agent_launch(
        "codex",
        &json!({"effort": "medium", "planModeEffort": "xhigh"}),
    )
    .unwrap();
    assert_eq!(
        launch.arguments,
        [
            "--config",
            "model_reasoning_effort=medium",
            "--config",
            "plan_mode_reasoning_effort=xhigh"
        ]
    );
    assert_eq!(
        build_managed_agent_launch("codex", &json!({"planModeEffort": "high"}))
            .unwrap()
            .arguments,
        ["--config", "plan_mode_reasoning_effort=high"]
    );
    assert!(
        build_managed_agent_launch("codex", &json!({"planModeEffort": "plan"})).is_err(),
        "an unsupported effort was accepted"
    );
    assert!(build_managed_agent_launch("claude", &json!({"planModeEffort": "high"})).is_err());
}

#[test]
fn claude_can_allow_bypass_without_starting_in_it() {
    let launch = build_managed_agent_launch(
        "claude",
        &json!({"permissionMode": "plan", "allowSkipPermissions": true}),
    )
    .unwrap();
    assert_eq!(
        launch.arguments,
        [
            "--permission-mode",
            "plan",
            "--allow-dangerously-skip-permissions"
        ]
    );
    assert!(
        build_managed_agent_launch("claude", &json!({"allowSkipPermissions": "yes"})).is_err(),
        "a non-boolean was accepted"
    );
    assert_eq!(
        build_managed_agent_launch("claude", &json!({"allowSkipPermissions": false}))
            .unwrap()
            .arguments,
        Vec::<String>::new()
    );
}

#[test]
fn a_claude_ccs_profile_must_be_a_single_name_that_is_not_an_option() {
    for rejected in [json!("--work"), json!("work extra"), json!("  "), json!(3)] {
        assert!(
            build_managed_agent_launch("claude", &json!({"ccsProfile": rejected})).is_err(),
            "accepted {rejected}"
        );
    }
    assert!(build_managed_agent_launch("codex", &json!({"ccsProfile": "work"})).is_err());
}

#[test]
fn grok_builds_interactive_session_flags_and_rejects_unknown_options() {
    let launch = build_managed_agent_launch(
        "grok",
        &json!({
            "model": "grok-4.6",
            "effort": "max",
            "agent": "grok-build",
            "permissionMode": "bypassPermissions",
            "sandbox": "strict"
        }),
    )
    .unwrap();
    assert_eq!(launch.executable, "grok");
    assert_eq!(
        launch.arguments,
        [
            "--model",
            "grok-4.6",
            "--effort",
            "max",
            "--agent",
            "grok-build",
            "--permission-mode",
            "bypassPermissions",
            "--sandbox",
            "strict"
        ]
    );
    assert!(
        build_managed_agent_launch("grok", &json!({"effort": "ultra"})).is_err(),
        "an unsupported grok effort was accepted"
    );
    assert!(
        build_managed_agent_launch("grok", &json!({"sandbox": "danger-full-access"})).is_err(),
        "an unsupported grok sandbox was accepted"
    );
    assert!(build_managed_agent_launch("grok", &json!({"webSearch": true})).is_err());
}

#[test]
fn agent_descriptor_snapshot_is_fresh() {
    let snapshot_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../lib/src/features/agent_profiles/domain/agent_descriptor_snapshot.dart");
    let checked_in = fs::read_to_string(&snapshot_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", snapshot_path.display()));
    let normalize = |contents: &str| {
        contents
            .replace("\r\n", "\n")
            .trim_end_matches('\n')
            .to_string()
    };

    assert_eq!(
        normalize(&crate::agent_descriptor::snapshot_dart::emit()),
        normalize(&checked_in),
        "agent descriptor snapshot is stale; regenerate with cargo run -p alera-cli -- export-agent-descriptors"
    );
}
