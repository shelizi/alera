//! Per-source streaming traversal for multi-account history mirroring.
//! Do not retain a filesystem inventory between sources or periodic passes.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct WalkStats {
    pub(super) skipped: usize,
    #[cfg(test)]
    pub(super) directories_read: usize,
}

/// Enumerate each directory of one source once, regardless of the number of
/// authorized destinations. The callback returns false when the global copy
/// budget is exhausted. Native files are never opened by the walker.
pub(super) fn walk_transcripts(
    root: &Path,
    max_depth: usize,
    mut visit: impl FnMut(&Path, &Path) -> bool,
) -> WalkStats {
    let mut stats = WalkStats::default();
    let mut stack = vec![(root.to_path_buf(), PathBuf::new(), 0)];
    while let Some((directory, relative, depth)) = stack.pop() {
        #[cfg(test)]
        {
            stats.directories_read += 1;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            stats.skipped += 1;
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                stats.skipped += 1;
                continue;
            }
            let path = entry.path();
            let child = relative.join(entry.file_name());
            if kind.is_dir() {
                if depth < max_depth {
                    stack.push((path, child, depth + 1));
                }
                continue;
            }
            if kind.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
                && !visit(&path, &child)
            {
                return stats;
            }
        }
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "alera-sync-walk-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("year/month/day")).unwrap();
        fs::write(root.join("first.jsonl"), b"{}\n").unwrap();
        fs::write(root.join("year/month/second.jsonl"), b"{}\n").unwrap();
        fs::write(root.join("year/month/day/third.jsonl"), b"{}\n").unwrap();
        fs::write(root.join("auth.json"), b"not history").unwrap();
        root
    }

    #[test]
    fn one_source_walk_fans_out_without_repeated_directory_reads() {
        let root = fixture();
        let mut seen = HashSet::new();
        let stats = walk_transcripts(&root, 12, |path, relative| {
            assert_eq!(path, root.join(relative));
            for target in 0..5 {
                assert!(seen.insert((target, relative.to_owned())));
            }
            true
        });
        assert_eq!(stats.directories_read, 4);
        assert_eq!(seen.len(), 15);
        assert_eq!(stats.skipped, 0);
        // Previous pair-major traversal opened four directories for each
        // of the five destinations; the same fan-out now opens four total.
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn budget_stop_and_depth_limit_prevent_more_work() {
        let root = fixture();
        let mut visited = 0;
        let stats = walk_transcripts(&root, 12, |_, _| {
            visited += 1;
            false
        });
        assert_eq!(visited, 1);
        assert_eq!(stats.directories_read, 1);
        let mut found = Vec::new();
        let stats = walk_transcripts(&root, 1, |_, relative| {
            found.push(relative.to_owned());
            true
        });
        assert_eq!(stats.directories_read, 2);
        assert_eq!(found, vec![PathBuf::from("first.jsonl")]);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_entries_are_not_walked_or_mirrored() {
        use std::os::unix::fs::symlink;
        let root = fixture();
        symlink(root.join("year"), root.join("link")).unwrap();
        symlink(root.join("first.jsonl"), root.join("linked.jsonl")).unwrap();
        let mut found = Vec::new();
        let stats = walk_transcripts(&root, 12, |_, relative| {
            found.push(relative.to_owned());
            true
        });
        assert_eq!(stats.skipped, 2);
        assert_eq!(found.len(), 3);
        fs::remove_dir_all(root).unwrap();
    }
}
