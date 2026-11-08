//! Bounded read-only history, commit preview and local branch comparison.
//! All repository I/O belongs on a background worker, never in GPUI render.
use git2::{BranchType, DiffFormat, DiffOptions, Oid, Repository, Sort};
use std::path::Path;

const MAX_HISTORY: usize = 96;
const MAX_BRANCHES: usize = 80;
const MAX_PATCH: usize = 24 * 1024;

#[derive(Debug, Clone)]
pub struct GitBranchRow {
    pub name: String,
    pub current: bool,
    pub tip: String,
    pub remote: bool,
    pub favorite: bool,
}

#[derive(Debug, Clone)]
pub struct GitCommitRow {
    pub oid: String,
    pub short: String,
    pub subject: String,
    pub author: String,
    pub parents: usize,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GitHistorySnapshot {
    pub current: String,
    pub branches: Vec<GitBranchRow>,
    pub commits: Vec<GitCommitRow>,
    pub truncated: bool,
}

/// Native-owned Git Graph selection. Favorites belong to independent Native
/// state, never to the Flutter edition or to the repository's Git config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitHistoryOptions {
    pub all_branches: bool,
    pub favorites_only: bool,
    pub selected_ref: Option<String>,
    pub favorites: Vec<String>,
}

fn repo_in(workspace: &Path) -> Result<Repository, String> {
    let repo = Repository::discover(workspace).map_err(|e| e.to_string())?;
    if repo.is_bare() {
        return Err("Bare repositories cannot be viewed in this GPUI workbench".into());
    }
    Ok(repo)
}

fn subject(line: &str, count: usize) -> String {
    line.chars().take(count).collect()
}

impl GitHistorySnapshot {
    pub fn load(workspace: &Path) -> Result<Self, String> {
        Self::load_filtered(workspace, &GitHistoryOptions::default())
    }

    pub fn load_filtered(workspace: &Path, options: &GitHistoryOptions) -> Result<Self, String> {
        let repo = repo_in(workspace)?;
        let current = repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().ok().map(str::to_owned))
            .unwrap_or_else(|| "(detached/unborn)".into());
        let mut branches = Vec::new();
        let mut tips = Vec::new();
        let mut truncated = false;
        for branch in repo
            .branches(if options.all_branches {
                None
            } else {
                Some(BranchType::Local)
            })
            .map_err(|e| e.to_string())?
        {
            let (branch, kind) = branch.map_err(|e| e.to_string())?;
            let Some(name) = branch.name().map_err(|e| e.to_string())?.map(str::to_owned) else {
                continue;
            };
            if kind == BranchType::Remote && name.ends_with("/HEAD") {
                continue;
            }
            let remote = kind == BranchType::Remote;
            let favorite_key = if remote {
                format!("refs/remotes/{name}")
            } else {
                name.clone()
            };
            let favorite = options.favorites.contains(&favorite_key);
            if options.favorites_only && !favorite {
                continue;
            }
            let Ok(commit) = branch.get().peel_to_commit() else {
                continue;
            };
            if branches.len() == MAX_BRANCHES {
                truncated = true;
                continue;
            }
            let tip = commit.id();
            tips.push((tip, name.clone(), remote));
            branches.push(GitBranchRow {
                current: branch.is_head(),
                name,
                tip: tip.to_string()[..8].to_owned(),
                remote,
                favorite,
            });
        }
        branches.sort_by(|a, b| b.current.cmp(&a.current).then(a.name.cmp(&b.name)));
        if let Some(requested) = options.selected_ref.as_deref() {
            // Host references are accepted only after resolving an exact,
            // visible branch. Do not interpret arbitrary revspec operators.
            let expected_remote = if requested.starts_with("refs/remotes/") {
                Some(true)
            } else if requested.starts_with("refs/heads/") {
                Some(false)
            } else {
                None
            };
            let requested = requested
                .strip_prefix("refs/heads/")
                .or_else(|| requested.strip_prefix("refs/remotes/"))
                .unwrap_or(requested);
            let selected = if requested == "HEAD" {
                current.as_str()
            } else {
                requested
            };
            let matching = tips
                .iter()
                .filter(|(_, name, remote)| {
                    name == selected && expected_remote.is_none_or(|expected| expected == *remote)
                })
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err("Requested Git Graph reference is missing or ambiguous".into());
            }
            let tip = (*matching[0]).clone();
            tips.clear();
            tips.push(tip);
        }
        let mut walk = repo.revwalk().map_err(|e| e.to_string())?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
            .map_err(|e| e.to_string())?;
        for (oid, _, _) in &tips {
            walk.push(*oid).map_err(|e| e.to_string())?;
        }
        if !options.favorites_only && options.selected_ref.is_none() {
            if let Ok(head) = repo.head().and_then(|head| head.peel_to_commit()) {
                walk.push(head.id()).map_err(|e| e.to_string())?;
            }
        }
        let mut commits = Vec::new();
        for oid in walk {
            let oid = oid.map_err(|e| e.to_string())?;
            if commits.len() >= MAX_HISTORY {
                truncated = true;
                break;
            }
            let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
            commits.push(GitCommitRow {
                oid: oid.to_string(),
                short: oid.to_string()[..8].to_owned(),
                subject: subject(
                    commit.summary().ok().flatten().unwrap_or("(no subject)"),
                    110,
                ),
                author: subject(commit.author().name().unwrap_or("Unknown"), 48),
                parents: commit.parent_count(),
                labels: tips
                    .iter()
                    .filter(|(tip, _, _)| *tip == oid)
                    .take(5)
                    .map(|(_, branch, remote)| {
                        if *remote {
                            format!("refs/remotes/{branch}")
                        } else {
                            branch.clone()
                        }
                    })
                    .collect(),
            });
        }
        Ok(Self {
            current,
            branches,
            commits,
            truncated,
        })
    }
}

