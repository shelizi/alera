//! Short-lived, watcher-backed text cache for read-only workspace searches.
//! Discovery, ignore rules and path validation still run for every query.
//! Never use cached bytes to preview or execute a replacement.
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use super::literal_index::IndexedText;

use notify::{
    event::{AccessKind, AccessMode},
    EventKind, RecursiveMode, Watcher,
};

const MAX_ROOTS: usize = 2;
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 8192;
pub(super) const MAX_CACHED_FILE_BYTES: u64 = 512 * 1024;
const MAX_AGE: Duration = Duration::from_secs(15);
const IDLE_ROOT_AGE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileStamp {
    len: u64,
    modified: SystemTime,
    #[cfg(windows)]
    created: u64,
    #[cfg(windows)]
    attributes: u32,
    #[cfg(windows)]
    change_time: i64,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl FileStamp {
    pub(super) fn from_metadata(metadata: &fs::Metadata) -> Option<Self> {
        if !metadata.is_file() {
            return None;
        }
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        #[cfg(windows)]
        use std::os::windows::fs::MetadataExt;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
            #[cfg(windows)]
            created: metadata.creation_time(),
            #[cfg(windows)]
            attributes: metadata.file_attributes(),
            #[cfg(windows)]
            change_time: 0,
            #[cfg(unix)]
            identity: (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        })
    }

    pub(super) fn from_file(file: &fs::File, metadata: &fs::Metadata) -> Option<Self> {
        let stamp = Self::from_metadata(metadata)?;
        #[cfg(windows)]
        {
            use std::mem::{size_of, MaybeUninit};
            use std::os::windows::fs::MetadataExt;
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                FileBasicInfo, GetFileInformationByHandleEx, FILE_BASIC_INFO,
            };
            let mut info = MaybeUninit::<FILE_BASIC_INFO>::uninit();
            // SAFETY: live file handle and correctly sized writable output.
            let ok = unsafe {
                GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    FileBasicInfo,
                    info.as_mut_ptr().cast(),
                    size_of::<FILE_BASIC_INFO>() as u32,
                )
            };
            if ok == 0 {
                return None;
            }
            let info = unsafe { info.assume_init() };
            // NTFS ChangeTime changes even when the user restores LastWriteTime.
            // Do not cache on filesystems lacking this stamp or if metadata
            // changed between the two handle queries.
            if info.ChangeTime <= 0
                || info.LastWriteTime as u64 != metadata.last_write_time()
                || info.CreationTime as u64 != metadata.creation_time()
                || info.FileAttributes != metadata.file_attributes()
            {
                return None;
            }
            Some(Self {
                change_time: info.ChangeTime,
                ..stamp
            })
        }
        #[cfg(not(windows))]
        {
            let _ = file;
            Some(stamp)
        }
    }
}

// None is a successfully read but non-searchable (binary/non-UTF-8) file.
pub(super) type Text = Option<Arc<IndexedText>>;

struct Entry {
    stamp: FileStamp,
    text: Text,
    read_at: Instant,
    charge: usize,
    order: u64,
}

struct State {
    healthy: bool,
    generation: u64,
    clock: u64,
    bytes: usize,
    entries: HashMap<PathBuf, Entry>,
    lru: BTreeMap<u64, PathBuf>,
    max_bytes: usize,
    max_entries: usize,
    #[cfg(test)]
    hits: usize,
}

