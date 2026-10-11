use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptions, OpenOptionsFollowExt};
use cap_std::{ambient_authority, fs::Dir};
use same_file::Handle;
use sha2::{Digest, Sha256};
use uuid::Uuid;

mod mime;
mod prompts;
mod quick_open;

use mime::{mime_type_for_path, path_has_binary_preview_mime};

pub use prompts::{list_codex_saved_prompts, CodexSavedPrompt, CodexSavedPromptScope};
pub use quick_open::{
    search_workspace_quick_open_session, start_workspace_quick_open_session,
    start_workspace_quick_open_session_without_symlinks, stop_workspace_quick_open_session,
    WorkspaceQuickOpenMatch, WorkspaceQuickOpenSession, DEFAULT_QUICK_OPEN_EXCLUDED_DIRECTORIES,
};

pub const MAX_REMOTE_READ_BYTES: u64 = 256 * 1024;
pub const MAX_REMOTE_EDIT_BYTES: usize = 256 * 1024;
pub const MAX_REMOTE_DIRECTORY_PAGE_ENTRIES: usize = 200;
const MAX_REMOTE_DIRECTORY_SCAN_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDirectoryEntry {
    pub name: String,
    pub relative_path: String,
    pub is_directory: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDirectoryPage {
    pub entries: Vec<WorkspaceDirectoryEntry>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFileErrorKind {
    InvalidPath,
    OutsideWorkspace,
    NotFound,
    Unsupported,
    Conflict,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTextWriteResult {
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug)]
pub struct WorkspaceFileError {
    pub kind: WorkspaceFileErrorKind,
    pub context: String,
}

impl std::fmt::Display for WorkspaceFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.context)
    }
}

impl std::error::Error for WorkspaceFileError {}

impl WorkspaceFileError {
    pub(super) fn new(kind: WorkspaceFileErrorKind, context: impl Into<String>) -> Self {
        Self {
            kind,
            context: context.into(),
        }
    }

    pub(super) fn from_io(error: std::io::Error, context: impl Into<String>) -> Self {
        let kind = if error.kind() == std::io::ErrorKind::NotFound {
            WorkspaceFileErrorKind::NotFound
        } else {
            WorkspaceFileErrorKind::Io
        };
        Self::new(kind, format!("{}: {error}", context.into()))
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceFileRange {
    pub bytes: Vec<u8>,
    pub offset: u64,
    pub next_offset: u64,
    pub total_bytes: u64,
    pub mime_type: String,
    pub is_text: bool,
}

pub struct WorkspaceFileRoot {
    directory: Dir,
    canonical_path: PathBuf,
}

impl WorkspaceFileRoot {
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }
}

pub fn open_workspace_file_root(
    workspace_path: &str,
) -> Result<WorkspaceFileRoot, WorkspaceFileError> {
    let directory = Dir::open_ambient_dir(workspace_path, ambient_authority())
        .map_err(|error| WorkspaceFileError::from_io(error, workspace_path))?;
    let canonical_path = dunce::canonicalize(workspace_path)
        .map_err(|error| WorkspaceFileError::from_io(error, workspace_path))?;
    // Authorization uses this path, so prove it still names the held directory.
    let opened_handle = Handle::from_file(
        directory
            .try_clone()
            .map_err(|error| WorkspaceFileError::from_io(error, workspace_path))?
            .into_std_file(),
    )
    .map_err(|error| WorkspaceFileError::from_io(error, workspace_path))?;
    let canonical_handle = Handle::from_path(&canonical_path)
        .map_err(|error| WorkspaceFileError::from_io(error, workspace_path))?;
    if opened_handle != canonical_handle {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            format!("Workspace root changed while it was being opened: {workspace_path}"),
        ));
    }
    Ok(WorkspaceFileRoot {
        directory,
        canonical_path,
    })
}

pub fn read_workspace_file_range(
    workspace_path: &str,
    relative_path: &str,
    offset: u64,
    length: u64,
) -> Result<WorkspaceFileRange, WorkspaceFileError> {
    let root = open_workspace_file_root(workspace_path)?;
    read_workspace_file_range_from_root(&root, relative_path, offset, length)
}

