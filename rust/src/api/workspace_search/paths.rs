use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::{WorkspaceSearchError, WorkspaceSearchErrorKind, PROTECTED_NAMES};

pub(super) fn safe_regular_file_metadata(
    root: &Path,
    path: &Path,
    relative_path: &str,
) -> Result<fs::Metadata, WorkspaceSearchError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| WorkspaceSearchError::from_io(error, relative_path))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(WorkspaceSearchError::new(
            WorkspaceSearchErrorKind::OutsideWorkspace,
            relative_path,
        ));
    }
    let canonical = dunce::canonicalize(path)
        .map_err(|error| WorkspaceSearchError::from_io(error, relative_path))?;
    if !canonical.starts_with(root) {
        return Err(WorkspaceSearchError::new(
            WorkspaceSearchErrorKind::OutsideWorkspace,
            relative_path,
        ));
    }
    Ok(metadata)
}

pub(super) struct OpenedSearchFile {
    pub(super) file: fs::File,
    pub(super) metadata: fs::Metadata,
    // A verified handle supplies both containment and metadata in one open.
    pub(super) handle_verified: bool,
}

pub(super) fn open_search_file(
    root: &Path,
    path: &Path,
    relative_path: &str,
) -> Option<OpenedSearchFile> {
    #[cfg(windows)]
    match windows_handle::open(root, path) {
        windows_handle::Outcome::Opened(opened) => return Some(opened),
        windows_handle::Outcome::Reject => return None,
        windows_handle::Outcome::Fallback => {}
    }
    open_search_file_legacy(root, path, relative_path)
}

fn open_search_file_legacy(
    root: &Path,
    path: &Path,
    relative_path: &str,
) -> Option<OpenedSearchFile> {
    let metadata = safe_regular_file_metadata(root, path, relative_path).ok()?;
    if metadata.len() > super::MAX_TEXT_FILE_BYTES {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    Some(OpenedSearchFile {
        file,
        metadata,
        handle_verified: false,
    })
}

#[cfg(windows)]
mod windows_handle {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFinalPathNameByHandleW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_NAME_NORMALIZED, VOLUME_NAME_DOS,
    };

    pub(super) enum Outcome {
        Opened(OpenedSearchFile),
        Reject,
        Fallback,
    }

    fn final_path(file: &fs::File) -> Option<PathBuf> {
        // Most paths fit on the stack. Retry with a bounded allocation only
        // for long Windows paths; the API reports the required UTF-16 size.
        let mut small = [0_u16; 512];
        let flags = FILE_NAME_NORMALIZED | VOLUME_NAME_DOS;
        // SAFETY: the File owns a live handle, and the buffer is writable for
        // exactly its supplied length. The result is copied before it drops.
        let len = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                small.as_mut_ptr(),
                small.len() as u32,
                flags,
            )
        } as usize;
        if len == 0 {
            return None;
        }
        let path = if len < small.len() {
            OsString::from_wide(&small[..len])
        } else {
            // Windows extended-length paths are bounded at about 32k UTF-16.
            if len > 32_768 {
                return None;
            }
            let mut long = vec![0_u16; len + 1];
            let actual = unsafe {
                GetFinalPathNameByHandleW(
                    file.as_raw_handle(),
                    long.as_mut_ptr(),
                    long.len() as u32,
                    flags,
                )
            } as usize;
            if actual == 0 || actual >= long.len() {
                return None;
            }
            OsString::from_wide(&long[..actual])
        };
        Some(dunce::simplified(Path::new(&path)).to_path_buf())
    }

    pub(super) fn open(root: &Path, path: &Path) -> Outcome {
        let Ok(file) = fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
        else {
            return Outcome::Fallback;
        };
        let Ok(metadata) = file.metadata() else {
            return Outcome::Fallback;
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Outcome::Reject;
        }
        // Non-symlink reparse points (e.g. cloud placeholders) may require
        // normal open semantics. Preserve the pre-existing path checks.
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Outcome::Fallback;
        }
        if metadata.len() > super::super::MAX_TEXT_FILE_BYTES {
            return Outcome::Reject;
        }
        let Some(canonical) = final_path(&file) else {
            return Outcome::Fallback;
        };
        if !canonical.starts_with(root) {
            return Outcome::Reject;
        }
        // Reject a handle resolving into protected metadata, even if an
        // intermediary reparse point was swapped after the directory walk.
        if relative_string(root, &canonical).is_none_or(|path| is_protected_relative_path(&path)) {
            return Outcome::Reject;
        }
        Outcome::Opened(OpenedSearchFile {
            file,
            metadata,
            handle_verified: true,
        })
    }
}

