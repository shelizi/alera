//! Selection of real worktree files from the Source Control list.
use crate::git_inspect::GitChangeRow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Git index entries are not filesystem objects: file operations always use
/// the current working tree. A selected row acts on selected visible rows,
/// deduplicating files that appear in both staged and unstaged groups.
pub fn selected_worktree_paths(
    root: &Path,
    row: &GitChangeRow,
    selected: &HashSet<GitChangeRow>,
    visible: &[GitChangeRow],
) -> Vec<PathBuf> {
    let mut relatives = if selected.contains(row) {
        visible
            .iter()
            .filter(|entry| selected.contains(*entry))
            .map(|entry| entry.relative.clone())
            .collect::<Vec<_>>()
    } else {
        vec![row.relative.clone()]
    };
    relatives.sort();
    relatives.dedup();
    relatives.truncate(120);
    relatives.into_iter().map(|path| root.join(path)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_inspect::GitChangeArea;

    fn row(path: &str, area: GitChangeArea) -> GitChangeRow {
        GitChangeRow {
            relative: path.into(),
            label: "M",
            area,
        }
    }

    #[test]
    fn selects_once_across_git_areas() {
        let root = Path::new("/repo");
        let staged = row("src/a.rs", GitChangeArea::Staged);
        let unstaged = row("src/a.rs", GitChangeArea::Changes);
        let other = row("src/b.rs", GitChangeArea::Untracked);
        let selected = HashSet::from([staged.clone(), unstaged.clone(), other.clone()]);
        let visible = vec![staged.clone(), unstaged, other];
        assert_eq!(
            selected_worktree_paths(root, &staged, &selected, &visible),
            vec![root.join("src/a.rs"), root.join("src/b.rs")]
        );
        let unselected = row("src/c.rs", GitChangeArea::Changes);
        assert_eq!(
            selected_worktree_paths(root, &unselected, &selected, &visible),
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
            selected_worktree_paths(root, &clicked.clone(), &selected, &[clicked]),
            vec![root.join("one")]
        );
    }
}
