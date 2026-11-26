use git2::{DiffDelta, Repository};

use super::{
    diff_for_area, diff_line_stats_for_paths,
    git_diff_render::{collect_modified_diff_line_stats, ordinary_modified_path},
    GitChangeArea, GitChangeEntry, GitError,
};

const STATUS_STATS_BATCH_SIZE: usize = 64;

struct DeferredStats {
    entry_index: usize,
    repo_path: String,
}

/// Per-status-request plan. Never retains Git state across requests. Full
/// status still runs the authoritative status/rename scan first; only the
/// expensive patch statistics for ordinary modified files are deferred.
pub(super) struct StatusLineStatsPlan {
    enabled: bool,
    batch: bool,
    staged: Vec<DeferredStats>,
    unstaged: Vec<DeferredStats>,
}

impl StatusLineStatsPlan {
    pub(super) fn new(enabled: bool, batch: bool) -> Self {
        Self {
            enabled,
            batch,
            staged: Vec::new(),
            unstaged: Vec::new(),
        }
    }

    pub(super) fn record(
        &mut self,
        repo: &Repository,
        delta: &DiffDelta<'_>,
        entry: &mut GitChangeEntry,
        entry_index: usize,
        repo_path: &str,
        old_path: Option<&str>,
    ) -> Result<(), GitError> {
        if !self.enabled || entry.submodule.is_some() {
            return Ok(());
        }
        if self.batch && old_path.is_none() && ordinary_modified_path(delta) == Some(repo_path) {
            let pending = match entry.area {
                GitChangeArea::Staged => &mut self.staged,
                GitChangeArea::Unstaged => &mut self.unstaged,
                GitChangeArea::Untracked => unreachable!("untracked entries need no patch stats"),
            };
            pending.push(DeferredStats {
                entry_index,
                repo_path: repo_path.to_owned(),
            });
        } else {
            set_counts(
                entry,
                diff_line_stats_for_paths(repo, repo_path, old_path, entry.area)?,
            );
        }
        Ok(())
    }

    pub(super) fn finish(
        self,
        repo: &Repository,
        entries: &mut [GitChangeEntry],
    ) -> Result<(), GitError> {
        for (area, pending) in [
            (GitChangeArea::Staged, self.staged),
            (GitChangeArea::Unstaged, self.unstaged),
        ] {
            for batch in pending.chunks(STATUS_STATS_BATCH_SIZE) {
                if batch.len() > 1 {
                    let paths: Vec<_> =
                        batch.iter().map(|entry| entry.repo_path.as_str()).collect();
                    if let Some(counts) = diff_for_area(repo, &paths, area)
                        .ok()
                        .and_then(|mut diff| collect_modified_diff_line_stats(&mut diff, &paths))
                    {
                        for (request, counts) in batch.iter().zip(counts) {
                            set_counts(&mut entries[request.entry_index], counts);
                        }
                        continue;
                    }
                }
                // Single files, mismatched delta sets and batch I/O errors
                // use exactly the existing per-file behavior and error path.
                for request in batch {
                    let counts = diff_line_stats_for_paths(repo, &request.repo_path, None, area)?;
                    set_counts(&mut entries[request.entry_index], counts);
                }
            }
        }
        Ok(())
    }
}

fn set_counts(entry: &mut GitChangeEntry, counts: Option<(u32, u32)>) {
    if let Some((added, removed)) = counts {
        entry.added = Some(added);
        entry.removed = Some(removed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn batch_requires_exact_ordinary_delta_set_and_preserves_binary_counts() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut index = repo.index().unwrap();
        for (name, bytes) in [
            ("a.txt", b"old\n".as_slice()),
            ("b.bin", b"\x00\x01".as_slice()),
        ] {
            std::fs::write(dir.path().join(name), bytes).unwrap();
            index.add_path(Path::new(name)).unwrap();
        }
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = git2::Signature::now("Test", "test@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "baseline", &tree, &[])
            .unwrap();
        drop(tree);
        std::fs::write(dir.path().join("a.txt"), b"new\nextra").unwrap();
        std::fs::write(dir.path().join("b.bin"), b"\x00\x02").unwrap();
        std::fs::write(dir.path().join("added.txt"), b"added").unwrap();
        for name in ["a.txt", "b.bin", "added.txt"] {
            index.add_path(Path::new(name)).unwrap();
        }
        index.write().unwrap();
        drop(index);

        let area = GitChangeArea::Staged;
        let exact = ["a.txt", "b.bin"];
        let mut diff = diff_for_area(&repo, &exact, area).unwrap();
        let counts = collect_modified_diff_line_stats(&mut diff, &exact).expect("accepted batch");
        assert_eq!(counts, vec![Some((2, 1)), None]);
        for (name, count) in exact.into_iter().zip(counts) {
            assert_eq!(
                count,
                diff_line_stats_for_paths(&repo, name, None, area).unwrap()
            );
        }
        // Extra or missing paths, duplicates and additions must not be used
        // as a successful optimization even when their patches are printable.
        let mut diff = diff_for_area(&repo, &[], area).unwrap();
        assert!(collect_modified_diff_line_stats(&mut diff, &exact).is_none());
        for paths in [
            ["a.txt", "missing.txt"],
            ["a.txt", "a.txt"],
            ["a.txt", "added.txt"],
        ] {
            let mut diff = diff_for_area(&repo, &paths, area).unwrap();
            assert!(collect_modified_diff_line_stats(&mut diff, &paths).is_none());
        }
    }
}
