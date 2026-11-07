//! Explicit, guarded filesystem mutations for the experimental GPUI Explorer.
//! The caller must provide an action and confirmation; merely browsing or
//! indexing must never reach this module. No symlinks or Git metadata writes.
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};

const MAX_NAME_BYTES: usize = 255;
const MAX_RECURSIVE_ENTRIES: usize = 4_096;

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
fn forbidden(message: &str) -> io::Error {
    io::Error::new(ErrorKind::PermissionDenied, message)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