fn bounded_patch(diff: &git2::Diff<'_>) -> Result<String, String> {
    let mut patch = String::new();
    diff.print(DiffFormat::Patch, |_, _, line| {
        if patch.len() < MAX_PATCH {
            if matches!(line.origin(), '+' | '-' | ' ') {
                patch.push(line.origin());
            }
            let raw = String::from_utf8_lossy(line.content());
            for ch in raw.chars() {
                if patch.len() + ch.len_utf8() > MAX_PATCH {
                    break;
                }
                patch.push(ch);
            }
        }
        true
    })
    .map_err(|e| e.to_string())?;
    if patch.len() == MAX_PATCH {
        patch.push_str("\n... patch preview limited");
    }
    Ok(patch)
}

/// Preview a commit's first-parent patch (merge commits show first-parent).
pub fn preview_commit(workspace: &Path, sha: &str) -> Result<String, String> {
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid full commit ID".into());
    }
    let repo = repo_in(workspace)?;
    let oid = Oid::from_str(sha).map_err(|e| e.to_string())?;
    let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
    let current = commit.tree().map_err(|e| e.to_string())?;
    let parent = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(|e| e.to_string())?
                .tree()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(3);
    let diff = repo
        .diff_tree_to_tree(parent.as_ref(), Some(&current), Some(&mut opts))
        .map_err(|e| e.to_string())?;
    let patch = bounded_patch(&diff)?;
    Ok(format!(
        "Commit {sha}\n{}\nAuthor: {}\nParents: {}\n\n{}",
        commit.summary().ok().flatten().unwrap_or("(no subject)"),
        commit.author().name().unwrap_or("Unknown"),
        commit.parent_count(),
        if patch.is_empty() {
            "(no first-parent text changes)"
        } else {
            &patch
        }
    ))
}

