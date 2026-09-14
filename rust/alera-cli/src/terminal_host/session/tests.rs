use super::*;
use std::io::Write;

#[path = "writer_tests.rs"]
mod writer_tests;

/// A sealed shell that never existed, for the cases that only care that the
/// field is cleared.
fn test_shell() -> ShellProcess {
    ShellProcess {
        pid: 4242,
        start_time: 1_700_000_000,
    }
}

struct BlockingWriter {
    started_tx: std::sync::mpsc::Sender<()>,
    release_rx: std::sync::mpsc::Receiver<()>,
}

impl Write for BlockingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.started_tx.send(()).unwrap();
        self.release_rx.recv().unwrap();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("writer failed"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct RecordingWriter {
    bytes: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn test_session() -> Session {
    Session {
        #[cfg(unix)]
        child_reaper: Arc::default(),
        instance_id: next_session_instance_id(),
        initial_agent_prompt_delivered: false,
        id: "session-1".to_string(),
        workspace_id: "workspace-1".to_string(),
        tab_id: "tab-1".to_string(),
        working_directory: "/repo".to_string(),
        clients: HashSet::new(),
        driver: SessionDriver::Idle,
        desktop_dims: None,
        current_dims: (80, 24),
        output_paused_clients: HashSet::new(),
        output_resync_pending_clients: HashSet::new(),
        delivered_output_cursors: HashMap::new(),
        buffer: ScrollbackBuffer::new(1024, &[]),
        running: true,
        exit_code: None,
        ended_at: None,
        shell: None,
        master: None,
        input_tx: None,
        killer: None,
        #[cfg(windows)]
        process_job: None,
        #[cfg(windows)]
        conpty_startup_cursor_query_tail: Vec::new(),
        #[cfg(windows)]
        conpty_startup_cursor_query_answered: false,
        terminated: false,
        checkpoint_gen: 0,
        checkpoint_armed: false,
        output_batch: Vec::new(),
        output_batch_gen: 0,
        output_batch_armed: false,
        durable_output_batch: Vec::new(),
        durable_output_batch_gen: 0,
        durable_output_batch_armed: false,
        durable_output_batch_sequence: 0,
        output_stream_bytes: 0,
        title_tracker: TerminalTitleTracker::default(),
    }
}

#[cfg(windows)]
#[test]
fn conpty_startup_cursor_query_is_detected_once_across_output_chunks() {
    let mut session = test_session();

    assert!(!session.take_initial_conpty_cursor_query(b"prefix\x1b["));
    assert!(session.take_initial_conpty_cursor_query(b"6n suffix"));
    assert!(!session.take_initial_conpty_cursor_query(b"\x1b[6n"));
}

#[test]
fn output_batch_coalesces_until_flush() {
    let mut session = test_session();
    assert_eq!(session.append_output(b"ab"), (Some(0), Some(0), None));
    assert_eq!(session.append_output(b"cd"), (None, None, None));
    assert!(session.output_batch_due(0));
    let batch = session.flush_output_batch().expect("batch");
    // Raw bytes, not an encoded payload: whether these go out as a binary
    // frame or as base64 inside JSON is the writer's decision, per client.
    assert_eq!(batch.data, b"abcd");
    assert_eq!(session.output_batch_len(), 0);
    assert!(!session.output_batch_due(0));
    let durable = session.flush_durable_output_batch().expect("durable batch");
    assert_eq!(durable.data, b"abcd");
    assert_eq!(durable.sequence, 0);
}

#[test]
fn output_backpressure_pauses_only_the_slow_client_until_resumed() {
    let mut session = test_session();
    session.attach(1);
    session.attach(2);

    assert!(session.mark_output_backpressured(1));
    assert!(!session.mark_output_backpressured(1));
    assert_eq!(session.output_clients(), vec![2]);
    assert!(session.output_resync_pending(1));

    session.mark_output_resync_sent(1);
    assert!(!session.output_resync_pending(1));
    assert_eq!(session.output_clients(), vec![2]);

    session.set_output_paused(1, false);
    let mut clients = session.output_clients();
    clients.sort_unstable();
    assert_eq!(clients, vec![1, 2]);
}

#[test]
fn output_batch_empty_flush_disarms_timer() {
    let mut session = test_session();
    assert_eq!(session.append_output(b"a"), (Some(0), Some(0), None));
    assert!(session.flush_output_batch().is_some());
    assert!(session.flush_output_batch().is_none());
    assert_eq!(session.append_output(b"b"), (Some(1), None, None));
    assert!(!session.output_batch_due(0));
    assert!(session.output_batch_due(1));
    assert!(session.flush_output_batch().is_some());
    let durable = session.flush_durable_output_batch().expect("durable batch");
    assert_eq!(durable.data, b"ab");
    assert_eq!(durable.sequence, 0);
}

#[test]
fn output_stream_range_remains_monotonic_when_scrollback_trims() {
    let mut session = test_session();
    session.set_max_bytes(4);
    session.append_output(b"abcd");
    assert_eq!(session.output_stream_range(), (0, 4));

    session.append_output(b"ef");
    assert_eq!(session.buffer.to_bytes(), b"cdef");
    assert_eq!(session.output_stream_range(), (2, 6));
}

#[test]
fn remint_keeps_the_persisted_absolute_stream_position() {
    assert_eq!(resumed_output_stream_bytes(10, 4), 10);
    assert_eq!(resumed_output_stream_bytes(0, 4), 4);
}

#[test]
fn session_reports_title_changes_from_pty_output() {
    let mut session = test_session();

    let (_, _, title_change) = session.append_output(b"\x1b]2;Review Tests\x07");

    assert_eq!(title_change.as_deref(), Some("Review Tests"));
    assert_eq!(session.runtime_title(), Some("Review Tests"));
}

#[tokio::test]
async fn restored_output_stream_range_keeps_absolute_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path()).await.unwrap();
    history.queue_checkpoint(TerminalHostCheckpoint {
            session_id: "restored".to_string(),
            workspace_id: "workspace-1".to_string(),
            tab_id: "tab-1".to_string(),
            working_directory: "/repo".to_string(),
            running: false,
            exit_code: Some(0),
            ended_at: Some(Utc::now()),
            output_stream_bytes: 10,
            updated_at: Utc::now(),
            buffer: Vec::new(),
        }, 4);
    history.queue_output("restored".to_string(), 0, b"tail".to_vec(), 4);

    let session = Session::restore_exited(
        "restored".to_string(),
        "workspace-1".to_string(),
        "tab-1".to_string(),
        &history,
        4,
    )
    .await
    .unwrap();

    assert_eq!(session.output_stream_range(), (6, 10));
}

