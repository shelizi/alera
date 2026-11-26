use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use notify::{RecursiveMode, Watcher};

use crate::frb_generated::StreamSink;

use super::{
    relative_string, resolve_existing, workspace_root, WorkspaceExplorerWatchBatch,
    WorkspaceExplorerWatcherHandle, WorkspaceFileError, WorkspaceFileErrorKind,
};

use super::watch_events::{watch_event_channel, WatchDelivery, WatchEventInbox, WatchStopper};

static NEXT_WATCHER_ID: AtomicU64 = AtomicU64::new(1);
static WATCHERS: OnceLock<Mutex<HashMap<String, WatcherSession>>> = OnceLock::new();

// The event worker takes a cheap immutable snapshot instead of cloning all
// loaded directories for every batch. Rebuild the reverse index only when
// the loaded-directory set changes.
type WatchedPaths = Arc<Mutex<Arc<WatchedDirectories>>>;

struct WatchedDirectories {
    by_relative: HashMap<String, PathBuf>,
    by_absolute: HashMap<PathBuf, Vec<String>>,
}

impl WatchedDirectories {
    fn new(by_relative: HashMap<String, PathBuf>) -> Self {
        let mut by_absolute: HashMap<PathBuf, Vec<String>> = HashMap::new();
        for (relative, absolute) in &by_relative {
            by_absolute
                .entry(absolute.clone())
                .or_default()
                .push(relative.clone());
        }
        Self {
            by_relative,
            by_absolute,
        }
    }

    fn affected(&self, path: &Path) -> Option<&[String]> {
        // Preserve the deepest loaded-directory match: the event path itself
        // or its immediate parent, never an arbitrary ancestor. Multiple
        // relative aliases of the same canonical directory all need refresh.
        self.by_absolute
            .get(path)
            .or_else(|| {
                path.parent()
                    .and_then(|parent| self.by_absolute.get(parent))
            })
            .map(Vec::as_slice)
    }
}

fn watched_paths_snapshot(paths: HashMap<String, PathBuf>) -> WatchedPaths {
    Arc::new(Mutex::new(Arc::new(WatchedDirectories::new(paths))))
}

const MAX_BATCH_PATHS: usize = 2048;

#[flutter_rust_bridge::frb(ignore)]
#[derive(Default)]
struct PendingWatchBatch {
    paths: HashSet<PathBuf>,
    immediate_paths: HashSet<PathBuf>,
    event_count: u32,
    collapsed: bool,
}

impl PendingWatchBatch {
    fn reconcile_all(&mut self, root: &Path) {
        self.paths.clear();
        self.immediate_paths.clear();
        self.paths.insert(root.to_path_buf());
        self.immediate_paths.insert(root.to_path_buf());
        self.collapsed = true;
    }

    fn push_delivery(&mut self, root: &Path, delivery: WatchDelivery) {
        match delivery {
            WatchDelivery::Event(event) => self.push(root, event),
            WatchDelivery::Reconcile(count) => {
                self.event_count = self.event_count.saturating_add(count);
                if !self.collapsed {
                    self.reconcile_all(root);
                }
            }
        }
    }

    fn push(&mut self, root: &Path, result: notify::Result<notify::Event>) {
        self.event_count = self.event_count.saturating_add(1);
        if self.collapsed {
            return;
        }
        match result {
            Ok(event) if !event.paths.is_empty() => {
                // Some backends attach paths to a rescan notification.
                // A nonempty path list must not suppress reconciliation.
                let rescan = event.need_rescan();
                if rescan {
                    self.paths.insert(root.to_path_buf());
                    self.immediate_paths.insert(root.to_path_buf());
                }
                // Windows reports FILE_ACTION_MODIFIED as Modify(Any);
                // it is only a candidate, never assumed to be content-only.
                let candidate = !rescan
                    && matches!(
                        event.kind,
                        notify::EventKind::Modify(
                            notify::event::ModifyKind::Data(_) | notify::event::ModifyKind::Any
                        )
                    );
                for path in event.paths {
                    let path = normalize_event_path(root, path);
                    if path.starts_with(root) {
                        if !candidate {
                            self.immediate_paths.insert(path.clone());
                        }
                        self.paths.insert(path);
                    } else {
                        // An unrecognized alias or out-of-root notification
                        // cannot be localized safely.
                        self.paths.insert(root.to_path_buf());
                        self.immediate_paths.insert(root.to_path_buf());
                    }
                    // Even a fast consumer must not accumulate an unbounded
                    // number of distinct paths during the 350 ms window.
                    if self.paths.len() > MAX_BATCH_PATHS {
                        self.reconcile_all(root);
                        return;
                    }
                }
            }
            // Empty-path events and backend errors must still request
            // authoritative root reconciliation, even in a duplicate burst.
            _ => {
                self.paths.insert(root.to_path_buf());
                self.immediate_paths.insert(root.to_path_buf());
            }
        }
    }
}

