//! Explicit, worktree-scoped Git writers. The GPUI render path never calls
//! this module; a native click starts a bounded background operation.
//! No command invokes reset --hard, rebase, shell or force push. Merges use
//! explicit local git2 operations and refuse unresolved conflicts.
use alera_core::git_cli::git_in_dir;
use git2::{build::CheckoutBuilder, BranchType, Index, Oid, Repository, StatusOptions};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Whole-file choices for explicitly confirmed, text-only merge conflicts.
/// This intentionally does not pretend to perform per-hunk/AI resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitConflictChoice {
    Ours,
    Theirs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitConflictFile {
    pub path: String,
    pub ancestor_preview: Option<String>,
    pub ours_preview: String,
    pub theirs_preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitConflictPlan {
    pub current_branch: String,
    pub source_branch: String,
    pub current_oid: String,
    pub source_oid: String,
    pub conflicts: Vec<GitConflictFile>,
}

const MAX_MERGE_CONFLICT_FILES: usize = 48;
const MAX_MERGE_CONFLICT_TEXT_BYTES: usize = 64 * 1024;

pub struct GitActions {
    repo: Repository,
    root: PathBuf,
}

impl GitActions {
    fn conflicting_merge_index(&self, source: &str) -> Result<(String, Oid, Oid, Index), String> {
        if !valid_branch(source) {
            return Err("Enter a valid local source branch".into());
        }
        self.require_clean_worktree()?;
        let head = self.repo.head().map_err(|error| error.to_string())?;
        if !head.is_branch() {
            return Err("Conflict resolution requires a checked-out local branch".into());
        }
        let current_name = head
            .shorthand()
            .map_err(|error| error.to_string())?
            .to_owned();
        let ours = head.peel_to_commit().map_err(|error| error.to_string())?;
        let theirs = self
            .repo
            .find_branch(source, BranchType::Local)
            .map_err(|error| error.to_string())?
            .get()
            .peel_to_commit()
            .map_err(|error| error.to_string())?;
        if ours.id() == theirs.id()
            || self
                .repo
                .graph_descendant_of(ours.id(), theirs.id())
                .map_err(|error| error.to_string())?
            || self
                .repo
                .graph_descendant_of(theirs.id(), ours.id())
                .map_err(|error| error.to_string())?
        {
            return Err("Branches are not diverged; use the regular Merge action".into());
        }
        let ancestor_id = self
            .repo
            .merge_base(ours.id(), theirs.id())
            .map_err(|_| "Merge refused: branches have no common ancestor".to_string())?;
        let ancestor = self
            .repo
            .find_commit(ancestor_id)
            .and_then(|commit| commit.tree())
            .map_err(|error| error.to_string())?;
        let ours_tree = ours.tree().map_err(|error| error.to_string())?;
        let theirs_tree = theirs.tree().map_err(|error| error.to_string())?;
        let index = self
            .repo
            .merge_trees(&ancestor, &ours_tree, &theirs_tree, None)
            .map_err(|error| error.to_string())?;
        if !index.has_conflicts() {
            return Err("Merge has no unresolved conflicts; use regular Merge".into());
        }
        Ok((current_name, ours.id(), theirs.id(), index))
    }

    fn conflict_stage_text(&self, stage: &git2::IndexEntry) -> Result<String, String> {
        if !matches!(stage.mode, 0o100644 | 0o100755) {
            return Err(
                "Symlink, submodule and non-regular merge conflicts require a dedicated merge tool"
                    .into(),
            );
        }
        let blob = self
            .repo
            .find_blob(stage.id)
            .map_err(|error| error.to_string())?;
        let bytes = blob.content();
        if bytes.len() > MAX_MERGE_CONFLICT_TEXT_BYTES || bytes.contains(&0) {
            return Err("Binary or large merge conflicts require a dedicated merge tool".into());
        }
        std::str::from_utf8(bytes)
            .map(ToString::to_string)
            .map_err(|_| "Non-UTF-8 merge conflicts require a dedicated merge tool".into())
    }

    /// No checkout, index writes, refs or file mutations. A per-file plan
    /// includes *complete* UTF-8 sides (bounded to 64 KiB per stage) so a
    /// user can compare them before selecting whole-file Ours or Theirs.
    pub fn preview_merge_conflicts(&self, source: &str) -> Result<GitConflictPlan, String> {
        let (current_branch, current_oid, source_oid, index) =
            self.conflicting_merge_index(source)?;
        let mut conflicts = Vec::new();
        for item in index.conflicts().map_err(|error| error.to_string())? {
            if conflicts.len() >= MAX_MERGE_CONFLICT_FILES {
                return Err("Too many merge conflict files for Native preview".into());
            }
            let item = item.map_err(|error| error.to_string())?;
            let (Some(ours), Some(theirs)) = (item.our, item.their) else {
                return Err(
                    "Delete/modify or add/delete conflicts require a dedicated merge tool".into(),
                );
            };
            if ours.path != theirs.path
                || item
                    .ancestor
                    .as_ref()
                    .is_some_and(|entry| entry.path != ours.path)
            {
                return Err("Rename/path merge conflicts require a dedicated merge tool".into());
            }
            let path = String::from_utf8(ours.path.clone())
                .map_err(|_| "Non-UTF-8 Git conflict paths are not supported".to_string())?;
            self.relative(Path::new(&path), true)?;
            let ancestor_preview = item
                .ancestor
                .as_ref()
                .map(|stage| self.conflict_stage_text(stage))
                .transpose()?;
            conflicts.push(GitConflictFile {
                path,
                ancestor_preview,
                ours_preview: self.conflict_stage_text(&ours)?,
                theirs_preview: self.conflict_stage_text(&theirs)?,
            });
        }
        conflicts.sort_by(|a, b| a.path.cmp(&b.path));
        if conflicts.is_empty() {
            return Err("No supported conflicts found".into());
        }
        Ok(GitConflictPlan {
            current_branch,
            source_branch: source.to_string(),
            current_oid: current_oid.to_string(),
            source_oid: source_oid.to_string(),
            conflicts,
        })
    }

    /// Apply only an exact, still-current preview. Every conflicting path
    /// must have one explicit choice, and all nonconflicting merge changes
    /// remain generated by Git's own three-way merge. This never produces
    /// conflict markers or mutates the user's files before checkout preflight.
    pub fn merge_with_conflict_choices(
        &self,
        preview: &GitConflictPlan,
        choices: &[(String, GitConflictChoice)],
    ) -> Result<String, String> {
        let fresh = self.preview_merge_conflicts(&preview.source_branch)?;
        if &fresh != preview {
            return Err(
                "Merge preview is stale; refresh the conflict list before continuing".into(),
            );
        }
        if choices.len() != preview.conflicts.len() {
            return Err("Choose exactly one side for every conflicting file".into());
        }
        let mut selection = HashMap::new();
        for (path, choice) in choices {
            if !preview.conflicts.iter().any(|file| file.path == *path)
                || selection.insert(path.as_str(), *choice).is_some()
            {
                return Err("Conflict choices contain unknown or duplicate file paths".into());
            }
        }
        let (branch, current_oid, source_oid, mut index) =
            self.conflicting_merge_index(&preview.source_branch)?;
        if branch != preview.current_branch
            || current_oid.to_string() != preview.current_oid
            || source_oid.to_string() != preview.source_oid
        {
            return Err("Merge refs changed after preview; refresh before continuing".into());
        }
        let mut entries = Vec::new();
        for item in index.conflicts().map_err(|error| error.to_string())? {
            let item = item.map_err(|error| error.to_string())?;
            let (Some(ours), Some(theirs)) = (item.our, item.their) else {
                return Err("Conflict type changed after preview".into());
            };
            let path = String::from_utf8(ours.path.clone())
                .map_err(|_| "Invalid Git conflict file name".to_string())?;
            let side = *selection
                .get(path.as_str())
                .ok_or("Missing Git conflict resolution choice")?;
            let mut chosen = match side {
                GitConflictChoice::Ours => ours,
                GitConflictChoice::Theirs => theirs,
            };
            // Git stages live in bits 12-13 of IndexEntry.flags; a resolved
            // stage-zero entry must not retain those conflict stage bits.
            chosen.flags &= !0x3000;
            entries.push((path, chosen));
        }
        for (path, entry) in entries {
            index
                .conflict_remove(Path::new(&path))
                .map_err(|error| error.to_string())?;
            index.add(&entry).map_err(|error| error.to_string())?;
        }
        if index.has_conflicts() {
            return Err("Refusing to commit: Git index still contains unresolved conflicts".into());
        }
        let tree_oid = index
            .write_tree_to(&self.repo)
            .map_err(|error| error.to_string())?;
        let merged_tree = self
            .repo
            .find_tree(tree_oid)
            .map_err(|error| error.to_string())?;
        let signature = self
            .repo
            .signature()
            .map_err(|_| "Configure Git user.name/user.email before merging".to_string())?;
        let ours = self
            .repo
            .find_commit(current_oid)
            .map_err(|error| error.to_string())?;
        let theirs = self
            .repo
            .find_commit(source_oid)
            .map_err(|error| error.to_string())?;
        let mut preflight = CheckoutBuilder::new();
        preflight.safe().dry_run();
        self.repo
            .checkout_tree(merged_tree.as_object(), Some(&mut preflight))
            .map_err(|error| format!("Merge checkout preflight refused: {error}"))?;
        self.require_clean_worktree()?;
        if self
            .repo
            .head()
            .map_err(|error| error.to_string())?
            .peel_to_commit()
            .map_err(|error| error.to_string())?
            .id()
            != current_oid
        {
            return Err("Local branch advanced during merge; no files were written".into());
        }
        let mut checkout = CheckoutBuilder::new();
        checkout.safe().recreate_missing(false);
        self.repo
            .checkout_tree(merged_tree.as_object(), Some(&mut checkout))
            .map_err(|error| format!("Merge checkout refused; branch ref unchanged: {error}"))?;
        let mut worktree_index = self.repo.index().map_err(|error| error.to_string())?;
        worktree_index
            .read_tree(&merged_tree)
            .map_err(|error| error.to_string())?;
        worktree_index.write().map_err(|error| error.to_string())?;
        let message = format!(
            "Merge branch '{}' into {} (explicit conflict choices)",
            preview.source_branch, branch
        );
        let merge_oid = self
            .repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                &message,
                &merged_tree,
                &[&ours, &theirs],
            )
            .map_err(|error| {
                format!("Merge commit failed; resolved changes remain staged: {error}")
            })?;
        Ok(format!(
            "Merged {} into {} with {} chosen file side(s) ({})",
            preview.source_branch,
            branch,
            choices.len(),
            &merge_oid.to_string()[..8],
        ))
    }

    /// Only local changes already tracked by Git are stashed. New, untracked
    /// files remain in place; hiding them without explicit consent is unsafe.
    /// This never drops an older stash entry.
    pub fn stash_tracked_changes(&mut self) -> Result<String, String> {
        if self
            .repo
            .index()
            .map_err(|error| error.to_string())?
            .has_conflicts()
        {
            return Err("Resolve index conflicts before creating a stash".into());
        }
        let mut options = StatusOptions::new();
        options.include_untracked(false);
        let changed = self
            .repo
            .statuses(Some(&mut options))
            .map_err(|error| error.to_string())?
            .iter()
            .any(|entry| {
                let status = entry.status();
                !status.is_empty()
                    && status != git2::Status::WT_NEW
                    && status != git2::Status::IGNORED
            });
        if !changed {
            return Err("No tracked changes to stash; untracked files are left untouched".into());
        }
        let signature = self.repo.signature().map_err(|_| {
            "Configure Git user.name/user.email before creating a stash".to_string()
        })?;
        let oid = self
            .repo
            .stash_save(&signature, "Alera Native tracked changes", None)
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "Saved tracked changes to stash {} (untracked files kept)",
            &oid.to_string()[..8]
        ))
    }

    /// Apply without consuming the stash. If conflicts arise libgit2 may
    /// leave conflict markers/index stages; the user's stash stays recoverable.
    pub fn apply_latest_stash(&mut self) -> Result<String, String> {
        self.require_clean_worktree()?;
        self.repo
            .stash_apply(0, None)
            .map_err(|error| format!("Could not apply latest stash; stash retained: {error}"))?;
        Ok("Applied latest stash; original stash retained until explicitly dropped".into())
    }

    /// Resolve only the current local branch's exact configured upstream.
    /// Arbitrary refspec, upstream-branch inference or force flags supplied
    /// through the UI are never accepted.
    fn current_tracking(&self) -> Result<(String, String, String), String> {
        let head = self.repo.head().map_err(|error| error.to_string())?;
        if !head.is_branch() {
            return Err("Pull/Push requires a checked-out local branch".into());
        }
        let current = head
            .shorthand()
            .map_err(|_| "Current Git branch name unavailable")?;
        if !valid_branch(current) {
            return Err("Current Git branch name is unsafe".into());
        }
        let branch = self
            .repo
            .find_branch(current, BranchType::Local)
            .map_err(|error| error.to_string())?;
        let upstream = branch.upstream().map_err(|_| {
            "Current branch has no configured upstream; publish/configure it in Git first"
                .to_string()
        })?;
        let config = self.repo.config().map_err(|error| error.to_string())?;
        let remote = config
            .get_string(&format!("branch.{current}.remote"))
            .map_err(|_| "Current branch has no configured remote".to_string())?;
        let target = config
            .get_string(&format!("branch.{current}.merge"))
            .map_err(|_| "Current branch has no configured upstream ref".to_string())?;
        let remote_branch = target
            .strip_prefix("refs/heads/")
            .ok_or("Upstream is not a remote branch")?;
        if remote.is_empty()
            || !remote
                .bytes()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphanumeric())
            || !remote
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_' | b'.'))
            || !valid_branch(remote_branch)
        {
            return Err("Remote or upstream branch name is unsafe".into());
        }
        let expected = format!("refs/remotes/{remote}/{remote_branch}");
        if upstream.get().name().map_err(|error| error.to_string())? != expected {
            return Err("Custom upstream ref mapping is unsupported for Native Pull/Push".into());
        }
        self.repo
            .find_remote(&remote)
            .map_err(|_| "Configured Git remote unavailable".to_string())?;
        Ok((current.to_owned(), remote, remote_branch.to_owned()))
    }

    /// Safe default: git CLI uses the user's existing credential helper
    /// (libgit2 was built without HTTPS/SSH), only --ff-only is allowed, and
    /// worktree/index/untracked files must be clean first.
    pub fn pull_ff_only(&self) -> Result<String, String> {
        let (current, remote, upstream) = self.current_tracking()?;
        self.require_clean_worktree()?;
        let refspec = format!("refs/heads/{upstream}");
        git_in_dir(
            &self.root,
            &["pull", "--ff-only", "--no-rebase", "--", &remote, &refspec],
        )
        .map_err(|_| "Fast-forward Pull failed; check upstream access and branch divergence. Inspect Git status before retrying".to_string())?;
        Ok(format!(
            "Pulled {remote}/{upstream} into {current} (fast-forward only)"
        ))
    }

    /// Never force-push and never publish to an arbitrary user-supplied
    /// destination. Reject known divergence locally; the server additionally
    /// rejects any race where remote history changed after our last Fetch.
    pub fn push_current_upstream(&self) -> Result<String, String> {
        let (current, remote, upstream) = self.current_tracking()?;
        let head = self
            .repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|error| error.to_string())?;
        let tracking = self
            .repo
            .find_reference(&format!("refs/remotes/{remote}/{upstream}"))
            .and_then(|reference| reference.peel_to_commit())
            .map_err(|_| "Upstream tracking ref unavailable; Fetch before Push".to_string())?;
        if head.id() != tracking.id()
            && !self
                .repo
                .graph_descendant_of(head.id(), tracking.id())
                .map_err(|error| error.to_string())?
        {
            return Err(
                "Push refused: local branch diverged from its last fetched upstream".into(),
            );
        }
        let refspec = format!("refs/heads/{current}:refs/heads/{upstream}");
        git_in_dir(
            &self.root,
            &["push", "--porcelain", "--", &remote, &refspec],
        )
        .map_err(|_| "Normal Push failed; no force attempted. Check upstream status, server permissions and Git credential helper".to_string())?;
        Ok(format!(
            "Pushed {current} to {remote}/{upstream} without force"
        ))
    }

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

    /// Validate the entire selection before modifying the index. A failure
    /// must not leave a partially staged batch.
    pub fn stage_files(&self, paths: &[PathBuf]) -> Result<usize, String> {
        if paths.is_empty() || paths.len() > 120 {
            return Err("Select between 1 and 120 files to stage".into());
        }
        let mut selected = std::collections::BTreeSet::new();
        for path in paths {
            selected.insert(self.relative(path, true)?);
        }
        let mut index = self.repo.index().map_err(|error| error.to_string())?;
        for path in &selected {
            let file = self.root.join(path);
            if file.exists() {
                index.add_path(path).map_err(|error| error.to_string())?;
            } else if index.get_path(path, 0).is_some() {
                index.remove_path(path).map_err(|error| error.to_string())?;
            } else {
                return Err(format!("Cannot stage missing file {}", path.display()));
            }
        }
        index.write().map_err(|error| error.to_string())?;
        Ok(selected.len())
    }

    /// Git resets selected index paths only; working files are preserved.
    pub fn unstage_files(&self, paths: &[PathBuf]) -> Result<usize, String> {
        if paths.is_empty() || paths.len() > 120 {
            return Err("Select between 1 and 120 files to unstage".into());
        }
        let mut selected = std::collections::BTreeSet::new();
        for path in paths {
            selected.insert(self.relative(path, true)?);
        }
        let index = self.repo.index().map_err(|error| error.to_string())?;
        if selected
            .iter()
            .any(|path| index.get_path(path, 0).is_none())
        {
            return Err("Selected file is not staged; refresh the Git changes list".into());
        }
        drop(index);
        match self.repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit().map_err(|error| error.to_string())?;
                self.repo
                    .reset_default(Some(commit.as_object()), &selected)
                    .map_err(|error| error.to_string())?;
            }
            Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
                let mut index = self.repo.index().map_err(|error| error.to_string())?;
                for path in &selected {
                    index.remove_path(path).map_err(|error| error.to_string())?;
                }
                index.write().map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
        Ok(selected.len())
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
        // This guard must run in the Git worker, not only in the GPUI
        // selection handler: worktrees can change after a menu is displayed.
        let checkout = crate::worktree_manager::WorktreeManager::discover(&self.root)
            .map_err(|error| format!("Cannot inspect Git worktrees: {error}"))?;
        let worktrees = checkout
            .list()
            .map_err(|error| format!("Cannot inspect Git worktrees: {error}"))?;
        if worktrees
            .iter()
            .any(|entry| entry.branch.as_deref() == Some(branch) && entry.path != self.root)
        {
            return Err(format!(
                "Branch {branch} is already checked out in another worktree; open its workspace instead"
            ));
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

    /// Fetch through the system Git credential helper (libgit2 in this
    /// workspace has HTTPS/SSH disabled). Auto Fetch is noninteractive; users
    /// must authenticate through their normal Git tooling if required.
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
        // -- separates even a remote whose configured name resembles an
        // option. Do not pass Git stderr to the UI: URLs may embed secrets.
        git_in_dir(
            &self.root,
            &[
                "-c",
                "credential.interactive=never",
                "fetch",
                "--",
                &remote_name,
            ],
        )
        .map(|_| ())
        .map_err(|_| {
            "Git Fetch failed; check configured remote and noninteractive credentials".into()
        })
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
        index.write().unwrap();
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
    fn batch_staging_validates_all_paths_before_writing_index() {
        let (_dir, actions) = repo_fixture();
        fs::write(actions.root().join("a.txt"), "changed").unwrap();
        fs::write(actions.root().join("new.txt"), "new").unwrap();
        assert!(actions
            .stage_files(&[PathBuf::from("a.txt"), PathBuf::from("../outside")])
            .is_err());
        let index = actions.repo.index().unwrap();
        let initial = index.get_path(Path::new("a.txt"), 0).unwrap().id;
        let head = actions.repo.head().unwrap().peel_to_tree().unwrap();
        assert_eq!(initial, head.get_path(Path::new("a.txt")).unwrap().id());
        drop(index);
        assert_eq!(
            actions
                .stage_files(&[
                    PathBuf::from("a.txt"),
                    PathBuf::from("new.txt"),
                    PathBuf::from("a.txt")
                ])
                .unwrap(),
            2
        );
        let index = actions.repo.index().unwrap();
        assert!(index.get_path(Path::new("new.txt"), 0).is_some());
        drop(index);
        assert_eq!(
            actions
                .unstage_files(&[PathBuf::from("a.txt"), PathBuf::from("new.txt")])
                .unwrap(),
            2
        );
        assert!(actions
            .repo
            .index()
            .unwrap()
            .get_path(Path::new("new.txt"), 0)
            .is_none());
        assert_eq!(
            fs::read_to_string(actions.root().join("a.txt")).unwrap(),
            "changed"
        );
        assert_eq!(
            fs::read_to_string(actions.root().join("new.txt")).unwrap(),
            "new"
        );
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
    fn branch_switch_cannot_checkout_branch_owned_by_linked_worktree() {
        let (_dir, actions) = repo_fixture();
        let manager = crate::worktree_manager::WorktreeManager::discover(actions.root()).unwrap();
        let linked = manager.create("occupied").unwrap();
        let initial = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        let failure = actions.switch_branch("occupied").unwrap_err();
        assert!(failure.contains("another worktree"), "{failure}");
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            initial
        );
        assert!(linked.exists());
        manager.remove(&linked, true).unwrap();
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

    #[test]
    fn stash_save_and_apply_preserve_untracked_files_and_stash_identity() {
        let (dir, mut actions) = repo_fixture();
        let before = actions.repo.head().unwrap().target().unwrap();
        let original = fs::read(dir.path().join("a.txt")).unwrap();
        fs::write(dir.path().join("a.txt"), "unsaved tracked edits").unwrap();
        fs::write(dir.path().join("untracked.txt"), "personal untracked").unwrap();
        assert!(actions
            .stash_tracked_changes()
            .unwrap()
            .contains("Saved tracked"));
        assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), original);
        assert_eq!(
            fs::read_to_string(dir.path().join("untracked.txt")).unwrap(),
            "personal untracked"
        );
        // Failed application must not remove the stash or touch an untracked
        // file the user created after saving their tracked changes.
        assert!(actions.apply_latest_stash().is_err());
        fs::remove_file(dir.path().join("untracked.txt")).unwrap();
        assert!(actions.apply_latest_stash().unwrap().contains("retained"));
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "unsaved tracked edits"
        );
        let mut entries = Vec::new();
        actions
            .repo
            .stash_foreach(|index, _message, oid| {
                entries.push((index, *oid));
                true
            })
            .unwrap();
        assert_eq!(entries.len(), 1, "Apply must never silently pop the stash");
        assert!(actions.apply_latest_stash().is_err());
        assert_eq!(actions.repo.head().unwrap().target().unwrap(), before);
        assert!(actions.stash_tracked_changes().is_ok());
    }

    #[test]
    fn stash_refuses_missing_tracked_changes_and_keeps_untracked_files() {
        let (dir, mut actions) = repo_fixture();
        fs::write(dir.path().join("untracked.txt"), "only untracked").unwrap();
        assert!(actions
            .stash_tracked_changes()
            .unwrap_err()
            .contains("No tracked"));
        assert!(actions.apply_latest_stash().is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("untracked.txt")).unwrap(),
            "only untracked"
        );
    }

    fn bare_remote_fixture() -> (tempfile::TempDir, tempfile::TempDir, GitActions, String) {
        let (work, actions) = repo_fixture();
        let remote = tempfile::tempdir().unwrap();
        let bare = Repository::init_bare(remote.path()).unwrap();
        let current = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        bare.set_head(&format!("refs/heads/{current}")).unwrap();
        let remote_path = remote.path().to_str().unwrap();
        git_in_dir(actions.root(), &["remote", "add", "origin", remote_path]).unwrap();
        let refspec = format!("refs/heads/{current}");
        git_in_dir(actions.root(), &["push", "-u", "origin", &refspec]).unwrap();
        assert_eq!(
            actions.current_tracking().unwrap(),
            (current.clone(), "origin".into(), current.clone())
        );
        (work, remote, actions, current)
    }

    #[test]
    fn explicit_push_and_ff_pull_exchange_commits_through_configured_local_remote() {
        let (work, remote, actions, branch) = bare_remote_fixture();
        let first = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        actions
            .fetch_current_remote()
            .expect("CLI Fetch works with configured local remote");
        let ours = fixture_commit(&actions, "a.txt", "local update", "local new");
        assert!(actions
            .push_current_upstream()
            .unwrap()
            .contains("without force"));
        assert_eq!(
            Repository::open_bare(remote.path())
                .unwrap()
                .find_reference(&format!("refs/heads/{branch}"))
                .unwrap()
                .target()
                .unwrap(),
            ours
        );
        assert_ne!(first, ours);
        let second = tempfile::tempdir().unwrap();
        let clone = second.path().join("clone");
        git_in_dir(
            second.path(),
            &[
                "clone",
                remote.path().to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        )
        .unwrap();
        let clone_actions = GitActions::open(&dunce::canonicalize(&clone).unwrap()).unwrap();
        let remote_tip = fixture_commit(&clone_actions, "a.txt", "remote update", "remote new");
        let refspec = format!("refs/heads/{branch}");
        git_in_dir(&clone, &["push", "origin", &refspec]).unwrap();
        assert!(actions.pull_ff_only().unwrap().contains("fast-forward"));
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            remote_tip
        );
        assert_eq!(
            fs::read_to_string(work.path().join("a.txt")).unwrap(),
            "remote update"
        );
        assert!(actions.repo.statuses(None).unwrap().is_empty());
    }

    #[test]
    fn pull_never_merges_divergence_or_overwrites_dirty_workspace() {
        let (work, remote, actions, branch) = bare_remote_fixture();
        let second = tempfile::tempdir().unwrap();
        let clone = second.path().join("clone");
        git_in_dir(
            second.path(),
            &[
                "clone",
                remote.path().to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        )
        .unwrap();
        let clone_actions = GitActions::open(&dunce::canonicalize(&clone).unwrap()).unwrap();
        fixture_commit(
            &clone_actions,
            "remote.txt",
            "remote change",
            "upstream diverges",
        );
        let refspec = format!("refs/heads/{branch}");
        git_in_dir(&clone, &["push", "origin", &refspec]).unwrap();
        let ours = fixture_commit(&actions, "a.txt", "local divergence", "local diverges");
        assert!(
            actions.pull_ff_only().is_err(),
            "Diverged histories must not auto merge"
        );
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            ours
        );
        assert_eq!(
            fs::read_to_string(work.path().join("a.txt")).unwrap(),
            "local divergence"
        );
        assert!(!work.path().join("remote.txt").exists());
        assert!(
            actions.push_current_upstream().is_err(),
            "Non-FF push must be rejected"
        );
        fs::write(work.path().join("untracked.txt"), "private").unwrap();
        assert!(
            actions.pull_ff_only().is_err(),
            "Untracked files block Pull"
        );
        assert_eq!(
            fs::read_to_string(work.path().join("untracked.txt")).unwrap(),
            "private"
        );
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            ours
        );
    }

    #[test]
    fn upstream_only_remote_actions_refuse_unpublished_branches() {
        let (dir, actions) = repo_fixture();
        let head = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        assert!(actions.pull_ff_only().is_err());
        assert!(actions.push_current_upstream().is_err());
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            head
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "before"
        );
    }

    #[test]
    fn conflict_preview_is_read_only_and_exposes_two_text_sides() {
        let (dir, actions, ours, theirs) = fixture_divergence(true);
        let index_before = actions.repo.index().unwrap().write_tree().unwrap();
        let plan = actions.preview_merge_conflicts("feature").unwrap();
        assert_eq!(plan.current_oid, ours.to_string());
        assert_eq!(plan.source_oid, theirs.to_string());
        assert_eq!(plan.source_branch, "feature");
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].path, "a.txt");
        assert_eq!(plan.conflicts[0].ours_preview, "ours\n");
        assert_eq!(plan.conflicts[0].theirs_preview, "theirs\n");
        assert_eq!(
            actions.repo.index().unwrap().write_tree().unwrap(),
            index_before
        );
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            ours
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "ours\n"
        );
        assert!(actions.repo.statuses(None).unwrap().is_empty());
    }

    #[test]
    fn conflict_choice_commits_two_parents_without_losing_unrelated_files() {
        for (choice, content) in [
            (GitConflictChoice::Ours, "ours\n"),
            (GitConflictChoice::Theirs, "theirs\n"),
        ] {
            let (dir, actions, ours, theirs) = fixture_divergence(true);
            let original_name = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
            let preview = actions.preview_merge_conflicts("feature").unwrap();
            let result = actions
                .merge_with_conflict_choices(&preview, &[("a.txt".into(), choice)])
                .unwrap();
            assert!(result.contains("Merged"), "{result}");
            let merged = actions.repo.head().unwrap().peel_to_commit().unwrap();
            assert_eq!(merged.parent_count(), 2);
            assert_eq!(merged.parent_id(0).unwrap(), ours);
            assert_eq!(merged.parent_id(1).unwrap(), theirs);
            assert_eq!(
                actions.repo.head().unwrap().shorthand().unwrap(),
                original_name
            );
            assert_eq!(
                fs::read_to_string(dir.path().join("a.txt")).unwrap(),
                content
            );
            assert!(actions.repo.statuses(None).unwrap().is_empty());
            assert!(!actions.repo.index().unwrap().has_conflicts());
        }
    }

    #[test]
    fn conflict_resolution_requires_exact_complete_choices_and_fresh_refs() {
        let (dir, actions, ours, _) = fixture_divergence(true);
        let plan = actions.preview_merge_conflicts("feature").unwrap();
        for choices in [
            vec![],
            vec![("wrong.txt".into(), GitConflictChoice::Ours)],
            vec![
                ("a.txt".into(), GitConflictChoice::Ours),
                ("a.txt".into(), GitConflictChoice::Theirs),
            ],
        ] {
            assert!(actions
                .merge_with_conflict_choices(&plan, &choices)
                .is_err());
            assert_eq!(
                actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
                ours
            );
        }
        fs::write(dir.path().join("untracked-notes.txt"), "private").unwrap();
        assert!(actions
            .merge_with_conflict_choices(&plan, &[("a.txt".into(), GitConflictChoice::Ours)])
            .is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("untracked-notes.txt")).unwrap(),
            "private"
        );
        fs::remove_file(dir.path().join("untracked-notes.txt")).unwrap();
        // The UI could have kept an older plan while another agent advanced
        // the source branch. Never apply a stale selection to a new commit.
        let original = actions.repo.head().unwrap().peel_to_commit().unwrap();
        actions
            .repo
            .branch("feature-updated", &original, false)
            .unwrap();
        let mut feature_branch = actions.repo.find_reference("refs/heads/feature").unwrap();
        feature_branch
            .set_target(ours, "fixture: ref changed after preview")
            .unwrap();
        assert!(actions
            .merge_with_conflict_choices(&plan, &[("a.txt".into(), GitConflictChoice::Ours)])
            .is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "ours\n"
        );
        assert!(actions.repo.statuses(None).unwrap().is_empty());
    }

    #[test]
    fn chosen_conflict_side_keeps_automatic_nonconflicting_source_additions() {
        let (dir, actions, ours, theirs) = fixture_divergence(true);
        let current = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        actions.repo.set_head("refs/heads/feature").unwrap();
        // Fixture-only force checkout while switching to the disposable
        // source branch, then restore its committed index before editing.
        actions
            .repo
            .checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
        let theirs_tree = actions.repo.find_commit(theirs).unwrap().tree().unwrap();
        let mut index = actions.repo.index().unwrap();
        index.read_tree(&theirs_tree).unwrap();
        index.write().unwrap();
        drop(index);
        let new_theirs = fixture_commit(
            &actions,
            "source-only.txt",
            "source auto-merged content\n",
            "source file addition",
        );
        actions
            .repo
            .set_head(&format!("refs/heads/{current}"))
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
        let plan = actions.preview_merge_conflicts("feature").unwrap();
        assert_eq!(plan.source_oid, new_theirs.to_string());
        assert_eq!(plan.conflicts.len(), 1);
        actions
            .merge_with_conflict_choices(&plan, &[("a.txt".into(), GitConflictChoice::Ours)])
            .unwrap();
        let merged = actions.repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(merged.parent_id(0).unwrap(), ours);
        assert_eq!(merged.parent_id(1).unwrap(), new_theirs);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "ours\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("source-only.txt")).unwrap(),
            "source auto-merged content\n"
        );
        assert!(actions.repo.statuses(None).unwrap().is_empty());
    }

    #[test]
    fn binary_merge_conflicts_are_never_offered_as_text_file_choices() {
        let (dir, actions, ours, _) = fixture_divergence(true);
        let new_ours = fixture_commit(&actions, "a.txt", "binary\0file", "binary ours");
        assert_ne!(new_ours, ours);
        let error = actions.preview_merge_conflicts("feature").unwrap_err();
        assert!(
            error.contains("Binary") || error.contains("binary"),
            "{error}"
        );
        assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"binary\0file");
        assert!(actions.repo.statuses(None).unwrap().is_empty());
    }
}
