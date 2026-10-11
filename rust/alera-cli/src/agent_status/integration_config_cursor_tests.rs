use serde_json::Map;

use super::*;

#[test]
fn installs_all_events_into_user_config_and_is_idempotent() {
    let home = tempfile::tempdir().unwrap();
    let script = Path::new("/home/user/.alera/agent-hooks/alera-runtime-agent-hook.sh");
    install_user_hooks(home.path(), script).unwrap();
    let path = home.path().join(".cursor/hooks.json");
    let config = read_json_object(&path).unwrap().unwrap();
    assert_eq!(config["version"], json!(1));
    let hooks = config["hooks"].as_object().unwrap();
    assert_eq!(hooks.len(), CURSOR_HOOK_EVENTS.len());
    for event in CURSOR_HOOK_EVENTS {
        assert_eq!(hooks[event].as_array().unwrap().len(), 1);
        assert_eq!(hooks[event][0]["timeout"], json!(5));
        assert_eq!(
            hooks[event][0]["command"],
            managed_command(script, "cursor", event)
        );
    }
    assert!(!hooks.contains_key("sessionStart"));
    let before = std::fs::read_to_string(&path).unwrap();
    install_user_hooks(home.path(), script).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), before);
}

#[test]
fn installation_preserves_user_hooks_and_replaces_legacy_hooks() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".cursor/hooks.json");
    let config = Map::from_iter([
        ("version".to_string(), json!(1)),
        ("custom".to_string(), json!(true)),
        (
            "hooks".to_string(),
            json!({
                "stop": [{"command": "audit.sh"}, {"command": "alera-cursor-hook.sh"}],
                "sessionStart": [{"command": "startup.sh"}],
            }),
        ),
    ]);
    write_json_object(&path, &config).unwrap();
    install_user_hooks(home.path(), Path::new("alera-runtime-agent-hook.sh")).unwrap();
    let installed = read_json_object(&path).unwrap().unwrap();
    assert_eq!(installed["custom"], json!(true));
    assert_eq!(installed["hooks"]["stop"].as_array().unwrap().len(), 2);
    assert_eq!(
        installed["hooks"]["stop"][0],
        json!({"command": "audit.sh"})
    );
    assert_eq!(
        installed["hooks"]["sessionStart"],
        config["hooks"]["sessionStart"]
    );
    cleanup_user_hooks(home.path()).unwrap();
    let cleaned = read_json_object(&path).unwrap().unwrap();
    assert_eq!(cleaned["hooks"]["stop"], json!([{"command": "audit.sh"}]));
}

#[test]
fn invalid_user_config_is_preserved() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".cursor/hooks.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for invalid in ["{invalid", "[]", r#"{"hooks": []}"#] {
        std::fs::write(&path, invalid).unwrap();
        assert!(install_user_hooks(home.path(), Path::new("hook.sh")).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
    }
}

#[test]
fn startup_removes_legacy_overlays_without_removing_user_hooks() {
    let runtime = tempfile::tempdir().unwrap();
    let overlay = runtime
        .path()
        .join("agent-runtime-overlays/cursor/session-1/bin");
    std::fs::create_dir_all(&overlay).unwrap();
    let home = tempfile::tempdir().unwrap();
    install_user_hooks(home.path(), Path::new("alera-runtime-agent-hook.sh")).unwrap();
    clear_stale_state(runtime.path()).unwrap();
    assert!(!overlay.exists());
    assert!(home.path().join(".cursor/hooks.json").exists());
}

#[test]
fn cleanup_removes_alera_definitions_and_keeps_the_users_own() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".cursor/hooks.json");
    write_json_object(
        &path,
        &Map::from_iter([
            ("version".to_string(), json!(1)),
            (
                "hooks".to_string(),
                json!({
                    "preToolUse": [
                        {"command": "/home/user/audit.sh"},
                        {"command": "/home/user/.alera/agent-hooks/alera-runtime-agent-hook.sh"},
                    ],
                    "stop": [
                        {"command": "/home/user/.alera/agent-hooks/alera-cursor-hook.sh"},
                    ],
                }),
            ),
        ]),
    )
    .unwrap();

    cleanup_user_hooks(home.path()).unwrap();

    let config = read_json_object(&path).unwrap().unwrap();
    let hooks = config["hooks"].as_object().unwrap();
    assert_eq!(
        hooks["preToolUse"],
        json!([{"command": "/home/user/audit.sh"}])
    );
    assert!(!hooks.contains_key("stop"));
    assert_eq!(config["version"], json!(1));
}

#[test]
fn cleanup_leaves_a_file_without_alera_definitions_untouched() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".cursor/hooks.json");
    write_json_object(
        &path,
        &Map::from_iter([(
            "hooks".to_string(),
            json!({ "stop": [{"command": "/home/user/audit.sh"}] }),
        )]),
    )
    .unwrap();
    let before = std::fs::read_to_string(&path).unwrap();

    cleanup_user_hooks(home.path()).unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[test]
fn cleanup_is_a_no_op_when_the_user_has_no_hooks_file() {
    let home = tempfile::tempdir().unwrap();

    cleanup_user_hooks(home.path()).unwrap();

    assert!(!home.path().join(".cursor/hooks.json").exists());
}