impl State {
    fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            healthy: true,
            generation: 0,
            clock: 0,
            bytes: 0,
            entries: HashMap::new(),
            lru: BTreeMap::new(),
            max_bytes,
            max_entries,
            #[cfg(test)]
            hits: 0,
        }
    }

    fn remove(&mut self, path: &Path) -> Option<Entry> {
        let entry = self.entries.remove(path)?;
        self.lru.remove(&entry.order);
        self.bytes -= entry.charge;
        Some(entry)
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
        self.bytes = 0;
    }

    fn invalidate_all(&mut self, disable: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.clear();
        if disable {
            self.healthy = false;
        }
    }

    fn lookup(&mut self, path: &Path, stamp: &FileStamp) -> Lookup {
        if !self.healthy {
            return Lookup::Disabled;
        }
        if let Some(entry) = self.entries.get_mut(path) {
            if &entry.stamp == stamp && entry.read_at.elapsed() < MAX_AGE {
                let text = entry.text.clone();
                self.clock = self.clock.wrapping_add(1);
                // Reuse the existing LRU path allocation on hits instead of
                // removing and reinserting the entire cached entry.
                let key = self
                    .lru
                    .remove(&entry.order)
                    .unwrap_or_else(|| path.to_path_buf());
                entry.order = self.clock;
                self.lru.insert(entry.order, key);
                #[cfg(test)]
                {
                    self.hits += 1;
                }
                return Lookup::Hit(text);
            }
            self.remove(path);
        }
        Lookup::Miss(self.generation)
    }

    fn insert(&mut self, path: PathBuf, stamp: FileStamp, text: Text, generation: u64) {
        if !self.healthy || generation != self.generation {
            return;
        }
        // Charge for both owned keys, entry/map overhead and retained text.
        let charge = text
            .as_ref()
            .map_or(0, |text| text.charged_capacity())
            .saturating_add(path.as_os_str().len().saturating_mul(2))
            .saturating_add(256);
        if charge > self.max_bytes || self.max_entries == 0 {
            return;
        }
        self.remove(&path);
        while self.bytes.saturating_add(charge) > self.max_bytes
            || self.entries.len() >= self.max_entries
        {
            let Some((_, oldest)) = self.lru.first_key_value() else {
                return;
            };
            let oldest = oldest.clone();
            self.remove(&oldest);
        }
        self.clock = self.clock.wrapping_add(1);
        self.bytes += charge;
        self.lru.insert(self.clock, path.clone());
        self.entries.insert(
            path,
            Entry {
                stamp,
                text,
                read_at: Instant::now(),
                charge,
                order: self.clock,
            },
        );
    }

    fn invalidate_path(&mut self, path: &Path) {
        self.generation = self.generation.wrapping_add(1);
        // A write to one existing file is the common case. Directory and
        // rename events must also remove cached descendants. On Windows,
        // a differently cased event is conservatively treated as an unknown
        // path rather than silently retaining a stale alias.
        if self.remove(path).is_some() {
            return;
        }
        let affected = self
            .entries
            .keys()
            .filter(|key| key.starts_with(path))
            .cloned()
            .collect::<Vec<_>>();
        if affected.is_empty() {
            #[cfg(windows)]
            self.clear();
        } else {
            for key in affected {
                self.remove(&key);
            }
        }
    }

    fn on_event(&mut self, root: &Path, result: notify::Result<notify::Event>) {
        let Ok(event) = result else {
            self.invalidate_all(true);
            return;
        };
        if event.need_rescan() {
            self.invalidate_all(true);
            return;
        }
        // Reading the file ourselves must not invalidate the cache.
        if matches!(event.kind, EventKind::Access(kind)
            if kind != AccessKind::Close(AccessMode::Write))
        {
            return;
        }
        if event.paths.is_empty() || matches!(event.kind, EventKind::Any | EventKind::Other) {
            self.invalidate_all(false);
            return;
        }
        for path in event.paths {
            let absolute = if path.is_absolute() {
                path
            } else {
                root.join(path)
            };
            let Ok(relative) = absolute.strip_prefix(root) else {
                self.invalidate_all(false);
                return;
            };
            if relative.as_os_str().is_empty() {
                // A replaced/renamed root may no longer have a valid watch.
                // Ordinary root metadata events merely require reconciliation.
                let lost_root = matches!(
                    event.kind,
                    EventKind::Remove(_) | EventKind::Modify(notify::event::ModifyKind::Name(_))
                );
                self.invalidate_all(lost_root);
                return;
            }
            if relative
                .components()
                .any(|part| matches!(part.as_os_str().to_str(), Some(".git" | ".hg" | ".svn")))
            {
                // Git checkout/index changes may restore file timestamps.
                self.invalidate_all(false);
                return;
            }
            self.invalidate_path(relative);
        }
    }
}

pub(super) enum Lookup {
    Hit(Text),
    Miss(u64),
    Disabled,
}

pub(super) struct RootCache {
    state: Arc<Mutex<State>>,
    // Holding the watcher keeps the recursive subscription alive; its
    // callback uses a weak state reference, avoiding an ownership cycle.
    _watcher: Mutex<notify::RecommendedWatcher>,
}

impl RootCache {
    pub(super) fn new(root: &Path) -> Option<Self> {
        let state = Arc::new(Mutex::new(State::new(MAX_CACHE_BYTES, MAX_CACHE_ENTRIES)));
        let weak = Arc::downgrade(&state);
        let watched_root = root.to_path_buf();
        let mut watcher = notify::recommended_watcher(move |event| {
            if let Some(state) = weak.upgrade() {
                if let Ok(mut state) = state.lock() {
                    state.on_event(&watched_root, event);
                }
            }
        })
        .ok()?;
        // A nonrecursive fallback cannot prove freshness for unopened dirs.
        watcher.watch(root, RecursiveMode::Recursive).ok()?;
        Some(Self {
            state,
            _watcher: Mutex::new(watcher),
        })
    }