pub fn read_workspace_file_range_from_root(
    root: &WorkspaceFileRoot,
    relative_path: &str,
    offset: u64,
    length: u64,
) -> Result<WorkspaceFileRange, WorkspaceFileError> {
    if length == 0 || length > MAX_REMOTE_READ_BYTES {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            format!("Read length must be between 1 and {MAX_REMOTE_READ_BYTES} bytes"),
        ));
    }
    let (mut file, normalized_relative) =
        open_workspace_file_without_symlinks(root, relative_path)?;
    let metadata = file
        .metadata()
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    if !metadata.is_file() {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Unsupported,
            format!("Workspace path is not a file: {relative_path}"),
        ));
    }
    if offset > metadata.len() {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            format!("Read offset exceeds file length: {relative_path}"),
        ));
    }
    let count = length.min(metadata.len().saturating_sub(offset));
    let is_text = file_is_probably_utf8(
        &mut file,
        &normalized_relative,
        metadata.len(),
        relative_path,
    )?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    let mut bytes = vec![0_u8; usize::try_from(count).unwrap_or(0)];
    file.read_exact(&mut bytes)
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    let mime_type = mime_type_for_path(&normalized_relative, is_text).to_string();
    Ok(WorkspaceFileRange {
        next_offset: offset.saturating_add(count),
        bytes,
        offset,
        total_bytes: metadata.len(),
        mime_type,
        is_text,
    })
}

/// List one bounded page from a workspace directory without following any
/// symlink on the way to the target. Protected VCS metadata is never exposed.
pub fn list_workspace_directory_from_root(
    root: &WorkspaceFileRoot,
    relative_directory: &str,
    offset: usize,
    limit: usize,
    include_hidden: bool,
) -> Result<WorkspaceDirectoryPage, WorkspaceFileError> {
    if limit == 0 || limit > MAX_REMOTE_DIRECTORY_PAGE_ENTRIES {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            "Invalid directory page size",
        ));
    }
    let relative = Path::new(relative_directory);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || is_protected_workspace_path(relative)
    {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            "Invalid workspace directory path",
        ));
    }
    let mut directory = root
        .directory
        .try_clone()
        .map_err(|err| WorkspaceFileError::from_io(err, relative_directory))?;
    for component in relative.components() {
        if let Component::Normal(part) = component {
            reject_workspace_symlink(&directory, part, relative_directory)?;
            directory = directory
                .open_dir_nofollow(part)
                .map_err(|err| WorkspaceFileError::from_io(err, relative_directory))?;
        }
    }
    let mut entries = Vec::new();
    for result in directory
        .read_dir(".")
        .map_err(|err| WorkspaceFileError::from_io(err, relative_directory))?
    {
        let entry = result.map_err(|err| WorkspaceFileError::from_io(err, relative_directory))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "." || name == ".." || (!include_hidden && name.starts_with('.')) {
            continue;
        }
        let child = relative.join(&name);
        if is_protected_workspace_path(&child) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|err| WorkspaceFileError::from_io(err, &name))?;
        if file_type.is_symlink() || (!file_type.is_dir() && !file_type.is_file()) {
            continue;
        }
        let size_bytes = if file_type.is_file() {
            entry
                .metadata()
                .map_err(|err| WorkspaceFileError::from_io(err, &name))?
                .len()
        } else {
            0
        };
        entries.push(WorkspaceDirectoryEntry {
            name,
            relative_path: child.to_string_lossy().replace('\\', "/"),
            is_directory: file_type.is_dir(),
            size_bytes,
        });
        if entries.len() > MAX_REMOTE_DIRECTORY_SCAN_ENTRIES {
            return Err(WorkspaceFileError::new(
                WorkspaceFileErrorKind::Unsupported,
                "Directory has too many entries to browse on mobile",
            ));
        }
    }
    entries.sort_by(|a, b| {
        b.is_directory
            .cmp(&a.is_directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    if offset > entries.len() {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            "Directory page offset exceeds entry count",
        ));
    }
    let has_more = entries.len().saturating_sub(offset) > limit;
    Ok(WorkspaceDirectoryPage {
        entries: entries.into_iter().skip(offset).take(limit).collect(),
        has_more,
    })
}

