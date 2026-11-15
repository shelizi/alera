//! Safe, read-only Native projection of Classic Host-owned ProjectConfig.
//! Never show setup command text or workspace prompts in an always-visible
//! Settings search preview; the Host retains exclusive configuration ownership.
use crate::runtime_catalog::RuntimeCatalog;
use serde_json::Value;
use std::path::Path;

/// Find the registered project owning this exact workspace, including linked
/// worktrees whose checkout path is not the primary project repository root.
pub fn project_id_for_workspace<'a>(
    catalog: &'a RuntimeCatalog,
    workspace: &Path,
) -> Option<&'a str> {
    if let Some(project_id) = catalog
        .workspaces
        .iter()
        .find(|item| item.path == workspace)
        .map(|item| item.project_id.as_str())
    {
        return catalog
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| project.id.as_str());
    }
    catalog
        .projects
        .iter()
        .find(|project| project.root == workspace)
        .map(|project| project.id.as_str())
}

/// Resolve the Project Settings selection only against the trusted Host
/// catalog. An explicit selection never changes the active workspace; if a
/// project disappears during refresh, fall back to the active workspace's
/// registered project or Classic's first registered project.
pub fn selected_project_for_settings<'a>(
    catalog: &'a RuntimeCatalog,
    workspace: &Path,
    selected_id: Option<&str>,
) -> Option<&'a str> {
    if let Some(id) = selected_id {
        if let Some(project) = catalog.projects.iter().find(|project| project.id == id) {
            return Some(project.id.as_str());
        }
    }
    project_id_for_workspace(catalog, workspace)
        .or_else(|| catalog.projects.first().map(|project| project.id.as_str()))
}

