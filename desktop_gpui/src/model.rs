//! Read-only file explorer model. No GPUI types or UI concerns in this module.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 400;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_PREVIEW_LINES: usize = 600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_directory: bool,
}

#[derive(Debug)]
pub struct WorkbenchModel {
    root: PathBuf,
    directory: PathBuf,
    branch: String,
    entries: Vec<ExplorerEntry>,
    selected_path: Option<PathBuf>,
    selected_content: String,
    status: String,
}

impl WorkbenchModel {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = dunce::canonicalize(root)?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Workspace is not a directory",
            ));
        }
        let branch = alera_core::git::current_branch(&root.to_string_lossy())
            .unwrap_or_else(|_| "No Git repository".to_string());
        let mut model = Self {
            directory: root.clone(),
            root,
            branch,
            entries: Vec::new(),
            selected_path: None,
            selected_content: String::new(),
            status: "Native GPUI preview: file browsing is read-only".into(),
        };
        model.refresh()?;
        Ok(model)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn branch(&self) -> &str {
        &self.branch
    }
    pub fn entries(&self) -> &[ExplorerEntry] {
        &self.entries
    }
    pub fn selected_path(&self) -> Option<&Path> {
        self.selected_path.as_deref()
    }
    pub fn content(&self) -> &str {
        &self.selected_content
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn can_go_up(&self) -> bool {
        self.directory.as_path() != self.root()
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.directory)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().to_string();
            if matches!(
                name.as_str(),
                ".git" | ".worktrees" | "target" | "node_modules" | "build" | "dist"
            ) {
                continue;
            }
            let file_type = item.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            entries.push(ExplorerEntry {
                name,
                path: item.path(),
                is_directory: file_type.is_dir(),
            });
            if entries.len() >= MAX_ENTRIES {
                break;
            }
        }
        entries.sort_by(|a, b| {
            b.is_directory
                .cmp(&a.is_directory)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.entries = entries;
        Ok(())
    }

    pub fn open_entry(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index).cloned() else {
            return;
        };
        // All children are revalidated at click-time, not merely at scan time.
        let path = match dunce::canonicalize(&entry.path) {
            Ok(path) if path.starts_with(&self.root) => path,
            Ok(_) => {
                self.status = "Blocked path outside workspace".into();
                return;
            }
            Err(error) => {
                self.status = format!("Unable to open: {error}");
                return;
            }
        };
        if entry.is_directory {
            if !path.is_dir() {
                self.status = "Folder no longer exists".into();
                return;
            }
            self.directory = path;
            match self.refresh() {
                Ok(()) => self.status = "Folder opened".into(),
                Err(error) => self.status = format!("Cannot browse folder: {error}"),
            }
        } else {
            match read_preview(&path) {
                Ok(content) => {
                    self.selected_content = content;
                    self.selected_path = Some(path);
                    self.status = format!("Opened {} (read-only)", entry.name);
                }
                Err(error) => self.status = format!("Cannot preview {}: {error}", entry.name),
            }
        }
    }

    pub fn go_up(&mut self) {
        if !self.can_go_up() {
            return;
        }
        let Some(parent) = self.directory.parent() else {
            return;
        };
        // Never allow directory traversal outside the selected workspace root.
        if !parent.starts_with(&self.root) {
            return;
        }
        self.directory = parent.to_path_buf();
        if let Err(error) = self.refresh() {
            self.status = format!("Cannot browse folder: {error}");
        }
    }
}

fn read_preview(path: &Path) -> io::Result<String> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Not a regular file",
        ));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "File too large for preview (256 KiB limit)",
        ));
    }
    let content = fs::read_to_string(path)?;
    if content.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Binary files are not supported",
        ));
    }
    let mut preview = content
        .lines()
        .take(MAX_PREVIEW_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if content.lines().count() > MAX_PREVIEW_LINES {
        preview.push_str("\n… preview limited to 600 lines");
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn sorts_directories_before_files_and_skips_generated() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::create_dir(temp.path().join("target")).unwrap();
        fs::write(temp.path().join("main.rs"), "fn main() {}").unwrap();
        let model = WorkbenchModel::new(temp.path()).unwrap();
        assert_eq!(model.entries().len(), 2);
        assert!(model.entries()[0].is_directory);
        assert_eq!(model.entries()[1].name, "main.rs");
    }

    #[test]
    fn can_navigate_and_preview_without_leaving_root() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        let mut source = fs::File::create(temp.path().join("src").join("main.rs")).unwrap();
        writeln!(source, "fn main() {{}}").unwrap();
        let mut model = WorkbenchModel::new(temp.path()).unwrap();
        assert!(!model.can_go_up());
        model.open_entry(0);
        assert!(model.can_go_up());
        model.open_entry(0);
        assert!(model.content().contains("fn main()"));
        model.go_up();
        model.go_up();
        assert_eq!(model.directory(), model.root());
    }

    #[test]
    fn refuses_oversized_previews() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("large.txt");
        fs::write(&file, vec![b'x'; MAX_FILE_BYTES as usize + 1]).unwrap();
        let err = read_preview(&file).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
