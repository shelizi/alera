//! GPUI consumes the same Runtime Host workbench ordering used by Classic.
//! Pure parsing/projection only: Native never saves a duplicate local order.
use crate::project_tree::ProjectTreeGroup;
use crate::runtime_catalog::RuntimeCatalog;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{self, ErrorKind};
use std::time::{Duration, Instant};

/// A drag can only start after a stationary 500ms press on the visible grip.
/// A quick drag attempt is rejected for the entire gesture, even if the user
/// continues holding while moving. This prevents accidental reorders.
pub const SIDEBAR_REORDER_HOLD: Duration = Duration::from_millis(500);

#[derive(Debug, Default)]
pub struct SidebarReorderHoldGate {
    source_key: Option<String>,
    pressed_at: Option<Instant>,
    drag_accepted: bool,
    drag_rejected: bool,
}

impl SidebarReorderHoldGate {
    pub fn press(&mut self, source_key: &str, now: Instant) {
        self.source_key = Some(source_key.to_owned());
        self.pressed_at = Some(now);
        self.drag_accepted = false;
        self.drag_rejected = false;
    }

    pub fn drag_started(&mut self, source_key: &str, now: Instant) -> bool {
        if self.source_key.as_deref() != Some(source_key) || self.drag_rejected {
            return false;
        }
        if self.drag_accepted {
            return true;
        }
        let held = self.pressed_at.is_some_and(|at| {
            now.checked_duration_since(at)
                .is_some_and(|elapsed| elapsed >= SIDEBAR_REORDER_HOLD)
        });
        if !held {
            self.drag_rejected = true;
            return false;
        }
        self.drag_accepted = true;
        true
    }

    /// Consume a successful drag once. A stale/fast/other-row drop cannot
    /// update the shared Host order.
    pub fn consume_drop(&mut self, source_key: &str) -> bool {
        if self.source_key.as_deref() != Some(source_key) || !self.drag_accepted {
            return false;
        }
        self.drag_accepted = false;
        self.source_key = None;
        self.pressed_at = None;
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarOrderPrefs {
    pub revision: i64,
    pub project_sort: String,
    pub workspace_sort: String,
    pub project_order: Vec<String>,
    pub workspace_order: Vec<String>,
}

impl SidebarOrderPrefs {
    /// Show the Project grip only when Project sort is explicitly Manual.
    pub fn project_is_manual(&self) -> bool {
        self.project_sort == "manual"
    }

    /// Show the Worktree grip only when Worktree sort is explicitly Manual.
    pub fn workspace_is_manual(&self) -> bool {
        self.workspace_sort == "manual"
    }

    pub fn from_json(value: &Value) -> io::Result<Self> {
        if value["kind"] != "sidebarOrder" {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Unexpected sidebar order kind",
            ));
        }
        let revision = value["revision"]
            .as_i64()
            .filter(|value| *value >= 0)
            .ok_or_else(|| {
                io::Error::new(ErrorKind::InvalidData, "Missing sidebar order revision")
            })?;
        let sort_mode = |key: &str| -> io::Result<String> {
            let mode = value[key].as_str().ok_or_else(|| {
                io::Error::new(ErrorKind::InvalidData, "Missing sidebar sort mode")
            })?;
            if !matches!(mode, "name" | "recent" | "activity" | "manual") {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Unexpected sidebar sort mode",
                ));
            }
            Ok(mode.to_owned())
        };
        let ids = |key: &str| -> io::Result<Vec<String>> {
            let values = value[key].as_array().ok_or_else(|| {
                io::Error::new(ErrorKind::InvalidData, "Missing sidebar order IDs")
            })?;
            if values.len() > 512 {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Sidebar order exceeds limit",
                ));
            }
            let mut seen = HashSet::new();
            let mut ids = Vec::with_capacity(values.len());
            for value in values {
                let id = value
                    .as_str()
                    .filter(|id| {
                        !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
                    })
                    .ok_or_else(|| {
                        io::Error::new(ErrorKind::InvalidData, "Invalid sidebar order ID")
                    })?;
                if !seen.insert(id) {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        "Repeated sidebar order ID",
                    ));
                }
                ids.push(id.to_owned());
            }
            Ok(ids)
        };
        Ok(Self {
            revision,
            project_sort: sort_mode("projectSort")?,
            workspace_sort: sort_mode("workspaceSort")?,
            project_order: ids("projectOrder")?,
            workspace_order: ids("workspaceOrder")?,
        })
    }
}

