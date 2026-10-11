#[cfg(any(test, unix))]
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptions, OpenOptionsFollowExt};
use cap_std::{ambient_authority, fs::Dir};
use same_file::Handle;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    create_workspace_text_from_root, file_is_probably_utf8, is_protected_workspace_path,
    mobile_workspace_leaf_name_is_safe, open_workspace_file_without_symlinks,
    reject_workspace_symlink, WorkspaceFileError, WorkspaceFileErrorKind, WorkspaceFileRoot,
    MAX_REMOTE_EDIT_BYTES,
};

const STORAGE_DIR: &str = "mobile-workspace-trash";
const MAX_RECORDS: usize = 64;
const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 2 * 1024 * 1024;
static TRASH_CHANGE_GATE: Mutex<()> = Mutex::new(());

pub struct WorkspaceTextTrashRoot {
    dir: Dir,
    workspace_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTextTrashEntry {
    pub id: String,
    pub relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub deleted_at_ms: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashRecord {
    version: u32,
    workspace_key: String,
    entry: WorkspaceTextTrashEntry,
    text: String,
}

fn trash_error(kind: WorkspaceFileErrorKind, message: &str) -> WorkspaceFileError {
    WorkspaceFileError::new(kind, message)
}

fn io_error(error: std::io::Error) -> WorkspaceFileError {
    WorkspaceFileError::from_io(error, "Mobile workspace trash")
}

fn open_or_create_private_directory(parent: &Dir, name: &str) -> Result<Dir, WorkspaceFileError> {
    match parent.create_dir(name) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(io_error(error)),
    }
    let dir = parent.open_dir_nofollow(name).map_err(io_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        dir.set_permissions(".", fs::Permissions::from_mode(0o700))
            .map_err(io_error)?;
    }
    Ok(dir)
}

/// The mobile trash is created under the trusted host Runtime directory, never
/// inside a repository. Workspace scope is a one-way SHA-256 directory name;
/// the phone cannot supply arbitrary backup locations.
pub fn open_workspace_text_trash(
    runtime_dir: &Path,
    workspace_id: &str,
    workspace: &WorkspaceFileRoot,
) -> Result<WorkspaceTextTrashRoot, WorkspaceFileError> {
    if workspace_id.is_empty() {
        return Err(trash_error(
            WorkspaceFileErrorKind::InvalidPath,
            "Missing workspace id",
        ));
    }
    let runtime = Dir::open_ambient_dir(runtime_dir, ambient_authority()).map_err(io_error)?;
    let canonical = dunce::canonicalize(runtime_dir).map_err(io_error)?;
    if canonical.starts_with(workspace.canonical_path()) {
        return Err(trash_error(
            WorkspaceFileErrorKind::InvalidPath,
            "Runtime trash location must be outside the workspace",
        ));
    }
    let runtime_handle = Handle::from_file(runtime.try_clone().map_err(io_error)?.into_std_file())
        .map_err(io_error)?;
    if runtime_handle != Handle::from_path(&canonical).map_err(io_error)? {
        return Err(trash_error(
            WorkspaceFileErrorKind::InvalidPath,
            "Runtime directory changed",
        ));
    }
    let root = open_or_create_private_directory(&runtime, STORAGE_DIR)?;
    let workspace_key = hex::encode(Sha256::digest(workspace_id.as_bytes()));
    let dir = open_or_create_private_directory(&root, &workspace_key)?;
    Ok(WorkspaceTextTrashRoot { dir, workspace_key })
}

fn checked_id(id: &str) -> Result<String, WorkspaceFileError> {
    let parsed = Uuid::parse_str(id).map_err(|_| {
        trash_error(
            WorkspaceFileErrorKind::InvalidPath,
            "Invalid trash entry id",
        )
    })?;
    let value = parsed.to_string();
    if value != id {
        return Err(trash_error(
            WorkspaceFileErrorKind::InvalidPath,
            "Noncanonical trash id",
        ));
    }
    Ok(format!("{value}.json"))
}

fn read_record(root: &WorkspaceTextTrashRoot, id: &str) -> Result<TrashRecord, WorkspaceFileError> {
    let name = checked_id(id)?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = root
        .dir
        .open_with(&name, &options)
        .map_err(io_error)?
        .into_std();
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES {
        return Err(trash_error(
            WorkspaceFileErrorKind::Unsupported,
            "Invalid trash record size",
        ));
    }
    let mut data = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(io_error)?;
    if data.len() as u64 > MAX_RECORD_BYTES {
        return Err(trash_error(
            WorkspaceFileErrorKind::Unsupported,
            "Trash record exceeded size limit",
        ));
    }
    let record: TrashRecord = serde_json::from_slice(&data)
        .map_err(|_| trash_error(WorkspaceFileErrorKind::InvalidPath, "Damaged trash record"))?;
    if record.version != 1
        || record.entry.id != id
        || record.workspace_key != root.workspace_key
        || record.entry.size_bytes != record.text.len() as u64
        || record.entry.size_bytes > MAX_REMOTE_EDIT_BYTES as u64
        || record.text.contains('\0')
        || hex::encode(Sha256::digest(record.text.as_bytes())) != record.entry.sha256
    {
        return Err(trash_error(
            WorkspaceFileErrorKind::InvalidPath,
            "Trash record failed verification",
        ));
    }
    let path = Path::new(&record.entry.relative_path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || is_protected_workspace_path(path)
        || !path
            .file_name()
            .map(mobile_workspace_leaf_name_is_safe)
            .unwrap_or(false)
    {
        return Err(trash_error(
            WorkspaceFileErrorKind::InvalidPath,
            "Trash record contains an unsafe filename",
        ));
    }
    Ok(record)
}

fn entries(
    root: &WorkspaceTextTrashRoot,
) -> Result<Vec<WorkspaceTextTrashEntry>, WorkspaceFileError> {
    let mut result = Vec::new();
    for item in root.dir.read_dir(".").map_err(io_error)? {
        let item = item.map_err(io_error)?;
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if checked_id(id).is_err() {
            return Err(trash_error(
                WorkspaceFileErrorKind::InvalidPath,
                "Unexpected trash record filename",
            ));
        }
        result.push(read_record(root, id)?.entry);
        if result.len() > MAX_RECORDS {
            return Err(trash_error(
                WorkspaceFileErrorKind::Unsupported,
                "Mobile trash capacity exceeded",
            ));
        }
    }
    result.sort_by(|left, right| {
        right
            .deleted_at_ms
            .cmp(&left.deleted_at_ms)
            .then_with(|| right.id.cmp(&left.id))
    });
    Ok(result)
}

pub fn list_workspace_text_trash(
    root: &WorkspaceTextTrashRoot,
) -> Result<Vec<WorkspaceTextTrashEntry>, WorkspaceFileError> {
    let _gate = TRASH_CHANGE_GATE.lock().map_err(|_| {
        trash_error(
            WorkspaceFileErrorKind::Io,
            "Mobile trash operation lock is unavailable",
        )
    })?;
    entries(root)
}

/// Keep an exact, fsynced copy outside the worktree *before* any unlink.
/// Failures never discard the backup: the phone may recover from the trash
/// even if the source was replaced while the operation was running.
pub fn move_workspace_text_to_trash(
    workspace: &WorkspaceFileRoot,
    trash: &WorkspaceTextTrashRoot,
    relative_path: &str,
    expected_sha256: &str,
) -> Result<WorkspaceTextTrashEntry, WorkspaceFileError> {
    let _gate = TRASH_CHANGE_GATE.lock().map_err(|_| {
        trash_error(
            WorkspaceFileErrorKind::Io,
            "Mobile trash operation lock is unavailable",
        )
    })?;
    let path = Path::new(relative_path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || is_protected_workspace_path(path)
        || !path
            .file_name()
            .map(mobile_workspace_leaf_name_is_safe)
            .unwrap_or(false)
        || expected_sha256.len() != 64
        || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(trash_error(
            WorkspaceFileErrorKind::InvalidPath,
            "Invalid trash path or checksum",
        ));
    }
    let (mut source, normalized) = open_workspace_file_without_symlinks(workspace, relative_path)?;
    let metadata = source.metadata().map_err(io_error)?;
    if !metadata.is_file()
        || metadata.len() > MAX_REMOTE_EDIT_BYTES as u64
        || metadata.permissions().readonly()
        || !file_is_probably_utf8(&mut source, &normalized, metadata.len(), relative_path)?
    {
        return Err(trash_error(
            WorkspaceFileErrorKind::Unsupported,
            "Only writable UTF-8 text files up to 256 KiB can be moved to trash",
        ));
    }
    source.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let mut bytes = Vec::new();
    (&mut source)
        .take(MAX_REMOTE_EDIT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_REMOTE_EDIT_BYTES
        || bytes.len() as u64 != metadata.len()
        || bytes.contains(&0)
        || std::str::from_utf8(&bytes).is_err()
        || !hex::encode(Sha256::digest(&bytes)).eq_ignore_ascii_case(expected_sha256)
    {
        return Err(trash_error(
            WorkspaceFileErrorKind::Conflict,
            "File changed since it was opened; reload before moving to trash",
        ));
    }
    let existing = entries(trash)?;
    let used = existing.iter().map(|entry| entry.size_bytes).sum::<u64>();
    if existing.len() >= MAX_RECORDS || used.saturating_add(bytes.len() as u64) > MAX_TOTAL_BYTES {
        return Err(trash_error(
            WorkspaceFileErrorKind::Unsupported,
            "Trash is full (64 files or 16 MiB); no source was removed",
        ));
    }
    let id = Uuid::new_v4().to_string();
    let entry = WorkspaceTextTrashEntry {
        id: id.clone(),
        relative_path: relative_path.to_string(),
        sha256: hex::encode(Sha256::digest(&bytes)),
        size_bytes: bytes.len() as u64,
        deleted_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };
    let text = String::from_utf8(bytes).expect("UTF-8 verified before trash record creation");
    let record = TrashRecord {
        version: 1,
        workspace_key: trash.workspace_key.clone(),
        entry: entry.clone(),
        text,
    };
    let encoded = serde_json::to_vec(&record).map_err(|_| {
        trash_error(
            WorkspaceFileErrorKind::Io,
            "Unable to serialize trash backup",
        )
    })?;
    if encoded.len() as u64 > MAX_RECORD_BYTES {
        return Err(trash_error(
            WorkspaceFileErrorKind::Unsupported,
            "Trash record too large",
        ));
    }
    let name = checked_id(&id)?;
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    let mut backup = trash
        .dir
        .open_with(&name, &options)
        .map_err(io_error)?
        .into_std();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        backup
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(io_error)?;
    }
    if let Err(error) = backup.write_all(&encoded).and_then(|_| backup.sync_all()) {
        drop(backup);
        let _ = trash.dir.remove_file(&name);
        return Err(io_error(error));
    }
    drop(backup);
    #[cfg(unix)]
    trash
        .dir
        .try_clone()
        .map_err(io_error)?
        .into_std_file()
        .sync_all()
        .map_err(io_error)?;

    // Reopen after recording the durable backup. Refuse unlink if a concurrent
    // actor replaced the source path, or modified its contents since preview.
    let held = Handle::from_file(source.try_clone().map_err(io_error)?).map_err(io_error)?;
    let (mut latest, _) = open_workspace_file_without_symlinks(workspace, relative_path)?;
    if Handle::from_file(latest.try_clone().map_err(io_error)?).map_err(io_error)? != held {
        return Err(trash_error(
            WorkspaceFileErrorKind::Conflict,
            "File was replaced during trash operation; backup retained",
        ));
    }
    let mut latest_bytes = Vec::new();
    (&mut latest)
        .take(MAX_REMOTE_EDIT_BYTES as u64 + 1)
        .read_to_end(&mut latest_bytes)
        .map_err(io_error)?;
    if hex::encode(Sha256::digest(&latest_bytes)) != entry.sha256 {
        return Err(trash_error(
            WorkspaceFileErrorKind::Conflict,
            "File changed during trash operation; backup retained",
        ));
    }
    let mut parent = workspace.directory.try_clone().map_err(io_error)?;
    let components = path.components().collect::<Vec<_>>();
    for component in &components[..components.len() - 1] {
        if let Component::Normal(name) = component {
            reject_workspace_symlink(&parent, name, relative_path)?;
            parent = parent.open_dir_nofollow(name).map_err(io_error)?;
        }
    }
    let Some(Component::Normal(file_name)) = components.last() else {
        unreachable!()
    };
    drop(latest);
    drop(source);
    parent.remove_file(file_name).map_err(io_error)?;
    Ok(entry)
}

/// Restore only when the original name is free; never overwrite work performed
/// since the phone moved the text to trash. The backup is deleted only after
/// a fully durable create-new restoration succeeds.
pub fn restore_workspace_text_from_trash(
    workspace: &WorkspaceFileRoot,
    trash: &WorkspaceTextTrashRoot,
    id: &str,
) -> Result<String, WorkspaceFileError> {
    let _gate = TRASH_CHANGE_GATE.lock().map_err(|_| {
        trash_error(
            WorkspaceFileErrorKind::Io,
            "Mobile trash operation lock is unavailable",
        )
    })?;
    let record = read_record(trash, id)?;
    let path = record.entry.relative_path;
    let created = create_workspace_text_from_root(workspace, &path, &record.text)?;
    if created.sha256 != record.entry.sha256 {
        return Err(trash_error(
            WorkspaceFileErrorKind::Io,
            "Restored file did not match trash checksum",
        ));
    }
    trash.dir.remove_file(checked_id(id)?).map_err(io_error)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_files::open_workspace_file_root;

    #[test]
    fn backup_then_restore_is_no_clobber_and_scoped_to_workspace() {
        let workspace_dir = tempfile::tempdir().unwrap();
        let other_dir = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        fs::create_dir(workspace_dir.path().join("src")).unwrap();
        fs::create_dir(other_dir.path().join("src")).unwrap();
        fs::write(workspace_dir.path().join("src/demo.md"), "old text\n").unwrap();
        let root = open_workspace_file_root(&workspace_dir.path().to_string_lossy()).unwrap();
        let other = open_workspace_file_root(&other_dir.path().to_string_lossy()).unwrap();
        let trash = open_workspace_text_trash(runtime.path(), "workspace-A", &root).unwrap();
        let other_trash = open_workspace_text_trash(runtime.path(), "workspace-B", &other).unwrap();
        let digest = hex::encode(Sha256::digest(b"old text\n"));
        assert!(move_workspace_text_to_trash(
            &root,
            &trash,
            "src/demo.md",
            &hex::encode(Sha256::digest(b"old"))
        )
        .is_err());
        assert!(workspace_dir.path().join("src/demo.md").exists());
        let removed = move_workspace_text_to_trash(&root, &trash, "src/demo.md", &digest).unwrap();
        assert!(!workspace_dir.path().join("src/demo.md").exists());
        let reopened = open_workspace_text_trash(runtime.path(), "workspace-A", &root).unwrap();
        assert_eq!(
            list_workspace_text_trash(&reopened).unwrap(),
            vec![removed.clone()]
        );
        assert_eq!(
            list_workspace_text_trash(&trash).unwrap(),
            vec![removed.clone()]
        );
        assert!(list_workspace_text_trash(&other_trash).unwrap().is_empty());
        assert!(restore_workspace_text_from_trash(&other, &other_trash, &removed.id).is_err());
        fs::write(workspace_dir.path().join("src/demo.md"), "newer text\n").unwrap();
        assert_eq!(
            restore_workspace_text_from_trash(&root, &trash, &removed.id)
                .unwrap_err()
                .kind,
            WorkspaceFileErrorKind::Conflict
        );
        assert_eq!(
            fs::read_to_string(workspace_dir.path().join("src/demo.md")).unwrap(),
            "newer text\n"
        );
        fs::remove_file(workspace_dir.path().join("src/demo.md")).unwrap();
        assert_eq!(
            restore_workspace_text_from_trash(&root, &trash, &removed.id).unwrap(),
            "src/demo.md"
        );
        assert_eq!(
            fs::read_to_string(workspace_dir.path().join("src/demo.md")).unwrap(),
            "old text\n"
        );
        assert!(list_workspace_text_trash(&trash).unwrap().is_empty());
        assert!(restore_workspace_text_from_trash(&root, &trash, &removed.id).is_err());
        assert!(restore_workspace_text_from_trash(&root, &trash, "../escape").is_err());
    }

    #[test]
    fn trash_rejects_protected_paths_symlinks_and_runtime_inside_repo() {
        let workspace_dir = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        fs::create_dir(workspace_dir.path().join(".git")).unwrap();
        fs::write(workspace_dir.path().join("document.txt"), "safe").unwrap();
        let root = open_workspace_file_root(&workspace_dir.path().to_string_lossy()).unwrap();
        assert!(open_workspace_text_trash(workspace_dir.path(), "workspace-A", &root).is_err());
        let trash = open_workspace_text_trash(runtime.path(), "workspace-A", &root).unwrap();
        let hash = hex::encode(Sha256::digest(b"safe"));
        for bad in [
            "../escape",
            ".git/config",
            "document.txt/other",
            "CON",
            "document.txt:ads",
        ] {
            assert!(
                move_workspace_text_to_trash(&root, &trash, bad, &hash).is_err(),
                "{bad}"
            );
        }
        assert!(workspace_dir.path().join("document.txt").exists());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(outside.path(), workspace_dir.path().join("link")).unwrap();
            assert!(move_workspace_text_to_trash(&root, &trash, "link/outside", &hash).is_err());
        }
    }

    #[test]
    fn full_trash_refuses_deletion_and_restoring_frees_capacity() {
        let workspace_dir = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let root = open_workspace_file_root(&workspace_dir.path().to_string_lossy()).unwrap();
        let trash = open_workspace_text_trash(runtime.path(), "workspace-capacity", &root).unwrap();
        let checksum = hex::encode(Sha256::digest(b"x"));
        let mut first = None;
        for index in 0..MAX_RECORDS {
            let name = format!("file-{index}.md");
            fs::write(workspace_dir.path().join(&name), b"x").unwrap();
            let entry = move_workspace_text_to_trash(&root, &trash, &name, &checksum).unwrap();
            if index == 0 {
                first = Some(entry.id);
            }
        }
        fs::write(workspace_dir.path().join("extra.md"), b"x").unwrap();
        assert_eq!(
            move_workspace_text_to_trash(&root, &trash, "extra.md", &checksum)
                .unwrap_err()
                .kind,
            WorkspaceFileErrorKind::Unsupported,
        );
        assert_eq!(
            fs::read_to_string(workspace_dir.path().join("extra.md")).unwrap(),
            "x"
        );
        restore_workspace_text_from_trash(&root, &trash, &first.unwrap()).unwrap();
        move_workspace_text_to_trash(&root, &trash, "extra.md", &checksum).unwrap();
        assert!(!workspace_dir.path().join("extra.md").exists());
    }

    #[test]
    fn damaged_backups_fail_closed_without_rewriting_a_path() {
        let workspace_dir = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        fs::write(workspace_dir.path().join("notes.md"), "before").unwrap();
        let root = open_workspace_file_root(&workspace_dir.path().to_string_lossy()).unwrap();
        let trash = open_workspace_text_trash(runtime.path(), "workspace-corrupt", &root).unwrap();
        let digest = hex::encode(Sha256::digest(b"before"));
        let item = move_workspace_text_to_trash(&root, &trash, "notes.md", &digest).unwrap();
        let record_path = runtime
            .path()
            .join(STORAGE_DIR)
            .join(&trash.workspace_key)
            .join(format!("{}.json", item.id));
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
        record["text"] = "tampered".into();
        fs::write(&record_path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(restore_workspace_text_from_trash(&root, &trash, &item.id).is_err());
        assert!(list_workspace_text_trash(&trash).is_err());
        assert!(!workspace_dir.path().join("notes.md").exists());
    }
}
