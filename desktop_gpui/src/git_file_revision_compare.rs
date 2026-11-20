//! Exact, read-only per-file comparison with a committed Git ref.
//! Unlike Stage/Unstage, this compares the selected ref's tree against the
//! whole working file, without touching HEAD, index, files or worktrees.
use git2::{BranchType, DiffFormat, DiffOptions, Repository};
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_COMPARE_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug)]
pub struct GitFileRevisionComparison {
    pub relative: PathBuf,
    pub reference: String,
    pub revision_oid: String,
    pub patch: String,
}

fn valid_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|p| matches!(p, Component::Normal(_)))
}

fn valid_local_branch(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= 255
        && !branch.starts_with('-')
        && !branch.contains(['\r', '\n', '\0', ':', '~', '^', '?', '*', '[', '\\'])
        && !branch.contains("..")
        && !branch.contains("@{")
        && branch
            .split('/')
            .all(|part| !part.is_empty() && part != ".")
}

/// List local branches without requiring History to have loaded first.
pub fn list_local_git_branches(workspace: &Path) -> Result<Vec<String>, String> {
    let repo = Repository::discover(workspace).map_err(|e| e.to_string())?;
    let workdir = repo.workdir().ok_or("Not a Git worktree")?;
    if dunce::canonicalize(workspace).map_err(|e| e.to_string())?
        != dunce::canonicalize(workdir).map_err(|e| e.to_string())?
    {
        return Err("Branch picker root must match active worktree".into());
    }
    let mut branches = Vec::new();
    for entry in repo
        .branches(Some(BranchType::Local))
        .map_err(|e| e.to_string())?
    {
        let (branch, _) = entry.map_err(|e| e.to_string())?;
        if let Some(name) = branch.name().map_err(|e| e.to_string())? {
            if valid_local_branch(name) {
                branches.push(name.to_owned());
            }
        }
        if branches.len() >= 2048 {
            break;
        }
    }
    branches.sort();
    branches.dedup();
    Ok(branches)
}

