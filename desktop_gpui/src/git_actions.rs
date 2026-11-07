//! Explicit, worktree-scoped Git writers. The GPUI render path never calls
//! this module; a native click starts a bounded background operation.
//! No command invokes reset --hard, merge, rebase, shell or force push.
use git2::{build::CheckoutBuilder, BranchType, Repository, StatusOptions};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub struct GitActions {
    repo: Repository,
    root: PathBuf,
}

impl GitActions {
    pub fn open(workspace: &Path) -> Result<Self, String> {
        let root = dunce::canonicalize(workspace).map_err(|error| error.to_string())?;
        if root != workspace || !root.is_dir() {
            return Err("Git action requires an existing canonical workspace root".into());
        }
        let repo = Repository::discover(&root).map_err(|error| error.to_string())?;
        if repo.is_bare() {
            return Err("Bare repositories cannot be edited through GPUI".into());
        }
        let workdir = repo
            .workdir()
            .ok_or_else(|| "Repository has no working directory".to_string())
            .and_then(|p| dunce::canonicalize(p).map_err(|error| error.to_string()))?;
        if workdir != root {
            return Err(
                "Git operation refused: selected workspace is not the repository worktree root"
                    .into(),
            );
        }
        Ok(Self { repo, root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn relative(&self, path: &Path, allow_deleted: bool) -> Result<PathBuf, String> {
        if path.as_os_str().is_empty() || path.is_absolute() {
            return Err("Git path must be a nonempty relative file path".into());
        }
        let mut candidate = self.root.clone();
        for component in path.components() {
            let Component::Normal(name) = component else {
                return Err("Git path traversal refused".into());
            };
            let name_str = name.to_string_lossy();
            if name_str == ".git" || name_str == ".worktrees" {
                return Err("Git internal metadata is protected".into());
            }
            candidate.push(name);
            match fs::symlink_metadata(&candidate) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err("Git path symlink/reparse point refused".into())
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && allow_deleted => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        if candidate.exists() {
            let canonical = dunce::canonicalize(&candidate).map_err(|e| e.to_string())?;
            if !canonical.starts_with(&self.root) || canonical != candidate {
                return Err("Git path escapes active workspace".into());
            }
            if !canonical.is_file() {
                return Err("Stage/unstage requires a regular file".into());
            }
        } else if !allow_deleted {
            return Err("File no longer exists".into());
        }
        Ok(path.to_path_buf())
    }

    pub fn stage_file(&self, path: &Path) -> Result<(), String> {
        let path = self.relative(path, true)?;
        let mut index = self.repo.index().map_err(|error| error.to_string())?;
        let existing = self.root.join(&path);
        if existing.exists() {
            index.add_path(&path).map_err(|error| error.to_string())?;
        } else if index.get_path(&path, 0).is_some() {
            index
                .remove_path(&path)
                .map_err(|error| error.to_string())?;
        } else {
            return Err("Cannot stage an untracked missing file".into());
        }
        index.write().map_err(|error| error.to_string())
    }

    pub fn unstage_file(&self, path: &Path) -> Result<(), String> {
        let path = self.relative(path, true)?;
        match self.repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit().map_err(|error| error.to_string())?;
                self.repo
                    .reset_default(Some(commit.as_object()), [&path])
                    .map_err(|error| error.to_string())
            }
            Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
                let mut index = self.repo.index().map_err(|error| error.to_string())?;
                if index.get_path(&path, 0).is_none() {
                    return Err("File is not staged".into());
                }
                index
                    .remove_path(&path)
                    .map_err(|error| error.to_string())?;
                index.write().map_err(|error| error.to_string())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn commit(&self, message: &str) -> Result<String, String> {
        let message = message.trim();
        if message.is_empty() || message.len() > 4096 || message.contains('\0') {
            return Err("Commit message must be nonempty, NUL-free and bounded".into());
        }
        let head = self.repo.head().map_err(|e| e.to_string())?;
        if !head.is_branch() {
            return Err("Committing from detached HEAD is disabled".into());
        }
        let parent = head.peel_to_commit().map_err(|error| error.to_string())?;
        let mut index = self.repo.index().map_err(|error| error.to_string())?;
        if index.has_conflicts() {
            return Err("Resolve Git index conflicts before committing".into());
        }
        let tree_oid = index.write_tree().map_err(|error| error.to_string())?;
        if tree_oid == parent.tree_id() {
            return Err("No staged changes to commit".into());
        }
        let tree = self
            .repo
            .find_tree(tree_oid)
            .map_err(|error| error.to_string())?;
        let author = self
            .repo
            .signature()
            .map_err(|_| "Configure Git user.name/user.email before committing".to_string())?;
        let oid = self
            .repo
            .commit(Some("HEAD"), &author, &author, message, &tree, &[&parent])
            .map_err(|error| error.to_string())?;
        Ok(oid.to_string())
    }

    pub fn switch_branch(&self, branch: &str) -> Result<(), String> {
        if !valid_branch(branch) {
            return Err("Invalid local branch name".into());
        }
        let mut options = StatusOptions::new();
        options.include_untracked(true).recurse_untracked_dirs(true);
        if !self
            .repo
            .statuses(Some(&mut options))
            .map_err(|e| e.to_string())?
            .is_empty()
        {
            return Err(
                "Save, commit, or stash working tree/index changes before switching branch".into(),
            );
        }
        let target = self
            .repo
            .find_branch(branch, BranchType::Local)
            .map_err(|error| error.to_string())?;
        let target = target
            .get()
            .peel_to_commit()
            .map_err(|error| error.to_string())?;
        let mut checkout = CheckoutBuilder::new();
        checkout.safe().recreate_missing(false);
        self.repo
            .checkout_tree(target.as_object(), Some(&mut checkout))
            .map_err(|error| error.to_string())?;
        self.repo
            .set_head(&format!("refs/heads/{branch}"))
            .map_err(|error| error.to_string())
    }

    /// Explicit user click only. Credentials are never harvested from GPUI;
    /// git2's configured transport handles the remote or reports an error.
    pub fn fetch_current_remote(&self) -> Result<(), String> {
        let remote_name = self
            .repo
            .find_remote("origin")
            .map(|_| "origin".to_string())
            .or_else(|_| {
                self.repo
                    .remotes()
                    .map_err(|e| e.to_string())
                    .and_then(|names| {
                        names
                            .iter()
                            .find_map(|name| name.ok().flatten())
                            .map(ToString::to_string)
                            .ok_or("No configured Git remote".to_string())
                    })
            })?;
        let mut remote = self
            .repo
            .find_remote(&remote_name)
            .map_err(|error| error.to_string())?;
        let refspecs: [&str; 0] = [];
        remote
            .fetch(&refspecs, None, Some("Alera GPUI explicit fetch"))
            .map_err(|error| error.to_string())
    }
}

fn valid_branch(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= 255
        && !branch.contains(['\0', '\n', '\r', ' ', '~', '^', ':', '?', '*', '[', '\\'])
        && !branch.contains("..")
        && !branch.contains("@{")
        && !branch.ends_with('.')
        && !branch.starts_with('/')
        && !branch.ends_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;

    fn repo_fixture() -> (tempfile::TempDir, GitActions) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        fs::write(dir.path().join("a.txt"), "before").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("Fixture", "fixture@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);
        let root = dunce::canonicalize(dir.path()).unwrap();
        let actions = GitActions::open(&root).unwrap();
        (dir, actions)
    }

    #[test]
    fn explicit_stage_unstage_commit_do_not_stage_unrelated_file() {
        let (_dir, actions) = repo_fixture();
        fs::write(actions.root().join("a.txt"), "after").unwrap();
        fs::write(actions.root().join("extra.txt"), "untracked").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        assert!(actions
            .repo
            .index()
            .unwrap()
            .get_path(Path::new("extra.txt"), 0)
            .is_none());
        actions.unstage_file(Path::new("a.txt")).unwrap();
        assert!(actions.commit("empty").is_err());
        actions.stage_file(Path::new("a.txt")).unwrap();
        assert!(!actions.commit("change").unwrap().is_empty());
        assert!(actions
            .repo
            .index()
            .unwrap()
            .get_path(Path::new("extra.txt"), 0)
            .is_none());
    }

    #[test]
    fn traversal_git_internals_and_wrong_workspace_are_refused() {
        let (_dir, actions) = repo_fixture();
        for path in [
            "../a.txt",
            ".git/config",
            "missing-file",
            "extra/../../a.txt",
        ] {
            assert!(actions.stage_file(Path::new(path)).is_err(), "{path}");
        }
        let child = actions.root().join("child");
        fs::create_dir(&child).unwrap();
        assert!(GitActions::open(&child).is_err());
        assert!(actions.switch_branch("missing").is_err());
        assert!(actions.commit(" ").is_err());
    }

    #[test]
    fn dirty_worktree_never_switches_branches() {
        let (_dir, actions) = repo_fixture();
        let head = actions.repo.head().unwrap().peel_to_commit().unwrap();
        actions.repo.branch("other", &head, false).unwrap();
        fs::write(actions.root().join("a.txt"), "dirty").unwrap();
        assert!(actions.switch_branch("other").is_err());
        assert_ne!(actions.repo.head().unwrap().shorthand().unwrap(), "other");
    }
}
