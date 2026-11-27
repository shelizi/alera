//! A bounded, read-only view of the Classic RuntimeStore project/workspace
//! catalog. The existing CLI owns database access; the GPUI preview only
//! consumes its JSON and never writes another shadow catalog.
use crate::worktree_manager::{WorktreeManager, WorktreeReadSnapshot};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

const MAX_PROJECTS: usize = 64;
const MAX_WORKSPACES: usize = 512;
const MAX_TAGS: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeProject {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeWorkspace {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub is_pinned: bool,
    pub tag_names: Vec<String>,
    pub tag_ids: Vec<String>,
    pub section_id: Option<String>,
    pub parent_workspace_id: Option<String>,
    pub is_archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTag {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSection {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeCatalog {
    pub projects: Vec<RuntimeProject>,
    pub workspaces: Vec<RuntimeWorkspace>,
    pub tags: Vec<RuntimeTag>,
    pub sections: Vec<RuntimeSection>,
    /// Authoritative Host update timestamps, in Unix milliseconds.
    pub project_updated_ms: HashMap<String, i64>,
    pub workspace_updated_ms: HashMap<String, i64>,
}

/// Borrow the current RuntimeCatalog for one sidebar projection. Preserve
/// source order and *all* rows for a path: legacy display/section metadata
/// chooses the first match, relationship/manual-order IDs choose the last,
/// and search/filters match any registered row. No persistence or I/O.
#[derive(Default)]
pub struct SidebarCatalogIndex<'a> {
    projects: HashMap<&'a Path, Vec<&'a RuntimeProject>>,
    workspaces: HashMap<&'a Path, Vec<&'a RuntimeWorkspace>>,
    sections: HashMap<&'a str, Vec<&'a RuntimeSection>>,
}

impl<'a> SidebarCatalogIndex<'a> {
    pub fn new(catalog: Option<&'a RuntimeCatalog>) -> Self {
        let mut index = Self::default();
        if let Some(catalog) = catalog {
            for project in &catalog.projects {
                index
                    .projects
                    .entry(project.root.as_path())
                    .or_default()
                    .push(project);
            }
            for workspace in &catalog.workspaces {
                index
                    .workspaces
                    .entry(workspace.path.as_path())
                    .or_default()
                    .push(workspace);
            }
            for section in &catalog.sections {
                index
                    .sections
                    .entry(section.id.as_str())
                    .or_default()
                    .push(section);
            }
        }
        index
    }

    pub fn projects_at(&self, path: &Path) -> &[&'a RuntimeProject] {
        self.projects.get(path).map_or(&[], Vec::as_slice)
    }

    pub fn workspaces_at(&self, path: &Path) -> &[&'a RuntimeWorkspace] {
        self.workspaces.get(path).map_or(&[], Vec::as_slice)
    }

    pub fn sections_with_id(&self, id: &str) -> &[&'a RuntimeSection] {
        self.sections.get(id).map_or(&[], Vec::as_slice)
    }
}

fn item_list<'a>(value: &'a Value, kind: &str) -> io::Result<&'a [Value]> {
    if value.get("kind").and_then(Value::as_str) != Some(kind) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("Unexpected CLI {kind} response"),
        ));
    }
    value
        .get("items")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Runtime catalog items missing"))
}

fn display_text(value: Option<&Value>, max_len: usize) -> Option<String> {
    let text = value?.as_str()?.trim();
    if text.is_empty() || text.len() > max_len || text.chars().any(char::is_control) {
        return None;
    }
    Some(text.to_owned())
}

fn timestamp_ms(value: Option<&Value>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value?.as_str()?)
        .ok()
        .map(|at| at.timestamp_millis())
}

fn existing_directory(value: Option<&Value>) -> Option<PathBuf> {
    let path = Path::new(value?.as_str()?);
    if !path.is_absolute() {
        return None;
    }
    dunce::canonicalize(path).ok().filter(|path| path.is_dir())
}

