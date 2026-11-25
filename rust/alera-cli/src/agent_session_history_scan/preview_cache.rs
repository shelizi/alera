//! Bounded, in-memory cache of consented previews. Never used by exact resume.
use std::collections::HashMap;
use std::fs::{File, Metadata};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use super::{read_provider_metadata_from_file, ProviderHistoryRoot};

const MAX_AGE: Duration = Duration::from_secs(60);

type ProviderPreview = (String, PathBuf, Option<String>);

/// Resolve the normalized DOS path on the very handle that will be read.
/// Windows std::fs::canonicalize otherwise opens a separate metadata handle.
/// Keep the verbatim spelling used by the scanner's canonical root; never
/// infer the target path from the directory entry or strip a UNC prefix.
#[cfg(windows)]
pub(super) fn open_canonical_preview_file(path: &Path) -> Option<(PathBuf, File)> {
    let file = File::open(path).ok()?;
    let canonical = canonical_path_from_handle(&file)?;
    Some((canonical, file))
}

#[cfg(windows)]
fn canonical_path_from_handle(file: &File) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, VOLUME_NAME_DOS};

    let handle = HANDLE(file.as_raw_handle());
    // VOLUME_NAME_DOS also selects the default FILE_NAME_NORMALIZED (0).
    let flags = VOLUME_NAME_DOS;
    let mut stack = [0u16; 512];
    // The handle remains open for both path resolution and content validation.
    let len = unsafe { GetFinalPathNameByHandleW(handle, &mut stack, flags) } as usize;
    if len == 0 {
        return None;
    }
    let canonical = if len < stack.len() {
        PathBuf::from(OsString::from_wide(&stack[..len]))
    } else {
        // Windows reports the required size including the terminator when the
        // buffer is insufficient. Bound the retry and reject a changing path.
        if len > 32_768 {
            return None;
        }
        let mut buffer = vec![0u16; len];
        let actual = unsafe { GetFinalPathNameByHandleW(handle, &mut buffer, flags) } as usize;
        if actual == 0 || actual >= buffer.len() {
            return None;
        }
        PathBuf::from(OsString::from_wide(&buffer[..actual]))
    };
    Some(canonical)
}

#[cfg(not(windows))]
pub(super) fn open_canonical_preview_file(_path: &Path) -> Option<(PathBuf, File)> {
    None
}

// Keep the snapshot tied to the handle it describes. Only the ordinary-file
// fast path supplies a prechecked stamp; compatibility opens read it normally.
pub(super) struct OpenedPreviewFile {
    file: File,
    stamp: Option<FileStamp>,
}

impl OpenedPreviewFile {
    pub(super) fn from_file(file: File) -> Self {
        Self { file, stamp: None }
    }

    /// The initial no-follow snapshot established an ordinary-file handle.
    /// Revalidate that same handle using the matching bounded information
    /// classes after parsing. Compatibility handles retain the old reader.
    #[cfg(windows)]
    fn stamp_after_parse(file: &File, regular: bool) -> Option<FileStamp> {
        if regular {
            read_regular_snapshot(file).map(|(_, stamp)| stamp)
        } else {
            FileStamp::read(file)
        }
    }

    #[cfg(not(windows))]
    fn stamp_after_parse(file: &File, _regular: bool) -> Option<FileStamp> {
        FileStamp::read(file)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct CandidateMetadata {
    pub(super) len: u64,
    pub(super) updated_at_ms: u64,
}

impl CandidateMetadata {
    pub(super) fn from_metadata(meta: &Metadata) -> Self {
        Self {
            len: meta.len(),
            updated_at_ms: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
                .unwrap_or(0),
        }
    }
}

#[cfg(windows)]
fn ordinary_windows_attributes(attributes: u32) -> bool {
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };
    attributes & (FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_REPARSE_POINT.0) == 0
}

/// Read only the required fields. GetFileInformationByHandle (used by
/// std::fs::File::metadata) also retrieves volume/link/index information
/// that this preview path does not need; FileIdInfo supplies the full ID.
#[cfg(windows)]
fn read_regular_snapshot(file: &File) -> Option<(CandidateMetadata, FileStamp)> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FileBasicInfo, FileIdInfo, FileStandardInfo, GetFileInformationByHandleEx, FILE_BASIC_INFO,
        FILE_ID_INFO, FILE_STANDARD_INFO,
    };

    let handle = HANDLE(file.as_raw_handle());
    let mut standard = FILE_STANDARD_INFO::default();
    let mut basic = FILE_BASIC_INFO::default();
    let mut id = FILE_ID_INFO::default();
    // Each information class has the matching initialized output structure.
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileStandardInfo,
            (&mut standard as *mut FILE_STANDARD_INFO).cast(),
            std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
        )
        .ok()?;
        GetFileInformationByHandleEx(
            handle,
            FileBasicInfo,
            (&mut basic as *mut FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
        .ok()?;
        if standard.Directory
            || standard.DeletePending
            || !ordinary_windows_attributes(basic.FileAttributes)
        {
            return None;
        }
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
        .ok()?;
    }
    let len = u64::try_from(standard.EndOfFile).ok()?;
    // FILETIME is 100 ns ticks since 1601. Preserve the scanner's
    // pre-Unix-epoch clamp and millisecond truncation. Unsupported signed
    // values fall back to the std metadata conversion.
    const UNIX_EPOCH_FILETIME: u64 = 116_444_736_000_000_000;
    let updated_at_ms = u64::try_from(basic.LastWriteTime)
        .ok()?
        .saturating_sub(UNIX_EPOCH_FILETIME)
        / 10_000;
    let stamp = FileStamp::from_windows_parts(len, &basic, &id)?;
    Some((CandidateMetadata { len, updated_at_ms }, stamp))
}