/// Replace an existing small UTF-8 file only when its content still matches
/// the exact bytes fetched by the phone. Write a sibling temporary file then
/// rename it through the already opened, no-follow parent directory handle.
/// This never creates an arbitrary new target path or follows a symlink.
pub fn write_workspace_text_from_root(
    root: &WorkspaceFileRoot,
    relative_path: &str,
    expected_sha256: &str,
    new_text: &str,
) -> Result<WorkspaceTextWriteResult, WorkspaceFileError> {
    if new_text.len() > MAX_REMOTE_EDIT_BYTES {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Unsupported,
            "Mobile edits are limited to 256 KiB",
        ));
    }
    if expected_sha256.len() != 64 || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            "Expected file checksum must be a SHA-256 hex digest",
        ));
    }
    let path = Path::new(relative_path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || is_protected_workspace_path(path)
    {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            "Invalid workspace edit path",
        ));
    }
    let (mut source, normalized) = open_workspace_file_without_symlinks(root, relative_path)?;
    let metadata = source
        .metadata()
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    if !metadata.is_file() || metadata.len() > MAX_REMOTE_EDIT_BYTES as u64 {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Unsupported,
            "Only existing UTF-8 text files up to 256 KiB can be edited",
        ));
    }
    if metadata.permissions().readonly() {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Unsupported,
            "Workspace file is read-only",
        ));
    }
    if !file_is_probably_utf8(&mut source, &normalized, metadata.len(), relative_path)? {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Unsupported,
            "Binary files cannot be edited on mobile",
        ));
    }
    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    let mut old_bytes = Vec::new();
    source
        .read_to_end(&mut old_bytes)
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    if old_bytes.contains(&0) || std::str::from_utf8(&old_bytes).is_err() {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Unsupported,
            "Binary or invalid UTF-8 content cannot be edited",
        ));
    }
    let old_hash = hex::encode(Sha256::digest(&old_bytes));
    if !old_hash.eq_ignore_ascii_case(expected_sha256) {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Conflict,
            "Workspace file changed since it was opened; reload before saving",
        ));
    }
    drop(source);

    let mut parent = root.directory.try_clone().map_err(|error| {
        WorkspaceFileError::from_io(error, root.canonical_path.display().to_string())
    })?;
    let components = path.components().collect::<Vec<_>>();
    for component in &components[..components.len() - 1] {
        if let Component::Normal(name) = component {
            reject_workspace_symlink(&parent, name, relative_path)?;
            parent = parent
                .open_dir_nofollow(name)
                .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
        }
    }
    let Component::Normal(file_name) = components[components.len() - 1] else {
        unreachable!("workspace edit path validated above")
    };
    let temp_name = format!(".alera-mobile-edit-{}.tmp", Uuid::new_v4());
    let result = (|| {
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        let mut temporary = parent
            .open_with(&temp_name, &options)
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?
            .into_std();
        temporary
            .set_permissions(metadata.permissions())
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
        temporary
            .write_all(new_text.as_bytes())
            .and_then(|_| temporary.sync_all())
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
        drop(temporary);
        // Detect a second editor or agent modifying the file while we were
        // preparing the replacement. Never merge blindly or overwrite it.
        let (mut latest, _) = open_workspace_file_without_symlinks(root, relative_path)?;
        let mut latest_bytes = Vec::new();
        latest
            .read_to_end(&mut latest_bytes)
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
        if hex::encode(Sha256::digest(&latest_bytes)) != old_hash {
            return Err(WorkspaceFileError::new(
                WorkspaceFileErrorKind::Conflict,
                "Workspace file changed during save; reload before retrying",
            ));
        }
        drop(latest);
        parent
            .rename(&temp_name, &parent, file_name)
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
        Ok(WorkspaceTextWriteResult {
            sha256: hex::encode(Sha256::digest(new_text.as_bytes())),
            size_bytes: new_text.len() as u64,
        })
    })();
    let _ = parent.remove_file(&temp_name);
    result
}

