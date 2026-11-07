//! Bounded, local state for the experimental GPUI *preview* frontend.
//! It stores file locations, never file contents, credentials or PTY state.
//! This does not replace the shipping Alera RuntimeStore.
use crate::model::WorkbenchModel;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const VERSION: u32 = 1;
const MAX_STATE_BYTES: u64 = 64 * 1024;
const MAX_RESTORED_TABS: usize = 24;
const MAX_RECENT_WORKSPACES: usize = 12;

#[derive(Debug, Clone)]
pub struct PreviewSessionStore {
    state_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedSession {
    version: u32,
    root: PathBuf,
    directory: PathBuf,
    tabs: Vec<PathBuf>,
    active: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LastWorkspace {
    version: u32,
    root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct RecentWorkspaces {
    version: u32,
    roots: Vec<PathBuf>,
}

impl PreviewSessionStore {
    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into(),
        }
    }

    /// Location of isolated GPUI local state for optional editor recovery.
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// Persist outside the project and outside Alera's existing RuntimeStore.
    pub fn platform_default() -> Option<Self> {
        // Explicit absolute override supports isolated smoke tests without
        // writing into the user's actual profile.
        if let Some(value) =
            std::env::var_os("ALERA_GPUI_STATE_DIR").filter(|value| !value.is_empty())
        {
            let path = PathBuf::from(value);
            return path.is_absolute().then(|| Self::new(path));
        }
        #[cfg(windows)]
        let base = std::env::var_os("APPDATA")
            .or_else(|| std::env::var_os("LOCALAPPDATA"))
            .map(PathBuf::from);
        #[cfg(target_os = "macos")]
        let base = std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
        });
        #[cfg(all(unix, not(target_os = "macos")))]
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local").join("state"))
            });
        #[cfg(not(any(windows, unix)))]
        let base: Option<PathBuf> = None;

        base.map(|base| Self::new(base.join("Alera").join("gpui-preview")))
    }

    /// Last used workspace is only a hint; its path must still exist and
    /// resolve to the same canonical directory. No remote or network IO.
    pub fn last_workspace(&self) -> io::Result<Option<PathBuf>> {
        let Some(saved) = self.read_json::<LastWorkspace>(&self.state_dir.join("last.json"))?
        else {
            return Ok(None);
        };
        if saved.version != VERSION {
            return Ok(None);
        }
        let root = match dunce::canonicalize(&saved.root) {
            Ok(path) if path.is_dir() && path == saved.root => path,
            _ => return Ok(None),
        };
        Ok(Some(root))
    }

    /// Return a bounded, newest-first list of existing canonical workspaces.
    /// Older preview installs only have last.json, which is still supported.
    /// The persisted list is never trusted to contain valid paths.
    pub fn recent_workspaces(&self) -> io::Result<Vec<PathBuf>> {
        let saved = self.read_json::<RecentWorkspaces>(&self.state_dir.join("recent.json"))?;
        let roots = match saved {
            Some(index) if index.version == VERSION => index.roots,
            Some(_) => return Ok(Vec::new()),
            None => return Ok(self.last_workspace()?.into_iter().collect()),
        };
        let mut recent = Vec::new();
        for path in roots {
            if recent.len() == MAX_RECENT_WORKSPACES {
                break;
            }
            let Ok(canonical) = dunce::canonicalize(&path) else {
                continue;
            };
            // The canonical-path equality check prevents redirects through
            // replaced symlinks and stale directory aliases.
            if canonical.is_dir() && canonical == path && !recent.contains(&canonical) {
                recent.push(canonical);
            }
        }
        Ok(recent)
    }

    /// Restore directory and tabs in their saved order. All file paths are
    /// canonicalized at restore time; missing, symlink-escaped and unsupported
    /// preview files are skipped without aborting the entire workspace.
    pub fn restore(&self, model: &mut WorkbenchModel) -> io::Result<usize> {
        let Some(saved) = self.read_json::<SavedSession>(&self.session_path(model.root()))? else {
            return Ok(0);
        };
        if saved.version != VERSION || saved.root != model.root() {
            return Ok(0);
        }

        if is_safe_relative(&saved.directory) {
            let _ = model.open_path(model.root().join(&saved.directory));
        }
        let mut count = 0;
        for relative in saved.tabs.into_iter().take(MAX_RESTORED_TABS) {
            if relative.as_os_str().is_empty() || !is_safe_relative(&relative) {
                continue;
            }
            let before = model.tabs().len();
            let _ = model.open_path(model.root().join(relative));
            if model.tabs().len() > before {
                count += 1;
            }
        }
        if let Some(active) = saved.active.filter(|p| is_safe_relative(p)) {
            let active = model.root().join(active);
            if let Ok(canonical) = dunce::canonicalize(active) {
                if let Some(index) = model.tabs().iter().position(|tab| tab.path == canonical) {
                    model.activate_tab(index);
                }
            }
        }
        model.set_status(format!("Restored {count} read-only preview tab(s)"));
        Ok(count)
    }

    pub fn save(&self, model: &WorkbenchModel) -> io::Result<()> {
        let root = model.root();
        // Load MRU before overwriting last.json: older versions without an
        // index rely on last.json to recover the departing workspace.
        let recent = self.recent_workspaces()?;
        let to_relative = |path: &Path| path.strip_prefix(root).ok().map(Path::to_path_buf);
        let session = SavedSession {
            version: VERSION,
            root: root.to_path_buf(),
            directory: to_relative(model.directory()).unwrap_or_default(),
            tabs: model
                .tabs()
                .iter()
                .filter_map(|tab| to_relative(&tab.path))
                .take(MAX_RESTORED_TABS)
                .collect(),
            active: model
                .active_document()
                .and_then(|tab| to_relative(&tab.path)),
        };
        self.write_json(&self.session_path(root), &session)?;
        if !recent.first().is_some_and(|path| path == root) {
            let roots = std::iter::once(root.to_path_buf())
                .chain(recent.into_iter().filter(|path| path != root))
                .take(MAX_RECENT_WORKSPACES)
                .collect();
            self.write_json(
                &self.state_dir.join("recent.json"),
                &RecentWorkspaces {
                    version: VERSION,
                    roots,
                },
            )?;
        }
        // Write the active hint last. If the index update fails, reopening
        // must still target the previously active workspace.
        self.write_json(
            &self.state_dir.join("last.json"),
            &LastWorkspace {
                version: VERSION,
                root: root.to_path_buf(),
            },
        )
    }

    /// Switch transaction: validate destination + restore its snapshot first,
    /// then save the departing workspace and update the active workspace hint.
    /// Caller replaces its WorkbenchModel only when the entire operation works.
    pub fn switch_workspace(
        &self,
        departing: &WorkbenchModel,
        destination: impl AsRef<Path>,
    ) -> io::Result<WorkbenchModel> {
        let mut next = WorkbenchModel::new(destination)?;
        self.restore(&mut next)?;
        self.save(departing)?;
        self.save(&next)?;
        Ok(next)
    }

    fn session_path(&self, root: &Path) -> PathBuf {
        // Deterministic FNV-1a is only a filename key, never a security check.
        // The saved record always validates its canonical root on load.
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in root.to_string_lossy().as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        self.state_dir.join(format!("workspace-{hash:016x}.json"))
    }

    fn read_json<T: for<'de> Deserialize<'de>>(&self, path: &Path) -> io::Result<Option<T>> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if file.metadata()?.len() > MAX_STATE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "State file exceeds 64 KiB",
            ));
        }
        let mut contents = Vec::new();
        file.take(MAX_STATE_BYTES + 1).read_to_end(&mut contents)?;
        if contents.len() as u64 > MAX_STATE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "State file exceeds 64 KiB",
            ));
        }
        serde_json::from_slice(&contents)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> io::Result<()> {
        let bytes = serde_json::to_vec(value)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if bytes.len() as u64 > MAX_STATE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "State file exceeds 64 KiB",
            ));
        }
        fs::create_dir_all(&self.state_dir)?;

        // Same-directory rename preserves the previous complete snapshot
        // if the process terminates while writing the replacement.
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp = path.with_extension(format!("tmp-{}-{suffix}", std::process::id()));
        let write_result = (|| -> io::Result<()> {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&tmp, path)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        write_result
    }
}

