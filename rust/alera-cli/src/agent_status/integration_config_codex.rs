use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use toml_edit::{DocumentMut, Item, Table, Value as EditValue};

use super::codex_hook_trust::codex_trusted_hash;
use super::{
    clean_managed_definitions, home_dir, is_alera_managed_definition, managed_command,
    object_field, read_json_object, write_json_object,
};

/// Installs Alera's managed Codex hooks directly into the effective Codex home.
///
/// Keeping Codex on its real home means authentication, settings, plugins,
/// history, session indexes, and resume state all remain native Codex state.
/// Alera only owns the hook definitions it appends and their trust records.
pub(super) fn prepare_codex(
    script: &Path,
    environment: &BTreeMap<String, String>,
) -> anyhow::Result<PathBuf> {
    let codex_home = effective_codex_home(environment)?;
    std::fs::create_dir_all(&codex_home)?;

    let hooks_path = codex_home.join("hooks.json");
    let mut config = read_json_object(&hooks_path)?.unwrap_or_default();
    let hooks = object_field(&mut config, "hooks");
    let events = [
        ("SessionStart", "session_start"),
        ("UserPromptSubmit", "user_prompt_submit"),
        ("PreToolUse", "pre_tool_use"),
        ("PermissionRequest", "permission_request"),
        ("PostToolUse", "post_tool_use"),
        ("Stop", "stop"),
        ("Interrupt", "interrupt"),
        ("SessionEnd", "session_end"),
    ];
    let mut trust = Vec::new();
    let mut previous_managed_trust = Vec::new();
    for (event, label) in events {
        let command = managed_command(script, "codex", event);
        let previous = hooks.remove(event);
        collect_managed_trust(&mut previous_managed_trust, previous.as_ref(), label);
        let definitions = clean_managed_definitions(previous);
        let index = definitions.len();
        let mut next = definitions;
        next.push(json!({
            "hooks": [{ "type": "command", "command": command }],
        }));
        hooks.insert(event.to_string(), Value::Array(next));
        trust.push((label, index, command));
    }
    write_json_object(&hooks_path, &config)?;

    let config_path = codex_home.join("config.toml");
    let source_config = std::fs::read_to_string(&config_path).unwrap_or_default();
    let canonical_hooks_path = dunce::canonicalize(&hooks_path).unwrap_or(hooks_path.clone());
    let managed_trust = trust
        .into_iter()
        .map(|(label, index, command)| {
            (
                format!("{}:{label}:{index}:0", canonical_hooks_path.display()),
                codex_trusted_hash(label, &command),
            )
        })
        .collect::<Vec<_>>();
    let previous_managed_trust = previous_managed_trust
        .into_iter()
        .map(|(label, group_index, handler_index, command)| {
            (
                format!(
                    "{}:{label}:{group_index}:{handler_index}",
                    canonical_hooks_path.display()
                ),
                codex_trusted_hash(label, &command),
            )
        })
        .collect::<Vec<_>>();
    let updated_config =
        update_codex_toml(&source_config, &previous_managed_trust, &managed_trust)?;
    if updated_config != source_config {
        std::fs::write(&config_path, updated_config)?;
    }
    Ok(codex_home)
}

fn effective_codex_home(environment: &BTreeMap<String, String>) -> anyhow::Result<PathBuf> {
    if let Some(configured) = environment
        .get("CODEX_HOME")
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(configured));
    }
    Ok(home_dir()?.join(".codex"))
}

fn collect_managed_trust(
    target: &mut Vec<(&'static str, usize, usize, String)>,
    value: Option<&Value>,
    event_label: &'static str,
) {
    let Some(definitions) = value.and_then(Value::as_array) else {
        return;
    };
    for (group_index, definition) in definitions.iter().enumerate() {
        if !is_alera_managed_definition(definition) {
            continue;
        }
        let Some(handlers) = definition.get("hooks").and_then(Value::as_array) else {
            continue;
        };
        for (handler_index, handler) in handlers.iter().enumerate() {
            let Some(command) = handler.get("command").and_then(Value::as_str) else {
                continue;
            };
            if is_alera_managed_definition(handler) {
                target.push((event_label, group_index, handler_index, command.to_string()));
            }
        }
    }
}

fn update_codex_toml(
    source_config: &str,
    previous_managed_trust: &[(String, String)],
    trust: &[(String, String)],
) -> anyhow::Result<String> {
    let mut document = source_config
        .parse::<DocumentMut>()
        .map_err(|error| anyhow::anyhow!("Failed to parse Codex config.toml: {error}"))?;

    let features = edit_table_field(document.as_table_mut(), "features", false)?;
    set_edit_value(features, "hooks", EditValue::from(true));

    let hooks = edit_table_field(document.as_table_mut(), "hooks", true)?;
    let state = edit_table_field(hooks, "state", true)?;
    for (key, hash) in previous_managed_trust {
        let matches_managed_hash = state
            .get(key)
            .and_then(Item::as_table)
            .and_then(|entry| entry.get("trusted_hash"))
            .and_then(Item::as_str)
            .is_some_and(|current| current == hash);
        if matches_managed_hash {
            state.remove(key);
        }
    }
    for (key, hash) in trust {
        let entry = edit_table_field(state, key, false)?;
        set_edit_value(entry, "enabled", EditValue::from(true));
        set_edit_value(entry, "trusted_hash", EditValue::from(hash.as_str()));
    }

    Ok(document.to_string())
}

fn edit_table_field<'a>(
    table: &'a mut Table,
    key: &str,
    implicit_when_created: bool,
) -> anyhow::Result<&'a mut Table> {
    if !table.contains_key(key) {
        let mut child = Table::new();
        child.set_implicit(implicit_when_created);
        table.insert(key, Item::Table(child));
    }
    let child = table
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| anyhow::anyhow!("Codex config.toml key `{key}` is not a table"))?;
    if !implicit_when_created {
        child.set_implicit(false);
    }
    Ok(child)
}