#[cfg(all(test, windows))]
mod windows_open_tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn normal_file_is_verified_by_its_actual_read_handle() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let path = root.join("normal.txt");
        fs::write(&path, "needle").unwrap();
        let mut opened = open_search_file(&root, &path, "normal.txt").unwrap();
        assert!(
            opened.handle_verified,
            "normal NTFS file should use the fast path"
        );
        assert_eq!(
            content_token(&opened.metadata),
            content_token(&safe_regular_file_metadata(&root, &path, "normal.txt").unwrap())
        );
        let mut text = String::new();
        opened.file.read_to_string(&mut text).unwrap();
        assert_eq!(text, "needle");
        assert!(open_search_file(&root, &root, "").is_none());
    }

    #[test]
    fn read_handle_rejects_outside_and_protected_targets() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let external = dunce::canonicalize(outside.path()).unwrap();
        fs::write(external.join("secret.txt"), "outside").unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("secret.txt"), "protected").unwrap();
        // Junctions do not require the file-symlink privilege on NTFS.
        for (name, target) in [
            ("external-link", external.as_path()),
            ("metadata-link", root.join(".git").as_path()),
        ] {
            let link = root.join(name);
            let created = std::process::Command::new("cmd.exe")
                .args(["/D", "/C", "mklink", "/J"])
                .arg(&link)
                .arg(target)
                .output()
                .unwrap();
            if !created.status.success() {
                eprintln!(
                    "junction creation unavailable: {}",
                    String::from_utf8_lossy(&created.stderr)
                );
                continue;
            }
            let candidate = link.join("secret.txt");
            assert!(open_search_file(&root, &candidate, &format!("{name}/secret.txt")).is_none());
            // Remove the junction itself; do not recurse through its target.
            fs::remove_dir(&link).unwrap();
            assert!(target.join("secret.txt").is_file());
            println!("rejected {name} junction");
        }
        // Independently check the actual opened handle for a direct outside path.
        assert!(open_search_file(&root, &external.join("secret.txt"), "secret.txt").is_none());
    }
}

pub(super) fn resolve_replace_file(
    root: &Path,
    relative_path: &str,
) -> Result<(PathBuf, fs::Metadata), WorkspaceSearchError> {
    let path = root.join(relative_path);
    if is_protected_relative_path(relative_path) {
        return Err(WorkspaceSearchError::new(
            WorkspaceSearchErrorKind::OutsideWorkspace,
            relative_path,
        ));
    }
    let metadata = safe_regular_file_metadata(root, &path, relative_path)?;
    Ok((path, metadata))
}

pub(super) fn should_walk_entry(root: &Path, path: &Path) -> bool {
    path == root || !is_protected_workspace_path(root, path)
}

pub(super) fn workspace_root(path: &str) -> Result<PathBuf, WorkspaceSearchError> {
    let root = PathBuf::from(path);
    let canonical =
        dunce::canonicalize(&root).map_err(|error| WorkspaceSearchError::from_io(error, path))?;
    if !canonical.is_dir() {
        return Err(WorkspaceSearchError::new(
            WorkspaceSearchErrorKind::InvalidPath,
            path,
        ));
    }
    Ok(canonical)
}

pub(super) fn relative_string(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

pub(super) fn is_protected_relative_path(relative_path: &str) -> bool {
    relative_path
        .split(['/', '\\'])
        .any(|component| PROTECTED_NAMES.contains(&component))
}

fn is_protected_workspace_path(root: &Path, path: &Path) -> bool {
    let Ok(relative_path) = path.strip_prefix(root) else {
        return true;
    };
    relative_path.components().any(|component| match component {
        Component::Normal(name) => PROTECTED_NAMES.contains(&name.to_string_lossy().as_ref()),
        _ => false,
    })
}

pub(super) fn content_token(metadata: &fs::Metadata) -> String {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("{}:{modified}", metadata.len())
}
