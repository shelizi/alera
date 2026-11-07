//! Read-only file explorer model. No GPUI types or UI concerns in this module.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 400;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_PREVIEW_LINES: usize = 600;
const MAX_TABS: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_directory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewTab {
    pub path: PathBuf,
    pub title: String,
    pub content: String,
}

#[derive(Debug)]
pub struct WorkbenchModel {
    root: PathBuf,
    directory: PathBuf,
    branch: String,
    entries: Vec<ExplorerEntry>,
    tabs: Vec<PreviewTab>,
    active_tab: Option<usize>,
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
            tabs: Vec::new(),
            active_tab: None,
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
    pub fn tabs(&self) -> &[PreviewTab] {
        &self.tabs
    }
    pub fn active_tab(&self) -> Option<usize> {
        self.active_tab
    }
    pub fn active_document(&self) -> Option<&PreviewTab> {
        self.active_tab.and_then(|index| self.tabs.get(index))
    }
    pub fn selected_path(&self) -> Option<&Path> {
        self.active_document().map(|tab| tab.path.as_path())
    }
    pub fn content(&self) -> &str {
        self.active_document()
            .map(|tab| tab.content.as_str())
            .unwrap_or("")
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
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

    pub fn refresh_directory(&mut self) {
        self.status = match self.refresh() {
            Ok(()) => "Explorer refreshed".into(),
            Err(error) => format!("Cannot refresh Explorer: {error}"),
        };
    }

    pub fn open_entry(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index).cloned() else {
            return;
        };
        self.open_path(&entry.path);
    }

    /// Open only files and directories contained by the current workspace.
    /// The same validation is used for clicks and persisted session restores.
    pub fn open_path(&mut self, candidate: impl AsRef<Path>) -> bool {
        // All children are revalidated at click-time, not merely at scan time.
        let path = match dunce::canonicalize(candidate) {
            Ok(path) if path.starts_with(&self.root) => path,
            Ok(_) => {
                self.status = "Blocked path outside workspace".into();
                return false;
            }
            Err(error) => {
                self.status = format!("Unable to open: {error}");
                return false;
            }
        };
        if path.is_dir() {
            self.directory = path;
            match self.refresh() {
                Ok(()) => self.status = "Folder opened".into(),
                Err(error) => self.status = format!("Cannot browse folder: {error}"),
            }
            true
        } else if path.is_file() {
            let title = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".into());
            let target = path.clone();
            self.open_file(path, title);
            self.tabs.iter().any(|tab| tab.path == target)
        } else {
            self.status = "Only regular files and folders can be opened".into();
            false
        }
    }

