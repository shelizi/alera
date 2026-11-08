//! Explicit, worktree-scoped Git writers. The GPUI render path never calls
//! this module; a native click starts a bounded background operation.
//! No command invokes reset --hard, rebase, shell or force push. Merges use
//! explicit local git2 operations and refuse unresolved conflicts.
use git2::{build::CheckoutBuilder, BranchType, Repository, StatusOptions};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub struct GitActions {
    repo: Repository,
    root: PathBuf,
}

impl GitActions {
    fn require_clean_worktree(&self) -> Result<(), String> {
        let mut options = StatusOptions::new();
        options.include_untracked(true).recurse_untracked_dirs(true);
        if !self
            .repo
            .statuses(Some(&mut options))
            .map_err(|e| e.to_string())?
            .is_empty()
        {
            return Err(
                "Merge refused: working tree/index must be clean (including untracked files)"
                    .into(),
            );
        }
        if self
            .repo
            .index()
            .map_err(|e| e.to_string())?
            .has_conflicts()
        {
            return Err("Merge refused: index contains unresolved conflicts".into());
        }
        Ok(())
    }

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

    /// Move the *currently checked-out local branch* forward to another
    /// local branch. This is a real merge for fast-forward histories; it
    /// never creates a conflict index, overwrites dirty files, rebases,
    /// makes a force update, or changes the currently selected branch name.
    /// Divergent histories require a separate explicit conflict workflow.
    pub fn merge_fast_forward(&self, source: &str) -> Result<String, String> {
        if !valid_branch(source) {
            return Err("Enter a valid local source branch".into());
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
                "Merge refused: working tree/index must be clean (including untracked files)"
                    .into(),
            );
        }
        let head = self.repo.head().map_err(|e| e.to_string())?;
        if !head.is_branch() {
            return Err("Merge requires a checked-out local branch".into());
        }
        let name = head
            .shorthand()
            .map_err(|_| "Current local branch is unnamed")?
            .to_string();
        let current = head.peel_to_commit().map_err(|e| e.to_string())?;
        let target = self
            .repo
            .find_branch(source, BranchType::Local)
            .map_err(|e| e.to_string())?
            .get()
            .peel_to_commit()
            .map_err(|e| e.to_string())?;
        if target.id() == current.id() {
            return Ok(format!("{name} is already up to date with {source}"));
        }
        if !self
            .repo
            .graph_descendant_of(target.id(), current.id())
            .map_err(|e| e.to_string())?
        {
            return Err(format!("{source} has diverged from {name}; automatic fast-forward is unavailable. Use the existing conflict merge workflow."));
        }
        let mut checkout = CheckoutBuilder::new();
        checkout.safe().recreate_missing(false);
        self.repo
            .checkout_tree(target.as_object(), Some(&mut checkout))
            .map_err(|e| format!("Merge checkout refused: {e}"))?;
        // HEAD remains symbolic and attached to the same local branch.
        self.repo
            .find_reference(&format!("refs/heads/{name}"))
            .map_err(|e| e.to_string())?
            .set_target(target.id(), "Alera GPUI explicit fast-forward merge")
            .map_err(|e| format!("Could not advance branch ref after checkout: {e}"))?;
        Ok(format!(
            "Fast-forwarded {name} to {source} ({})",
            &target.id().to_string()[..8]
        ))
    }

    /// Merge a source local branch without opening a conflict index. Divergent
    /// histories are three-way merged in memory and committed only if the
    /// entire tree resolves automatically; dirty worktrees are not touched.
    pub fn merge_without_conflicts(&self, source: &str) -> Result<String, String> {
        if !valid_branch(source) {
            return Err("Enter a valid local source branch".into());
        }
        self.require_clean_worktree()?;
        let head = self.repo.head().map_err(|e| e.to_string())?;
        if !head.is_branch() {
            return Err("Merge requires a checked-out local branch".into());
        }
        let current_name = head.shorthand().map_err(|e| e.to_string())?.to_owned();
        let ours = head.peel_to_commit().map_err(|e| e.to_string())?;
        let theirs = self
            .repo
            .find_branch(source, BranchType::Local)
            .map_err(|e| e.to_string())?
            .get()
            .peel_to_commit()
            .map_err(|e| e.to_string())?;
        if ours.id() == theirs.id()
            || self
                .repo
                .graph_descendant_of(ours.id(), theirs.id())
                .map_err(|e| e.to_string())?
        {
            return Ok(format!("{current_name} already contains {source}"));
        }
        if self
            .repo
            .graph_descendant_of(theirs.id(), ours.id())
            .map_err(|e| e.to_string())?
        {
            drop(head);
            return self.merge_fast_forward(source);
        }
        let ancestor_id = self
            .repo
            .merge_base(ours.id(), theirs.id())
            .map_err(|_| "Merge refused: branches have no common ancestor".to_owned())?;
        let ancestor = self
            .repo
            .find_commit(ancestor_id)
            .map_err(|e| e.to_string())?
            .tree()
            .map_err(|e| e.to_string())?;
        let ours_tree = ours.tree().map_err(|e| e.to_string())?;
        let theirs_tree = theirs.tree().map_err(|e| e.to_string())?;
        let mut proposed = self
            .repo
            .merge_trees(&ancestor, &ours_tree, &theirs_tree, None)
            .map_err(|e| format!("Cannot calculate merge: {e}"))?;
        if proposed.has_conflicts() {
            return Err(format!("Merge refused: {source} conflicts with {current_name}. No files or refs were modified."));
        }
        let merged_tree_id = proposed
            .write_tree_to(&self.repo)
            .map_err(|e| e.to_string())?;
        let merged_tree = self
            .repo
            .find_tree(merged_tree_id)
            .map_err(|e| e.to_string())?;
        // Obtain author information before ever modifying the worktree.
        let identity = self.repo.signature().map_err(|_| {
            "Configure Git user.name/user.email before creating a merge commit".to_owned()
        })?;
        let mut preflight = CheckoutBuilder::new();
        preflight.safe().dry_run();
        self.repo
            .checkout_tree(merged_tree.as_object(), Some(&mut preflight))
            .map_err(|e| format!("Merge checkout preflight refused: {e}"))?;
        self.require_clean_worktree()?;
        let mut checkout = CheckoutBuilder::new();
        checkout.safe().recreate_missing(false);
        self.repo
            .checkout_tree(merged_tree.as_object(), Some(&mut checkout))
            .map_err(|e| format!("Merge checkout refused; branch ref unchanged: {e}"))?;
        let mut index = self.repo.index().map_err(|e| e.to_string())?;
        index.read_tree(&merged_tree).map_err(|e| e.to_string())?;
        index.write().map_err(|e| e.to_string())?;
        let message = format!("Merge branch '{source}' into {current_name}");
        let commit_id = self
            .repo
            .commit(
                Some("HEAD"),
                &identity,
                &identity,
                &message,
                &merged_tree,
                &[&ours, &theirs],
            )
            .map_err(|e| format!("Could not commit merge; merged changes remain staged: {e}"))?;
        Ok(format!(
            "Merged {source} into {current_name} ({})",
            &commit_id.to_string()[..8]
        ))
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
        // Keep disposable Windows test checkouts byte-for-byte LF; the
        // developer's global Git autocrlf must not contaminate fixtures.
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
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

    #[test]
    fn explicit_fast_forward_merge_advances_current_branch_without_switching() {
        let (dir, actions) = repo_fixture();
        let first = actions.repo.head().unwrap().peel_to_commit().unwrap();
        let initial_name = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        actions.repo.branch("feature", &first, false).unwrap();
        actions.repo.set_head("refs/heads/feature").unwrap();
        // Both local refs start at the same commit: there is no worktree
        // checkout needed before committing new fixture changes on feature.
        fs::write(dir.path().join("a.txt"), "feature changes").unwrap();
        let mut index = actions.repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        let tree = actions.repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = Signature::now("Fixture", "fixture@example.invalid").unwrap();
        let feature_oid = actions
            .repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "feature",
                &tree,
                &[&first],
            )
            .unwrap();
        drop(tree);
        drop(index);
        actions
            .repo
            .set_head(&format!("refs/heads/{initial_name}"))
            .unwrap();
        // Fixture-only reset after switching symbolic HEAD; unlike the
        // production merge action this disposable checkout may discard the
        // fixture's previous feature-branch index/workdir contents.
        actions
            .repo
            .checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "before"
        );
        let message = actions.merge_fast_forward("feature").unwrap();
        assert!(message.contains("Fast-forwarded"));
        assert_eq!(
            actions.repo.head().unwrap().shorthand().unwrap(),
            initial_name
        );
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            feature_oid
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "feature changes"
        );
        assert!(actions
            .merge_fast_forward("feature")
            .unwrap()
            .contains("already up to date"));
    }

    #[test]
    fn fast_forward_refuses_dirty_or_divergent_histories() {
        let (dir, actions) = repo_fixture();
        let initial = actions.repo.head().unwrap().peel_to_commit().unwrap();
        actions.repo.branch("feature", &initial, false).unwrap();
        fs::write(dir.path().join("untracked.txt"), "do not change").unwrap();
        assert!(actions.merge_fast_forward("feature").is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("untracked.txt")).unwrap(),
            "do not change"
        );
        fs::remove_file(dir.path().join("untracked.txt")).unwrap();
        // The source is behind HEAD after another commit; it cannot be
        // fast-forwarded into the new HEAD.
        fs::write(dir.path().join("a.txt"), "new main").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        actions.commit("main advanced").unwrap();
        let before = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        assert!(actions.merge_fast_forward("feature").is_err());
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            before
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "new main"
        );
    }

    fn fixture_commit(actions: &GitActions, file: &str, content: &str, message: &str) -> git2::Oid {
        fs::write(actions.root.join(file), content).unwrap();
        let mut index = actions.repo.index().unwrap();
        index.add_path(Path::new(file)).unwrap();
        let tree = actions.repo.find_tree(index.write_tree().unwrap()).unwrap();
        index.write().unwrap();
        let parent = actions.repo.head().unwrap().peel_to_commit().unwrap();
        let signature = Signature::now("Merge Fixture", "merge@example.invalid").unwrap();
        actions
            .repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[&parent],
            )
            .unwrap()
    }

    fn fixture_divergence(
        conflicting: bool,
    ) -> (tempfile::TempDir, GitActions, git2::Oid, git2::Oid) {
        let (dir, actions) = repo_fixture();
        let initial_name = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        let first = actions.repo.head().unwrap().peel_to_commit().unwrap();
        actions.repo.branch("feature", &first, false).unwrap();
        let ours = fixture_commit(
            &actions,
            if conflicting { "a.txt" } else { "ours.txt" },
            "ours\n",
            "ours change",
        );
        actions.repo.set_head("refs/heads/feature").unwrap();
        // Force checkout is fixture-only, to restore the other disposable tree.
        actions
            .repo
            .checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
        let mut index = actions.repo.index().unwrap();
        index.read_tree(&first.tree().unwrap()).unwrap();
        index.write().unwrap();
        drop(index);
        let theirs = fixture_commit(
            &actions,
            if conflicting { "a.txt" } else { "theirs.txt" },
            "theirs\n",
            "theirs change",
        );
        actions
            .repo
            .set_head(&format!("refs/heads/{initial_name}"))
            .unwrap();
        actions
            .repo
            .checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
        let ours_tree = actions.repo.find_commit(ours).unwrap().tree().unwrap();
        let mut index = actions.repo.index().unwrap();
        index.read_tree(&ours_tree).unwrap();
        index.write().unwrap();
        drop(index);
        drop(ours_tree);
        drop(first);
        (dir, actions, ours, theirs)
    }

    #[test]
    fn non_fast_forward_without_conflicts_creates_two_parent_commit_and_clean_tree() {
        let (dir, actions, ours, theirs) = fixture_divergence(false);
        let name = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        let result = actions
            .merge_without_conflicts("feature")
            .unwrap_or_else(|error| {
                let statuses = actions.repo.statuses(None).unwrap();
                panic!(
                    "{error}; fixture statuses: {:?}",
                    statuses
                        .iter()
                        .map(|s| (s.path().map(str::to_owned), s.status()))
                        .collect::<Vec<_>>()
                );
            });
        assert!(result.contains("Merged"), "{result}");
        let merged = actions.repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(merged.parent_count(), 2);
        assert_eq!(merged.parent_id(0).unwrap(), ours);
        assert_eq!(merged.parent_id(1).unwrap(), theirs);
        assert_eq!(actions.repo.head().unwrap().shorthand().unwrap(), name);
        assert_eq!(
            fs::read_to_string(dir.path().join("ours.txt")).unwrap(),
            "ours\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("theirs.txt")).unwrap(),
            "theirs\n"
        );
        assert!(actions.repo.statuses(None).unwrap().is_empty());
    }

    #[test]
    fn divergent_conflicts_do_not_modify_index_worktree_or_branch() {
        let (dir, actions, ours, theirs) = fixture_divergence(true);
        let index_tree = actions.repo.index().unwrap().write_tree().unwrap();
        let error = actions.merge_without_conflicts("feature").unwrap_err();
        assert!(error.contains("conflict"), "{error}");
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            ours
        );
        assert_eq!(
            actions
                .repo
                .find_branch("feature", BranchType::Local)
                .unwrap()
                .get()
                .peel_to_commit()
                .unwrap()
                .id(),
            theirs
        );
        assert_eq!(
            actions.repo.index().unwrap().write_tree().unwrap(),
            index_tree
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "ours\n"
        );
        assert!(actions.repo.statuses(None).unwrap().is_empty());
    }

    #[test]
    fn divergent_merge_rejects_untracked_files_without_modifying_them() {
        let (dir, actions, ours, _) = fixture_divergence(false);
        fs::write(dir.path().join("private-notes.txt"), "leave me").unwrap();
        assert!(actions.merge_without_conflicts("feature").is_err());
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            ours
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("private-notes.txt")).unwrap(),
            "leave me"
        );
        assert!(!dir.path().join("theirs.txt").exists());
    }
}
