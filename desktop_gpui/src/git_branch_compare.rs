//! Read-only, structured local-branch comparison for the Native Git Graph.
//! Compare immutable committed trees, either merge-base -> target (sidebar)
//! or current tip -> selected tip (central Graph). Never touch the worktree.
use crate::git_inspect::{GitChangeArea, GitOriginalSnapshot};
use git2::{BranchType, Delta, DiffFindOptions, DiffFormat, DiffOptions, Oid, Repository};
use std::path::{Component, Path, PathBuf};

const MAX_FILES: usize = 128;
const MAX_PREVIEW_BYTES: usize = 48 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareDirection {
    /// Source: current branch; target: selected branch.
    CurrentToSelected,
    /// Source: selected branch; target: current branch.
    SelectedToCurrent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonBasis {
    MergeBase,
    BranchTips,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparedFile {
    pub old_path: Option<PathBuf>,
    pub new_path: Option<PathBuf>,
    pub status: &'static str,
}

impl ComparedFile {
    pub fn display_path(&self) -> String {
        match (&self.old_path, &self.new_path) {
            (Some(old), Some(new)) if old != new => {
                format!("{} → {}", old.display(), new.display())
            }
            (Some(path), _) | (_, Some(path)) => path.display().to_string(),
            (None, None) => "(unknown)".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BranchFileComparison {
    pub current_branch: String,
    pub selected_branch: String,
    pub direction: CompareDirection,
    pub basis: ComparisonBasis,
    pub merge_base_oid: String,
    pub source_oid: String,
    pub source_branch: String,
    pub target_oid: String,
    pub target_branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub files: Vec<ComparedFile>,
}

impl BranchFileComparison {
    pub fn title(&self) -> String {
        format!(
            "{} -> {} ({})",
            self.source_branch,
            self.target_branch,
            match self.basis {
                ComparisonBasis::MergeBase => "merge-base..target",
                ComparisonBasis::BranchTips => "committed tips",
            }
        )
    }

    pub fn summary(&self) -> String {
        format!(
            "Compare: {}\nMerge base: {}\nSource: {}\nTarget: {}\nTarget: {} ahead / {} behind\nChanged files: {}\nRead only - no branch switch",
            self.title(), self.merge_base_oid, self.source_oid, self.target_oid,
            self.ahead, self.behind, self.files.len()
        )
    }
}

fn valid_branch(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('-')
        && !name.contains(['\n', '\r', '\0', ':', '~', '^', '?', '*', '[', '\\'])
        && !name.contains("..")
        && !name.contains("@{")
        && name.split('/').all(|part| !part.is_empty() && part != ".")
}

fn status(delta: Delta) -> &'static str {
    match delta {
        Delta::Added => "A",
        Delta::Deleted => "D",
        Delta::Modified => "M",
        Delta::Renamed => "R",
        Delta::Copied => "C",
        Delta::Typechange => "T",
        Delta::Untracked => "?",
        Delta::Ignored => "I",
        Delta::Unreadable => "!",
        Delta::Conflicted => "!",
        Delta::Unmodified => " ",
    }
}

fn repo_at(root: &Path) -> Result<Repository, String> {
    let repo = Repository::discover(root).map_err(|error| error.to_string())?;
    let workdir = repo.workdir().ok_or("Not a Git working tree")?;
    let actual = dunce::canonicalize(workdir).map_err(|error| error.to_string())?;
    let requested = dunce::canonicalize(root).map_err(|error| error.to_string())?;
    if actual != requested {
        return Err("Comparison root must be the active Git worktree".into());
    }
    Ok(repo)
}

pub fn compare_branch_files(
    root: &Path,
    selected_branch: &str,
    direction: CompareDirection,
) -> Result<BranchFileComparison, String> {
    compare_branch_files_with_basis(root, selected_branch, direction, ComparisonBasis::MergeBase)
}

/// Compare both complete branch tips. Swapping direction reverses additions,
/// deletions, and patch signs, unlike independent merge-base comparisons.
pub fn compare_branch_trees(
    root: &Path,
    selected_branch: &str,
    direction: CompareDirection,
) -> Result<BranchFileComparison, String> {
    compare_branch_files_with_basis(
        root,
        selected_branch,
        direction,
        ComparisonBasis::BranchTips,
    )
}

fn compare_branch_files_with_basis(
    root: &Path,
    selected_branch: &str,
    direction: CompareDirection,
    basis: ComparisonBasis,
) -> Result<BranchFileComparison, String> {
    let (name, kind) = if basis == ComparisonBasis::BranchTips {
        selected_branch
            .strip_prefix("refs/remotes/")
            .map_or((selected_branch, BranchType::Local), |name| {
                (name, BranchType::Remote)
            })
    } else {
        (selected_branch, BranchType::Local)
    };
    if !valid_branch(name) {
        return Err("Invalid branch for Git comparison".into());
    }
    let repo = repo_at(root)?;
    let head = repo.head().map_err(|error| error.to_string())?;
    if !head.is_branch() {
        return Err("Git comparison requires a checked-out branch".into());
    }
    let current_name = head
        .shorthand()
        .map_err(|error| error.to_string())?
        .to_owned();
    let current = head.peel_to_commit().map_err(|error| error.to_string())?;
    let selected = repo
        .find_branch(name, kind)
        .map_err(|_| "Selected branch is unavailable".to_owned())?
        .get()
        .peel_to_commit()
        .map_err(|error| error.to_string())?;
    let common = repo
        .merge_base(current.id(), selected.id())
        .map_err(|_| "Selected branch and current branch have no merge-base".to_owned())?;
    let display_name = name.to_owned();
    let (source, target, source_name, target_name) = match direction {
        CompareDirection::CurrentToSelected => (
            &current,
            &selected,
            current_name.clone(),
            display_name.clone(),
        ),
        CompareDirection::SelectedToCurrent => {
            (&selected, &current, display_name, current_name.clone())
        }
    };
    let (ahead, behind) = repo
        .graph_ahead_behind(target.id(), source.id())
        .map_err(|error| error.to_string())?;
    let base_tree = repo
        .find_commit(common)
        .and_then(|commit| commit.tree())
        .map_err(|error| error.to_string())?;
    let source_tree = if basis == ComparisonBasis::BranchTips {
        source.tree().map_err(|error| error.to_string())?
    } else {
        base_tree
    };
    let target_tree = target.tree().map_err(|error| error.to_string())?;
    let mut diff = repo
        .diff_tree_to_tree(Some(&source_tree), Some(&target_tree), None)
        .map_err(|error| error.to_string())?;
    // Rename detection is bounded; large renames remain visible as add/delete.
    if diff.deltas().len() <= 512 {
        let mut options = DiffFindOptions::new();
        options.renames(true).rename_limit(200);
        diff.find_similar(Some(&mut options))
            .map_err(|error| error.to_string())?;
    }
    if diff.deltas().len() > MAX_FILES {
        return Err("Too many changed files for Native comparison (limit 128)".into());
    }
    let files = diff
        .deltas()
        .map(|item| ComparedFile {
            old_path: item.old_file().path().map(Path::to_path_buf),
            new_path: item.new_file().path().map(Path::to_path_buf),
            status: status(item.status()),
        })
        .collect();
    Ok(BranchFileComparison {
        current_branch: current_name,
        selected_branch: selected_branch.to_owned(),
        direction,
        basis,
        merge_base_oid: common.to_string(),
        source_oid: if basis == ComparisonBasis::MergeBase {
            common.to_string()
        } else {
            source.id().to_string()
        },
        source_branch: if basis == ComparisonBasis::MergeBase {
            "merge-base".to_owned()
        } else {
            source_name
        },
        target_oid: target.id().to_string(),
        target_branch: target_name,
        ahead,
        behind,
        files,
    })
}

fn safe_git_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn bounded_text(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_PREVIEW_BYTES {
        return Ok(format!(
            "Git object exceeds {} KiB; preview unavailable",
            MAX_PREVIEW_BYTES / 1024
        ));
    }
    if bytes.contains(&0) {
        return Ok("Binary Git object; text preview unavailable".into());
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| "Git object is not UTF-8 text".to_string())?;
    Ok(text.to_owned())
}

/// Read the immutable source-side blob, never the working disk file.
pub fn original_compared_file(
    root: &Path,
    comparison: &BranchFileComparison,
    index: usize,
) -> Result<String, String> {
    let selected = comparison
        .files
        .get(index)
        .ok_or("Comparison file index is stale")?;
    let path = selected
        .old_path
        .as_deref()
        .or(selected.new_path.as_deref())
        .ok_or("Comparison has no source path")?;
    if !safe_git_path(path) {
        return Err("Unsafe Git comparison path".into());
    }
    let repo = repo_at(root)?;
    let source = repo
        .find_commit(
            Oid::from_str(&comparison.source_oid).map_err(|_| "Invalid comparison source ID")?,
        )
        .and_then(|commit| commit.tree())
        .map_err(|error| error.to_string())?;
    let entry = match source.get_path(path) {
        Ok(entry) => entry,
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            return Ok(format!(
                "{}\n(New file; no original at comparison source)",
                path.display()
            ))
        }
        Err(error) => return Err(error.to_string()),
    };
    let blob = repo
        .find_blob(entry.id())
        .map_err(|_| "Original is not a regular Git blob".to_owned())?;
    Ok(format!(
        "Original at {}: {}\n\n{}",
        comparison.source_branch,
        path.display(),
        bounded_text(blob.content())?
    ))
}

/// Open the exact source-side committed blob as an immutable Native Git
/// Original tab. Unlike a textual comparison preview, this preserves a
/// selectable tab and never reads or saves the current worktree file.
pub fn open_compared_original_snapshot(
    root: &Path,
    comparison: &BranchFileComparison,
    index: usize,
) -> Result<GitOriginalSnapshot, String> {
    let item = comparison
        .files
        .get(index)
        .ok_or("Comparison file selection is stale")?;
    let path = item
        .old_path
        .as_deref()
        .or(item.new_path.as_deref())
        .ok_or("Comparison has no original path")?;
    if !safe_git_path(path) {
        return Err("Unsafe comparison original path".into());
    }
    if comparison.source_oid.len() != 40
        || !comparison
            .source_oid
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Invalid comparison source commit ID".into());
    }
    let repo = repo_at(root)?;
    let source = repo
        .find_commit(Oid::from_str(&comparison.source_oid).map_err(|e| e.to_string())?)
        .and_then(|commit| commit.tree())
        .map_err(|error| error.to_string())?;
    let entry = source
        .get_path(path)
        .map_err(|_| "New file has no comparison source original to open".to_owned())?;
    let blob = repo
        .find_blob(entry.id())
        .map_err(|_| "Comparison source original is not a regular Git blob".to_owned())?;
    if blob.content().len() > 32 * 1024 {
        return Err("Comparison source original exceeds read-only tab size limit".into());
    }
    if blob.is_binary() || blob.content().contains(&0) {
        return Err("Binary comparison source original cannot open as text".into());
    }
    let content = std::str::from_utf8(blob.content())
        .map_err(|_| "Comparison source original is not UTF-8 text".to_owned())?
        .to_owned();
    Ok(GitOriginalSnapshot {
        relative: path.to_path_buf(),
        area: GitChangeArea::Staged,
        blob_oid: entry.id().to_string(),
        content,
        revision_label: Some(format!(
            "{} {}",
            if comparison.basis == ComparisonBasis::MergeBase {
                "MERGE BASE"
            } else {
                "SOURCE"
            },
            &comparison.source_oid[..8]
        )),
    })
}
/// Show the patch for a selected file using the captured immutable Git trees.
/// The result is bounded and never operates on the current index/workdir.
pub fn diff_compared_file(
    root: &Path,
    comparison: &BranchFileComparison,
    index: usize,
) -> Result<String, String> {
    let selected = comparison
        .files
        .get(index)
        .ok_or("Comparison file index is stale")?;
    for path in [selected.old_path.as_deref(), selected.new_path.as_deref()]
        .into_iter()
        .flatten()
    {
        if !safe_git_path(path) {
            return Err("Unsafe Git comparison path".into());
        }
    }
    let repo = repo_at(root)?;
    let source = repo
        .find_commit(
            Oid::from_str(&comparison.source_oid).map_err(|_| "Invalid comparison source ID")?,
        )
        .and_then(|commit| commit.tree())
        .map_err(|error| error.to_string())?;
    let target = repo
        .find_commit(
            Oid::from_str(&comparison.target_oid).map_err(|_| "Invalid comparison target ID")?,
        )
        .and_then(|commit| commit.tree())
        .map_err(|error| error.to_string())?;
    let mut options = DiffOptions::new();
    options.context_lines(3);
    let mut diff = repo
        .diff_tree_to_tree(Some(&source), Some(&target), Some(&mut options))
        .map_err(|error| error.to_string())?;
    if diff.deltas().len() <= 512 {
        let mut find = DiffFindOptions::new();
        find.renames(true).rename_limit(200);
        diff.find_similar(Some(&mut find))
            .map_err(|error| error.to_string())?;
    }
    // Filter the final full-tree diff by exact delta identity, not pathspec.
    // Pathspec loses rename identity and can inadvertently combine two files.
    let same_file = |old: Option<&Path>, new: Option<&Path>, delta_status: Delta| {
        selected.old_path.as_deref() == old
            && selected.new_path.as_deref() == new
            && selected.status == status(delta_status)
    };
    if !diff.deltas().any(|item| {
        same_file(
            item.old_file().path(),
            item.new_file().path(),
            item.status(),
        )
    }) {
        return Err("Comparison file changed; refresh before preview".into());
    }
    let mut result = format!(
        "{}\n{} [{}]\n\n",
        comparison.title(),
        selected.display_path(),
        selected.status
    );
    let mut truncated = false;
    diff.print(DiffFormat::Patch, |delta, _, line| {
        if !same_file(
            delta.old_file().path(),
            delta.new_file().path(),
            delta.status(),
        ) {
            return true;
        }
        if result.len() >= MAX_PREVIEW_BYTES {
            truncated = true;
            return true;
        }
        if matches!(line.origin(), '+' | '-' | ' ') {
            result.push(line.origin());
        }
        let content = String::from_utf8_lossy(line.content());
        let room = MAX_PREVIEW_BYTES.saturating_sub(result.len());
        let mut boundary = room.min(content.len());
        while !content.is_char_boundary(boundary) {
            boundary -= 1;
        }
        result.push_str(&content[..boundary]);
        truncated |= boundary < content.len();
        true
    })
    .map_err(|error| error.to_string())?;
    if truncated {
        result.push_str("\n... preview truncated");
    }
    Ok(result)
}

#[cfg(test)]
#[path = "git_branch_compare_tests.rs"]
mod tests;
