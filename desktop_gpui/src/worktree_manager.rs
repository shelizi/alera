//! Guarded Git worktree discovery/create/remove for native GPUI Projects.
//! Git is invoked directly (never through a shell), prompts are disabled, and
//! every invocation is bounded by a timeout. Only an explicitly managed
//! checkout directly below the repository's own .worktrees/ may be removed.
use std::ffi::OsStr;
use std::fs;
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const MAX_WORKTREES: usize = 96;
const MAX_GIT_OUTPUT_BYTES: usize = 256 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(60);
const GIT_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const GIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
    pub is_primary: bool,
}

/// Exact, safe branch-to-worktree resolution shared by Projects and Git Graph.
/// Never pick a detached, locked or prunable checkout as a navigation target.
pub fn matching_branch_worktree<'a>(
    entries: &'a [WorktreeEntry],
    branch: &str,
) -> Option<&'a WorktreeEntry> {
    if branch.is_empty() {
        return None;
    }
    entries.iter().find(|entry| {
        !entry.detached
            && !entry.locked
            && !entry.prunable
            && entry.branch.as_deref() == Some(branch)
    })
}

#[derive(Debug, Clone)]
pub struct WorktreeManager {
    repo_root: PathBuf,
}

impl WorktreeManager {
    pub fn discover(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = dunce::canonicalize(root)?;
        let repo = git2::Repository::discover(&root)
            .map_err(|e| io::Error::new(ErrorKind::NotFound, e))?;
        let repo_root = repo
            .workdir()
            .ok_or_else(|| invalid("Bare repositories cannot manage GUI worktrees"))?;
        let repo_root = dunce::canonicalize(repo_root)?;
        Ok(Self { repo_root })
    }

    pub fn root(&self) -> &Path {
        &self.repo_root
    }

    pub fn list(&self) -> io::Result<Vec<WorktreeEntry>> {
        let raw = self.git(&["worktree", "list", "--porcelain"])?;
        let mut entries = Vec::new();
        let mut entry: Option<WorktreeEntry> = None;
        for line in raw.lines().chain(std::iter::once("")) {
            if line.is_empty() {
                if let Some(found) = entry.take() {
                    if entries.len() == MAX_WORKTREES {
                        return Err(invalid("Too many Git worktrees for this preview"));
                    }
                    entries.push(found);
                }
            } else if let Some(value) = line.strip_prefix("worktree ") {
                let path = PathBuf::from(unquote_git_path(value));
                entry = Some(WorktreeEntry {
                    path: dunce::canonicalize(&path).unwrap_or(path),
                    branch: None,
                    detached: false,
                    locked: false,
                    prunable: false,
                    is_primary: false,
                });
            } else if let Some(current) = &mut entry {
                if let Some(branch) = line.strip_prefix("branch refs/heads/") {
                    current.branch = Some(branch.to_string());
                } else if line == "detached" {
                    current.detached = true;
                } else if line.starts_with("locked") {
                    current.locked = true;
                } else if line.starts_with("prunable") {
                    current.prunable = true;
                }
            }
        }
        if entries.is_empty() {
            return Err(invalid("Git returned no worktrees"));
        }
        // The main worktree keeps a real .git directory while every linked
        // worktree has a .git file. Prefer that filesystem evidence and fall
        // back to git's documented ordering (main worktree listed first).
        if let Some(index) = entries
            .iter()
            .position(|entry| is_main_worktree(&entry.path))
        {
            entries[index].is_primary = true;
        } else if let Some(first) = entries.first_mut() {
            first.is_primary = true;
        }
        Ok(entries)
    }

    pub fn create(&self, branch: &str) -> io::Result<PathBuf> {
        self.create_at_ref(branch, "HEAD")
    }