/// Compare the current local branch with another local branch. No files,
/// refs, Git index or worktree state are changed by this operation.
pub fn compare_local_branch(workspace: &Path, branch: &str) -> Result<String, String> {
    if branch.is_empty() || branch.len() > 255 || branch.contains(['\n', '\r', '\0']) {
        return Err("Enter a valid local branch to compare".into());
    }
    let repo = repo_in(workspace)?;
    let current = repo
        .head()
        .map_err(|e| e.to_string())?
        .peel_to_commit()
        .map_err(|e| e.to_string())?;
    let current_name = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().ok().map(str::to_owned))
        .unwrap_or_else(|| "HEAD".into());
    let other = repo
        .find_branch(branch, BranchType::Local)
        .map_err(|e| e.to_string())?
        .get()
        .peel_to_commit()
        .map_err(|e| e.to_string())?;
    let (ahead, behind) = repo
        .graph_ahead_behind(other.id(), current.id())
        .map_err(|e| e.to_string())?;
    let current_tree = current.tree().map_err(|e| e.to_string())?;
    let other_tree = other.tree().map_err(|e| e.to_string())?;
    let diff = repo
        .diff_tree_to_tree(Some(&current_tree), Some(&other_tree), None)
        .map_err(|e| e.to_string())?;
    let patch = bounded_patch(&diff)?;
    Ok(format!(
        "Compare {branch} vs {current_name}\n{branch}: {ahead} ahead / {behind} behind\n\n{}",
        if patch.is_empty() {
            "(trees are identical)"
        } else {
            &patch
        }
    ))
}

