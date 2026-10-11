use std::path::Path;

use serde_json::{json, Value};

use super::{
    clean_managed_definitions, managed_command, object_field, read_json_object, write_json_object,
};

/// Events Alera registers with Cursor. `sessionStart` is deliberately absent:
/// it fires when the CLI opens, before any prompt, and the normalizer maps it
/// to `working`, which would mark an idle terminal as busy.
pub(super) const CURSOR_HOOK_EVENTS: [&str; 11] = [
    "beforeSubmitPrompt",
    "stop",
    "sessionEnd",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
    "beforeShellExecution",
    "afterShellExecution",
    "beforeMCPExecution",
    "afterMCPExecution",
    "afterAgentResponse",
];

/// Cursor's own default is 60s, long enough for one unreachable socket to hold
/// an agent turn. The hook only posts to a loopback port.
const CURSOR_HOOK_TIMEOUT_SECONDS: u64 = 5;

pub(super) fn install_cursor(script: &Path) -> anyhow::Result<()> {
    install_user_hooks(&super::home_dir()?, script)
}

fn install_user_hooks(home: &Path, script: &Path) -> anyhow::Result<()> {
    let path = home.join(".cursor/hooks.json");
    let config = match read_json_object(&path)? {
        Some(config) => config,
        None if !path.exists() => Default::default(),
        None => anyhow::bail!("Cursor hooks.json must contain an object"),
    };
    if config.get("hooks").is_some_and(|hooks| !hooks.is_object()) {
        anyhow::bail!("Cursor hooks.json hooks must contain an object");
    }
    let mut updated = config.clone();
    updated.entry("version").or_insert(json!(1));
    let hooks = object_field(&mut updated, "hooks");
    for event in CURSOR_HOOK_EVENTS {
        let mut definitions = clean_managed_definitions(hooks.remove(event));
        definitions.push(json!({
            "command": managed_command(script, "cursor", event),
            "timeout": CURSOR_HOOK_TIMEOUT_SECONDS,
        }));
        hooks.insert(event.to_string(), Value::Array(definitions));
    }
    if updated == config {
        return Ok(());
    }
    write_json_object(&path, &updated)
}

pub(super) fn cleanup_user_hooks(home: &Path) -> anyhow::Result<()> {
    super::user_hooks::cleanup_managed_hooks_file(&home.join(".cursor/hooks.json"))
}

// Only host startup may remove overlays: it owns no live terminal sessions yet.
pub(super) fn clear_stale_state(runtime_dir: &Path) -> anyhow::Result<()> {
    let root = runtime_dir.join("agent-runtime-overlays/cursor");
    if root.exists() {
        std::fs::remove_dir_all(root)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "integration_config_cursor_tests.rs"]
mod tests;
