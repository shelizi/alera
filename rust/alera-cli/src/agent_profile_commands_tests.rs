use chrono::Utc;
use clap::Parser;

use super::*;
use crate::cli::{AgentProfileRevisionSelectorArgs, ManagedConfigInputArgs};

#[test]
fn agent_profile_cli_supports_explicit_no_start_without_changing_legacy_default() {
    use crate::cli::{Cli, Command};
    let normal = Cli::try_parse_from(["alera", "agent-profile", "--json", "list"]).unwrap();
    let Command::AgentProfile(normal) = normal.command else {
        panic!("not a profile command")
    };
    assert!(!normal.no_start, "legacy CLI behavior remains opt-in");
    let native = Cli::try_parse_from([
        "alera",
        "agent-profile",
        "--no-start",
        "--json",
        "--runtime-dir",
        "test-runtime",
        "launch",
        "--profile-id=prof_1",
        "--workspace=ws_1",
        "--prompt-stdin",
    ])
    .unwrap();
    let Command::AgentProfile(native) = native.command else {
        panic!("not a profile command")
    };
    assert!(native.no_start);
    assert!(matches!(native.action, AgentProfileAction::Launch(_)));
}

fn profile(id: &str, name: &str, revision: i64) -> AgentProfile {
    AgentProfile {
        id: id.to_string(),
        name: name.to_string(),
        sort_order: 0,
        agent_type: "codex".to_string(),
        command: "codex".to_string(),
        launch_mode: AgentProfileLaunchMode::Command,
        managed_config: None,
        custom_prompt: String::new(),
        description: String::new(),
        quota_group: None,
        revision,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[test]
fn selectors_accept_ids_and_case_insensitive_names() {
    let profiles = vec![profile("prof_1", "Codex Sol", 2)];
    let by_id = AgentProfileSelectorArgs {
        profile_id: Some("prof_1".to_string()),
        profile_name: None,
        profile: None,
    };
    let by_name = AgentProfileSelectorArgs {
        profile_id: None,
        profile_name: Some("codex sol".to_string()),
        profile: None,
    };
    let by_alias = AgentProfileSelectorArgs {
        profile_id: None,
        profile_name: None,
        profile: Some("CODEX SOL".to_string()),
    };

    assert_eq!(select_profile(&profiles, &by_id).unwrap().id, "prof_1");
    assert_eq!(select_profile(&profiles, &by_name).unwrap().id, "prof_1");
    assert_eq!(select_profile(&profiles, &by_alias).unwrap().id, "prof_1");
}

#[test]
fn reorder_requires_the_complete_catalog_and_applies_overrides() {
    let profiles = vec![profile("prof_1", "One", 2), profile("prof_2", "Two", 3)];
    let args = AgentProfileReorderArgs {
        ids: vec!["prof_2".to_string(), "prof_1".to_string()],
        expected_revisions: vec!["prof_1=1".to_string()],
    };

    let revisions = reorder_revisions(&profiles, &args).unwrap();

    assert_eq!(revisions["prof_1"], 1);
    assert_eq!(revisions["prof_2"], 3);
}

#[test]
fn reorder_rejects_missing_profiles() {
    let profiles = vec![profile("prof_1", "One", 2), profile("prof_2", "Two", 3)];
    let args = AgentProfileReorderArgs {
        ids: vec!["prof_1".to_string()],
        expected_revisions: vec![],
    };

    assert!(reorder_revisions(&profiles, &args).is_err());
}

fn update_args() -> AgentProfileUpdateArgs {
    AgentProfileUpdateArgs {
        target: AgentProfileRevisionSelectorArgs {
            selector: AgentProfileSelectorArgs {
                profile_id: Some("prof_1".to_string()),
                profile_name: None,
                profile: None,
            },
            expected_revision: Some(3),
        },
        name: None,
        agent_type: None,
        launch_mode: None,
        command: None,
        managed: ManagedConfigInputArgs {
            managed_config: None,
            managed_config_file: None,
            managed_config_stdin: false,
        },
        custom_prompt: None,
        clear_custom_prompt: false,
        description: None,
        clear_description: false,
        quota_group: None,
        clear_quota_group: false,
        edit_content_stdin: true,
        confirm_reduced_protections: false,
    }
}

#[test]
fn update_edit_content_stdin_is_opt_in_revision_guarded_and_conflicts_with_argv() {
    use crate::cli::{Cli, Command};

    let cli = Cli::try_parse_from([
        "alera",
        "agent-profile",
        "--no-start",
        "--json",
        "update",
        "--profile-id=prof_1",
        "--expected-revision=3",
        "--edit-content-stdin",
    ])
    .unwrap();
    let Command::AgentProfile(profile) = cli.command else {
        panic!("not a profile command")
    };
    let AgentProfileAction::Update(args) = profile.action else {
        panic!("not an update command")
    };
    assert!(args.edit_content_stdin);
    assert_eq!(args.target.expected_revision, Some(3));

    assert!(
        Cli::try_parse_from([
            "alera",
            "agent-profile",
            "update",
            "--profile-id=prof_1",
            "--edit-content-stdin",
        ])
        .is_err(),
        "stdin edits must require an observed Host revision"
    );
    for extra in [
        "--command=codex",
        "--custom-prompt=secret",
        "--clear-custom-prompt",
        "--managed-config-stdin",
    ] {
        assert!(
            Cli::try_parse_from([
                "alera",
                "agent-profile",
                "update",
                "--profile-id=prof_1",
                "--expected-revision=3",
                "--edit-content-stdin",
                extra,
            ])
            .is_err(),
            "expected argv conflict for {extra}"
        );
    }
}

#[test]
fn edit_content_json_accepts_command_prompt_replace_and_explicit_clear() {
    let edit = parse_edit_content(
        r#"{"command":"codex --profile=review","customPrompt":"Focus on tests"}"#,
    )
    .unwrap();
    assert_eq!(edit.command.as_deref(), Some("codex --profile=review"));
    assert_eq!(edit.custom_prompt.as_deref(), Some("Focus on tests"));
    assert!(!edit.clear_custom_prompt);

    let clear = parse_edit_content(r#"{"clearCustomPrompt":true}"#).unwrap();
    assert!(clear.command.is_none());
    assert!(clear.custom_prompt.is_none());
    assert!(clear.clear_custom_prompt);

    let trimmed = parse_edit_content(" { \"command\" : \"  codex  \" } ").unwrap();
    assert_eq!(trimmed.command.as_deref(), Some("codex"));
}

#[test]
fn edit_content_json_rejects_malformed_and_unsafe_bodies() {
    for raw in [
        "",
        "not json",
        "[]",
        "\"command\"",
        "null",
        "{}",
        r#"{"unknown":"x"}"#,
        r#"{"command":42}"#,
        r#"{"command":"   "}"#,
        r#"{"customPrompt":""}"#,
        r#"{"customPrompt":"  "}"#,
        r#"{"customPrompt":"a","clearCustomPrompt":true}"#,
        r#"{"clearCustomPrompt":false}"#,
        r#"{"clearCustomPrompt":"yes"}"#,
        r#"{"command":"ok","extra":null}"#,
    ] {
        assert!(
            parse_edit_content(raw).is_err(),
            "expected rejection for {raw}"
        );
    }
}

#[test]
fn edit_content_json_enforces_command_and_prompt_byte_limits() {
    let max_command = "c".repeat(MAX_EDIT_COMMAND_BYTES);
    let ok = parse_edit_content(&format!(r#"{{"command":"{max_command}"}}"#)).unwrap();
    assert_eq!(ok.command.unwrap().len(), MAX_EDIT_COMMAND_BYTES);

    let over_command = "c".repeat(MAX_EDIT_COMMAND_BYTES + 1);
    assert!(parse_edit_content(&format!(r#"{{"command":"{over_command}"}}"#)).is_err());

    let max_prompt = "p".repeat(MAX_EDIT_PROMPT_BYTES);
    assert!(parse_edit_content(&format!(r#"{{"customPrompt":"{max_prompt}"}}"#)).is_ok());

    let over_prompt = "p".repeat(MAX_EDIT_PROMPT_BYTES + 1);
    assert!(parse_edit_content(&format!(r#"{{"customPrompt":"{over_prompt}"}}"#)).is_err());
}

#[test]
fn edit_content_stdin_is_bounded_before_parsing() {
    let body = br#"{"command":"codex"}"#;
    let mut good: &[u8] = body;
    let edit = read_edit_content(&mut good).unwrap();
    assert_eq!(edit.command.as_deref(), Some("codex"));

    let oversized = vec![b' '; MAX_EDIT_CONTENT_STDIN_BYTES as usize + 1];
    let mut bad: &[u8] = &oversized;
    assert!(read_edit_content(&mut bad).is_err());
}

#[test]
fn stdin_edit_content_updates_only_command_and_prompt_fields() {
    let existing = profile("prof_1", "Review", 3);
    let mut args = update_args();
    let edit = parse_edit_content(r#"{"command":"codex --profile=review","customPrompt":"Focus"}"#)
        .unwrap();
    apply_edit_content(&mut args, edit);

    let draft = draft_for_update(&existing, &args, None).unwrap();

    assert_eq!(draft.command, "codex --profile=review");
    assert_eq!(draft.custom_prompt, "Focus");
    assert_eq!(draft.name, existing.name);
    assert_eq!(draft.agent_type, existing.agent_type);
    assert_eq!(draft.description, existing.description);
    assert_eq!(draft.quota_group, existing.quota_group);
    assert_eq!(draft.launch_mode, existing.launch_mode);
}

#[test]
fn stdin_edit_content_clear_and_untouched_fields_stay_explicit() {
    let mut existing = profile("prof_1", "Review", 3);
    existing.custom_prompt = "Keep me".to_string();
    let mut args = update_args();
    let edit = parse_edit_content(r#"{"clearCustomPrompt":true}"#).unwrap();
    apply_edit_content(&mut args, edit);

    let draft = draft_for_update(&existing, &args, None).unwrap();

    assert_eq!(draft.custom_prompt, "");
    assert_eq!(draft.command, existing.command, "command stays untouched");
    assert_eq!(draft.name, existing.name);
}

#[test]
fn stdin_command_edit_rejects_managed_profiles_before_mutation() {
    let mut existing = profile("prof_1", "Review", 3);
    existing.launch_mode = AgentProfileLaunchMode::Managed;
    existing.managed_config = Some(json!({"model": "gpt-5"}));
    existing.command = String::new();
    let mut args = update_args();
    let edit = parse_edit_content(r#"{"command":"codex"}"#).unwrap();
    apply_edit_content(&mut args, edit);

    let error = draft_for_update(&existing, &args, None).unwrap_err();

    assert!(error.to_string().contains("--launch-mode command"));
}
