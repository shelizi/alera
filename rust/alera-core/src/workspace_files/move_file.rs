use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path};

use cap_fs_ext::DirExt;
use cap_std::fs::Dir;
use same_file::Handle;
use sha2::{Digest, Sha256};

use super::{
    file_is_probably_utf8, is_protected_workspace_path, mobile_workspace_leaf_name_is_safe,
    open_workspace_file_without_symlinks, reject_workspace_symlink, WorkspaceFileError,
    WorkspaceFileErrorKind, WorkspaceFileRoot, MAX_REMOTE_EDIT_BYTES,
};

fn invalid_path(message: &'static str) -> WorkspaceFileError {
    WorkspaceFileError::new(WorkspaceFileErrorKind::InvalidPath, message)
}

fn moved_conflict(message: &'static str) -> WorkspaceFileError {
    WorkspaceFileError::new(WorkspaceFileErrorKind::Conflict, message)
}

fn rooted_existing_folder(
    root: &WorkspaceFileRoot,
    relative: &Path,
) -> Result<Dir, WorkspaceFileError> {
    let mut directory = root.directory.try_clone().map_err(|error| {
        WorkspaceFileError::from_io(error, root.canonical_path.display().to_string())
    })?;
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(invalid_path("Unsafe mobile destination directory"));
        };
        if !mobile_workspace_leaf_name_is_safe(name) {
            return Err(invalid_path(
                "Unsupported directory name in mobile destination",
            ));
        }
        reject_workspace_symlink(&directory, name, "Mobile destination directory")?;
        directory = directory
            .open_dir_nofollow(name)
            .map_err(|error| WorkspaceFileError::from_io(error, "Mobile destination directory"))?;
    }
    Ok(directory)
}

