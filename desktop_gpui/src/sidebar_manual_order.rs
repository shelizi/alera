//! GPUI consumes the same Runtime Host workbench ordering used by Classic.
//! Pure parsing/projection only: Native never saves a duplicate local order.
use crate::project_tree::ProjectTreeGroup;
use crate::runtime_catalog::RuntimeCatalog;
use crate::terminal::{AgentActivityState, TerminalSession};
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
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
    pub workspace_activity_ms: HashMap<String, i64>,
}

pub const SIDEBAR_SORT_CHOICES: [(&str, &str); 4] = [
    ("name", "Name"),
    ("recent", "Recent"),
    ("activity", "Agent Activity"),
    ("manual", "Manual"),
];

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
            workspace_activity_ms: value["workspaceActivity"]
                .as_object()
                .into_iter()
                .flat_map(|entries| entries.iter().take(512))
                .filter_map(|(id, at)| {
                    if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
                        return None;
                    }
                    let at = chrono::DateTime::parse_from_rfc3339(at.as_str()?).ok()?;
                    Some((id.clone(), at.timestamp_millis()))
                })
                .collect(),
        })
    }
}

/// Classic urgency: needs input/interrupted, done, working, idle, no terminal.
pub type SidebarActivityRank = (u8, Reverse<i64>);
pub const NO_ACTIVITY: SidebarActivityRank = (4, Reverse(i64::MIN));
const STALE_AGENT_MS: i64 = 30 * 60 * 1000;

/// Build one read-only rank snapshot per sidebar projection. Missing or stale
/// Host data cannot make an agent look busy. No Git or IPC work happens here.
#[derive(Default)]
pub struct SidebarSortContext {
    recent: HashMap<String, i64>,
    activity: HashMap<String, SidebarActivityRank>,
    ids_by_path: HashMap<PathBuf, String>,
}

impl SidebarSortContext {
    pub fn new(
        catalog: Option<&RuntimeCatalog>,
        prefs: Option<&SidebarOrderPrefs>,
        sessions: &[TerminalSession],
        inventory_current: bool,
        now_ms: i64,
    ) -> Self {
        let Some(catalog) = catalog else {
            return Self::default();
        };
        let mut result = Self {
            recent: catalog.workspace_updated_ms.clone(),
            ids_by_path: catalog
                .workspaces
                .iter()
                .map(|ws| (ws.path.clone(), ws.id.clone()))
                .collect(),
            ..Self::default()
        };
        if let Some(prefs) = prefs {
            for (id, at) in &prefs.workspace_activity_ms {
                result
                    .recent
                    .entry(id.clone())
                    .and_modify(|current| *current = (*current).max(*at))
                    .or_insert(*at);
            }
        }
        if inventory_current {
            for session in sessions.iter().filter(|session| session.running) {
                let parse = |value: Option<&str>| {
                    value
                        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
                        .map(|at| at.timestamp_millis())
                };
                let updated = parse(session.agent_updated_at.as_deref());
                if let Some(at) = updated {
                    result
                        .recent
                        .entry(session.workspace_id.clone())
                        .and_modify(|current| *current = (*current).max(at))
                        .or_insert(at);
                }
            }
            for session in sessions.iter().filter(|session| session.running) {
                let parse = |value: Option<&str>| {
                    value
                        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
                        .map(|at| at.timestamp_millis())
                };
                let updated = parse(session.agent_updated_at.as_deref());
                let fresh = updated.is_some_and(|at| now_ms.saturating_sub(at) <= STALE_AGENT_MS);
                let class = if fresh && session.agent_type.is_some() {
                    match (session.interrupted, session.agent_state) {
                        (true, Some(_))
                        | (_, Some(AgentActivityState::Blocked | AgentActivityState::Waiting)) => 0,
                        (_, Some(AgentActivityState::Done)) => 1,
                        (_, Some(AgentActivityState::Working)) => 2,
                        _ => 3,
                    }
                } else {
                    3
                };
                let at = if class < 3 {
                    parse(session.agent_state_started_at.as_deref())
                        .or(updated)
                        .unwrap_or_else(|| result.recent_for_id(&session.workspace_id))
                } else {
                    result.recent_for_id(&session.workspace_id)
                };
                let rank = (class, Reverse(at));
                result
                    .activity
                    .entry(session.workspace_id.clone())
                    .and_modify(|old| *old = (*old).min(rank))
                    .or_insert(rank);
            }
        }
        result
    }

    fn recent_for_id(&self, id: &str) -> i64 {
        self.recent.get(id).copied().unwrap_or(i64::MIN)
    }

    pub fn recent_for_path(&self, path: &Path) -> i64 {
        self.ids_by_path
            .get(path)
            .map_or(i64::MIN, |id| self.recent_for_id(id))
    }

    pub fn activity_for_path(&self, path: &Path) -> SidebarActivityRank {
        self.ids_by_path
            .get(path)
            .and_then(|id| self.activity.get(id))
            .copied()
            .unwrap_or(NO_ACTIVITY)
    }

