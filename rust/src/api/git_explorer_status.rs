use alera_core::git::explorer_ancestors::merge_ranked_ancestors;
use std::collections::HashMap;
use std::path::Path;

use super::git::{
    git_status_entries_for_explorer, GitChangeEntry, GitChangeStatus, GitError, GitErrorKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitExplorerStatus {
    Untracked,
    Added,
    Modified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitExplorerStatusEntry {
    pub path: String,
    pub status: GitExplorerStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitExplorerStatusSnapshot {
    pub entries: Vec<GitExplorerStatusEntry>,
}

pub fn git_explorer_status_snapshot(path: String) -> Result<GitExplorerStatusSnapshot, GitError> {
    let entries = match git_status_entries_for_explorer(path) {
        Ok(entries) => entries,
        Err(error) if error.kind == GitErrorKind::NotARepository => {
            return Ok(GitExplorerStatusSnapshot {
                entries: Vec::new(),
            });
        }
        Err(error) => return Err(error),
    };
    Ok(project_explorer_status(entries))
}

// Test oracle and benchmark: retain the exact former full status path and
// the same projection, without exposing a second production API.
#[cfg(test)]
pub(super) fn legacy_explorer_status_snapshot(
    path: String,
) -> Result<GitExplorerStatusSnapshot, GitError> {
    Ok(project_explorer_status_legacy(
        super::git::legacy_git_status_for_test(path)?.entries,
    ))
}

fn explorer_status(status: GitChangeStatus) -> GitExplorerStatus {
    match status {
        GitChangeStatus::Untracked => GitExplorerStatus::Untracked,
        GitChangeStatus::Added => GitExplorerStatus::Added,
        GitChangeStatus::Modified
        | GitChangeStatus::Deleted
        | GitChangeStatus::Renamed
        | GitChangeStatus::Copied => GitExplorerStatus::Modified,
    }
}

fn priority(status: GitExplorerStatus) -> u8 {
    match status {
        GitExplorerStatus::Untracked => 1,
        GitExplorerStatus::Added => 2,
        GitExplorerStatus::Modified => 3,
    }
}

fn canonical_slash_path(path: &str) -> bool {
    // Git status normally emits normalized slash-separated relative paths.
    // Colons, backslashes, empty components and dot traversal may produce
    // different parent chains under native Path semantics. Use the exact old
    // algorithm for the entire snapshot if any key is ambiguous.
    !path.contains(['\\', ':'])
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn project_explorer_status(entries: Vec<GitChangeEntry>) -> GitExplorerStatusSnapshot {
    if !entries
        .iter()
        .all(|entry| canonical_slash_path(&entry.path))
    {
        return project_explorer_status_legacy(entries);
    }
    let mut statuses = HashMap::<String, GitExplorerStatus>::new();
    for entry in entries {
        let ancestors = std::iter::successors(Some(entry.path.as_str()), |path| {
            path.rsplit_once('/').map(|(parent, _)| parent)
        });
        merge_ranked_ancestors(
            &mut statuses,
            ancestors,
            explorer_status(entry.status),
            priority,
        );
    }
    snapshot_from_statuses(statuses)
}

fn project_explorer_status_legacy(entries: Vec<GitChangeEntry>) -> GitExplorerStatusSnapshot {
    let mut statuses = HashMap::<String, GitExplorerStatus>::new();
    for entry in entries {
        let status = explorer_status(entry.status);
        merge_explorer_status(&mut statuses, entry.path.clone(), status);
        let mut parent = Path::new(&entry.path).parent();
        while let Some(path) = parent {
            if path.as_os_str().is_empty() {
                break;
            }
            merge_explorer_status(
                &mut statuses,
                path.to_string_lossy().replace('\\', "/"),
                status,
            );
            parent = path.parent();
        }
    }
    snapshot_from_statuses(statuses)
}

fn snapshot_from_statuses(
    statuses: HashMap<String, GitExplorerStatus>,
) -> GitExplorerStatusSnapshot {
    let mut entries = statuses
        .into_iter()
        .map(|(path, status)| GitExplorerStatusEntry { path, status })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    GitExplorerStatusSnapshot { entries }
}

fn merge_explorer_status(
    statuses: &mut HashMap<String, GitExplorerStatus>,
    path: String,
    status: GitExplorerStatus,
) {
    if statuses
        .get(&path)
        .is_some_and(|existing| priority(*existing) >= priority(status))
    {
        return;
    }
    statuses.insert(path, status);
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use crate::api::git::GitChangeArea;

    fn change(path: impl Into<String>, status: GitChangeStatus) -> GitChangeEntry {
        GitChangeEntry {
            path: path.into(),
            old_path: None,
            area: GitChangeArea::Unstaged,
            status,
            added: None,
            removed: None,
            is_binary: false,
            is_large: false,
            submodule: None,
        }
    }

    fn fixture(count: usize) -> Vec<GitChangeEntry> {
        let statuses = [
            GitChangeStatus::Untracked,
            GitChangeStatus::Added,
            GitChangeStatus::Modified,
            GitChangeStatus::Deleted,
            GitChangeStatus::Renamed,
            GitChangeStatus::Copied,
        ];
        (0..count)
            .map(|index| {
                change(
                    format!(
                        "packages/group-{}/src/features/deep/nested/module-{}/file-{index:05}.rs",
                        index % 17,
                        index % 5
                    ),
                    statuses[(index / 85) % statuses.len()],
                )
            })
            .collect()
    }

    #[test]
    fn shared_ancestor_projection_preserves_priority_duplicates_and_unicode() {
        let mut entries = fixture(1800);
        entries.extend([
            change("root.rs", GitChangeStatus::Added),
            change(
                "packages/group-0/src/features/deep/nested/module-0/file-00000.rs",
                GitChangeStatus::Modified,
            ),
            change(
                "unicode/\u{4e2d}\u{6587}/\u{1f680}.rs",
                GitChangeStatus::Added,
            ),
            change(
                "unicode/\u{4e2d}\u{6587}/other.rs",
                GitChangeStatus::Untracked,
            ),
            change("packages/group-0", GitChangeStatus::Deleted),
            change("packages/group-0", GitChangeStatus::Untracked),
        ]);
        assert!(entries
            .iter()
            .all(|entry| canonical_slash_path(&entry.path)));
        for reversed in [false, true] {
            if reversed {
                entries.reverse();
            }
            assert_eq!(
                project_explorer_status(entries.clone()),
                project_explorer_status_legacy(entries.clone())
            );
        }
    }

    #[test]
    fn ambiguous_path_snapshots_keep_exact_legacy_parent_semantics() {
        for path in [
            "",
            ".",
            "..",
            "/root/file",
            "a//b/c",
            "a/./b",
            "a/../b",
            "a\\b/c",
            "a/b/",
            "C:/root/file",
            "a:b/c",
        ] {
            assert!(!canonical_slash_path(path), "{path:?}");
            let entries = vec![
                change(path, GitChangeStatus::Modified),
                change("a/b/child.rs", GitChangeStatus::Added),
                change("a/b", GitChangeStatus::Untracked),
            ];
            assert_eq!(
                project_explorer_status(entries.clone()),
                project_explorer_status_legacy(entries),
                "{path:?}"
            );
        }
    }

    #[test]
    #[ignore = "in-memory ancestor projection benchmark"]
    fn benchmark_shared_explorer_ancestor_projection() {
        use std::time::{Duration, Instant};
        let entries = fixture(20_000);
        let expected = project_explorer_status_legacy(entries.clone());
        assert_eq!(project_explorer_status(entries.clone()), expected);
        let mut old = Vec::new();
        let mut new = Vec::new();
        for index in 0..7 {
            for optimized in if index % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let input = entries.clone();
                let started = Instant::now();
                let actual = if optimized {
                    project_explorer_status(input)
                } else {
                    project_explorer_status_legacy(input)
                };
                let elapsed = started.elapsed();
                assert_eq!(actual, expected);
                if optimized {
                    new.push(elapsed);
                } else {
                    old.push(elapsed);
                }
            }
        }
        fn median(values: &mut [Duration]) -> f64 {
            values.sort();
            values[values.len() / 2].as_secs_f64() * 1000.0
        }
        let mut visits = 0;
        let mut legacy_visits = 0;
        let mut map = HashMap::<String, GitExplorerStatus>::new();
        for entry in &entries {
            let ancestors = || {
                std::iter::successors(Some(entry.path.as_str()), |path| {
                    path.rsplit_once('/').map(|(parent, _)| parent)
                })
            };
            legacy_visits += ancestors().count();
            merge_ranked_ancestors(
                &mut map,
                ancestors().inspect(|_| visits += 1),
                explorer_status(entry.status),
                priority,
            );
        }
        assert_eq!(snapshot_from_statuses(map), expected);
        let old = median(&mut old);
        let new = median(&mut new);
        println!("Classic ancestor projection changes={} rows={} visits_old={legacy_visits} visits_new={visits} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", entries.len(), expected.entries.len(), old/new);
    }
}
