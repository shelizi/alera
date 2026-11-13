//! Explicit, worktree-scoped Git writers. The GPUI render path never calls
//! this module; a native click starts a bounded background operation.
//! No command invokes reset --hard, shell or force push. Guarded history
//! operations preserve a preflighted backup and refuse published rewrites.
//! A soft reset uses atomic compare-and-swap to preserve the index/worktree.
//! Merges use explicit local git2 operations and refuse unresolved conflicts.
use alera_core::git_cli::git_in_dir;
use git2::{build::CheckoutBuilder, BranchType, Delta, Index, Oid, Repository, StatusOptions};
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

/// Non-destructive Git Graph reference creation. Both kinds point to an
/// existing immutable commit and never move HEAD, index or working files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitCommitRefKind {
    LocalBranch,
    LightweightTag,
}

impl GitCommitRefKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::LocalBranch => "local branch",
            Self::LightweightTag => "lightweight tag",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitCommitApplyKind {
    CherryPick,
    Revert,
}

impl GitCommitApplyKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::CherryPick => "Cherry-pick",
            Self::Revert => "Revert",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCommitApplyPreview {
    pub kind: GitCommitApplyKind,
    /// Zero for ordinary single-parent commits; 1 or 2 selects merge mainline.
    pub mainline: u32,
    pub source_oid: String,
    pub source_subject: String,
    pub current_branch: String,
    pub current_oid: String,
    pub resulting_tree: String,
    pub changed_paths: Vec<String>,
}

/// Exact amend plan: HEAD, staged index tree and message are revalidated
/// before rewriting the current local HEAD. Never rewrite an upstream tip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitAmendPreview {
    pub head_oid: String,
    pub staged_tree_oid: String,
    /// Whether the confirmed amend also changes commit contents, not just its message.
    pub staged_changes: bool,
    pub current_message: String,
    pub replacement_message: String,
}

/// Local, offline draft based only on staged filenames and change kinds.
/// This is not AI-generated and must not inspect unstaged or untracked data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCommitMessageDraft {
    pub text: String,
    pub staged_files: usize,
    pub head_oid: String,
}

/// Rewinds only the selected, checked-out local branch ref. A soft reset
/// leaves the index and working files unchanged. The exact old HEAD and
/// tracked upstream are rechecked immediately before the atomic ref update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitSoftResetPreview {
    pub current_branch: String,
    pub current_oid: String,
    pub target_oid: String,
    pub target_subject: String,
    pub removed_commits: usize,
    pub upstream_oid: Option<String>,
}

/// Preview of an unpublished, linear current branch replay onto a different
/// local branch. No repository refs or files change while building a plan.
/// Merge conflicts are not predicted; on failure the replay must abort.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRebasePreview {
    pub current_branch: String,
    pub current_oid: String,
    pub onto_branch: String,
    pub onto_oid: String,
    pub merge_base_oid: String,
    pub upstream_oid: Option<String>,
    /// Topologically ordered, oldest-first linear commits to replay.
    pub commits: Vec<(String, String)>,
}

/// An explicitly selected untracked regular file's immutable content check.
/// Revalidate every item before any OS recycle-bin operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitUntrackedTrashCandidate {
    pub relative: PathBuf,
    pub size: u64,
    pub blob_oid: String,
}

const MAX_TRASH_FILES: usize = 80;
const MAX_TRASH_BYTES: u64 = 32 * 1024 * 1024;

/// Delegate to the platform trash/recycle bin. Never fall back to permanent
/// deletion if the trash provider is missing or refuses the operation.
fn send_file_to_os_trash(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;
        use std::os::windows::ffi::OsStrExt;
        #[repr(C)]
        struct FileOperation {
            hwnd: isize,
            function: u32,
            from: *const u16,
            to: *const u16,
            flags: u16,
            aborted: i32,
            name_mappings: *mut c_void,
            progress_title: *const u16,
        }
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn SHFileOperationW(operation: *mut FileOperation) -> i32;
        }
        let names: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
        let mut operation = FileOperation {
            hwnd: 0,
            function: 3, // FO_DELETE, with FOF_ALLOWUNDO -> recycle bin
            from: names.as_ptr(),
            to: std::ptr::null(),
            flags: 0x0040 | 0x0010 | 0x0400, // ALLOWUNDO, NOCONFIRMATION, NOERRORUI
            aborted: 0,
            name_mappings: std::ptr::null_mut(),
            progress_title: std::ptr::null(),
        };
        let result = unsafe { SHFileOperationW(&mut operation) };
        if result != 0 || operation.aborted != 0 {
            return Err(format!(
                "Windows Recycle Bin refused operation (code {result}, aborted {})",
                operation.aborted
            ));
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "tell application \"Finder\" to delete (POSIX file (item 1 of argv))",
                "-e",
                "end run",
            ])
            .arg(path)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("macOS Finder refused to move file to Trash".into());
        }
        Ok(())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let output = std::process::Command::new("gio")
            .arg("trash")
            .arg(path)
            .output()
            .map_err(|e| format!("GIO Trash unavailable: {e}"))?;
        if !output.status.success() {
            return Err("Linux Trash provider refused file; nothing permanently deleted".into());
        }
        Ok(())
    }
}

pub struct GitActions {
    repo: Repository,
    root: PathBuf,
}

impl GitActions {
    pub fn draft_commit_message(&self) -> Result<GitCommitMessageDraft, String> {
        if self.repo.state() != git2::RepositoryState::Clean {
            return Err("Resolve the in-progress Git operation before drafting a commit".into());
        }
        let head = self.repo.head().map_err(|error| error.to_string())?;
        if !head.is_branch() {
            return Err("Commit drafts require an active local branch".into());
        }
        let commit = head.peel_to_commit().map_err(|error| error.to_string())?;
        let tree = commit.tree().map_err(|error| error.to_string())?;
        let index = self.repo.index().map_err(|error| error.to_string())?;
        if index.has_conflicts() {
            return Err("Resolve staged conflicts before drafting a commit".into());
        }
        let diff = self
            .repo
            .diff_tree_to_index(Some(&tree), Some(&index), None)
            .map_err(|error| error.to_string())?;
        let mut changes = Vec::new();
        for change in diff.deltas() {
            if changes.len() == 256 {
                return Err("Offline commit draft supports at most 256 staged files".into());
            }
            let path = if change.status() == Delta::Deleted {
                change.old_file().path()
            } else {
                change.new_file().path()
            }
            .ok_or("Staged change has no usable path")?;
            let display = path
                .to_string_lossy()
                .chars()
                .map(|ch| if ch.is_control() { ' ' } else { ch })
                .take(80)
                .collect::<String>();
            changes.push((change.status(), display.trim().to_string()));
        }
        if changes.is_empty() {
            return Err("Stage at least one file before drafting a commit message".into());
        }
        let text = if changes.len() == 1 {
            let (status, path) = &changes[0];
            let action = match status {
                Delta::Added => "Add",
                Delta::Deleted => "Remove",
                Delta::Renamed => "Rename",
                _ => "Update",
            };
            format!("{action} {path}")
        } else {
            let verb = if changes.iter().all(|(status, _)| *status == Delta::Added) {
                "Add"
            } else if changes.iter().all(|(status, _)| *status == Delta::Deleted) {
                "Remove"
            } else {
                "Update"
            };
            format!("{verb} {} staged files", changes.len())
        };
        Ok(GitCommitMessageDraft {
            text,
            staged_files: changes.len(),
            head_oid: commit.id().to_string(),
        })
    }

