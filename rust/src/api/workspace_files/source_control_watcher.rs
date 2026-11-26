use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use notify::event::{AccessKind, AccessMode};
use notify::{EventKind, RecursiveMode, Watcher};

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
        // info/exclude, sparse-checkout and attributes can change status.
        metadata.private_dir.join("info"),
        metadata.common_dir.join("info"),
    ] {
        if dir.is_dir() && watched.insert(dir.clone()) {
            watcher.watch(&dir, RecursiveMode::Recursive)?;
        }
    }
    Ok(())
}

fn metadata_relative_path_is_relevant(relative: &Path, shared: bool) -> bool {
    let mut components = relative.components();
    let first = components.next().and_then(|part| part.as_os_str().to_str());
    if matches!(first, Some("objects" | "logs" | "hooks" | "lfs")) {
        return false;
    }
    // The main checkout watches its .git directory recursively. Git's
    // administrative directories also contain other worktrees' private
    // HEAD/index/reflog files. Those do not change this checkout's status.
    // Keep structural worktree events, shared refs/config, and unknown
    // metadata; don't suppress a whole worktree administration subtree.
    if shared && first == Some("worktrees") && components.next().is_some() {
        let name = components.next().and_then(|part| part.as_os_str().to_str());
        if name == Some("logs") {
            return false;
        }
        if matches!(name, Some("HEAD" | "HEAD.lock" | "index" | "index.lock"))
            && components.next().is_none()
        {
            return false;
        }
    }
    true
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
    // Overflow/rescan can carry paths and even an access event kind. Never
    // let the filtering policy hide a request for full reconciliation.
    if event.need_rescan() || event.paths.is_empty() {
        return true;
    }
    if matches!(
        event.kind,
        EventKind::Access(
            AccessKind::Read
                | AccessKind::Open(AccessMode::Read | AccessMode::Execute)
                | AccessKind::Close(AccessMode::Read | AccessMode::Execute)
        )
    ) {
        return false;
    }
    event.paths.iter().any(|path| {
        if let Some(git) = metadata {
            // A linked checkout's own private metadata wins over the common
            // directory rule, because it is physically inside worktrees/.
            if let Ok(relative) = path.strip_prefix(&git.private_dir) {
                return metadata_relative_path_is_relevant(
                    relative,
                    git.private_dir == git.common_dir,
                );
            }
            if let Ok(relative) = path.strip_prefix(&git.common_dir) {
                return metadata_relative_path_is_relevant(relative, true);
            }
        }
        path_is_relevant(root, path)
    })
}

