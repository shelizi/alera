//! Read-only, structured local-branch comparison for the Native Git Graph.
//! A comparison always uses merge-base -> selected target. It never switches
//! HEAD, touches the index, or reads uncommitted working tree contents.
use git2::{BranchType, Delta, DiffFindOptions, DiffFormat, DiffOptions, Oid, Repository};
use std::path::{Component, Path, PathBuf};

const MAX_FILES: usize = 128;
const MAX_PREVIEW_BYTES: usize = 48 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareDirection {
    /// Changes introduced on the selected branch since the common ancestor.
    CurrentToSelected,
    /// Changes introduced on the current branch since the common ancestor.
    SelectedToCurrent,
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

    fn preview_path(&self) -> Result<&Path, String> {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .ok_or_else(|| "No path for Git comparison item".to_owned())
    }
}

#[derive(Debug, Clone)]
pub struct BranchFileComparison {
    pub current_branch: String,
    pub selected_branch: String,
    pub direction: CompareDirection,
    pub merge_base_oid: String,
    pub target_oid: String,
    pub target_branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub files: Vec<ComparedFile>,
}

impl BranchFileComparison {
    pub fn title(&self) -> String {
        format!(
            "{} → {} (merge-base..target)",
            match self.direction {
                CompareDirection::CurrentToSelected => &self.current_branch,
                CompareDirection::SelectedToCurrent => &self.selected_branch,
            },
            self.target_branch
        )
    }
    pub fn summary(&self) -> String {
        format!("Compare: {}\nMerge base: {}\nTarget: {}\nTarget: {} ahead / {} behind\nChanged files: {}\nRead only · no branch switch",
            self.title(), self.merge_base_oid, self.target_oid,
            self.ahead, self.behind, self.files.len())
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
    if !valid_branch(selected_branch) {
        return Err("Invalid local branch for Git comparison".into());
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
        .find_branch(selected_branch, BranchType::Local)
        .map_err(|_| "Selected local branch is unavailable".to_owned())?
        .get()
        .peel_to_commit()
        .map_err(|error| error.to_string())?;
    let common = repo
        .merge_base(current.id(), selected.id())
        .map_err(|_| "Selected branch and current branch have no merge-base".to_owned())?;
    let (target, other, target_name) = match direction {
        CompareDirection::CurrentToSelected => (&selected, &current, selected_branch.to_owned()),
        CompareDirection::SelectedToCurrent => (&current, &selected, current_name.clone()),
    };
    let (ahead, behind) = repo
        .graph_ahead_behind(target.id(), other.id())
        .map_err(|error| error.to_string())?;
    let base_tree = repo
        .find_commit(common)
        .and_then(|commit| commit.tree())
        .map_err(|error| error.to_string())?;
    let target_tree = target.tree().map_err(|error| error.to_string())?;
    let mut diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&target_tree), None)
        .map_err(|error| error.to_string())?;
    // Rename detection is bounded; a rename still remains correctly visible
    // as add/delete if it is too large to analyze.
    if diff.deltas().len() <= 512 {
        let mut options = DiffFindOptions::new();
        options.renames(true).rename_limit(200);
        diff.find_similar(Some(&mut options))
            .map_err(|error| error.to_string())?;
    }
    if diff.deltas().len() > MAX_FILES {
        return Err("Too many changed files for Native comparison (limit 128)".into());
    }
    let mut files = Vec::new();
    for item in diff.deltas() {
        let old_path = item.old_file().path().map(Path::to_path_buf);
        let new_path = item.new_file().path().map(Path::to_path_buf);
        files.push(ComparedFile {
            old_path,
            new_path,
            status: status(item.status()),
        });
    }
    Ok(BranchFileComparison {
        current_branch: current_name,
        selected_branch: selected_branch.to_owned(),
        direction,
        merge_base_oid: common.to_string(),
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

/// Read the original committed blob from the exact merge-base tree, never the
/// working disk file. An absent base blob means the file was newly added.
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
    let base = repo
        .find_commit(
            Oid::from_str(&comparison.merge_base_oid).map_err(|_| "Invalid comparison base ID")?,
        )
        .and_then(|commit| commit.tree())
        .map_err(|error| error.to_string())?;
    let entry = match base.get_path(path) {
        Ok(entry) => entry,
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            return Ok(format!(
                "{}\n(New file; no original at merge-base)",
                path.display()
            ))
        }
        Err(error) => return Err(error.to_string()),
    };
    let blob = repo
        .find_blob(entry.id())
        .map_err(|_| "Original is not a regular Git blob".to_owned())?;
    Ok(format!(
        "Original at merge-base: {}\n\n{}",
        path.display(),
        bounded_text(blob.content())?
    ))
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
    let path = selected.preview_path()?;
    if !safe_git_path(path)
        || selected
            .old_path
            .as_deref()
            .is_some_and(|p| !safe_git_path(p))
    {
        return Err("Unsafe Git comparison path".into());
    }
    let repo = repo_at(root)?;
    let base = repo
        .find_commit(
            Oid::from_str(&comparison.merge_base_oid).map_err(|_| "Invalid comparison base ID")?,
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
    options.pathspec(path);
    if let Some(old_path) = &selected.old_path {
        if old_path != path {
            options.pathspec(old_path);
        }
    }
    let diff = repo
        .diff_tree_to_tree(Some(&base), Some(&target), Some(&mut options))
        .map_err(|error| error.to_string())?;
    let mut result = format!("{}\n{}\n\n", comparison.title(), selected.display_path());
    diff.print(DiffFormat::Patch, |_, _, line| {
        if result.len() < MAX_PREVIEW_BYTES {
            if matches!(line.origin(), '+' | '-' | ' ') {
                result.push(line.origin());
            }
            let content = String::from_utf8_lossy(line.content());
            let room = MAX_PREVIEW_BYTES.saturating_sub(result.len());
            result.push_str(&content.chars().take(room).collect::<String>());
        }
        true
    })
    .map_err(|error| error.to_string())?;
    if result.len() >= MAX_PREVIEW_BYTES {
        result.push_str("\n… preview truncated");
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};

    fn commit_file(repo: &Repository, root: &Path, path: &str, contents: &str, title: &str) {
        std::fs::write(root.join(path), contents).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(path)).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let author = Signature::now("Test", "test@example.invalid").unwrap();
        if let Some(parent) = &parent {
            repo.commit(Some("HEAD"), &author, &author, title, &tree, &[parent])
                .unwrap();
        } else {
            repo.commit(Some("HEAD"), &author, &author, title, &tree, &[])
                .unwrap();
        }
        index.write().unwrap();
    }

    #[test]
    fn directional_comparison_only_reads_committed_tree_contents() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        commit_file(&repo, dir.path(), "shared.txt", "base\n", "base");
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        let current_name = repo.head().unwrap().shorthand().unwrap().to_owned();
        repo.branch("topic", &base, false).unwrap();
        drop(base);
        commit_file(
            &repo,
            dir.path(),
            "shared.txt",
            "current\n",
            "current change",
        );
        let current_id = repo.head().unwrap().target().unwrap();
        repo.set_head("refs/heads/topic").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        commit_file(&repo, dir.path(), "shared.txt", "topic\n", "topic change");
        let topic_id = repo.head().unwrap().target().unwrap();
        repo.set_head(&format!("refs/heads/{current_name}"))
            .unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        std::fs::write(dir.path().join("shared.txt"), "uncommitted secret\n").unwrap();
        let current_to_selected =
            compare_branch_files(dir.path(), "topic", CompareDirection::CurrentToSelected).unwrap();
        assert_eq!(current_to_selected.target_oid, topic_id.to_string());
        assert_eq!(current_to_selected.files.len(), 1);
        assert_eq!(current_to_selected.ahead, 1);
        assert_eq!(current_to_selected.behind, 1);
        assert!(diff_compared_file(dir.path(), &current_to_selected, 0)
            .unwrap()
            .contains("+topic"));
        assert!(!diff_compared_file(dir.path(), &current_to_selected, 0)
            .unwrap()
            .contains("uncommitted secret"));
        assert!(original_compared_file(dir.path(), &current_to_selected, 0)
            .unwrap()
            .contains("base\n"));
        let selected_to_current =
            compare_branch_files(dir.path(), "topic", CompareDirection::SelectedToCurrent).unwrap();
        assert_eq!(selected_to_current.target_oid, current_id.to_string());
        assert!(diff_compared_file(dir.path(), &selected_to_current, 0)
            .unwrap()
            .contains("+current"));
        assert_eq!(repo.head().unwrap().target().unwrap(), current_id);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
            "uncommitted secret\n"
        );
    }

    #[test]
    fn invalid_names_paths_and_stale_indices_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        commit_file(&repo, dir.path(), "file.txt", "base\n", "initial");
        for name in ["", "topic~1", "../topic", "missing"] {
            assert!(
                compare_branch_files(dir.path(), name, CompareDirection::CurrentToSelected)
                    .is_err(),
                "{name}"
            );
        }
        let snapshot = compare_branch_files(
            dir.path(),
            repo.head().unwrap().shorthand().unwrap(),
            CompareDirection::CurrentToSelected,
        )
        .unwrap();
        assert!(diff_compared_file(dir.path(), &snapshot, 0).is_err());
    }
}
