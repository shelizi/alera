//! Private, immutable version snapshots for stale shared Agent transcripts.
//! This module deliberately uses only std so its file safety tests can run
//! without linking the entire Alera Runtime Host on Windows.
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ARCHIVE_VERSIONS: usize = 16;
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArchiveResult {
    Created,
    AlreadyPresent,
    CapacityReached,
}

/// Keep an immutable, content-addressed version in the private Runtime Host
/// storage directory. This never writes into a provider's native history tree.
/// Caller validates that content is a stable, complete JSONL snapshot.
pub(crate) fn preserve_version(
    archive_root: &Path,
    identity_sha256: &str,
    content_sha256: &str,
    bytes: &[u8],
    metadata_json: &[u8],
) -> io::Result<ArchiveResult> {
    if !valid_sha256(identity_sha256) || !valid_sha256(content_sha256) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Invalid snapshot identity",
        ));
    }
    if bytes.is_empty()
        || bytes.len() > MAX_SNAPSHOT_BYTES
        || bytes.last() != Some(&b'\n')
        || metadata_json.is_empty()
        || metadata_json.len() > 4096
    {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Invalid JSONL snapshot",
        ));
    }
    ensure_private_directory(archive_root)?;
    let bucket = archive_root.join(identity_sha256);
    ensure_private_directory(&bucket)?;
    let name = format!("{content_sha256}.jsonl");
    let target = bucket.join(name);
    match fs::symlink_metadata(&target) {
        Ok(meta) if meta.file_type().is_file() => {
            verify_existing(&target, bytes)?;
            verify_existing(&bucket.join("origin.json"), metadata_json)?;
            return Ok(ArchiveResult::AlreadyPresent);
        }
        Ok(_) => {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Snapshot path is not a file",
            ))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    // No eviction. When the cap is reached, fail closed and retain all previous
    // versions; the background caller logs the retention limit.
    let mut count = 0;
    for entry in fs::read_dir(&bucket)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|ext| ext == "jsonl") {
            count += 1;
            if count >= MAX_ARCHIVE_VERSIONS {
                return Ok(ArchiveResult::CapacityReached);
            }
        }
    }

    write_immutable(&bucket.join("origin.json"), metadata_json)?;
    let result = write_immutable(&target, bytes)?;
    Ok(if result {
        ArchiveResult::Created
    } else {
        ArchiveResult::AlreadyPresent
    })
}

/// Immutable files must match their claimed content; never silently accept
/// an existing filename that points to tampered or divergent data.
fn verify_existing(path: &Path, expected: &[u8]) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file()
        || meta.len() != expected.len() as u64
        || fs::read(path)? != expected
    {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Archive content mismatch",
        ));
    }
    Ok(())
}

/// Bounded, metadata-only inventory. No transcript or account path leaves the
/// Runtime Host; used by Classic/GPUI settings diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ArchiveSummary {
    pub sessions: u64,
    pub versions: u64,
    pub truncated: bool,
}

pub(crate) fn summarize_private_versions(archive_root: &Path) -> io::Result<ArchiveSummary> {
    let metadata = match fs::symlink_metadata(archive_root) {
        Ok(value) => value,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(ArchiveSummary::default()),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_dir() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Unsafe archive root",
        ));
    }
    let mut summary = ArchiveSummary::default();
    for (index, entry) in fs::read_dir(archive_root)?.enumerate() {
        if index >= 512 {
            summary.truncated = true;
            break;
        }
        let entry = entry?;
        let name = entry.file_name();
        if !valid_sha256(&name.to_string_lossy()) || !entry.file_type()?.is_dir() {
            continue;
        }
        let mut versions = 0u64;
        for (index, file) in fs::read_dir(entry.path())?.enumerate() {
            if index >= 64 {
                summary.truncated = true;
                break;
            }
            let file = file?;
            let file_name = file.file_name();
            let file_name = file_name.to_string_lossy();
            if let Some(hash) = file_name.strip_suffix(".jsonl") {
                if valid_sha256(hash) && file.file_type()?.is_file() {
                    versions += 1;
                }
            }
        }
        if versions > 0 {
            summary.sessions += 1;
            summary.versions += versions;
        }
    }
    Ok(summary)
}

fn valid_sha256(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|char| char.is_ascii_hexdigit())
}

fn ensure_private_directory(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            }
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_dir() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Unsafe archive directory",
        ));
    }
    Ok(())
}