fn path_is_relevant(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut components = relative.components();
    let first = components.next().and_then(|part| part.as_os_str().to_str());
    if first == Some(".git") {
        return metadata_relative_path_is_relevant(components.as_path(), true);
    }
    if matches!(
        first,
        Some(".dart_tool" | "build" | "node_modules" | "target")
    ) {
        return false;
    }
    // Avoid a Vec allocation on every filesystem notification.
    !components.any(|component| component.as_os_str() == "node_modules")
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
    fn read_only_access_never_requests_a_status_scan() {
        let root = PathBuf::from("repo");
        let event = |kind| Event::new(EventKind::Access(kind)).add_path(root.join("src/main.rs"));
        for kind in [
            AccessKind::Read,
            AccessKind::Open(AccessMode::Read),
            AccessKind::Close(AccessMode::Read),
            AccessKind::Open(AccessMode::Execute),
            AccessKind::Close(AccessMode::Execute),
        ] {
            assert!(!event_is_relevant(&root, None, &event(kind)), "{kind:?}");
        }
        for kind in [
            AccessKind::Any,
            AccessKind::Other,
            AccessKind::Open(AccessMode::Any),
            AccessKind::Open(AccessMode::Write),
            AccessKind::Close(AccessMode::Write),
        ] {
            assert!(event_is_relevant(&root, None, &event(kind)), "{kind:?}");
        }
        assert!(event_is_relevant(
            &root,
            None,
            &Event::new(EventKind::Access(AccessKind::Read)),
        ));
        assert!(event_is_relevant(
            &root,
            None,
            &Event::new(EventKind::Access(AccessKind::Read))
                .add_path(root.join("node_modules/pkg"))
                .set_flag(notify::event::Flag::Rescan),
        ));
        assert!(event_is_relevant(
            &root,
            None,
            &Event::new(EventKind::Other)
                .add_path(root.join(".git/objects/aa/bb"))
                .set_flag(notify::event::Flag::Rescan),
        ));
    }

    #[test]
    fn private_worktree_metadata_only_invalidates_its_owner() {
        let main = PathBuf::from("repo");
        let common = main.join(".git");
        let own = common.join("worktrees/own");
        let sibling = common.join("worktrees/sibling");
        let linked_root = PathBuf::from("linked");
        let metadata = LinkedGitMetadata {
            private_dir: own.clone(),
            common_dir: common.clone(),
        };
        let event = |path: PathBuf| Event::new(EventKind::Any).add_path(path);
        for name in ["HEAD", "HEAD.lock", "index", "index.lock"] {
            assert!(!event_is_relevant(&main, None, &event(own.join(name))));
            assert!(event_is_relevant(
                &linked_root,
                Some(&metadata),
                &event(own.join(name))
            ));
            assert!(!event_is_relevant(
                &linked_root,
                Some(&metadata),
                &event(sibling.join(name))
            ));
            assert!(event_is_relevant(&main, None, &event(common.join(name))));
            // A linked checkout must still observe shared metadata and
            // conservatively retain direct common-directory notifications.
            assert!(event_is_relevant(
                &linked_root,
                Some(&metadata),
                &event(common.join(name))
            ));
        }
        assert!(!event_is_relevant(
            &main,
            None,
            &event(sibling.join("logs/HEAD"))
        ));
        assert!(!event_is_relevant(
            &linked_root,
            Some(&metadata),
            &event(own.join("logs/HEAD"))
        ));
        for path in [
            common.join("refs/heads/feature"),
            common.join("refs/stash"),
            common.join("packed-refs"),
            common.join("config"),
            common.join("info/exclude"),
            common.join("info/attributes"),
            own.join("info/sparse-checkout"),
            sibling.clone(),
            sibling.join("gitdir"),
            sibling.join("commondir"),
            sibling.join("refs/worktree/private"),
            sibling.join("index/nested"),
        ] {
            assert!(
                event_is_relevant(&main, None, &event(path.clone())),
                "{path:?}"
            );
            assert!(
                event_is_relevant(&linked_root, Some(&metadata), &event(path.clone())),
                "{path:?}"
            );
        }
        // A rename with a relevant destination must be retained, even if
        // the other path alone would have been filtered.
        let rename = Event::new(EventKind::Modify(notify::event::ModifyKind::Name(
            notify::event::RenameMode::Both,
        )))
        .add_path(sibling.join("index.lock"))
        .add_path(common.join("index"));
        assert!(event_is_relevant(&main, None, &rename));

        // An external Git directory without commondir owns its HEAD/index.
        let external = LinkedGitMetadata {
            private_dir: common.clone(),
            common_dir: common.clone(),
        };
        assert!(event_is_relevant(
            &linked_root,
            Some(&external),
            &event(common.join("index"))
        ));
    }

    #[test]
    fn event_storm_filters_private_index_traffic_and_reads_not_real_changes() {
        let root = PathBuf::from("repo");
        let mut accepted = 0;
        let mut submitted = 0;
        let mut route = |event| {
            submitted += 1;
            if event_is_relevant(&root, None, &event) {
                accepted += 1;
                true
            } else {
                false
            }
        };
        // Each event previously passed the path-only filter, including
        // reads caused by Git/status or editor activity.
        for i in 0..100 {
            for _ in 0..10 {
                for name in ["index", "index.lock", "HEAD", "HEAD.lock"] {
                    assert!(!route(
                        Event::new(EventKind::Any)
                            .add_path(root.join(format!(".git/worktrees/wt-{i}/{name}")))
                    ));
                }
                assert!(!route(
                    Event::new(EventKind::Access(AccessKind::Read))
                        .add_path(root.join(format!("src/file-{i}.rs")))
                ));
            }
        }
        // Preserve all 100 real edits plus a shared ref update, ignore-rule
        // update, and a rescan of an otherwise excluded directory.
        for i in 0..100 {
            assert!(route(
                Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
                    .add_path(root.join(format!("src/file-{i}.rs")))
            ));
        }
        assert!(route(
            Event::new(EventKind::Any).add_path(root.join(".git/refs/heads/main"))
        ));
        assert!(route(
            Event::new(EventKind::Any).add_path(root.join(".git/info/exclude"))
        ));
        assert!(route(
            Event::new(EventKind::Other)
                .add_path(root.join("build"))
                .set_flag(notify::event::Flag::Rescan)
        ));
        assert_eq!(submitted, 5103);
        assert_eq!(accepted, 103);
        // Old policy accepted 5101 events (the final info/rescan were
        // incorrectly suppressed). Count admitted events, not full scans.
        println!("Source Control routing fixture: old accepted 5101, new accepted {accepted}, submitted {submitted}");
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
        std::fs::create_dir_all(common.join("info")).unwrap();
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
        let main_root = common.parent().unwrap();
        // Real OS notification delivery, including info/exclude outside
        // the checkout root. The index is relevant only to its owner.
        let index_path = metadata.private_dir.join("index");
        let info_path = metadata.common_dir.join("info/exclude");
        let ref_path = metadata.common_dir.join("refs/test-ref");
        let main_root = dunce::canonicalize(main_root).unwrap();
        std::fs::write(&index_path, b"new index").unwrap();
        std::fs::write(&info_path, b"ignored.txt\n").unwrap();
        std::fs::write(&ref_path, b"ref update").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut observed_index = false;
        let mut observed_info = false;
        let mut observed_ref = false;
        while Instant::now() < deadline && !(observed_index && observed_info && observed_ref) {
            let timeout = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(timeout) {
                Ok(event) => {
                    for path in &event.paths {
                        if path == &index_path {
                            observed_index = true;
                            assert!(!event_is_relevant(
                                &main_root,
                                None,
                                &Event::new(event.kind).add_path(path.clone()),
                            ));
                        }
                        if path == &info_path {
                            observed_info = true;
                        }
                        if path == &ref_path {
                            observed_ref = true;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        assert!(observed_index, "linked worktree index notification missing");
        assert!(observed_info, "shared info/exclude notification missing");
        assert!(observed_ref, "shared refs notification missing");
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
