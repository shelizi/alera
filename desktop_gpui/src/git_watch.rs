//! Bounded event-driven Git status invalidation. The notify callback never
//! reads Git or touches GPUI. A one-item channel coalesces bursts; callers
//! schedule a single asynchronous Git snapshot via GitRefreshGate.
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

const QUIET_PERIOD: Duration = Duration::from_millis(250);
const MAX_DELAY: Duration = Duration::from_millis(1400);

/// The status snapshot and Graph history have different invalidation costs.
/// File edits refresh Source Control; ref/HEAD metadata also refreshes Graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GitWatchInvalidation {
    pub status: bool,
    pub graph: bool,
}

fn affects_graph_history(path: &Path) -> bool {
    let first = path.components().next();
    match first {
        Some(Component::Normal(name)) => {
            matches!(
                name.to_string_lossy().as_ref(),
                "HEAD" | "packed-refs" | "refs" | "logs" | "shallow" | "FETCH_HEAD"
            )
        }
        _ => false,
    }
}

pub struct GitWatchService {
    _watcher: RecommendedWatcher,
    inbox: Receiver<()>,
    graph_inbox: Receiver<()>,
    pending_status: bool,
    pending_graph: bool,
    first_event: Option<Instant>,
    last_event: Option<Instant>,
    watched_root: PathBuf,
}

fn interesting_path(path: &Path) -> bool {
    // Git status ignores build output and dependency caches in the normal
    // workspace. Do not wake the renderer for those high-churn subtrees.
    !path.components().any(|c| match c {
        Component::Normal(name) => matches!(
            name.to_string_lossy().as_ref(),
            "node_modules" | "target" | "dist" | ".dart_tool" | ".worktrees"
        ),
        _ => false,
    })
}

