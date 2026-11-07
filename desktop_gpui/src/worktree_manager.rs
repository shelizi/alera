//! Guarded Git worktree discovery/create/remove for native GPUI Projects.
//! Git is invoked directly (never through a shell), and only an explicitly
//! managed checkout below the repository's own .worktrees/ may be removed.
use std::ffi::OsStr;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MAX_WORKTREES: usize = 96;
const MAX_GIT_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
    pub is_primary: bool,
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
                if let Some(mut found) = entry.take() {
                    found.is_primary = entries.is_empty();
                    if entries.len() == MAX_WORKTREES {
                        return Err(invalid("Too many Git worktrees for this preview"));
                    }
                    entries.push(found);
                }
            } else if let Some(value) = line.strip_prefix("worktree ") {
                let path = PathBuf::from(value);
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
        Ok(entries)
    }

    pub fn create(&self, branch: &str) -> io::Result<PathBuf> {
        validate_branch(branch)?;
        let entries = self.list()?;
        let primary = &entries[0].path;
        if entries
            .iter()
            .any(|item| item.branch.as_deref() == Some(branch))
        {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                "Branch already checked out in a worktree",
            ));
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
        if dunce::canonicalize(&directory)? != directory {
            return Err(forbidden("Worktree storage resolves to another directory"));
        }
        // Full branch name -> unique one-segment folder name, no traversal.
        let folder = branch.replace('/', "--");
        let path = directory.join(folder);
        if fs::symlink_metadata(&path).is_ok() {
            return Err(io::Error::new(
                ErrorKind::AlreadyExists,
                "Managed worktree folder already exists",
            ));
        }
        let output = self.git_args(&[
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("-b"),
            OsStr::new(branch),
            path.as_os_str(),
            OsStr::new("HEAD"),
        ])?;
        let _ = output;
        dunce::canonicalize(path)
    }

    /// Delete only a managed checkout after a deliberate UI confirmation.
    /// Does not force-remove dirty/locked/untracked worktrees or delete branch.
    pub fn remove(&self, target: &Path, confirmed: bool) -> io::Result<()> {
        if !confirmed {
            return Err(forbidden("Worktree removal requires confirmation"));
        }
        let target = dunce::canonicalize(target)?;
        let entries = self.list()?;
        let primary = &entries[0].path;
        let managed = primary.join(".worktrees");
        if target.parent() != Some(managed.as_path()) {
            return Err(forbidden(
                "Only worktrees in the managed .worktrees folder may be removed",
            ));
        }
        let entry = entries
            .iter()
            .find(|entry| entry.path == target)
            .ok_or_else(|| invalid("Worktree is not registered with Git"))?;
        if entry.is_primary || entry.locked || entry.prunable || self.repo_root == target {
            return Err(forbidden(
                "Cannot remove primary, active, locked or stale worktree",
            ));
        }
        let clean = self.git_in(&target, &["status", "--porcelain", "--untracked-files=all"])?;
        if !clean.trim().is_empty() {
            return Err(forbidden("Worktree has uncommitted or untracked files"));
        }
        self.git_args(&[
            OsStr::new("worktree"),
            OsStr::new("remove"),
            target.as_os_str(),
        ])?;
        Ok(())
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
        command.stdin(Stdio::null()).stderr(Stdio::piped());
        command
    }
}

fn capture(mut command: Command) -> io::Result<String> {
    let output = command.output()?;
    if output.stdout.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(invalid("Git output exceeded 256 KiB"));
    }
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!(
            "Git command failed ({}): {}",
            output.status,
            reason.trim().chars().take(500).collect::<String>()
        )));
    }
    String::from_utf8(output.stdout).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

fn validate_branch(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name.len() > 80
        || name.starts_with('-')
        || name.starts_with('/')
        || name.ends_with('/')
        || name.ends_with('.')
        || name.contains("..")
        || name.contains("//")
        || name.contains("@{")
        || name.contains(".lock")
        || name
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/')))
    {
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
        assert_eq!(manager.list().unwrap().len(), 1);
    }

    #[test]
    fn rejects_bad_branches_and_unmanaged_or_main_deletion() {
        let (_dir, manager) = initialized_repo();
        for branch in ["", "../escape", "--force", "bad name", "foo//bar", "a.lock"] {
            assert!(manager.create(branch).is_err());
        }
        assert!(manager.remove(manager.root(), true).is_err());
        let external = tempfile::tempdir().unwrap();
        assert!(manager.remove(external.path(), true).is_err());
    }
}