fn write_immutable(target: &Path, bytes: &[u8]) -> io::Result<bool> {
    match fs::symlink_metadata(target) {
        Ok(meta) if meta.file_type().is_file() => {
            verify_existing(target, bytes)?;
            return Ok(false);
        }
        Ok(_) => {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Unexpected archive item",
            ))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "No parent"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let temp = parent.join(format!(".alera-{}-{stamp}.tmp", std::process::id()));
    let result = (|| -> io::Result<bool> {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // Atomic no-clobber creation even if a second worker races.
        match fs::hard_link(&temp, target) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                verify_existing(target, bytes)?;
                Ok(false)
            }
            Err(error) => Err(error),
        }
    })();
    let _ = fs::remove_file(temp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "alera-snapshot-test-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
    fn hash(letter: char) -> String {
        std::iter::repeat_n(letter, 64).collect()
    }
    fn store(root: &Path, version: char, content: &[u8]) -> io::Result<ArchiveResult> {
        preserve_version(
            root,
            &hash('a'),
            &hash(version),
            content,
            b"{\"provider\":\"codex\"}",
        )
    }

    #[test]
    fn immutable_versions_accept_new_data_and_dedupe_identical_content() {
        let fixture = fixture();
        let root = fixture.join("archive");
        assert_eq!(
            store(&root, 'b', b"{\"turn\":1}\n").unwrap(),
            ArchiveResult::Created
        );
        assert_eq!(
            store(&root, 'b', b"{\"turn\":1}\n").unwrap(),
            ArchiveResult::AlreadyPresent
        );
        assert_eq!(
            store(&root, 'c', b"{\"turn\":1}\n{\"turn\":2}\n").unwrap(),
            ArchiveResult::Created
        );
        assert_eq!(
            fs::read(root.join(hash('a')).join(format!("{}.jsonl", hash('b')))).unwrap(),
            b"{\"turn\":1}\n"
        );
        assert_eq!(
            fs::read(root.join(hash('a')).join(format!("{}.jsonl", hash('c')))).unwrap(),
            b"{\"turn\":1}\n{\"turn\":2}\n"
        );
        assert_eq!(fs::read_dir(root.join(hash('a'))).unwrap().count(), 3);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn existing_archive_or_metadata_tampering_is_reported_without_overwrite() {
        let fixture = fixture();
        let root = fixture.join("archive");
        assert_eq!(store(&root, 'b', b"{}\n").unwrap(), ArchiveResult::Created);
        let version = root.join(hash('a')).join(format!("{}.jsonl", hash('b')));
        fs::write(&version, b"tampered\n").unwrap();
        assert_eq!(
            store(&root, 'b', b"{}\n").unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(fs::read(&version).unwrap(), b"tampered\n");
        fs::remove_file(&version).unwrap();
        let metadata = root.join(hash('a')).join("origin.json");
        fs::write(&version, b"{}\n").unwrap();
        fs::write(&metadata, b"altered").unwrap();
        assert_eq!(
            store(&root, 'b', b"{}\n").unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(fs::read(&metadata).unwrap(), b"altered");
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn bounded_status_counts_versions_without_exposing_session_content() {
        let fixture = fixture();
        let root = fixture.join("archive");
        assert_eq!(
            summarize_private_versions(&root).unwrap(),
            ArchiveSummary::default()
        );
        assert_eq!(store(&root, 'b', b"{}\n").unwrap(), ArchiveResult::Created);
        assert_eq!(
            store(&root, 'c', b"{\"turn\":2}\n").unwrap(),
            ArchiveResult::Created
        );
        let counts = summarize_private_versions(&root).unwrap();
        assert_eq!(counts.sessions, 1);
        assert_eq!(counts.versions, 2);
        assert!(!counts.truncated);
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn no_credentials_or_partial_lines_enter_archives() {
        let fixture = fixture();
        let root = fixture.join("archive");
        assert!(store(&root, 'b', b"unfinished").is_err());
        assert!(preserve_version(&root, "../account", &hash('b'), b"{}\n", b"{}").is_err());
        assert!(preserve_version(&root, &hash('a'), &hash('b'), b"{}\n", b"").is_err());
        assert!(
            !root.exists(),
            "validation must fail before creating archive directories"
        );
        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn version_cap_preserves_previous_versions_and_never_deletes_them() {
        let fixture = fixture();
        let root = fixture.join("archive");
        for i in 0..MAX_ARCHIVE_VERSIONS {
            let version = format!("{i:064x}");
            assert_eq!(
                preserve_version(&root, &hash('a'), &version, b"{}\n", b"{}").unwrap(),
                ArchiveResult::Created
            );
        }
        assert_eq!(
            preserve_version(&root, &hash('a'), &hash('f'), b"{}\n", b"{}").unwrap(),
            ArchiveResult::CapacityReached
        );
        assert_eq!(
            fs::read_dir(root.join(hash('a'))).unwrap().count(),
            MAX_ARCHIVE_VERSIONS + 1
        );
        fs::remove_dir_all(fixture).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_private_archive_root() {
        use std::os::unix::fs::symlink;
        let fixture = fixture();
        fs::create_dir(fixture.join("real")).unwrap();
        symlink(fixture.join("real"), fixture.join("linked")).unwrap();
        assert!(store(&fixture.join("linked"), 'b', b"{}\n").is_err());
        fs::remove_dir_all(fixture).unwrap();
    }
}