    /// Create one managed worktree and new local branch at an exact commit,
    /// leaving this worktree's HEAD, index and files untouched. Existing
    /// refs or target folders are never overwritten.
    pub fn create_at_commit(&self, branch: &str, commit_sha: &str) -> io::Result<PathBuf> {
        if commit_sha.len() != 40 || !commit_sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid("Git Graph Worktree requires an exact commit SHA"));
        }
        let repo = git2::Repository::discover(&self.repo_root)
            .map_err(|_| invalid("Cannot inspect Git repository for selected commit"))?;
        let oid = git2::Oid::from_str(commit_sha).map_err(|_| invalid("Invalid Git commit ID"))?;
        repo.find_commit(oid)
            .map_err(|_| invalid("Selected Git commit does not exist"))?;
        self.create_at_ref(branch, commit_sha)
    }

    fn create_at_ref(&self, branch: &str, source_ref: &str) -> io::Result<PathBuf> {
        self.create_for_branch(branch, false, source_ref)
    }

    /// Check out an existing local branch without creating or resetting it.
    /// Remote-tracking branches are never selected implicitly.
    pub fn create_existing(&self, branch: &str) -> io::Result<PathBuf> {
        self.create_for_branch(branch, true, "HEAD")
    }

    fn create_for_branch(
        &self,
        branch: &str,
        existing: bool,
        source_ref: &str,
    ) -> io::Result<PathBuf> {
        validate_branch(branch)?;
        let repo = git2::Repository::discover(&self.repo_root)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let branch_exists = repo.find_reference(&format!("refs/heads/{branch}")).is_ok();
        if existing && !branch_exists {
            return Err(io::Error::new(
                ErrorKind::NotFound,
                "The selected local branch does not exist",
            ));
        }
        if !existing && branch_exists {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                "Branch already exists; use the existing-branch option",
            ));
        }
        let entries = self.list()?;
        let primary = primary_worktree(&entries)?;
        if entries
            .iter()
            .any(|item| item.branch.as_deref() == Some(branch))
        {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                "Branch already checked out in a worktree",
            ));
        }
        let folder = worktree_folder_name(branch);
        if is_windows_device_name(&folder) {
            return Err(invalid("Branch name is reserved on Windows"));
        }
        let directory = primary.join(".worktrees");
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(forbidden("Worktree storage must be a real directory"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => fs::create_dir(&directory)?,
            Err(error) => return Err(error),
        }
        let directory = dunce::canonicalize(&directory)?;
        if directory.parent() != Some(primary) {
            return Err(forbidden("Worktree storage resolves to another directory"));
        }
        // The encoding is injective, so this path can only collide with a
        // leftover directory or an already-registered worktree.
        let path = directory.join(folder);
        if fs::symlink_metadata(&path).is_ok() || entries.iter().any(|item| item.path == path) {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                "Managed worktree folder already exists",
            ));
        }
        if existing {
            self.git_args(&[
                OsStr::new("worktree"),
                OsStr::new("add"),
                path.as_os_str(),
                OsStr::new(branch),
            ])?;
        } else {
            self.git_args(&[
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("-b"),
                OsStr::new(branch),
                path.as_os_str(),
                OsStr::new(source_ref),
            ])?;
        }
        dunce::canonicalize(path)
    }

    /// Delete only a managed checkout after a deliberate UI confirmation.
    /// Never uses --force, never deletes the branch, and refuses anything
    /// active, primary, dirty, untracked, locked, unregistered, or outside the
    /// managed .worktrees folder.
    pub fn remove(&self, target: &Path, confirmed: bool) -> io::Result<()> {
        if !confirmed {
            return Err(forbidden("Worktree removal requires confirmation"));
        }
        let target = self.preflight_remove(target)?;
        // No --force: git itself also refuses dirty or locked worktrees, and
        // the branch is intentionally left behind.
        self.git_args(&[
            OsStr::new("worktree"),
            OsStr::new("remove"),
            target.as_os_str(),
        ])?;
        Ok(())
    }

    /// Shared non-destructive safety gate for local and Host-managed removals.
    /// Protect the primary/active worktree, out-of-tree paths and all changes.
    pub fn preflight_remove(&self, target: &Path) -> io::Result<PathBuf> {
        let target = dunce::canonicalize(target)?;
        let entries = self.list()?;
        let primary = primary_worktree(&entries)?;
        let entry = entries
            .iter()
            .find(|entry| entry.path == target)
            .ok_or_else(|| invalid("Worktree is not registered with Git"))?;
        if entry.is_primary
            || entry.locked
            || entry.prunable
            || target.as_path() == primary
            || target == self.repo_root
            || self.repo_root.starts_with(&target)
        {
            return Err(forbidden(
                "Cannot remove primary, active, locked or stale worktree",
            ));
        }
        if let Ok(cwd) = std::env::current_dir() {
            let cwd = dunce::canonicalize(&cwd).unwrap_or(cwd);
            if cwd.starts_with(&target) {
                return Err(forbidden(
                    "Cannot remove the worktree this process is running inside",
                ));
            }
        }
        let managed_raw = primary.join(".worktrees");
        match fs::symlink_metadata(&managed_raw) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(forbidden("Worktree storage must be a real directory"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(forbidden("No managed worktree storage exists"));
            }
            Err(error) => return Err(error),
        }
        let managed = dunce::canonicalize(&managed_raw)?;
        if managed.parent() != Some(primary) || target.parent() != Some(managed.as_path()) {
            return Err(forbidden(
                "Only worktrees in the managed .worktrees folder may be removed",
            ));
        }
        let clean = self.git_in(&target, &["status", "--porcelain", "--untracked-files=all"])?;
        if !clean.trim().is_empty() {
            return Err(forbidden("Worktree has uncommitted or untracked files"));
        }
        Ok(target)
    }

    fn git(&self, args: &[&str]) -> io::Result<String> {
        self.git_in(&self.repo_root, args)
    }

    fn git_in(&self, directory: &Path, args: &[&str]) -> io::Result<String> {
        let mut command = self.git_command();
        command.arg("-C").arg(directory).args(args);
        capture(command)
    }

    fn git_args(&self, args: &[&OsStr]) -> io::Result<String> {
        let mut command = self.git_command();
        command.arg("-C").arg(&self.repo_root).args(args);
        capture(command)
    }

    fn git_command(&self) -> Command {
        let mut command = alera_core::child_process::windowless_command("git");
        command.stdin(Stdio::null()).env("GIT_TERMINAL_PROMPT", "0");
        command
    }
}