    fn healthy(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.healthy)
    }

    pub(super) fn lookup(&self, path: &Path, stamp: &FileStamp) -> Lookup {
        self.state
            .lock()
            .map_or(Lookup::Disabled, |mut state| state.lookup(path, stamp))
    }

    pub(super) fn insert(&self, path: PathBuf, stamp: FileStamp, text: Text, generation: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.insert(path, stamp, text, generation);
        }
    }

    #[cfg(test)]
    pub(super) fn stats(&self) -> (usize, usize, usize) {
        let state = self.state.lock().unwrap();
        (state.hits, state.entries.len(), state.bytes)
    }

    #[cfg(test)]
    pub(super) fn index_stats(&self) -> (usize, usize, usize) {
        let state = self.state.lock().unwrap();
        state
            .entries
            .values()
            .filter_map(|entry| entry.text.as_ref())
            .map(|text| text.index_stats())
            .fold((0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2))
    }
}

struct RootEntry {
    cache: Arc<RootCache>,
    used_at: Instant,
}
static ROOTS: OnceLock<Mutex<HashMap<PathBuf, RootEntry>>> = OnceLock::new();

pub(super) fn for_root(root: &Path) -> Option<Arc<RootCache>> {
    let mut roots = ROOTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()?;
    roots.retain(|_, entry| {
        Arc::strong_count(&entry.cache) > 1
            || (entry.used_at.elapsed() < IDLE_ROOT_AGE && entry.cache.healthy())
    });
    if let Some(entry) = roots.get_mut(root) {
        if !entry.cache.healthy() {
            return None;
        }
        entry.used_at = Instant::now();
        return Some(entry.cache.clone());
    }
    if roots.len() >= MAX_ROOTS {
        let oldest = roots
            .iter()
            .filter(|(_, entry)| Arc::strong_count(&entry.cache) == 1)
            .min_by_key(|(_, entry)| entry.used_at)
            .map(|(root, _)| root.clone())?;
        roots.remove(&oldest);
    }
    let cache = Arc::new(RootCache::new(root)?);
    roots.insert(
        root.to_path_buf(),
        RootEntry {
            cache: cache.clone(),
            used_at: Instant::now(),
        },
    );
    Some(cache)
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{DataChange, ModifyKind, RenameMode};

    fn stamp() -> FileStamp {
        let file = tempfile::NamedTempFile::new().unwrap();
        FileStamp::from_file(file.as_file(), &file.as_file().metadata().unwrap()).unwrap()
    }

    #[test]
    fn lru_budget_expiry_and_metadata_changes_are_enforced() {
        let mut state = State::new(4096, 2);
        let stamp = stamp();
        for path in ["a", "b"] {
            state.insert(
                path.into(),
                stamp.clone(),
                Some(Arc::new(IndexedText::new("text".into()))),
                0,
            );
        }
        assert!(matches!(
            state.lookup(Path::new("a"), &stamp),
            Lookup::Hit(_)
        ));
        state.insert("c".into(), stamp.clone(), None, 0);
        assert!(!state.entries.contains_key(Path::new("b")));
        assert!(matches!(
            state.lookup(Path::new("c"), &stamp),
            Lookup::Hit(None)
        ));
        assert!(state.bytes <= 4096);
        let mut changed = stamp.clone();
        changed.len += 1;
        assert!(matches!(
            state.lookup(Path::new("a"), &changed),
            Lookup::Miss(_)
        ));
        state.entries.get_mut(Path::new("c")).unwrap().read_at =
            Instant::now().checked_sub(MAX_AGE).unwrap();
        assert!(matches!(
            state.lookup(Path::new("c"), &stamp),
            Lookup::Miss(_)
        ));
        assert_eq!(state.bytes, 0);
    }

    #[test]
    fn watch_events_invalidate_files_directories_and_inflight_reads() {
        let root = Path::new("root");
        let mut state = State::new(8192, 10);
        let stamp = stamp();
        for path in ["dir/a", "dir/b", "keep"] {
            state.insert(
                path.into(),
                stamp.clone(),
                Some(Arc::new(IndexedText::new("old".into()))),
                0,
            );
        }
        state.on_event(
            root,
            Ok(
                notify::Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Any)))
                    .add_path("dir/a".into()),
            ),
        );
        assert!(!state.entries.contains_key(Path::new("dir/a")));
        assert!(state.entries.contains_key(Path::new("dir/b")));
        state.insert(
            "dir/a".into(),
            stamp.clone(),
            Some(Arc::new(IndexedText::new("stale".into()))),
            0,
        );
        assert!(!state.entries.contains_key(Path::new("dir/a")));
        state.on_event(
            root,
            Ok(
                notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
                    .add_path("dir".into()),
            ),
        );
        assert!(!state.entries.contains_key(Path::new("dir/b")));
        assert!(state.entries.contains_key(Path::new("keep")));
        state.on_event(root, Err(notify::Error::generic("overflow")));
        assert!(matches!(
            state.lookup(Path::new("keep"), &stamp),
            Lookup::Disabled
        ));
        assert_eq!(state.bytes, 0);
    }
}
