use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use notify::{RecursiveMode, Watcher};

use crate::frb_generated::StreamSink;

use super::{
    workspace_root, SourceControlWatchSignal, SourceControlWatcherHandle, WorkspaceFileError,
    WorkspaceFileErrorKind,
};

use super::watch_events::{watch_event_channel, WatchEventInbox, WatchStopper};

#[derive(Debug, Clone)]
struct LinkedGitMetadata {
    private_dir: PathBuf,
    common_dir: PathBuf,
}

// For linked worktrees, HEAD and index live outside the workspace root.
fn linked_git_metadata(root: &Path) -> Option<LinkedGitMetadata> {
    let pointer = std::fs::read_to_string(root.join(".git")).ok()?;
    let git_dir = pointer
        .lines()
        .find_map(|line| line.trim().strip_prefix("gitdir:").map(str::trim))?;
    let private_dir = canonical_git_directory(root, git_dir)?;
    let common_dir = match std::fs::read_to_string(private_dir.join("commondir")) {
        Ok(common) if !common.trim().is_empty() => {
            canonical_git_directory(&private_dir, common.trim())?
        }
        _ => private_dir.clone(),
    };
    Some(LinkedGitMetadata {
        private_dir,
        common_dir,
    })
}

fn canonical_git_directory(base: &Path, value: &str) -> Option<PathBuf> {
    let candidate = Path::new(value);
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    };
    let path = dunce::canonicalize(candidate).ok()?;
    path.is_dir().then_some(path)
}

// Avoid recursively watching the noisy Git object database.
fn watch_linked_git_metadata(
    watcher: &mut notify::RecommendedWatcher,
    metadata: &LinkedGitMetadata,
) -> notify::Result<()> {
    let mut watched = HashSet::<PathBuf>::new();
    for dir in [&metadata.private_dir, &metadata.common_dir] {
        if watched.insert(dir.clone()) {
            watcher.watch(dir, RecursiveMode::NonRecursive)?;
        }
    }
    for dir in [
        metadata.private_dir.join("refs"),
        metadata.common_dir.join("refs"),
    ] {
        if dir.is_dir() && watched.insert(dir.clone()) {
            watcher.watch(&dir, RecursiveMode::Recursive)?;
        }
    }
    Ok(())
}

fn metadata_path_is_relevant(metadata_root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(metadata_root) else {
        return false;
    };
    let first = relative
        .components()
        .next()
        .and_then(|part| part.as_os_str().to_str());
    !matches!(first, Some("objects" | "logs" | "hooks" | "info" | "lfs"))
}

static NEXT_WATCHER_ID: AtomicU64 = AtomicU64::new(1);
static WATCHERS: OnceLock<Mutex<HashMap<String, SourceControlWatcherSession>>> = OnceLock::new();

struct SourceControlWatcherSession {
    // Held only to keep the OS watch alive; dropping it stops file notifications.
    #[allow(dead_code)]
    watcher: notify::RecommendedWatcher,
    sink: Arc<Mutex<Option<StreamSink<SourceControlWatchSignal>>>>,
    stop: WatchStopper,
    thread: Option<thread::JoinHandle<()>>,
}

impl SourceControlWatcherSession {
    fn stop(mut self) {
        self.stop.request_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(super) fn start_source_control_watcher(
    workspace_path: String,
) -> Result<SourceControlWatcherHandle, WorkspaceFileError> {
    let root = workspace_root(&workspace_path)?;
    let id = NEXT_WATCHER_ID.fetch_add(1, Ordering::Relaxed).to_string();
    let (event_tx, stop, inbox) = watch_event_channel();
    let sink = Arc::new(Mutex::new(None));
    let event_root = root.clone();
    let metadata = linked_git_metadata(&root);
    let event_metadata = metadata.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if result
            .as_ref()
            .is_ok_and(|event| !event_is_relevant(&event_root, event_metadata.as_ref(), event))
        {
            return;
        }
        let _ = event_tx.send(result);
    })
    .map_err(notify_error)?;
    // Recursive watch of the whole working tree, including `.git/`, so commits,
    // branch switches, staging and out-of-app file edits all surface here.
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(notify_error)?;
    // Linked worktree metadata and shared refs are outside the watched root.
    if let Some(metadata) = metadata.as_ref() {
        watch_linked_git_metadata(&mut watcher, metadata).map_err(notify_error)?;
    }

