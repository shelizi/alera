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
const MAX_SAVED_PROJECTS: usize = 32;
const MAX_TERMINAL_LAYOUT_TABS: usize = 8;
const MAX_GIT_FAVORITES: usize = 80;

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

/// Explicit project registry. Separate from MRU so pin/unpin never removes
/// preview tabs, drafts, worktrees or project files on disk.
#[derive(Debug, Serialize, Deserialize)]
struct SavedProjects {
    version: u32,
    roots: Vec<PathBuf>,
}

/// UI-only fold state. This is deliberately separate from the RuntimeStore
/// project and workspace catalog (which remains authoritative).
#[derive(Debug, Serialize, Deserialize)]
struct SavedProjectTreeFolds {
    version: u32,
    roots: Vec<PathBuf>,
}

/// Only public Host session identities, never PTY text or credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalLayoutTab {
    pub handle: String,
    pub workspace_id: String,
    pub tab_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalLayout {
    pub tabs: Vec<TerminalLayoutTab>,
    pub active_handle: Option<String>,
    pub transcript_view: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedTerminalLayout {
    version: u32,
    root: PathBuf,
    layout: TerminalLayout,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedGitFavorites {
    version: u32,
    root: PathBuf,
    branches: Vec<String>,
}

fn validate_git_favorites(branches: &[String]) -> io::Result<()> {
    if branches.len() > MAX_GIT_FAVORITES
        || branches.iter().any(|name| {
            name.is_empty()
                || name.len() > 128
                || name.starts_with('-')
                || name.chars().any(char::is_control)
        })
        || branches
            .iter()
            .enumerate()
            .any(|(index, name)| branches[..index].contains(name))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid or duplicate Native favorite Git branch",
        ));
    }
    Ok(())
}

fn valid_terminal_layout_id(id: &str, max_bytes: usize) -> bool {
    !id.is_empty()
        && id.len() <= max_bytes
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn validate_terminal_layout(layout: &TerminalLayout) -> io::Result<()> {
    if layout.tabs.len() > MAX_TERMINAL_LAYOUT_TABS
        || layout.tabs.iter().any(|tab| {
            !valid_terminal_layout_id(&tab.handle, 128)
                || !valid_terminal_layout_id(&tab.workspace_id, 256)
                || !valid_terminal_layout_id(&tab.tab_id, 256)
        })
        || layout.tabs.iter().enumerate().any(|(index, tab)| {
            layout.tabs[..index].iter().any(|prior| {
                prior.handle == tab.handle
                    || (prior.workspace_id == tab.workspace_id && prior.tab_id == tab.tab_id)
            })
        })
        || layout
            .active_handle
            .as_ref()
            .is_some_and(|handle| !layout.tabs.iter().any(|tab| tab.handle == *handle))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Terminal layout has invalid or duplicate Host session references",
        ));
    }
    Ok(())
}