    fn open_file(&mut self, path: PathBuf, title: String) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.path == path) {
            self.activate_tab(index);
            return;
        }
        if self.tabs.len() >= MAX_TABS {
            self.status = format!("Preview limit reached ({MAX_TABS}). Close a tab first");
            return;
        }
        match read_preview(&path) {
            Ok(content) => {
                self.tabs.push(PreviewTab {
                    path,
                    title: title.clone(),
                    content,
                });
                self.active_tab = Some(self.tabs.len() - 1);
                self.status = format!("Opened {title} (read-only)");
            }
            Err(error) => self.status = format!("Cannot preview {title}: {error}"),
        }
    }

    pub fn activate_tab(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get(index) {
            self.active_tab = Some(index);
            self.status = format!("Viewing {} (read-only)", tab.title);
        }
    }

    pub fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let closed = self.tabs.remove(index);
        self.active_tab = match self.active_tab {
            None => None,
            Some(_) if self.tabs.is_empty() => None,
            Some(active) if active == index => Some(index.min(self.tabs.len() - 1)),
            Some(active) if active > index => Some(active - 1),
            Some(active) => Some(active),
        };
        self.status = format!("Closed {}", closed.title);
    }

    /// Drop previews of a renamed, moved or deleted path (and descendants),
    /// retaining unrelated open previews. A real editor must first be checked
    /// for dirty changes by the UI before any filesystem mutation.
    pub fn forget_path_and_children(&mut self, path: &Path) {
        let active_path = self.active_document().map(|tab| tab.path.clone());
        let prior_active_index = self.active_tab;
        self.tabs.retain(|tab| !tab.path.starts_with(path));
        self.active_tab = active_path
            .and_then(|active| self.tabs.iter().position(|tab| tab.path == active))
            .or_else(|| {
                prior_active_index.and_then(|index| {
                    (!self.tabs.is_empty()).then(|| index.min(self.tabs.len() - 1))
                })
            });
        if self.directory.starts_with(path) {
            self.directory = self.root.clone();
        }
        self.refresh_directory();
    }

    pub fn reload_active(&mut self) {
        let Some(index) = self.active_tab else {
            self.status = "No preview to reload".into();
            return;
        };
        let path = self.tabs[index].path.clone();
        // Revalidate containment after file moves or replacements.
        match dunce::canonicalize(&path) {
            Ok(canonical) if canonical.starts_with(&self.root) && canonical == path => {
                match read_preview(&canonical) {
                    Ok(content) => {
                        self.tabs[index].content = content;
                        self.status = format!("Reloaded {}", self.tabs[index].title);
                    }
                    Err(error) => self.status = format!("Cannot reload preview: {error}"),
                }
            }
            Ok(_) => self.status = "Preview no longer belongs to this workspace".into(),
            Err(error) => self.status = format!("Cannot reload preview: {error}"),
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
    fn forgetting_previous_or_active_file_keeps_correct_preview_selected() {
        let temporary = tempfile::tempdir().unwrap();
        for (name, content) in [("a.txt", "A"), ("b.txt", "B"), ("c.txt", "C")] {
            fs::write(temporary.path().join(name), content).unwrap();
        }
        let mut model = WorkbenchModel::new(temporary.path()).unwrap();
        let root = model.root().to_path_buf();
        for name in ["a.txt", "b.txt", "c.txt"] {
            assert!(model.open_path(root.join(name)));
        }
        assert_eq!(model.content(), "C");
        model.forget_path_and_children(&root.join("a.txt"));
        assert_eq!(model.active_tab(), Some(1));
        assert_eq!(model.content(), "C");
        model.forget_path_and_children(&root.join("c.txt"));
        assert_eq!(model.active_tab(), Some(0));
        assert_eq!(model.content(), "B");
        model.forget_path_and_children(&root.join("b.txt"));
        assert_eq!(model.active_tab(), None);
        assert!(model.tabs().is_empty());
    }

    #[test]
    fn forgetting_directory_closes_only_descendant_previews() {
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir(temporary.path().join("sub")).unwrap();
        fs::write(temporary.path().join("outside.txt"), "outside").unwrap();
        fs::write(temporary.path().join("sub").join("inside.txt"), "inside").unwrap();
        let mut model = WorkbenchModel::new(temporary.path()).unwrap();
        let root = model.root().to_path_buf();
        assert!(model.open_path(root.join("outside.txt")));
        assert!(model.open_path(root.join("sub").join("inside.txt")));
        model.forget_path_and_children(&root.join("sub"));
        assert_eq!(model.tabs().len(), 1);
        assert_eq!(model.content(), "outside");
        assert_eq!(model.directory(), root.as_path());
    }

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

    #[test]
    fn tab_activation_closing_and_reopen_have_stable_indices() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("a.txt"), "A").unwrap();
        fs::write(temp.path().join("b.txt"), "B").unwrap();
        fs::write(temp.path().join("c.txt"), "C").unwrap();
        let mut model = WorkbenchModel::new(temp.path()).unwrap();
        model.open_entry(0);
        model.open_entry(1);
        model.open_entry(2);
        assert_eq!(model.tabs().len(), 3);
        assert_eq!(model.active_tab(), Some(2));
        model.open_entry(1);
        assert_eq!(model.tabs().len(), 3, "already-open file reuses its tab");
        assert_eq!(model.active_tab(), Some(1));
        model.close_tab(0);
        assert_eq!(model.active_tab(), Some(0));
        assert_eq!(model.content(), "B");
        model.close_tab(0);
        assert_eq!(model.active_tab(), Some(0));
        assert_eq!(model.content(), "C");
        model.close_tab(0);
        assert_eq!(model.active_tab(), None);
        assert_eq!(model.content(), "");
    }

    #[test]
    fn reload_updates_preview_without_discarding_last_good_content_on_failure() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("note.txt");
        fs::write(&file, "before").unwrap();
        let mut model = WorkbenchModel::new(temp.path()).unwrap();
        model.open_entry(0);
        fs::write(&file, "after").unwrap();
        model.reload_active();
        assert_eq!(model.content(), "after");
        fs::write(&file, vec![b'x'; MAX_FILE_BYTES as usize + 1]).unwrap();
        model.reload_active();
        assert_eq!(model.content(), "after", "keep last good preview");
        assert!(model.status().contains("Cannot reload"));
    }

    #[test]
    fn caps_open_tabs_and_rejects_out_of_range_controls() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..MAX_TABS + 1 {
            fs::write(temp.path().join(format!("{index:02}.txt")), "preview").unwrap();
        }
        let mut model = WorkbenchModel::new(temp.path()).unwrap();
        for index in 0..MAX_TABS + 1 {
            model.open_entry(index);
        }
        assert_eq!(model.tabs().len(), MAX_TABS);
        assert!(model.status().contains("Preview limit reached"));
        model.close_tab(MAX_TABS);
        model.activate_tab(MAX_TABS);
        assert_eq!(model.tabs().len(), MAX_TABS);
    }
}
