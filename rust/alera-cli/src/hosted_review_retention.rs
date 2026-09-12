use std::collections::HashSet;
use std::path::{Component, Path};

use alera_core::runtime::{RuntimeStore, WorkspaceTabRecord};

#[path = "hosted_review_operation_liveness.rs"]
mod operation_liveness;

use operation_liveness::active_operation_ids;

const GIT_DIFF_ROOT_KEY: &str = "gitDiffRoot";
const RETENTION_ID_KEY: &str = "gitDiffHostedReviewRetentionId";

pub(crate) struct HostedReviewRetention {
    repo_path: String,
    fallback_repo_path: Option<String>,
    retention_id: String,
}

pub(crate) async fn for_tab(store: &RuntimeStore, tab_id: &str) -> Vec<HostedReviewRetention> {
    let Ok(Some(tab)) = store.find_workspace_tab(tab_id).await else {
        return Vec::new();
    };
    for_records(store, [tab]).await
}

pub(crate) async fn for_workspace(
    store: &RuntimeStore,
    workspace_id: &str,
) -> Vec<HostedReviewRetention> {
    let Ok(tabs) = store.list_workspace_tabs(workspace_id).await else {
        return Vec::new();
    };
    for_records(store, tabs).await
}

pub(crate) async fn for_project(
    store: &RuntimeStore,
    project_id: &str,
) -> Vec<HostedReviewRetention> {
    let Ok(workspaces) = store.list_workspaces(project_id).await else {
        return Vec::new();
    };
    let mut retentions = Vec::new();
    for workspace in workspaces {
        retentions.extend(for_workspace(store, &workspace.id).await);
    }
    retentions
}

pub(crate) fn release(retentions: Vec<HostedReviewRetention>) {
    for retention in retentions {
        let released = alera_core::git::hosted_review::release_hosted_review_range(
            &retention.repo_path,
            &retention.retention_id,
        );
        if released.is_err() {
            if let Some(fallback_repo_path) = retention.fallback_repo_path {
                let _ = alera_core::git::hosted_review::release_hosted_review_range(
                    &fallback_repo_path,
                    &retention.retention_id,
                );
            }
        }
    }
}

pub(crate) async fn remove_project(store: RuntimeStore, project_id: &str) -> anyhow::Result<()> {
    let retentions = for_project(&store, project_id).await;
    store.remove_project(project_id).await?;
    release(retentions);
    Ok(())
}

pub(crate) async fn remove_workspace(
    store: RuntimeStore,
    workspace_id: &str,
) -> anyhow::Result<()> {
    let retentions = for_workspace(&store, workspace_id).await;
    store.remove_workspace(workspace_id, true).await?;
    release(retentions);
    Ok(())
}

pub(crate) async fn reconcile(store: &RuntimeStore) {
    let Ok(projects) = store.list_projects().await else {
        return;
    };
    let Ok(workspaces) = store.list_all_workspaces().await else {
        return;
    };
    let mut repo_paths = projects
        .into_iter()
        .map(|project| project.repo_path)
        .chain(workspaces.iter().map(|workspace| workspace.path.clone()))
        .collect::<HashSet<_>>();
    let operations = alera_core::git::hosted_review::hosted_review_operations();
    repo_paths.extend(
        operations
            .iter()
            .map(|operation| operation.repo_path.clone()),
    );
    let mut retentions = Vec::new();
    for workspace in workspaces {
        let Ok(tabs) = store.list_workspace_tabs(&workspace.id).await else {
            continue;
        };
        retentions.extend(for_records(store, tabs).await);
    }
    let active_operation_ids = active_operation_ids(&operations);
    let stale_operation_ids = operations
        .iter()
        .filter(|operation| !active_operation_ids.contains(&operation.retention_id))
        .map(|operation| operation.retention_id.clone())
        .collect::<HashSet<_>>();
    let retained_ids = retentions
        .iter()
        .map(|retention| retention.retention_id.clone())
        .chain(active_operation_ids)
        .collect::<HashSet<_>>();
    for retention in &retentions {
        repo_paths.insert(retention.repo_path.clone());
        let persisted = alera_core::git::hosted_review::persist_hosted_review_range(
            &retention.repo_path,
            &retention.retention_id,
        );
        if persisted.is_err() {
            if let Some(fallback) = &retention.fallback_repo_path {
                repo_paths.insert(fallback.clone());
                let _ = alera_core::git::hosted_review::persist_hosted_review_range(
                    fallback,
                    &retention.retention_id,
                );
            }
        }
    }
    let retained_ids = retained_ids.into_iter().collect::<Vec<_>>();
    let stale_operation_ids = stale_operation_ids.into_iter().collect::<Vec<_>>();
    let mut swept_repo_paths = HashSet::new();
    for repo_path in repo_paths {
        if alera_core::git::hosted_review::sweep_hosted_review_ranges(
            &repo_path,
            &retained_ids,
            &stale_operation_ids,
        )
        .is_ok()
        {
            swept_repo_paths.insert(repo_path);
        }
    }
    for operation in operations {
        if !stale_operation_ids.contains(&operation.retention_id) {
            continue;
        }
        if swept_repo_paths.contains(&operation.repo_path)
            || !Path::new(&operation.repo_path).exists()
        {
            let _ = alera_core::git::hosted_review::clear_hosted_review_operation(
                &operation.retention_id,
            );
        }
    }
}

async fn for_records(
    store: &RuntimeStore,
    tabs: impl IntoIterator<Item = WorkspaceTabRecord>,
) -> Vec<HostedReviewRetention> {
    let mut retentions = Vec::new();
    for tab in tabs {
        let Some(retention_id) = tab
            .payload
            .get(RETENTION_ID_KEY)
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        let Ok(Some(workspace)) = store.find_workspace(&tab.workspace_id).await else {
            continue;
        };
        let relative_root = tab
            .payload
            .get(GIT_DIFF_ROOT_KEY)
            .and_then(|value| value.as_str());
        let Some(repo_path) = repo_path_for_root(&workspace.path, relative_root) else {
            continue;
        };
        let fallback_repo_path = match store.find_project(&workspace.project_id).await {
            Ok(Some(project)) => repo_path_for_root(&project.repo_path, relative_root)
                .filter(|fallback| fallback != &repo_path),
            _ => None,
        };
        retentions.push(HostedReviewRetention {
            repo_path,
            fallback_repo_path,
            retention_id: retention_id.to_string(),
        });
    }
    retentions
}

fn repo_path_for_root(root_path: &str, relative_root: Option<&str>) -> Option<String> {
    let Some(relative_root) = relative_root else {
        return Some(root_path.to_string());
    };
    if relative_root.trim().is_empty() {
        return Some(root_path.to_string());
    }
    let relative_root = Path::new(relative_root);
    if relative_root
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    Some(
        Path::new(root_path)
            .join(relative_root)
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(test)]
#[path = "hosted_review_retention_tests.rs"]
mod tests;