/// Display useful settings facts without echoing arbitrary setup commands,
/// copied paths, secrets or new-workspace prompt text into UI search results.
pub fn project_settings_summary(value: &Value) -> Result<Vec<(String, String)>, &'static str> {
    let config = value.as_object().ok_or("Invalid Host ProjectConfig")?;
    let worktree = config.get("worktree").and_then(Value::as_object);
    let copy = worktree
        .and_then(|item| item.get("copy"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let setup = worktree
        .and_then(|item| item.get("setup"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let prompt = config
        .get("newWorkspace")
        .and_then(Value::as_object)
        .and_then(|item| item.get("promptAppend"))
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    let hosting = match config.get("gitHostingProvider") {
        None | Some(Value::Null) => "Auto-detect",
        Some(Value::String(s)) if matches!(s.as_str(), "github" | "gitlab" | "azureDevops") => {
            return Ok(vec![
                ("Git hosting".into(), s.clone()),
                ("Worktree copy rules".into(), copy.to_string()),
                ("Worktree setup commands".into(), setup.to_string()),
                (
                    "New-workspace prompt".into(),
                    if prompt { "Configured" } else { "Default" }.into(),
                ),
            ]);
        }
        _ => "Custom override",
    };
    Ok(vec![
        ("Git hosting".into(), hosting.into()),
        ("Worktree copy rules".into(), copy.to_string()),
        ("Worktree setup commands".into(), setup.to_string()),
        (
            "New-workspace prompt".into(),
            if prompt { "Configured" } else { "Default" }.into(),
        ),
    ])
}

/// Strict mirror of the Host ProjectConfig wire shape. Native deliberately
/// does not depend on alera-core's heavy `runtime` feature; that feature would
/// drag RuntimeStore/SQLx and PTY ownership into the standalone UI. Reject
/// unknown data instead of silently dropping it on the Host's replace-all CLI.
#[derive(Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SafeHostProjectConfig {
    #[serde(default)]
    worktree: SafeHostWorktreeConfig,
    #[serde(default)]
    new_workspace: SafeHostNewWorkspaceConfig,
    #[serde(default)]
    git_hosting_provider: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SafeHostWorktreeConfig {
    #[serde(default)]
    copy: Vec<SafeHostCopyRule>,
    #[serde(default)]
    setup: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SafeHostCopyRule {
    from: String,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SafeHostNewWorkspaceConfig {
    #[serde(default)]
    prompt_append: String,
}

/// Classic/GPUI ProjectConfig writes replace the ENTIRE Host record. Reject
/// unexpected fields rather than dropping unknown future data by round-tripping
/// through a narrower Rust struct, and patch only the fields this Native form
/// can edit. Worktree copy rules, setup commands and their exact contents must
/// survive every safe update byte-for-byte at the JSON-value level.
pub fn updated_project_config(
    observed: &Value,
    provider: Option<&str>,
    prompt_append: &str,
) -> Result<Value, &'static str> {
    fn known_fields(value: &serde_json::Map<String, Value>, allowed: &[&str]) -> bool {
        value.keys().all(|key| allowed.contains(&key.as_str()))
    }

    let root = observed
        .as_object()
        .ok_or("Host ProjectConfig is not an object")?;
    if !known_fields(root, &["worktree", "newWorkspace", "gitHostingProvider"]) {
        return Err("Host ProjectConfig contains unknown fields; edit in Classic");
    }
    if let Some(worktree) = root.get("worktree") {
        let worktree = worktree
            .as_object()
            .ok_or("Invalid Host worktree settings")?;
        if !known_fields(worktree, &["copy", "setup"]) {
            return Err("Host worktree settings contain unknown fields; edit in Classic");
        }
        if let Some(copy) = worktree.get("copy") {
            let copy = copy.as_array().ok_or("Invalid Host copy rules")?;
            if !copy.iter().all(|rule| {
                rule.as_object()
                    .is_some_and(|map| known_fields(map, &["from", "to", "overwrite"]))
            }) {
                return Err("Host copy rules contain unsupported fields");
            }
        }
    }
    if let Some(new_workspace) = root.get("newWorkspace") {
        let new_workspace = new_workspace
            .as_object()
            .ok_or("Invalid new workspace settings")?;
        if !known_fields(new_workspace, &["promptAppend"]) {
            return Err("Host new workspace settings contain unsupported fields");
        }
    }

    // The effective record must satisfy the actual Runtime Host model before
    // we consider replacing it. In particular no arbitrary payload is pushed
    // through --config-json to the CLI.
    let _: SafeHostProjectConfig = serde_json::from_value(observed.clone())
        .map_err(|_| "Host ProjectConfig does not match the supported schema")?;

    let current_provider = root.get("gitHostingProvider").and_then(Value::as_str);
    let provider_allowed = provider.is_none()
        || provider.is_some_and(|value| {
            matches!(value, "github" | "gitlab" | "azureDevops") || Some(value) == current_provider
        });
    if !provider_allowed {
        return Err("Unsupported Git hosting provider");
    }
    if prompt_append.len() > 8 * 1024 || prompt_append.contains('\0') {
        return Err("Project prompt must be at most 8 KiB and contain no NUL");
    }

    let mut config = observed.clone();
    let map = config
        .as_object_mut()
        .expect("checked Host ProjectConfig object");
    if let Some(provider) = provider {
        map.insert(
            "gitHostingProvider".to_owned(),
            Value::String(provider.to_owned()),
        );
    } else {
        map.remove("gitHostingProvider");
    }
    let new_workspace = map
        .entry("newWorkspace")
        .or_insert_with(|| serde_json::json!({}));
    new_workspace
        .as_object_mut()
        .ok_or("Invalid new workspace settings")?
        .insert(
            "promptAppend".into(),
            Value::String(prompt_append.to_owned()),
        );
    let _: SafeHostProjectConfig = serde_json::from_value(config.clone())
        .map_err(|_| "Edited ProjectConfig does not match Runtime Host schema")?;
    if serde_json::to_string(&config)
        .map_err(|_| "Could not encode ProjectConfig")?
        .len()
        > 32 * 1024
    {
        return Err("Edited ProjectConfig exceeds Host size limit");
    }
    Ok(config)
}

/// Editable Native copy-rule draft. The UI never expands this in the Settings
/// search preview; only an explicit Project Settings Edit reveals its paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCopyRuleDraft {
    pub from: String,
    pub to: String,
    pub overwrite: bool,
}

pub const MAX_PROJECT_RULES: usize = 64;
pub const MAX_PROJECT_PATH_BYTES: usize = 512;
pub const MAX_PROJECT_SETUP_COMMAND_BYTES: usize = 4 * 1024;

/// Decode the exact Host ProjectConfig as source-of-truth, with bounded Native
/// input controls. Never truncate existing commands or paths silently.
pub fn project_worktree_editor_seed(
    original: &Value,
) -> Result<(Vec<ProjectCopyRuleDraft>, Vec<String>), &'static str> {
    let parsed: SafeHostProjectConfig = serde_json::from_value(original.clone())
        .map_err(|_| "Host ProjectConfig contains unsupported fields")?;
    if parsed.worktree.copy.len() > MAX_PROJECT_RULES
        || parsed.worktree.setup.len() > MAX_PROJECT_RULES
    {
        return Err("Too many Worktree rules to edit safely in Native");
    }
    let mut rules = Vec::with_capacity(parsed.worktree.copy.len());
    for rule in parsed.worktree.copy {
        let to = rule.to.unwrap_or_default();
        if rule.from.len() > MAX_PROJECT_PATH_BYTES || to.len() > MAX_PROJECT_PATH_BYTES {
            return Err("Worktree copy path exceeds Native editor limit");
        }
        rules.push(ProjectCopyRuleDraft {
            from: rule.from,
            to,
            overwrite: rule.overwrite,
        });
    }
    if parsed
        .worktree
        .setup
        .iter()
        .any(|command| command.len() > MAX_PROJECT_SETUP_COMMAND_BYTES || command.contains('\0'))
    {
        return Err("Worktree setup command exceeds Native editor limit");
    }
    Ok((rules, parsed.worktree.setup))
}

fn normalize_project_copy_path(path: &str) -> Result<String, &'static str> {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || normalized.contains('\0')
    {
        return Err("Worktree copy paths must be relative and non-empty");
    }
    let mut parts = Vec::new();
    for segment in normalized.split('/') {
        if segment == ".." {
            return Err("Worktree copy paths cannot leave the project");
        }
        if segment.is_empty() || segment == "." {
            continue;
        }
        parts.push(segment);
    }
    if parts.is_empty() {
        return Err("Worktree copy paths must contain a file or folder name");
    }
    let output = parts.join("/");
    if output.len() > MAX_PROJECT_PATH_BYTES {
        return Err("Worktree copy path exceeds Native editor limit");
    }
    Ok(output)
}

/// Apply *structured* Copy Rules + Setup Commands to S07's fully validated
/// Host config. A no-op draft preserves the original worktree JSON exactly.
/// Other Host project fields are retained; the Host's replace-all operation is
/// still preceded by an external refresh-compare and followed by readback.
pub fn updated_project_config_with_worktree(
    original: &Value,
    provider: Option<&str>,
    prompt_append: &str,
    copy_rules: &[ProjectCopyRuleDraft],
    setup_commands: &[String],
) -> Result<Value, &'static str> {
    let mut updated = updated_project_config(original, provider, prompt_append)?;
    if copy_rules.len() > MAX_PROJECT_RULES || setup_commands.len() > MAX_PROJECT_RULES {
        return Err("Too many Worktree rules");
    }
    let (original_copy, original_setup) = project_worktree_editor_seed(original)?;
    if copy_rules == original_copy && setup_commands == original_setup {
        // Changing only Hosting/Prompt must not rewrite or normalize any
        // existing Worktree copy path, shell whitespace or command bytes.
        return Ok(updated);
    }
    let mut copy_json = Vec::with_capacity(copy_rules.len());
    for rule in copy_rules {
        let from = normalize_project_copy_path(&rule.from)?;
        let to = if rule.to.trim().is_empty() {
            None
        } else {
            Some(normalize_project_copy_path(&rule.to)?)
        };
        let mut serialized = serde_json::json!({
            "from": from,
            "overwrite": rule.overwrite,
        });
        if let Some(to) = to {
            serialized["to"] = Value::String(to);
        }
        copy_json.push(serialized);
    }
    let mut setup_json = Vec::with_capacity(setup_commands.len());
    for command in setup_commands {
        if command.len() > MAX_PROJECT_SETUP_COMMAND_BYTES || command.contains('\0') {
            return Err("Worktree setup command exceeds the 4 KiB limit");
        }
        if !command.trim().is_empty() {
            // Preserve leading/trailing whitespace in real shell commands.
            setup_json.push(Value::String(command.to_owned()));
        }
    }
    let current_worktree = original
        .get("worktree")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let mut next_worktree = current_worktree.clone();
    let map = next_worktree
        .as_object_mut()
        .ok_or("Invalid Host Worktree settings")?;
    map.insert("copy".to_owned(), Value::Array(copy_json));
    map.insert("setup".to_owned(), Value::Array(setup_json));
    // Preserve worktree byte-level JSON representation if structurally
    // unchanged (e.g. absent overwrite:false, absent empty list fields).
    let current_typed: SafeHostWorktreeConfig = serde_json::from_value(current_worktree)
        .map_err(|_| "Host Worktree settings contain unsupported fields")?;
    let new_typed: SafeHostWorktreeConfig = serde_json::from_value(next_worktree.clone())
        .map_err(|_| "Edited Worktree settings are not valid")?;
    if current_typed != new_typed {
        updated
            .as_object_mut()
            .ok_or("Invalid ProjectConfig root")?
            .insert("worktree".to_owned(), next_worktree);
    }
    let _: SafeHostProjectConfig = serde_json::from_value(updated.clone())
        .map_err(|_| "Edited ProjectConfig does not match Runtime Host schema")?;
    if serde_json::to_string(&updated)
        .map_err(|_| "Could not encode ProjectConfig")?
        .len()
        > 32 * 1024
    {
        return Err("Edited ProjectConfig exceeds Host size limit");
    }
    Ok(updated)
}