pub fn order_project_groups(
    groups: &mut [ProjectTreeGroup],
    catalog: Option<&RuntimeCatalog>,
    prefs: Option<&SidebarOrderPrefs>,
) {
    let Some(prefs) = prefs.filter(|prefs| prefs.project_sort == "manual") else {
        return;
    };
    let positions: HashMap<&str, usize> = prefs
        .project_order
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let key_for = |group: &ProjectTreeGroup| {
        let project = catalog.and_then(|catalog| {
            catalog
                .projects
                .iter()
                .find(|project| project.root == group.root)
        });
        let rank = project
            .and_then(|project| positions.get(project.id.as_str()).copied())
            .unwrap_or(usize::MAX);
        let name = project.map_or(group.label.as_str(), |project| project.name.as_str());
        (rank, name.to_lowercase())
    };
    groups.sort_by(|left, right| {
        key_for(left)
            .cmp(&key_for(right))
            .then_with(|| left.root.cmp(&right.root))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_catalog::RuntimeProject;
    use std::path::PathBuf;

    #[test]
    fn long_press_requires_full_delay_before_first_drag_move() {
        let now = Instant::now();
        let mut gate = SidebarReorderHoldGate::default();
        gate.press("project:a", now);
        assert!(!gate.drag_started("project:a", now + Duration::from_millis(490)));
        assert!(!gate.drag_started("project:a", now + Duration::from_secs(1)));
        assert!(!gate.consume_drop("project:a"));
        gate.press("project:a", now + Duration::from_secs(2));
        assert!(gate.drag_started("project:a", now + Duration::from_millis(2500)));
        assert!(gate.consume_drop("project:a"));
        assert!(
            !gate.consume_drop("project:a"),
            "one drag cannot save twice"
        );
    }

    #[test]
    fn unrelated_row_cannot_consume_long_press_or_drop() {
        let now = Instant::now();
        let mut gate = SidebarReorderHoldGate::default();
        gate.press("project:a", now);
        assert!(!gate.drag_started("workspace:a", now + Duration::from_secs(1)));
        assert!(gate.drag_started("project:a", now + Duration::from_secs(1)));
        assert!(!gate.consume_drop("workspace:a"));
        assert!(gate.consume_drop("project:a"));
    }

    fn group(root: &str) -> ProjectTreeGroup {
        ProjectTreeGroup {
            root: PathBuf::from(root),
            label: root.into(),
            is_saved: true,
            is_active: false,
            worktrees: vec![],
            error: None,
        }
    }

    #[test]
    fn shared_manual_project_order_does_not_drop_unregistered_rows() {
        let catalog = RuntimeCatalog {
            projects: vec![
                RuntimeProject {
                    id: "a".into(),
                    name: "A".into(),
                    root: PathBuf::from("A"),
                },
                RuntimeProject {
                    id: "b".into(),
                    name: "B".into(),
                    root: PathBuf::from("B"),
                },
            ],
            ..RuntimeCatalog::default()
        };
        let prefs = SidebarOrderPrefs {
            project_sort: "manual".into(),
            project_order: vec!["b".into(), "a".into()],
            ..SidebarOrderPrefs::default()
        };
        let mut groups = vec![group("A"), group("unknown"), group("B")];
        order_project_groups(&mut groups, Some(&catalog), Some(&prefs));
        assert_eq!(
            groups.into_iter().map(|g| g.label).collect::<Vec<_>>(),
            vec!["B", "A", "unknown"]
        );
    }

    #[test]
    fn reordering_grips_require_matching_manual_sort_independently() {
        let mut prefs = SidebarOrderPrefs::default();
        assert!(!prefs.project_is_manual());
        assert!(!prefs.workspace_is_manual());

        prefs.project_sort = "manual".into();
        assert!(prefs.project_is_manual());
        assert!(!prefs.workspace_is_manual());

        prefs.project_sort = "activity".into();
        prefs.workspace_sort = "manual".into();
        assert!(!prefs.project_is_manual());
        assert!(prefs.workspace_is_manual());

        prefs.project_sort = "manual".into();
        assert!(prefs.project_is_manual());
        assert!(prefs.workspace_is_manual());
    }

    #[test]
    fn readback_rejects_unexpected_payload_and_duplicate_ids() {
        assert!(SidebarOrderPrefs::from_json(&serde_json::json!({
            "kind": "sidebarOrder", "revision": 0,
            "projectSort": "manual", "workspaceSort": "name",
            "projectOrder": ["a","a"], "workspaceOrder": [],
        }))
        .is_err());
    }
}
