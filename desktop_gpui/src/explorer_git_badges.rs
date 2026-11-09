//! Read-only Explorer Git decorations derived from the existing bounded
//! GitSnapshot. No Git status read is performed while rendering a row.
use crate::git_inspect::GitSnapshot;
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
            let mut path = snapshot.repository_root.join(&change.relative);
            // Mark changed files and their parent directories; stop at the
            // repository root and never decorate outside it.
            while path.starts_with(&snapshot.repository_root) && path != snapshot.repository_root {
                let current = entries.entry(path.clone()).or_insert(badge);
                if badge.priority() > current.priority() {
                    *current = badge;
                }
                let Some(parent) = path.parent() else { break };
                path = parent.to_path_buf();
            }
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

    #[test]
    fn status_badges_follow_changed_files_and_aggregate_directory_priority() {
        let root = PathBuf::from("C:/repo");
        let snapshot = GitSnapshot {
            branch: "feature".into(),
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
