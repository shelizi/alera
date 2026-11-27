//! Read-only Explorer Git decorations derived from the existing bounded
//! GitSnapshot. No Git status read is performed while rendering a row.
use crate::git_inspect::GitSnapshot;
use alera_core::git::explorer_ancestors::merge_ranked_ancestors;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerGitBadge {
    Untracked,
    Added,
    Modified,
    Renamed,
    Deleted,
    Conflict,
}

impl ExplorerGitBadge {
    pub fn label(self) -> &'static str {
        match self {
            Self::Untracked => "U",
            Self::Added => "A",
            Self::Modified => "M",
            Self::Renamed => "R",
            Self::Deleted => "D",
            Self::Conflict => "!",
        }
    }

    pub fn color(self) -> u32 {
        match self {
            Self::Added | Self::Untracked => 0x77c79b,
            Self::Modified | Self::Renamed => 0xe7b96a,
            Self::Deleted | Self::Conflict => 0xe58d8d,
        }
    }

    fn priority(self) -> u8 {
        match self {
            Self::Conflict => 6,
            Self::Deleted => 5,
            Self::Modified => 4,
            Self::Renamed => 3,
            Self::Added => 2,
            Self::Untracked => 1,
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        match label {
            "?" | "U" => Some(Self::Untracked),
            "A" => Some(Self::Added),
            "M" | "T" => Some(Self::Modified),
            "R" => Some(Self::Renamed),
            "D" => Some(Self::Deleted),
            "CONFLICT" => Some(Self::Conflict),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ExplorerGitBadges {
    entries: HashMap<PathBuf, ExplorerGitBadge>,
}

impl ExplorerGitBadges {
    pub fn from_snapshot(snapshot: Option<&GitSnapshot>) -> Self {
        let Some(snapshot) = snapshot else {
            return Self::default();
        };
        let mut entries = HashMap::new();
        for change in &snapshot.changed_files {
            if !change
                .relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
            {
                continue;
            }
            let Some(badge) = ExplorerGitBadge::from_label(change.label) else {
                continue;
            };
            let path = snapshot.repository_root.join(&change.relative);
            // Mark changed files and their parent directories; stop at the
            // repository root and never decorate outside it. Borrow the
            // already propagated ancestor keys instead of cloning each one.
            merge_ranked_ancestors(
                &mut entries,
                path.ancestors().take_while(|parent| {
                    parent.starts_with(&snapshot.repository_root)
                        && *parent != snapshot.repository_root
                }),
                badge,
                ExplorerGitBadge::priority,
            );
        }
        Self { entries }
    }

    pub fn get(&self, absolute_path: &Path) -> Option<ExplorerGitBadge> {
        self.entries.get(absolute_path).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_inspect::GitChange;

    fn legacy_badges(snapshot: &GitSnapshot) -> HashMap<PathBuf, ExplorerGitBadge> {
        let mut entries = HashMap::new();
        for change in &snapshot.changed_files {
            if !change
                .relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
            {
                continue;
            }
            let Some(badge) = ExplorerGitBadge::from_label(change.label) else {
                continue;
            };
            let mut path = snapshot.repository_root.join(&change.relative);
            while path.starts_with(&snapshot.repository_root) && path != snapshot.repository_root {
                let current = entries.entry(path.clone()).or_insert(badge);
                if badge.priority() > current.priority() {
                    *current = badge;
                }
                let Some(parent) = path.parent() else {
                    break;
                };
                path = parent.to_path_buf();
            }
        }
        entries
    }

    fn fixture(count: usize) -> GitSnapshot {
        let labels = ["?", "U", "A", "R", "M", "T", "D", "CONFLICT", "unknown"];
        GitSnapshot {
            branch: "feature".into(),
            upstream: None,
            upstream_configured: false,
            has_stashes: false,
            has_head_commit: true,
            repository_root: PathBuf::from("C:/workspace/project"),
            truncated: false,
            changed_files: (0..count)
                .map(|index| GitChange {
                    relative: PathBuf::from(format!(
                        "packages/group-{}/src/features/deep/nested/module-{}/file-{index:04}.rs",
                        index % 3,
                        index % 2
                    )),
                    label: labels[(index / 6) % labels.len()],
                    status: git2::Status::WT_MODIFIED,
                })
                .collect(),
        }
    }

    #[test]
    fn shared_badges_match_previous_projection_with_mixed_statuses_and_paths() {
        let mut snapshot = fixture(120);
        for (relative, label) in [
            (
                "packages/group-0/src/features/deep/nested/module-0/file-0000.rs",
                "CONFLICT",
            ),
            ("packages/group-0", "D"),
            ("packages/group-0", "A"),
            ("unicode/\u{4e2d}\u{6587}.rs", "M"),
            ("unicode/\u{1f680}.rs", "A"),
            ("../outside", "CONFLICT"),
            ("/absolute", "M"),
            ("./dot-prefixed", "M"),
            ("nested/./child", "R"),
            ("nested//child", "D"),
            ("nested/child", "A"),
            ("", "M"),
        ] {
            snapshot.changed_files.push(GitChange {
                relative: PathBuf::from(relative),
                label,
                status: git2::Status::WT_MODIFIED,
            });
        }
        for reversed in [false, true] {
            if reversed {
                snapshot.changed_files.reverse();
            }
            let actual = ExplorerGitBadges::from_snapshot(Some(&snapshot));
            assert_eq!(actual.entries, legacy_badges(&snapshot));
            assert_eq!(actual.get(&snapshot.repository_root), None);
        }
    }

    #[test]
    #[ignore = "in-memory bounded snapshot projection benchmark"]
    fn benchmark_shared_gpui_badge_projection() {
        use std::time::{Duration, Instant};
        // Production GitSnapshot currently caps changes at 120 rows.
        let snapshot = fixture(120);
        let expected = legacy_badges(&snapshot);
        assert_eq!(
            ExplorerGitBadges::from_snapshot(Some(&snapshot)).entries,
            expected
        );
        let mut old = Vec::new();
        let mut new = Vec::new();
        for sample in 0..7 {
            for optimized in if sample % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let started = Instant::now();
                for _ in 0..100 {
                    let actual = if optimized {
                        ExplorerGitBadges::from_snapshot(Some(&snapshot)).entries
                    } else {
                        legacy_badges(&snapshot)
                    };
                    assert_eq!(actual, expected);
                }
                let elapsed = started.elapsed();
                if optimized {
                    new.push(elapsed);
                } else {
                    old.push(elapsed);
                }
            }
        }
        fn median(values: &mut [Duration]) -> f64 {
            values.sort();
            values[values.len() / 2].as_secs_f64() * 1000.0 / 100.0
        }
        let old = median(&mut old);
        let new = median(&mut new);
        println!("GPUI badge projection changes={} rows={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x (100 projections/sample, includes parity checks)", snapshot.changed_files.len(), expected.len(), old/new);
    }

    #[test]
    fn status_badges_follow_changed_files_and_aggregate_directory_priority() {
        let root = PathBuf::from("C:/repo");
        let snapshot = GitSnapshot {
            branch: "feature".into(),
            upstream: None,
            upstream_configured: false,
            has_stashes: false,
            has_head_commit: true,
            repository_root: root.clone(),
            truncated: false,
            changed_files: vec![
                GitChange {
                    relative: PathBuf::from("src/new.rs"),
                    label: "?",
                    status: git2::Status::WT_NEW,
                },
                GitChange {
                    relative: PathBuf::from("src/modified.rs"),
                    label: "M",
                    status: git2::Status::WT_MODIFIED,
                },
                GitChange {
                    relative: PathBuf::from("src/deleted.rs"),
                    label: "D",
                    status: git2::Status::WT_DELETED,
                },
                GitChange {
                    relative: PathBuf::from("docs/readme.md"),
                    label: "A",
                    status: git2::Status::INDEX_NEW,
                },
            ],
        };
        let badges = ExplorerGitBadges::from_snapshot(Some(&snapshot));
        assert_eq!(
            badges.get(&root.join("src/new.rs")),
            Some(ExplorerGitBadge::Untracked)
        );
        assert_eq!(
            badges.get(&root.join("src/modified.rs")),
            Some(ExplorerGitBadge::Modified)
        );
        assert_eq!(
            badges.get(&root.join("src")),
            Some(ExplorerGitBadge::Deleted)
        );
        assert_eq!(
            badges.get(&root.join("docs")),
            Some(ExplorerGitBadge::Added)
        );
        assert!(badges.get(&root.join("unchanged.rs")).is_none());
        assert!(badges.get(&PathBuf::from("C:/elsewhere/src")).is_none());
    }

    #[test]
    fn unknown_snapshot_and_bad_paths_never_invent_a_status() {
        assert!(ExplorerGitBadges::from_snapshot(None)
            .get(Path::new("C:/repo/file"))
            .is_none());
        let snapshot = GitSnapshot {
            branch: "feature".into(),
            upstream: None,
            upstream_configured: false,
            has_stashes: false,
            has_head_commit: true,
            repository_root: PathBuf::from("C:/repo"),
            truncated: true,
            changed_files: vec![
                GitChange {
                    relative: PathBuf::from("../outside"),
                    label: "M",
                    status: git2::Status::WT_MODIFIED,
                },
                GitChange {
                    relative: PathBuf::from("safe"),
                    label: "?",
                    status: git2::Status::WT_NEW,
                },
                GitChange {
                    relative: PathBuf::from("clean"),
                    label: "·",
                    status: git2::Status::CURRENT,
                },
            ],
        };
        let badges = ExplorerGitBadges::from_snapshot(Some(&snapshot));
        assert!(badges.get(Path::new("C:/outside")).is_none());
        assert!(badges.get(Path::new("C:/repo/clean")).is_none());
        assert_eq!(
            badges.get(Path::new("C:/repo/safe")),
            Some(ExplorerGitBadge::Untracked)
        );
    }
}
