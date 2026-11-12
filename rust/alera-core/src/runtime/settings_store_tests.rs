use std::collections::HashMap;

#[tokio::test]
async fn history_sync_policy_is_visible_to_two_clients_of_same_runtime_store() {
    use super::{RuntimeAgentHistorySyncMode, RuntimeAgentHistorySyncSettings};
    use std::collections::BTreeMap;

    let directory = tempfile::tempdir().unwrap();
    let classic = RuntimeStore::open(directory.path()).await.unwrap();
    let gpui = RuntimeStore::open(directory.path()).await.unwrap();

    let mut settings = classic.agent_quota_settings().await.unwrap();
    assert_eq!(
        settings.history_sync.mode,
        RuntimeAgentHistorySyncMode::Separate
    );
    settings.history_sync = RuntimeAgentHistorySyncSettings {
        mode: RuntimeAgentHistorySyncMode::Groups,
        account_groups: BTreeMap::from([
            ("codex:default".into(), "team".into()),
            ("codex:work".into(), "team".into()),
            ("claude:default".into(), "private".into()),
        ]),
    };
    classic.set_agent_quota_settings(settings).await.unwrap();

    let seen_in_gpui = gpui.agent_quota_settings().await.unwrap();
    assert_eq!(
        seen_in_gpui.history_sync.mode,
        RuntimeAgentHistorySyncMode::Groups
    );
    assert!(seen_in_gpui
        .history_sync
        .should_sync("codex", "default", "work"));
    assert!(!seen_in_gpui
        .history_sync
        .should_sync("claude", "default", "work"));

    let mut gpui_settings = seen_in_gpui;
    gpui_settings.history_sync.mode = RuntimeAgentHistorySyncMode::Separate;
    gpui.set_agent_quota_settings(gpui_settings).await.unwrap();

    let after_gpui_change = classic.agent_quota_settings().await.unwrap();
    assert_eq!(
        after_gpui_change.history_sync.mode,
        RuntimeAgentHistorySyncMode::Separate
    );
    assert!(!after_gpui_change
        .history_sync
        .should_sync("codex", "default", "work"));

    let reopened = RuntimeStore::open(directory.path()).await.unwrap();
    assert_eq!(
        reopened
            .agent_quota_settings()
            .await
            .unwrap()
            .history_sync
            .mode,
        RuntimeAgentHistorySyncMode::Separate
    );
}
use super::{
    RuntimeAiAssistSettings, RuntimeSettings, RuntimeStore, RuntimeTextAction,
    RuntimeTextActionsSettings,
};

#[test]
fn ai_assist_settings_keep_historical_json_key() {
    let settings = RuntimeSettings {
        ai_assist: Some(RuntimeAiAssistSettings::default()),
        ..RuntimeSettings::default()
    };
    let value = serde_json::to_value(&settings).unwrap();
    assert!(value.get("aiTextGeneration").is_some());
    assert!(value.get("aiAssist").is_none());
}

#[tokio::test]
async fn text_actions_are_optional_and_round_trip_through_runtime_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let store = RuntimeStore::open(directory.path()).await.unwrap();

    assert!(store
        .runtime_settings()
        .await
        .unwrap()
        .text_actions
        .is_none());

    let runtime = store
        .set_text_actions_settings(RuntimeTextActionsSettings {
            actions: vec![RuntimeTextAction {
                id: " action-1 ".to_string(),
                name: " Polish ".to_string(),
                prompt: " Improve clarity. ".to_string(),
                enabled: true,
                agent_override: Some(" CODEX ".to_string()),
                model_override: Some(" gpt-5.5 ".to_string()),
                reasoning_by_model: HashMap::from([(
                    " gpt-5.5 ".to_string(),
                    " high ".to_string(),
                )]),
            }],
        })
        .await
        .unwrap();

    let action = &runtime.text_actions.unwrap().actions[0];
    assert_eq!(action.id, "action-1");
    assert_eq!(action.name, "Polish");
    assert_eq!(action.prompt, "Improve clarity.");
    assert_eq!(action.agent_override.as_deref(), Some("codex"));
    assert_eq!(action.model_override.as_deref(), Some("gpt-5.5"));
    assert_eq!(action.reasoning_by_model["gpt-5.5"], "high");

    let reopened = RuntimeStore::open(directory.path()).await.unwrap();
    assert_eq!(
        reopened
            .runtime_settings()
            .await
            .unwrap()
            .text_actions
            .unwrap()
            .actions
            .len(),
        1
    );
}
