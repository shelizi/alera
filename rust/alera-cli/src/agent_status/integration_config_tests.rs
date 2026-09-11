use super::*;

#[test]
fn removes_current_and_legacy_alera_definitions_only() {
    let definitions = vec![
        json!({"command": "/home/user/custom-hook.sh"}),
        json!({"command": "/home/user/.alera/agent-hooks/alera-claude-hook.sh"}),
        json!({"command": "/home/user/.alera/agent-hooks/alera-runtime-agent-hook.sh"}),
        json!({"command": "/home/user/.orca/agent-hooks/claude-hook.sh"}),
    ];

    let cleaned = clean_managed_definitions(Some(Value::Array(definitions)));

    assert_eq!(cleaned.len(), 2);
    assert!(cleaned[0].to_string().contains("custom-hook.sh"));
    assert!(cleaned[1].to_string().contains(".orca/agent-hooks"));
}

#[test]
fn omits_the_matcher_key_for_events_without_a_matcher() {
    let definition = managed_hook_definition(None, "/home/user/hook.sh");

    assert_eq!(
        definition,
        json!({ "hooks": [{ "type": "command", "command": "/home/user/hook.sh" }] })
    );
    assert!(definition.get("matcher").is_none());
}

#[test]
fn keeps_the_matcher_key_for_tool_scoped_events() {
    let definition = managed_hook_definition(Some("*"), "/home/user/hook.sh");

    assert_eq!(definition["matcher"], json!("*"));
}

#[test]
fn devin_hooks_preserve_user_definitions_and_omit_matchers() {
    let mut config = Map::from_iter([
        ("theme".to_string(), json!("dark")),
        (
            "hooks".to_string(),
            json!({
                "SessionStart": [
                    {"hooks": [{"type": "command", "command": "echo user-start"}]}
                ],
                "PreToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [{"type": "command", "command": "echo user-tool"}]
                    },
                    {
                        "hooks": [{
                            "type": "command",
                            "command": "/old/.alera/agent-hooks/alera-runtime-agent-hook.sh"
                        }]
                    }
                ]
            }),
        ),
    ]);

    apply_devin_hooks(
        &mut config,
        Path::new("/home/user/.alera/agent-hooks/alera-runtime-agent-hook.sh"),
    );

    assert_eq!(config["theme"], json!("dark"));
    let hooks = config["hooks"].as_object().expect("hooks object");
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "Stop",
        "SessionEnd",
    ] {
        let definitions = hooks[event].as_array().expect("hook definitions");
        let managed = definitions.last().expect("managed definition");
        assert!(
            managed.get("matcher").is_none(),
            "{event} must omit matcher"
        );
        let command = managed["hooks"][0]["command"].as_str().expect("command");
        assert!(command.contains("devin"), "{event} must target Devin");
        assert!(command.contains("alera-runtime-agent-hook"));
    }
    assert!(hooks["SessionStart"].to_string().contains("user-start"));
    assert!(hooks["PreToolUse"].to_string().contains("user-tool"));
    assert!(!hooks["PreToolUse"].to_string().contains("/old/.alera/"));
}

#[cfg(windows)]
#[test]
fn devin_windows_hooks_avoid_nested_cmd_quoting() {
    let mut config = Map::new();
    apply_devin_hooks(
        &mut config,
        Path::new(r"C:\Users\u\Alera Hooks\alera-runtime-agent-hook.cmd"),
    );

    let command = config["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        .as_str()
        .expect("managed Devin command");

    assert_eq!(
        command,
        r#"set "ALERA_AGENT_TYPE=devin"&& set "ALERA_AGENT_HOOK_EVENT=SessionStart"&& call "C:\Users\u\Alera Hooks\alera-runtime-agent-hook.cmd""#
    );
    assert!(!command.contains("cmd /d /s /c"));
}

fn agy_bundle(config: &Map<String, Value>) -> &Map<String, Value> {
    config["alera-status"].as_object().expect("bundle object")
}

#[test]
fn agy_bundle_uses_the_documented_lifecycle_and_tool_schemas() {
    let mut config = Map::new();

    apply_agy_bundle(
        &mut config,
        Path::new("/home/user/.alera/agent-hooks/hook.sh"),
    );

    let bundle = agy_bundle(&config);
    for event in ["PreInvocation", "PostInvocation", "Stop"] {
        let definition = bundle[event].as_array().expect("array")[0].clone();
        assert_eq!(definition["type"], json!("command"));
        assert_eq!(definition["timeout"], json!(10));
        assert!(definition.get("hooks").is_none(), "{event} must stay flat");
    }
    let tool = bundle["PostToolUse"].as_array().expect("array")[0].clone();
    assert_eq!(tool["matcher"], json!("*"));
    let handler = tool["hooks"].as_array().expect("array")[0].clone();
    assert_eq!(handler["type"], json!("command"));
    // Without this the handler inherits Antigravity's documented 30s default.
    assert_eq!(handler["timeout"], json!(10));
    // Antigravity requires a permission decision from PreToolUse, so Alera's
    // observational hook must not register it.
    assert!(bundle.get("PreToolUse").is_none());
}

#[test]
fn agy_bundle_keeps_user_entries_and_drops_alera_ones() {
    let mut config = Map::from_iter([(
        "alera-status".to_string(),
        json!({
            "enabled": false,
            "Stop": [
                { "type": "command", "command": "echo user" },
                { "type": "command", "command": "/home/user/.alera/agent-hooks/alera-agy-hook.sh" },
                { "type": "command", "command": "/home/user/.alera/agent-hooks/alera-runtime-agent-hook.sh" }
            ]
        }),
    )]);

    apply_agy_bundle(
        &mut config,
        Path::new("/home/user/.alera/agent-hooks/hook.sh"),
    );

    let bundle = agy_bundle(&config);
    // Installing is an explicit enable, so the documented opt-out cannot stay.
    assert!(bundle.get("enabled").is_none());
    let stop = bundle["Stop"].as_array().expect("array");
    assert_eq!(stop.len(), 2, "one user handler plus one Alera handler");
    assert_eq!(stop[0]["command"], json!("echo user"));
    assert!(stop[1]["command"]
        .as_str()
        .expect("command")
        .contains("hook.sh"));
}

#[cfg(windows)]
#[test]
fn agy_bundle_drops_desktop_windows_wrapper_handlers() {
    let mut config = Map::from_iter([(
        "alera-status".to_string(),
        json!({
            "Stop": [
                { "type": "command", "command": "C:\\Users\\u\\.alera\\agent-hooks\\alera-agy-stop.cmd" }
            ]
        }),
    )]);

    apply_agy_bundle(
        &mut config,
        Path::new("C:\\Users\\u\\.alera\\agent-hooks\\hook.cmd"),
    );

    assert_eq!(
        agy_bundle(&config)["Stop"].as_array().expect("array").len(),
        1
    );
}
