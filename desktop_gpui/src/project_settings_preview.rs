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