/// Compare a completed write with Host readback using the same typed Runtime
/// Host schema; absent fields and explicit defaults are equivalent.
pub fn host_config_matches(expected: &Value, readback: &Value) -> bool {
    let left = serde_json::from_value::<SafeHostProjectConfig>(expected.clone());
    let right = serde_json::from_value::<SafeHostProjectConfig>(readback.clone());
    matches!((left, right), (Ok(a), Ok(b)) if a == b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_catalog::{RuntimeProject, RuntimeWorkspace};
    use std::path::PathBuf;

    #[test]
    fn unedited_worktree_paths_and_setup_whitespace_are_exactly_preserved() {
        let original = serde_json::json!({
            "worktree": {
                "copy": [{"from": "src\\windows\\file", "to": "out/./file"}],
                "setup": ["  export TOKEN=private_value  ", "echo first\necho second\n"]
            },
            "newWorkspace": {"promptAppend":"old"},
            "gitHostingProvider": "github",
        });
        let (copy, setup) = project_worktree_editor_seed(&original).unwrap();
        let updated = updated_project_config_with_worktree(
            &original,
            Some("gitlab"),
            "new prompt",
            &copy,
            &setup,
        )
        .unwrap();
        assert_eq!(updated["worktree"], original["worktree"]);
        assert_eq!(updated["gitHostingProvider"], "gitlab");
        assert_eq!(updated["newWorkspace"]["promptAppend"], "new prompt");
    }

    #[test]
    fn worktree_edit_can_add_modify_and_delete_without_touching_unedited_fields() {
        let initial = serde_json::json!({
            "gitHostingProvider": "github",
            "newWorkspace": {"promptAppend": "existing"},
            "worktree": {
                "copy": [{"from": ".env", "overwrite": false}, {"from": "src", "to": "src-copy", "overwrite": true}],
                "setup": ["pnpm install", "echo old"]
            }
        });
        let (mut copy, mut setup) = project_worktree_editor_seed(&initial).unwrap();
        assert_eq!(copy.len(), 2);
        assert_eq!(setup.len(), 2);
        let untouched = updated_project_config_with_worktree(
            &initial,
            Some("github"),
            "existing",
            &copy,
            &setup,
        )
        .unwrap();
        assert_eq!(untouched["worktree"], initial["worktree"]);
        copy.remove(0);
        copy[0].to = "changed".into();
        copy[0].overwrite = false;
        copy.push(ProjectCopyRuleDraft {
            from: "docs/./manual".into(),
            to: "".into(),
            overwrite: true,
        });
        setup.remove(1);
        setup.push("make bootstrap".into());
        let edited =
            updated_project_config_with_worktree(&initial, Some("gitlab"), "new", &copy, &setup)
                .unwrap();
        assert_eq!(edited["gitHostingProvider"], "gitlab");
        assert_eq!(edited["newWorkspace"]["promptAppend"], "new");
        assert_eq!(
            edited["worktree"]["copy"],
            serde_json::json!([
                {"from":"src","to":"changed","overwrite":false},
                {"from":"docs/manual","overwrite":true},
            ])
        );
        assert_eq!(
            edited["worktree"]["setup"],
            serde_json::json!(["pnpm install", "make bootstrap"])
        );
        assert!(!host_config_matches(&initial, &edited));
        assert!(host_config_matches(&edited, &edited));
        let cleared = updated_project_config_with_worktree(&edited, None, "", &[], &[]).unwrap();
        assert_eq!(cleared["worktree"]["copy"], serde_json::json!([]));
        assert_eq!(cleared["worktree"]["setup"], serde_json::json!([]));
    }

    #[test]
    fn worktree_edit_rejects_relative_escape_windows_absolute_paths_and_bounds() {
        let original = serde_json::json!({
            "worktree": {"copy": [], "setup": []},
            "newWorkspace": {"promptAppend": ""},
        });
        let make = |path: &str| {
            vec![ProjectCopyRuleDraft {
                from: path.to_owned(),
                to: String::new(),
                overwrite: false,
            }]
        };
        for path in [
            "",
            ".",
            "..",
            "../.env",
            "foo/../outside",
            "C:\\secrets",
            "/etc/passwd",
            "\\\\server\\share",
        ] {
            assert!(
                updated_project_config_with_worktree(&original, None, "", &make(path), &[],)
                    .is_err(),
                "{path} should be rejected"
            );
        }
        assert!(updated_project_config_with_worktree(
            &original,
            None,
            "",
            &make("safe/path"),
            &["x".repeat(4097)],
        )
        .is_err());
        assert!(updated_project_config_with_worktree(
            &original,
            None,
            "",
            &make("safe/path"),
            &vec!["ok".to_owned(); MAX_PROJECT_RULES + 1],
        )
        .is_err());
        assert!(updated_project_config_with_worktree(
            &original,
            None,
            "",
            &vec![
                ProjectCopyRuleDraft {
                    from: "x".into(),
                    to: "".into(),
                    overwrite: false,
                };
                MAX_PROJECT_RULES + 1
            ],
            &[],
        )
        .is_err());
        assert!(project_worktree_editor_seed(&serde_json::json!({
            "worktree":{"copy":[{"from":"x".repeat(MAX_PROJECT_PATH_BYTES + 1)}]}
        }))
        .is_err());
    }

    #[test]
    fn edits_only_explicit_safe_fields_and_preserves_full_worktree_config() {
        let original = serde_json::json!({
            "gitHostingProvider": "github",
            "worktree": {
                "copy": [{"from": ".env", "to": "secrets/.env", "overwrite": true}],
                "setup": ["export SECRET=hunter2", "pnpm install"]
            },
            "newWorkspace": {"promptAppend": "existing project prompt"}
        });
        let updated =
            updated_project_config(&original, Some("azureDevops"), "new\nproject prompt").unwrap();
        assert_eq!(updated["gitHostingProvider"], "azureDevops");
        assert_eq!(
            updated["newWorkspace"]["promptAppend"],
            "new\nproject prompt"
        );
        assert_eq!(updated["worktree"], original["worktree"]);
        assert_eq!(original["gitHostingProvider"], "github");
        assert!(host_config_matches(&updated, &updated));
        assert!(!host_config_matches(&updated, &original));
        let cleared = updated_project_config(&updated, None, "").unwrap();
        assert!(cleared.get("gitHostingProvider").is_none());
        assert_eq!(cleared["worktree"], original["worktree"]);
        assert!(host_config_matches(
            &cleared,
            &serde_json::json!({
                "worktree": original["worktree"],
                "newWorkspace": {"promptAppend": ""}
            })
        ));
    }

    #[test]
    fn refuses_unknown_future_fields_unrecognized_providers_and_overlong_prompts() {
        let base = serde_json::json!({
            "worktree": {"copy": [{"from": "README.md"}], "setup": []},
            "newWorkspace": {"promptAppend": "original"},
            "gitHostingProvider": "gitlab",
        });
        assert!(updated_project_config(&base, Some("bitbucket"), "replacement").is_err());
        assert!(updated_project_config(&base, Some("github"), &"x".repeat(8193)).is_err());
        assert!(updated_project_config(&base, None, "bad\0prompt").is_err());
        let mut future = base.clone();
        future["worktree"]["unsafeFutureField"] = serde_json::json!(true);
        assert!(updated_project_config(&future, Some("github"), "replacement").is_err());
        future = base.clone();
        future["worktree"]["copy"][0]["future"] = serde_json::json!("preserve!");
        assert!(updated_project_config(&future, Some("github"), "replacement").is_err());
        future = base.clone();
        future["mystery"] = serde_json::json!("unknown");
        assert!(updated_project_config(&future, Some("github"), "replacement").is_err());
        // An opaque Host override may remain intact when editing only
        // the prompt, though a new arbitrary override cannot be introduced.
        future = base;
        future["gitHostingProvider"] = serde_json::json!("customEnterpriseForge");
        assert_eq!(
            updated_project_config(&future, Some("customEnterpriseForge"), "new").unwrap()
                ["gitHostingProvider"],
            "customEnterpriseForge"
        );
    }

    #[test]
    fn s10_selects_any_registered_host_project_without_switching_workspace() {
        let mut catalog = RuntimeCatalog::default();
        catalog.projects.push(RuntimeProject {
            id: "current".into(),
            name: "Current workspace".into(),
            root: PathBuf::from("/current-repo"),
        });
        catalog.projects.push(RuntimeProject {
            id: "other".into(),
            name: "Other registered project".into(),
            root: PathBuf::from("/other-repo"),
        });
        catalog.workspaces.push(RuntimeWorkspace {
            id: "current-ws".into(),
            project_id: "current".into(),
            name: "feature".into(),
            path: PathBuf::from("/current-worktree"),
            branch: None,
            is_pinned: false,
            tag_names: vec![],
            tag_ids: vec![],
            section_id: None,
            parent_workspace_id: None,
            is_archived: false,
        });
        let active = Path::new("/current-worktree");
        assert_eq!(
            selected_project_for_settings(&catalog, active, None),
            Some("current")
        );
        assert_eq!(
            selected_project_for_settings(&catalog, active, Some("other")),
            Some("other")
        );
        assert_eq!(
            selected_project_for_settings(&catalog, active, Some("unknown-or-deleted")),
            Some("current")
        );
        assert_eq!(
            selected_project_for_settings(&catalog, Path::new("/unregistered"), None),
            Some("current")
        );
        assert_eq!(
            selected_project_for_settings(&catalog, Path::new("/unregistered"), Some("other")),
            Some("other")
        );
        catalog.projects.clear();
        assert_eq!(
            selected_project_for_settings(&catalog, active, Some("other")),
            None
        );
    }

    #[test]
    fn linked_workspace_resolves_its_host_project_not_an_unrelated_repo() {
        let mut catalog = RuntimeCatalog::default();
        catalog.projects.push(RuntimeProject {
            id: "p".into(),
            name: "Alera".into(),
            root: PathBuf::from("/repo"),
        });
        catalog.workspaces.push(RuntimeWorkspace {
            id: "w".into(),
            project_id: "p".into(),
            name: "feature".into(),
            path: PathBuf::from("/external/linked"),
            branch: None,
            is_pinned: false,
            tag_names: vec![],
            tag_ids: vec![],
            section_id: None,
            parent_workspace_id: None,
            is_archived: false,
        });
        assert_eq!(
            project_id_for_workspace(&catalog, Path::new("/external/linked")),
            Some("p")
        );
        assert_eq!(
            project_id_for_workspace(&catalog, Path::new("/repo")),
            Some("p")
        );
        assert_eq!(
            project_id_for_workspace(&catalog, Path::new("/unknown")),
            None
        );
    }

    #[test]
    fn summary_never_echoes_command_secret_copy_path_or_prompt_body() {
        let value = serde_json::json!({
            "worktree": {
                "setup": ["export API_KEY=secret-value"],
                "copy": [{"from": "/secrets/credentials.json"}],
            },
            "newWorkspace": {"promptAppend": "private project instructions"},
            "gitHostingProvider": "github",
        });
        let rows = project_settings_summary(&value).unwrap();
        let rendered = format!("{rows:?}");
        assert_eq!(rows.len(), 4);
        assert!(rendered.contains("Worktree setup commands"));
        assert!(rendered.contains("Configured"));
        for forbidden in [
            "secret-value",
            "credentials.json",
            "private project instructions",
        ] {
            assert!(!rendered.contains(forbidden));
        }
        assert!(project_settings_summary(&serde_json::json!([])).is_err());
    }
}