/// Avoid the separate path-based metadata open for ordinary files. Opening
/// the final component without following reparse points is essential: directory
/// enumeration may be stale. Special files, unavailable stamps and errors
/// fall back to the original path-based checks in the scanner.
#[cfg(windows)]
pub(super) fn open_regular_preview_candidate(
    path: &Path,
) -> Option<(PathBuf, CandidateMetadata, OpenedPreviewFile)> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

    let file = File::options()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .ok()?;
    let (meta, stamp) = read_regular_snapshot(&file)?;
    let canonical = canonical_path_from_handle(&file)?;
    Some((
        canonical,
        meta,
        OpenedPreviewFile {
            file,
            stamp: Some(stamp),
        },
    ))
}

#[cfg(not(windows))]
pub(super) fn open_regular_preview_candidate(
    _path: &Path,
) -> Option<(PathBuf, CandidateMetadata, OpenedPreviewFile)> {
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileStamp {
    len: u64,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64, i64, i64),
    #[cfg(windows)]
    identity: (u64, [u8; 16], i64, i64, i64),
}

impl FileStamp {
    /// Stable identity only: the private SQLite index may change contents between searches.
    pub(crate) fn same_file_as(&self, other: &Self) -> bool {
        #[cfg(unix)]
        {
            self.identity.0 == other.identity.0 && self.identity.1 == other.identity.1
        }
        #[cfg(windows)]
        {
            self.identity.0 == other.identity.0
                && self.identity.1 == other.identity.1
                && self.identity.2 == other.identity.2
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = other;
            false
        }
    }
    #[cfg(unix)]
    pub(crate) fn read(file: &File) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata().ok()?;
        if !meta.is_file() || meta.ino() == 0 {
            return None;
        }
        Some(Self {
            len: meta.len(),
            identity: (
                meta.dev(),
                meta.ino(),
                meta.mtime(),
                meta.mtime_nsec(),
                meta.ctime(),
                meta.ctime_nsec(),
            ),
        })
    }

    #[cfg(windows)]
    pub(crate) fn read(file: &File) -> Option<Self> {
        let meta = file.metadata().ok()?;
        Self::read_with_metadata(file, &meta)
    }

    #[cfg(windows)]
    fn read_with_metadata(file: &File, meta: &Metadata) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            FileBasicInfo, FileIdInfo, GetFileInformationByHandleEx, FILE_BASIC_INFO, FILE_ID_INFO,
        };

        if !meta.is_file() {
            return None;
        }
        let handle = HANDLE(file.as_raw_handle());
        let mut basic = FILE_BASIC_INFO::default();
        let mut id = FILE_ID_INFO::default();
        // Both calls use the same open read handle and initialized output buffers.
        // Unsupported file systems fail closed to a fresh parse, rather than
        // trusting only the easily preserved last-write timestamp.
        unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileBasicInfo,
                (&mut basic as *mut FILE_BASIC_INFO).cast(),
                std::mem::size_of::<FILE_BASIC_INFO>() as u32,
            )
            .ok()?;
            GetFileInformationByHandleEx(
                handle,
                FileIdInfo,
                (&mut id as *mut FILE_ID_INFO).cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
            .ok()?;
        }
        Self::from_windows_parts(meta.len(), &basic, &id)
    }

    #[cfg(windows)]
    fn from_windows_parts(
        len: u64,
        basic: &windows::Win32::Storage::FileSystem::FILE_BASIC_INFO,
        id: &windows::Win32::Storage::FileSystem::FILE_ID_INFO,
    ) -> Option<Self> {
        if basic.ChangeTime == 0 || id.FileId.Identifier == [0; 16] {
            return None;
        }
        Some(Self {
            len,
            identity: (
                id.VolumeSerialNumber,
                id.FileId.Identifier,
                basic.CreationTime,
                basic.LastWriteTime,
                basic.ChangeTime,
            ),
        })
    }

    #[cfg(not(any(unix, windows)))]
    pub(crate) fn read(_file: &File) -> Option<Self> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreviewKey {
    provider: &'static str,
    account: String,
    path: PathBuf,
}

