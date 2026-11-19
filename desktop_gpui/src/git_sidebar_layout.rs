//! Pure, bounded projection of Git changes into Classic-like sidebar sections,
//! folders and files. No repository I/O or GPUI in this module.
//! Filtering, collapse state and layout mode must not change Git index state.

use crate::git_inspect::{GitChangeArea, GitChangeRow};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub type SectionKey = Option<GitChangeArea>;
pub type FolderKey = (SectionKey, PathBuf);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitSidebarGroupMode {
    ByArea,
    Unified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitSidebarViewMode {
    Flat,
    Tree,
}

#[derive(Clone, Debug)]
pub struct GitSidebarOptions {
    pub filter: String,
    pub grouping: GitSidebarGroupMode,
    pub view: GitSidebarViewMode,
    pub collapsed_sections: BTreeSet<SectionKey>,
    pub collapsed_folders: BTreeSet<FolderKey>,
}

impl Default for GitSidebarOptions {
    fn default() -> Self {
        Self {
            filter: String::new(),
            grouping: GitSidebarGroupMode::ByArea,
            view: GitSidebarViewMode::Flat,
            collapsed_sections: BTreeSet::new(),
            collapsed_folders: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitSidebarEntry {
    Section {
        key: SectionKey,
        total: usize,
        matching: usize,
        expanded: bool,
    },
    Folder {
        key: FolderKey,
        depth: usize,
        total: usize,
        matching: usize,
        expanded: bool,
    },
    File {
        change: GitChangeRow,
        depth: usize,
    },
}

impl GitSidebarEntry {
    pub fn file(&self) -> Option<&GitChangeRow> {
        match self {
            Self::File { change, .. } => Some(change),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GitSidebarLayout {
    pub entries: Vec<GitSidebarEntry>,
    /// Total matching files irrespective of collapse state.
    pub matches: usize,
    /// Total GitChangeRow entries, including staged + unstaged copies.
    pub total: usize,
}

/// Section bulk actions are based on the unfiltered Git snapshot, matching
/// Classic's section counts even when a search hides some file rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitSidebarSectionActions {
    pub stage: Vec<PathBuf>,
    pub unstage: Vec<PathBuf>,
}

pub fn section_actions(all: &[GitChangeRow], section: SectionKey) -> GitSidebarSectionActions {
    let mut actions = GitSidebarSectionActions::default();
    for row in all.iter().filter(|row| belongs(row, section)) {
        if row.area.can_stage() {
            actions.stage.push(row.relative.clone());
        }
        if row.area.can_unstage() {
            actions.unstage.push(row.relative.clone());
        }
    }
    actions.stage.sort();
    actions.stage.dedup();
    actions.unstage.sort();
    actions.unstage.dedup();
    actions
}

impl GitSidebarLayout {
    /// Rows in actual rendered order only. Shift selection must not traverse
    /// invisible rows in collapsed sections/folders.
    pub fn visible_files(&self) -> Vec<GitChangeRow> {
        self.entries
            .iter()
            .filter_map(|entry| entry.file().cloned())
            .collect()
    }
}

const AREA_ORDER: [GitChangeArea; 4] = [
    GitChangeArea::Conflicts,
    GitChangeArea::Staged,
    GitChangeArea::Changes,
    GitChangeArea::Untracked,
];

pub fn section_keys(mode: GitSidebarGroupMode, all: &[GitChangeRow]) -> Vec<SectionKey> {
    match mode {
        GitSidebarGroupMode::Unified => {
            if all.is_empty() {
                Vec::new()
            } else {
                vec![None]
            }
        }
        GitSidebarGroupMode::ByArea => AREA_ORDER
            .into_iter()
            .filter(|area| all.iter().any(|row| row.area == *area))
            .map(Some)
            .collect(),
    }
}

fn belongs(change: &GitChangeRow, section: SectionKey) -> bool {
    section.is_none_or(|area| change.area == area)
}

fn matches_filter(change: &GitChangeRow, filter: &str) -> bool {
    change
        .relative
        .to_string_lossy()
        .to_lowercase()
        .contains(filter)
}

fn ancestor_folders(path: &Path) -> Vec<PathBuf> {
    let mut ancestors = Vec::new();
    let mut current = PathBuf::new();
    if let Some(parent) = path.parent() {
        for component in parent.components() {
            current.push(component.as_os_str());
            ancestors.push(current.clone());
        }
    }
    ancestors
}

fn folder_counts(rows: &[&GitChangeRow]) -> BTreeMap<PathBuf, usize> {
    let mut counts = BTreeMap::new();
    for change in rows {
        for ancestor in ancestor_folders(&change.relative) {
            *counts.entry(ancestor).or_insert(0) += 1;
        }
    }
    counts
}

/// All inputs are UI snapshots, not operations. Group membership and totals
/// are calculated before filtering, so "3 / 9" remains accurate while typing.
pub fn layout_git_sidebar(all: &[GitChangeRow], options: &GitSidebarOptions) -> GitSidebarLayout {
    let filter = options.filter.trim().to_lowercase();
    let mut layout = GitSidebarLayout {
        entries: Vec::new(),
        matches: 0,
        total: all.len(),
    };

    for section in section_keys(options.grouping, all) {
        let section_all = all
            .iter()
            .filter(|row| belongs(row, section))
            .collect::<Vec<_>>();
        let mut matching = section_all
            .iter()
            .filter(|row| matches_filter(row, &filter))
            .copied()
            .collect::<Vec<_>>();
        layout.matches += matching.len();
        if matching.is_empty() {
            continue;
        }
        matching.sort_by(|a, b| {
            a.relative
                .cmp(&b.relative)
                .then_with(|| a.area.cmp(&b.area))
        });
        let expanded = !options.collapsed_sections.contains(&section);
        layout.entries.push(GitSidebarEntry::Section {
            key: section,
            total: section_all.len(),
            matching: matching.len(),
            expanded,
        });
        if !expanded {
            continue;
        }

        if options.view == GitSidebarViewMode::Flat {
            for change in matching {
                layout.entries.push(GitSidebarEntry::File {
                    change: change.clone(),
                    depth: 0,
                });
            }
            continue;
        }

        let all_folders = folder_counts(&section_all);
        let matching_folders = folder_counts(&matching);
        let mut seen_folders = BTreeSet::new();
        for change in matching {
            let mut hidden = false;
            let ancestors = ancestor_folders(&change.relative);
            for (depth, folder) in ancestors.iter().enumerate() {
                let key = (section, folder.clone());
                if seen_folders.insert(folder.clone()) && !hidden {
                    layout.entries.push(GitSidebarEntry::Folder {
                        key: key.clone(),
                        depth,
                        total: *all_folders.get(folder).unwrap_or(&0),
                        matching: *matching_folders.get(folder).unwrap_or(&0),
                        expanded: !options.collapsed_folders.contains(&key),
                    });
                }
                if options.collapsed_folders.contains(&key) {
                    hidden = true;
                    break;
                }
            }
            if !hidden {
                layout.entries.push(GitSidebarEntry::File {
                    change: change.clone(),
                    depth: ancestors.len(),
                });
            }
        }
    }

    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(area: GitChangeArea, path: &str) -> GitChangeRow {
        GitChangeRow {
            area,
            relative: PathBuf::from(path),
            label: "M",
        }
    }

    fn sample() -> Vec<GitChangeRow> {
        vec![
            file(GitChangeArea::Changes, "src/ui/main.rs"),
            file(GitChangeArea::Staged, "src/ui/main.rs"),
            file(GitChangeArea::Changes, "src/utils/mod.rs"),
            file(GitChangeArea::Untracked, "notes.txt"),
            file(GitChangeArea::Conflicts, "src/conflict.rs"),
        ]
    }

    #[test]
    fn sections_have_real_counts_and_staged_worktree_copies_are_distinct() {
        let snapshot = sample();
        let layout = layout_git_sidebar(&snapshot, &GitSidebarOptions::default());
        assert_eq!(layout.total, 5);
        assert_eq!(layout.matches, 5);
        let titles = layout
            .entries
            .iter()
            .filter_map(|row| match row {
                GitSidebarEntry::Section {
                    key,
                    total,
                    matching,
                    ..
                } => Some((*key, *total, *matching)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            titles,
            vec![
                (Some(GitChangeArea::Conflicts), 1, 1),
                (Some(GitChangeArea::Staged), 1, 1),
                (Some(GitChangeArea::Changes), 2, 2),
                (Some(GitChangeArea::Untracked), 1, 1),
            ]
        );
        assert_eq!(layout.visible_files().len(), 5);
        assert_eq!(
            layout
                .visible_files()
                .iter()
                .filter(|r| r.relative == Path::new("src/ui/main.rs"))
                .count(),
            2
        );
    }

    #[test]
    fn filter_counts_remain_grounded_in_unfiltered_state() {
        let mut opts = GitSidebarOptions::default();
        opts.filter = "MAIN.RS".into();
        let layout = layout_git_sidebar(&sample(), &opts);
        assert_eq!(layout.total, 5);
        assert_eq!(layout.matches, 2);
        assert_eq!(layout.visible_files().len(), 2);
        let groups = layout
            .entries
            .iter()
            .filter_map(|e| match e {
                GitSidebarEntry::Section {
                    total, matching, ..
                } => Some((*total, *matching)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(groups, vec![(1, 1), (2, 1)]);
    }

    #[test]
    fn tree_groups_nested_folders_and_collapsing_hides_descendants_only() {
        let mut opts = GitSidebarOptions {
            view: GitSidebarViewMode::Tree,
            ..GitSidebarOptions::default()
        };
        let all = sample();
        let expanded = layout_git_sidebar(&all, &opts);
        let changes_folders = expanded
            .entries
            .iter()
            .filter_map(|e| match e {
                GitSidebarEntry::Folder {
                    key: (Some(GitChangeArea::Changes), path),
                    depth,
                    ..
                } => Some((path.to_string_lossy().replace('\\', "/"), *depth)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            changes_folders,
            vec![
                ("src".into(), 0),
                ("src/ui".into(), 1),
                ("src/utils".into(), 1)
            ]
        );
        opts.collapsed_folders
            .insert((Some(GitChangeArea::Changes), PathBuf::from("src/ui")));
        let collapsed = layout_git_sidebar(&all, &opts);
        assert!(collapsed
            .visible_files()
            .iter()
            .any(|r| r.area == GitChangeArea::Staged && r.relative == Path::new("src/ui/main.rs")));
        assert!(
            !collapsed
                .visible_files()
                .iter()
                .any(|r| r.area == GitChangeArea::Changes
                    && r.relative == Path::new("src/ui/main.rs"))
        );
        assert!(collapsed.visible_files().iter().any(
            |r| r.area == GitChangeArea::Changes && r.relative == Path::new("src/utils/mod.rs")
        ));
    }

    #[test]
    fn section_collapse_preserves_correct_counts_and_hides_shift_targets() {
        let mut opts = GitSidebarOptions::default();
        opts.collapsed_sections.insert(Some(GitChangeArea::Changes));
        let layout = layout_git_sidebar(&sample(), &opts);
        assert_eq!(layout.matches, 5);
        assert_eq!(layout.visible_files().len(), 3);
        assert!(layout.entries.iter().any(|e| matches!(
            e,
            GitSidebarEntry::Section {
                key: Some(GitChangeArea::Changes),
                total: 2,
                expanded: false,
                ..
            }
        )));
    }

    #[test]
    fn unified_group_keeps_staged_and_worktree_as_separate_file_rows() {
        let mut opts = GitSidebarOptions {
            grouping: GitSidebarGroupMode::Unified,
            view: GitSidebarViewMode::Tree,
            ..GitSidebarOptions::default()
        };
        let all = sample();
        let grouped = layout_git_sidebar(&all, &opts);
        assert_eq!(
            grouped
                .entries
                .iter()
                .filter(|e| matches!(e, GitSidebarEntry::Section { .. }))
                .count(),
            1
        );
        assert_eq!(grouped.visible_files().len(), 5);
        assert_eq!(
            grouped
                .entries
                .iter()
                .filter(|e| matches!(e, GitSidebarEntry::Folder { key: (None, _), .. }))
                .count(),
            3
        );
        opts.collapsed_folders
            .insert((None, PathBuf::from("src/ui")));
        let collapsed = layout_git_sidebar(&all, &opts);
        assert_eq!(collapsed.visible_files().len(), 3);
    }

    #[test]
    fn section_actions_use_all_rows_and_keep_index_areas_separate() {
        let all = sample();
        assert_eq!(
            section_actions(&all, Some(GitChangeArea::Changes)).stage,
            vec![
                PathBuf::from("src/ui/main.rs"),
                PathBuf::from("src/utils/mod.rs")
            ]
        );
        assert_eq!(
            section_actions(&all, Some(GitChangeArea::Staged)).unstage,
            vec![PathBuf::from("src/ui/main.rs")]
        );
        let unified = section_actions(&all, None);
        assert_eq!(unified.stage.len(), 3);
        assert_eq!(unified.unstage.len(), 1);
        assert!(!unified.stage.contains(&PathBuf::from("src/conflict.rs")));
    }
}