fn set_edit_value(table: &mut Table, key: &str, mut next: EditValue) {
    if let Some(existing) = table.get_mut(key).and_then(Item::as_value_mut) {
        let decor = existing.decor().clone();
        *next.decor_mut() = decor;
        *existing = next;
        return;
    }
    table.insert(key, Item::Value(next));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_installs_interrupt_and_session_end_hooks() {
        let home = tempfile::tempdir().expect("Codex home");
        let mut environment = BTreeMap::new();
        environment.insert(
            "CODEX_HOME".to_string(),
            home.path().to_string_lossy().into_owned(),
        );

        prepare_codex(Path::new("/tmp/alera-runtime-agent-hook.sh"), &environment)
            .expect("prepare Codex hooks");

        let config = read_json_object(&home.path().join("hooks.json"))
            .expect("read hooks")
            .expect("hooks object");
        let hooks = config["hooks"].as_object().expect("hooks map");
        assert!(hooks.contains_key("Interrupt"));
        assert!(hooks.contains_key("SessionEnd"));
    }

    #[test]
    fn codex_toml_update_preserves_user_comments_and_formatting() {
        let source = concat!(
            "# user preamble\n",
            "model = \"gpt-5\" # keep this comment\n",
            "\n",
            "[features] # keep section note\n",
            "hooks = false # keep hook note\n",
            "experimental = true # untouched\n",
            "\n",
            "[projects.\"C:\\\\repo\"]\n",
            "trust_level = \"trusted\" # keep project note\n",
        );
        let trust = vec![(
            r"C:\Users\u\.codex\hooks.json:session_start:0:0".to_string(),
            "sha256:managed".to_string(),
        )];

        let updated = update_codex_toml(source, &[], &trust).expect("updated Codex TOML");

        assert!(updated.contains("# user preamble"));
        assert!(updated.contains("model = \"gpt-5\" # keep this comment"));
        assert!(updated.contains("[features] # keep section note"));
        assert!(updated.contains("hooks = true # keep hook note"));
        assert!(updated.contains("experimental = true # untouched"));
        assert!(updated.contains("trust_level = \"trusted\" # keep project note"));
        assert!(updated.contains("enabled = true"));
        assert!(updated.contains("trusted_hash = \"sha256:managed\""));
        let parsed =
            toml::from_str::<toml::Value>(&updated).expect("valid TOML after managed edit");
        let managed = &parsed["hooks"]["state"][&trust[0].0];
        assert_eq!(managed["enabled"].as_bool(), Some(true));
        assert_eq!(managed["trusted_hash"].as_str(), Some("sha256:managed"));
    }

    #[test]
    fn codex_toml_update_removes_only_matching_stale_managed_trust() {
        let source = concat!(
            "[hooks.state.\"/home/u/.codex/hooks.json:stop:2:0\"]\n",
            "enabled = true\n",
            "trusted_hash = \"sha256:old-managed\"\n",
            "\n",
            "[hooks.state.\"/home/u/.codex/hooks.json:stop:7:0\"]\n",
            "enabled = true\n",
            "trusted_hash = \"sha256:user\"\n",
        );
        let previous = vec![(
            "/home/u/.codex/hooks.json:stop:2:0".to_string(),
            "sha256:old-managed".to_string(),
        )];
        let next = vec![(
            "/home/u/.codex/hooks.json:stop:1:0".to_string(),
            "sha256:new-managed".to_string(),
        )];

        let updated = update_codex_toml(source, &previous, &next).expect("updated Codex TOML");
        let parsed = toml::from_str::<toml::Value>(&updated).expect("valid TOML");
        let state = parsed["hooks"]["state"]
            .as_table()
            .expect("hook state table");

        assert!(!state.contains_key("/home/u/.codex/hooks.json:stop:2:0"));
        assert_eq!(
            state["/home/u/.codex/hooks.json:stop:7:0"]["trusted_hash"].as_str(),
            Some("sha256:user")
        );
        assert_eq!(
            state["/home/u/.codex/hooks.json:stop:1:0"]["trusted_hash"].as_str(),
            Some("sha256:new-managed")
        );
    }

    #[test]
    fn codex_toml_update_rejects_invalid_user_config_without_rebuilding_it() {
        let source = "model = [invalid\n# keep me\n";
        let result = update_codex_toml(source, &[], &[]);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse Codex config.toml"));
    }
}