impl GitWatchService {
    pub fn start(workspace: &Path) -> Result<Self, String> {
        let root = dunce::canonicalize(workspace).map_err(|e| e.to_string())?;
        let repo = git2::Repository::discover(&root).map_err(|e| e.to_string())?;
        let workdir = dunce::canonicalize(repo.workdir().ok_or("Bare Git repository")?)
            .map_err(|e| e.to_string())?;
        if workdir != root {
            return Err("Watchers require an exact worktree root".into());
        }
        let gitdir = dunce::canonicalize(repo.path()).map_err(|e| e.to_string())?;
        let common_dir = dunce::canonicalize(repo.commondir()).map_err(|e| e.to_string())?;
        let (tx, inbox) = sync_channel(1);
        let (graph_tx, graph_inbox) = sync_channel(1);
        let event_root = root.clone();
        let event_gitdir = gitdir.clone();
        let event_common = common_dir.clone();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    if matches!(
                        event.kind,
                        EventKind::Create(_)
                            | EventKind::Modify(_)
                            | EventKind::Remove(_)
                            | EventKind::Any
                    ) && (event.paths.is_empty()
                        || event.paths.iter().any(|path| {
                            let relative = path
                                .strip_prefix(&event_common)
                                .or_else(|_| path.strip_prefix(&event_gitdir))
                                .or_else(|_| path.strip_prefix(&event_root))
                                .unwrap_or(path);
                            interesting_path(relative)
                        }))
                    {
                        let _ = tx.try_send(());
                        if event.paths.is_empty()
                            || event.paths.iter().any(|path| {
                                path.strip_prefix(&event_gitdir)
                                    .ok()
                                    .filter(|relative| affects_graph_history(relative))
                                    .or_else(|| {
                                        path.strip_prefix(&event_common)
                                            .ok()
                                            .filter(|relative| affects_graph_history(relative))
                                    })
                                    .or_else(|| {
                                        path.strip_prefix(&event_root)
                                            .ok()
                                            .and_then(|relative| relative.strip_prefix(".git").ok())
                                            .filter(|relative| affects_graph_history(relative))
                                    })
                                    .is_some()
                            })
                        {
                            let _ = graph_tx.try_send(());
                        }
                    }
                }
            })
            .map_err(|e| format!("Git filesystem watcher unavailable: {e}"))?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| format!("Cannot watch worktree: {e}"))?;
        // In linked worktrees, refs live under the common .git directory,
        // not under .git/worktrees/<name>; watching only repo.path() misses
        // branch/tag changes performed by other worktrees.
        if !common_dir.starts_with(&root) {
            watcher
                .watch(&common_dir, RecursiveMode::Recursive)
                .map_err(|e| format!("Cannot watch common Git refs: {e}"))?;
        }
        if !gitdir.starts_with(&root) && !gitdir.starts_with(&common_dir) {
            watcher
                .watch(&gitdir, RecursiveMode::Recursive)
                .map_err(|e| format!("Cannot watch Git worktree metadata: {e}"))?;
        }
        Ok(Self {
            _watcher: watcher,
            inbox,
            graph_inbox,
            pending_status: false,
            pending_graph: false,
            first_event: None,
            last_event: None,
            watched_root: root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.watched_root
    }

    /// Polling is cheap and never touches Git. Debounce short bursts and cap
    /// starvation when events arrive continuously.
    pub fn poll(&mut self, now: Instant) -> GitWatchInvalidation {
        let mut changed = false;
        loop {
            match self.inbox.try_recv() {
                Ok(()) => changed = true,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if changed {
            self.pending_status = true;
        }
        let mut graph_changed = false;
        loop {
            match self.graph_inbox.try_recv() {
                Ok(()) => graph_changed = true,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if graph_changed {
            self.pending_graph = true;
        }
        if changed || graph_changed {
            self.first_event.get_or_insert(now);
            self.last_event = Some(now);
        }
        match (self.first_event, self.last_event) {
            (Some(first), Some(last))
                if now.duration_since(last) >= QUIET_PERIOD
                    || now.duration_since(first) >= MAX_DELAY =>
            {
                self.first_event = None;
                self.last_event = None;
                let result = GitWatchInvalidation {
                    status: self.pending_status,
                    graph: self.pending_graph,
                };
                self.pending_status = false;
                self.pending_graph = false;
                result
            }
            _ => GitWatchInvalidation::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skips_high_churn_build_paths() {
        assert!(!interesting_path(Path::new("repo/node_modules/pkg/a.js")));
        assert!(!interesting_path(Path::new("repo/target/debug/file")));
        assert!(interesting_path(Path::new("repo/src/main.rs")));
        assert!(interesting_path(Path::new("repo/.git/index")));
        // Absolute parent .worktrees names must not hide a linked checkout;
        // the callback strips the watched root before applying this filter.
        let linked = Path::new("C:/repo/.worktrees/topic/src/main.rs");
        let relative = linked.strip_prefix("C:/repo/.worktrees/topic").unwrap();
        assert!(interesting_path(relative));
    }

    #[test]
    fn recognizes_graph_ref_metadata_without_treating_index_as_history() {
        for path in [
            "HEAD",
            "refs/heads/main",
            "refs/tags/v1",
            "packed-refs",
            "logs/HEAD",
            "FETCH_HEAD",
        ] {
            assert!(affects_graph_history(Path::new(path)), "{path}");
        }
        for path in ["index", "objects/pack/a.pack", "config", "COMMIT_EDITMSG"] {
            assert!(!affects_graph_history(Path::new(path)), "{path}");
        }
        let common = Path::new("C:/project/.git");
        let linked = common.join("worktrees").join("branch_2");
        let head = linked.join("HEAD");
        let own_relative = head.strip_prefix(&linked).unwrap();
        assert!(affects_graph_history(own_relative));
        let shared = common.join("refs/heads/feature");
        assert!(affects_graph_history(shared.strip_prefix(common).unwrap()));
    }

    #[test]
    fn watches_external_branch_creation_as_graph_and_status_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("tracked.txt"), "initial\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
            .unwrap();
        drop(tree);
        let mut watcher = GitWatchService::start(&root).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("external-update", &head, false).unwrap();
        let mut result = GitWatchInvalidation::default();
        for _ in 0..35 {
            std::thread::sleep(Duration::from_millis(70));
            let next = watcher.poll(Instant::now());
            result.status |= next.status;
            result.graph |= next.graph;
            if result.graph {
                break;
            }
        }
        assert!(
            result.graph,
            "Git Graph did not observe external branch creation"
        );
        assert!(
            result.status,
            "Git status should refresh along with external ref changes"
        );
    }

    #[test]
    fn observes_worktree_file_changes_without_git_writes() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let mut watcher = GitWatchService::start(&root).unwrap();
        drop(repo);
        fs::write(root.join("watched.txt"), "hello").unwrap();
        // Windows events are asynchronous; bounded wait avoids a flaky
        // assumption that a notification arrives in the same thread turn.
        let mut observed = false;
        for _ in 0..35 {
            std::thread::sleep(Duration::from_millis(70));
            if watcher.poll(Instant::now()).status {
                observed = true;
                break;
            }
        }
        assert!(observed, "worktree watcher did not observe file creation");
    }
}
