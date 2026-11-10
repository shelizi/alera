//! Workspace-bound in-app Explorer drag payload.
//! Never trusts the directory target or source list until drop time.
use crate::explorer_ops::{ExplorerClipboardBatch, ExplorerClipboardMode, ExplorerFiles};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerDragBatch {
    root: PathBuf,
    generation: u64,
    sources: Vec<PathBuf>,
}

impl ExplorerDragBatch {
    /// Dragging an already-selected row moves the selected top-level items;
    /// dragging a row outside the selection moves that row alone.
    pub fn for_row(root: &Path, generation: u64, row: &Path, selected: &[PathBuf]) -> Self {
        let sources = if selected.iter().any(|source| row.starts_with(source)) {
            selected.to_vec()
        } else {
            vec![row.to_path_buf()]
        };
        Self {
            root: root.to_path_buf(),
            generation,
            sources,
        }
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// A clean file still has an active editor identity and original save
    /// target. Moving it underneath that editor would leave a stale path;
    /// fail closed until the file is closed (or tab relocation is supported).
    pub fn affects_open_files(&self, open_paths: &[PathBuf]) -> bool {
        self.sources
            .iter()
            .any(|source| open_paths.iter().any(|open| open.starts_with(source)))
    }

    /// Revalidate everything at drop time. The existing batch Cut preflight
    /// protects symlinks, Git internals, overwrite/cycles and duplicate names.
    /// Unexpected filesystem errors can still leave a partial move and must
    /// be reported as such by the caller.
    pub fn move_into(
        &self,
        files: &ExplorerFiles,
        target_directory: &Path,
        current_generation: u64,
    ) -> io::Result<Vec<PathBuf>> {
        if current_generation != self.generation || files.root() != self.root {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Explorer drag belongs to a different workspace generation",
            ));
        }
        let mut batch =
            ExplorerClipboardBatch::capture(files, &self.sources, ExplorerClipboardMode::Cut)?;
        batch.paste_into(files, target_directory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn group_drag_moves_only_selected_top_level_items() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        for dir in ["from", "to"] {
            fs::create_dir(root.join(dir)).unwrap();
        }
        let one = root.join("from/one.txt");
        let two = root.join("from/two.txt");
        fs::write(&one, "one").unwrap();
        fs::write(&two, "two").unwrap();
        let files = ExplorerFiles::new(&root).unwrap();
        let drag = ExplorerDragBatch::for_row(&root, 4, &one, &[one.clone(), two.clone()]);
        assert_eq!(drag.len(), 2);
        let moved = drag.move_into(&files, &root.join("to"), 4).unwrap();
        assert_eq!(moved.len(), 2);
        assert!(!one.exists());
        assert!(!two.exists());
        assert_eq!(fs::read_to_string(root.join("to/one.txt")).unwrap(), "one");
        assert_eq!(fs::read_to_string(root.join("to/two.txt")).unwrap(), "two");
    }

    #[test]
    fn stale_workspace_cycle_and_name_collision_do_not_move_anything() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        fs::create_dir(root.join("folder")).unwrap();
        fs::create_dir(root.join("folder/child")).unwrap();
        fs::create_dir(root.join("to")).unwrap();
        fs::write(root.join("folder/a"), "a").unwrap();
        fs::write(root.join("to/a"), "occupied").unwrap();
        let files = ExplorerFiles::new(&root).unwrap();
        let folder = root.join("folder");
        let drag = ExplorerDragBatch::for_row(&root, 10, &folder, &[]);
        assert!(drag
            .move_into(&files, &root.join("folder/child"), 10)
            .is_err());
        assert!(drag.move_into(&files, &root.join("to"), 11).is_err());
        assert!(folder.exists());
        let file = folder.join("a");
        let drag_file = ExplorerDragBatch::for_row(&root, 10, &file, &[root.join("to/a")]);
        assert!(drag_file.move_into(&files, &root.join("to"), 10).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "a");
        assert_eq!(fs::read_to_string(root.join("to/a")).unwrap(), "occupied");
    }

    #[test]
    fn selected_parent_subsumes_child_and_open_file_blocks_move() {
        let root = PathBuf::from("C:/safe");
        let parent = root.join("src");
        let child = parent.join("nested.txt");
        let drag = ExplorerDragBatch::for_row(&root, 3, &child, &[parent.clone()]);
        assert_eq!(drag.sources, vec![parent]);
        assert!(drag.affects_open_files(&[child.clone()]));
        assert!(!drag.affects_open_files(&[root.join("other.txt")]));
        let single = ExplorerDragBatch::for_row(&root, 3, &child, &[root.join("other.txt")]);
        assert_eq!(single.sources, vec![child]);
    }
}