#[tokio::test]
async fn restored_session_recovers_the_latest_title_from_scrollback() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path()).await.unwrap();
    history.queue_checkpoint(TerminalHostCheckpoint {
            session_id: "restored-title".to_string(),
            workspace_id: "workspace-1".to_string(),
            tab_id: "tab-1".to_string(),
            working_directory: "/repo".to_string(),
            running: false,
            exit_code: Some(0),
            ended_at: Some(Utc::now()),
            output_stream_bytes: 0,
            updated_at: Utc::now(),
            buffer: Vec::new(),
        }, 1024);
    history.queue_output(
        "restored-title".to_string(),
        0,
        b"\x1b]0;Restored Task\x07".to_vec(),
        1024,
    );

    let session = Session::restore_exited(
        "restored-title".to_string(),
        "workspace-1".to_string(),
        "tab-1".to_string(),
        &history,
        1024,
    )
    .await
    .unwrap();

    assert_eq!(session.runtime_title(), Some("Restored Task"));
}

#[test]
fn exiting_clears_the_shell() {
    let mut session = test_session();
    session.shell = Some(test_shell());
    assert_eq!(session.shell(), Some(test_shell()));

    session.handle_exit(0);

    // The OS recycles PIDs, so keeping the old value would let the resource
    // sampler attribute an unrelated process to this session.
    assert_eq!(session.shell(), None);
}

#[tokio::test]
async fn terminating_clears_the_shell() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path()).await.unwrap();
    let mut session = test_session();
    session.shell = Some(test_shell());

    session.terminate(true, &history, 1024).await;

    assert_eq!(session.shell(), None);
}

#[test]
fn a_session_without_a_pty_has_no_shell() {
    // Stubs and restored checkpoints have no process behind them, so there is
    // nothing to sample.
    let session = Session::driver_test_stub("stub", 80, 24);
    assert_eq!(session.shell(), None);
}

#[test]
fn a_spawned_shell_is_sealed_with_its_start_time() {
    // Covers the seal end to end against a real process table: a pid alone
    // would not survive being recycled.
    let sealed = seal_shell_process(std::process::id()).expect("this process is live");

    assert_eq!(sealed.pid, std::process::id());
    assert!(sealed.start_time > 0);
}

#[test]
fn sealing_an_absent_pid_yields_nothing() {
    // Better unmeasured than measured against a guess.
    assert_eq!(seal_shell_process(u32::MAX), None);
}
