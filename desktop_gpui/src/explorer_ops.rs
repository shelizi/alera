//! Explicit, guarded filesystem mutations for the experimental GPUI Explorer.
//! The caller must provide an action and confirmation; merely browsing or
//! indexing must never reach this module. No symlinks or Git metadata writes.
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Read};
use std::path::{Component, Path, PathBuf};

const MAX_NAME_BYTES: usize = 255;
const MAX_RECURSIVE_ENTRIES: usize = 4_096;
const MAX_COPY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SINGLE_FILE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ExplorerFiles {
    root: PathBuf,
}

impl ExplorerFiles {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = dunce::canonicalize(root)?;
        if !root.is_dir() {
            return Err(invalid("Explorer workspace is not a directory"));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn existing(&self, candidate: &Path) -> io::Result<PathBuf> {
        let relative = candidate
            .strip_prefix(&self.root)
            .map_err(|_| forbidden("Path outside the active workspace"))?;
        if relative.as_os_str().is_empty() {
            return Err(forbidden("Cannot modify workspace root"));
        }
        let mut checked = self.root.clone();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(forbidden("Invalid relative path"));
            };
            check_name(part.to_string_lossy().as_ref())?;
            checked.push(part);
            if fs::symlink_metadata(&checked)?.file_type().is_symlink() {
                return Err(forbidden("Symlink or junction refused"));
            }
        }
        let canonical = dunce::canonicalize(&checked)?;
        if !canonical.starts_with(&self.root) || canonical != checked {
            return Err(forbidden("Path is not a canonical workspace child"));
        }
        Ok(canonical)
    }

    fn folder(&self, candidate: &Path) -> io::Result<PathBuf> {
        if candidate == self.root {
            return Ok(self.root.clone());
        }
        let path = self.existing(candidate)?;
        if !path.is_dir() {
            return Err(invalid("Destination is not a folder"));
        }
        Ok(path)
    }

    fn destination(&self, parent: &Path, name: &str) -> io::Result<PathBuf> {
        check_name(name)?;
        let parent = self.folder(parent)?;
        let dest = parent.join(name);
        match fs::symlink_metadata(&dest) {
            Ok(_) => Err(io::Error::new(
                ErrorKind::AlreadyExists,
                "Target already exists",
            )),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(dest),
            Err(error) => Err(error),
        }
    }

    pub fn create_file(&self, folder: &Path, name: &str) -> io::Result<PathBuf> {
        let path = self.destination(folder, name)?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(path)
    }

    pub fn create_folder(&self, folder: &Path, name: &str) -> io::Result<PathBuf> {
        let path = self.destination(folder, name)?;
        fs::create_dir(&path)?;
        Ok(path)
    }

    pub fn rename(&self, original: &Path, new_name: &str) -> io::Result<PathBuf> {
        let source = self.existing(original)?;
        let parent = source.parent().ok_or_else(|| forbidden("Missing parent"))?;
        let dest = self.destination(parent, new_name)?;
        fs::rename(&source, &dest)?;
        Ok(dest)
    }

    pub fn move_into(&self, original: &Path, dest_folder: &Path) -> io::Result<PathBuf> {
        let source = self.existing(original)?;
        let folder = self.folder(dest_folder)?;
        if folder == source || folder.starts_with(&source) {
            return Err(forbidden("Cannot move a folder into itself"));
        }
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("Invalid source filename"))?;
        let dest = self.destination(&folder, name)?;
        fs::rename(&source, &dest)?;
        Ok(dest)
    }

    /// Copy a normal file or directory into an existing workspace folder.
    /// Every destination is new: no overwrite, no symlink following, and
    /// copying into one's own subtree is prohibited.
    pub fn copy_into(&self, original: &Path, dest_folder: &Path) -> io::Result<PathBuf> {
        let source = self.existing(original)?;
        let folder = self.folder(dest_folder)?;
        if folder == source || folder.starts_with(&source) {
            return Err(forbidden("Cannot copy a folder into itself"));
        }
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("Invalid source filename"))?;
        let destination = self.destination(&folder, name)?;
        self.copy_to_exact(&source, &destination)?;
        Ok(destination)
    }

    /// Explicit OS file-manager drop/import. Unlike ordinary Copy/Cut this
    /// accepts an outside source, but never follows a symlink, overwrites a
    /// destination, or imports Git metadata. Limits apply to directories too.
    pub fn import_external(&self, source: &Path, dest_folder: &Path) -> io::Result<PathBuf> {
        let folder = self.folder(dest_folder)?;
        let file_type = fs::symlink_metadata(source)?.file_type();
        if !file_type.is_file() && !file_type.is_dir() {
            return Err(forbidden(
                "External import refuses symlinks and special files",
            ));
        }
        let source = dunce::canonicalize(source)?;
        if source.starts_with(&self.root) {
            return self.copy_into(&source, &folder);
        }
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("External file has no valid name"))?;
        let destination = self.destination(&folder, name)?;
        let mut pending = vec![(source.clone(), destination.clone())];
        let mut entries_left = MAX_RECURSIVE_ENTRIES;
        let mut bytes_left = MAX_COPY_BYTES;
        let mut created_root = false;
        let result = (|| -> io::Result<()> {
            while let Some((from, to)) = pending.pop() {
                if entries_left == 0 {
                    return Err(forbidden("External import exceeds entry limit"));
                }
                entries_left -= 1;
                let file_type = fs::symlink_metadata(&from)?.file_type();
                if !file_type.is_dir() && !file_type.is_file() {
                    return Err(forbidden("External import refuses symlink/special files"));
                }
                let canonical = dunce::canonicalize(&from)?;
                if !canonical.starts_with(&source) {
                    return Err(forbidden("External source escaped its selected root"));
                }
                if file_type.is_dir() {
                    fs::create_dir(&to)?;
                    if to == destination {
                        created_root = true;
                    }
                    for entry in fs::read_dir(&canonical)? {
                        let entry = entry?;
                        let name = entry.file_name();
                        check_name(
                            name.to_str()
                                .ok_or_else(|| invalid("Invalid imported filename"))?,
                        )?;
                        pending.push((entry.path(), to.join(name)));
                    }
                } else {
                    let size = fs::metadata(&canonical)?.len();
                    if size > MAX_SINGLE_FILE_BYTES || size > bytes_left {
                        return Err(forbidden("External import exceeded size budget"));
                    }
                    let reader = fs::File::open(&canonical)?;
                    let mut writer = OpenOptions::new().write(true).create_new(true).open(&to)?;
                    if to == destination {
                        created_root = true;
                    }
                    let transferred = io::copy(&mut reader.take(size + 1), &mut writer)?;
                    if transferred != size || transferred > bytes_left {
                        return Err(forbidden("External source changed while copying"));
                    }
                    bytes_left -= transferred;
                }
            }
            Ok(())
        })();
        if result.is_err() && created_root {
            // Only undo our newly created destination, never external source.
            if let Ok(meta) = fs::symlink_metadata(&destination) {
                if meta.file_type().is_dir() {
                    let _ = fs::remove_dir_all(&destination);
                } else if meta.file_type().is_file() {
                    let _ = fs::remove_file(&destination);
                }
            }
        }
        result.map(|()| destination)
    }

    /// Make a sibling copy with a collision-free name, preserving source.
    pub fn duplicate(&self, original: &Path) -> io::Result<PathBuf> {
        let source = self.existing(original)?;
        let parent = source.parent().ok_or_else(|| forbidden("Missing parent"))?;
        let stem = source
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("Invalid filename stem"))?;
        let extension = if source.is_file() {
            source.extension().and_then(|ext| ext.to_str())
        } else {
            None
        };
        for index in 1..=99 {
            let suffix = if index == 1 {
                " (copy)".to_owned()
            } else {
                format!(" (copy {index})")
            };
            let name = if let Some(extension) = extension {
                format!("{stem}{suffix}.{extension}")
            } else {
                format!("{stem}{suffix}")
            };
            match self.destination(parent, &name) {
                Ok(destination) => {
                    self.copy_to_exact(&source, &destination)?;
                    return Ok(destination);
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            ErrorKind::AlreadyExists,
            "No available duplicate filename",
        ))
    }

    fn copy_to_exact(&self, source: &Path, destination: &Path) -> io::Result<()> {
        let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
        let mut remaining_entries = MAX_RECURSIVE_ENTRIES;
        let mut remaining_bytes = MAX_COPY_BYTES;
        let mut created_destination = false;
        let result = (|| -> io::Result<()> {
            while let Some((from, to)) = pending.pop() {
                if remaining_entries == 0 {
                    return Err(forbidden("Copy exceeds the entry limit"));
                }
                remaining_entries -= 1;
                let from = self.existing(&from)?;
                let kind = fs::symlink_metadata(&from)?.file_type();
                if kind.is_dir() {
                    fs::create_dir(&to)?;
                    if to == destination {
                        created_destination = true;
                    }
                    for entry in fs::read_dir(&from)? {
                        let entry = entry?;
                        let name = entry.file_name();
                        check_name(
                            name.to_str()
                                .ok_or_else(|| invalid("Invalid child filename"))?,
                        )?;
                        pending.push((entry.path(), to.join(name)));
                    }
                } else if kind.is_file() {
                    let size = fs::metadata(&from)?.len();
                    if size > MAX_SINGLE_FILE_BYTES || size > remaining_bytes {
                        return Err(forbidden("Copy exceeds 32 MiB per file or 64 MiB total"));
                    }
                    let reader = fs::File::open(&from)?;
                    let mut writer = OpenOptions::new().write(true).create_new(true).open(&to)?;
                    if to == destination {
                        created_destination = true;
                    }
                    let copied = io::copy(&mut reader.take(size.saturating_add(1)), &mut writer)?;
                    if copied != size || copied > remaining_bytes {
                        return Err(forbidden("Source changed or copy exceeded byte limit"));
                    }
                    remaining_bytes -= copied;
                } else {
                    return Err(forbidden("Cannot copy special filesystem entries"));
                }
            }
            Ok(())
        })();
        if result.is_err() && created_destination {
            // Clean only the newly created destination, never source.
            if let Ok(meta) = fs::symlink_metadata(destination) {
                if meta.file_type().is_dir() {
                    let _ = fs::remove_dir_all(destination);
                } else if meta.file_type().is_file() {
                    let _ = fs::remove_file(destination);
                }
            }
        }
        result
    }

    /// Permanent deletion. The caller must obtain distinct, visible user
    /// confirmation first. By default only files and empty folders are allowed.
    /// For explicit recursive deletion, perform a bounded, symlink-free audit.
    pub fn delete(&self, original: &Path, recursive: bool) -> io::Result<()> {
        let path = self.existing(original)?;
        let file_type = fs::symlink_metadata(&path)?.file_type();
        if file_type.is_file() {
            fs::remove_file(path)
        } else if file_type.is_dir() {
            if recursive {
                let mut remaining = MAX_RECURSIVE_ENTRIES;
                self.audit_tree(&path, &mut remaining)?;
                fs::remove_dir_all(path)
            } else {
                fs::remove_dir(path)
            }
        } else {
            Err(invalid("Unsupported filesystem entry"))
        }
    }

    /// Bounded, iterative audit run before recursive deletion. Every entry is
    /// revalidated through `existing`, so symlinks, junctions, path escapes,
    /// and protected `.git`/`.worktrees` components refuse the whole delete.
    /// An explicit stack keeps arbitrarily deep trees from overflowing the
    /// call stack; `remaining` still caps the total entry count.
    fn audit_tree(&self, folder: &Path, remaining: &mut usize) -> io::Result<()> {
        let mut pending = vec![folder.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for child in fs::read_dir(&dir)? {
                let child = child?;
                if *remaining == 0 {
                    return Err(forbidden("Folder is too large for safe recursive deletion"));
                }
                *remaining -= 1;
                let path = self.existing(&child.path())?;
                let file_type = fs::symlink_metadata(&path)?.file_type();
                if file_type.is_dir() {
                    pending.push(path);
                } else if !file_type.is_file() {
                    return Err(forbidden("Special filesystem entry refused"));
                }
            }
        }
        Ok(())
    }
}