/// Preserve stable CLI order, skip deleted/invalid paths and removed
/// workspaces, and cap all collections. Neither parsing nor canonicalization
/// can create Git worktrees, start a Host, or touch RuntimeStore.
pub fn parse_runtime_catalog(
    projects_json: &Value,
    workspaces_json: &Value,
) -> io::Result<RuntimeCatalog> {
    parse_runtime_catalog_with_snapshot(
        projects_json,
        workspaces_json,
        &mut WorktreeReadSnapshot::default(),
    )
}

pub(crate) fn parse_runtime_catalog_with_snapshot(
    projects_json: &Value,
    workspaces_json: &Value,
    snapshot: &mut WorktreeReadSnapshot,
) -> io::Result<RuntimeCatalog> {
    let project_items = item_list(projects_json, "projects")?;
    let workspace_items = item_list(workspaces_json, "workspaces")?;
    if project_items.len() > MAX_PROJECTS || workspace_items.len() > MAX_WORKSPACES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Runtime project/workspace catalog exceeds native display limits",
        ));
    }

    let mut catalog = RuntimeCatalog::default();
    let mut project_ids = HashSet::new();
    for value in project_items {
        let Some(id) = display_text(value.get("id"), 128) else {
            continue;
        };
        let Some(name) = display_text(value.get("name"), 256) else {
            continue;
        };
        let Some(root) = existing_directory(value.get("repoPath")) else {
            continue;
        };
        if project_ids.insert(id.clone()) {
            if let Some(at) = timestamp_ms(value.get("updatedAt")) {
                catalog.project_updated_ms.insert(id.clone(), at);
            }
            catalog.projects.push(RuntimeProject { id, name, root });
        }
    }

    let projects_by_id: HashMap<_, _> = catalog
        .projects
        .iter()
        .map(|project| (project.id.as_str(), project))
        .collect();
    let mut workspace_ids = HashSet::new();
    // Avoid one Git subprocess for every registered checkout of a project.
    let mut project_checkouts: HashMap<String, Option<HashSet<PathBuf>>> = HashMap::new();
    for value in workspace_items {
        if value.get("status").and_then(Value::as_str) == Some("removed") {
            continue;
        }
        let Some(id) = display_text(value.get("id"), 128) else {
            continue;
        };
        let Some(project_id) = display_text(value.get("projectId"), 128) else {
            continue;
        };
        let Some(project) = projects_by_id.get(project_id.as_str()) else {
            continue;
        };
        let Some(path) = existing_directory(value.get("path")) else {
            continue;
        };
        // A managed checkout can live outside the primary root; a registered
        // workspace is accepted only if it is that project's Git checkout.
        // Folder projects may have a plain primary directory instead.
        if path != project.root {
            let recognized = project_checkouts
                .entry(project.id.clone())
                .or_insert_with(|| {
                    WorktreeManager::discover_canonical(&project.root)
                        .ok()
                        .and_then(|manager| snapshot.list(&manager).ok())
                        .map(|entries| entries.iter().map(|entry| entry.path.clone()).collect())
                });
            if !recognized
                .as_ref()
                .is_some_and(|paths| paths.contains(&path))
            {
                continue;
            }
        }
        let Some(name) = display_text(value.get("name"), 256) else {
            continue;
        };
        if !workspace_ids.insert(id.clone()) {
            continue;
        }
        let tags = value
            .get("tagNames")
            .and_then(Value::as_array)
            .into_iter()
            .flat_map(|names| names.iter())
            .filter_map(|name| display_text(Some(name), 64))
            .take(MAX_TAGS)
            .collect();
        if let Some(at) = timestamp_ms(value.get("updatedAt")) {
            catalog.workspace_updated_ms.insert(id.clone(), at);
        }
        catalog.workspaces.push(RuntimeWorkspace {
            id,
            project_id,
            name,
            path,
            branch: display_text(value.get("branch"), 256),
            is_pinned: value
                .get("isPinned")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            tag_ids: value
                .get("tagIds")
                .and_then(Value::as_array)
                .into_iter()
                .flat_map(|ids| ids.iter())
                .filter_map(|id| display_text(Some(id), 128))
                .take(MAX_TAGS)
                .collect(),
            tag_names: tags,
            section_id: display_text(value.get("sectionId"), 128),
            parent_workspace_id: display_text(value.get("parentWorkspaceId"), 128),
            is_archived: value
                .get("archivedAt")
                .is_some_and(|value| !value.is_null()),
        });
    }
    Ok(catalog)
}

