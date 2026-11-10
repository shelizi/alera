use chrono::Utc;
use clap::Parser;

use super::*;

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