fn check_name(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.trim() != name
        || name.ends_with('.')
        || name.len() > MAX_NAME_BYTES
        || name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
        || name.chars().any(char::is_control)
        || matches!(name.to_ascii_lowercase().as_str(), ".git" | ".worktrees")
    {
        return Err(forbidden("Invalid or protected filename"));
    }
    #[cfg(windows)]
    {
        if name
            .chars()
            .any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Err(forbidden("Invalid Windows filename"));
        }
        let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
        if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ["COM", "LPT"].iter().any(|prefix| {
                base.strip_prefix(prefix).is_some_and(|suffix| {
                    suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
                })
            })
        {
            return Err(forbidden("Reserved Windows filename"));
        }
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, message)
}
/// Local Explorer clipboard; never trusts OS clipboard text as a path to
/// overwrite and never moves a file across different Git worktrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerClipboardMode {
    Copy,
    Cut,
}

#[derive(Debug, Clone)]
pub struct ExplorerClipboard {
    workspace_root: PathBuf,
    selected: PathBuf,
    mode: ExplorerClipboardMode,
}

impl ExplorerClipboard {
    pub fn capture(
        files: &ExplorerFiles,
        selected: &Path,
        mode: ExplorerClipboardMode,
    ) -> io::Result<Self> {
        let selected = files.existing(selected)?;
        if selected
            .components()
            .any(|part| matches!(part, Component::Normal(name) if name == ".git" || name == ".worktrees"))
        {
            return Err(forbidden("Git metadata may not enter the Explorer clipboard"));
        }
        Ok(Self {
            workspace_root: files.root().to_path_buf(),
            selected,
            mode,
        })
    }