fn capture(mut command: Command) -> io::Result<String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (stdout_tx, stdout_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = stdout_tx.send(read_capped(stdout));
    });
    let (stderr_tx, stderr_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = stderr_tx.send(read_capped(stderr));
    });
    let deadline = Instant::now() + GIT_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                // Give the readers a moment to observe EOF; a grandchild that
                // inherited the pipe may keep them blocked, so this is bounded.
                let _ = stdout_rx.recv_timeout(GIT_DRAIN_TIMEOUT);
                let _ = stderr_rx.recv_timeout(GIT_DRAIN_TIMEOUT);
                return Err(io::Error::new(ErrorKind::TimedOut, "Git command timed out"));
            }
            Ok(None) => thread::sleep(GIT_POLL_INTERVAL),
            Err(error) => {
                let _ = child.kill();
                return Err(error);
            }
        }
    };
    let stdout = stdout_rx
        .recv_timeout(GIT_DRAIN_TIMEOUT)
        .map_err(|_| io::Error::new(ErrorKind::TimedOut, "Git stdout did not close"))??;
    let stderr = stderr_rx
        .recv_timeout(GIT_DRAIN_TIMEOUT)
        .map_err(|_| io::Error::new(ErrorKind::TimedOut, "Git stderr did not close"))??;
    if stdout.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(invalid("Git output exceeded 256 KiB"));
    }
    if !status.success() {
        let reason = String::from_utf8_lossy(&stderr);
        return Err(io::Error::other(format!(
            "Git command failed ({}): {}",
            status,
            reason.trim().chars().take(500).collect::<String>()
        )));
    }
    String::from_utf8(stdout).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

