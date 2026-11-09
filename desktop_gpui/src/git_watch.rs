//! Bounded event-driven Git status invalidation. The notify callback never
//! reads Git or touches GPUI. A one-item channel coalesces bursts; callers
//! schedule a single asynchronous Git snapshot via GitRefreshGate.
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

const QUIET_PERIOD: Duration = Duration::from_millis(250);
const MAX_DELAY: Duration = Duration::from_millis(1400);

pub struct GitWatchService {
    _watcher: RecommendedWatcher,
    inbox: Receiver<()>,
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
        let (tx, inbox) = sync_channel(1);
        let event_root = root.clone();
        let event_gitdir = gitdir.clone();
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
                                .strip_prefix(&event_root)
                                .or_else(|_| path.strip_prefix(&event_gitdir))
                                .unwrap_or(path);
                            interesting_path(relative)
                        }))
                    {
                        let _ = tx.try_send(());
                    }
                }
            })
            .map_err(|e| format!("Git filesystem watcher unavailable: {e}"))?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| format!("Cannot watch worktree: {e}"))?;
        if !gitdir.starts_with(&root) {
            watcher
                .watch(&gitdir, RecursiveMode::Recursive)
                .map_err(|e| format!("Cannot watch Git metadata: {e}"))?;
        }
        Ok(Self {
            _watcher: watcher,
            inbox,
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
    pub fn poll(&mut self, now: Instant) -> bool {
        let mut got_event = false;
        loop {
            match self.inbox.try_recv() {
                Ok(()) => got_event = true,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if got_event {
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
                true
            }
            _ => false,
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
            if watcher.poll(Instant::now()) {
                observed = true;
                break;
            }
        }
        assert!(observed, "worktree watcher did not observe file creation");
    }
}