    pub fn source(&self) -> &Path {
        &self.selected
    }

    pub fn mode(&self) -> ExplorerClipboardMode {
        self.mode
    }

    /// Successful Cut clears clipboard; failure keeps source and clipboard.
    pub fn paste_into(
        &mut self,
        files: &ExplorerFiles,
        destination_folder: &Path,
    ) -> io::Result<PathBuf> {
        if files.root() != self.workspace_root {
            return Err(forbidden(
                "Paste across workspaces requires explicit import",
            ));
        }
        let result = match self.mode {
            ExplorerClipboardMode::Copy => files.copy_into(&self.selected, destination_folder),
            ExplorerClipboardMode::Cut => files.move_into(&self.selected, destination_folder),
        }?;
        if self.mode == ExplorerClipboardMode::Cut {
            self.selected.clear();
        }
        Ok(result)
    }

    pub fn consumed(&self) -> bool {
        self.selected.as_os_str().is_empty()
    }
}

/// Clipboard for several selected items. All source paths are validated
/// before capture, and a paste preflights every destination before writing
/// the first byte. A directory selection subsumes nested selected children.
/// This remains workspace-local; it never reads arbitrary OS clipboard paths.
#[derive(Debug, Clone)]
pub struct ExplorerClipboardBatch {
    workspace_root: PathBuf,
    entries: Vec<ExplorerClipboard>,
}