impl PreviewSessionStore {
    /// A missing legacy file is *not* an explicit empty favorite selection:
    /// don't let a new Native startup seed [] before Classic's first import.
    pub fn has_legacy_git_favorites(&self, root: &Path) -> io::Result<bool> {
        let canonical = dunce::canonicalize(root)?;
        if canonical != root || !canonical.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Git favorites root must be an existing canonical workspace",
            ));
        }
        Ok(self.git_favorites_path(root).exists())
    }

    /// Test-only writer for exercising pre-Rust Native JSON migration fixtures.
    /// Production GPUI cannot mutate this former second source of truth.
    #[cfg(test)]
    pub fn save_git_favorites(&self, root: &Path, branches: &[String]) -> io::Result<()> {
        let canonical = dunce::canonicalize(root)?;
        if canonical != root || !canonical.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Git favorite workspace root must be canonical",
            ));
        }
        validate_git_favorites(branches)?;
        // A malformed or future-version file may contain user data unknown
        // to this preview. Never silently overwrite it during a star click.
        if self.git_favorites_path(root).exists() {
            self.load_git_favorites(root)?;
        }
        self.write_json(
            &self.git_favorites_path(root),
            &SavedGitFavorites {
                version: VERSION,
                root: root.to_path_buf(),
                branches: branches.to_vec(),
            },
        )
    }

    /// Read-only old GPUI source, only used to seed absent Host favorites
    /// during migration. Even cleared [] remains a meaningful legacy value.
    pub fn load_git_favorites(&self, root: &Path) -> io::Result<Vec<String>> {
        let canonical = dunce::canonicalize(root)?;
        if canonical != root || !canonical.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Git favorite workspace root must be canonical",
            ));
        }
        let Some(saved) = self.read_json::<SavedGitFavorites>(&self.git_favorites_path(root))?
        else {
            return Ok(Vec::new());
        };
        if saved.version != VERSION || saved.root != root {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Git favorites do not belong to this Native workspace/version",
            ));
        }
        validate_git_favorites(&saved.branches)?;
        Ok(saved.branches)
    }

    fn git_favorites_path(&self, root: &Path) -> PathBuf {
        let file = self.session_path(root);
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        self.state_dir
            .join(name.replacen("workspace-", "git-favorites-", 1))
    }

    /// A separate, bounded view-only layout scoped to one canonical root.
    /// Never writes Alera's RuntimeStore or creates/attaches a Host session.
    pub fn save_terminal_layout(&self, root: &Path, layout: &TerminalLayout) -> io::Result<()> {
        let canonical = dunce::canonicalize(root)?;
        if canonical != root || !canonical.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Terminal layout root is not a canonical existing directory",
            ));
        }
        validate_terminal_layout(layout)?;
        self.write_json(
            &self.terminal_layout_path(root),
            &SavedTerminalLayout {
                version: VERSION,
                root: root.to_path_buf(),
                layout: layout.clone(),
            },
        )
    }

    pub fn load_terminal_layout(&self, root: &Path) -> io::Result<Option<TerminalLayout>> {
        let canonical = dunce::canonicalize(root)?;
        if canonical != root || !canonical.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Terminal layout root is not a canonical existing directory",
            ));
        }
        let Some(saved) =
            self.read_json::<SavedTerminalLayout>(&self.terminal_layout_path(root))?
        else {
            return Ok(None);
        };
        if saved.version != VERSION {
            return Ok(None);
        }
        if saved.root != root {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Terminal layout belongs to a different workspace",
            ));
        }
        validate_terminal_layout(&saved.layout)?;
        Ok(Some(saved.layout))
    }

    fn terminal_layout_path(&self, root: &Path) -> PathBuf {
        let file = self.session_path(root);
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        self.state_dir
            .join(name.replacen("workspace-", "terminal-layout-", 1))
    }

    /// Scoped to the same canonical workspace key as preview sessions but
    /// stored separately, so a mixed pane layout cannot overwrite drafts,
    /// terminal Host identities or the shipping RuntimeStore.
    pub fn workbench_layout_path(&self, root: &Path) -> io::Result<PathBuf> {
        let canonical = dunce::canonicalize(root)?;
        if canonical != root || !canonical.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Workbench layout requires a canonical existing workspace",
            ));
        }
        let file = self.session_path(root);
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        Ok(self
            .state_dir
            .join(name.replacen("workspace-", "workbench-layout-", 1)))
    }

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

        base.map(|base| {
            Self::new(crate::edition::native_state_directory_preserving_preview(
                &base,
            ))
        })
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

    /// Clear an inactive Classic project from native recent hints without
    /// deleting project files, Git worktrees, or local preview sessions.
    pub fn forget_recent_workspace(&self, path: &Path) -> io::Result<()> {
        let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let roots = self
            .recent_workspaces()?
            .into_iter()
            .filter(|root| root != &canonical)
            .collect();
        self.write_json(
            &self.state_dir.join("recent.json"),
            &RecentWorkspaces {
                version: VERSION,
                roots,
            },
        )
    }

    pub fn collapsed_project_roots(&self) -> io::Result<Vec<PathBuf>> {
        let saved =
            self.read_json::<SavedProjectTreeFolds>(&self.state_dir.join("project-folds.json"))?;
        let Some(saved) = saved else {
            return Ok(Vec::new());
        };
        if saved.version != VERSION {
            return Ok(Vec::new());
        }
        let mut roots = Vec::new();
        for path in saved.roots.into_iter().take(64) {
            if let Ok(canonical) = dunce::canonicalize(&path) {
                if canonical.is_dir() && canonical == path && !roots.contains(&canonical) {
                    roots.push(canonical);
                }
            }
        }
        Ok(roots)
    }

    pub fn save_collapsed_project_roots(&self, roots: &[PathBuf]) -> io::Result<()> {
        if roots.len() > 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Too many folded projects",
            ));
        }
        let mut valid = Vec::new();
        for path in roots {
            let canonical = dunce::canonicalize(path)?;
            if canonical != *path || !canonical.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Folded project root is invalid",
                ));
            }
            if !valid.contains(&canonical) {
                valid.push(canonical);
            }
        }
        self.write_json(
            &self.state_dir.join("project-folds.json"),
            &SavedProjectTreeFolds {
                version: VERSION,
                roots: valid,
            },
        )
    }

    /// Pinned project roots, independent from most-recently-opened history.
    /// Invalid, missing and symlink-redirected paths are skipped on read.
    pub fn saved_projects(&self) -> io::Result<Vec<PathBuf>> {
        let saved = self.read_json::<SavedProjects>(&self.state_dir.join("projects.json"))?;
        let Some(saved) = saved else {
            return Ok(Vec::new());
        };
        if saved.version != VERSION {
            return Ok(Vec::new());
        }
        let mut roots = Vec::new();
        for path in saved.roots {
            if roots.len() == MAX_SAVED_PROJECTS {
                break;
            }
            if let Ok(canonical) = dunce::canonicalize(&path) {
                if canonical.is_dir() && canonical == path && !roots.contains(&canonical) {
                    roots.push(canonical);
                }
            }
        }
        Ok(roots)
    }

    /// Register the project with the native desktop UI. No Git or filesystem
    /// content is modified, and the current workspace is not switched.
    pub fn save_project(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let canonical = dunce::canonicalize(path)?;
        if !canonical.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Project must be a directory",
            ));
        }
        let roots = std::iter::once(canonical.clone())
            .chain(
                self.saved_projects()?
                    .into_iter()
                    .filter(|root| root != &canonical),
            )
            .take(MAX_SAVED_PROJECTS)
            .collect();
        self.write_json(
            &self.state_dir.join("projects.json"),
            &SavedProjects {
                version: VERSION,
                roots,
            },
        )
    }

    /// Unpin only; never deletes on-disk projects, preview sessions, or Git.
    /// Missing or non-directory aliases are a no-op rather than an error.
    pub fn unsave_project(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mut removed = false;
        let roots = self
            .saved_projects()?
            .into_iter()
            .filter(|root| {
                let matches = root == &canonical || root.as_path() == path;
                removed |= matches;
                !matches
            })
            .collect::<Vec<_>>();
        if !removed {
            return Ok(());
        }
        self.write_json(
            &self.state_dir.join("projects.json"),
            &SavedProjects {
                version: VERSION,
                roots,
            },
        )
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
        self.write_json(&self.session_path(root), &session_snapshot(model))?;
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
    /// then atomically persist both workspace snapshots and the active hints.
    /// Caller replaces its WorkbenchModel only when the entire operation works.
    pub fn switch_workspace(
        &self,
        departing: &WorkbenchModel,
        destination: impl AsRef<Path>,
    ) -> io::Result<WorkbenchModel> {
        let mut next = WorkbenchModel::new(destination)?;
        self.restore(&mut next)?;

        // Reproduce save(departing) followed by save(next) for the MRU index.
        // The index is only rewritten when one of those saves would rewrite it.
        let mut recent = self.recent_workspaces()?;
        let mut recent_dirty = false;
        for root in [departing.root().to_path_buf(), next.root().to_path_buf()] {
            if recent.first().is_some_and(|path| path == &root) {
                continue;
            }
            let previous = std::mem::take(&mut recent);
            recent = std::iter::once(root.clone())
                .chain(previous.into_iter().filter(|path| path != &root))
                .take(MAX_RECENT_WORKSPACES)
                .collect();
            recent_dirty = true;
        }

        let mut writes = Vec::new();
        push_write(
            &mut writes,
            self.session_path(departing.root()),
            encode_json(&session_snapshot(departing))?,
        );
        push_write(
            &mut writes,
            self.session_path(next.root()),
            encode_json(&session_snapshot(&next))?,
        );
        if recent_dirty {
            push_write(
                &mut writes,
                self.state_dir.join("recent.json"),
                encode_json(&RecentWorkspaces {
                    version: VERSION,
                    roots: recent,
                })?,
            );
        }
        push_write(
            &mut writes,
            self.state_dir.join("last.json"),
            encode_json(&LastWorkspace {
                version: VERSION,
                root: next.root().to_path_buf(),
            })?,
        );
        self.write_transaction(writes)?;
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
        let bytes = encode_json(value)?;
        fs::create_dir_all(&self.state_dir)?;
        self.write_bytes_atomic(path, &bytes)
    }

    /// Persist a small set of state files as one rollback-able transaction.
    /// All target paths are backed up before the first rename so a failed
    /// switch cannot leave a half-written MRU/session state behind.
    fn write_transaction(&self, writes: Vec<(PathBuf, Vec<u8>)>) -> io::Result<()> {
        if writes.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&self.state_dir)?;
        let mut backups: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::with_capacity(writes.len());
        for (path, _) in &writes {
            let contents = match fs::File::open(path) {
                Ok(file) => {
                    if file.metadata()?.len() > MAX_STATE_BYTES {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Existing session state exceeds 64 KiB",
                        ));
                    }
                    let mut bytes = Vec::new();
                    file.take(MAX_STATE_BYTES + 1).read_to_end(&mut bytes)?;
                    if bytes.len() as u64 > MAX_STATE_BYTES {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Existing session state exceeds 64 KiB",
                        ));
                    }
                    Some(bytes)
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            backups.push((path.to_path_buf(), contents));
        }

        let mut written = Vec::with_capacity(writes.len());
        for (path, bytes) in &writes {
            if let Err(error) = self.write_bytes_atomic(path, bytes) {
                self.rollback_transaction(&backups, &written);
                return Err(error);
            }
            written.push(path.to_path_buf());
        }
        Ok(())
    }

    fn rollback_transaction(&self, backups: &[(PathBuf, Option<Vec<u8>>)], written: &[PathBuf]) {
        let mut restored: Vec<PathBuf> = Vec::new();
        for path in written.iter().rev() {
            if restored.contains(path) {
                continue;
            }
            restored.push(path.to_path_buf());
            for (backup_path, contents) in backups {
                if backup_path != path {
                    continue;
                }
                if let Some(bytes) = contents {
                    let _ = self.write_bytes_atomic(path, bytes);
                } else {
                    let _ = fs::remove_file(path);
                }
                break;
            }
        }
    }

    fn write_bytes_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        // Same-directory rename preserves the previous complete snapshot
        // if the process terminates while writing the replacement.
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp = path.with_extension(format!("tmp-{}-{suffix}", std::process::id()));
        let write_result = (|| -> io::Result<()> {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            file.write_all(bytes)?;
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

fn session_snapshot(model: &WorkbenchModel) -> SavedSession {
    let root = model.root();
    let to_relative = |path: &Path| path.strip_prefix(root).ok().map(Path::to_path_buf);
    SavedSession {
        version: VERSION,
        root: root.to_path_buf(),
        directory: to_relative(model.directory()).unwrap_or_default(),
        tabs: model
            .tabs()
            .iter()
            .filter(|tab| !tab.transient)
            .filter_map(|tab| to_relative(&tab.path))
            .take(MAX_RESTORED_TABS)
            .collect(),
        active: model
            .active_document()
            .filter(|tab| !tab.transient)
            .and_then(|tab| to_relative(&tab.path)),
    }
}

fn encode_json<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "State file exceeds 64 KiB",
        ));
    }
    Ok(bytes)
}