    /// Project recency and urgency aggregate only the visible workspace rows.
    pub fn project_key<'a>(
        &self,
        group: &ProjectTreeGroup,
        paths: impl Iterator<Item = &'a Path>,
        catalog: Option<&RuntimeCatalog>,
        prefs: Option<&SidebarOrderPrefs>,
    ) -> (usize, u8, Reverse<i64>, String, PathBuf) {
        let project = catalog.and_then(|cat| cat.projects.iter().find(|p| p.root == group.root));
        let name = project
            .map_or(group.label.as_str(), |p| p.name.as_str())
            .to_lowercase();
        let mode = prefs.map_or("name", |p| p.project_sort.as_str());
        let mut manual = 0;
        let mut class = 0;
        let mut time = i64::MIN;
        match mode {
            "manual" => {
                manual = project
                    .and_then(|p| prefs?.project_order.iter().position(|id| id == &p.id))
                    .unwrap_or(usize::MAX);
            }
            "recent" => {
                time = project
                    .and_then(|p| catalog?.project_updated_ms.get(&p.id).copied())
                    .unwrap_or(i64::MIN);
                for path in paths {
                    time = time.max(self.recent_for_path(path));
                }
            }
            "activity" => {
                let rank = paths
                    .map(|path| self.activity_for_path(path))
                    .min()
                    .unwrap_or(NO_ACTIVITY);
                class = rank.0;
                time = rank.1 .0;
            }
            _ => {}
        }
        (manual, class, Reverse(time), name, group.root.clone())
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
    groups.sort_by_cached_key(|group| (key_for(group), group.root.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_catalog::RuntimeProject;
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
    fn project_sort_uses_classic_names_recency_urgency_and_manual_ids() {
        use crate::runtime_catalog::RuntimeWorkspace;
        let mut catalog = RuntimeCatalog::default();
        for (id, root, name, workspace) in [("a", "A", "Zulu", "wa"), ("b", "B", "Alpha", "wb")] {
            catalog.projects.push(RuntimeProject {
                id: id.into(),
                name: name.into(),
                root: root.into(),
            });
            catalog.workspaces.push(RuntimeWorkspace {
                id: workspace.into(),
                project_id: id.into(),
                name: workspace.into(),
                path: PathBuf::from(root).join("work"),
                branch: None,
                is_pinned: false,
                tag_names: vec![],
                tag_ids: vec![],
                section_id: None,
                parent_workspace_id: None,
                is_archived: false,
            });
        }
        let mut prefs = SidebarOrderPrefs {
            project_sort: "name".into(),
            workspace_activity_ms: HashMap::from([("wa".into(), 400), ("wb".into(), 200)]),
            project_order: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-24T12:00:00Z")
            .unwrap()
            .timestamp_millis();
        let session = |id: &str, state| TerminalSession {
            handle: id.into(),
            workspace_id: id.into(),
            tab_id: id.into(),
            running: true,
            agent_type: Some("agy".into()),
            agent_state: Some(state),
            interrupted: false,
            agent_state_started_at: Some("2026-11-24T11:59:00Z".into()),
            agent_updated_at: Some("2026-11-24T11:59:00Z".into()),
            agent_context: None,
            builtin_mcp_status: None,
        };
        let sessions = [
            session("wa", AgentActivityState::Working),
            session("wb", AgentActivityState::Waiting),
        ];
        let context = SidebarSortContext::new(Some(&catalog), Some(&prefs), &[], true, now);
        let mut groups = vec![group("A"), group("B")];
        let sort_groups = |groups: &mut Vec<ProjectTreeGroup>,
                           prefs: &SidebarOrderPrefs,
                           context: &SidebarSortContext| {
            groups.sort_by_cached_key(|group| {
                let paths = [group.root.join("work")];
                context.project_key(
                    group,
                    paths.iter().map(|p| p.as_path()),
                    Some(&catalog),
                    Some(prefs),
                )
            });
        };
        sort_groups(&mut groups, &prefs, &context);
        assert_eq!(groups[0].label, "B");
        prefs.project_sort = "recent".into();
        sort_groups(&mut groups, &prefs, &context);
        assert_eq!(groups[0].label, "A");
        prefs.project_sort = "activity".into();
        let live = SidebarSortContext::new(Some(&catalog), Some(&prefs), &sessions, true, now);
        sort_groups(&mut groups, &prefs, &live);
        assert_eq!(groups[0].label, "B");
        let stale = SidebarSortContext::new(
            Some(&catalog),
            Some(&prefs),
            &sessions,
            true,
            now + STALE_AGENT_MS + 60_000,
        );
        assert_eq!(stale.activity_for_path(&PathBuf::from("B/work")).0, 3);
        let offline = SidebarSortContext::new(Some(&catalog), Some(&prefs), &sessions, false, now);
        assert_eq!(
            offline.activity_for_path(&PathBuf::from("B/work")),
            NO_ACTIVITY
        );
        prefs.project_sort = "manual".into();
        sort_groups(&mut groups, &prefs, &context);
        assert_eq!(groups[0].label, "A");
    }

    #[test]
    fn sort_dropdowns_follow_classic_options_and_are_translated() {
        use crate::native_i18n::{tr, UiLocale};
        let classic = include_str!("../../lib/src/features/workbench/presentation/widgets/workbench_view_options_controls.dart");
        let native = include_str!("native_sidebar_view_options.rs");
        assert!(native.contains("Sort Projects By"));
        assert!(native.contains("Then Workspaces By"));
        for (_, label) in SIDEBAR_SORT_CHOICES {
            assert!(classic.contains(label), "{label}");
            for locale in [
                UiLocale::TraditionalChinese,
                UiLocale::SimplifiedChinese,
                UiLocale::Japanese,
            ] {
                assert_ne!(tr(locale, label), label, "{label} / {}", locale.tag());
            }
        }
    }

    #[test]
    fn shared_activity_projection_is_optional_and_validates_timestamps() {
        let prefs = SidebarOrderPrefs::from_json(&serde_json::json!({
            "kind": "sidebarOrder", "revision": 2,
            "projectSort": "recent", "workspaceSort": "activity",
            "projectOrder": [], "workspaceOrder": [],
            "workspaceActivity": {"ok": "2026-11-24T12:00:00Z", "bad": "not-a-date"}
        }))
        .unwrap();
        assert_eq!(prefs.workspace_activity_ms.len(), 1);
        assert!(prefs.workspace_activity_ms.contains_key("ok"));
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
