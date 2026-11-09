//! Shared, read-only staged Git context for AI-assisted commit messages.
//! Both desktop frontends can reuse the same prompt and index snapshot.
//! This module never invokes an agent or changes Git refs/index/work files.
use super::{GitError, GitErrorKind};
use git2::{Delta, DiffFormat, Repository, RepositoryState};
use std::path::Path;

const MAX_STAGED_FILES: usize = 256;
const MAX_PATCH_CHARS: usize = 200_000;
const MAX_SUMMARY_CHARS: usize = 6_000;
const MAX_INSTRUCTIONS_CHARS: usize = 4_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitStagedCommitPrompt {
    pub prompt: String,
    pub staged_summary: String,
    pub branch: String,
    pub staged_files: usize,
    pub head_oid: String,
    pub index_tree_oid: String,
    pub patch_truncated: bool,
}

fn open_staged_repo(workspace: &Path) -> Result<Repository, GitError> {
    let repo = Repository::discover(workspace).map_err(GitError::from_git2)?;
    if repo.is_bare() {
        return Err(GitError::new(
            GitErrorKind::NotARepository,
            "AI commit context requires a working tree",
        ));
    }
    if repo.state() != RepositoryState::Clean {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "Finish or abort the in-progress Git operation before generating a commit prompt",
        ));
    }
    Ok(repo)
}

fn head_and_index(repo: &Repository) -> Result<(String, String, String), GitError> {
    let head = repo.head().map_err(GitError::from_git2)?;
    if !head.is_branch() {
        return Err(GitError::new(
            GitErrorKind::DetachedHead,
            "AI commit context requires a local branch",
        ));
    }
    let branch = head.shorthand().map_err(GitError::from_git2)?.to_owned();
    let head_oid = head.peel_to_commit().map_err(GitError::from_git2)?.id();
    let mut index = repo.index().map_err(GitError::from_git2)?;
    if index.has_conflicts() {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "Resolve Git index conflicts before generating an AI commit prompt",
        ));
    }
    let tree_oid = index.write_tree_to(repo).map_err(GitError::from_git2)?;
    Ok((branch, head_oid.to_string(), tree_oid.to_string()))
}

fn bounded(value: &str, max: usize) -> String {
    let count = value.chars().count();
    if count <= max {
        return value.to_owned();
    }
    let beginning: String = value.chars().take(max).collect();
    format!(
        "{beginning}\n\n[truncated: {} characters omitted]",
        count - max
    )
}

/// Read only the staged HEAD-to-index diff (not unstaged or untracked files).
/// The prompt follows Classic's rules and char budgets, but generating the AI
/// completion remains the responsibility of the approved Agent runner.
pub fn prepare_staged_commit_prompt(
    workspace: &Path,
    custom_instructions: &str,
) -> Result<GitStagedCommitPrompt, GitError> {
    let repo = open_staged_repo(workspace)?;
    let (branch, head_oid, index_tree_oid) = head_and_index(&repo)?;
    let head = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(GitError::from_git2)?;
    let head_tree = head.tree().map_err(GitError::from_git2)?;
    let index = repo.index().map_err(GitError::from_git2)?;
    let diff = repo
        .diff_tree_to_index(Some(&head_tree), Some(&index), None)
        .map_err(GitError::from_git2)?;
    if diff.deltas().len() == 0 {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "Stage at least one file before generating an AI commit prompt",
        ));
    }
    if diff.deltas().len() > MAX_STAGED_FILES {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "AI commit prompt supports at most 256 staged files",
        ));
    }
    let mut lines = Vec::new();
    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .ok_or_else(|| GitError::new(GitErrorKind::Internal, "Staged change has no path"))?;
        let path = path
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>();
        let change = match delta.status() {
            Delta::Added => "added",
            Delta::Deleted => "deleted",
            Delta::Renamed => "renamed",
            Delta::Copied => "copied",
            _ => "modified",
        };
        lines.push(format!("- {path} ({change})"));
    }
    let staged_summary = bounded(&lines.join("\n"), MAX_SUMMARY_CHARS);

    let mut patch = String::new();
    let mut patch_chars = 0usize;
    let mut patch_truncated = false;
    let print_result = diff.print(DiffFormat::Patch, |_, _, line| {
        let origin = line.origin();
        let prefix = matches!(origin, '+' | '-' | ' ');
        let content = String::from_utf8_lossy(line.content());
        let line_len = content.chars().count() + usize::from(prefix);
        if patch_chars.saturating_add(line_len) > MAX_PATCH_CHARS {
            let remaining = MAX_PATCH_CHARS.saturating_sub(patch_chars);
            if prefix && remaining > 0 {
                patch.push(origin);
            }
            let available = remaining.saturating_sub(usize::from(prefix));
            patch.extend(content.chars().take(available));
            patch_truncated = true;
            return false;
        }
        if prefix {
            patch.push(origin);
        }
        patch.push_str(&content);
        patch_chars += line_len;
        true
    });
    if let Err(error) = print_result {
        if !patch_truncated {
            return Err(GitError::from_git2(error));
        }
    }
    if patch_truncated {
        patch.push_str("\n...(diff truncated, additional characters omitted)");
    }
    let mut prompt = format!(
        "You are generating a single git commit message.\n\
Return only the commit message text. Do not include a preamble, quotes, or code fences.\n\
\nRules:\n\
- First line: imperative mood, <= 72 chars, no trailing period.\n\
- Optional body: blank line, then short wrapped bullet points or prose explaining WHY.\n\
- Capture the primary user-visible or developer-visible change.\n\
- Use only the staged changes below as context.\n\
- Do not include \"Co-authored-by\" or other git trailers.\n\
\nBranch: {branch}\n\
\nStaged files:\n{staged_summary}\n\
\nStaged patch:\n```diff\n{patch}\n```"
    );
    if !custom_instructions.trim().is_empty() {
        prompt.push_str("\n\nAdditional user instructions:\n");
        prompt.push_str(&bounded(custom_instructions.trim(), MAX_INSTRUCTIONS_CHARS));
    }
    Ok(GitStagedCommitPrompt {
        prompt,
        staged_summary,
        branch,
        staged_files: lines.len(),
        head_oid,
        index_tree_oid,
        patch_truncated,
    })
}

