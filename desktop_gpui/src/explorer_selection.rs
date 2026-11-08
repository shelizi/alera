//! Workspace-scoped Explorer multi-selection shared by tree/flat/filter rows.
//! Selection alone never opens, modifies, or reads a file.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const MAX_SELECTED: usize = 128;

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

    pub fn clear(&mut self) {
        self.paths.clear();
        self.anchor = None;
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