/// Read-only Host-authorized two-ref Git range comparison. Match Classic's
/// merge-base..head semantics; never touch worktree files or the index.
pub fn compare_git_refs(
    workspace: &Path,
    base_ref: &str,
    head_ref: &str,
    scope: Option<&str>,
) -> Result<String, String> {
    fn valid_ref(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 128
            && !name.starts_with('-')
            && !name.contains("..")
            && !name.contains("@{")
            && !name.ends_with(['.', '/'])
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"/._-".contains(&c))
    }
    if !valid_ref(base_ref) || !valid_ref(head_ref) {
        return Err("Invalid Git comparison ref".into());
    }
    let repo = repo_in(workspace)?;
    let base = repo
        .revparse_single(base_ref)
        .and_then(|value| value.peel_to_commit())
        .map_err(|_| format!("Git base ref unavailable: {base_ref}"))?;
    let head = repo
        .revparse_single(head_ref)
        .and_then(|value| value.peel_to_commit())
        .map_err(|_| format!("Git head ref unavailable: {head_ref}"))?;
    let ancestor = repo
        .merge_base(base.id(), head.id())
        .map_err(|_| "Refs have no common merge base".to_string())?;
    let merge_tree = repo
        .find_commit(ancestor)
        .and_then(|commit| commit.tree())
        .map_err(|e| e.to_string())?;
    let head_tree = head.tree().map_err(|e| e.to_string())?;
    let mut options = DiffOptions::new();
    options.context_lines(3);
    if let Some(scope) = scope {
        use std::path::Component;
        let relative = Path::new(scope);
        if scope.is_empty()
            || scope.len() > 1024
            || !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err("Invalid Git comparison subdirectory".into());
        }
        let repo_root = repo.workdir().ok_or("Git worktree path unavailable")?;
        let target = dunce::canonicalize(workspace.join(relative))
            .map_err(|_| "Git comparison scope unavailable".to_string())?;
        if !target.is_dir() || !target.starts_with(repo_root) {
            return Err("Git comparison scope outside worktree".into());
        }
        let in_repo = target
            .strip_prefix(repo_root)
            .map_err(|_| "Invalid Git scope".to_string())?;
        options.pathspec(in_repo);
    }
    let diff = repo
        .diff_tree_to_tree(Some(&merge_tree), Some(&head_tree), Some(&mut options))
        .map_err(|e| e.to_string())?;
    let patch = bounded_patch(&diff)?;
    let (ahead, behind) = repo
        .graph_ahead_behind(head.id(), base.id())
        .map_err(|e| e.to_string())?;
    Ok(format!("Compare {base_ref}..{head_ref}\nBase: {}\nHead: {}\nMerge base: {ancestor}\nHead: {ahead} ahead / {behind} behind\n\n{}",
        base.id(), head.id(), if patch.is_empty() { "(no changed files in this scope)" } else { &patch }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn history_compare_and_commit_preview_are_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        fs::write(dir.path().join("中文.txt"), "first\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("中文.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
        let first = repo
            .commit(Some("HEAD"), &sig, &sig, "first", &tree, &[])
            .unwrap();
        repo.branch("feature", &repo.find_commit(first).unwrap(), false)
            .unwrap();
        drop(tree);
        fs::write(dir.path().join("中文.txt"), "second\n").unwrap();
        index.add_path(Path::new("中文.txt")).unwrap();
        let next = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let second = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "second",
                &next,
                &[&repo.find_commit(first).unwrap()],
            )
            .unwrap();
        repo.reference(
            "refs/remotes/origin/feature",
            second,
            false,
            "disposable fixture remote branch",
        )
        .unwrap();
        drop(next);
        drop(index);
        let snapshot = GitHistorySnapshot::load(dir.path()).unwrap();
        assert_eq!(snapshot.commits.len(), 2);
        assert_eq!(snapshot.branches.len(), 2);
        assert!(snapshot
            .commits
            .iter()
            .any(|row| row.oid == second.to_string()));
        let all = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                all_branches: true,
                favorites: vec!["feature".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.branches.len(), 3);
        assert!(all
            .branches
            .iter()
            .any(|row| row.remote && row.name == "origin/feature"));
        assert!(all
            .branches
            .iter()
            .any(|row| row.favorite && row.name == "feature"));
        let focused = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                favorites_only: true,
                selected_ref: Some("feature".into()),
                favorites: vec!["feature".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(focused.branches.len(), 1);
        assert_eq!(
            focused.commits.len(),
            1,
            "filter must not include unfavorited HEAD"
        );
        assert_eq!(focused.commits[0].oid, first.to_string());
        let remote = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                all_branches: true,
                selected_ref: Some("refs/remotes/origin/feature".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(remote.commits[0].oid, second.to_string());
        assert!(
            GitHistorySnapshot::load_filtered(
                dir.path(),
                &GitHistoryOptions {
                    selected_ref: Some("origin/feature".into()),
                    ..Default::default()
                },
            )
            .is_err(),
            "remote references require allBranches"
        );
        assert!(
            GitHistorySnapshot::load_filtered(
                dir.path(),
                &GitHistoryOptions {
                    selected_ref: Some("feature~1".into()),
                    ..Default::default()
                },
            )
            .is_err(),
            "MCP cannot smuggle a revspec into branch selection"
        );
        assert!(preview_commit(dir.path(), &second.to_string())
            .unwrap()
            .contains("+second"));
        assert!(compare_local_branch(dir.path(), "feature")
            .unwrap()
            .contains("0 ahead / 1 behind"));
        let ref_comparison = compare_git_refs(dir.path(), "feature", "HEAD", None).unwrap();
        assert!(ref_comparison.contains("Compare feature..HEAD"));
        assert!(ref_comparison.contains("+second"));
        assert!(compare_git_refs(dir.path(), "../unsafe", "HEAD", None).is_err());
        assert!(compare_git_refs(dir.path(), "feature", "missing", None).is_err());
        assert!(compare_git_refs(dir.path(), "feature", "HEAD", Some("../outside")).is_err());
        assert!(preview_commit(dir.path(), "../invalid").is_err());
        assert!(compare_local_branch(dir.path(), "unknown").is_err());
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), second);
        assert_eq!(
            fs::read_to_string(dir.path().join("中文.txt")).unwrap(),
            "second\n"
        );
    }
}