fn is_safe_relative(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_tabs_order_selected_file_and_directory_without_storing_text() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        fs::create_dir(workspace.path().join("src")).unwrap();
        fs::write(
            workspace.path().join("src").join("a.txt"),
            "original secret",
        )
        .unwrap();
        fs::write(workspace.path().join("src").join("b.txt"), "second").unwrap();
        let store = PreviewSessionStore::new(state.path());
        let mut model = WorkbenchModel::new(workspace.path()).unwrap();
        assert!(model.open_path(workspace.path().join("src")));
        assert!(model.open_path(workspace.path().join("src").join("a.txt")));
        assert!(model.open_path(workspace.path().join("src").join("b.txt")));
        model.activate_tab(0);
        store.save(&model).unwrap();
        let data = fs::read_to_string(store.session_path(model.root())).unwrap();
        assert!(
            !data.contains("original secret"),
            "session must never store file text"
        );
        assert_eq!(
            store.last_workspace().unwrap(),
            Some(model.root().to_path_buf())
        );
        // Always reload from the latest disk contents, never stale cached text.
        fs::write(
            workspace.path().join("src").join("a.txt"),
            "modified on disk",
        )
        .unwrap();
        let mut reopened = WorkbenchModel::new(workspace.path()).unwrap();
        assert_eq!(store.restore(&mut reopened).unwrap(), 2);
        assert_eq!(reopened.tabs().len(), 2);
        assert_eq!(reopened.active_tab(), Some(0));
        assert_eq!(reopened.content(), "modified on disk");
        assert!(reopened.directory().ends_with("src"));
    }

    #[test]
    fn multiple_workspaces_have_independent_snapshots() {
        let root_a = tempfile::tempdir().unwrap();
        let root_b = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        fs::write(root_a.path().join("a.txt"), "A").unwrap();
        fs::write(root_b.path().join("b.txt"), "B").unwrap();
        let store = PreviewSessionStore::new(state.path());
        let mut a = WorkbenchModel::new(root_a.path()).unwrap();
        let mut b = WorkbenchModel::new(root_b.path()).unwrap();
        a.open_entry(0);
        store.save(&a).unwrap();
        b.open_entry(0);
        store.save(&b).unwrap();
        assert_ne!(store.session_path(a.root()), store.session_path(b.root()));
        let mut restored = WorkbenchModel::new(root_a.path()).unwrap();
        assert_eq!(store.restore(&mut restored).unwrap(), 1);
        assert_eq!(restored.content(), "A");
        assert_eq!(
            store.last_workspace().unwrap(),
            Some(b.root().to_path_buf())
        );
    }

    #[test]
    fn rejects_invalid_paths_and_skips_deleted_files() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        fs::write(workspace.path().join("keep.txt"), "kept").unwrap();
        fs::write(outside.path().join("outside.txt"), "not allowed").unwrap();
        let mut model = WorkbenchModel::new(workspace.path()).unwrap();
        let store = PreviewSessionStore::new(state.path());
        let poisoned = SavedSession {
            version: VERSION,
            root: model.root().to_path_buf(),
            directory: PathBuf::from(".."),
            tabs: vec![
                PathBuf::from("../outside.txt"),
                outside.path().join("outside.txt"),
                PathBuf::from("missing.txt"),
                PathBuf::from("keep.txt"),
            ],
            active: Some(PathBuf::from("../outside.txt")),
        };
        store
            .write_json(&store.session_path(model.root()), &poisoned)
            .unwrap();
        assert_eq!(store.restore(&mut model).unwrap(), 1);
        assert_eq!(model.content(), "kept");
        assert_eq!(model.directory(), model.root());
    }

    #[test]
    fn corrupt_and_oversized_state_are_errors_without_workspace_mutation() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = PreviewSessionStore::new(state.path());
        let mut model = WorkbenchModel::new(workspace.path()).unwrap();
        fs::create_dir_all(state.path()).unwrap();
        fs::write(store.session_path(model.root()), "not json").unwrap();
        assert_eq!(
            store.restore(&mut model).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        fs::write(
            store.session_path(model.root()),
            vec![b'x'; MAX_STATE_BYTES as usize + 1],
        )
        .unwrap();
        assert_eq!(
            store.restore(&mut model).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(model.tabs().is_empty());
    }

    #[test]
    fn repeated_save_atomically_replaces_previous_state() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = PreviewSessionStore::new(state.path());
        fs::write(workspace.path().join("x.txt"), "x").unwrap();
        let mut model = WorkbenchModel::new(workspace.path()).unwrap();
        store.save(&model).unwrap();
        model.open_entry(0);
        store.save(&model).unwrap();
        let mut restored = WorkbenchModel::new(workspace.path()).unwrap();
        assert_eq!(store.restore(&mut restored).unwrap(), 1);
        assert_eq!(restored.content(), "x");
    }

    #[test]
    fn recent_workspace_history_deduplicates_and_caps_entries() {
        let workspaces = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = PreviewSessionStore::new(state.path());
        let mut roots = Vec::new();
        for i in 0..MAX_RECENT_WORKSPACES + 2 {
            let path = workspaces.path().join(format!("project-{i:02}"));
            fs::create_dir(&path).unwrap();
            let model = WorkbenchModel::new(&path).unwrap();
            store.save(&model).unwrap();
            roots.push(model.root().to_path_buf());
        }
        let recent = store.recent_workspaces().unwrap();
        assert_eq!(recent.len(), MAX_RECENT_WORKSPACES);
        assert_eq!(recent[0], *roots.last().unwrap());
        assert!(!recent.contains(&roots[0]));
        let moved = WorkbenchModel::new(&roots[4]).unwrap();
        store.save(&moved).unwrap();
        let recent = store.recent_workspaces().unwrap();
        assert_eq!(recent[0], roots[4]);
        assert_eq!(recent.len(), MAX_RECENT_WORKSPACES);
        assert_eq!(recent.iter().filter(|root| **root == roots[4]).count(), 1);
        fs::remove_dir(&roots[4]).unwrap();
        assert!(!store.recent_workspaces().unwrap().contains(&roots[4]));
    }

    #[test]
    fn recent_workspace_skips_tampered_and_alias_paths() {
        let root = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = PreviewSessionStore::new(state.path());
        let canonical = dunce::canonicalize(root.path()).unwrap();
        store
            .write_json(
                &state.path().join("recent.json"),
                &RecentWorkspaces {
                    version: VERSION,
                    roots: vec![
                        root.path().join(".."),
                        PathBuf::from("not-an-absolute-workspace"),
                        canonical.clone(),
                        canonical.clone(),
                    ],
                },
            )
            .unwrap();
        assert_eq!(store.recent_workspaces().unwrap(), vec![canonical]);
    }

    #[test]
    fn switching_workspaces_restores_tabs_and_keeps_previous_state_independent() {
        let state = tempfile::tempdir().unwrap();
        let root_a = tempfile::tempdir().unwrap();
        let root_b = tempfile::tempdir().unwrap();
        fs::write(root_a.path().join("a.txt"), "A").unwrap();
        fs::write(root_b.path().join("b.txt"), "B").unwrap();
        let store = PreviewSessionStore::new(state.path());
        let mut previous = WorkbenchModel::new(root_a.path()).unwrap();
        previous.open_entry(0);
        let mut b = WorkbenchModel::new(root_b.path()).unwrap();
        b.open_entry(0);
        store.save(&b).unwrap();
        store.save(&previous).unwrap();
        let switched = store.switch_workspace(&previous, root_b.path()).unwrap();
        assert_eq!(switched.root(), b.root());
        assert_eq!(switched.content(), "B");
        assert_eq!(previous.content(), "A", "old workspace is untouched");
        assert_eq!(
            store.last_workspace().unwrap(),
            Some(b.root().to_path_buf())
        );
        assert_eq!(
            store.recent_workspaces().unwrap()[..2],
            [b.root(), previous.root()]
        );
        let restored_a = store.switch_workspace(&switched, root_a.path()).unwrap();
        assert_eq!(restored_a.content(), "A");
    }

    #[test]
    fn failed_switch_does_not_overwrite_current_workspace_or_last_hint() {
        let state = tempfile::tempdir().unwrap();
        let root_a = tempfile::tempdir().unwrap();
        let root_b = tempfile::tempdir().unwrap();
        fs::write(root_a.path().join("note.txt"), "stay").unwrap();
        let store = PreviewSessionStore::new(state.path());
        let mut active = WorkbenchModel::new(root_a.path()).unwrap();
        active.open_entry(0);
        store.save(&active).unwrap();
        let other = WorkbenchModel::new(root_b.path()).unwrap();
        fs::write(store.session_path(other.root()), "corrupted session").unwrap();
        assert!(store.switch_workspace(&active, other.root()).is_err());
        assert_eq!(active.content(), "stay");
        assert_eq!(
            store.last_workspace().unwrap(),
            Some(active.root().to_path_buf())
        );
        assert!(store
            .switch_workspace(&active, state.path().join("missing"))
            .is_err());
    }
}