    let thread_sink = sink.clone();
    let thread = thread::Builder::new()
        .name(format!("alera-source-control-watch-{id}"))
        .spawn(move || {
            event_loop(thread_sink, inbox);
        })
        .map_err(|error| WorkspaceFileError::from_io(error, "source control watcher"))?;

    let session = SourceControlWatcherSession {
        watcher,
        sink,
        stop,
        thread: Some(thread),
    };
    watchers()
        .lock()
        .map_err(|_| WorkspaceFileError::new(WorkspaceFileErrorKind::Io, "watcher lock poisoned"))?
        .insert(id.clone(), session);
    Ok(SourceControlWatcherHandle { id })
}

fn event_is_relevant(
    root: &Path,
    metadata: Option<&LinkedGitMetadata>,
    event: &notify::Event,
) -> bool {
    // Unknown/overflow events must always trigger a status reconciliation.
    event.paths.is_empty()
        || event.paths.iter().any(|path| {
            path_is_relevant(root, path)
                || metadata.is_some_and(|git| {
                    metadata_path_is_relevant(&git.private_dir, path)
                        || metadata_path_is_relevant(&git.common_dir, path)
                })
        })
}

fn path_is_relevant(root: &std::path::Path, path: &std::path::Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let components = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    let Some(first) = components.first().copied() else {
        return true;
    };
    if matches!(first, ".dart_tool" | "build" | "node_modules" | "target") {
        return false;
    }
    if components.contains(&"node_modules") {
        return false;
    }
    if first != ".git" {
        return true;
    }
    // Merge/rebase markers, index locks and refs affect repository state too.
    // Ignore only noisy object/log storage rather than a fragile allowlist.
    !matches!(
        components.get(1).copied(),
        Some("objects" | "logs" | "hooks" | "info" | "lfs")
    )
}