struct CachedPreview {
    stamp: FileStamp,
    parsed_at: Instant,
    value: ProviderPreview,
}

#[derive(Default)]
pub(crate) struct HistoryPreviewCache {
    entries: HashMap<PreviewKey, CachedPreview>,
}

pub(super) struct PreviewScan {
    previous: HistoryPreviewCache,
    next: HistoryPreviewCache,
}

pub(super) struct PreviewRead {
    key: PreviewKey,
    stamp: Option<FileStamp>,
    parsed_at: Instant,
    pub(super) value: ProviderPreview,
    pub(super) reused: bool,
}

impl PreviewScan {
    pub(super) fn new(previous: HistoryPreviewCache) -> Self {
        Self {
            previous,
            next: HistoryPreviewCache::default(),
        }
    }

    pub(super) fn finish(self) -> HistoryPreviewCache {
        self.next
    }

    pub(super) fn read(
        &mut self,
        root: &ProviderHistoryRoot,
        path: &Path,
        opened: Option<OpenedPreviewFile>,
    ) -> Option<PreviewRead> {
        // Every search opens a fresh readable handle, even on a cache hit.
        let OpenedPreviewFile { file, stamp } = match opened {
            Some(opened) => opened,
            None => OpenedPreviewFile::from_file(File::open(path).ok()?),
        };
        let key = PreviewKey {
            provider: root.provider,
            account: root.account_id.clone(),
            path: path.to_owned(),
        };
        let regular = stamp.is_some();
        let stamp = stamp.or_else(|| FileStamp::read(&file));
        let now = Instant::now();
        if let Some(old) = self.previous.entries.remove(&key) {
            if stamp.as_ref() == Some(&old.stamp)
                && now.saturating_duration_since(old.parsed_at) <= MAX_AGE
            {
                return Some(PreviewRead {
                    key,
                    stamp,
                    parsed_at: old.parsed_at,
                    value: old.value,
                    reused: true,
                });
            }
        }
        let value = read_provider_metadata_from_file(root.provider, path, true, &file)?;
        // Never retain a preview if the source changed during parsing.
        let stamp = stamp.filter(|before| {
            OpenedPreviewFile::stamp_after_parse(&file, regular).as_ref() == Some(before)
        });
        Some(PreviewRead {
            key,
            stamp,
            parsed_at: now,
            value,
            reused: false,
        })
    }

    /// Only cache after the scanner has revalidated the registered CWD.
    /// Move the existing allocations into the next generation. The response
    /// needs only its own session ID and preview, not a second copy of the CWD.
    pub(super) fn remember(&mut self, read: PreviewRead) -> (String, Option<String>) {
        let PreviewRead {
            key,
            stamp,
            parsed_at,
            value,
            ..
        } = read;
        if let Some(stamp) = stamp {
            let output = (value.0.clone(), value.2.clone());
            self.next.entries.insert(
                key,
                CachedPreview {
                    stamp,
                    parsed_at,
                    value,
                },
            );
            output
        } else {
            let (native_id, _, preview) = value;
            (native_id, preview)
        }
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn ordinary_candidate_reuses_handle_metadata_and_rejects_reparse_points() {
        use std::os::windows::fs::symlink_file;

        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("source.jsonl");
        std::fs::write(&path, b"history").unwrap();
        let old_meta = std::fs::symlink_metadata(&path).unwrap();
        let (canonical, meta, opened) = open_regular_preview_candidate(&path).unwrap();
        assert_eq!(canonical, std::fs::canonicalize(&path).unwrap());
        assert_eq!(meta, CandidateMetadata::from_metadata(&old_meta));
        assert_eq!(opened.stamp, FileStamp::read(&opened.file));
        assert!(open_regular_preview_candidate(home.path()).is_none());
        assert!(open_regular_preview_candidate(&home.path().join("missing")).is_none());

        // Creating symlinks requires Developer Mode or elevated privileges
        // on some Windows hosts; exercise actual reparse rejection when available.
        let link = home.path().join("link.jsonl");
        match symlink_file(&path, &link) {
            Ok(()) => {
                use std::os::windows::fs::MetadataExt;
                assert!(!ordinary_windows_attributes(
                    std::fs::symlink_metadata(&link).unwrap().file_attributes()
                ));
                assert!(open_regular_preview_candidate(&link).is_none());
                println!("Windows symlink reparse rejection exercised");
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    || error.raw_os_error() == Some(1314) =>
            {
                println!("Windows symlink fixture unavailable: {error}");
            }
            Err(error) => panic!("unexpected symlink fixture error: {error}"),
        }
    }

    #[cfg(windows)]
    #[test]
    fn direct_snapshot_matches_legacy_eof_and_filetime() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("source.jsonl");
        let file = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        // EOF must not be confused with the filesystem's allocation size.
        file.set_len(8193).unwrap();
        for modified in [
            UNIX_EPOCH - Duration::from_secs(86_400),
            UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_789),
            UNIX_EPOCH + Duration::new(1_700_000_001, 999_999_900),
        ] {
            file.set_modified(modified).unwrap();
            let (meta, stamp) = read_regular_snapshot(&file).unwrap();
            assert_eq!(meta.len, 8193);
            assert_eq!(
                meta,
                CandidateMetadata::from_metadata(&file.metadata().unwrap())
            );
            assert_eq!(Some(stamp), FileStamp::read(&file));
        }
    }