struct WatcherSession {
    root: PathBuf,
    watcher: notify::RecommendedWatcher,
    recursive_root: bool,
    quick_open_journal: Arc<alera_core::workspace_files::QuickOpenWatchJournal>,
    watched_paths: WatchedPaths,
    sink: Arc<Mutex<Option<StreamSink<WorkspaceExplorerWatchBatch>>>>,
    stop: WatchStopper,
    thread: Option<thread::JoinHandle<()>>,
}

impl WatcherSession {
    fn stop(mut self) {
        self.quick_open_journal.deactivate();
        self.stop.request_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WatcherSession {
    fn drop(&mut self) {
        self.quick_open_journal.deactivate();
    }
}

fn record_quick_open_event(
    journal: &alera_core::workspace_files::QuickOpenWatchJournal,
    result: &notify::Result<notify::Event>,
) {
    use notify::event::{AccessKind, AccessMode};
    let Ok(event) = result else {
        journal.invalidate();
        return;
    };
    if event.need_rescan()
        || matches!(
            event.kind,
            notify::EventKind::Any | notify::EventKind::Other
        )
    {
        journal.invalidate();
        return;
    }
    if matches!(event.kind, notify::EventKind::Access(kind) if kind != AccessKind::Close(AccessMode::Write))
    {
        return;
    }
    journal.record(&event.paths);
}

pub(super) fn start_workspace_explorer_watcher(
    workspace_path: String,
) -> Result<WorkspaceExplorerWatcherHandle, WorkspaceFileError> {
    let root = workspace_root(&workspace_path)?;
    let id = NEXT_WATCHER_ID.fetch_add(1, Ordering::Relaxed).to_string();
    let (event_tx, stop, inbox) = watch_event_channel();
    let watched_paths = watched_paths_snapshot(HashMap::from([(String::new(), root.clone())]));
    let sink = Arc::new(Mutex::new(None));
    let quick_open_journal = alera_core::workspace_files::QuickOpenWatchJournal::new(root.clone());
    let callback_journal = quick_open_journal.clone();
    let mut watcher = notify::recommended_watcher(move |result| {
        // Record before batching/transport so a concurrent index build
        // cannot consume the notification before its journal revision.
        record_quick_open_event(&callback_journal, &result);
        if !event_tx.send(result) {
            callback_journal.deactivate();
        }
    })
    .map_err(notify_error)?;
    // A recursive root watch keeps searches current even in collapsed folders.
    // Fall back to the loaded-directory watches when the OS watch limit is hit.
    let recursive_root = watcher.watch(&root, RecursiveMode::Recursive).is_ok();
    if !recursive_root {
        watcher
            .watch(&root, RecursiveMode::NonRecursive)
            .map_err(notify_error)?;
    }
    let thread_root = root.clone();
    let thread_watched_paths = watched_paths.clone();
    let thread_sink = sink.clone();
    let thread = thread::Builder::new()
        .name(format!("alera-explorer-watch-{id}"))
        .spawn(move || {
            event_loop(thread_root, thread_watched_paths, thread_sink, inbox);
        })
        .map_err(|error| WorkspaceFileError::from_io(error, "workspace explorer watcher"))?;

    if recursive_root {
        quick_open_journal.activate();
    }
    let session = WatcherSession {
        root,
        watcher,
        recursive_root,
        quick_open_journal,
        watched_paths,
        sink,
        stop,
        thread: Some(thread),
    };
    watchers()
        .lock()
        .map_err(|_| WorkspaceFileError::new(WorkspaceFileErrorKind::Io, "watcher lock poisoned"))?
        .insert(id.clone(), session);
    Ok(WorkspaceExplorerWatcherHandle { id })
}

pub(super) fn update_workspace_explorer_watcher(
    handle: WorkspaceExplorerWatcherHandle,
    watched_relative_paths: Vec<String>,
) -> Result<(), WorkspaceFileError> {
    let mut watchers = watchers().lock().map_err(|_| {
        WorkspaceFileError::new(WorkspaceFileErrorKind::Io, "watcher lock poisoned")
    })?;
    let session = watchers.get_mut(&handle.id).ok_or_else(|| {
        WorkspaceFileError::new(
            WorkspaceFileErrorKind::NotFound,
            "workspace explorer watcher",
        )
    })?;
    let mut next_paths = HashMap::from([(String::new(), session.root.clone())]);
    let mut seen = HashSet::new();
    for relative_path in watched_relative_paths {
        let relative_path = normalize_relative_path(&relative_path);
        if !seen.insert(relative_path.clone()) {
            continue;
        }
        match resolve_existing(&session.root, &relative_path) {
            Ok(path) => {
                let metadata = std::fs::symlink_metadata(&path)
                    .map_err(|error| WorkspaceFileError::from_io(error, &relative_path))?;
                if metadata.is_dir() {
                    next_paths.insert(relative_path, path);
                }
            }
            Err(error) if error.kind == WorkspaceFileErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    let current_paths = session
        .watched_paths
        .lock()
        .map_err(|_| WorkspaceFileError::new(WorkspaceFileErrorKind::Io, "watcher lock poisoned"))?
        .clone();

    if current_paths.by_relative == next_paths {
        return Ok(());
    }
    if !session.recursive_root {
        for (relative_path, absolute_path) in &current_paths.by_relative {
            if !relative_path.is_empty() && next_paths.get(relative_path) != Some(absolute_path) {
                let _ = session.watcher.unwatch(absolute_path);
            }
        }
        for (relative_path, absolute_path) in &next_paths {
            if !relative_path.is_empty()
                && current_paths.by_relative.get(relative_path) != Some(absolute_path)
            {
                session
                    .watcher
                    .watch(absolute_path, RecursiveMode::NonRecursive)
                    .map_err(notify_error)?;
            }
        }
    }
    *session.watched_paths.lock().map_err(|_| {
        WorkspaceFileError::new(WorkspaceFileErrorKind::Io, "watcher lock poisoned")
    })? = Arc::new(WatchedDirectories::new(next_paths));
    Ok(())
}

pub(super) fn watch_workspace_explorer_events(
    handle: WorkspaceExplorerWatcherHandle,
    sink: StreamSink<WorkspaceExplorerWatchBatch>,
) {
    let session_sink = {
        let Ok(watchers) = watchers().lock() else {
            return;
        };
        watchers.get(&handle.id).map(|session| session.sink.clone())
    };
    let Some(session_sink) = session_sink else {
        return;
    };
    {
        if let Ok(mut current_sink) = session_sink.lock() {
            *current_sink = Some(sink);
        };
    }
}

pub(super) fn stop_workspace_explorer_watcher(handle: WorkspaceExplorerWatcherHandle) {
    let session = watchers()
        .lock()
        .ok()
        .and_then(|mut watchers| watchers.remove(&handle.id));
    if let Some(session) = session {
        session.stop();
    }
}

fn event_loop(
    root: PathBuf,
    watched_paths: WatchedPaths,
    sink: Arc<Mutex<Option<StreamSink<WorkspaceExplorerWatchBatch>>>>,
    inbox: WatchEventInbox,
) {
    loop {
        let mut pending = PendingWatchBatch::default();
        if !inbox.collect_batch(|event| pending.push_delivery(&root, event)) {
            return;
        }
        if let Some(batch) = build_batch(&root, &watched_paths, pending) {
            if inbox.is_stopping() {
                return;
            }
            emit_batch(&sink, batch);
        }
    }
}

fn build_batch(
    root: &Path,
    watched_paths: &WatchedPaths,
    pending: PendingWatchBatch,
) -> Option<WorkspaceExplorerWatchBatch> {
    let watched_paths = watched_paths.lock().ok()?.clone();
    if watched_paths.by_relative.is_empty() {
        return None;
    }
    let mut directory_relative_paths = HashSet::<String>::new();
    let mut changed_relative_paths = HashSet::<String>::new();
    if pending.immediate_paths.contains(root) {
        // Unknown changes can be inside any loaded directory, not just
        // the root's immediate children. Include all loaded aliases.
        directory_relative_paths.extend(watched_paths.by_relative.keys().cloned());
    }
    let deferred = if pending.immediate_paths.contains(root) || pending.paths.len() > 64 {
        HashSet::new()
    } else {
        let candidates = pending
            .paths
            .difference(&pending.immediate_paths)
            .cloned()
            .collect::<Vec<_>>();
        alera_core::workspace_files::quick_open_verified_regular_paths(root, &candidates)
    };
    let mut deferred_quick_open_paths = Vec::new();
    for event_path in pending.paths {
        if let Ok(relative_path) = relative_string(root, &event_path) {
            if deferred.contains(&event_path) {
                deferred_quick_open_paths.push(relative_path.clone());
            }
            changed_relative_paths.insert(relative_path);
        } else {
            // Malformed relative paths cannot safely be localized.
            changed_relative_paths.insert(String::new());
            directory_relative_paths.insert(String::new());
        }
        if let Some(relative_paths) = watched_paths.affected(&event_path) {
            directory_relative_paths.extend(relative_paths.iter().cloned());
        }
    }
    if directory_relative_paths.is_empty() && changed_relative_paths.is_empty() {
        return None;
    }
    let mut directory_relative_paths = directory_relative_paths.into_iter().collect::<Vec<_>>();
    let mut changed_relative_paths = changed_relative_paths.into_iter().collect::<Vec<_>>();
    directory_relative_paths.sort();
    changed_relative_paths.sort();
    deferred_quick_open_paths.sort();
    deferred_quick_open_paths.dedup();
    Some(WorkspaceExplorerWatchBatch {
        directory_relative_paths,
        changed_relative_paths,
        deferred_quick_open_paths: Some(deferred_quick_open_paths),
        coalesced_event_count: pending.event_count,
    })
}

fn emit_batch(
    sink: &Arc<Mutex<Option<StreamSink<WorkspaceExplorerWatchBatch>>>>,
    batch: WorkspaceExplorerWatchBatch,
) {
    // Closed Flutter sinks must not poison the watcher thread: drop the sink
    // and keep the OS watch alive until the client re-subscribes or stops.
    let Ok(mut sink) = sink.lock() else {
        return;
    };
    let Some(current_sink) = sink.as_ref() else {
        return;
    };
    if current_sink.add(batch).is_err() {
        *sink = None;
    }
}

fn watchers() -> &'static Mutex<HashMap<String, WatcherSession>> {
    WATCHERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn notify_error(error: notify::Error) -> WorkspaceFileError {
    WorkspaceFileError::new(WorkspaceFileErrorKind::Io, error.to_string())
}

fn normalize_event_path(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn normalize_relative_path(relative_path: &str) -> String {
    relative_path.trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::{Event, EventKind};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn batch_from_events(
        root: &Path,
        watched: &WatchedPaths,
        events: Vec<Event>,
    ) -> Option<WorkspaceExplorerWatchBatch> {
        let mut pending = PendingWatchBatch::default();
        for event in events {
            pending.push(root, Ok(event));
        }
        build_batch(root, watched, pending)
    }

    #[test]
    fn modification_hints_require_live_index_and_preserve_structural_priority() {
        use alera_core::workspace_files::{
            start_workspace_quick_open_session, stop_workspace_quick_open_session,
        };
        let workspace = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(workspace.path()).unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "original").unwrap();
        std::fs::write(root.join(".gitignore"), "").unwrap();
        let watched = watched_paths_snapshot(HashMap::from([
            (String::new(), root.clone()),
            ("src".into(), root.join("src")),
        ]));
        let modify = |path: &str| {
            Event::new(EventKind::Modify(notify::event::ModifyKind::Any)).add_path(root.join(path))
        };
        let no_index = batch_from_events(&root, &watched, vec![modify("src/main.rs")]).unwrap();
        assert_eq!(no_index.deferred_quick_open_paths, Some(vec![]));

        let session =
            start_workspace_quick_open_session(root.to_string_lossy().into_owned(), vec![])
                .unwrap();
        std::fs::write(root.join("src/main.rs"), "changed content").unwrap();
        let batch = batch_from_events(
            &root,
            &watched,
            vec![modify("src/main.rs"), modify("src/main.rs")],
        )
        .unwrap();
        assert_eq!(batch.changed_relative_paths, ["src/main.rs"]);
        assert_eq!(batch.directory_relative_paths, ["src"]);
        assert_eq!(batch.coalesced_event_count, 2);
        assert_eq!(
            batch.deferred_quick_open_paths,
            Some(vec!["src/main.rs".into()])
        );

        std::fs::write(root.join("src/new.rs"), "new").unwrap();
        for path in ["src/new.rs", "src", ".gitignore", "src/missing.rs"] {
            let batch = batch_from_events(&root, &watched, vec![modify(path)]).unwrap();
            assert_eq!(batch.deferred_quick_open_paths, Some(vec![]), "{path}");
        }
        for kind in [
            EventKind::Create(notify::event::CreateKind::File),
            EventKind::Remove(notify::event::RemoveKind::File),
            EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::Any,
            )),
            EventKind::Modify(notify::event::ModifyKind::Metadata(
                notify::event::MetadataKind::Any,
            )),
            EventKind::Any,
        ] {
            let structural = Event::new(kind).add_path(root.join("src/main.rs"));
            let batch = batch_from_events(&root, &watched, vec![modify("src/main.rs"), structural])
                .unwrap();
            assert_eq!(batch.deferred_quick_open_paths, Some(vec![]));
        }
        let batch = batch_from_events(
            &root,
            &watched,
            vec![
                modify("src/main.rs"),
                modify("src/new.rs"),
                modify(".gitignore"),
            ],
        )
        .unwrap();
        assert_eq!(
            batch.changed_relative_paths,
            [".gitignore", "src/main.rs", "src/new.rs"]
        );
        assert_eq!(
            batch.deferred_quick_open_paths,
            Some(vec!["src/main.rs".into()])
        );
        let batch = batch_from_events(
            &root,
            &watched,
            vec![
                modify("src/main.rs"),
                modify("src/new.rs").set_flag(notify::event::Flag::Rescan),
            ],
        )
        .unwrap();
        assert_eq!(batch.deferred_quick_open_paths, Some(vec![]));
        assert!(batch.changed_relative_paths.contains(&String::new()));

        // Multiple scopes cannot reuse a hint from only one of the live
        // indexes. One excludes src and therefore cannot prove membership.
        let excluded = start_workspace_quick_open_session(
            root.to_string_lossy().into_owned(),
            vec!["src".into()],
        )
        .unwrap();
        let batch = batch_from_events(&root, &watched, vec![modify("src/main.rs")]).unwrap();
        assert_eq!(batch.deferred_quick_open_paths, Some(vec![]));
        stop_workspace_quick_open_session(excluded);
        stop_workspace_quick_open_session(session);
    }

    #[cfg(windows)]
    #[test]
    fn native_existing_file_write_produces_verified_modification_hint() {
        use alera_core::workspace_files::{
            start_workspace_quick_open_session, stop_workspace_quick_open_session,
        };
        let workspace = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(workspace.path()).unwrap();
        let file = root.join("ordinary.txt");
        std::fs::write(&file, "old").unwrap();
        let session =
            start_workspace_quick_open_session(root.to_string_lossy().into_owned(), vec![])
                .unwrap();
        let watched = watched_paths_snapshot(HashMap::from([(String::new(), root.clone())]));
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let _ = tx.send(event);
        })
        .unwrap();
        watcher.watch(&root, RecursiveMode::Recursive).unwrap();
        std::fs::write(&file, "new ordinary contents").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut observed = false;
        while Instant::now() < deadline {
            let Ok(result) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            else {
                break;
            };
            let Ok(event) = result else {
                continue;
            };
            let batch = batch_from_events(&root, &watched, vec![event]);
            if let Some(batch) = batch {
                if batch.deferred_quick_open_paths.as_deref() == Some(&["ordinary.txt".to_string()])
                {
                    observed = true;
                    assert_eq!(batch.changed_relative_paths, ["ordinary.txt"]);
                    break;
                }
            }
        }
        drop(watcher);
        stop_workspace_quick_open_session(session);
        assert!(
            observed,
            "native watcher did not deliver a verifiable ordinary file modification"
        );
    }

