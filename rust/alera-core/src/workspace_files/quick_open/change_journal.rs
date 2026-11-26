//! Bounded, in-memory hints from an already-running recursive Explorer watch.
//! The journal is not a filesystem snapshot or a security boundary. A full
//! reconciliation is always required after a short-lived incremental update.
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

const MAX_DIRTY_PATHS: usize = 128;
const MAX_JOURNAL_ROOTS: usize = 32;

#[derive(Debug, Default)]
struct State {
    active: bool,
    revision: u64,
    invalidated: u64,
    paths: HashMap<PathBuf, u64>,
}

#[derive(Debug)]
pub struct QuickOpenWatchJournal {
    root: PathBuf,
    state: Mutex<State>,
}

static ROOTS: OnceLock<Mutex<HashMap<PathBuf, Weak<QuickOpenWatchJournal>>>> = OnceLock::new();

impl QuickOpenWatchJournal {
    pub fn new(root: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            root,
            state: Mutex::new(State::default()),
        })
    }

    /// Register only after a recursive watch has successfully attached.
    pub fn activate(self: &Arc<Self>) {
        if let Ok(mut state) = self.state.lock() {
            state.active = true;
        }
        if let Ok(mut roots) = ROOTS.get_or_init(|| Mutex::new(HashMap::new())).lock() {
            roots.retain(|_, entry| entry.strong_count() != 0);
            if roots.len() < MAX_JOURNAL_ROOTS || roots.contains_key(&self.root) {
                roots.insert(self.root.clone(), Arc::downgrade(self));
            }
        }
    }

    pub fn deactivate(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.active = false;
            state.paths.clear();
        }
    }

    pub fn invalidate(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.revision = state.revision.saturating_add(1);
            state.invalidated = state.revision;
            state.paths.clear();
        }
    }

    pub fn record(&self, paths: &[PathBuf]) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !state.active {
            return;
        }
        state.revision = state.revision.saturating_add(1);
        let revision = state.revision;
        for path in paths {
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                self.root.join(path)
            };
            let valid = absolute
                .strip_prefix(&self.root)
                .ok()
                .is_some_and(|relative| {
                    !relative.as_os_str().is_empty() && relative.components().all(|part| {
                        matches!(part, Component::Normal(name) if name.to_str().is_some_and(|name|
                        !matches!(name.to_ascii_lowercase().as_str(),
                            ".git" | ".hg" | ".svn" | ".gitignore" | ".ignore" |
                            ".gitconfig" | ".gitmodules" | ".gitattributes")))
                    })
                });
            if !valid {
                state.invalidated = revision;
                state.paths.clear();
                return;
            }
            state.paths.insert(absolute, revision);
            if state.paths.len() > MAX_DIRTY_PATHS {
                state.invalidated = revision;
                state.paths.clear();
                return;
            }
        }
        if paths.is_empty() {
            state.invalidated = revision;
            state.paths.clear();
        }
    }

    pub(super) fn cursor(&self) -> Option<u64> {
        let state = self.state.lock().ok()?;
        state.active.then_some(state.revision)
    }

    pub(super) fn since(&self, revision: u64) -> Option<(u64, Vec<PathBuf>)> {
        let state = self.state.lock().ok()?;
        if !state.active || state.invalidated > revision || state.revision < revision {
            return None;
        }
        Some((
            state.revision,
            state
                .paths
                .iter()
                .filter(|(_, changed)| **changed > revision)
                .map(|(path, _)| path.clone())
                .collect(),
        ))
    }
}

pub(super) fn for_root(root: &Path) -> Option<Arc<QuickOpenWatchJournal>> {
    let roots = ROOTS.get()?.lock().ok()?;
    let journal = roots.get(root)?.upgrade()?;
    journal.cursor()?;
    Some(journal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_preserves_independent_cursors_and_invalidates_gaps() {
        let root = tempfile::tempdir().unwrap();
        let journal = QuickOpenWatchJournal::new(root.path().to_path_buf());
        journal.activate();
        let zero = journal.cursor().unwrap();
        journal.record(&[root.path().join("src/a")]);
        let one = journal.cursor().unwrap();
        journal.record(&[root.path().join("src/b"), root.path().join("src/a")]);
        assert_eq!(journal.since(zero).unwrap().1.len(), 2);
        assert_eq!(journal.since(one).unwrap().1.len(), 2);
        journal.record(&[root.path().join("src/.gitignore")]);
        assert!(journal.since(one).is_none());
        let next = journal.cursor().unwrap();
        assert!(journal.since(next).unwrap().1.is_empty());
        for n in 0..=MAX_DIRTY_PATHS {
            journal.record(&[root.path().join(format!("src/{n}"))]);
        }
        assert!(journal.since(next).is_none());
        journal.deactivate();
        assert!(journal.cursor().is_none());
    }
}