/// Refuse copying a stale staged-diff prompt if the current commit or index
/// changed while the async preview was open. Never updates the index.
pub fn verify_staged_commit_prompt(
    workspace: &Path,
    preview: &GitStagedCommitPrompt,
) -> Result<(), GitError> {
    let repo = open_staged_repo(workspace)?;
    let (branch, head_oid, index_tree_oid) = head_and_index(&repo)?;
    if branch != preview.branch
        || head_oid != preview.head_oid
        || index_tree_oid != preview.index_tree_oid
    {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "Staged Git changes or branch changed since AI prompt preview",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        fs::write(dir.path().join("tracked.txt"), "old\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("Fixture", "fixture@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        index.write().unwrap();
        drop(tree);
        (dir, repo)
    }

    #[test]
    fn staged_prompt_excludes_unstaged_and_untracked_secrets_and_is_read_only() {
        let (dir, repo) = fixture();
        let before_head = repo.head().unwrap().target();
        fs::write(dir.path().join("tracked.txt"), "staged-visible\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        index.write().unwrap();
        fs::write(dir.path().join("tracked.txt"), "SECRET-UNSTAGED\n").unwrap();
        fs::write(dir.path().join("private.txt"), "SECRET-UNTRACKED\n").unwrap();
        let prepared = prepare_staged_commit_prompt(dir.path(), "Use Traditional Chinese").unwrap();
        assert_eq!(prepared.staged_files, 1);
        assert!(prepared.prompt.contains("staged-visible"));
        assert!(prepared.prompt.contains("Use Traditional Chinese"));
        assert!(!prepared.prompt.contains("SECRET-UNSTAGED"));
        assert!(!prepared.prompt.contains("SECRET-UNTRACKED"));
        verify_staged_commit_prompt(dir.path(), &prepared).unwrap();
        assert_eq!(repo.head().unwrap().target(), before_head);
        assert!(repo
            .index()
            .unwrap()
            .get_path(Path::new("private.txt"), 0)
            .is_none());
        assert_eq!(
            fs::read_to_string(dir.path().join("tracked.txt")).unwrap(),
            "SECRET-UNSTAGED\n"
        );
    }

    #[test]
    fn prompt_requires_staged_changes_and_copy_rejects_stale_index() {
        let (dir, repo) = fixture();
        assert!(prepare_staged_commit_prompt(dir.path(), "").is_err());
        fs::write(dir.path().join("tracked.txt"), "staged\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        index.write().unwrap();
        let prepared = prepare_staged_commit_prompt(dir.path(), "").unwrap();
        fs::write(dir.path().join("new.txt"), "new").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("new.txt")).unwrap();
        index.write().unwrap();
        assert!(verify_staged_commit_prompt(dir.path(), &prepared).is_err());
    }

    #[test]
    fn prompt_is_bounded_for_very_large_staged_patches_and_user_instructions() {
        let (dir, repo) = fixture();
        let large = format!("{}\n", "x".repeat(240_000));
        fs::write(dir.path().join("tracked.txt"), large).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        index.write().unwrap();
        let prepared = prepare_staged_commit_prompt(dir.path(), &"z".repeat(8_000)).unwrap();
        assert_eq!(prepared.staged_files, 1);
        assert!(prepared.patch_truncated);
        assert!(prepared.prompt.contains("diff truncated"));
        assert!(prepared
            .prompt
            .contains("[truncated: 4000 characters omitted]"));
        assert!(prepared.prompt.chars().count() < 210_000);
        verify_staged_commit_prompt(dir.path(), &prepared).unwrap();
    }

    #[test]
    fn copy_validation_rejects_new_head_even_with_unchanged_staged_tree() {
        let (dir, repo) = fixture();
        fs::write(dir.path().join("tracked.txt"), "staged\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        index.write().unwrap();
        let prepared = prepare_staged_commit_prompt(dir.path(), "").unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = Signature::now("Fixture", "fixture@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "change", &tree, &[&parent])
            .unwrap();
        assert!(verify_staged_commit_prompt(dir.path(), &prepared).is_err());
    }
}