/// Read at most MAX_GIT_OUTPUT_BYTES + 1 into memory, then drain the rest so
/// the child never stalls on a full pipe. Oversized output is reported by the
/// caller once the child has exited.
fn read_capped<R: Read>(mut reader: R) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    (&mut reader)
        .take(MAX_GIT_OUTPUT_BYTES as u64 + 1)
        .read_to_end(&mut buffer)?;
    io::copy(&mut reader, &mut io::sink())?;
    Ok(buffer)
}

fn primary_worktree(entries: &[WorktreeEntry]) -> io::Result<&Path> {
    entries
        .iter()
        .find(|entry| entry.is_primary)
        .map(|entry| entry.path.as_path())
        .ok_or_else(|| invalid("Git returned no primary worktree"))
}

fn is_main_worktree(path: &Path) -> bool {
    fs::metadata(path.join(".git"))
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

/// `git worktree list --porcelain` C-quotes paths containing special bytes.
fn unquote_git_path(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
        return value.to_string();
    }
    let inner = &bytes[1..bytes.len() - 1];
    let mut out = Vec::with_capacity(inner.len());
    let mut index = 0;
    while index < inner.len() {
        let byte = inner[index];
        index += 1;
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        if index >= inner.len() {
            out.push(b'\\');
            break;
        }
        let escape = inner[index];
        index += 1;
        match escape {
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0C),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0B),
            b'\\' => out.push(b'\\'),
            b'"' => out.push(b'"'),
            b'0'..=b'7' => {
                let mut value = (escape - b'0') as u32;
                for _ in 0..2 {
                    if index < inner.len() && inner[index].is_ascii_digit() {
                        value = value * 8 + (inner[index] - b'0') as u32;
                        index += 1;
                    }
                }
                out.push(u8::try_from(value).unwrap_or(b'?'));
            }
            other => {
                out.push(b'\\');
                out.push(other);
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Map a validated branch name to a single path segment. `+` can never appear
/// in a validated branch name, so the encoding is injective: `a/b` and `a--b`
/// or `a+b` can never claim the same folder.
fn worktree_folder_name(branch: &str) -> String {
    branch.replace('/', "+")
}

fn is_windows_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

pub(crate) fn validate_branch(name: &str) -> io::Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 80
        && name != "@"
        && !name.starts_with('-')
        && !name.starts_with('/')
        && !name.ends_with('/')
        && !name.ends_with('.')
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
        && name.split('/').all(|segment| {
            !segment.is_empty() && !segment.starts_with('.') && !segment.ends_with(".lock")
        });
    if !valid {
        return Err(invalid(
            "Use a simple branch name, e.g. feature/native-editor",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}
fn forbidden(message: &str) -> io::Error {
    io::Error::new(ErrorKind::PermissionDenied, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};

    fn initialized_repo() -> (tempfile::TempDir, WorktreeManager) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let repo = Repository::init(&root).unwrap();
        fs::write(root.join("README"), "initial").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = Signature::now("GPUI Test", "gpui@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);
        let manager = WorktreeManager::discover(&root).unwrap();
        (temp, manager)
    }

    #[test]
    fn graph_branch_navigation_requires_exact_live_worktree() {
        let base = WorktreeEntry {
            path: PathBuf::from("C:/repo/.worktrees/feature"),
            branch: Some("feature".into()),
            detached: false,
            locked: false,
            prunable: false,
            is_primary: false,
        };
        let refs = vec![
            WorktreeEntry {
                path: PathBuf::from("C:/repo/.worktrees/stale"),
                branch: Some("feature".into()),
                locked: true,
                ..base.clone()
            },
            base.clone(),
            WorktreeEntry {
                path: PathBuf::from("C:/repo/.worktrees/feat"),
                branch: Some("feat".into()),
                ..base.clone()
            },
            WorktreeEntry {
                path: PathBuf::from("C:/repo/.worktrees/detached"),
                detached: true,
                ..base.clone()
            },
        ];
        assert_eq!(
            matching_branch_worktree(&refs, "feature").unwrap().path,
            base.path
        );
        assert_eq!(
            matching_branch_worktree(&refs, "feat")
                .unwrap()
                .branch
                .as_deref(),
            Some("feat")
        );
        assert!(matching_branch_worktree(&refs, "").is_none());
        assert!(matching_branch_worktree(&refs, "Feature").is_none());
        assert!(matching_branch_worktree(&refs, "other").is_none());
        let prunable = [WorktreeEntry {
            prunable: true,
            ..base.clone()
        }];
        assert!(matching_branch_worktree(&prunable, "feature").is_none());
    }

    #[test]
    fn create_at_commit_starts_new_managed_branch_without_moving_current_head() {
        let (_dir, manager) = initialized_repo();
        let repo = git2::Repository::discover(manager.root()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let original_oid = head.id();
        let original_branch = repo.head().unwrap().shorthand().unwrap().to_owned();
        let original_file = fs::read(manager.root().join("tracked.txt")).ok();
        let sha = original_oid.to_string();
        let worktree = manager.create_at_commit("from-history", &sha).unwrap();
        let linked = git2::Repository::discover(&worktree).unwrap();
        assert_eq!(
            linked.head().unwrap().peel_to_commit().unwrap().id(),
            original_oid
        );
        assert_eq!(linked.head().unwrap().shorthand().unwrap(), "from-history");
        assert_eq!(repo.head().unwrap().target(), Some(original_oid));
        assert_eq!(repo.head().unwrap().shorthand().unwrap(), original_branch);
        assert_eq!(
            fs::read(manager.root().join("tracked.txt")).ok(),
            original_file
        );
        assert!(manager.create_at_commit("from-history", &sha).is_err());
        assert!(manager.create_at_commit("another", "abcdef").is_err());
        assert!(manager.create_at_commit("../escape", &sha).is_err());
        assert!(manager
            .create_at_commit("missing", &"0".repeat(40))
            .is_err());
        manager.remove(&worktree, true).unwrap();
    }

    #[test]
    fn host_removal_preflight_is_non_destructive_and_refuses_dirty_worktrees() {
        let (_dir, manager) = initialized_repo();
        let target = manager.create("test/preflight").unwrap();
        assert_eq!(manager.preflight_remove(&target).unwrap(), target);
        assert!(target.is_dir());
        fs::write(target.join("UNTRACKED"), "keep my data").unwrap();
        assert_eq!(
            manager.preflight_remove(&target).unwrap_err().kind(),
            ErrorKind::PermissionDenied
        );
        assert!(target.join("UNTRACKED").exists());
        assert_eq!(
            manager.preflight_remove(manager.root()).unwrap_err().kind(),
            ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn lists_creates_and_removes_only_clean_managed_worktrees() {
        let (_dir, manager) = initialized_repo();
        let initial = manager.list().unwrap();
        assert_eq!(initial.len(), 1);
        assert!(initial[0].is_primary);
        let created = manager.create("feature/new-native").unwrap();
        assert!(created.is_dir());
        assert_eq!(manager.list().unwrap().len(), 2);
        assert!(manager.remove(&created, false).is_err());
        fs::write(created.join("dirty.txt"), "untracked").unwrap();
        assert!(manager.remove(&created, true).is_err());
        fs::remove_file(created.join("dirty.txt")).unwrap();
        manager.remove(&created, true).unwrap();
        assert!(!created.exists());
        // Removal never deletes the branch.
        let branches = manager
            .git(&["branch", "--list", "feature/new-native"])
            .unwrap();
        assert!(branches.contains("feature/new-native"));
        assert_eq!(manager.list().unwrap().len(), 1);
    }

    #[test]
    fn existing_local_branch_checkout_reuses_branch_without_mutating_it() {
        let (_dir, manager) = initialized_repo();
        let repo = Repository::open(manager.root()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let original_commit = head.id();
        repo.branch("feature/existing", &head, false).unwrap();
        drop(head);
        drop(repo);

        assert_eq!(
            manager
                .create_existing("feature/missing")
                .unwrap_err()
                .kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            manager.create("feature/existing").unwrap_err().kind(),
            ErrorKind::AlreadyExists
        );
        let path = manager.create_existing("feature/existing").unwrap();
        assert!(manager.list().unwrap().iter().any(|entry| {
            entry.path == path && entry.branch.as_deref() == Some("feature/existing")
        }));
        assert!(manager.create_existing("feature/existing").is_err());
        manager.remove(&path, true).unwrap();

        let repo = Repository::open(manager.root()).unwrap();
        assert_eq!(
            repo.find_reference("refs/heads/feature/existing")
                .unwrap()
                .target(),
            Some(original_commit)
        );
    }

    #[test]
    fn discovers_linked_worktree_and_keeps_primary_protected() {
        let (_dir, manager) = initialized_repo();
        let created = manager.create("feature/linked").unwrap();
        let canonical = dunce::canonicalize(&created).unwrap();
        // Discovering from inside a linked worktree must still identify the
        // main checkout as primary.
        let linked = WorktreeManager::discover(&created).unwrap();
        assert_eq!(linked.root(), canonical.as_path());
        let entries = linked.list().unwrap();
        assert_eq!(entries.len(), 2);
        let primaries: Vec<&WorktreeEntry> =
            entries.iter().filter(|entry| entry.is_primary).collect();
        assert_eq!(primaries.len(), 1);
        assert_eq!(primaries[0].path.as_path(), manager.root());
        // The primary checkout is never removable, even via the linked view.
        assert!(linked.remove(manager.root(), true).is_err());
        // A manager obtained from a linked checkout must not delete itself.
        assert!(linked.remove(&created, true).is_err());
        manager.remove(&created, true).unwrap();
        assert!(!canonical.exists());
        assert_eq!(manager.list().unwrap().len(), 1);
    }

    #[test]
    fn refuses_locked_worktree_until_unlocked() {
        let (_dir, manager) = initialized_repo();
        let created = manager.create("feature/locked").unwrap();
        let target = created.to_string_lossy().into_owned();
        manager.git(&["worktree", "lock", target.as_str()]).unwrap();
        let entries = manager.list().unwrap();
        assert!(
            entries
                .iter()
                .find(|entry| entry.path == created)
                .unwrap()
                .locked
        );
        assert!(manager.remove(&created, true).is_err());
        manager
            .git(&["worktree", "unlock", target.as_str()])
            .unwrap();
        manager.remove(&created, true).unwrap();
    }

    #[test]
    fn managed_folder_names_never_collide() {
        let (_dir, manager) = initialized_repo();
        let nested = manager.create("feature/shared").unwrap();
        let dashed = manager.create("feature--shared").unwrap();
        assert_ne!(nested, dashed);
        assert_eq!(manager.list().unwrap().len(), 3);
    }

    #[test]
    fn rejects_bad_branches_and_unmanaged_or_main_deletion() {
        let (_dir, manager) = initialized_repo();
        for branch in [
            "",
            "../escape",
            "--force",
            "bad name",
            "foo//bar",
            "a.lock",
            "a@{b}",
            "feature..x",
            ".hidden/x",
            "con",
            "lpt1.dat",
        ] {
            assert!(manager.create(branch).is_err(), "branch: {branch}");
        }
        assert!(manager.remove(manager.root(), true).is_err());
        let external = tempfile::tempdir().unwrap();
        assert!(manager.remove(external.path(), true).is_err());
        // A plain directory inside .worktrees that Git never registered.
        let stray = manager.root().join(".worktrees").join("stray");
        fs::create_dir_all(&stray).unwrap();
        assert!(manager.remove(&stray, true).is_err());
        // A Git-registered worktree outside the managed folder.
        let outside_dir = tempfile::tempdir().unwrap();
        let outside = outside_dir.path().join("outside");
        let outside_arg = outside.to_string_lossy().into_owned();
        manager
            .git(&["worktree", "add", "--detach", outside_arg.as_str(), "HEAD"])
            .unwrap();
        assert!(manager.remove(&outside, true).is_err());
    }
}
