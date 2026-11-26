//! Selection of real worktree files from the Source Control list.
use crate::git_inspect::GitChangeRow;
use crate::git_sidebar_layout::{FolderKey, GitSidebarSelectionItem};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Copy and paste must use a current visible selection. A filtered or
/// deselected anchor must not become an invisible paste destination.
pub fn preferred_visible_selection<'a>(
    anchor: Option<&GitSidebarSelectionItem>,
    selected_files: &HashSet<GitChangeRow>,
    selected_folders: &HashSet<FolderKey>,
    visible: &'a [GitSidebarSelectionItem],
) -> Option<&'a GitSidebarSelectionItem> {
    let is_selected = |item: &GitSidebarSelectionItem| match item {
        GitSidebarSelectionItem::File(file) => selected_files.contains(file),
        GitSidebarSelectionItem::Folder(folder) => selected_folders.contains(folder),
    };
    visible
        .iter()
        .find(|item| Some(*item) == anchor && is_selected(item))
        .or_else(|| visible.iter().find(|item| is_selected(item)))
}

/// Git index entries are not filesystem objects: operations always use the
/// working tree. A selected file or folder operates on selected visible rows
/// (including mixed selections), removes duplicate staged/unstaged paths, and
/// collapses a selected folder's descendants into the one top-level folder.
pub fn selected_worktree_paths(
    root: &Path,
    row: &GitSidebarSelectionItem,
    selected_files: &HashSet<GitChangeRow>,
    selected_folders: &HashSet<FolderKey>,
    visible: &[GitSidebarSelectionItem],
) -> Vec<PathBuf> {
    let is_selected = |item: &GitSidebarSelectionItem| match item {
        GitSidebarSelectionItem::File(file) => selected_files.contains(file),
        GitSidebarSelectionItem::Folder(folder) => selected_folders.contains(folder),
    };
    let mut relatives = if is_selected(row) {
        visible
            .iter()
            .filter(|entry| is_selected(entry))
            .map(|entry| entry.relative_path().to_path_buf())
            .collect::<Vec<_>>()
    } else {
        vec![row.relative_path().to_path_buf()]
    };
    relatives.sort();
    relatives.dedup();
    let mut parents = Vec::<PathBuf>::new();
    for relative in relatives {
        if !parents.iter().any(|parent| relative.starts_with(parent)) {
            parents.push(relative);
        }
    }
    parents.truncate(120);
    parents.into_iter().map(|path| root.join(path)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer_drag::ExplorerDragBatch;
    use crate::explorer_ops::{ExplorerClipboardBatch, ExplorerClipboardMode, ExplorerFiles};
    use crate::git_inspect::GitChangeArea;
    use std::fs;

    fn row(path: &str, area: GitChangeArea) -> GitChangeRow {
        GitChangeRow {
            relative: path.into(),
            label: "M",
            area,
        }
    }

    #[test]
    fn keyboard_target_prefers_visible_selected_anchor_and_never_hidden_rows() {
        let first = row("first.rs", GitChangeArea::Changes);
        let hidden = row("hidden.rs", GitChangeArea::Changes);
        let folder = (None, PathBuf::from("folder"));
        let first_item = GitSidebarSelectionItem::File(first.clone());
        let hidden_item = GitSidebarSelectionItem::File(hidden.clone());
        let folder_item = GitSidebarSelectionItem::Folder(folder.clone());
        let visible = vec![first_item.clone(), folder_item.clone()];
        let files = HashSet::from([first, hidden]);
        let folders = HashSet::from([folder]);
        assert_eq!(
            preferred_visible_selection(Some(&folder_item), &files, &folders, &visible),
            Some(&folder_item)
        );
        assert_eq!(
            preferred_visible_selection(Some(&hidden_item), &files, &folders, &visible),
            Some(&first_item)
        );
        assert_eq!(
            preferred_visible_selection(Some(&first_item), &HashSet::new(), &folders, &visible),
            Some(&folder_item)
        );
        assert_eq!(
            preferred_visible_selection(Some(&hidden_item), &files, &folders, &[]),
            None
        );
    }

    #[test]
    fn selects_once_across_git_areas() {
        let root = Path::new("/repo");
        let staged = row("src/a.rs", GitChangeArea::Staged);
        let unstaged = row("src/a.rs", GitChangeArea::Changes);
        let other = row("src/b.rs", GitChangeArea::Untracked);
        let selected = HashSet::from([staged.clone(), unstaged.clone(), other.clone()]);
        let visible = vec![staged.clone(), unstaged, other]
            .into_iter()
            .map(GitSidebarSelectionItem::File)
            .collect::<Vec<_>>();
        assert_eq!(
            selected_worktree_paths(
                root,
                &GitSidebarSelectionItem::File(staged),
                &selected,
                &HashSet::new(),
                &visible
            ),
            vec![root.join("src/a.rs"), root.join("src/b.rs")]
        );
        let unselected = row("src/c.rs", GitChangeArea::Changes);
        assert_eq!(
            selected_worktree_paths(
                root,
                &GitSidebarSelectionItem::File(unselected),
                &selected,
                &HashSet::new(),
                &visible
            ),
            vec![root.join("src/c.rs")]
        );
    }

    #[test]
    fn ignores_selection_no_longer_visible() {
        let root = Path::new("/repo");
        let clicked = row("one", GitChangeArea::Changes);
        let hidden = row("secret", GitChangeArea::Untracked);
        let selected = HashSet::from([clicked.clone(), hidden]);
        assert_eq!(
            selected_worktree_paths(
                root,
                &GitSidebarSelectionItem::File(clicked.clone()),
                &selected,
                &HashSet::new(),
                &[GitSidebarSelectionItem::File(clicked)]
            ),
            vec![root.join("one")]
        );
    }

    #[test]
    fn folder_and_files_mixed_selection_exports_only_top_level_paths() {
        let root = Path::new("/repo");
        let folder = (Some(GitChangeArea::Changes), PathBuf::from("src"));
        let nested = row("src/lib.rs", GitChangeArea::Changes);
        let sibling = row("notes.txt", GitChangeArea::Untracked);
        let visible = vec![
            GitSidebarSelectionItem::Folder(folder.clone()),
            GitSidebarSelectionItem::File(nested.clone()),
            GitSidebarSelectionItem::File(sibling.clone()),
        ];
        let files = HashSet::from([nested, sibling]);
        let folders = HashSet::from([folder.clone()]);
        assert_eq!(
            selected_worktree_paths(
                root,
                &GitSidebarSelectionItem::Folder(folder),
                &files,
                &folders,
                &visible
            ),
            vec![root.join("notes.txt"), root.join("src")]
        );
    }

    #[test]
    fn mixed_folder_file_clipboard_and_native_drag_keep_directory_contents() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        fs::create_dir_all(root.join("src/dir")).unwrap();
        fs::create_dir(root.join("dest")).unwrap();
        fs::write(root.join("src/dir/nested.txt"), "nested").unwrap();
        fs::write(root.join("src/one.txt"), "one").unwrap();
        let folder = (Some(GitChangeArea::Changes), PathBuf::from("src/dir"));
        let file = row("src/one.txt", GitChangeArea::Untracked);
        let visible = vec![
            GitSidebarSelectionItem::Folder(folder.clone()),
            GitSidebarSelectionItem::File(file.clone()),
        ];
        let paths = selected_worktree_paths(
            &root,
            &GitSidebarSelectionItem::Folder(folder.clone()),
            &HashSet::from([file]),
            &HashSet::from([folder]),
            &visible,
        );
        let files = ExplorerFiles::new(&root).unwrap();
        let drag = ExplorerDragBatch::for_row(&root, 5, &root.join("src/dir"), &paths);
        assert_eq!(
            drag.external_entries(&files, 5).unwrap(),
            vec![
                (root.join("src/dir"), true),
                (root.join("src/one.txt"), false),
            ]
        );
        let mut batch =
            ExplorerClipboardBatch::capture(&files, &paths, ExplorerClipboardMode::Copy).unwrap();
        assert_eq!(batch.len(), 2);
        let copied = batch.paste_into(&files, &root.join("dest")).unwrap();
        assert_eq!(copied.len(), 2);
        assert_eq!(
            fs::read_to_string(root.join("dest/dir/nested.txt")).unwrap(),
            "nested"
        );
        assert_eq!(
            fs::read_to_string(root.join("dest/one.txt")).unwrap(),
            "one"
        );
        assert_eq!(
            fs::read_to_string(root.join("src/dir/nested.txt")).unwrap(),
            "nested"
        );
        fs::create_dir(root.join("moved")).unwrap();
        let mut cut =
            ExplorerClipboardBatch::capture(&files, &paths, ExplorerClipboardMode::Cut).unwrap();
        assert_eq!(
            cut.paste_into(&files, &root.join("moved")).unwrap().len(),
            2
        );
        assert!(cut.consumed());
        assert!(!root.join("src/dir").exists());
        assert!(!root.join("src/one.txt").exists());
        assert_eq!(
            fs::read_to_string(root.join("moved/dir/nested.txt")).unwrap(),
            "nested"
        );
        assert_eq!(
            fs::read_to_string(root.join("moved/one.txt")).unwrap(),
            "one"
        );
    }

    #[test]
    fn right_clicked_unselected_folder_does_not_use_unrelated_selection() {
        let root = Path::new("/repo");
        let folder = (None, PathBuf::from("different"));
        let file = row("a.rs", GitChangeArea::Changes);
        assert_eq!(
            selected_worktree_paths(
                root,
                &GitSidebarSelectionItem::Folder(folder),
                &HashSet::from([file.clone()]),
                &HashSet::new(),
                &[GitSidebarSelectionItem::File(file)]
            ),
            vec![root.join("different")]
        );
    }
}