    #[test]
    fn native_file_creation_notifies_loaded_directory() {
        let workspace = tempfile::tempdir().expect("workspace");
        let root = workspace.path().to_path_buf();
        let watched = watched_paths_snapshot(HashMap::from([(String::new(), root.clone())]));
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if let Ok(event) = event {
                let _ = tx.send(event);
            }
        })
        .unwrap();
        watcher.watch(&root, RecursiveMode::Recursive).unwrap();
        std::fs::write(root.join("new_file.dart"), b"hello").unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut observed = false;
        while Instant::now() < deadline {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(event) => {
                    if let Some(batch) = batch_from_events(&root, &watched, vec![event]) {
                        if batch.directory_relative_paths.contains(&String::new())
                            && batch
                                .changed_relative_paths
                                .contains(&"new_file.dart".to_string())
                        {
                            observed = true;
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        assert!(observed, "native watch did not report a newly created file");
    }

    #[test]
    fn native_watch_overflow_requests_root_reconciliation() {
        let workspace = tempfile::tempdir().expect("workspace");
        let watched = watched_paths_snapshot(HashMap::from([(
            String::new(),
            workspace.path().to_path_buf(),
        )]));
        let error_fallback = Event::new(EventKind::Any).add_path(workspace.path().to_path_buf());
        let batch = batch_from_events(workspace.path(), &watched, vec![error_fallback])
            .expect("root refresh after watcher overflow");
        assert_eq!(batch.directory_relative_paths, vec![String::new()]);
    }
    #[test]
    fn rescan_with_concrete_paths_and_unrecognized_paths_reconcile_root() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let watched = watched_paths_snapshot(HashMap::from([(String::new(), root.to_path_buf())]));
        let event = Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(root.join("src/deep/a.rs"))
            .set_flag(notify::event::Flag::Rescan);
        let batch = batch_from_events(root, &watched, vec![event]).unwrap();
        assert_eq!(batch.directory_relative_paths, vec![String::new()]);
        assert_eq!(batch.changed_relative_paths, vec!["", "src/deep/a.rs"]);
        let outside = root.parent().unwrap().join("not-this-workspace");
        let event = Event::new(EventKind::Modify(notify::event::ModifyKind::Any)).add_path(outside);
        let batch = batch_from_events(root, &watched, vec![event]).unwrap();
        assert_eq!(batch.changed_relative_paths, vec![""]);
    }

    #[test]
    fn recursive_event_from_unloaded_subfolder_updates_search_only() {
        let workspace = tempfile::tempdir().expect("workspace");
        let watched_paths = watched_paths_snapshot(HashMap::from([(
            String::new(),
            workspace.path().to_path_buf(),
        )]));
        let event = Event::new(EventKind::Create(notify::event::CreateKind::File))
            .add_path(workspace.path().join("src/deep/new.dart"));

        let batch = batch_from_events(workspace.path(), &watched_paths, vec![event])
            .expect("search index refresh for collapsed subfolder");

        assert!(batch.directory_relative_paths.is_empty());
        assert_eq!(batch.changed_relative_paths, vec!["src/deep/new.dart"]);
    }

    #[test]
    fn explicitly_watched_ignored_named_directory_emits_refresh() {
        let workspace = tempfile::tempdir().expect("workspace");
        let build = workspace.path().join("build");
        let watched_paths =
            watched_paths_snapshot(HashMap::from([("build".to_string(), build.clone())]));
        let event = Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(build.join("artifact.txt"));

        let batch = batch_from_events(workspace.path(), &watched_paths, vec![event])
            .expect("watched directory refresh");

        assert_eq!(batch.directory_relative_paths, vec!["build"]);
        assert_eq!(batch.changed_relative_paths, vec!["build/artifact.txt"]);
    }

    #[test]
    fn native_burst_deduplicates_paths_and_preserves_rename_and_error_hints() {
        let workspace = tempfile::tempdir().expect("workspace");
        let root = workspace.path();
        let mut loaded = HashMap::from([(String::new(), root.to_path_buf())]);
        for i in 0..2000 {
            let name = format!("dir-{i:04}");
            loaded.insert(name.clone(), root.join(&name));
        }
        let watched = watched_paths_snapshot(loaded);
        let old_path = root.join("dir-0000/old.txt");
        let new_path = root.join("dir-1999/new.txt");
        let mut pending = PendingWatchBatch::default();
        for _ in 0..8000 {
            pending.push(
                root,
                Ok(
                    Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
                        .add_path(old_path.clone()),
                ),
            );
        }
        pending.push(
            root,
            Ok(
                Event::new(EventKind::Modify(notify::event::ModifyKind::Name(
                    notify::event::RenameMode::Both,
                )))
                .add_path(old_path)
                .add_path(new_path),
            ),
        );
        pending.push(root, Ok(Event::new(EventKind::Any)));
        pending.push(root, Err(notify::Error::generic("test overflow")));
        assert_eq!(pending.event_count, 8003);
        assert_eq!(pending.paths.len(), 3);
        let batch = build_batch(root, &watched, pending).expect("burst batch");
        assert_eq!(batch.coalesced_event_count, 8003);
        assert_eq!(
            batch.changed_relative_paths,
            vec!["", "dir-0000/old.txt", "dir-1999/new.txt"]
        );
        assert_eq!(batch.directory_relative_paths.len(), 2001);
        assert_eq!(batch.directory_relative_paths.first().unwrap(), "");
        assert_eq!(batch.directory_relative_paths.last().unwrap(), "dir-1999");
    }

    #[test]
    fn mailbox_overflow_reconciles_every_loaded_directory() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let watched = watched_paths_snapshot(HashMap::from([
            (String::new(), root.to_path_buf()),
            ("src".into(), root.join("src")),
            ("nested".into(), root.join("nested")),
        ]));
        let (tx, _stop, inbox) = watch_event_channel();
        for i in 0..10_000 {
            assert!(tx.send(Ok(
                Event::new(EventKind::Any).add_path(root.join(format!("src/{i}.rs")))
            )));
        }
        let mut pending = PendingWatchBatch::default();
        assert!(inbox.collect_batch(|delivery| pending.push_delivery(root, delivery)));
        assert_eq!(pending.event_count, 10_000);
        assert_eq!(pending.paths.len(), 1);
        let batch = build_batch(root, &watched, pending).unwrap();
        assert_eq!(batch.directory_relative_paths, ["", "nested", "src"]);
        assert_eq!(batch.changed_relative_paths, [""]);
        assert_eq!(batch.deferred_quick_open_paths, Some(vec![]));
        assert_eq!(batch.coalesced_event_count, 10_000);
    }

    #[test]
    fn fast_consumer_distinct_path_budget_also_collapses_to_root() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let watched = watched_paths_snapshot(HashMap::from([
            (String::new(), root.to_path_buf()),
            ("src".into(), root.join("src")),
        ]));
        let mut pending = PendingWatchBatch::default();
        for i in 0..10_000 {
            pending.push(
                root,
                Ok(Event::new(EventKind::Any).add_path(root.join(format!("src/{i}.rs")))),
            );
            assert!(pending.paths.len() <= MAX_BATCH_PATHS);
        }
        assert!(pending.collapsed);
        assert_eq!(pending.paths.len(), 1);
        let batch = build_batch(root, &watched, pending).unwrap();
        assert_eq!(batch.directory_relative_paths, ["", "src"]);
        assert_eq!(batch.changed_relative_paths, [""]);
        assert_eq!(batch.coalesced_event_count, 10_000);
    }

    #[test]
    fn native_watcher_matches_deepest_loaded_directory_and_all_aliases() {
        let workspace = tempfile::tempdir().expect("workspace");
        let root = workspace.path();
        let child = root.join("src");
        let watched = watched_paths_snapshot(HashMap::from([
            (String::new(), root.to_path_buf()),
            ("src".to_string(), child.clone()),
            ("alias".to_string(), child.clone()),
        ]));
        let batch = batch_from_events(
            root,
            &watched,
            vec![
                Event::new(EventKind::Any).add_path(child.clone()),
                Event::new(EventKind::Any).add_path(child.join("file.txt")),
                Event::new(EventKind::Any).add_path(child.join("unloaded/deep.txt")),
            ],
        )
        .expect("batch");
        assert_eq!(batch.directory_relative_paths, vec!["alias", "src"]);
        assert_eq!(
            batch.changed_relative_paths,
            vec!["src", "src/file.txt", "src/unloaded/deep.txt"]
        );
    }
}
