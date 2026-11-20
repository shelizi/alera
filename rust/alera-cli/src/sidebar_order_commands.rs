//! Narrow, secret-free bridge from Native drag/drop to shared Workbench prefs.
//! The Runtime Host remains the sole owner of persisted sidebar order.
use alera_core::runtime::{SharedWorkbenchSortBy, SharedWorkbenchViewPrefs};
use serde_json::{json, Value};

use crate::cli::DesktopSettingsAction;
use crate::runtime_host_client::RuntimeHostRpcClient;
use crate::{print_error, print_value};

pub async fn run(
    client: &mut RuntimeHostRpcClient,
    action: &DesktopSettingsAction,
    json_output: bool,
) -> i32 {
    let record = match client
        .request_value("workbenchViewPrefs.get", &json!({}))
        .await
    {
        Ok(value) => value,
        Err(_) => return print_error("Shared sidebar order is unavailable; refresh the Host"),
    };
    let Some((mut prefs, revision)) = read_prefs(&record) else {
        return print_error("Host returned invalid shared sidebar order preferences");
    };
    match action {
        DesktopSettingsAction::SidebarOrderGet => {
            print_value(
                &projection(revision, &prefs),
                json_output,
                "Shared sidebar order",
            );
            0
        }
        DesktopSettingsAction::SidebarOrderMove {
            kind,
            source_id,
            target_id,
            visible_order,
        } => {
            let visible: Vec<String> = match serde_json::from_str(visible_order) {
                Ok(ids) => ids,
                Err(_) => return print_error("Sidebar order requires a JSON ID array"),
            };
            if visible.len() > 512
                || visible.iter().any(|id| id.is_empty() || id.len() > 128)
                || visible
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != visible.len()
                || !visible.contains(source_id)
                || !visible.contains(target_id)
            {
                return print_error("Invalid sidebar drag target or visible ID snapshot");
            }
            let (order, manual) = match kind.as_str() {
                "project" => (
                    &prefs.project_order,
                    prefs.project_sort == SharedWorkbenchSortBy::Manual,
                ),
                "workspace" => (
                    &prefs.workspace_order,
                    prefs.workspace_sort == SharedWorkbenchSortBy::Manual,
                ),
                _ => return print_error("Sidebar reorder kind must be project or workspace"),
            };
            let Some(next) = move_ids(order, &visible, source_id, target_id, manual) else {
                return print_error("Sidebar drag has no valid distinct source/target");
            };
            match kind.as_str() {
                "project" => {
                    prefs.project_order = next;
                    prefs.project_sort = SharedWorkbenchSortBy::Manual;
                }
                "workspace" => {
                    prefs.workspace_order = next;
                    prefs.workspace_sort = SharedWorkbenchSortBy::Manual;
                }
                _ => unreachable!(),
            }
            let payload = json!({"expectedRevision": revision, "prefs": prefs});
            let readback = match client
                .request_value("workbenchViewPrefs.update", &payload)
                .await
            {
                Ok(value) => value,
                Err(_) => {
                    return print_error("Host rejected sidebar reorder; refresh and try again")
                }
            };
            let Some((saved, saved_revision)) = read_prefs(&readback) else {
                return print_error("Sidebar order Host save readback was invalid");
            };
            if saved_revision <= revision
                || saved.project_sort != prefs.project_sort
                || saved.workspace_sort != prefs.workspace_sort
                || saved.project_order != prefs.project_order
                || saved.workspace_order != prefs.workspace_order
            {
                return print_error("Host did not confirm saved sidebar order");
            }
            print_value(
                &projection(saved_revision, &saved),
                json_output,
                "Shared sidebar order saved",
            );
            0
        }
        _ => unreachable!("Sidebar order CLI entry requires an order action"),
    }
}

fn read_prefs(value: &Value) -> Option<(SharedWorkbenchViewPrefs, i64)> {
    let revision = value.get("revision")?.as_i64()?;
    let prefs = serde_json::from_value(value.get("prefs")?.clone()).ok()?;
    Some((prefs, revision))
}

fn projection(revision: i64, prefs: &SharedWorkbenchViewPrefs) -> Value {
    json!({
        "kind": "sidebarOrder",
        "revision": revision,
        "projectSort": prefs.project_sort,
        "workspaceSort": prefs.workspace_sort,
        "projectOrder": prefs.project_order,
        "workspaceOrder": prefs.workspace_order,
    })
}

/// Moving down inserts after the hovered row, moving up before it.
/// Unknown/hidden IDs survive an edit; duplicates from stale snapshots do not.
fn move_ids(
    saved: &[String],
    visible: &[String],
    source: &str,
    target: &str,
    manual: bool,
) -> Option<Vec<String>> {
    if source == target {
        return None;
    }
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();
    let mut append = |ids: &[String]| {
        for id in ids {
            if seen.insert(id.clone()) {
                result.push(id.clone());
            }
        }
    };
    if manual {
        append(saved);
    }
    append(visible);
    if !manual {
        append(saved);
    }
    let from = result.iter().position(|id| id == source)?;
    let to = result.iter().position(|id| id == target)?;
    result.remove(from);
    let target_after = result.iter().position(|id| id == target)?;
    result.insert(
        if from < to {
            target_after + 1
        } else {
            target_after
        },
        source.to_owned(),
    );
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_down_and_up_preserves_hidden_saved_ids() {
        let saved: Vec<String> = ["a", "hidden", "b", "c"].map(String::from).into();
        let visible: Vec<String> = ["a", "b", "c"].map(String::from).into();
        assert_eq!(
            move_ids(&saved, &visible, "a", "c", true).unwrap(),
            ["hidden", "b", "c", "a"]
        );
        assert_eq!(
            move_ids(&saved, &visible, "c", "a", true).unwrap(),
            ["c", "a", "hidden", "b"]
        );
        assert_eq!(
            move_ids(&[], &visible, "a", "c", false).unwrap(),
            ["b", "c", "a"]
        );
    }

    #[test]
    fn projection_does_not_expose_other_sidebar_or_runtime_fields() {
        let prefs = SharedWorkbenchViewPrefs::default();
        let text = projection(4, &prefs).to_string();
        assert!(!text.contains("selectedProjectIds"));
        assert!(!text.contains("collapsedProjectIds"));
    }
}
