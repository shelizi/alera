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

use super::watch_events::{watch_event_channel, WatchEventInbox, WatchStopper};

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

#[derive(Default)]
struct PendingWatchBatch {
    paths: HashSet<PathBuf>,
    event_count: u32,
}

impl PendingWatchBatch {
    fn push(&mut self, root: &Path, result: notify::Result<notify::Event>) {
        self.event_count = self.event_count.saturating_add(1);
        match result {
            Ok(event) if !event.paths.is_empty() => {
                self.paths.extend(
                    event
                        .paths
                        .into_iter()
                        .map(|path| normalize_event_path(root, path)),
                );
            }
            // Empty-path events and backend errors must still request
            // authoritative root reconciliation, even in a duplicate burst.
            _ => {
                self.paths.insert(root.to_path_buf());
            }
        }
    }
}

struct WatcherSession {
    root: PathBuf,
    watcher: notify::RecommendedWatcher,
    recursive_root: bool,
    watched_paths: WatchedPaths,
    sink: Arc<Mutex<Option<StreamSink<WorkspaceExplorerWatchBatch>>>>,
    stop: WatchStopper,
    thread: Option<thread::JoinHandle<()>>,
}

impl WatcherSession {
    fn stop(mut self) {
        self.stop.request_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn start_workspace_explorer_watcher(
    workspace_path: String,
) -> Result<WorkspaceExplorerWatcherHandle, WorkspaceFileError> {
    let root = workspace_root(&workspace_path)?;
    let id = NEXT_WATCHER_ID.fetch_add(1, Ordering::Relaxed).to_string();
    let (event_tx, stop, inbox) = watch_event_channel();
    let watched_paths = watched_paths_snapshot(HashMap::from([(String::new(), root.clone())]));
    let sink = Arc::new(Mutex::new(None));
    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = event_tx.send(result);
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

    let session = WatcherSession {
        root,
        watcher,
        recursive_root,
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
        if !inbox.collect_batch(|event| pending.push(&root, event)) {
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
    for event_path in pending.paths {
        if let Ok(relative_path) = relative_string(root, &event_path) {
            changed_relative_paths.insert(relative_path);
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
    Some(WorkspaceExplorerWatchBatch {
        directory_relative_paths,
        changed_relative_paths,
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
        assert_eq!(
            batch.directory_relative_paths,
            vec!["", "dir-0000", "dir-1999"]
        );
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