pub(super) fn watch_source_control_events(
    handle: SourceControlWatcherHandle,
    sink: StreamSink<SourceControlWatchSignal>,
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

pub(super) fn stop_source_control_watcher(handle: SourceControlWatcherHandle) {
    let session = watchers()
        .lock()
        .ok()
        .and_then(|mut watchers| watchers.remove(&handle.id));
    if let Some(session) = session {
        session.stop();
    }
}

fn event_loop(
    sink: Arc<Mutex<Option<StreamSink<SourceControlWatchSignal>>>>,
    inbox: WatchEventInbox,
) {
    while let Some(signal) = collect_signal(&inbox) {
        if inbox.is_stopping() {
            return;
        }
        emit_signal(&sink, signal);
    }
}

fn collect_signal(inbox: &WatchEventInbox) -> Option<SourceControlWatchSignal> {
    let mut coalesced_event_count = 0u32;
    inbox
        .collect_batch(|_| {
            coalesced_event_count = coalesced_event_count.saturating_add(1);
        })
        .then_some(SourceControlWatchSignal {
            coalesced_event_count,
        })
}

fn emit_signal(
    sink: &Arc<Mutex<Option<StreamSink<SourceControlWatchSignal>>>>,
    signal: SourceControlWatchSignal,
) {
    // Closed Flutter sinks must not poison the watcher thread: drop the sink
    // and keep the OS watch alive until the client re-subscribes or stops.
    let Ok(mut sink) = sink.lock() else {
        return;
    };
    let Some(current_sink) = sink.as_ref() else {
        return;
    };
    if current_sink.add(signal).is_err() {
        *sink = None;
    }
}

fn watchers() -> &'static Mutex<HashMap<String, SourceControlWatcherSession>> {
    WATCHERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn notify_error(error: notify::Error) -> WorkspaceFileError {
    WorkspaceFileError::new(WorkspaceFileErrorKind::Io, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::{Event, EventKind};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn ok_event() -> notify::Result<Event> {
        Ok(Event::new(EventKind::Modify(
            notify::event::ModifyKind::Any,
        )))
    }

    #[test]
    fn linked_worktree_metadata_resolves_private_and_shared_git_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("checkout");
        let common = temp.path().join("original").join(".git");
        let private = common.join("worktrees").join("checkout");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&private).unwrap();
        std::fs::write(
            root.join(".git"),
            "gitdir: ../original/.git/worktrees/checkout\n",
        )
        .unwrap();
        std::fs::write(private.join("commondir"), "../..\n").unwrap();
        let metadata = linked_git_metadata(&root).expect("linked worktree metadata");
        assert_eq!(metadata.private_dir, dunce::canonicalize(&private).unwrap());
        assert_eq!(metadata.common_dir, dunce::canonicalize(&common).unwrap());

        let event = |path: &Path| Event::new(EventKind::Any).add_path(path.to_path_buf());
        for path in [
            private.join("HEAD"),
            private.join("index"),
            common.join("packed-refs"),
            common.join("refs").join("heads").join("main"),
        ] {
            assert!(
                event_is_relevant(&root, Some(&metadata), &event(&path)),
                "{path:?}"
            );
        }
        assert!(!event_is_relevant(
            &root,
            Some(&metadata),
            &event(&common.join("objects").join("00").join("ab"))
        ));
    }

    #[test]
    fn linked_worktree_index_change_triggers_real_os_notification() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("checkout");
        let common = temp.path().join("repo").join(".git");
        let private = common.join("worktrees").join("checkout");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&private).unwrap();
        std::fs::create_dir_all(common.join("refs")).unwrap();
        std::fs::write(
            root.join(".git"),
            "gitdir: ../repo/.git/worktrees/checkout\n",
        )
        .unwrap();
        std::fs::write(private.join("commondir"), "../..\n").unwrap();
        let metadata = linked_git_metadata(&root).unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = {
            let metadata_for_event = metadata.clone();
            let root_for_event = root.clone();
            let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
                if let Ok(event) = event {
                    if event_is_relevant(&root_for_event, Some(&metadata_for_event), &event) {
                        let _ = tx.send(event);
                    }
                }
            })
            .unwrap();
            watcher.watch(&root, RecursiveMode::Recursive).unwrap();
            watch_linked_git_metadata(&mut watcher, &metadata).unwrap();
            watcher
        };
        std::fs::write(private.join("index"), b"new index").unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut observed = false;
        while Instant::now() < deadline {
            let timeout = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(timeout) {
                Ok(event)
                    if event
                        .paths
                        .iter()
                        .any(|path| path == &private.join("index")) =>
                {
                    observed = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(
            observed,
            "linked worktree index did not emit a filesystem event"
        );
    }
    #[test]
    fn coalesces_a_burst_into_one_signal() {
        let (event_tx, _stop, inbox) = watch_event_channel();
        for _ in 0..4 {
            assert!(event_tx.send(ok_event()));
        }

        let signal = collect_signal(&inbox).expect("coalesced signal");
        assert_eq!(signal.coalesced_event_count, 4);
    }

    #[test]
    fn watcher_errors_still_trigger_reconciliation() {
        let (event_tx, _stop, inbox) = watch_event_channel();
        assert!(event_tx.send(Err(notify::Error::generic("watch overflow"))));
        let signal = collect_signal(&inbox).expect("watcher errors must refresh status");
        assert_eq!(signal.coalesced_event_count, 1);
    }

    #[test]
    fn idle_worker_stops_without_an_event() {
        let (_event_tx, stop, inbox) = watch_event_channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            done_tx.send(collect_signal(&inbox).is_none()).unwrap();
        });
        assert!(done_rx.recv_timeout(Duration::from_millis(130)).is_err());
        stop.request_stop();
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        worker.join().unwrap();
    }

    #[test]
    fn shuts_down_when_event_channel_disconnects() {
        let (event_tx, _stop, inbox) = watch_event_channel();
        drop(event_tx);

        assert!(collect_signal(&inbox).is_none());
    }

    #[test]
    fn filters_generated_output_but_keeps_worktree_and_git_state_changes() {
        let root = std::path::PathBuf::from("repo");
        let event = |path: &str| Event {
            paths: vec![root.join(path)],
            ..Event::new(EventKind::Any)
        };

        assert!(!event_is_relevant(&root, None, &event(".dart_tool/cache")));
        assert!(!event_is_relevant(
            &root,
            None,
            &event("app/node_modules/pkg")
        ));
        assert!(!event_is_relevant(
            &root,
            None,
            &event(".git/objects/aa/bb")
        ));
        assert!(event_is_relevant(&root, None, &event("lib/main.dart")));
        assert!(event_is_relevant(&root, None, &event(".git/index")));
        assert!(event_is_relevant(&root, None, &event(".git/MERGE_HEAD")));
        assert!(event_is_relevant(&root, None, &event(".git/index.lock")));
        assert!(event_is_relevant(
            &root,
            None,
            &event(".git/refs/heads/main")
        ));
    }
}
