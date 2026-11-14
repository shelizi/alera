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
        Some(Value::String(s)) if matches!(s.as_str(), "github" | "gitlab" | "bitbucket") => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_catalog::{RuntimeProject, RuntimeWorkspace};
    use std::path::PathBuf;

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