    #[cfg(windows)]
    #[test]
    fn post_parse_snapshot_detects_same_length_rewrite_and_preserves_compatibility() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("source.jsonl");
        std::fs::write(&path, b"history").unwrap();
        let (_, _, opened) = open_regular_preview_candidate(&path).unwrap();
        let before = opened.stamp.as_ref().unwrap();
        assert_eq!(
            OpenedPreviewFile::stamp_after_parse(&opened.file, true).as_ref(),
            Some(before)
        );
        assert_eq!(
            OpenedPreviewFile::stamp_after_parse(&opened.file, false).as_ref(),
            Some(before)
        );

        // Restore mtime after rewriting the same number of bytes. The full
        // change-time/identity check, not just length and mtime, must reject
        // the original parsed snapshot on this already-open file handle.
        let modified = opened.file.metadata().unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, b"changed").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        let after = OpenedPreviewFile::stamp_after_parse(&opened.file, true).unwrap();
        assert_ne!(&after, before);
        assert!(after.same_file_as(before));
        assert_eq!(
            Some(after),
            OpenedPreviewFile::stamp_after_parse(&opened.file, false)
        );
    }

    #[test]
    fn accepted_preview_moves_allocations_into_cache_and_uncacheable_output() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("history.jsonl");
        std::fs::write(&path, b"history").unwrap();
        let file = File::open(&path).unwrap();
        let stamp = FileStamp::read(&file).unwrap();
        let key = PreviewKey {
            provider: "codex",
            account: "default".to_owned(),
            path,
        };
        let value = (
            "550e8400-e29b-41d4-a716-446655440000".to_owned(),
            home.path().to_owned(),
            Some("first user message".to_owned()),
        );
        let mut scan = PreviewScan::new(HistoryPreviewCache::default());
        let key_ptr = key.account.as_ptr();
        let id_ptr = value.0.as_ptr();
        let preview_ptr = value.2.as_ref().unwrap().as_ptr();
        let expected = (value.0.clone(), value.2.clone());
        let output = scan.remember(PreviewRead {
            key,
            stamp: Some(stamp),
            parsed_at: Instant::now(),
            value,
            reused: false,
        });
        assert_eq!(output, expected);
        let next = scan.finish();
        let (key, cached) = next.entries.iter().next().unwrap();
        assert_eq!(key.account.as_ptr(), key_ptr);
        assert_eq!(cached.value.0.as_ptr(), id_ptr);
        assert_eq!(cached.value.2.as_ref().unwrap().as_ptr(), preview_ptr);
        assert_ne!(output.0.as_ptr(), id_ptr);
        assert_ne!(output.1.as_ref().unwrap().as_ptr(), preview_ptr);

        // When stamps are unsupported or change while parsing, no cache
        // entry is retained and the original output strings can be moved.
        let mut scan = PreviewScan::new(HistoryPreviewCache::default());
        let value = (output.0, home.path().to_owned(), output.1);
        let id_ptr = value.0.as_ptr();
        let preview_ptr = value.2.as_ref().unwrap().as_ptr();
        let output = scan.remember(PreviewRead {
            key: key.clone(),
            stamp: None,
            parsed_at: Instant::now(),
            value,
            reused: false,
        });
        assert_eq!(output.0.as_ptr(), id_ptr);
        assert_eq!(output.1.as_ref().unwrap().as_ptr(), preview_ptr);
        assert!(scan.finish().entries.is_empty());
    }
}