pub fn compare_file_with_git_reference(
    workspace: &Path,
    relative: &Path,
    branch: Option<&str>,
) -> Result<GitFileRevisionComparison, String> {
    if !valid_relative(relative) {
        return Err("Unsafe relative path for Git file comparison".into());
    }
    let repo = Repository::discover(workspace).map_err(|e| e.to_string())?;
    let workdir = repo.workdir().ok_or("Not a Git worktree")?;
    if dunce::canonicalize(workspace).map_err(|e| e.to_string())?
        != dunce::canonicalize(workdir).map_err(|e| e.to_string())?
    {
        return Err("Compare root must match the active Git worktree".into());
    }
    let (commit, reference) = if let Some(branch) = branch {
        if !valid_local_branch(branch) {
            return Err("Invalid local Git branch".into());
        }
        let commit = repo
            .find_branch(branch, BranchType::Local)
            .map_err(|_| "Selected Git branch no longer exists".to_owned())?
            .get()
            .peel_to_commit()
            .map_err(|e| e.to_string())?;
        (commit, branch.to_owned())
    } else {
        let commit = repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|_| "No committed HEAD for file comparison".to_owned())?;
        (commit, "HEAD".to_owned())
    };
    let tree = commit.tree().map_err(|e| e.to_string())?;
    let entry = tree
        .get_path(relative)
        .map_err(|_| format!("This file has no revision on {reference}"))?;
    if entry.kind() != Some(git2::ObjectType::Blob) {
        return Err("Reference path is not a committed regular file".into());
    }
    let file = workdir.join(relative);
    match fs::symlink_metadata(&file) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err("Refusing to compare a worktree symlink".into());
        }
        Ok(meta) if !meta.file_type().is_file() => {
            return Err("Worktree comparison target is not a file".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }

    let mut opts = DiffOptions::new();
    // Treat special characters in literal filenames as text, not Git glob
    // expressions that could accidentally include unrelated workspace files.
    opts.pathspec(relative)
        .disable_pathspec_match(true)
        .context_lines(3);
    let diff = repo
        .diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))
        .map_err(|e| e.to_string())?;
    if diff.deltas().any(|delta| {
        delta.old_file().path() != Some(relative) && delta.new_file().path() != Some(relative)
    }) {
        return Err("Git comparison returned an unexpected file path".into());
    }
    let mut patch = String::new();
    let mut truncated = false;
    diff.print(DiffFormat::Patch, |_, _, line| {
        if truncated {
            return false;
        }
        let prefix = matches!(line.origin(), ' ' | '+' | '-');
        let bytes = line.content();
        let limit = MAX_COMPARE_BYTES.saturating_sub(patch.len());
        if limit <= usize::from(prefix) {
            truncated = true;
            return false;
        }
        if prefix {
            patch.push(line.origin());
        }
        let room = MAX_COMPARE_BYTES.saturating_sub(patch.len());
        let line = String::from_utf8_lossy(&bytes[..bytes.len().min(room)]);
        let slice = &line[..line.floor_char_boundary(room.min(line.len()))];
        patch.push_str(slice);
        if bytes.len() > room {
            truncated = true;
            return false;
        }
        true
    })
    .or_else(|err| if truncated { Ok(()) } else { Err(err) })
    .map_err(|e| e.to_string())?;
    if truncated {
        patch.push_str("\n… Comparison truncated at 128 KiB. No files were modified.");
    }
    if patch.is_empty() {
        patch = format!("No differences between {reference} and the current working file.");
    }
    Ok(GitFileRevisionComparison {
        relative: relative.to_path_buf(),
        reference,
        revision_oid: commit.id().to_string(),
        patch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit_file(repo: &Repository, name: &str, text: &str, message: &str) {
        fs::write(repo.workdir().unwrap().join(name), text).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let author = git2::Signature::now("QA", "qa@example.invalid").unwrap();
        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        repo.commit(
            Some("HEAD"),
            &author,
            &author,
            message,
            &tree,
            &parent.iter().collect::<Vec<_>>(),
        )
        .unwrap();
    }

    #[test]
    fn head_comparison_includes_staged_and_unstaged_without_changing_index() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        commit_file(&repo, "a.txt", "base\n", "baseline");
        fs::write(tmp.path().join("a.txt"), "staged\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let index_before = fs::read(repo.path().join("index")).unwrap();
        fs::write(tmp.path().join("a.txt"), "working\n").unwrap();
        let result = compare_file_with_git_reference(tmp.path(), Path::new("a.txt"), None).unwrap();
        assert!(result.patch.contains("-base"), "{}", result.patch);
        assert!(result.patch.contains("+working"), "{}", result.patch);
        assert!(!result.patch.contains("+staged"));
        assert_eq!(fs::read(repo.path().join("index")).unwrap(), index_before);
    }

    #[test]
    fn chosen_branch_is_not_switched_or_modified() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        commit_file(&repo, "a.txt", "original\n", "baseline");
        let initial = repo.head().unwrap().target().unwrap();
        repo.branch("reference", &repo.find_commit(initial).unwrap(), false)
            .unwrap();
        assert!(list_local_git_branches(tmp.path())
            .unwrap()
            .contains(&"reference".to_owned()));
        commit_file(&repo, "a.txt", "current\n", "updated");
        let original_head = repo.head().unwrap().target().unwrap();
        let result =
            compare_file_with_git_reference(tmp.path(), Path::new("a.txt"), Some("reference"))
                .unwrap();
        assert!(result.patch.contains("-original"));
        assert!(result.patch.contains("+current"));
        assert_eq!(repo.head().unwrap().target().unwrap(), original_head);
        assert_eq!(
            repo.find_branch("reference", BranchType::Local)
                .unwrap()
                .get()
                .target()
                .unwrap(),
            initial
        );
    }

    #[test]
    fn deleted_and_wildcard_filename_are_scoped_exactly() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        commit_file(&repo, "a[1].txt", "old\n", "baseline");
        fs::write(tmp.path().join("a1.txt"), "untracked\n").unwrap();
        fs::remove_file(tmp.path().join("a[1].txt")).unwrap();
        let result =
            compare_file_with_git_reference(tmp.path(), Path::new("a[1].txt"), None).unwrap();
        assert!(result.patch.contains("-old"));
        assert!(!result.patch.contains("a1.txt"), "{}", result.patch);
        assert!(compare_file_with_git_reference(tmp.path(), Path::new("../escape"), None).is_err());
        assert!(compare_file_with_git_reference(tmp.path(), Path::new("/escape"), None).is_err());
        assert!(compare_file_with_git_reference(
            tmp.path(),
            Path::new("a[1].txt"),
            Some("../branch")
        )
        .is_err());
        assert!(compare_file_with_git_reference(tmp.path(), Path::new("a1.txt"), None).is_err());
    }
}
