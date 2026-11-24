//! Bounded, in-memory cache of consented previews. Never used by exact resume.
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{read_provider_metadata_from_file, ProviderHistoryRoot};

const MAX_AGE: Duration = Duration::from_secs(60);

type ProviderPreview = (String, PathBuf, Option<String>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64, i64, i64),
    #[cfg(windows)]
    identity: (u64, [u8; 16], i64, i64, i64),
}

impl FileStamp {
    #[cfg(unix)]
    fn read(file: &File) -> Option<Self> {
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
    fn read(file: &File) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            FileBasicInfo, FileIdInfo, GetFileInformationByHandleEx, FILE_BASIC_INFO, FILE_ID_INFO,
        };

        let meta = file.metadata().ok()?;
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
        if basic.ChangeTime == 0 || id.FileId.Identifier == [0; 16] {
            return None;
        }
        Some(Self {
            len: meta.len(),
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
    fn read(_file: &File) -> Option<Self> {
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

    pub(super) fn read(&mut self, root: &ProviderHistoryRoot, path: &Path) -> Option<PreviewRead> {
        // Reopen every candidate so missing or no-longer-readable sources never
        // become visible merely because we have an old preview in memory.
        let file = File::open(path).ok()?;
        let key = PreviewKey {
            provider: root.provider,
            account: root.account_id.clone(),
            path: path.to_owned(),
        };
        let stamp = FileStamp::read(&file);
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
        let stamp = stamp.filter(|before| FileStamp::read(&file).as_ref() == Some(before));
        Some(PreviewRead {
            key,
            stamp,
            parsed_at: now,
            value,
            reused: false,
        })
    }

    /// Only cache after the scanner has revalidated the registered CWD.
    pub(super) fn remember(&mut self, read: &PreviewRead) {
        if let Some(stamp) = &read.stamp {
            self.next.entries.insert(
                read.key.clone(),
                CachedPreview {
                    stamp: stamp.clone(),
                    parsed_at: read.parsed_at,
                    value: read.value.clone(),
                },
            );
        }
    }
}