/// Incorporate the existing Classic tag catalog, but never persist a
/// duplicate GPUI registry.
pub fn attach_runtime_tags(catalog: &mut RuntimeCatalog, tags_json: &Value) -> io::Result<()> {
    let items = item_list(tags_json, "tags")?;
    if items.len() > 64 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Too many Runtime tags",
        ));
    }
    let mut seen = HashSet::new();
    for item in items {
        let (Some(id), Some(name)) = (
            display_text(item.get("id"), 128),
            display_text(item.get("name"), 64),
        ) else {
            continue;
        };
        if seen.insert(id.clone()) {
            catalog.tags.push(RuntimeTag { id, name });
        }
    }
    Ok(())
}

/// Section names are owned by Classic RuntimeStore, not the native UI.
pub fn attach_runtime_sections(
    catalog: &mut RuntimeCatalog,
    sections_json: &Value,
) -> io::Result<()> {
    let items = item_list(sections_json, "workspaceSections")?;
    if items.len() > 64 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Too many Runtime sections",
        ));
    }
    let mut seen = HashSet::new();
    for item in items {
        let (Some(id), Some(name)) = (
            display_text(item.get("id"), 128),
            display_text(item.get("name"), 128),
        ) else {
            continue;
        };
        if seen.insert(id.clone()) {
            catalog.sections.push(RuntimeSection { id, name });
        }
    }
    Ok(())
}