    pub fn preview_rebase_onto_branch(&self, target: &str) -> Result<GitRebasePreview, String> {
        if !valid_branch(target) {
            return Err("Rebase target must be an exact local branch name".into());
        }
        if self.repo.state() != git2::RepositoryState::Clean {
            return Err("Finish or abort the in-progress Git operation before Rebase".into());
        }
        self.require_clean_worktree().map_err(|_| {
            "Rebase requires a clean index and worktree, including untracked files".to_string()
        })?;
        let head = self.repo.head().map_err(|error| error.to_string())?;
        if !head.is_branch() {
            return Err("Rebase requires a checked-out local branch".into());
        }
        let current_branch = head
            .shorthand()
            .map_err(|_| "Cannot identify the current local branch")?
            .to_owned();
        if !valid_branch(&current_branch)
            || matches!(
                current_branch.as_str(),
                "main" | "master" | "develop" | "development"
            )
        {
            return Err("Rebase is disabled for protected or invalid current branches".into());
        }
        if target == current_branch {
            return Err("Select a different local branch to Rebase onto".into());
        }
        let current_oid = head
            .peel_to_commit()
            .map_err(|error| error.to_string())?
            .id();
        let onto_oid = self
            .repo
            .find_branch(target, BranchType::Local)
            .map_err(|_| "Selected local Rebase target is missing".to_string())?
            .get()
            .peel_to_commit()
            .map_err(|error| error.to_string())?
            .id();
        let base = self
            .repo
            .merge_base(current_oid, onto_oid)
            .map_err(|_| "Rebase requires a common ancestor".to_string())?;
        if base == onto_oid {
            return Err(
                "Current branch already contains the selected branch; Rebase is unnecessary".into(),
            );
        }
        if base == current_oid {
            return Err(
                "Selected branch already contains current HEAD; use Fast-forward, not Rebase"
                    .into(),
            );
        }
        let local = self
            .repo
            .find_branch(&current_branch, BranchType::Local)
            .map_err(|error| error.to_string())?;
        let upstream = match local.upstream() {
            Ok(branch) => Some(
                branch
                    .get()
                    .peel_to_commit()
                    .map_err(|_| "Configured upstream is not a commit".to_string())?
                    .id(),
            ),
            Err(error) if error.code() == git2::ErrorCode::NotFound => None,
            Err(error) => return Err(format!("Cannot validate the configured upstream: {error}")),
        };
        if let Some(published) = upstream {
            let current_contains = published == current_oid
                || self
                    .repo
                    .graph_descendant_of(current_oid, published)
                    .map_err(|error| error.to_string())?;
            let base_contains = published == base
                || self
                    .repo
                    .graph_descendant_of(base, published)
                    .map_err(|error| error.to_string())?;
            if !current_contains || !base_contains {
                return Err(
                    "Rebase would replay commits already published upstream; refusing rewrite"
                        .into(),
                );
            }
        }
        let mut walk = self.repo.revwalk().map_err(|error| error.to_string())?;
        walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)
            .map_err(|error| error.to_string())?;
        walk.push(current_oid).map_err(|error| error.to_string())?;
        walk.hide(base).map_err(|error| error.to_string())?;
        let mut commits = Vec::new();
        for oid in walk {
            let oid = oid.map_err(|error| error.to_string())?;
            let commit = self
                .repo
                .find_commit(oid)
                .map_err(|error| error.to_string())?;
            if commit.parent_count() != 1 {
                return Err(
                    "Rebase of merge commits or roots requires an external Git tool".into(),
                );
            }
            if commits.len() >= 64 {
                return Err("Native Rebase is limited to at most 64 local linear commits".into());
            }
            commits.push((
                oid.to_string(),
                commit
                    .summary()
                    .ok()
                    .flatten()
                    .unwrap_or("(no subject)")
                    .chars()
                    .take(110)
                    .collect(),
            ));
        }
        if commits.is_empty() {
            return Err("There are no local commits to Rebase".into());
        }
        self.repo
            .signature()
            .map_err(|_| "Configure Git user.name and user.email before Rebase".to_string())?;
        Ok(GitRebasePreview {
            current_branch,
            current_oid: current_oid.to_string(),
            onto_branch: target.to_owned(),
            onto_oid: onto_oid.to_string(),
            merge_base_oid: base.to_string(),
            upstream_oid: upstream.map(|oid| oid.to_string()),
            commits,
        })
    }

    /// Before any replay, retain a reachable original HEAD in a namespaced
    /// recovery ref. Revalidate both branch tips and upstream immediately
    /// before execution. The existing Rust Core rebase aborts on conflict.
    pub fn apply_rebase_preview(&self, plan: &GitRebasePreview) -> Result<String, String> {
        let fresh = self.preview_rebase_onto_branch(&plan.onto_branch)?;
        if &fresh != plan {
            return Err(
                "Rebase preview is stale: HEAD, target, upstream or worktree changed".into(),
            );
        }
        let backup = format!("refs/alera/native-rebase-backup/{}", plan.current_oid);
        let previous = Oid::from_str(&plan.current_oid).map_err(|error| error.to_string())?;
        match self.repo.find_reference(&backup) {
            Ok(existing) if existing.target() == Some(previous) => {}
            Ok(_) => return Err("Rebase backup name already exists with a different commit".into()),
            Err(error) if error.code() == git2::ErrorCode::NotFound => {
                self.repo
                    .reference(&backup, previous, false, "alera native rebase safety")
                    .map_err(|error| format!("Cannot create Rebase recovery reference: {error}"))?;
            }
            Err(error) => return Err(format!("Cannot validate Rebase backup: {error}")),
        }
        let workspace = self
            .root
            .to_str()
            .ok_or("Rebase requires a Unicode workspace path")?;
        // Pin the reviewed commit ID, not a branch name that can move
        // between preflight and libgit2's rebase initialization.
        alera_core::git::rebase_onto(workspace, &plan.onto_oid).map_err(|error| {
            format!("Rebase stopped: {error}. Original HEAD is saved at {backup}")
        })?;
        let current = self.repo.head().map_err(|error| {
            format!("Rebase finished but HEAD cannot be verified: {error}; backup {backup}")
        })?;
        if current.shorthand().ok() != Some(plan.current_branch.as_str())
            || self.repo.state() != git2::RepositoryState::Clean
        {
            return Err(format!("Rebase finished in an unexpected state; review before further changes. Backup: {backup}"));
        }
        Ok(format!(
            "Rebased {} onto {} ({} local commit(s)); original tip kept in {}",
            plan.current_branch,
            plan.onto_branch,
            plan.commits.len(),
            backup
        ))
    }

    pub fn preview_soft_reset(&self, sha: &str) -> Result<GitSoftResetPreview, String> {
        if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Soft reset requires a full 40-character commit hash".into());
        }
        if self.repo.state() != git2::RepositoryState::Clean {
            return Err("Finish or abort the current Git operation before resetting".into());
        }
        self.require_clean_worktree().map_err(|_| {
            "Soft reset requires a clean worktree and index, including untracked files".to_string()
        })?;
        let head = self.repo.head().map_err(|error| error.to_string())?;
        if !head.is_branch() {
            return Err("Soft reset requires a checked-out local branch".into());
        }
        let branch = head
            .shorthand()
            .map_err(|_| "Cannot identify current local branch")?
            .to_owned();
        if !valid_branch(&branch)
            || matches!(
                branch.as_str(),
                "main" | "master" | "develop" | "development"
            )
        {
            return Err("Soft reset is disabled on protected or invalid branches".into());
        }
        let current = head
            .peel_to_commit()
            .map_err(|error| error.to_string())?
            .id();
        let target = Oid::from_str(sha).map_err(|error| error.to_string())?;
        let target_commit = self
            .repo
            .find_commit(target)
            .map_err(|_| "Selected reset commit is not available".to_string())?;
        if target == current
            || !self
                .repo
                .graph_descendant_of(current, target)
                .map_err(|error| error.to_string())?
        {
            return Err("Soft reset only permits an older ancestor of the current HEAD".into());
        }
        let (removed_commits, behind) = self
            .repo
            .graph_ahead_behind(current, target)
            .map_err(|error| error.to_string())?;
        if behind != 0 || removed_commits == 0 || removed_commits > 512 {
            return Err(
                "Soft reset refuses non-ancestor targets or more than 512 rewritten commits".into(),
            );
        }
        let local = self
            .repo
            .find_branch(&branch, BranchType::Local)
            .map_err(|error| error.to_string())?;
        let upstream = match local.upstream() {
            Ok(upstream) => Some(
                upstream
                    .get()
                    .peel_to_commit()
                    .map_err(|_| "Configured upstream is not a valid commit".to_string())?
                    .id(),
            ),
            Err(error) if error.code() == git2::ErrorCode::NotFound => None,
            Err(error) => return Err(format!("Cannot verify configured upstream: {error}")),
        };
        if let Some(remote) = upstream {
            let current_contains_remote = remote == current
                || self
                    .repo
                    .graph_descendant_of(current, remote)
                    .map_err(|error| error.to_string())?;
            let target_contains_remote = remote == target
                || self
                    .repo
                    .graph_descendant_of(target, remote)
                    .map_err(|error| error.to_string())?;
            if !current_contains_remote || !target_contains_remote {
                return Err(
                    "Soft reset would rewrite published or divergent upstream history".into(),
                );
            }
        }
        Ok(GitSoftResetPreview {
            current_branch: branch,
            current_oid: current.to_string(),
            target_oid: target.to_string(),
            target_subject: target_commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or("(no subject)")
                .chars()
                .take(110)
                .collect(),
            removed_commits,
            upstream_oid: upstream.map(|id| id.to_string()),
        })
    }

    pub fn apply_soft_reset(&self, plan: &GitSoftResetPreview) -> Result<String, String> {
        let fresh = self.preview_soft_reset(&plan.target_oid)?;
        if &fresh != plan {
            return Err("Soft reset preview is stale; refresh and review again".into());
        }
        let branch_ref = format!("refs/heads/{}", plan.current_branch);
        let reflog_message = format!(
            "alera native soft reset: {} to {}",
            &plan.current_oid[..8],
            &plan.target_oid[..8],
        );
        git_in_dir(
            &self.root,
            &[
                "update-ref",
                "-m",
                &reflog_message,
                &branch_ref,
                &plan.target_oid,
                &plan.current_oid,
            ],
        )
        .map_err(|_| {
            "Atomic soft reset refused: branch advanced or Git ref is locked".to_string()
        })?;
        Ok(format!(
            "Soft reset {} from {} to {}; index and worktree untouched (reflog available)",
            plan.current_branch,
            &plan.current_oid[..8],
            &plan.target_oid[..8]
        ))
    }

    /// Reset HEAD and the index to a reviewed unpublished ancestor while
    /// leaving all working files untouched. A recovery ref is retained in
    /// case Git fails after beginning its own mixed-reset transaction.
    pub fn apply_mixed_reset(&self, plan: &GitSoftResetPreview) -> Result<String, String> {
        let fresh = self.preview_soft_reset(&plan.target_oid)?;
        if &fresh != plan {
            return Err("Mixed Reset preview is stale; refresh and review again".into());
        }
        let previous = Oid::from_str(&plan.current_oid).map_err(|error| error.to_string())?;
        let target = self
            .repo
            .find_commit(Oid::from_str(&plan.target_oid).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        let backup = format!("refs/alera/native-mixed-reset-backup/{}", plan.current_oid);
        match self.repo.find_reference(&backup) {
            Ok(existing) if existing.target() == Some(previous) => {}
            Ok(_) => return Err("Mixed Reset backup already points to a different commit".into()),
            Err(error) if error.code() == git2::ErrorCode::NotFound => {
                self.repo
                    .reference(&backup, previous, false, "alera native mixed reset safety")
                    .map_err(|error| format!("Cannot create Mixed Reset recovery ref: {error}"))?;
            }
            Err(error) => return Err(format!("Cannot validate Mixed Reset recovery ref: {error}")),
        }
        self.repo
            .reset(target.as_object(), git2::ResetType::Mixed, None)
            .map_err(|error| {
                format!(
                    "Mixed Reset failed: {error}. Review Git state; original HEAD is at {backup}"
                )
            })?;
        let head = self.repo.head().map_err(|error| {
            format!("Cannot verify HEAD after Mixed Reset: {error}; backup {backup}")
        })?;
        let tree = self
            .repo
            .index()
            .and_then(|mut index| index.write_tree())
            .map_err(|error| {
                format!("Cannot verify Index after Mixed Reset: {error}; backup {backup}")
            })?;
        if head.shorthand().ok() != Some(plan.current_branch.as_str())
            || head.target() != Some(target.id())
            || tree != target.tree_id()
        {
            return Err(format!(
                "Mixed Reset ended in an unexpected state. Review before further changes; backup {backup}"
            ));
        }
        Ok(format!(
            "Mixed Reset {} from {} to {}; Index updated and working files preserved. Backup: {}",
            plan.current_branch,
            &plan.current_oid[..8],
            &plan.target_oid[..8],
            backup
        ))
    }

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
        self.tracking_for_branch(current)
    }

    /// Only an exact local branch with an established remote-tracking ref is
    /// eligible for push/pull without checkout. No arbitrary remote/refspec.
    fn tracking_for_branch(&self, current: &str) -> Result<(String, String, String), String> {
        if !valid_branch(current) {
            return Err("Git branch name is unsafe".into());
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

    /// Fetch and advance a local *non-checked-out* branch with an atomic
    /// compare-and-swap ref update. This does not change HEAD or any worktree.
    /// Fail closed if any linked worktree checks out the requested branch.
    pub fn pull_branch_without_switch(&self, selected: &str) -> Result<String, String> {
        let (branch, remote, upstream) = self.tracking_for_branch(selected)?;
        let manager = crate::worktree_manager::WorktreeManager::discover(&self.root)
            .map_err(|error| format!("Cannot inspect worktree owners: {error}"))?;
        if manager
            .list()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|entry| entry.branch.as_deref() == Some(branch.as_str()))
        {
            return Err(
                "Branch is checked out in a worktree. Open that worktree to pull instead".into(),
            );
        }
        let old = self
            .repo
            .find_branch(&branch, BranchType::Local)
            .and_then(|branch| branch.get().peel_to_commit())
            .map_err(|error| error.to_string())?
            .id();
        let remote_ref = format!("refs/heads/{upstream}");
        git_in_dir(
            &self.root,
            &["fetch", "--no-tags", "--", &remote, &remote_ref],
        )
        .map_err(|_| "Fetch of selected branch failed; no local branch moved".to_string())?;
        let updated = self
            .repo
            .find_reference(&format!("refs/remotes/{remote}/{upstream}"))
            .and_then(|reference| reference.peel_to_commit())
            .map_err(|_| {
                "Updated remote tracking ref unavailable; local branch unchanged".to_string()
            })?
            .id();
        if old == updated {
            return Ok(format!(
                "{branch} already up-to-date with {remote}/{upstream}"
            ));
        }
        if !self
            .repo
            .graph_descendant_of(updated, old)
            .map_err(|error| error.to_string())?
        {
            return Err("Selected branch has diverged from upstream; fast-forward refused".into());
        }
        // git update-ref <name> <new> <old> is an atomic CAS. It fails if the
        // local branch moved concurrently; it never checks out worktree files.
        let local_ref = format!("refs/heads/{branch}");
        let new_id = updated.to_string();
        let expected_old = old.to_string();
        git_in_dir(
            &self.root,
            &["update-ref", &local_ref, &new_id, &expected_old],
        )
        .map_err(|_| "Selected branch advanced concurrently; update-ref refused".to_string())?;
        Ok(format!(
            "Fast-forwarded {branch} from {remote}/{upstream} without switching"
        ))
    }

    /// Push a specifically chosen local branch to its exact configured
    /// upstream without switching HEAD, force flags, or remote inference.
    pub fn push_branch_without_switch(&self, selected: &str) -> Result<String, String> {
        let (branch, remote, upstream) = self.tracking_for_branch(selected)?;
        let local = self
            .repo
            .find_branch(&branch, BranchType::Local)
            .and_then(|branch| branch.get().peel_to_commit())
            .map_err(|error| error.to_string())?
            .id();
        let tracking = self
            .repo
            .find_reference(&format!("refs/remotes/{remote}/{upstream}"))
            .and_then(|reference| reference.peel_to_commit())
            .map_err(|_| "Upstream tracking ref unavailable; Fetch before Push".to_string())?
            .id();
        if local != tracking
            && !self
                .repo
                .graph_descendant_of(local, tracking)
                .map_err(|error| error.to_string())?
        {
            return Err("Push refused: selected branch diverged from fetched upstream".into());
        }
        let refspec = format!("refs/heads/{branch}:refs/heads/{upstream}");
        git_in_dir(
            &self.root,
            &["push", "--porcelain", "--", &remote, &refspec],
        )
        .map_err(|_| "Selected-branch Push failed; no force attempted".to_string())?;
        Ok(format!(
            "Pushed {branch} to {remote}/{upstream} without switching"
        ))
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

    fn validate_untracked_trash_paths(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<GitUntrackedTrashCandidate>, String> {
        if paths.is_empty() || paths.len() > MAX_TRASH_FILES {
            return Err("Select between 1 and 80 untracked files for Trash".into());
        }
        let index = self.repo.index().map_err(|e| e.to_string())?;
        let mut unique = std::collections::BTreeSet::new();
        for path in paths {
            unique.insert(self.relative(path, false)?);
        }
        let mut selected = Vec::new();
        let mut total = 0u64;
        for path in unique {
            if index.get_path(&path, 0).is_some() {
                return Err(format!(
                    "{} is staged/tracked; only untracked files can move to Trash",
                    path.display()
                ));
            }
            let status = self.repo.status_file(&path).map_err(|e| e.to_string())?;
            if status != git2::Status::WT_NEW {
                return Err(format!(
                    "{} is not an unchanged untracked selection; refresh status",
                    path.display()
                ));
            }
            let full = self.root.join(&path);
            let metadata = fs::symlink_metadata(&full).map_err(|e| e.to_string())?;
            if !metadata.file_type().is_file() {
                return Err(format!(
                    "{} is not a regular file (symlinks and folders are refused)",
                    path.display()
                ));
            }
            total = total
                .checked_add(metadata.len())
                .ok_or("Trash selection exceeds size limit")?;
            if total > MAX_TRASH_BYTES {
                return Err("Trash preview limited to 32 MiB total; use the OS file manager for larger files".into());
            }
            let bytes = fs::read(&full).map_err(|e| e.to_string())?;
            if bytes.len() as u64 != metadata.len() {
                return Err("Untracked file changed during Trash preview".into());
            }
            let digest =
                Oid::hash_object(git2::ObjectType::Blob, &bytes).map_err(|e| e.to_string())?;
            selected.push(GitUntrackedTrashCandidate {
                relative: path,
                size: metadata.len(),
                blob_oid: digest.to_string(),
            });
        }
        Ok(selected)
    }

    /// Preflight only: never creates files, Git objects or recycle-bin entries.
    pub fn preview_untracked_trash(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<GitUntrackedTrashCandidate>, String> {
        self.validate_untracked_trash_paths(paths)
    }

    /// The full content fingerprints must be unchanged since confirmation.
    /// Moves files into the system Trash/Recycle Bin, never deletes permanently.
    /// If one OS move fails, earlier moves may already be in Trash.
    pub fn apply_untracked_trash(
        &self,
        plan: &[GitUntrackedTrashCandidate],
    ) -> Result<usize, String> {
        let paths: Vec<PathBuf> = plan.iter().map(|item| item.relative.clone()).collect();
        let fresh = self.validate_untracked_trash_paths(&paths)?;
        if fresh != plan {
            return Err(
                "Untracked files changed since Trash preview; inspect and confirm again".into(),
            );
        }
        for (index, entry) in fresh.iter().enumerate() {
            // Recheck immediately before each OS call, including symlink
            // and status changes after the batch preflight.
            if self.validate_untracked_trash_paths(&[entry.relative.clone()])?
                != vec![entry.clone()]
            {
                return Err(format!("Trash selection changed after {index} file(s) moved; remaining files untouched"));
            }
            send_file_to_os_trash(&self.root.join(&entry.relative))
                .map_err(|e| format!("Trash refused after {index} successful move(s): {e}"))?;
        }
        Ok(fresh.len())
    }

    /// Explicitly confirmed discard of tracked, unstaged working-tree edits.
    /// Restore from the current index, NEVER from HEAD: staged content must
    /// remain staged. Untracked files are not removed by this API.
    ///
    /// Validate all paths before invoking one Git restore command so an
    /// invalid/renamed/unsafe target cannot cause a partial batch restore.
    pub fn discard_unstaged_tracked(&self, paths: &[PathBuf]) -> Result<usize, String> {
        use git2::Status;
        if paths.is_empty() || paths.len() > 120 {
            return Err("Select between 1 and 120 tracked changed files to restore".into());
        }
        let mut selected = std::collections::BTreeSet::new();
        for path in paths {
            selected.insert(self.relative(path, true)?);
        }
        let index = self.repo.index().map_err(|error| error.to_string())?;
        if index.has_conflicts() {
            return Err("Resolve merge conflicts before discarding tracked edits".into());
        }
        let mut arguments = vec![
            "restore".to_owned(),
            "--worktree".to_owned(),
            "--".to_owned(),
        ];
        for path in &selected {
            let entry = index.get_path(path, 0).ok_or_else(|| {
                format!(
                    "{} is not tracked in the index; untracked files must be handled separately",
                    path.display()
                )
            })?;
            if !matches!(entry.mode, 0o100644 | 0o100755) {
                return Err(format!("{} is not a regular tracked file", path.display()));
            }
            let status = self
                .repo
                .status_file(path)
                .map_err(|error| error.to_string())?;
            if !status.intersects(Status::WT_MODIFIED | Status::WT_DELETED)
                || status.intersects(
                    Status::WT_NEW
                        | Status::WT_RENAMED
                        | Status::WT_TYPECHANGE
                        | Status::CONFLICTED,
                )
            {
                return Err(format!(
                    "{} no longer has safe unstaged tracked edits; refresh Git status",
                    path.display()
                ));
            }
            let name = path
                .to_str()
                .ok_or("Non-Unicode Git path is unsupported for native discard")?;
            if name.len() > 1024 {
                return Err("Git discard file path exceeds safety limit".into());
            }
            arguments.push(name.to_owned());
        }
        // One explicit command, exact relative paths after '--', no shell,
        // no --hard, no index/ref modifications.
        let argv = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        git_in_dir(&self.root, &argv).map_err(|error| {
            format!("Git worktree restore failed; inspect Git status before retrying: {error}")
        })?;
        Ok(selected.len())
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
        use git2::Status;
        let index = self.repo.index().map_err(|error| error.to_string())?;
        if index.has_conflicts() {
            return Err("Resolve index conflicts before unstaging files".into());
        }
        drop(index);
        // A staged deletion has no stage-0 index entry. Validate the INDEX
        // status of every path before updating any index entries.
        let staged_status = Status::INDEX_NEW
            | Status::INDEX_MODIFIED
            | Status::INDEX_DELETED
            | Status::INDEX_RENAMED
            | Status::INDEX_TYPECHANGE;
        for path in &selected {
            let status = self
                .repo
                .status_file(path)
                .map_err(|error| error.to_string())?;
            if status.contains(Status::CONFLICTED) || !status.intersects(staged_status) {
                return Err("Selected file is not staged; refresh the Git changes list".into());
            }
        }
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
        // A fresh repository has a symbolic local branch HEAD without any
        // commit yet. Permit its first real staged commit, but not detached
        // HEAD or an empty initial tree.
        let parent = match self.repo.head() {
            Ok(head) => {
                if !head.is_branch() {
                    return Err("Committing from detached HEAD is disabled".into());
                }
                Some(head.peel_to_commit().map_err(|error| error.to_string())?)
            }
            Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
                let head = self
                    .repo
                    .find_reference("HEAD")
                    .map_err(|e| e.to_string())?;
                if !head
                    .symbolic_target()
                    .map_err(|e| e.to_string())?
                    .is_some_and(|target| target.starts_with("refs/heads/"))
                {
                    return Err("Initial commit requires a symbolic local branch HEAD".into());
                }
                None
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut index = self.repo.index().map_err(|error| error.to_string())?;
        if index.has_conflicts() {
            return Err("Resolve Git index conflicts before committing".into());
        }
        let tree_oid = index.write_tree().map_err(|error| error.to_string())?;
        if (parent.is_none() && index.len() == 0)
            || parent
                .as_ref()
                .is_some_and(|parent| tree_oid == parent.tree_id())
        {
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
        let parents = parent.iter().collect::<Vec<_>>();
        let oid = self
            .repo
            .commit(Some("HEAD"), &author, &author, message, &tree, &parents)
            .map_err(|error| error.to_string())?;
        Ok(oid.to_string())
    }

    pub fn preview_amend_commit(&self, replacement: &str) -> Result<GitAmendPreview, String> {
        let replacement = replacement.trim();
        if replacement.is_empty() || replacement.len() > 4096 || replacement.contains('\0') {
            return Err("Amend message must be nonempty, NUL-free and at most 4096 bytes".into());
        }
        let head = self.repo.head().map_err(|e| e.to_string())?;
        if !head.is_branch() {
            return Err("Cannot amend detached HEAD".into());
        }
        let branch_name = head
            .shorthand()
            .map_err(|_| "Cannot identify current branch")?;
        let commit = head.peel_to_commit().map_err(|e| e.to_string())?;
        if commit.parent_count() > 1 {
            return Err("Merge commits cannot be amended from Native Source Control".into());
        }
        // Refuse rewriting the commit reachable from the configured upstream.
        let local_branch = self
            .repo
            .find_branch(branch_name, BranchType::Local)
            .map_err(|e| e.to_string())?;
        if let Ok(upstream) = local_branch.upstream() {
            if let Some(remote_tip) = upstream.get().target() {
                if remote_tip == commit.id()
                    || self
                        .repo
                        .graph_descendant_of(remote_tip, commit.id())
                        .unwrap_or(false)
                {
                    return Err(
                        "HEAD is already on the tracked upstream; refusing history rewrite".into(),
                    );
                }
            }
        }
        let mut index = self.repo.index().map_err(|e| e.to_string())?;
        if index.has_conflicts() {
            return Err("Cannot amend with index conflicts".into());
        }
        let tree = index.write_tree().map_err(|e| e.to_string())?;
        let current_message = commit
            .message()
            .map_err(|_| "Current commit message is not UTF-8")?
            .to_owned();
        if tree == commit.tree_id() && replacement == current_message.trim() {
            return Err("No staged changes or message differences to amend".into());
        }
        Ok(GitAmendPreview {
            head_oid: commit.id().to_string(),
            staged_tree_oid: tree.to_string(),
            staged_changes: tree != commit.tree_id(),
            current_message,
            replacement_message: replacement.to_owned(),
        })
    }

    pub fn apply_amend_preview(&self, preview: &GitAmendPreview) -> Result<String, String> {
        let fresh = self.preview_amend_commit(&preview.replacement_message)?;
        if &fresh != preview {
            return Err("Amend preview is stale: HEAD, index or message changed".into());
        }
        let head = self
            .repo
            .find_commit(Oid::from_str(&preview.head_oid).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let tree = self
            .repo
            .find_tree(Oid::from_str(&preview.staged_tree_oid).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let committer = self
            .repo
            .signature()
            .map_err(|_| "Configure Git committer identity before amend".to_string())?;
        let id = head
            .amend(
                Some("HEAD"),
                None,
                Some(&committer),
                None,
                Some(&preview.replacement_message),
                Some(&tree),
            )
            .map_err(|e| format!("Amend refused: {e}"))?;
        Ok(format!(
            "Amended local HEAD {} to {}; upstream unchanged",
            &preview.head_oid[..8],
            &id.to_string()[..8]
        ))
    }

    /// Preview against HEAD in memory, with an explicit parent choice for
    /// two-parent merges. Never change the real index, HEAD or worktree.
    pub fn preview_commit_apply(
        &self,
        sha: &str,
        kind: GitCommitApplyKind,
    ) -> Result<GitCommitApplyPreview, String> {
        self.preview_commit_apply_with_mainline(sha, kind, 0)
    }

    pub fn preview_commit_apply_with_mainline(
        &self,
        sha: &str,
        kind: GitCommitApplyKind,
        mainline: u32,
    ) -> Result<GitCommitApplyPreview, String> {
        self.require_clean_worktree()?;
        if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Select an exact 40-character commit SHA".into());
        }
        let target = self
            .repo
            .find_commit(Oid::from_str(sha).map_err(|e| e.to_string())?)
            .map_err(|_| "Selected Git commit does not exist".to_string())?;
        match target.parent_count() {
            1 if mainline == 0 => {}
            2 if matches!(mainline, 1 | 2) => {}
            0 => {
                return Err("Root commits cannot be cherry-picked/reverted with a mainline".into())
            }
            1 => return Err("Single-parent commits must use the ordinary operation".into()),
            2 => return Err("Select explicit merge parent 1 or 2 (mainline)".into()),
            _ => return Err("Octopus merges are unsupported; use a dedicated Git tool".into()),
        }
        let head = self.repo.head().map_err(|e| e.to_string())?;
        if !head.is_branch() {
            return Err("Checkout a local branch before applying a commit".into());
        }
        let branch = head.shorthand().map_err(|e| e.to_string())?.to_owned();
        let current = head.peel_to_commit().map_err(|e| e.to_string())?;
        if kind == GitCommitApplyKind::CherryPick && target.id() == current.id() {
            return Err("Cannot cherry-pick the active HEAD".into());
        }
        let mut index = match kind {
            GitCommitApplyKind::CherryPick => self
                .repo
                .cherrypick_commit(&target, &current, mainline, None),
            GitCommitApplyKind::Revert => {
                self.repo.revert_commit(&target, &current, mainline, None)
            }
        }
        .map_err(|e| format!("Cannot calculate {}: {e}", kind.label()))?;
        if index.has_conflicts() {
            return Err(format!(
                "{} would create conflicts; no worktree or HEAD changes applied",
                kind.label()
            ));
        }
        let tree_oid = index.write_tree_to(&self.repo).map_err(|e| e.to_string())?;
        let head_tree = current.tree().map_err(|e| e.to_string())?;
        if head_tree.id() == tree_oid {
            return Err(format!(
                "{} has no changes against current HEAD",
                kind.label()
            ));
        }
        let proposed = self.repo.find_tree(tree_oid).map_err(|e| e.to_string())?;
        let diff = self
            .repo
            .diff_tree_to_tree(Some(&head_tree), Some(&proposed), None)
            .map_err(|e| e.to_string())?;
        if diff.deltas().len() > 80 {
            return Err("Commit changes more than 80 files".into());
        }
        let mut paths = Vec::new();
        for delta in diff.deltas() {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .ok_or("Git delta has no file path")?;
            if path.is_absolute()
                || path
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
            {
                return Err("Commit contains an unsafe path".into());
            }
            paths.push(path.to_string_lossy().into_owned());
        }
        if paths.is_empty() {
            return Err("Commit produces no changed files".into());
        }
        Ok(GitCommitApplyPreview {
            kind,
            mainline,
            source_oid: sha.to_owned(),
            source_subject: target
                .summary()
                .ok()
                .flatten()
                .unwrap_or("(untitled)")
                .to_owned(),
            current_branch: branch,
            current_oid: current.id().to_string(),
            resulting_tree: tree_oid.to_string(),
            changed_paths: paths,
        })
    }

    /// Recompute the full preview before any checkout; only conflict-free
    /// exact matches are applied. A late commit error leaves changes staged
    /// rather than force resetting or deleting user content.
    pub fn apply_commit_preview(&self, preview: &GitCommitApplyPreview) -> Result<String, String> {
        let fresh = self.preview_commit_apply_with_mainline(
            &preview.source_oid,
            preview.kind,
            preview.mainline,
        )?;
        if &fresh != preview {
            return Err("Git Graph preview is stale; inspect the commit again".into());
        }
        let current = self
            .repo
            .find_commit(Oid::from_str(&preview.current_oid).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let source = self
            .repo
            .find_commit(Oid::from_str(&preview.source_oid).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let new_tree = self
            .repo
            .find_tree(Oid::from_str(&preview.resulting_tree).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let identity = self
            .repo
            .signature()
            .map_err(|_| "Set Git user.name and user.email before applying commits".to_string())?;
        let mut dry_run = CheckoutBuilder::new();
        dry_run.safe().dry_run();
        self.repo
            .checkout_tree(new_tree.as_object(), Some(&mut dry_run))
            .map_err(|e| format!("Checkout preflight refused: {e}"))?;
        self.require_clean_worktree()?;
        let head = self.repo.head().map_err(|e| e.to_string())?;
        if !head.is_branch()
            || head.shorthand().ok() != Some(preview.current_branch.as_str())
            || head.target() != Some(current.id())
        {
            return Err("HEAD moved since preview; no files modified".into());
        }
        let mut checkout = CheckoutBuilder::new();
        checkout.safe().recreate_missing(false);
        self.repo
            .checkout_tree(new_tree.as_object(), Some(&mut checkout))
            .map_err(|e| format!("Checkout refused; branch ref unchanged: {e}"))?;
        let mut index = self.repo.index().map_err(|e| e.to_string())?;
        index.read_tree(&new_tree).map_err(|e| e.to_string())?;
        index.write().map_err(|e| e.to_string())?;
        let message = match preview.kind {
            GitCommitApplyKind::CherryPick => format!(
                "{}\n\n(cherry picked from commit {})",
                source.message().unwrap_or("Cherry-picked commit"),
                source.id()
            ),
            GitCommitApplyKind::Revert => format!(
                "Revert \"{}\"\n\nThis reverts commit {}.",
                preview.source_subject,
                source.id()
            ),
        };
        let id = self
            .repo
            .commit(
                Some("HEAD"),
                &identity,
                &identity,
                &message,
                &new_tree,
                &[&current],
            )
            .map_err(|e| format!("Commit creation failed; changes remain staged: {e}"))?;
        Ok(format!(
            "{} succeeded on {} as {} ({} file(s))",
            preview.kind.label(),
            preview.current_branch,
            &id.to_string()[..8],
            preview.changed_paths.len()
        ))
    }

    /// Create a branch or lightweight tag at the exact selected Git Graph
    /// commit. No shell invocation, checkout, Git index update or force flag.
    /// All names are validated as complete Git refs before libgit2 writes.
    pub fn create_ref_at_commit(
        &self,
        kind: GitCommitRefKind,
        commit_sha: &str,
        name: &str,
    ) -> Result<String, String> {
        if commit_sha.len() != 40 || !commit_sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Use a full 40-character hexadecimal commit ID".into());
        }
        if !valid_branch(name) || name.starts_with('-') || name.ends_with(".lock") {
            return Err("Invalid new branch/tag name".into());
        }
        let ref_name = match kind {
            GitCommitRefKind::LocalBranch => format!("refs/heads/{name}"),
            GitCommitRefKind::LightweightTag => format!("refs/tags/{name}"),
        };
        if !git2::Reference::is_valid_name(&ref_name) {
            return Err("Name violates Git reference rules".into());
        }
        if self.repo.find_reference(&ref_name).is_ok() {
            return Err(format!(
                "{} already exists; overwrite is not permitted",
                kind.label()
            ));
        }
        let oid = Oid::from_str(commit_sha).map_err(|_| "Invalid Git commit ID".to_owned())?;
        let commit = self
            .repo
            .find_commit(oid)
            .map_err(|_| "Selected commit no longer exists; refresh Git Graph".to_owned())?;
        match kind {
            GitCommitRefKind::LocalBranch => {
                self.repo
                    .branch(name, &commit, false)
                    .map_err(|error| format!("Cannot create Git branch: {error}"))?;
            }
            GitCommitRefKind::LightweightTag => {
                self.repo
                    .tag_lightweight(name, commit.as_object(), false)
                    .map_err(|error| format!("Cannot create lightweight tag: {error}"))?;
            }
        }
        Ok(format!(
            "Created {} '{name}' at {} without checkout",
            kind.label(),
            &commit_sha[..8]
        ))
    }

    /// Rename only a local branch that is not checked out by any linked
    /// worktree. Never force overwrite another ref or rename the active HEAD.
    /// The confirmation UI supplies both exact names, revalidated here.
    pub fn rename_inactive_branch(
        &self,
        source: &str,
        destination: &str,
    ) -> Result<String, String> {
        if !valid_branch(source) || !valid_branch(destination) || source == destination {
            return Err("Provide distinct valid local Git branch names".into());
        }
        let manager = crate::worktree_manager::WorktreeManager::discover(&self.root)
            .map_err(|error| format!("Cannot inspect worktrees: {error}"))?;
        let entries = manager.list().map_err(|error| error.to_string())?;
        if entries
            .iter()
            .any(|entry| entry.branch.as_deref() == Some(source))
        {
            return Err(
                "Source branch is checked out in a worktree; open it to manage its name".into(),
            );
        }
        if self
            .repo
            .find_branch(destination, BranchType::Local)
            .is_ok()
        {
            return Err("Destination branch already exists; no overwrite permitted".into());
        }
        let mut old = self
            .repo
            .find_branch(source, BranchType::Local)
            .map_err(|error| error.to_string())?;
        old.rename(destination, false)
            .map_err(|error| format!("Git branch rename failed; inspect branch refs: {error}"))?;
        Ok(format!("Renamed local branch {source} to {destination}; upstream and remote branches unchanged"))
    }

    /// Rename only the checked-out branch owned by this exact worktree.
    /// Git CLI updates the proper linked-worktree HEAD, reflog and branch
    /// configuration together; never edit common refs or another worktree's
    /// HEAD directly. No force overwrite, no checkout and no file changes.
    pub fn rename_owned_branch(&self, source: &str, destination: &str) -> Result<String, String> {
        if !valid_branch(source) || !valid_branch(destination) || source == destination {
            return Err("Provide distinct valid local branch names".into());
        }
        let head = self.repo.head().map_err(|e| e.to_string())?;
        if !head.is_branch() || head.shorthand().map_err(|e| e.to_string())? != source {
            return Err("Branch is not checked out in the current worktree".into());
        }
        let original = head.peel_to_commit().map_err(|e| e.to_string())?.id();
        let manager = crate::worktree_manager::WorktreeManager::discover(&self.root)
            .map_err(|e| e.to_string())?;
        let entries = manager.list().map_err(|e| e.to_string())?;
        let owners = entries
            .iter()
            .filter(|entry| entry.branch.as_deref() == Some(source))
            .collect::<Vec<_>>();
        if owners.len() != 1
            || owners[0].path != self.root
            || owners[0].locked
            || owners[0].prunable
            || owners[0].detached
        {
            return Err("Only the owning, live worktree may rename its checked-out branch".into());
        }
        if self
            .repo
            .find_branch(destination, BranchType::Local)
            .is_ok()
            || entries
                .iter()
                .any(|entry| entry.branch.as_deref() == Some(destination))
        {
            return Err("Destination branch already exists or is checked out".into());
        }
        // A managed Git command (never a shell) knows how to update HEAD for
        // linked worktrees; libgit2 ref-only rename would risk stale HEAD.
        git_in_dir(&self.root, &["branch", "-m", "--", source, destination])
            .map_err(|e| format!("Checked-out branch rename refused: {e}"))?;
        let moved = self.repo.head().map_err(|e| e.to_string())?;
        if !moved.is_branch()
            || moved.shorthand().map_err(|e| e.to_string())? != destination
            || moved.peel_to_commit().map_err(|e| e.to_string())?.id() != original
        {
            return Err("Branch moved but HEAD validation failed; refresh worktrees before further Git operations".into());
        }
        Ok(format!("Renamed checked-out branch {source} to {destination} in its owning worktree; no checkout or worktree file changes"))
    }

    /// Delete a disposable local branch only after proving it is merged into
    /// the active HEAD, not checked out in any worktree, and not protected.
    /// Never delete remote refs, unreachable commits, or the current branch.
    pub fn delete_merged_inactive_branch(&self, name: &str) -> Result<String, String> {
        if !valid_branch(name) || matches!(name, "main" | "master" | "develop" | "development") {
            return Err("Refusing unsafe or protected branch deletion".into());
        }
        let head = self.repo.head().map_err(|e| e.to_string())?;
        if !head.is_branch() || head.shorthand().map_err(|e| e.to_string())? == name {
            return Err("Cannot delete the current branch or detached HEAD".into());
        }
        let current = head.peel_to_commit().map_err(|e| e.to_string())?.id();
        let manager = crate::worktree_manager::WorktreeManager::discover(&self.root)
            .map_err(|e| e.to_string())?;
        let entries = manager.list().map_err(|e| e.to_string())?;
        if entries
            .iter()
            .any(|entry| entry.branch.as_deref() == Some(name))
        {
            return Err("Branch is checked out in another worktree; cannot delete".into());
        }
        let mut branch = self
            .repo
            .find_branch(name, BranchType::Local)
            .map_err(|e| e.to_string())?;
        let tip = branch
            .get()
            .peel_to_commit()
            .map_err(|e| e.to_string())?
            .id();
        if tip != current
            && !self
                .repo
                .graph_descendant_of(current, tip)
                .map_err(|e| e.to_string())?
        {
            return Err("Branch has unmerged commits; refusing history loss".into());
        }
        branch
            .delete()
            .map_err(|e| format!("Git branch delete refused: {e}"))?;
        Ok(format!(
            "Deleted merged inactive local branch {name}; worktrees and remote branches unchanged"
        ))
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

    fn soft_reset_fixture() -> (tempfile::TempDir, GitActions, Oid, Oid) {
        let (dir, actions) = repo_fixture();
        let root = actions.root.clone();
        drop(actions);
        let repo = Repository::open(&root).unwrap();
        let first = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature/reset", &first, false).unwrap();
        repo.set_head("refs/heads/feature/reset").unwrap();
        fs::write(root.join("a.txt"), "after").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("Fixture", "fixture@example.invalid").unwrap();
        let second = repo
            .commit(Some("HEAD"), &sig, &sig, "second", &tree, &[&first])
            .unwrap();
        let first_oid = first.id();
        drop(tree);
        drop(first);
        drop(repo);
        (dir, GitActions::open(&root).unwrap(), first_oid, second)
    }

    #[test]
    fn soft_reset_preserves_index_and_worktree_and_records_reflog() {
        let (_dir, actions, first, second) = soft_reset_fixture();
        let before = fs::read(actions.root.join("a.txt")).unwrap();
        let index_tree = actions.repo.index().unwrap().write_tree().unwrap();
        let plan = actions.preview_soft_reset(&first.to_string()).unwrap();
        assert_eq!(plan.current_oid, second.to_string());
        assert_eq!(plan.removed_commits, 1);
        assert_eq!(plan.upstream_oid, None);
        let message = actions.apply_soft_reset(&plan).unwrap();
        assert!(message.contains("index and worktree untouched"));
        assert_eq!(actions.repo.head().unwrap().target(), Some(first));
        assert_eq!(fs::read(actions.root.join("a.txt")).unwrap(), before);
        assert_eq!(
            actions.repo.index().unwrap().write_tree().unwrap(),
            index_tree
        );
        assert_eq!(
            actions
                .repo
                .reflog("refs/heads/feature/reset")
                .unwrap()
                .get(0)
                .unwrap()
                .id_new(),
            first
        );
        assert!(actions.preview_soft_reset(&first.to_string()).is_err());
    }

    #[test]
    fn soft_reset_rejects_stale_head_dirty_files_and_nonancestor_targets() {
        let (_dir, actions, first, _second) = soft_reset_fixture();
        assert!(actions.preview_soft_reset("HEAD~1").is_err());
        let plan = actions.preview_soft_reset(&first.to_string()).unwrap();
        fs::write(actions.root.join("untracked.tmp"), "new").unwrap();
        assert!(actions.preview_soft_reset(&first.to_string()).is_err());
        assert!(actions.apply_soft_reset(&plan).is_err());
        fs::remove_file(actions.root.join("untracked.tmp")).unwrap();
        let head = actions.repo.head().unwrap().peel_to_commit().unwrap();
        let tree = head.tree().unwrap();
        let sig = Signature::now("Fixture", "fixture@example.invalid").unwrap();
        actions
            .repo
            .commit(Some("HEAD"), &sig, &sig, "third", &tree, &[&head])
            .unwrap();
        assert!(actions.apply_soft_reset(&plan).is_err());
        assert!(actions.repo.head().unwrap().target().is_some());
    }

    #[test]
    fn soft_reset_blocks_upstream_published_commits_and_protected_branches() {
        let (_dir, actions, first, second) = soft_reset_fixture();
        actions
            .repo
            .remote("origin", "https://example.invalid/repo.git")
            .unwrap();
        actions
            .repo
            .reference("refs/remotes/origin/feature/reset", second, true, "fixture")
            .unwrap();
        actions
            .repo
            .config()
            .unwrap()
            .set_str("branch.feature/reset.remote", "origin")
            .unwrap();
        actions
            .repo
            .config()
            .unwrap()
            .set_str("branch.feature/reset.merge", "refs/heads/feature/reset")
            .unwrap();
        assert!(actions.preview_soft_reset(&first.to_string()).is_err());
        actions
            .repo
            .branch("main", &actions.repo.find_commit(second).unwrap(), false)
            .unwrap();
        actions.repo.set_head("refs/heads/main").unwrap();
        assert!(actions.preview_soft_reset(&first.to_string()).is_err());
    }

    #[test]
    fn mixed_reset_moves_head_and_index_but_preserves_working_file_bytes() {
        let (_dir, actions, first, second) = soft_reset_fixture();
        let before = fs::read(actions.root.join("a.txt")).unwrap();
        let plan = actions.preview_soft_reset(&first.to_string()).unwrap();
        let result = actions.apply_mixed_reset(&plan).unwrap();
        assert!(result.contains("working files preserved"));
        assert_eq!(actions.repo.head().unwrap().target(), Some(first));
        assert_eq!(
            actions.repo.index().unwrap().write_tree().unwrap(),
            actions.repo.find_commit(first).unwrap().tree_id()
        );
        assert_eq!(fs::read(actions.root.join("a.txt")).unwrap(), before);
        assert!(actions
            .repo
            .status_file(Path::new("a.txt"))
            .unwrap()
            .contains(git2::Status::WT_MODIFIED));
        assert_eq!(
            actions
                .repo
                .find_reference(&format!("refs/alera/native-mixed-reset-backup/{second}"))
                .unwrap()
                .target(),
            Some(second)
        );
        assert_eq!(actions.repo.state(), git2::RepositoryState::Clean);
    }

    #[test]
    fn mixed_reset_revalidates_dirty_and_published_state_without_backup() {
        let (_dir, actions, first, second) = soft_reset_fixture();
        let plan = actions.preview_soft_reset(&first.to_string()).unwrap();
        fs::write(actions.root.join("extra.tmp"), "untracked").unwrap();
        assert!(actions.apply_mixed_reset(&plan).is_err());
        fs::remove_file(actions.root.join("extra.tmp")).unwrap();
        actions
            .repo
            .remote("origin", "https://example.invalid/repo.git")
            .unwrap();
        actions
            .repo
            .reference("refs/remotes/origin/feature/reset", second, true, "fixture")
            .unwrap();
        actions
            .repo
            .config()
            .unwrap()
            .set_str("branch.feature/reset.remote", "origin")
            .unwrap();
        actions
            .repo
            .config()
            .unwrap()
            .set_str("branch.feature/reset.merge", "refs/heads/feature/reset")
            .unwrap();
        assert!(actions.apply_mixed_reset(&plan).is_err());
        assert_eq!(actions.repo.head().unwrap().target(), Some(second));
        assert!(actions
            .repo
            .find_reference(&format!("refs/alera/native-mixed-reset-backup/{second}"))
            .is_err());
    }

    fn rebase_fixture(conflicting: bool) -> (tempfile::TempDir, GitActions, Oid, Oid) {
        let (dir, actions) = repo_fixture();
        let root = actions.root.clone();
        drop(actions);
        let repo = Repository::open(&root).unwrap();
        repo.config()
            .unwrap()
            .set_str("user.name", "Fixture")
            .unwrap();
        repo.config()
            .unwrap()
            .set_str("user.email", "fixture@example.invalid")
            .unwrap();
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = base.tree().unwrap();
        let mut builder = repo.treebuilder(Some(&tree)).unwrap();
        let target_file = if conflicting { "a.txt" } else { "target.txt" };
        let blob = repo.blob(b"target content").unwrap();
        builder.insert(target_file, blob, 0o100644).unwrap();
        let target_tree = repo.find_tree(builder.write().unwrap()).unwrap();
        let sig = Signature::now("Fixture", "fixture@example.invalid").unwrap();
        let target_oid = repo
            .commit(
                Some("refs/heads/target"),
                &sig,
                &sig,
                "target change",
                &target_tree,
                &[&base],
            )
            .unwrap();
        repo.branch("feature/rebase", &base, false).unwrap();
        repo.set_head("refs/heads/feature/rebase").unwrap();
        let mut previous = base.id();
        for n in 0..2 {
            let name = if conflicting && n == 0 {
                "a.txt".to_owned()
            } else {
                format!("feature{n}.txt")
            };
            fs::write(root.join(&name), format!("feature content {n}")).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new(&name)).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parent = repo.find_commit(previous).unwrap();
            previous = repo
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("feature change {n}"),
                    &tree,
                    &[&parent],
                )
                .unwrap();
        }
        drop(target_tree);
        drop(builder);
        drop(tree);
        drop(base);
        drop(repo);
        (dir, GitActions::open(&root).unwrap(), previous, target_oid)
    }

    #[test]
    fn rebase_preview_and_apply_replay_local_linear_commits_with_backup() {
        let (_dir, actions, old_head, target) = rebase_fixture(false);
        let plan = actions.preview_rebase_onto_branch("target").unwrap();
        assert_eq!(plan.current_oid, old_head.to_string());
        assert_eq!(plan.onto_oid, target.to_string());
        assert_eq!(plan.commits.len(), 2);
        assert_eq!(plan.commits[0].1, "feature change 0");
        assert_eq!(actions.repo.head().unwrap().target(), Some(old_head));
        let status = actions.apply_rebase_preview(&plan).unwrap();
        assert!(status.contains("original tip kept"));
        let updated = actions.repo.head().unwrap().peel_to_commit().unwrap();
        assert_ne!(updated.id(), old_head);
        assert!(actions
            .repo
            .graph_descendant_of(updated.id(), target)
            .unwrap());
        let tree = updated.tree().unwrap();
        for name in ["target.txt", "feature0.txt", "feature1.txt"] {
            assert!(tree.get_name(name).is_some(), "missing {name}");
        }
        let backup = format!("refs/alera/native-rebase-backup/{old_head}");
        assert_eq!(
            actions.repo.find_reference(&backup).unwrap().target(),
            Some(old_head)
        );
        assert_eq!(actions.repo.state(), git2::RepositoryState::Clean);
        assert!(actions.require_clean_worktree().is_ok());
    }

    #[test]
    fn rebase_conflict_aborts_and_keeps_original_branch_and_backup() {
        let (_dir, actions, old_head, _target) = rebase_fixture(true);
        let original_file = fs::read(actions.root.join("a.txt")).unwrap();
        let plan = actions.preview_rebase_onto_branch("target").unwrap();
        assert!(actions.apply_rebase_preview(&plan).is_err());
        assert_eq!(actions.repo.head().unwrap().target(), Some(old_head));
        assert_eq!(fs::read(actions.root.join("a.txt")).unwrap(), original_file);
        assert_eq!(actions.repo.state(), git2::RepositoryState::Clean);
        assert!(actions.require_clean_worktree().is_ok());
        let backup = format!("refs/alera/native-rebase-backup/{old_head}");
        assert_eq!(
            actions.repo.find_reference(&backup).unwrap().target(),
            Some(old_head)
        );
    }

    #[test]
    fn rebase_rejects_changed_target_before_creating_backup() {
        let (_dir, actions, old_head, target) = rebase_fixture(false);
        let plan = actions.preview_rebase_onto_branch("target").unwrap();
        let target_commit = actions.repo.find_commit(target).unwrap();
        let tree = target_commit.tree().unwrap();
        let sig = Signature::now("Fixture", "fixture@example.invalid").unwrap();
        actions
            .repo
            .commit(
                Some("refs/heads/target"),
                &sig,
                &sig,
                "target moved",
                &tree,
                &[&target_commit],
            )
            .unwrap();
        assert!(actions.apply_rebase_preview(&plan).is_err());
        assert_eq!(actions.repo.head().unwrap().target(), Some(old_head));
        assert!(actions
            .repo
            .find_reference(&format!("refs/alera/native-rebase-backup/{old_head}"))
            .is_err());
        assert!(actions.require_clean_worktree().is_ok());
    }

    #[test]
    fn rebase_accepts_published_common_base_but_rejects_protected_current_branch() {
        let (_dir, actions, old_head, target) = rebase_fixture(false);
        let base = actions.repo.merge_base(old_head, target).unwrap();
        actions
            .repo
            .remote("origin", "https://example.invalid/repo.git")
            .unwrap();
        actions
            .repo
            .reference("refs/remotes/origin/feature/rebase", base, true, "fixture")
            .unwrap();
        actions
            .repo
            .config()
            .unwrap()
            .set_str("branch.feature/rebase.remote", "origin")
            .unwrap();
        actions
            .repo
            .config()
            .unwrap()
            .set_str("branch.feature/rebase.merge", "refs/heads/feature/rebase")
            .unwrap();
        let plan = actions.preview_rebase_onto_branch("target").unwrap();
        assert_eq!(plan.upstream_oid, Some(base.to_string()));
        actions
            .repo
            .reference("refs/heads/main", old_head, true, "fixture")
            .unwrap();
        actions.repo.set_head("refs/heads/main").unwrap();
        assert!(actions.preview_rebase_onto_branch("target").is_err());
        assert!(actions.apply_rebase_preview(&plan).is_err());
        assert!(actions
            .repo
            .find_reference(&format!("refs/alera/native-rebase-backup/{old_head}"))
            .is_err());
    }

    #[test]
    fn rebase_rejects_dirty_stale_and_published_replay() {
        let (_dir, actions, old_head, _target) = rebase_fixture(false);
        let plan = actions.preview_rebase_onto_branch("target").unwrap();
        assert!(actions
            .preview_rebase_onto_branch("feature/rebase")
            .is_err());
        assert!(actions
            .preview_rebase_onto_branch("refs/remotes/origin/main")
            .is_err());
        fs::write(actions.root.join("untracked.txt"), "must stay").unwrap();
        assert!(actions.preview_rebase_onto_branch("target").is_err());
        assert!(actions.apply_rebase_preview(&plan).is_err());
        fs::remove_file(actions.root.join("untracked.txt")).unwrap();
        actions
            .repo
            .remote("origin", "https://example.invalid/repo.git")
            .unwrap();
        actions
            .repo
            .reference(
                "refs/remotes/origin/feature/rebase",
                old_head,
                true,
                "fixture",
            )
            .unwrap();
        actions
            .repo
            .config()
            .unwrap()
            .set_str("branch.feature/rebase.remote", "origin")
            .unwrap();
        actions
            .repo
            .config()
            .unwrap()
            .set_str("branch.feature/rebase.merge", "refs/heads/feature/rebase")
            .unwrap();
        assert!(actions.preview_rebase_onto_branch("target").is_err());
        assert!(actions.apply_rebase_preview(&plan).is_err());
        assert_eq!(actions.repo.head().unwrap().target(), Some(old_head));
        assert!(actions
            .repo
            .find_reference(&format!("refs/alera/native-rebase-backup/{old_head}"))
            .is_err());
    }

    #[test]
    fn g01_batch_stage_unstage_includes_staged_deletion_and_preserves_worktree() {
        let (_dir, actions) = repo_fixture();
        let root = actions.root().to_path_buf();
        // Commit a second tracked file, so its deletion is a normal Git
        // staged deletion: the path is *absent* from the current index.
        fs::write(root.join("removed.txt"), "remove from disk").unwrap();
        actions.stage_file(Path::new("removed.txt")).unwrap();
        actions.commit("track second file").unwrap();
        let before_head = actions.repo.head().unwrap().target();
        fs::remove_file(root.join("removed.txt")).unwrap();
        fs::write(root.join("a.txt"), "first saved version").unwrap();
        fs::write(root.join("new.txt"), "untracked contents").unwrap();
        let selection = [
            PathBuf::from("a.txt"),
            PathBuf::from("new.txt"),
            PathBuf::from("removed.txt"),
        ];
        assert_eq!(actions.stage_files(&selection).unwrap(), 3);
        assert!(actions
            .repo
            .index()
            .unwrap()
            .get_path(Path::new("removed.txt"), 0)
            .is_none());
        // A later working-tree edit must remain untouched by Unstage.
        fs::write(root.join("a.txt"), "later disk edit").unwrap();
        let staged = crate::git_inspect::GitSnapshot::load(&root)
            .unwrap()
            .visible_rows("");
        assert!(staged
            .iter()
            .any(|row| row.area == crate::git_inspect::GitChangeArea::Staged
                && row.relative == Path::new("removed.txt")
                && row.label == "D"));
        assert!(staged
            .iter()
            .any(|row| row.area == crate::git_inspect::GitChangeArea::Changes
                && row.relative == Path::new("a.txt")));
        assert_eq!(actions.unstage_files(&selection).unwrap(), 3);
        let remaining = crate::git_inspect::GitSnapshot::load(&root)
            .unwrap()
            .visible_rows("");
        assert!(remaining
            .iter()
            .all(|row| row.area != crate::git_inspect::GitChangeArea::Staged));
        assert!(remaining
            .iter()
            .any(|row| row.area == crate::git_inspect::GitChangeArea::Changes
                && row.relative == Path::new("removed.txt")));
        assert!(remaining.iter().any(|row| row.area
            == crate::git_inspect::GitChangeArea::Untracked
            && row.relative == Path::new("new.txt")));
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "later disk edit"
        );
        assert_eq!(
            fs::read_to_string(root.join("new.txt")).unwrap(),
            "untracked contents"
        );
        assert!(!root.join("removed.txt").exists());
        assert_eq!(actions.repo.head().unwrap().target(), before_head);
    }

    #[test]
    fn g01_unstage_many_rejects_unstaged_selected_row_without_index_change() {
        let (_dir, actions) = repo_fixture();
        let root = actions.root();
        fs::write(root.join("a.txt"), "staged content").unwrap();
        fs::write(root.join("other.txt"), "not staged").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        let before = actions.repo.index().unwrap().write_tree().unwrap();
        assert!(actions
            .unstage_files(&[PathBuf::from("a.txt"), PathBuf::from("other.txt")])
            .is_err());
        assert_eq!(actions.repo.index().unwrap().write_tree().unwrap(), before);
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "staged content"
        );
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
    fn g03_initial_commit_then_history_and_preview_in_new_repo() {
        let temp = tempfile::tempdir().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        repo.config()
            .unwrap()
            .set_str("user.name", "Git QA")
            .unwrap();
        repo.config()
            .unwrap()
            .set_str("user.email", "qa@example.invalid")
            .unwrap();
        assert!(repo.head().is_err(), "fresh repo must have no HEAD commit");
        let root = dunce::canonicalize(temp.path()).unwrap();
        fs::write(root.join("initial.txt"), "first saved content\n").unwrap();
        fs::write(root.join("unstaged.txt"), "private untracked content\n").unwrap();
        drop(repo);
        let actions = GitActions::open(&root).unwrap();
        assert!(actions.commit("empty initial commit").is_err());
        actions.stage_file(Path::new("initial.txt")).unwrap();
        let new_oid = actions.commit("first Native commit").unwrap();
        let head = actions.repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.id().to_string(), new_oid);
        assert_eq!(head.parent_count(), 0);
        assert_eq!(head.message().unwrap(), "first Native commit");
        assert!(head
            .tree()
            .unwrap()
            .get_path(Path::new("initial.txt"))
            .is_ok());
        assert!(head
            .tree()
            .unwrap()
            .get_path(Path::new("unstaged.txt"))
            .is_err());
        let history = crate::git_history::GitHistorySnapshot::load(&root).unwrap();
        assert_eq!(history.commits.len(), 1);
        assert_eq!(history.commits[0].oid, new_oid);
        let preview = crate::git_history::preview_commit(&root, &new_oid).unwrap();
        assert!(preview.contains("first Native commit"), "{preview}");
        assert!(preview.contains("+first saved content"), "{preview}");
        assert_eq!(
            fs::read_to_string(root.join("unstaged.txt")).unwrap(),
            "private untracked content\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("initial.txt")).unwrap(),
            "first saved content\n"
        );
    }

    #[test]
    fn g03_commit_amend_history_readback_preserves_unsaved_disk_changes() {
        let (_temp, actions) = repo_fixture();
        let root = actions.root().to_path_buf();
        fs::write(root.join("a.txt"), "staged version\n").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        fs::write(root.join("a.txt"), "later worktree edit\n").unwrap();
        fs::write(root.join("untracked.txt"), "private not staged\n").unwrap();
        let new_oid = actions.commit("work in progress").unwrap();
        let new_commit = actions
            .repo
            .find_commit(Oid::from_str(&new_oid).unwrap())
            .unwrap();
        assert_eq!(new_commit.parent_count(), 1);
        let blob = new_commit
            .tree()
            .unwrap()
            .get_path(Path::new("a.txt"))
            .unwrap()
            .to_object(&actions.repo)
            .unwrap()
            .peel_to_blob()
            .unwrap();
        assert_eq!(blob.content(), b"staged version\n");
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "later worktree edit\n"
        );
        let preview = actions.preview_amend_commit("finished work").unwrap();
        assert_eq!(preview.head_oid, new_oid);
        actions.apply_amend_preview(&preview).unwrap();
        let amended_oid = actions.repo.head().unwrap().target().unwrap();
        assert_ne!(amended_oid.to_string(), new_oid);
        let history = crate::git_history::GitHistorySnapshot::load(&root).unwrap();
        assert_eq!(history.commits[0].oid, amended_oid.to_string());
        assert_eq!(history.commits[0].subject, "finished work");
        let changed = crate::git_history::preview_commit(&root, &amended_oid.to_string()).unwrap();
        assert!(changed.contains("+staged version"), "{changed}");
        assert!(!changed.contains("later worktree edit"));
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "later worktree edit\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("untracked.txt")).unwrap(),
            "private not staged\n"
        );
    }
    #[test]
    fn offline_commit_draft_uses_staged_changes_only_and_never_moves_head() {
        let (_dir, actions) = repo_fixture();
        let before = actions.repo.head().unwrap().target();
        assert!(actions.draft_commit_message().is_err());
        fs::write(actions.root.join("a.txt"), "after").unwrap();
        fs::write(actions.root.join("private-untracked.txt"), "SECRET").unwrap();
        assert!(actions.draft_commit_message().is_err());
        actions.stage_file(Path::new("a.txt")).unwrap();
        let draft = actions.draft_commit_message().unwrap();
        assert_eq!(draft.text, "Update a.txt");
        assert_eq!(draft.staged_files, 1);
        assert_eq!(draft.head_oid, before.unwrap().to_string());
        assert!(!draft.text.contains("private"));
        assert_eq!(actions.repo.head().unwrap().target(), before);
        assert_eq!(
            fs::read(actions.root.join("private-untracked.txt")).unwrap(),
            b"SECRET"
        );
        fs::write(actions.root.join("second.txt"), "second").unwrap();
        actions.stage_file(Path::new("second.txt")).unwrap();
        let draft = actions.draft_commit_message().unwrap();
        assert_eq!(draft.text, "Update 2 staged files");
        assert_eq!(draft.staged_files, 2);
        assert_eq!(actions.repo.head().unwrap().target(), before);
    }

    #[test]
    fn offline_commit_draft_distinguishes_added_and_deleted_files() {
        let (_dir, actions) = repo_fixture();
        fs::write(actions.root.join("added.txt"), "new").unwrap();
        actions.stage_file(Path::new("added.txt")).unwrap();
        assert_eq!(
            actions.draft_commit_message().unwrap().text,
            "Add added.txt"
        );
        actions.unstage_file(Path::new("added.txt")).unwrap();
        fs::remove_file(actions.root.join("a.txt")).unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        assert_eq!(actions.draft_commit_message().unwrap().text, "Remove a.txt");
    }

    #[test]
    fn trash_preview_is_read_only_and_rejects_staged_symlink_and_stale_data() {
        let (_dir, actions) = repo_fixture();
        let root = actions.root();
        fs::write(root.join("new-note.txt"), "private untracked data").unwrap();
        let plan = actions
            .preview_untracked_trash(&[PathBuf::from("new-note.txt")])
            .unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].size, "private untracked data".len() as u64);
        assert_eq!(
            fs::read_to_string(root.join("new-note.txt")).unwrap(),
            "private untracked data"
        );
        assert!(actions
            .preview_untracked_trash(&[PathBuf::from("a.txt")])
            .is_err());
        assert!(actions
            .preview_untracked_trash(&[PathBuf::from("../escape")])
            .is_err());
        assert!(actions
            .preview_untracked_trash(&[PathBuf::from("missing.txt")])
            .is_err());
        fs::write(root.join("new-note.txt"), "changed after preview").unwrap();
        assert!(actions.apply_untracked_trash(&plan).is_err());
        assert_eq!(
            fs::read_to_string(root.join("new-note.txt")).unwrap(),
            "changed after preview"
        );
        actions.stage_file(Path::new("new-note.txt")).unwrap();
        assert!(actions
            .preview_untracked_trash(&[PathBuf::from("new-note.txt")])
            .is_err());
        assert!(actions.apply_untracked_trash(&plan).is_err());
        assert!(root.join("new-note.txt").exists());
    }

    #[test]
    fn trash_preview_rejects_invalid_batch_without_touching_other_files() {
        let (_dir, actions) = repo_fixture();
        fs::write(actions.root().join("valid.txt"), "keep").unwrap();
        let result =
            actions.preview_untracked_trash(&[PathBuf::from("valid.txt"), PathBuf::from("a.txt")]);
        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(actions.root().join("valid.txt")).unwrap(),
            "keep"
        );
        assert!(actions
            .repo
            .index()
            .unwrap()
            .get_path(Path::new("valid.txt"), 0)
            .is_none());
    }

    #[test]
    fn discard_restores_only_unstaged_tracked_files_and_preserves_index() {
        let (_dir, actions) = repo_fixture();
        let root = actions.root();
        // A file can have BOTH staged and unstaged changes. The discard must
        // retain the staged snapshot rather than resetting to HEAD.
        fs::write(root.join("a.txt"), "staged version").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        fs::write(root.join("a.txt"), "unstaged version").unwrap();
        fs::write(root.join("untracked.txt"), "do not touch").unwrap();
        let staged = actions
            .repo
            .index()
            .unwrap()
            .get_path(Path::new("a.txt"), 0)
            .unwrap()
            .id;
        assert_eq!(
            actions
                .discard_unstaged_tracked(&[PathBuf::from("a.txt")])
                .unwrap(),
            1
        );
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "staged version"
        );
        assert_eq!(
            fs::read_to_string(root.join("untracked.txt")).unwrap(),
            "do not touch"
        );
        assert_eq!(
            actions
                .repo
                .index()
                .unwrap()
                .get_path(Path::new("a.txt"), 0)
                .unwrap()
                .id,
            staged
        );
        assert!(actions
            .repo
            .status_file(Path::new("a.txt"))
            .unwrap()
            .contains(git2::Status::INDEX_MODIFIED));
        assert!(!actions
            .repo
            .status_file(Path::new("a.txt"))
            .unwrap()
            .contains(git2::Status::WT_MODIFIED));
    }

    #[test]
    fn discard_preflight_rejects_entire_batch_on_invalid_target() {
        let (_dir, actions) = repo_fixture();
        let root = actions.root();
        fs::write(root.join("a.txt"), "keep this unstaged edit").unwrap();
        fs::write(root.join("untracked.txt"), "private new file").unwrap();
        for bad in ["untracked.txt", "../outside", ".git/config"] {
            assert!(
                actions
                    .discard_unstaged_tracked(&[PathBuf::from("a.txt"), PathBuf::from(bad)])
                    .is_err(),
                "{bad}"
            );
            assert_eq!(
                fs::read_to_string(root.join("a.txt")).unwrap(),
                "keep this unstaged edit"
            );
            assert_eq!(
                fs::read_to_string(root.join("untracked.txt")).unwrap(),
                "private new file"
            );
        }
        assert!(actions
            .discard_unstaged_tracked(&[PathBuf::from("a.txt")])
            .is_ok());
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "before");
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
    fn amend_requires_exact_head_index_and_replaces_only_local_commit() {
        let (_dir, actions) = repo_fixture();
        let old = actions.repo.head().unwrap().target().unwrap();
        let path = actions.root().join("a.txt");
        fs::write(&path, "amended content").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        let preview = actions
            .preview_amend_commit("replace commit message")
            .unwrap();
        assert_eq!(preview.head_oid, old.to_string());
        assert!(preview.staged_changes);
        assert_eq!(actions.repo.head().unwrap().target(), Some(old));
        let message = actions.apply_amend_preview(&preview).unwrap();
        assert!(message.contains("Amended"));
        let new_oid = actions.repo.head().unwrap().target().unwrap();
        assert_ne!(new_oid, old);
        assert_eq!(
            actions
                .repo
                .find_commit(new_oid)
                .unwrap()
                .message()
                .unwrap(),
            "replace commit message"
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "amended content");
        assert!(actions.repo.statuses(None).unwrap().is_empty());
        assert!(actions.apply_amend_preview(&preview).is_err());
    }

    #[test]
    fn amend_refuses_stale_index_and_no_op() {
        let (_dir, actions) = repo_fixture();
        let original = actions
            .repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message()
            .unwrap()
            .to_owned();
        assert!(actions.preview_amend_commit(&original).is_err());
        assert!(actions.preview_amend_commit("").is_err());
        let preview = actions.preview_amend_commit("message-only amend").unwrap();
        assert!(!preview.staged_changes);
        fs::write(actions.root().join("a.txt"), "changed after preview").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        assert!(actions.apply_amend_preview(&preview).is_err());
        assert_eq!(
            actions.repo.head().unwrap().target().unwrap().to_string(),
            preview.head_oid
        );
    }

    #[test]
    fn two_parent_merge_requires_explicit_mainline_and_keeps_head_untouched_during_preview() {
        let (_dir, actions) = repo_fixture();
        let repo = &actions.repo;
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        let original = base.id();
        let sig = git2::Signature::now("Merge Tester", "merge@example.invalid").unwrap();
        let base_tree = base.tree().unwrap();
        let mut builder = repo.treebuilder(Some(&base_tree)).unwrap();
        let topic_blob = repo.blob(b"topic data\n").unwrap();
        builder
            .insert("topic-file.txt", topic_blob, 0o100644)
            .unwrap();
        let topic_tree = repo.find_tree(builder.write().unwrap()).unwrap();
        let topic_oid = repo
            .commit(
                Some("refs/heads/topic"),
                &sig,
                &sig,
                "topic",
                &topic_tree,
                &[&base],
            )
            .unwrap();
        let topic = repo.find_commit(topic_oid).unwrap();
        let merge_oid = repo
            .commit(
                Some("refs/heads/merge-source"),
                &sig,
                &sig,
                "merge topic",
                &topic_tree,
                &[&base, &topic],
            )
            .unwrap();
        let sha = merge_oid.to_string();
        assert!(actions
            .preview_commit_apply(&sha, GitCommitApplyKind::CherryPick)
            .is_err());
        assert!(actions
            .preview_commit_apply_with_mainline(&sha, GitCommitApplyKind::CherryPick, 3)
            .is_err());
        assert!(actions
            .preview_commit_apply_with_mainline(&sha, GitCommitApplyKind::Revert, 0)
            .is_err());
        let preview = actions
            .preview_commit_apply_with_mainline(&sha, GitCommitApplyKind::CherryPick, 1)
            .unwrap();
        assert_eq!(preview.mainline, 1);
        assert_eq!(repo.head().unwrap().target(), Some(original));
        assert!(!actions.root().join("topic-file.txt").exists());
        assert!(actions.apply_commit_preview(&preview).is_ok());
        assert_eq!(
            fs::read_to_string(actions.root().join("topic-file.txt")).unwrap(),
            "topic data\n"
        );
        let revert = actions
            .preview_commit_apply_with_mainline(&sha, GitCommitApplyKind::Revert, 1)
            .unwrap();
        assert_eq!(revert.mainline, 1);
        assert!(actions.apply_commit_preview(&revert).is_ok());
        assert!(!actions.root().join("topic-file.txt").exists());
        assert!(repo.statuses(None).unwrap().is_empty());
    }

    #[test]
    fn commit_apply_preview_and_confirm_cherry_pick_and_revert() {
        let (_dir, actions) = repo_fixture();
        let base = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        let old_branch = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        actions
            .repo
            .branch("topic", &actions.repo.find_commit(base).unwrap(), false)
            .unwrap();
        actions.switch_branch("topic").unwrap();
        fs::write(actions.root().join("topic.txt"), "topic content\n").unwrap();
        actions.stage_file(Path::new("topic.txt")).unwrap();
        let topic_commit = actions.commit("topic change").unwrap();
        actions.switch_branch(&old_branch).unwrap();
        let initial = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        let cherry = actions
            .preview_commit_apply(&topic_commit, GitCommitApplyKind::CherryPick)
            .unwrap();
        assert_eq!(cherry.current_oid, initial.to_string());
        assert!(cherry.changed_paths.iter().any(|path| path == "topic.txt"));
        assert!(!actions.root().join("topic.txt").exists());
        assert_eq!(actions.repo.head().unwrap().target(), Some(initial));
        actions.apply_commit_preview(&cherry).unwrap();
        let cherry_oid = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        assert_ne!(cherry_oid, initial);
        assert_eq!(
            fs::read_to_string(actions.root().join("topic.txt")).unwrap(),
            "topic content\n"
        );
        let revert = actions
            .preview_commit_apply(&cherry_oid.to_string(), GitCommitApplyKind::Revert)
            .unwrap();
        assert_eq!(actions.repo.head().unwrap().target(), Some(cherry_oid));
        actions.apply_commit_preview(&revert).unwrap();
        assert!(!actions.root().join("topic.txt").exists());
        assert_ne!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            cherry_oid
        );
        assert!(actions.repo.statuses(None).unwrap().is_empty());
    }

    #[test]
    fn commit_apply_conflicts_and_stale_preview_never_overwrite_user_work() {
        let (_dir, actions) = repo_fixture();
        let current_name = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        let head = actions.repo.head().unwrap().peel_to_commit().unwrap();
        actions.repo.branch("conflicting", &head, false).unwrap();
        drop(head);
        actions.switch_branch("conflicting").unwrap();
        fs::write(actions.root().join("a.txt"), "theirs").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        let conflicting = actions.commit("theirs").unwrap();
        actions.switch_branch(&current_name).unwrap();
        fs::write(actions.root().join("a.txt"), "ours").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        actions.commit("ours").unwrap();
        let before_head = actions.repo.head().unwrap().target().unwrap();
        assert!(actions
            .preview_commit_apply(&conflicting, GitCommitApplyKind::CherryPick)
            .is_err());
        assert_eq!(actions.repo.head().unwrap().target(), Some(before_head));
        assert_eq!(
            fs::read_to_string(actions.root().join("a.txt")).unwrap(),
            "ours"
        );
        assert!(actions.repo.statuses(None).unwrap().is_empty());

        let commit = actions.repo.find_commit(before_head).unwrap();
        actions.repo.branch("separate", &commit, false).unwrap();
        drop(commit);
        actions.switch_branch("separate").unwrap();
        fs::write(actions.root().join("extra.txt"), "extra").unwrap();
        actions.stage_file(Path::new("extra.txt")).unwrap();
        let extra = actions.commit("extra").unwrap();
        actions.switch_branch(&current_name).unwrap();
        let stale = actions
            .preview_commit_apply(&extra, GitCommitApplyKind::CherryPick)
            .unwrap();
        fs::write(actions.root().join("a.txt"), "changed again").unwrap();
        actions.stage_file(Path::new("a.txt")).unwrap();
        actions.commit("advance current").unwrap();
        let advanced = actions.repo.head().unwrap().target().unwrap();
        assert!(actions.apply_commit_preview(&stale).is_err());
        assert_eq!(actions.repo.head().unwrap().target(), Some(advanced));
        assert!(!actions.root().join("extra.txt").exists());
        assert_eq!(
            fs::read_to_string(actions.root().join("a.txt")).unwrap(),
            "changed again"
        );
    }

    #[test]
    fn commit_apply_refuses_dirty_stale_and_invalid_preview() {
        let (_dir, actions) = repo_fixture();
        let base = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        assert!(actions
            .preview_commit_apply(&base.to_string(), GitCommitApplyKind::CherryPick)
            .is_err());
        assert!(actions
            .preview_commit_apply("abc", GitCommitApplyKind::Revert)
            .is_err());
        fs::write(actions.root().join("untracked.txt"), "private").unwrap();
        assert!(actions
            .preview_commit_apply(&base.to_string(), GitCommitApplyKind::Revert)
            .is_err());
        assert!(fs::read_to_string(actions.root().join("untracked.txt")).is_ok());
    }

    #[test]
    fn create_ref_at_commit_does_not_move_head_or_working_files() {
        let (_dir, actions) = repo_fixture();
        let head = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        let original = fs::read(actions.root().join("a.txt")).unwrap();
        let sha = head.to_string();
        let branch = actions
            .create_ref_at_commit(GitCommitRefKind::LocalBranch, &sha, "from-graph/topic")
            .unwrap();
        assert!(branch.contains("local branch"));
        assert_eq!(
            actions
                .repo
                .find_reference("refs/heads/from-graph/topic")
                .unwrap()
                .target(),
            Some(head)
        );
        let tag = actions
            .create_ref_at_commit(GitCommitRefKind::LightweightTag, &sha, "v1.0.0")
            .unwrap();
        assert!(tag.contains("lightweight tag"));
        assert_eq!(
            actions
                .repo
                .find_reference("refs/tags/v1.0.0")
                .unwrap()
                .target(),
            Some(head)
        );
        assert_eq!(actions.repo.head().unwrap().target(), Some(head));
        assert_eq!(fs::read(actions.root().join("a.txt")).unwrap(), original);
    }

    #[test]
    fn create_ref_at_commit_refuses_existing_unsafe_or_noncommit_target() {
        let (_dir, actions) = repo_fixture();
        let oid = actions.repo.head().unwrap().target().unwrap().to_string();
        actions
            .create_ref_at_commit(GitCommitRefKind::LocalBranch, &oid, "created")
            .unwrap();
        for kind in [
            GitCommitRefKind::LocalBranch,
            GitCommitRefKind::LightweightTag,
        ] {
            for name in [
                "",
                "../escape",
                "evil..branch",
                "-option",
                "bad name",
                "refs/heads/a.lock",
                "a//b",
            ] {
                assert!(
                    actions.create_ref_at_commit(kind, &oid, name).is_err(),
                    "{name}"
                );
            }
        }
        assert!(actions
            .create_ref_at_commit(GitCommitRefKind::LocalBranch, &oid, "created")
            .is_err());
        assert!(actions
            .create_ref_at_commit(GitCommitRefKind::LightweightTag, &oid, "v0")
            .is_ok());
        assert!(actions
            .create_ref_at_commit(GitCommitRefKind::LightweightTag, &oid, "v0")
            .is_err());
        assert!(actions
            .create_ref_at_commit(GitCommitRefKind::LocalBranch, "0123", "new")
            .is_err());
        assert!(actions
            .create_ref_at_commit(GitCommitRefKind::LightweightTag, &"0".repeat(40), "new")
            .is_err());
    }

    #[test]
    fn inactive_branch_rename_preserves_worktree_head_and_does_not_overwrite() {
        let (_dir, actions) = repo_fixture();
        let current = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        let head_id = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        let head_commit = actions.repo.find_commit(head_id).unwrap();
        actions.repo.branch("source", &head_commit, false).unwrap();
        actions
            .repo
            .branch("existing", &head_commit, false)
            .unwrap();
        assert!(actions
            .rename_inactive_branch("source", "existing")
            .is_err());
        assert!(actions
            .rename_inactive_branch("source", "../traversal")
            .is_err());
        assert!(actions
            .rename_inactive_branch(&current, "renamed-current")
            .is_err());
        assert!(actions
            .repo
            .find_branch("source", BranchType::Local)
            .is_ok());
        assert!(actions
            .rename_inactive_branch("source", "renamed-source")
            .is_ok());
        assert!(actions
            .repo
            .find_branch("source", BranchType::Local)
            .is_err());
        assert!(actions
            .repo
            .find_branch("renamed-source", BranchType::Local)
            .is_ok());
        assert_eq!(actions.repo.head().unwrap().shorthand().unwrap(), current);
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            head_id
        );
    }

    #[test]
    fn current_branch_rename_preserves_head_and_refuses_other_worktree_ownership() {
        let (_dir, actions) = repo_fixture();
        let prior = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        let oid = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        assert!(actions
            .rename_owned_branch("not-current", "renamed")
            .is_err());
        assert!(actions.rename_owned_branch(&prior, "../unsafe").is_err());
        assert!(actions
            .rename_owned_branch(&prior, "renamed-current")
            .is_ok());
        assert_eq!(
            actions.repo.head().unwrap().shorthand().unwrap(),
            "renamed-current"
        );
        assert_eq!(actions.repo.head().unwrap().target(), Some(oid));
        assert!(actions.repo.find_branch(&prior, BranchType::Local).is_err());
        assert!(actions.rename_owned_branch(&prior, "again").is_err());
        assert_eq!(
            fs::read_to_string(actions.root().join("a.txt")).unwrap(),
            "before"
        );
    }

    #[test]
    fn linked_worktree_branch_rename_only_from_owning_checkout() {
        let (_dir, actions) = repo_fixture();
        let manager = crate::worktree_manager::WorktreeManager::discover(actions.root()).unwrap();
        let current = actions.repo.head().unwrap().shorthand().unwrap().to_owned();
        let original = actions.repo.head().unwrap().target();
        let linked = manager.create("linked-owned").unwrap();
        assert!(actions
            .rename_owned_branch("linked-owned", "not-allowed")
            .is_err());
        let owner = GitActions::open(&linked).unwrap();
        assert!(owner
            .rename_owned_branch("linked-owned", "linked-renamed")
            .is_ok());
        assert_eq!(
            owner.repo.head().unwrap().shorthand().unwrap(),
            "linked-renamed"
        );
        assert!(actions
            .repo
            .find_branch("linked-renamed", BranchType::Local)
            .is_ok());
        assert!(actions
            .repo
            .find_branch("linked-owned", BranchType::Local)
            .is_err());
        assert_eq!(actions.repo.head().unwrap().target(), original);
        assert_eq!(actions.repo.head().unwrap().shorthand().unwrap(), current);
        assert!(
            manager
                .list()
                .unwrap()
                .iter()
                .any(|entry| entry.path == linked
                    && entry.branch.as_deref() == Some("linked-renamed"))
        );
        drop(owner);
        manager.remove(&linked, true).unwrap();
    }

    #[test]
    fn branch_rename_refuses_linked_worktree_owner() {
        let (_dir, actions) = repo_fixture();
        let manager = crate::worktree_manager::WorktreeManager::discover(actions.root()).unwrap();
        let linked = manager.create("owned").unwrap();
        assert!(actions.rename_inactive_branch("owned", "renamed").is_err());
        assert!(linked.exists());
        assert!(actions.repo.find_branch("owned", BranchType::Local).is_ok());
        manager.remove(&linked, true).unwrap();
    }

    #[test]
    fn safe_branch_delete_only_allows_merged_inactive_local() {
        let (_dir, actions) = repo_fixture();
        let repo = &actions.repo;
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let current_id = head.id();
        let active_name = repo.head().unwrap().shorthand().unwrap().to_owned();
        repo.branch("merged-safe", &head, false).unwrap();
        repo.branch("develop", &head, false).unwrap();
        assert!(actions.delete_merged_inactive_branch("develop").is_err());
        assert!(actions.delete_merged_inactive_branch(&active_name).is_err());
        assert!(actions.delete_merged_inactive_branch("../escape").is_err());
        assert!(actions.delete_merged_inactive_branch("merged-safe").is_ok());
        assert!(repo.find_branch("merged-safe", BranchType::Local).is_err());
        assert_eq!(repo.head().unwrap().target(), Some(current_id));
        assert_eq!(repo.head().unwrap().shorthand().unwrap(), active_name);

        // A unique branch commit not reachable from current HEAD is protected.
        let sig = git2::Signature::now("Test", "test@example.invalid").unwrap();
        let next = repo
            .commit(
                Some("refs/heads/unmerged-safe"),
                &sig,
                &sig,
                "unique commit",
                &head.tree().unwrap(),
                &[&head],
            )
            .unwrap();
        assert_ne!(next, current_id);
        assert!(actions
            .delete_merged_inactive_branch("unmerged-safe")
            .is_err());
        assert!(repo.find_branch("unmerged-safe", BranchType::Local).is_ok());
        assert_eq!(repo.head().unwrap().target(), Some(current_id));
    }

    #[test]
    fn safe_branch_delete_refuses_any_worktree_owner() {
        let (_dir, actions) = repo_fixture();
        let manager = crate::worktree_manager::WorktreeManager::discover(actions.root()).unwrap();
        let linked = manager.create("occupied-delete").unwrap();
        assert!(actions
            .delete_merged_inactive_branch("occupied-delete")
            .is_err());
        assert!(actions
            .repo
            .find_branch("occupied-delete", BranchType::Local)
            .is_ok());
        manager.remove(&linked, true).unwrap();
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
    fn selected_branch_fetch_fast_forwards_ref_without_switching_or_touching_files() {
        let (work, remote, actions, current) = bare_remote_fixture();
        let original = actions.repo.head().unwrap().peel_to_commit().unwrap().id();
        actions
            .repo
            .branch("topic", &actions.repo.find_commit(original).unwrap(), false)
            .unwrap();
        git_in_dir(
            actions.root(),
            &["push", "-u", "origin", "refs/heads/topic"],
        )
        .unwrap();
        let clone_dir = tempfile::tempdir().unwrap();
        let clone = clone_dir.path().join("clone");
        git_in_dir(
            clone_dir.path(),
            &[
                "clone",
                remote.path().to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        )
        .unwrap();
        git_in_dir(
            &clone,
            &["checkout", "-b", "topic", "--track", "origin/topic"],
        )
        .unwrap();
        let clone_actions = GitActions::open(&dunce::canonicalize(&clone).unwrap()).unwrap();
        let upstream_oid =
            fixture_commit(&clone_actions, "topic.txt", "branch update", "topic new");
        git_in_dir(&clone, &["push", "origin", "refs/heads/topic"]).unwrap();
        let before_file = fs::read(work.path().join("a.txt")).unwrap();
        assert!(
            actions.pull_branch_without_switch(&current).is_err(),
            "checked-out current branch must not be advanced without updating its worktree"
        );
        assert!(actions
            .pull_branch_without_switch("topic")
            .unwrap()
            .contains("without switching"));
        assert_eq!(
            actions
                .repo
                .find_branch("topic", BranchType::Local)
                .unwrap()
                .get()
                .peel_to_commit()
                .unwrap()
                .id(),
            upstream_oid
        );
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            original
        );
        assert_eq!(actions.repo.head().unwrap().shorthand().unwrap(), current);
        assert_eq!(fs::read(work.path().join("a.txt")).unwrap(), before_file);
        assert!(!work.path().join("topic.txt").exists());
    }

    #[test]
    fn selected_branch_push_uses_exact_upstream_and_never_moves_head() {
        let (work, remote, actions, _current) = bare_remote_fixture();
        let head = actions.repo.head().unwrap().peel_to_commit().unwrap();
        let original = head.id();
        actions.repo.branch("topic", &head, false).unwrap();
        git_in_dir(
            actions.root(),
            &["push", "-u", "origin", "refs/heads/topic"],
        )
        .unwrap();
        let tree = head.tree().unwrap();
        let signature = Signature::now("Test", "test@example.invalid").unwrap();
        let new = actions
            .repo
            .commit(None, &signature, &signature, "topic only", &tree, &[&head])
            .unwrap();
        actions
            .repo
            .find_reference("refs/heads/topic")
            .unwrap()
            .set_target(new, "test fixture advances non-checked-out topic")
            .unwrap();
        assert!(actions
            .push_branch_without_switch("topic")
            .unwrap()
            .contains("without switching"));
        assert_eq!(
            Repository::open_bare(remote.path())
                .unwrap()
                .find_reference("refs/heads/topic")
                .unwrap()
                .target()
                .unwrap(),
            new
        );
        assert_eq!(
            actions.repo.head().unwrap().peel_to_commit().unwrap().id(),
            original
        );
        assert_eq!(
            fs::read_to_string(work.path().join("a.txt")).unwrap(),
            "before"
        );
        assert!(actions.push_branch_without_switch("../unsafe").is_err());
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