/// Create a new UTF-8 file inside an already-existing, no-follow workspace
/// directory. `create_new` atomically refuses an existing file or symlink;
/// mobile clients never get a replace/overwrite variant of this operation.
pub fn create_workspace_text_from_root(
    root: &WorkspaceFileRoot,
    relative_path: &str,
    text: &str,
) -> Result<WorkspaceTextWriteResult, WorkspaceFileError> {
    if text.len() > MAX_REMOTE_EDIT_BYTES || text.contains('\0') {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Unsupported,
            "Mobile file creation requires UTF-8 text up to 256 KiB, without NUL bytes",
        ));
    }
    let path = Path::new(relative_path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || is_protected_workspace_path(path)
    {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            "Invalid workspace file creation path",
        ));
    }
    let components = path.components().collect::<Vec<_>>();
    let mut parent = root.directory.try_clone().map_err(|error| {
        WorkspaceFileError::from_io(error, root.canonical_path.display().to_string())
    })?;
    for component in &components[..components.len() - 1] {
        if let Component::Normal(name) = component {
            reject_workspace_symlink(&parent, name, relative_path)?;
            parent = parent
                .open_dir_nofollow(name)
                .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
        }
    }
    let Component::Normal(name) = components[components.len() - 1] else {
        unreachable!("workspace creation path validated above")
    };
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    let mut file = parent.open_with(name, &options).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            WorkspaceFileError::new(
                WorkspaceFileErrorKind::Conflict,
                "A file with this name already exists; choose another name",
            )
        } else {
            WorkspaceFileError::from_io(error, relative_path)
        }
    })?;
    let result = file
        .write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path));
    drop(file);
    if let Err(error) = result {
        let _ = parent.remove_file(name);
        return Err(error);
    }
    Ok(WorkspaceTextWriteResult {
        sha256: hex::encode(Sha256::digest(text.as_bytes())),
        size_bytes: text.len() as u64,
    })
}

fn open_workspace_file_without_symlinks(
    root: &WorkspaceFileRoot,
    relative_path: &str,
) -> Result<(fs::File, PathBuf), WorkspaceFileError> {
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        || is_protected_workspace_path(relative)
    {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            relative_path,
        ));
    }
    let components = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_os_string()),
            Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some((file_name, directory_components)) = components.split_last() else {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            relative_path,
        ));
    };
    let mut normalized_relative = PathBuf::new();
    for component in &components {
        normalized_relative.push(component);
    }
    let mut directory = root.directory.try_clone().map_err(|error| {
        WorkspaceFileError::from_io(error, root.canonical_path.display().to_string())
    })?;
    for component in directory_components {
        reject_workspace_symlink(&directory, component, relative_path)?;
        directory = directory
            .open_dir_nofollow(component)
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    }
    reject_workspace_symlink(&directory, file_name, relative_path)?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = directory
        .open_with(file_name, &options)
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?
        .into_std();
    Ok((file, normalized_relative))
}

fn reject_workspace_symlink(
    directory: &Dir,
    component: &std::ffi::OsStr,
    context: &str,
) -> Result<(), WorkspaceFileError> {
    let metadata = directory
        .symlink_metadata(component)
        .map_err(|error| WorkspaceFileError::from_io(error, context))?;
    if metadata.file_type().is_symlink() {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            context,
        ));
    }
    Ok(())
}

fn file_is_probably_utf8(
    file: &mut fs::File,
    path: &Path,
    total_bytes: u64,
    context: &str,
) -> Result<bool, WorkspaceFileError> {
    if path_has_binary_preview_mime(path) {
        return Ok(false);
    }
    let sample_bytes = total_bytes.min(MAX_REMOTE_READ_BYTES);
    let mut sample = vec![0_u8; usize::try_from(sample_bytes).unwrap_or(0)];
    file.read_exact(&mut sample)
        .map_err(|error| WorkspaceFileError::from_io(error, context))?;
    Ok(range_is_probably_utf8(&sample, sample_bytes < total_bytes))
}

fn range_is_probably_utf8(bytes: &[u8], allow_incomplete_suffix: bool) -> bool {
    if bytes.contains(&0) {
        return false;
    }
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(error) => allow_incomplete_suffix && error.error_len().is_none(),
    }
}

pub(super) fn workspace_root(value: &str) -> Result<PathBuf, WorkspaceFileError> {
    let path = PathBuf::from(value);
    let root =
        dunce::canonicalize(&path).map_err(|error| WorkspaceFileError::from_io(error, value))?;
    if !root.is_dir() {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::InvalidPath,
            format!("Workspace root is not a directory: {value}"),
        ));
    }
    Ok(root)
}

