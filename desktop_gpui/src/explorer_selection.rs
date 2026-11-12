//! Workspace-scoped Explorer multi-selection shared by tree/flat/filter rows.
//! Selection alone never opens, modifies, or reads a file.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const MAX_SELECTED: usize = 128;

/// Keyboard navigation is scoped to the current visible Explorer rows.
/// Moving the highlight never opens a file or performs a filesystem write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerNavigation {
    Previous,
    Next,
    First,
    Last,
}

#[derive(Debug, Clone)]
pub struct ExplorerMultiSelection {
    root: PathBuf,
    paths: BTreeSet<PathBuf>,
    anchor: Option<PathBuf>,
}

impl ExplorerMultiSelection {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            paths: BTreeSet::new(),
            anchor: None,
        }
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.paths.contains(path)
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// GPUI ScrollHandle indexes *painted direct children*, not filesystem
    /// paths. Use the exact visible order (filter/tree/flat) rather than an
    /// index from a different presentation or a guessed row height.
    pub fn visible_row_index(&self, path: &Path, visible: &[PathBuf]) -> Option<usize> {
        if path == self.root || !path.starts_with(&self.root) {
            return None;
        }
        visible.iter().position(|row| row == path)
    }

    /// A stale selection (after refresh/filter/collapse) is not used to
    /// calculate an out-of-range target. Invalid rows can never be selected.
    pub fn navigate(
        &mut self,
        selected: Option<&Path>,
        visible: &[PathBuf],
        direction: ExplorerNavigation,
    ) -> Option<PathBuf> {
        let items: Vec<_> = visible
            .iter()
            .filter(|path| path.starts_with(&self.root) && *path != &self.root)
            .collect();
        if items.is_empty() {
            return None;
        }
        let current =
            selected.and_then(|path| items.iter().position(|item| item.as_path() == path));
        let index = match direction {
            ExplorerNavigation::First => 0,
            ExplorerNavigation::Last => items.len() - 1,
            ExplorerNavigation::Previous => current
                .map(|index| index.saturating_sub(1))
                .unwrap_or(items.len() - 1),
            ExplorerNavigation::Next => current
                .map(|index| (index + 1).min(items.len() - 1))
                .unwrap_or(0),
        };
        let next = items[index].to_path_buf();
        // Reset a previous Ctrl/Shift range just like a plain row click.
        if !self.click(&next, visible, false, false) {
            return None;
        }
        Some(next)
    }

    pub fn clear(&mut self) {
        self.paths.clear();
        self.anchor = None;
    }

    /// Ctrl+A selects only currently visible workspace rows, bounded so a
    /// large result set cannot silently become an unbounded batch mutation.
    pub fn select_visible_all(&mut self, visible: &[PathBuf]) -> usize {
        self.clear();
        for path in visible {
            if self.paths.len() >= MAX_SELECTED {
                break;
            }
            if path.starts_with(&self.root) {
                self.paths.insert(path.clone());
                self.anchor.get_or_insert_with(|| path.clone());
            }
        }
        self.paths.len()
    }

    /// Return false when the clicked path was not a current, visible row.
    /// Ctrl toggles one row, Shift selects the inclusive visual range, and
    /// a plain click clears the old range. The anchor remains steady for
    /// repeated Shift clicks until a fresh Ctrl/plain click.
    pub fn click(&mut self, path: &Path, visible: &[PathBuf], ctrl: bool, shift: bool) -> bool {
        if !path.starts_with(&self.root) || !visible.iter().any(|entry| entry == path) {
            return false;
        }
        if shift {
            if let Some(anchor_index) = self
                .anchor
                .as_ref()
                .and_then(|anchor| visible.iter().position(|item| item == anchor))
            {
                let current = visible.iter().position(|item| item == path).unwrap();
                let (lo, hi) = if anchor_index < current {
                    (anchor_index, current)
                } else {
                    (current, anchor_index)
                };
                if !ctrl {
                    self.paths.clear();
                }
                for item in &visible[lo..=hi] {
                    if item.starts_with(&self.root) && self.paths.len() < MAX_SELECTED {
                        self.paths.insert(item.clone());
                    }
                }
                return true;
            }
        }
        if ctrl {
            if !self.paths.remove(path) && self.paths.len() < MAX_SELECTED {
                self.paths.insert(path.to_path_buf());
            }
        } else {
            self.paths.clear();
            self.paths.insert(path.to_path_buf());
        }
        self.anchor = Some(path.to_path_buf());
        true
    }

    /// Right-clicking any already-selected item preserves the group. A
    /// nonselected item changes the target to that single item instead.
    pub fn context_target(&mut self, path: &Path) {
        if path.starts_with(&self.root) && !self.paths.contains(path) {
            self.paths.clear();
            self.paths.insert(path.to_path_buf());
            self.anchor = Some(path.to_path_buf());
        }
    }

    /// A selected directory subsumes its selected descendants for copy,
    /// move and delete; the returned order is stable for tests/UI.
    pub fn top_level_paths(&self) -> Vec<PathBuf> {
        self.paths
            .iter()
            .filter(|path| {
                let mut parent = path.parent();
                while let Some(candidate) = parent {
                    if candidate == self.root {
                        break;
                    }
                    if self.paths.contains(candidate) {
                        return false;
                    }
                    parent = candidate.parent();
                }
                true
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_target_index_matches_current_visible_presentation_only() {
        let root = PathBuf::from("C:/safe");
        let nested = root.join("src/main.rs");
        let sibling = root.join("README.md");
        let hidden = root.join("src/hidden.rs");
        let selection = ExplorerMultiSelection::new(&root);
        let filtered = vec![nested.clone(), sibling.clone()];
        let tree = vec![root.join("src"), sibling.clone(), nested.clone()];
        assert_eq!(selection.visible_row_index(&nested, &filtered), Some(0));
        assert_eq!(selection.visible_row_index(&nested, &tree), Some(2));
        assert_eq!(selection.visible_row_index(&hidden, &filtered), None);
        assert_eq!(selection.visible_row_index(&root, &filtered), None);
        assert_eq!(
            selection.visible_row_index(&PathBuf::from("C:/outside"), &tree),
            None
        );
    }

    #[test]
    fn arrow_navigation_keeps_one_visible_target_and_clamps_ends() {
        let root = PathBuf::from("C:/safe");
        let rows: Vec<_> = ["a", "b", "c"].iter().map(|name| root.join(name)).collect();
        let mut selection = ExplorerMultiSelection::new(&root);
        let first = selection
            .navigate(None, &rows, ExplorerNavigation::Next)
            .unwrap();
        assert_eq!(first, rows[0]);
        let second = selection
            .navigate(Some(&first), &rows, ExplorerNavigation::Next)
            .unwrap();
        assert_eq!(second, rows[1]);
        assert_eq!(selection.len(), 1);
        assert!(!selection.contains(&rows[0]));
        assert!(selection.contains(&rows[1]));
        assert_eq!(
            selection.navigate(Some(&second), &rows, ExplorerNavigation::First),
            Some(rows[0].clone())
        );
        assert_eq!(
            selection.navigate(Some(&rows[0]), &rows, ExplorerNavigation::Previous),
            Some(rows[0].clone())
        );
        assert_eq!(
            selection.navigate(Some(&rows[2]), &rows, ExplorerNavigation::Next),
            Some(rows[2].clone())
        );
        assert_eq!(
            selection.navigate(None, &rows, ExplorerNavigation::Previous),
            Some(rows[2].clone())
        );
    }

    #[test]
    fn keyboard_navigation_rejects_outside_rows_and_stale_selected_path() {
        let root = PathBuf::from("C:/safe");
        let outside = PathBuf::from("C:/other/private");
        let first = root.join("one");
        let second = root.join("two");
        let visible = vec![outside.clone(), first.clone(), second.clone()];
        let mut selection = ExplorerMultiSelection::new(&root);
        assert_eq!(
            selection.navigate(Some(&outside), &visible, ExplorerNavigation::Next),
            Some(first)
        );
        assert_eq!(
            selection.navigate(
                Some(&root.join("missing")),
                &visible,
                ExplorerNavigation::Last
            ),
            Some(second)
        );
        assert!(selection
            .navigate(None, &[outside], ExplorerNavigation::Next)
            .is_none());
        assert_eq!(
            selection.len(),
            1,
            "an unavailable view must not discard earlier selection"
        );
    }

    #[test]
    fn select_all_only_accepts_visible_workspace_rows_and_obeys_cap() {
        let root = PathBuf::from("C:/safe");
        let mut visible = vec![PathBuf::from("C:/other/secret")];
        visible.extend((0..140).map(|i| root.join(format!("file-{i}"))));
        let mut selection = ExplorerMultiSelection::new(&root);
        assert_eq!(selection.select_visible_all(&visible), MAX_SELECTED);
        assert!(!selection.contains(&visible[0]));
        assert!(selection.contains(&visible[1]));
        assert!(!selection.contains(visible.last().unwrap()));
        assert_eq!(selection.select_visible_all(&[]), 0);
        assert!(selection.top_level_paths().is_empty());
    }

    #[test]
    fn ctrl_shift_selection_follows_visible_order_and_stable_anchor() {
        let root = PathBuf::from("C:/safe");
        let rows: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|name| root.join(name))
            .collect();
        let mut selection = ExplorerMultiSelection::new(&root);
        assert!(selection.click(&rows[1], &rows, false, false));
        assert!(selection.click(&rows[3], &rows, false, true));
        assert_eq!(selection.len(), 3);
        assert!(selection.contains(&rows[1]));
        assert!(selection.contains(&rows[2]));
        assert!(selection.contains(&rows[3]));
        assert!(selection.click(&rows[0], &rows, true, false));
        assert_eq!(selection.len(), 4);
        assert!(selection.click(&rows[2], &rows, false, true));
        assert_eq!(selection.len(), 3);
        assert!(!selection.contains(&rows[3]));
        assert!(!selection.click(&PathBuf::from("C:/other/file"), &rows, false, false));
        assert!(!selection.click(&root.join("not-listed"), &rows, false, false));
        assert_eq!(selection.len(), 3);
    }

    #[test]
    fn right_click_keeps_group_and_parent_subsumes_children() {
        let root = PathBuf::from("C:/safe");
        let rows = vec![root.join("dir"), root.join("dir/a"), root.join("other")];
        let mut selected = ExplorerMultiSelection::new(&root);
        selected.click(&rows[0], &rows, false, false);
        selected.click(&rows[1], &rows, true, false);
        selected.context_target(&rows[1]);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected.top_level_paths(), vec![rows[0].clone()]);
        selected.context_target(&rows[2]);
        assert_eq!(selected.top_level_paths(), vec![rows[2].clone()]);
        selected.clear();
        assert_eq!(selected.len(), 0);
    }

    #[test]
    fn selection_cannot_exceed_limit() {
        let root = PathBuf::from("C:/safe");
        let rows: Vec<_> = (0..200).map(|i| root.join(format!("{i:03}"))).collect();
        let mut selection = ExplorerMultiSelection::new(&root);
        assert!(selection.click(&rows[0], &rows, false, false));
        assert!(selection.click(&rows[199], &rows, false, true));
        assert_eq!(selection.len(), MAX_SELECTED);
    }
}
