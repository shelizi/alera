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
                &projection_with_activity(client, revision, &prefs).await,
                json_output,
                "Shared sidebar order",
            );
            0
        }
        DesktopSettingsAction::SidebarSortSet { kind, mode } => {
            let changed = match set_sort_mode(&mut prefs, kind, mode) {
                Some(changed) => changed,
                None => return print_error("Invalid sidebar sort kind or mode"),
            };
            if !changed {
                print_value(
                    &projection_with_activity(client, revision, &prefs).await,
                    json_output,
                    "Shared sidebar order",
                );
                return 0;
            }
            let payload = json!({"expectedRevision": revision, "prefs": prefs});
            let readback = match client
                .request_value("workbenchViewPrefs.update", &payload)
                .await
            {
                Ok(value) => value,
                Err(_) => return print_error("Host rejected sidebar sort; refresh and try again"),
            };
            let Some((saved, saved_revision)) = read_prefs(&readback) else {
                return print_error("Sidebar sort Host save readback was invalid");
            };
            if saved_revision <= revision
                || saved.project_sort != prefs.project_sort
                || saved.workspace_sort != prefs.workspace_sort
                || saved.project_order != prefs.project_order
                || saved.workspace_order != prefs.workspace_order
            {
                return print_error("Host did not confirm saved sidebar sort");
            }
            print_value(
                &projection_with_activity(client, saved_revision, &saved).await,
                json_output,
                "Shared sidebar sort saved",
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
            if !manual {
                return print_error("Switch this sidebar section to Manual sort before reordering");
            }
            let Some(next) = move_ids(order, &visible, source_id, target_id, manual) else {
                return print_error("Sidebar drag has no valid distinct source/target");
            };
            match kind.as_str() {
                "project" => {
                    prefs.project_order = next;
                }
                "workspace" => {
                    prefs.workspace_order = next;
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
                &projection_with_activity(client, saved_revision, &saved).await,
                json_output,
                "Shared sidebar order saved",
            );
            0
        }
        _ => unreachable!("Sidebar order CLI entry requires an order action"),
    }
}

/// Patch one field of the fresh shared record; never reset filters or manual IDs.
fn set_sort_mode(prefs: &mut SharedWorkbenchViewPrefs, kind: &str, mode: &str) -> Option<bool> {
    let next = match mode {
        "name" => SharedWorkbenchSortBy::Name,
        "recent" => SharedWorkbenchSortBy::Recent,
        "activity" => SharedWorkbenchSortBy::Activity,
        "manual" => SharedWorkbenchSortBy::Manual,
        _ => return None,
    };
    let field = match kind {
        "project" => &mut prefs.project_sort,
        "workspace" => &mut prefs.workspace_sort,
        _ => return None,
    };
    let changed = *field != next;
    *field = next;
    Some(changed)
}

fn read_prefs(value: &Value) -> Option<(SharedWorkbenchViewPrefs, i64)> {
    let revision = value.get("revision")?.as_i64()?;
    let prefs = serde_json::from_value(value.get("prefs")?.clone()).ok()?;
    Some((prefs, revision))
}

/// Read Classic's persisted activity without exposing the rest of its preferences.
/// Older Hosts can still return sort settings; catalog timestamps are the fallback.
async fn projection_with_activity(
    client: &mut RuntimeHostRpcClient,
    revision: i64,
    prefs: &SharedWorkbenchViewPrefs,
) -> Value {
    let mut value = projection(revision, prefs);
    let needs_activity = [prefs.project_sort, prefs.workspace_sort]
        .iter()
        .any(|mode| {
            matches!(
                mode,
                SharedWorkbenchSortBy::Recent | SharedWorkbenchSortBy::Activity
            )
        });
    if needs_activity {
        if let Ok(activity) = client
            .request_value("workspaceActivity.list", &json!({}))
            .await
        {
            if let Some(entries) = activity.as_object() {
                // Keep within the Native bridge's 128 KiB response limit.
                let room = (120 * 1024usize).saturating_sub(value.to_string().len());
                value["workspaceActivity"] = Value::Object(bounded_activity(entries, room));
            }
        }
    }
    value
}

fn bounded_activity(
    entries: &serde_json::Map<String, Value>,
    room: usize,
) -> serde_json::Map<String, Value> {
    let mut candidates = entries
        .iter()
        .filter_map(|(id, at)| {
            if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
                return None;
            }
            let raw = at.as_str()?;
            if raw.len() > 64 {
                return None;
            }
            let timestamp = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
            Some((timestamp, id, at))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    let mut result = serde_json::Map::new();
    let mut used = 32usize; // field name, object delimiters, and final newline
    for (_, id, at) in candidates.into_iter().take(512) {
        let bytes = serde_json::to_string(id).unwrap_or_default().len() + at.to_string().len() + 2;
        if used.saturating_add(bytes) > room {
            break;
        }
        used += bytes;
        result.insert(id.clone(), at.clone());
    }
    result
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
    if !manual || source == target {
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
    append(saved);
    append(visible);
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
    fn sort_patch_preserves_other_mode_filters_and_manual_orders() {
        let mut prefs = SharedWorkbenchViewPrefs::default();
        prefs.project_order = vec!["p1".into()];
        prefs.workspace_order = vec!["w1".into()];
        prefs.selected_tag_ids = vec!["tag".into()];
        let mut expected = prefs.clone();
        expected.workspace_sort = SharedWorkbenchSortBy::Activity;
        assert_eq!(
            set_sort_mode(&mut prefs, "workspace", "activity"),
            Some(true)
        );
        assert_eq!(prefs, expected);
        assert_eq!(
            set_sort_mode(&mut prefs, "workspace", "activity"),
            Some(false)
        );
        assert_eq!(set_sort_mode(&mut prefs, "project", "bogus"), None);
        assert_eq!(set_sort_mode(&mut prefs, "section", "name"), None);
        assert_eq!(prefs, expected);
    }

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
        assert_eq!(move_ids(&[], &visible, "a", "c", false), None,);
    }

    #[test]
    fn activity_projection_prioritizes_newest_and_respects_budget() {
        let entries = json!({
            "old": "2026-11-23T00:00:00Z",
            "new": "2026-11-24T00:00:00Z",
            "invalid": "not a timestamp",
        });
        let bounded = bounded_activity(entries.as_object().unwrap(), 68);
        assert_eq!(bounded.len(), 1);
        assert!(bounded.contains_key("new"));
        assert!(bounded_activity(entries.as_object().unwrap(), 0).is_empty());
    }

    #[test]
    fn projection_does_not_expose_other_sidebar_or_runtime_fields() {
        let prefs = SharedWorkbenchViewPrefs::default();
        let text = projection(4, &prefs).to_string();
        assert!(!text.contains("selectedProjectIds"));
        assert!(!text.contains("collapsedProjectIds"));
    }
}