pub(super) fn relative_string(root: &Path, path: &Path) -> Result<String, WorkspaceFileError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        WorkspaceFileError::new(
            WorkspaceFileErrorKind::OutsideWorkspace,
            path.display().to_string(),
        )
    })?;
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => components.push(value.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => {
                return Err(WorkspaceFileError::new(
                    WorkspaceFileErrorKind::OutsideWorkspace,
                    path.display().to_string(),
                ));
            }
        }
    }
    Ok(components.join("/"))
}

pub fn is_protected_workspace_path(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component, Component::Normal(value) if value.to_str().is_some_and(|value| {
            matches!(value.to_ascii_lowercase().as_str(), ".git" | ".hg" | ".svn")
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_create_new_text_file_never_overwrites_or_leaves_workspace() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join("src/old.txt"), "leave alone").unwrap();
        let root = open_workspace_file_root(&temp.path().to_string_lossy()).unwrap();
        let created = create_workspace_text_from_root(&root, "src/new.txt", "hello\n").unwrap();
        assert_eq!(created.sha256, hex::encode(Sha256::digest(b"hello\n")));
        assert_eq!(created.size_bytes, 6);
        assert_eq!(
            fs::read_to_string(temp.path().join("src/new.txt")).unwrap(),
            "hello\n"
        );
        let conflict =
            create_workspace_text_from_root(&root, "src/new.txt", "overwrite").unwrap_err();
        assert_eq!(conflict.kind, WorkspaceFileErrorKind::Conflict);
        assert_eq!(
            fs::read_to_string(temp.path().join("src/new.txt")).unwrap(),
            "hello\n"
        );
        assert_eq!(
            fs::read_to_string(temp.path().join("src/old.txt")).unwrap(),
            "leave alone"
        );
        for bad in [
            "",
            "../escape.txt",
            "src/../escape.txt",
            ".git/config",
            "src/missing/file.txt",
        ] {
            assert!(
                create_workspace_text_from_root(&root, bad, "x").is_err(),
                "{bad}"
            );
        }
        assert!(create_workspace_text_from_root(&root, "src/nul.txt", "bad\0text").is_err());
        assert!(create_workspace_text_from_root(
            &root,
            "src/large.txt",
            &"x".repeat(MAX_REMOTE_EDIT_BYTES + 1)
        )
        .is_err());
        assert!(!temp.path().join("src/large.txt").exists());
        assert!(!temp.path().join("src/nul.txt").exists());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(outside.path(), temp.path().join("link")).unwrap();
            assert!(create_workspace_text_from_root(&root, "link/outside.txt", "x").is_err());
            std::os::unix::fs::symlink(
                temp.path().join("src/old.txt"),
                temp.path().join("src/alias.txt"),
            )
            .unwrap();
            assert!(create_workspace_text_from_root(&root, "src/alias.txt", "x").is_err());
        }
    }

    #[test]
    fn mobile_edit_requires_unchanged_existing_utf8_file() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("example.txt");
        fs::write(&file, "original\n").unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join(".git/config"), "secret").unwrap();
        let root = open_workspace_file_root(&temp.path().to_string_lossy()).unwrap();
        let expected = hex::encode(Sha256::digest(b"original\n"));
        let result =
            write_workspace_text_from_root(&root, "example.txt", &expected, "edited\n").unwrap();
        assert_eq!(result.sha256, hex::encode(Sha256::digest(b"edited\n")));
        assert_eq!(fs::read_to_string(&file).unwrap(), "edited\n");
        let conflict =
            write_workspace_text_from_root(&root, "example.txt", &expected, "lost\n").unwrap_err();
        assert_eq!(conflict.kind, WorkspaceFileErrorKind::Conflict);
        assert_eq!(fs::read_to_string(&file).unwrap(), "edited\n");
        assert!(write_workspace_text_from_root(&root, "../escape.txt", &expected, "x").is_err());
        assert!(write_workspace_text_from_root(&root, ".git/config", &expected, "x").is_err());
        assert!(write_workspace_text_from_root(&root, "missing.txt", &expected, "x").is_err());
        let large = "x".repeat(MAX_REMOTE_EDIT_BYTES + 1);
        assert!(
            write_workspace_text_from_root(&root, "example.txt", &result.sha256, &large).is_err()
        );
        fs::write(temp.path().join("binary.png"), [0, 1, 2, 3]).unwrap();
        let binary_hash = hex::encode(Sha256::digest([0, 1, 2, 3]));
        assert!(write_workspace_text_from_root(&root, "binary.png", &binary_hash, "text").is_err());
    }

    #[test]
    fn workspace_directory_pages_are_sorted_bounded_and_hide_metadata() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join("z.txt"), "z").unwrap();
        fs::write(temp.path().join("a.txt"), "a").unwrap();
        fs::write(temp.path().join(".secret"), "hidden").unwrap();
        fs::write(temp.path().join(".git/config"), "protected").unwrap();
        let root = open_workspace_file_root(&temp.path().to_string_lossy()).unwrap();

        let first = list_workspace_directory_from_root(&root, "", 0, 2, false).unwrap();
        assert_eq!(first.entries.len(), 2);
        assert_eq!(first.entries[0].relative_path, "src");
        assert_eq!(first.entries[1].relative_path, "a.txt");
        assert!(first.has_more);
        let second = list_workspace_directory_from_root(&root, "", 2, 2, false).unwrap();
        assert_eq!(second.entries[0].relative_path, "z.txt");
        assert!(!second.has_more);

        let with_hidden = list_workspace_directory_from_root(&root, "", 0, 100, true).unwrap();
        assert!(with_hidden
            .entries
            .iter()
            .any(|entry| entry.name == ".secret"));
        assert!(!with_hidden.entries.iter().any(|entry| entry.name == ".git"));
        assert!(list_workspace_directory_from_root(&root, "../", 0, 2, true).is_err());
        assert!(list_workspace_directory_from_root(&root, ".git", 0, 2, true).is_err());
        assert!(list_workspace_directory_from_root(&root, "", 0, 201, false).is_err());
    }

    #[test]
    fn bounded_read_rejects_traversal_and_symlink_escape() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(workspace.path().join("inside.txt"), "hello").unwrap();
        fs::create_dir(workspace.path().join(".git")).unwrap();
        fs::write(workspace.path().join(".git/config"), "secret").unwrap();
        fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        let root = workspace.path().to_string_lossy();
        assert!(read_workspace_file_range(&root, "../secret.txt", 0, 10).is_err());
        assert_eq!(
            read_workspace_file_range(&root, ".git/config", 0, 10)
                .unwrap_err()
                .kind,
            WorkspaceFileErrorKind::InvalidPath
        );
        assert!(is_protected_workspace_path(Path::new(".GIT/config")));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                workspace.path().join(".git/config"),
                workspace.path().join("config-link"),
            )
            .unwrap();
            assert_eq!(
                read_workspace_file_range(&root, "config-link", 0, 10)
                    .unwrap_err()
                    .kind,
                WorkspaceFileErrorKind::InvalidPath
            );
            std::os::unix::fs::symlink(
                workspace.path().join(".git"),
                workspace.path().join("metadata-link"),
            )
            .unwrap();
            assert_eq!(
                read_workspace_file_range(&root, "metadata-link/config", 0, 10)
                    .unwrap_err()
                    .kind,
                WorkspaceFileErrorKind::InvalidPath
            );
            std::os::unix::fs::symlink(
                outside.path().join("secret.txt"),
                workspace.path().join("link.txt"),
            )
            .unwrap();
            assert!(read_workspace_file_range(&root, "link.txt", 0, 10).is_err());
        }
        let result = read_workspace_file_range(&root, "inside.txt", 1, 3).unwrap();
        assert_eq!(result.bytes, b"ell");
        assert_eq!(result.next_offset, 4);
    }

    #[cfg(unix)]
    #[test]
    fn opened_root_stays_pinned_when_workspace_path_is_replaced() {
        let parent = tempfile::tempdir().unwrap();
        let workspace = parent.path().join("workspace");
        let moved_workspace = parent.path().join("workspace-moved");
        let replacement = parent.path().join("replacement");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir(&replacement).unwrap();
        fs::write(workspace.join("inside.txt"), "inside").unwrap();
        fs::write(replacement.join("config"), "secret").unwrap();
        let root = open_workspace_file_root(&workspace.to_string_lossy()).unwrap();

        fs::rename(&workspace, &moved_workspace).unwrap();
        std::os::unix::fs::symlink(&replacement, &workspace).unwrap();

        let inside = read_workspace_file_range_from_root(&root, "inside.txt", 0, 10).unwrap();
        assert_eq!(inside.bytes, b"inside");
        assert!(read_workspace_file_range_from_root(&root, "config", 0, 10).is_err());
    }

    #[test]
    fn ranged_text_detection_allows_split_utf8_code_points() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("utf8.txt");
        fs::write(&path, "aéz").unwrap();
        let root = workspace.path().to_string_lossy();

        let leading_half = read_workspace_file_range(&root, "utf8.txt", 0, 2).unwrap();
        let trailing_half = read_workspace_file_range(&root, "utf8.txt", 2, 2).unwrap();

        assert!(leading_half.is_text);
        assert!(trailing_half.is_text);
    }

    #[test]
    fn ranged_text_detection_rejects_invalid_interior_utf8() {
        assert!(!range_is_probably_utf8(&[b'a', 0xff, b'z'], false));
        assert!(!range_is_probably_utf8(&[b'a', 0, b'z'], false));
        assert!(!range_is_probably_utf8(&[b'a', 0xc3], false));
        assert!(!range_is_probably_utf8(&[0x80, b'a'], false));
        assert!(range_is_probably_utf8(&[b'a', 0xc3], true));
    }

    #[test]
    fn ranged_text_detection_rejects_incomplete_utf8_at_eof() {
        let workspace = tempfile::tempdir().unwrap();
        fs::write(workspace.path().join("broken.bin"), [b'a', 0xc3]).unwrap();
        fs::write(workspace.path().join("invalid-prefix.bin"), [0x80, b'a']).unwrap();
        let root = workspace.path().to_string_lossy();

        let result = read_workspace_file_range(&root, "broken.bin", 0, 2).unwrap();
        assert!(!result.is_text);
        assert_eq!(result.mime_type, "application/octet-stream");

        let result = read_workspace_file_range(&root, "invalid-prefix.bin", 0, 2).unwrap();
        assert!(!result.is_text);
        assert_eq!(result.mime_type, "application/octet-stream");
    }

    #[cfg(unix)]
    #[test]
    fn relative_paths_preserve_literal_backslashes_in_file_names() {
        let workspace = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(workspace.path()).unwrap();
        let literal = root.join("foo\\bar.txt");
        fs::write(&literal, b"literal").unwrap();
        let canonical = dunce::canonicalize(&literal).unwrap();

        assert_eq!(relative_string(&root, &canonical).unwrap(), "foo\\bar.txt");
        let range =
            read_workspace_file_range(&root.to_string_lossy(), "foo\\bar.txt", 0, 7).unwrap();
        assert_eq!(range.bytes, b"literal");
    }

    #[test]
    fn ranged_reads_keep_mixed_file_classification_stable() {
        let workspace = tempfile::tempdir().unwrap();
        fs::write(workspace.path().join("mixed.bin"), b"hello\0world").unwrap();
        let root = workspace.path().to_string_lossy();

        let ascii_range = read_workspace_file_range(&root, "mixed.bin", 0, 5).unwrap();
        let binary_range = read_workspace_file_range(&root, "mixed.bin", 5, 6).unwrap();

        assert!(!ascii_range.is_text);
        assert!(!binary_range.is_text);
        assert_eq!(ascii_range.mime_type, "application/octet-stream");
        assert_eq!(binary_range.mime_type, "application/octet-stream");
    }

    #[test]
    fn binary_preview_mime_is_not_exposed_as_text() {
        let workspace = tempfile::tempdir().unwrap();
        fs::write(workspace.path().join("document.pdf"), b"plain ascii").unwrap();
        let root = workspace.path().to_string_lossy();

        let result = read_workspace_file_range(&root, "document.pdf", 0, 5).unwrap();

        assert!(!result.is_text);
        assert_eq!(result.mime_type, "application/pdf");
    }
}