fn push_write(writes: &mut Vec<(PathBuf, Vec<u8>)>, path: PathBuf, bytes: Vec<u8>) {
    if let Some(index) = writes.iter().position(|(candidate, _)| candidate == &path) {
        writes[index].1 = bytes;
    } else {
        writes.push((path, bytes));
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
    fn transient_explorer_preview_is_not_restored_or_stored_as_file_tab() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let pinned = workspace.path().join("pinned.txt");
        let scratch = workspace.path().join("preview.txt");
        fs::write(&pinned, "keep").unwrap();
        fs::write(&scratch, "replaceable").unwrap();
        let store = PreviewSessionStore::new(state.path());
        let mut model = WorkbenchModel::new(workspace.path()).unwrap();
        assert!(model.open_path(&pinned));
        assert!(model.preview_path(&scratch));
        assert!(model.active_document().unwrap().transient);
        let snapshot = session_snapshot(&model);
        assert_eq!(snapshot.tabs, vec![PathBuf::from("pinned.txt")]);
        assert_eq!(snapshot.active, None);
        store.save(&model).unwrap();
        let mut reopened = WorkbenchModel::new(workspace.path()).unwrap();
        assert_eq!(store.restore(&mut reopened).unwrap(), 1);
        assert_eq!(reopened.tabs().len(), 1);
        assert!(!reopened.tabs()[0].transient);
        assert_eq!(reopened.tabs()[0].path, pinned);
    }

    #[test]
    fn native_git_favorites_survive_restart_and_stay_scoped_to_canonical_workspace() {
        let state = tempfile::tempdir().unwrap();
        let workspace_a = tempfile::tempdir().unwrap();
        let workspace_b = tempfile::tempdir().unwrap();
        let root_a = dunce::canonicalize(workspace_a.path()).unwrap();
        let root_b = dunce::canonicalize(workspace_b.path()).unwrap();
        let store = PreviewSessionStore::new(state.path());
        assert!(!store.has_legacy_git_favorites(&root_a).unwrap());
        assert!(!store.has_legacy_git_favorites(&root_b).unwrap());
        assert!(store.load_git_favorites(&root_a).unwrap().is_empty());
        let saved = vec!["main".into(), "origin/feature".into()];
        store.save_git_favorites(&root_a, &saved).unwrap();
        assert!(store.has_legacy_git_favorites(&root_a).unwrap());
        let reopened = PreviewSessionStore::new(state.path());
        assert_eq!(reopened.load_git_favorites(&root_a).unwrap(), saved);
        assert!(reopened.load_git_favorites(&root_b).unwrap().is_empty());
        assert!(store
            .save_git_favorites(&root_a, &["main".into(), "main".into()])
            .is_err());
        assert!(store
            .save_git_favorites(&root_a, &["bad\nname".into()])
            .is_err());
        let path = store.git_favorites_path(&root_a);
        fs::write(
            &path,
            b"{\"version\":999,\"root\":\"unsafe\",\"branches\":[]}",
        )
        .unwrap();
        assert!(reopened.load_git_favorites(&root_a).is_err());
        assert!(reopened
            .save_git_favorites(&root_a, &["feature".into()])
            .is_err());
        assert!(
            path.exists(),
            "Corrupt favorite state must never be silently replaced"
        );
    }

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
    fn collapsed_project_roots_persist_across_restart_without_touching_runtime() {
        let state = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(project.path()).unwrap();
        let store = PreviewSessionStore::new(state.path());
        assert!(store.collapsed_project_roots().unwrap().is_empty());
        store
            .save_collapsed_project_roots(std::slice::from_ref(&root))
            .unwrap();
        let reopened = PreviewSessionStore::new(state.path());
        assert_eq!(
            reopened.collapsed_project_roots().unwrap(),
            vec![root.clone()]
        );
        reopened.save_collapsed_project_roots(&[]).unwrap();
        assert!(reopened.collapsed_project_roots().unwrap().is_empty());
        assert!(root.is_dir());
    }

    #[test]
    fn saved_projects_are_explicit_persistent_and_never_deleted_on_unpin() {
        let state = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        fs::write(first.path().join("readme.txt"), "permanent").unwrap();
        let store = PreviewSessionStore::new(state.path());
        assert!(store.saved_projects().unwrap().is_empty());
        store.save_project(first.path()).unwrap();
        store.save_project(second.path()).unwrap();
        store.save_project(first.path()).unwrap();
        let items = store.saved_projects().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], dunce::canonicalize(first.path()).unwrap());
        store.unsave_project(first.path()).unwrap();
        assert_eq!(
            store.saved_projects().unwrap(),
            vec![dunce::canonicalize(second.path()).unwrap()]
        );
        assert_eq!(
            fs::read_to_string(first.path().join("readme.txt")).unwrap(),
            "permanent"
        );
        assert!(store.last_workspace().unwrap().is_none());
    }

    #[test]
    fn saved_projects_ignore_missing_and_uncanonical_paths() {
        let state = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        let store = PreviewSessionStore::new(state.path());
        let canonical = dunce::canonicalize(first.path()).unwrap();
        store
            .write_json(
                &state.path().join("projects.json"),
                &SavedProjects {
                    version: VERSION,
                    roots: vec![
                        canonical.clone(),
                        canonical.clone(),
                        canonical.join(".."),
                        PathBuf::from("relative"),
                    ],
                },
            )
            .unwrap();
        assert_eq!(store.saved_projects().unwrap(), vec![canonical]);
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

    #[test]
    fn saved_projects_are_capped_unique_and_independent_from_mru() {
        let state = tempfile::tempdir().unwrap();
        let workspaces = tempfile::tempdir().unwrap();
        let store = PreviewSessionStore::new(state.path());
        let total = MAX_SAVED_PROJECTS + 2;
        let mut paths = Vec::new();
        let mut canonical = Vec::new();
        for index in 0..total {
            let path = workspaces.path().join(format!("project-{index:02}"));
            fs::create_dir(&path).unwrap();
            store.save_project(&path).unwrap();
            paths.push(path.clone());
            canonical.push(dunce::canonicalize(&path).unwrap());
        }
        let dropped = canonical.len() - MAX_SAVED_PROJECTS;
        let expected: Vec<PathBuf> = canonical[dropped..].iter().rev().cloned().collect();
        assert_eq!(store.saved_projects().unwrap(), expected);
        assert!(!store.saved_projects().unwrap().contains(&canonical[0]));

        // Re-saving through a `.` alias keeps the same canonical pin set.
        store.save_project(paths.last().unwrap().join(".")).unwrap();
        assert_eq!(store.saved_projects().unwrap(), expected);

        assert!(store.recent_workspaces().unwrap().is_empty());
        assert!(store.last_workspace().unwrap().is_none());
    }

    #[test]
    fn unpin_preserves_project_files_workspace_preview_and_open_tabs() {
        let state = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let store = PreviewSessionStore::new(state.path());
        fs::write(project.path().join("note.txt"), "draft source").unwrap();
        let mut model = WorkbenchModel::new(project.path()).unwrap();
        assert!(model.open_path(project.path().join("note.txt")));
        store.save(&model).unwrap();
        let session_path = store.session_path(model.root());
        assert!(session_path.exists());
        let recent = store.recent_workspaces().unwrap();
        let last = store.last_workspace().unwrap();

        store.save_project(project.path()).unwrap();
        assert_eq!(
            store.saved_projects().unwrap(),
            vec![dunce::canonicalize(project.path()).unwrap()]
        );
        store.unsave_project(project.path().join(".")).unwrap();
        assert!(store.saved_projects().unwrap().is_empty());
        assert!(session_path.exists());
        assert_eq!(
            fs::read_to_string(project.path().join("note.txt")).unwrap(),
            "draft source"
        );
        assert_eq!(store.recent_workspaces().unwrap(), recent);
        assert_eq!(store.last_workspace().unwrap(), last);

        let mut restored = WorkbenchModel::new(project.path()).unwrap();
        assert_eq!(store.restore(&mut restored).unwrap(), 1);
        assert_eq!(restored.content(), "draft source");
    }

    #[test]
    fn failed_switch_leaves_workspace_snapshot_and_mru_untouched() {
        let state = tempfile::tempdir().unwrap();
        let root_a = tempfile::tempdir().unwrap();
        let root_b = tempfile::tempdir().unwrap();
        fs::write(root_a.path().join("a.txt"), "A").unwrap();
        fs::write(root_a.path().join("a2.txt"), "A2").unwrap();
        fs::write(root_b.path().join("b.txt"), "B").unwrap();
        let store = PreviewSessionStore::new(state.path());
        let mut a = WorkbenchModel::new(root_a.path()).unwrap();
        assert!(a.open_path(root_a.path().join("a.txt")));
        let mut b = WorkbenchModel::new(root_b.path()).unwrap();
        assert!(b.open_path(root_b.path().join("b.txt")));
        store.save(&b).unwrap();
        store.save(&a).unwrap();
        assert!(a.open_path(root_a.path().join("a2.txt")));

        let last_path = state.path().join("last.json");
        fs::remove_file(&last_path).unwrap();
        fs::create_dir(&last_path).unwrap();

        assert!(store.switch_workspace(&a, root_b.path()).is_err());
        assert_eq!(
            store.recent_workspaces().unwrap(),
            vec![a.root().to_path_buf(), b.root().to_path_buf()]
        );
        let mut restored_a = WorkbenchModel::new(root_a.path()).unwrap();
        assert_eq!(store.restore(&mut restored_a).unwrap(), 1);
        assert_eq!(restored_a.content(), "A");
        let mut restored_b = WorkbenchModel::new(root_b.path()).unwrap();
        assert_eq!(store.restore(&mut restored_b).unwrap(), 1);
        assert_eq!(restored_b.content(), "B");
        assert!(last_path.is_dir());
    }

    #[test]
    fn terminal_layout_roundtrip_is_workspace_scoped_and_metadata_only() {
        let state = tempfile::tempdir().unwrap();
        let root_a = tempfile::tempdir().unwrap();
        let root_b = tempfile::tempdir().unwrap();
        let root_a = dunce::canonicalize(root_a.path()).unwrap();
        let root_b = dunce::canonicalize(root_b.path()).unwrap();
        let store = PreviewSessionStore::new(state.path());
        let layout = TerminalLayout {
            tabs: vec![
                TerminalLayoutTab {
                    handle: "handle-a".into(),
                    workspace_id: "workspace-1".into(),
                    tab_id: "tab-1".into(),
                },
                TerminalLayoutTab {
                    handle: "handle-b".into(),
                    workspace_id: "workspace-1".into(),
                    tab_id: "tab-2".into(),
                },
            ],
            active_handle: Some("handle-a".into()),
            transcript_view: true,
        };
        store.save_terminal_layout(&root_a, &layout).unwrap();
        assert_eq!(store.load_terminal_layout(&root_a).unwrap(), Some(layout));
        assert_eq!(store.load_terminal_layout(&root_b).unwrap(), None);
        let bytes = fs::read(store.terminal_layout_path(&root_a)).unwrap();
        let serialized = String::from_utf8(bytes).unwrap();
        assert!(!serialized.contains("output"));
        assert!(!serialized.contains("command"));
        assert!(!serialized.contains("token"));
    }

    #[test]
    fn terminal_layout_rejects_duplicate_handles_bad_ids_and_corrupted_storage() {
        let state = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(root.path()).unwrap();
        let store = PreviewSessionStore::new(state.path());
        let tab = TerminalLayoutTab {
            handle: "handle-1".into(),
            workspace_id: "workspace-1".into(),
            tab_id: "tab-1".into(),
        };
        let mut layout = TerminalLayout {
            tabs: vec![tab.clone(), tab],
            active_handle: Some("handle-1".into()),
            transcript_view: false,
        };
        assert!(store.save_terminal_layout(&root, &layout).is_err());
        layout.tabs.pop();
        layout.tabs[0].handle = "bad\nhandle".into();
        assert!(store.save_terminal_layout(&root, &layout).is_err());
        layout.tabs[0].handle = "handle-1".into();
        layout.active_handle = Some("unknown".into());
        assert!(store.save_terminal_layout(&root, &layout).is_err());
        layout.active_handle = Some("handle-1".into());
        store.save_terminal_layout(&root, &layout).unwrap();
        fs::write(store.terminal_layout_path(&root), "{invalid json").unwrap();
        assert!(store.load_terminal_layout(&root).is_err());
    }

    #[test]
    fn terminal_layout_must_not_accept_tampered_workspace_root_or_excess_tabs() {
        let state = tempfile::tempdir().unwrap();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let a = dunce::canonicalize(a.path()).unwrap();
        let b = dunce::canonicalize(b.path()).unwrap();
        let store = PreviewSessionStore::new(state.path());
        let layout = TerminalLayout {
            tabs: (0..9)
                .map(|i| TerminalLayoutTab {
                    handle: format!("term-{i}"),
                    workspace_id: "workspace".into(),
                    tab_id: format!("tab-{i}"),
                })
                .collect(),
            active_handle: None,
            transcript_view: false,
        };
        assert!(store.save_terminal_layout(&a, &layout).is_err());
        let valid = TerminalLayout::default();
        store.save_terminal_layout(&a, &valid).unwrap();
        let file_a = store.terminal_layout_path(&a);
        let file_b = store.terminal_layout_path(&b);
        fs::copy(&file_a, &file_b).unwrap();
        assert!(store.load_terminal_layout(&b).is_err());
    }

    #[test]
    fn terminal_layout_rejects_same_host_tab_aliased_under_different_handles() {
        let state = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(root.path()).unwrap();
        let store = PreviewSessionStore::new(state.path());
        let layout = TerminalLayout {
            tabs: vec![
                TerminalLayoutTab {
                    handle: "one".into(),
                    workspace_id: "ws-1".into(),
                    tab_id: "same-tab".into(),
                },
                TerminalLayoutTab {
                    handle: "two".into(),
                    workspace_id: "ws-1".into(),
                    tab_id: "same-tab".into(),
                },
            ],
            active_handle: Some("one".into()),
            transcript_view: false,
        };
        assert!(
            store.save_terminal_layout(&root, &layout).is_err(),
            "duplicate Host tab must not be saved under different handles"
        );
    }

    #[test]
    fn terminal_layout_can_be_updated_repeatedly_without_losing_prior_good_state() {
        let state = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(root.path()).unwrap();
        let store = PreviewSessionStore::new(state.path());
        let mut layout = TerminalLayout {
            tabs: vec![TerminalLayoutTab {
                handle: "one".into(),
                workspace_id: "ws-1".into(),
                tab_id: "tab-1".into(),
            }],
            active_handle: Some("one".into()),
            transcript_view: false,
        };
        store.save_terminal_layout(&root, &layout).unwrap();
        layout.transcript_view = true;
        store.save_terminal_layout(&root, &layout).unwrap();
        assert_eq!(
            store.load_terminal_layout(&root).unwrap(),
            Some(layout.clone())
        );
        layout.active_handle = Some("nonexistent".into());
        assert!(store.save_terminal_layout(&root, &layout).is_err());
        layout.active_handle = Some("one".into());
        assert_eq!(store.load_terminal_layout(&root).unwrap(), Some(layout));
    }
}