/// Parent-first traversal of a Classic workspace family for bulk pin actions.
/// Only workspaces in the same registered project are touched; invalid parent
/// links and cycles cannot escape the project or duplicate an operation.
pub fn workspace_family_ids(catalog: &RuntimeCatalog, root_id: &str) -> Vec<String> {
    let Some(root) = catalog.workspaces.iter().find(|ws| ws.id == root_id) else {
        return Vec::new();
    };
    let project_id = &root.project_id;
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = std::collections::VecDeque::from([root.id.clone()]);
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(workspace) = catalog
            .workspaces
            .iter()
            .find(|ws| ws.id == id && &ws.project_id == project_id)
        else {
            continue;
        };
        ids.push(workspace.id.clone());
        if ids.len() >= MAX_WORKSPACES {
            break;
        }
        for child in catalog.workspaces.iter().filter(|ws| {
            &ws.project_id == project_id && ws.parent_workspace_id.as_deref() == Some(&id)
        }) {
            if !seen.contains(&child.id) {
                queue.push_back(child.id.clone());
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn render_catalog_fixture() -> RuntimeCatalog {
        let mut catalog = RuntimeCatalog::default();
        for index in 0..12 {
            catalog.tags.push(RuntimeTag {
                id: format!("tag-{index}"),
                name: format!("Review tag {index}"),
            });
        }
        for index in 0..4 {
            catalog.sections.push(RuntimeSection {
                id: format!("section-{index}"),
                name: format!("Feature section {index}"),
            });
        }
        for project in 0..64 {
            let root = PathBuf::from(format!("C:/work/projects/repository-{project:02}"));
            let id = format!("project-{project}");
            catalog.projects.push(RuntimeProject {
                id: id.clone(),
                name: format!("Project {project}"),
                root: root.clone(),
            });
            catalog
                .project_updated_ms
                .insert(id.clone(), project as i64);
            for workspace in 0..8 {
                let ws_id = format!("workspace-{project}-{workspace}");
                catalog
                    .workspace_updated_ms
                    .insert(ws_id.clone(), (project * 8 + workspace) as i64);
                catalog.workspaces.push(RuntimeWorkspace {
                    id: ws_id,
                    project_id: id.clone(),
                    name: format!("Workspace {workspace}"),
                    path: root.join(format!("checkout-{workspace}")),
                    branch: Some(format!("feature/workspace-{workspace}")),
                    is_pinned: workspace % 3 == 0,
                    tag_names: catalog
                        .tags
                        .iter()
                        .take(3)
                        .map(|tag| tag.name.clone())
                        .collect(),
                    tag_ids: catalog
                        .tags
                        .iter()
                        .take(3)
                        .map(|tag| tag.id.clone())
                        .collect(),
                    section_id: Some(format!("section-{}", workspace % 4)),
                    parent_workspace_id: (workspace > 0).then(|| format!("workspace-{project}-0")),
                    is_archived: workspace % 7 == 6,
                });
            }
        }
        catalog
    }

    #[test]
    fn shared_catalog_generations_keep_menu_reads_consistent_and_refresh_fresh() {
        use std::sync::Arc;
        let mut current = Some(Arc::new(render_catalog_fixture()));
        let render = current.clone();
        let menu = render.clone();
        assert!(Arc::ptr_eq(
            current.as_ref().unwrap(),
            render.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(
            render.as_ref().unwrap(),
            menu.as_ref().unwrap()
        ));
        let path = render.as_ref().unwrap().workspaces[0].path.clone();
        let index = SidebarCatalogIndex::new(render.as_deref());
        let previous_name = index.workspaces_at(&path)[0].name.clone();

        // Publishing a complete new catalog leaves existing render/menu
        // references intact; no in-place mutation of old callback data.
        let mut replacement = render.as_ref().unwrap().as_ref().clone();
        replacement.workspaces[0].name = "Renamed".into();
        replacement.workspaces[0].is_archived = true;
        current = Some(Arc::new(replacement));
        let next = SidebarCatalogIndex::new(current.as_deref());
        assert_eq!(next.workspaces_at(&path)[0].name, "Renamed");
        assert!(next.workspaces_at(&path)[0].is_archived);
        assert_eq!(index.workspaces_at(&path)[0].name, previous_name);
        assert_eq!(menu.as_ref().unwrap().workspaces[0].name, previous_name);
        assert!(!menu.as_ref().unwrap().workspaces[0].is_archived);
        drop(next);
        current = None;
        assert!(SidebarCatalogIndex::new(current.as_deref())
            .workspaces_at(&path)
            .is_empty());
        assert_eq!(index.workspaces_at(&path)[0].name, previous_name);
    }

    #[test]
    #[ignore = "synthetic sidebar catalog snapshot performance benchmark"]
    fn benchmark_shared_sidebar_catalog_snapshot() {
        use std::hint::black_box;
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        let current = Arc::new(render_catalog_fixture());
        let roots: Vec<_> = current
            .projects
            .iter()
            .take(45)
            .map(|project| project.root.clone())
            .collect();
        let paths: Vec<_> = current
            .workspaces
            .iter()
            .take(45 * 8)
            .map(|workspace| workspace.path.clone())
            .collect();

        // Simulate the two former deep clones for render and menu callbacks,
        // then the two Arc clones from the immutable published generation.
        let clones = |optimized| {
            if optimized {
                let render = Arc::clone(&current);
                let menu = Arc::clone(&render);
                black_box((&render, &menu));
            } else {
                let render = current.as_ref().clone();
                let menu = Arc::new(render.clone());
                black_box((&render, &menu));
            }
        };
        let metadata = |optimized: bool| {
            // The preceding search/order stage already built this index in
            // Phase 27. Both modes pay its construction cost here; the new
            // render path reuses it instead of walking the catalog again.
            let index = SidebarCatalogIndex::new(Some(current.as_ref()));
            let old_render = (!optimized).then(|| current.as_ref().clone());
            let old_menu = old_render.as_ref().map(|render| Arc::new(render.clone()));
            let render = optimized.then(|| Arc::clone(&current));
            let menu = render.clone();
            black_box((&old_menu, &menu));
            let data = old_render.as_ref().unwrap_or(current.as_ref());
            let mut visible_projects = Vec::new();
            let mut visible_workspaces = Vec::new();
            let mut titles = Vec::new();
            let mut workspace_metadata = Vec::new();
            for root in &roots {
                let project = if optimized {
                    index.projects_at(root).first().copied()
                } else {
                    data.projects.iter().find(|project| project.root == *root)
                };
                if let Some(project) = project {
                    visible_projects.push(project.id.clone());
                }
            }
            for root in &roots {
                let project = if optimized {
                    index.projects_at(root).first().copied()
                } else {
                    data.projects.iter().find(|project| project.root == *root)
                };
                if let Some(project) = project {
                    titles.push((project.id.clone(), project.name.clone()));
                }
            }
            for path in &paths {
                let workspace = if optimized {
                    index.workspaces_at(path).first().copied()
                } else {
                    data.workspaces
                        .iter()
                        .find(|workspace| workspace.path == *path)
                };
                if let Some(workspace) = workspace {
                    visible_workspaces.push(workspace.id.clone());
                }
            }
            for path in &paths {
                let workspace = if optimized {
                    index.workspaces_at(path).first().copied()
                } else {
                    data.workspaces
                        .iter()
                        .find(|workspace| workspace.path == *path)
                };
                if let Some(workspace) = workspace {
                    workspace_metadata.push((
                        workspace.id.clone(),
                        workspace.name.clone(),
                        workspace.is_pinned,
                        workspace.is_archived,
                    ));
                }
            }
            (
                visible_projects,
                titles,
                visible_workspaces,
                workspace_metadata,
            )
        };
        assert_eq!(metadata(false), metadata(true));
        clones(false);
        clones(true);

        fn median(samples: &mut [Duration]) -> f64 {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1000.0
        }
        const FRAMES: usize = 100;
        for full_metadata in [false, true] {
            let mut old = Vec::new();
            let mut new = Vec::new();
            for sample in 0..7 {
                for optimized in if sample % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let start = Instant::now();
                    for _ in 0..FRAMES {
                        if full_metadata {
                            black_box(metadata(optimized));
                        } else {
                            clones(optimized);
                        }
                    }
                    let elapsed = start.elapsed();
                    if optimized {
                        new.push(elapsed);
                    } else {
                        old.push(elapsed);
                    }
                }
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("GPUI shared catalog full_metadata={full_metadata} projects={} workspaces={} visible_projects={} visible_workspaces={} frames={FRAMES} old_ms={old:.6} new_ms={new:.6} speedup={:.2}x", current.projects.len(), current.workspaces.len(), roots.len(), paths.len(), old/new);
        }
    }

    #[test]
    fn workspace_family_bulk_actions_stay_in_project_and_are_cycle_safe() {
        let mut catalog = RuntimeCatalog::default();
        let make = |id: &str, project_id: &str, parent: Option<&str>| RuntimeWorkspace {
            id: id.into(),
            project_id: project_id.into(),
            name: id.into(),
            path: PathBuf::from(format!("/tmp/{id}")),
            branch: None,
            is_pinned: false,
            tag_names: vec![],
            tag_ids: vec![],
            section_id: None,
            parent_workspace_id: parent.map(str::to_owned),
            is_archived: false,
        };
        catalog.workspaces.extend([
            make("root", "project-a", None),
            make("child", "project-a", Some("root")),
            make("leaf", "project-a", Some("child")),
            make("foreign", "project-b", Some("root")),
            make("orphan", "project-a", Some("missing")),
        ]);
        assert_eq!(
            workspace_family_ids(&catalog, "root"),
            ["root", "child", "leaf"]
        );
        assert!(workspace_family_ids(&catalog, "missing").is_empty());
        catalog.workspaces[0].parent_workspace_id = Some("leaf".into());
        assert_eq!(
            workspace_family_ids(&catalog, "root"),
            ["root", "child", "leaf"]
        );
    }

    #[test]
    fn section_list_preserves_unique_classic_sections() {
        let mut catalog = RuntimeCatalog::default();
        attach_runtime_sections(
            &mut catalog,
            &json!({
                "kind":"workspaceSections","items":[
                    {"id":"s1","name":"Frontend"},
                    {"id":"s1","name":"Duplicate"},
                    {"id":"s2","name":"Backend"}
                ]
            }),
        )
        .unwrap();
        assert_eq!(catalog.sections.len(), 2);
        assert_eq!(catalog.sections[0].name, "Frontend");
        assert!(attach_runtime_sections(&mut catalog, &json!([])).is_err());
    }

    #[test]
    fn tag_catalog_and_assigned_tag_ids_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let projects = json!({"kind":"projects","items":[{"id":"p","name":"P","repoPath":root}]});
        let workspaces = json!({"kind":"workspaces","items":[{
            "id":"w","projectId":"p","name":"Main","path":root,
            "tagIds":["tag-1"],"tagNames":["Review"]
        }]});
        let tags = json!({"kind":"tags","items":[{"id":"tag-1","name":"Review"}]});
        let mut catalog = parse_runtime_catalog(&projects, &workspaces).unwrap();
        attach_runtime_tags(&mut catalog, &tags).unwrap();
        assert_eq!(catalog.tags[0].name, "Review");
        assert_eq!(catalog.workspaces[0].tag_ids, ["tag-1"]);
    }

    #[test]
    fn reads_registered_project_and_workspace_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let projects = json!({"kind":"projects", "items":[
            {"id":"project-1","name":"Alera Desktop","repoPath":root},
            {"id":"project-1","name":"Duplicate","repoPath":root}
        ]});
        let workspaces = json!({"kind":"workspaces", "items":[
            {"id":"workspace-1","projectId":"project-1","name":"Main","path":root,
             "status":"active","isPinned":true,"tagNames":["daily","review"],
             "sectionId":"frontend","parentWorkspaceId":null,"archivedAt":null},
            {"id":"workspace-2","projectId":"project-1","name":"Archived","path":root,
             "status":"active","archivedAt":"2026-11-08T00:00:00Z"},
            {"id":"workspace-3","projectId":"project-1","name":"Removed","path":root,
             "status":"removed"}
        ]});
        let parsed = parse_runtime_catalog(&projects, &workspaces).unwrap();
        assert_eq!(parsed.projects.len(), 1);
        assert_eq!(parsed.projects[0].name, "Alera Desktop");
        assert_eq!(parsed.workspaces.len(), 2);
        assert!(parsed.workspaces[0].is_pinned);
        assert_eq!(parsed.workspaces[0].tag_names, ["daily", "review"]);
        assert_eq!(parsed.workspaces[0].section_id.as_deref(), Some("frontend"));
        assert!(parsed.workspaces[1].is_archived);
    }

    #[test]
    fn rejects_wrong_contract_and_skips_unregistered_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let projects = json!({"kind":"projects","items":[
            {"id":"p","name":"P","repoPath":root}
        ]});
        let workspaces = json!({"kind":"workspaces","items":[
            {"id":"w","projectId":"unknown","name":"No","path":root},
            {"id":"untrusted","projectId":"p","name":"No","path":"relative/path"}
        ]});
        assert!(parse_runtime_catalog(&json!([]), &workspaces).is_err());
        let parsed = parse_runtime_catalog(&projects, &workspaces).unwrap();
        assert!(parsed.workspaces.is_empty());
    }
}