impl ExplorerClipboardBatch {
    pub fn capture(
        files: &ExplorerFiles,
        paths: &[PathBuf],
        mode: ExplorerClipboardMode,
    ) -> io::Result<Self> {
        if paths.is_empty() || paths.len() > 24 {
            return Err(invalid("Select between 1 and 24 Explorer items"));
        }
        let mut canonical = paths
            .iter()
            .map(|path| files.existing(path))
            .collect::<io::Result<Vec<_>>>()?;
        canonical.sort();
        canonical.dedup();
        let top_level: Vec<_> = canonical
            .iter()
            .filter(|path| {
                !canonical.iter().any(|parent| {
                    parent.as_path() != path.as_path() && path.starts_with(parent.as_path())
                })
            })
            .collect();
        let entries = top_level
            .into_iter()
            .map(|path| ExplorerClipboard::capture(files, path, mode))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            workspace_root: files.root().to_path_buf(),
            entries,
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn consumed(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn paste_into(
        &mut self,
        files: &ExplorerFiles,
        destination_folder: &Path,
    ) -> io::Result<Vec<PathBuf>> {
        if files.root() != self.workspace_root {
            return Err(forbidden("Cannot paste a batch across workspaces"));
        }
        let destination_folder = files.folder(destination_folder)?;
        let mut destinations = HashSet::new();
        // Do not begin mutating until all predictable errors are rejected,
        // including conflicting basenames from two source directories.
        for clipboard in &self.entries {
            let source = files.existing(clipboard.source())?;
            if destination_folder == source || destination_folder.starts_with(&source) {
                return Err(forbidden("Cannot paste a folder into itself"));
            }
            let name = source
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| invalid("Source has no valid name"))?;
            let target = files.destination(&destination_folder, name)?;
            if !destinations.insert(target) {
                return Err(invalid("Selected files have the same destination filename"));
            }
        }
        let mut completed = Vec::new();
        let total = self.entries.len();
        for index in 0..total {
            match self.entries[index].paste_into(files, &destination_folder) {
                Ok(path) => completed.push(path),
                Err(error) => {
                    self.entries.retain(|clip| !clip.consumed());
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "Batch paste stopped after {} of {} item(s): {error}",
                            completed.len(),
                            total
                        ),
                    ));
                }
            }
        }
        self.entries.retain(|clip| !clip.consumed());
        Ok(completed)
    }
}