/// Move a small, fully-read UTF-8 file to an *existing* directory inside the
/// same workspace and filesystem. The basename and file metadata are kept.
/// Hard-link reservation refuses overwriting targets, including symlinks.
/// There is deliberately no copy-and-delete fallback for cross-volume moves.
pub fn move_workspace_text_from_root(
    root: &WorkspaceFileRoot,
    relative_path: &str,
    destination_directory: &str,
    expected_sha256: &str,
) -> Result<String, WorkspaceFileError> {
    let source_path = Path::new(relative_path);
    let destination_path = Path::new(destination_directory);
    if source_path.as_os_str().is_empty()
        || source_path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || destination_path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || is_protected_workspace_path(source_path)
        || is_protected_workspace_path(destination_path)
        || expected_sha256.len() != 64
        || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid_path(
            "Unsafe mobile workspace move path or checksum",
        ));
    }
    let Some(file_name) = source_path.file_name() else {
        return Err(invalid_path("Missing mobile move source filename"));
    };
    if !mobile_workspace_leaf_name_is_safe(file_name) {
        return Err(invalid_path("Unsupported mobile move source filename"));
    }
    let source_directory = source_path.parent().unwrap_or(Path::new(""));
    if source_directory == destination_path {
        return Err(moved_conflict(
            "Move target is already the current directory",
        ));
    }
    let mut target_path = destination_path.to_path_buf();
    target_path.push(file_name);
    if is_protected_workspace_path(&target_path) {
        return Err(invalid_path("Protected mobile workspace move destination"));
    }
    let normalized_target = target_path
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");

    let (mut source, normalized_source) =
        open_workspace_file_without_symlinks(root, relative_path)?;
    let metadata = source
        .metadata()
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    if !metadata.is_file()
        || metadata.len() > MAX_REMOTE_EDIT_BYTES as u64
        || metadata.permissions().readonly()
        || !file_is_probably_utf8(
            &mut source,
            &normalized_source,
            metadata.len(),
            relative_path,
        )?
    {
        return Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Unsupported,
            "Only writable UTF-8 text files up to 256 KiB may be moved",
        ));
    }
    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    let mut bytes = Vec::new();
    (&mut source)
        .take(MAX_REMOTE_EDIT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    if bytes.len() as u64 != metadata.len()
        || bytes.len() > MAX_REMOTE_EDIT_BYTES
        || bytes.contains(&0)
        || std::str::from_utf8(&bytes).is_err()
        || !hex::encode(Sha256::digest(&bytes)).eq_ignore_ascii_case(expected_sha256)
    {
        return Err(moved_conflict("Source file changed; reopen before moving"));
    }
    let source_identity = Handle::from_file(
        source
            .try_clone()
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?,
    )
    .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
    let source_parent = rooted_existing_folder(root, source_directory)?;
    let destination_parent = rooted_existing_folder(root, destination_path)?;
    source_parent
        .hard_link(file_name, &destination_parent, file_name)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                moved_conflict("Destination already exists; existing files are not replaced")
            } else {
                WorkspaceFileError::from_io(
                    error,
                    "Move requires both directories on the same filesystem",
                )
            }
        })?;

    let outcome = (|| {
        let (mut source_now, _) = open_workspace_file_without_symlinks(root, relative_path)?;
        let (mut target_now, _) = open_workspace_file_without_symlinks(root, &normalized_target)?;
        for opened in [&source_now, &target_now] {
            let identity = Handle::from_file(
                opened
                    .try_clone()
                    .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?,
            )
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
            if identity != source_identity {
                return Err(moved_conflict("File identity changed during move"));
            }
        }
        let mut data = Vec::new();
        (&mut source_now)
            .take(MAX_REMOTE_EDIT_BYTES as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
        if !hex::encode(Sha256::digest(&data)).eq_ignore_ascii_case(expected_sha256) {
            return Err(moved_conflict("Source content changed during move"));
        }
        data.clear();
        (&mut target_now)
            .take(MAX_REMOTE_EDIT_BYTES as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
        if !hex::encode(Sha256::digest(&data)).eq_ignore_ascii_case(expected_sha256) {
            return Err(moved_conflict("Target content changed during move"));
        }
        drop(source_now);
        drop(target_now);
        drop(source);
        source_parent
            .remove_file(file_name)
            .map_err(|error| WorkspaceFileError::from_io(error, relative_path))?;
        Ok(normalized_target.clone())
    })();
    if outcome.is_err() {
        // Leave the original intact. Only undo a link if both current names
        // still refer to the opened original: never delete other agent work.
        let original = open_workspace_file_without_symlinks(root, relative_path)
            .ok()
            .and_then(|(file, _)| Handle::from_file(file).ok());
        let destination = open_workspace_file_without_symlinks(root, &normalized_target)
            .ok()
            .and_then(|(file, _)| Handle::from_file(file).ok());
        if original.as_ref() == Some(&source_identity)
            && destination.as_ref() == Some(&source_identity)
        {
            let _ = destination_parent.remove_file(file_name);
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_files::open_workspace_file_root;
    use std::fs;

    #[test]
    fn move_preserves_content_and_metadata_and_rejects_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("src/nested")).unwrap();
        fs::create_dir_all(tmp.path().join("target/nested")).unwrap();
        fs::write(tmp.path().join("src/nested/demo.md"), "content\n").unwrap();
        fs::write(tmp.path().join("target/nested/demo.md"), "must remain").unwrap();
        let root = open_workspace_file_root(&tmp.path().to_string_lossy()).unwrap();
        let hash = hex::encode(Sha256::digest(b"content\n"));
        let source = "src/nested/demo.md";
        assert_eq!(
            move_workspace_text_from_root(&root, source, "target/nested", &hash)
                .unwrap_err()
                .kind,
            WorkspaceFileErrorKind::Conflict
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("target/nested/demo.md")).unwrap(),
            "must remain"
        );
        assert!(tmp.path().join(source).exists());
        fs::remove_file(tmp.path().join("target/nested/demo.md")).unwrap();
        let original_identity = Handle::from_path(tmp.path().join(source)).unwrap();
        let moved = move_workspace_text_from_root(&root, source, "target/nested", &hash).unwrap();
        assert_eq!(moved, "target/nested/demo.md");
        assert!(!tmp.path().join(source).exists());
        assert_eq!(
            Handle::from_path(tmp.path().join("target/nested/demo.md")).unwrap(),
            original_identity,
            "Cross-folder move must preserve filesystem file identity, not copy bytes",
        );
        assert_eq!(
            fs::read_to_string(tmp.path().join("target/nested/demo.md")).unwrap(),
            "content\n"
        );
        let back =
            move_workspace_text_from_root(&root, "target/nested/demo.md", "", &hash).unwrap();
        assert_eq!(back, "demo.md");
        assert_eq!(
            fs::read_to_string(tmp.path().join("demo.md")).unwrap(),
            "content\n"
        );
    }

    #[test]
    fn move_rejects_stale_hash_nonexistent_dirs_and_protected_paths() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::create_dir(tmp.path().join("dst")).unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join("src/note.txt"), "unchanged").unwrap();
        fs::write(tmp.path().join("src/binary.txt"), [0, 1, 2]).unwrap();
        let root = open_workspace_file_root(&tmp.path().to_string_lossy()).unwrap();
        let sha = hex::encode(Sha256::digest(b"unchanged"));
        for (source, dest, digest) in [
            (
                "src/note.txt",
                "dst",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
            ("src/note.txt", "src", sha.as_str()),
            ("src/note.txt", "missing", sha.as_str()),
            ("src/note.txt", "../outside", sha.as_str()),
            ("src/note.txt", ".git", sha.as_str()),
            (".git/config", "dst", sha.as_str()),
            ("../escape", "dst", sha.as_str()),
            ("src/binary.txt", "dst", sha.as_str()),
        ] {
            assert!(
                move_workspace_text_from_root(&root, source, dest, digest).is_err(),
                "{source} -> {dest}"
            );
        }
        assert_eq!(
            fs::read_to_string(tmp.path().join("src/note.txt")).unwrap(),
            "unchanged"
        );
        assert!(!tmp.path().join("dst/note.txt").exists());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(outside.path(), tmp.path().join("link")).unwrap();
            assert!(move_workspace_text_from_root(&root, "src/note.txt", "link", &sha).is_err());
            std::os::unix::fs::symlink(
                tmp.path().join("src/note.txt"),
                tmp.path().join("src/symlink.txt"),
            )
            .unwrap();
            assert!(move_workspace_text_from_root(&root, "src/symlink.txt", "dst", &sha).is_err());
        }
    }
}