fn forbidden(message: &str) -> io::Error {
    io::Error::new(ErrorKind::PermissionDenied, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_clipboard_preflights_collision_and_preserves_sources() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let a = root.join("a");
        let b = root.join("b");
        let target = root.join("target");
        for folder in [&a, &b, &target] {
            fs::create_dir(folder).unwrap();
        }
        fs::write(a.join("same.txt"), "A").unwrap();
        fs::write(b.join("same.txt"), "B").unwrap();
        let files = ExplorerFiles::new(&root).unwrap();
        let mut batch = ExplorerClipboardBatch::capture(
            &files,
            &[a.join("same.txt"), b.join("same.txt")],
            ExplorerClipboardMode::Cut,
        )
        .unwrap();
        assert!(batch.paste_into(&files, &target).is_err());
        assert!(!target.join("same.txt").exists());
        assert!(a.join("same.txt").exists());
        assert!(b.join("same.txt").exists());
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn batch_copy_then_cut_remains_scoped_and_does_not_duplicate_nested_parent() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let files = ExplorerFiles::new(&root).unwrap();
        fs::create_dir(root.join("from")).unwrap();
        fs::create_dir(root.join("into")).unwrap();
        fs::write(root.join("from/a.txt"), "A").unwrap();
        fs::write(root.join("from/b.txt"), "B").unwrap();
        let mut copy = ExplorerClipboardBatch::capture(
            &files,
            &[root.join("from/a.txt"), root.join("from/b.txt")],
            ExplorerClipboardMode::Copy,
        )
        .unwrap();
        assert_eq!(
            copy.paste_into(&files, &root.join("into")).unwrap().len(),
            2
        );
        assert!(!copy.consumed());
        assert_eq!(fs::read_to_string(root.join("into/a.txt")).unwrap(), "A");
        let mut cut = ExplorerClipboardBatch::capture(
            &files,
            &[root.join("from"), root.join("from/a.txt")],
            ExplorerClipboardMode::Cut,
        )
        .unwrap();
        assert_eq!(cut.len(), 1);
        assert_eq!(cut.paste_into(&files, &root.join("into")).unwrap().len(), 1);
        assert!(cut.consumed());
        assert!(!root.join("from").exists());
        assert_eq!(
            fs::read_to_string(root.join("into/from/a.txt")).unwrap(),
            "A"
        );
        let other = tempfile::tempdir().unwrap();
        let other_files = ExplorerFiles::new(other.path()).unwrap();
        assert!(copy.paste_into(&other_files, other.path()).is_err());
    }

    #[test]
    fn external_drop_imports_bounded_file_and_directory_without_overwrite() {
        let workspace = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(workspace.path()).unwrap();
        let files = ExplorerFiles::new(&root).unwrap();
        let source = external.path().join("outside.txt");
        fs::write(&source, "external payload").unwrap();
        let imported = files.import_external(&source, &root).unwrap();
        assert_eq!(fs::read_to_string(&imported).unwrap(), "external payload");
        assert!(
            files.import_external(&source, &root).is_err(),
            "no overwrite"
        );
        let tree = external.path().join("folder");
        fs::create_dir(&tree).unwrap();
        fs::write(tree.join("nested.txt"), "nested").unwrap();
        let imported_tree = files.import_external(&tree, &root).unwrap();
        assert_eq!(
            fs::read_to_string(imported_tree.join("nested.txt")).unwrap(),
            "nested"
        );
        assert_eq!(
            fs::read_to_string(tree.join("nested.txt")).unwrap(),
            "nested"
        );
    }

    #[test]
    fn external_drop_rejects_git_metadata_and_excessive_content() {
        let workspace = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(workspace.path()).unwrap();
        let files = ExplorerFiles::new(&root).unwrap();
        let git_dir = external.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        assert!(files.import_external(&git_dir, &root).is_err());
        let large = external.path().join("huge.bin");
        let writer = fs::File::create(&large).unwrap();
        writer.set_len(32 * 1024 * 1024 + 1).unwrap();
        assert!(files.import_external(&large, &root).is_err());
        assert!(!root.join("huge.bin").exists());
    }

    #[test]
    fn clipboard_copy_then_cut_paste_works_without_overwriting_other_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let files = ExplorerFiles::new(&root).unwrap();
        let one = files.create_folder(&root, "one").unwrap();
        let two = files.create_folder(&root, "two").unwrap();
        let source = files.create_file(&one, "item.txt").unwrap();
        fs::write(&source, "original").unwrap();
        let mut copy =
            ExplorerClipboard::capture(&files, &source, ExplorerClipboardMode::Copy).unwrap();
        assert_eq!(copy.paste_into(&files, &two).unwrap(), two.join("item.txt"));
        assert_eq!(fs::read_to_string(&source).unwrap(), "original");
        assert!(
            copy.paste_into(&files, &two).is_err(),
            "must never overwrite target"
        );
        let mut cut =
            ExplorerClipboard::capture(&files, &source, ExplorerClipboardMode::Cut).unwrap();
        assert!(
            cut.paste_into(&files, &two).is_err(),
            "collision must preserve source"
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), "original");
        fs::remove_file(two.join("item.txt")).unwrap();
        assert_eq!(cut.paste_into(&files, &two).unwrap(), two.join("item.txt"));
        assert!(!source.exists());
        assert!(cut.consumed());
    }

    #[test]
    fn clipboard_refuses_cross_workspace_and_git_metadata() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let first_root = dunce::canonicalize(first.path()).unwrap();
        let second_root = dunce::canonicalize(second.path()).unwrap();
        let files = ExplorerFiles::new(&first_root).unwrap();
        let different = ExplorerFiles::new(&second_root).unwrap();
        let source = files.create_file(&first_root, "safe.txt").unwrap();
        fs::write(&source, "keep").unwrap();
        let mut copy =
            ExplorerClipboard::capture(&files, &source, ExplorerClipboardMode::Copy).unwrap();
        assert_eq!(
            copy.paste_into(&different, &second_root)
                .unwrap_err()
                .kind(),
            ErrorKind::PermissionDenied
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), "keep");
        assert!(
            ExplorerClipboard::capture(&files, &first_root, ExplorerClipboardMode::Copy).is_err()
        );
    }

    #[test]
    fn create_rename_move_delete_without_touching_unrelated_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        let dest = ops.create_folder(&root, "destination").unwrap();
        let created = ops.create_file(&root, "note.txt").unwrap();
        fs::write(&created, "important").unwrap();
        assert!(ops.create_file(&root, "note.txt").is_err());
        let renamed = ops.rename(&created, "renamed.txt").unwrap();
        assert_eq!(fs::read_to_string(&renamed).unwrap(), "important");
        let moved = ops.move_into(&renamed, &dest).unwrap();
        assert_eq!(fs::read_to_string(&moved).unwrap(), "important");
        ops.delete(&moved, false).unwrap();
        ops.delete(&dest, false).unwrap();
        assert!(!dest.exists());
    }

    #[test]
    fn refuses_git_control_paths_escape_overwrite_and_root_delete() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        for name in [".git", ".worktrees", "..", "../outside", "x/y", "x\\y", ""] {
            assert!(ops.create_file(&root, name).is_err());
        }
        assert!(ops.delete(&root, true).is_err());
        let file = ops.create_file(&root, "item").unwrap();
        assert!(ops.rename(&file, "item").is_err());
        let external = tempfile::tempdir().unwrap();
        assert!(ops.create_file(external.path(), "outside").is_err());
        assert!(ops.move_into(&file, external.path()).is_err());
    }

    #[test]
    fn recursive_delete_requires_explicit_opt_in_and_refuses_protected_children() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        let folder = ops.create_folder(&root, "folder").unwrap();
        ops.create_file(&folder, "child").unwrap();
        assert!(ops.delete(&folder, false).is_err());
        ops.delete(&folder, true).unwrap();
        let keep = ops.create_folder(&root, "protected").unwrap();
        fs::create_dir(keep.join(".git")).unwrap();
        assert!(ops.delete(&keep, true).is_err());
        assert!(keep.join(".git").exists());
    }

    #[test]
    fn rejects_folder_cycle_and_duplicate_destination() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        let a = ops.create_folder(&root, "a").unwrap();
        let nested = ops.create_folder(&a, "nested").unwrap();
        assert!(ops.move_into(&a, &nested).is_err());
        ops.create_folder(&root, "existing").unwrap();
        assert!(ops.rename(&a, "existing").is_err());
    }

    #[test]
    fn rejects_trailing_dots_whitespace_protected_and_escape_names() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        for name in [
            ".",
            "..",
            "...",
            "foo.",
            "foo..",
            "name ",
            " name",
            ".git",
            ".Git",
            ".worktrees",
            ".WORKTREES",
        ] {
            assert!(ops.create_file(&root, name).is_err(), "file {name:?}");
            assert!(ops.create_folder(&root, name).is_err(), "folder {name:?}");
        }
        #[cfg(windows)]
        for name in [
            "CON",
            "con.txt",
            "aux.",
            "lpt3.dat",
            "COM1",
            "com9.ini",
            "file?.txt",
            "a|b",
        ] {
            assert!(ops.create_file(&root, name).is_err(), "file {name:?}");
            assert!(ops.create_folder(&root, name).is_err(), "folder {name:?}");
        }
        // Parent paths must not escape through `..` or reach protected dirs.
        assert!(ops
            .create_file(&root.join("a").join("..").join("b"), "x")
            .is_err());
        assert!(ops.create_file(&root.join(".git"), "config").is_err());
        let parent = ops.create_folder(&root, "parent").unwrap();
        assert!(ops.create_file(&parent, ".git").is_err());
        assert!(ops.create_folder(&parent, ".worktrees").is_err());
    }

    #[test]
    fn recursive_delete_is_iterative_and_refuses_nested_protected_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        // A deep chain must not overflow the call stack during the audit.
        let mut dir = root.clone();
        for _ in 0..80 {
            dir = dir.join("d");
            fs::create_dir(&dir).unwrap();
        }
        let leaf = ops.create_file(&dir, "leaf").unwrap();
        assert!(leaf.starts_with(&root));
        ops.delete(&root.join("d"), true).unwrap();
        assert!(!root.join("d").exists());
        // A `.git` dir nested arbitrarily deep still blocks recursive delete.
        let tree = ops.create_folder(&root, "tree").unwrap();
        let deep = tree.join("a").join("b").join("c");
        fs::create_dir_all(&deep).unwrap();
        fs::create_dir(deep.join(".git")).unwrap();
        assert!(ops.delete(&tree, true).is_err());
        assert!(deep.join(".git").exists());
        // The same protection applies to `.worktrees`.
        fs::remove_dir_all(deep.join(".git")).unwrap();
        fs::create_dir(deep.join(".worktrees")).unwrap();
        assert!(ops.delete(&tree, true).is_err());
        assert!(deep.join(".worktrees").exists());
    }

    #[test]
    fn refuses_symlinked_entries_when_supported() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let folder = ops.create_folder(&root, "folder").unwrap();
        let inner = folder.join("inner");
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(outside.path(), &inner).is_ok();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(outside.path(), &inner).is_ok();
        #[cfg(not(any(unix, windows)))]
        let made = false;
        if !made {
            return;
        }
        // Paths through the link and destinations that are links are refused.
        assert!(ops.create_file(&inner, "escaped.txt").is_err());
        assert!(ops.move_into(&folder, &inner).is_err());
        assert!(ops.delete(&inner, true).is_err());
        // Recursive deletion audits every child, so the link blocks the tree.
        assert!(ops.delete(&folder, true).is_err());
        assert!(fs::symlink_metadata(&inner).is_ok());
        assert!(folder.is_dir());
    }

    #[test]
    fn copy_and_duplicate_preserve_original_without_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        let original = ops.create_file(&root, "note.txt").unwrap();
        fs::write(&original, "original").unwrap();
        let target_dir = ops.create_folder(&root, "target").unwrap();
        let copied = ops.copy_into(&original, &target_dir).unwrap();
        assert_eq!(fs::read_to_string(&copied).unwrap(), "original");
        assert!(ops.copy_into(&original, &target_dir).is_err());
        let duplicated = ops.duplicate(&original).unwrap();
        assert_eq!(duplicated.file_name().unwrap(), "note (copy).txt");
        let second = ops.duplicate(&original).unwrap();
        assert_eq!(second.file_name().unwrap(), "note (copy 2).txt");
        assert_eq!(fs::read_to_string(&original).unwrap(), "original");
        assert_eq!(fs::read_to_string(&second).unwrap(), "original");
    }

    #[test]
    fn copy_folder_refuses_descendant_and_protected_git_paths() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        let source = ops.create_folder(&root, "source").unwrap();
        let nested = ops.create_folder(&source, "nested").unwrap();
        fs::write(nested.join("note"), "contents").unwrap();
        let target_dir = ops.create_folder(&root, "target").unwrap();
        assert!(ops.copy_into(&source, &nested).is_err());
        let copy = ops.copy_into(&source, &target_dir).unwrap();
        assert_eq!(
            fs::read_to_string(copy.join("nested").join("note")).unwrap(),
            "contents"
        );
        fs::create_dir(source.join(".git")).unwrap();
        let duplicated = root.join("source (copy)");
        assert!(ops.duplicate(&source).is_err());
        assert!(!duplicated.exists(), "failed copy cleans new destination");
        assert!(source.join(".git").is_dir(), "source remains untouched");
    }

    #[test]
    fn oversized_copy_is_refused_before_writing_target() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        let source = ops.create_file(&root, "big.bin").unwrap();
        let file = OpenOptions::new().write(true).open(&source).unwrap();
        file.set_len(MAX_SINGLE_FILE_BYTES + 1).unwrap();
        let target_dir = ops.create_folder(&root, "dest").unwrap();
        assert!(ops.copy_into(&source, &target_dir).is_err());
        assert!(!target_dir.join("big.bin").exists());
        assert_eq!(
            fs::metadata(&source).unwrap().len(),
            MAX_SINGLE_FILE_BYTES + 1
        );
    }

    #[test]
    fn failed_destination_create_does_not_remove_existing_file_or_folder() {
        let temp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(temp.path()).unwrap();
        let ops = ExplorerFiles::new(&root).unwrap();
        let source = ops.create_file(&root, "source.txt").unwrap();
        fs::write(&source, "copy me").unwrap();
        let existing = ops.create_file(&root, "existing.txt").unwrap();
        fs::write(&existing, "keep me").unwrap();
        assert!(ops.copy_to_exact(&source, &existing).is_err());
        assert_eq!(fs::read_to_string(&existing).unwrap(), "keep me");
        assert_eq!(fs::read_to_string(&source).unwrap(), "copy me");

        let source_folder = ops.create_folder(&root, "source-folder").unwrap();
        fs::write(source_folder.join("child.txt"), "source").unwrap();
        let destination_folder = ops.create_folder(&root, "existing-folder").unwrap();
        fs::write(destination_folder.join("preserve.txt"), "keep this").unwrap();
        assert!(ops
            .copy_to_exact(&source_folder, &destination_folder)
            .is_err());
        assert_eq!(
            fs::read_to_string(destination_folder.join("preserve.txt")).unwrap(),
            "keep this"
        );
    }
}
