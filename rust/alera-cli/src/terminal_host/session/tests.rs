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
        verified_history_provider: None,
        working_directory: "/repo".to_string(),
        clients: HashSet::new(),
        view_leases: HashMap::new(),
        unscoped_clients: HashSet::new(),
        moved_workspace_fence: HashSet::new(),
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
        worker_process: None,
        #[cfg(windows)]
        conpty_startup_cursor_query: StartupCursorQuery::default(),
        terminated: false,
        checkpoint_gen: 0,
        checkpoint_armed: false,
        last_queued_checkpoint: None,
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
    session.attach(1);
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
fn owned_output_reuses_reader_allocation_across_independent_flushes() {
    let mut session = test_session();
    session.attach(1);
    let first = vec![0xff; 16 * 1024];
    let first_ptr = first.as_ptr();
    assert_eq!(session.append_owned_output(first), (Some(0), Some(0), None));
    assert_eq!(session.durable_output_batch.as_ptr(), first_ptr);
    let live = session.flush_output_batch().unwrap();
    assert_eq!(live.data, vec![0xff; 16 * 1024]);
    assert!(session.durable_output_batch_due(0));

    let second = vec![0x80; 8 * 1024];
    let second_ptr = second.as_ptr();
    assert_eq!(session.append_owned_output(second), (Some(1), None, None));
    assert_eq!(session.output_batch.as_ptr(), second_ptr);
    let durable = session.flush_durable_output_batch().unwrap();
    assert_eq!(durable.sequence, 0);
    assert_eq!(durable.end_cursor, 24 * 1024);
    assert_eq!(&durable.data[..16 * 1024], &vec![0xff; 16 * 1024]);
    assert_eq!(&durable.data[16 * 1024..], &vec![0x80; 8 * 1024]);
    assert!(session.output_batch_due(1));

    let third = vec![0x00; 4096];
    let third_ptr = third.as_ptr();
    assert_eq!(session.append_owned_output(third), (None, Some(1), None));
    let durable = session.flush_durable_output_batch().unwrap();
    assert_eq!(durable.data.as_ptr(), third_ptr);
    assert_eq!(durable.data, vec![0; 4096]);
    assert_eq!(durable.sequence, 1);
    assert_eq!(durable.end_cursor, 28 * 1024);
    let live = session.flush_output_batch().unwrap();
    assert_eq!(&live.data[..8192], &vec![0x80; 8192]);
    assert_eq!(&live.data[8192..], &vec![0; 4096]);
    assert_eq!(session.output_stream_range(), (27 * 1024, 28 * 1024));
    assert_eq!(session.buffer.to_bytes(), vec![0; 1024]);
}

#[test]
fn owned_output_does_not_retain_disproportionate_reader_capacity() {
    let mut session = test_session();
    session.attach(1);
    let mut data = Vec::with_capacity(1024 * 1024);
    data.extend_from_slice(b"tiny");
    let original = data.as_ptr();
    session.append_owned_output(data);
    assert_ne!(session.durable_output_batch.as_ptr(), original);
    assert_ne!(session.output_batch.as_ptr(), original);
    assert!(session.durable_output_batch.capacity() <= 8);
    assert!(session.output_batch.capacity() <= 8);
    assert_eq!(session.flush_output_batch().unwrap().data, b"tiny");
    assert_eq!(session.flush_durable_output_batch().unwrap().data, b"tiny");
}

#[test]
fn snapshot_encoding_matches_legacy_aligned_tail_across_ring_wrap() {
    use crate::terminal_host::protocol::{decode_bytes, encode_bytes};
    let mut session = test_session();
    session.set_max_bytes(23);
    session.append_output(b"01234567890123456789012");
    session.append_output(b"\xff\x00\x1b[0m\n\xe4\xb8\xad\xf0\x9f\x98\x80tail");
    let original = session.buffer.to_bytes();
    let (first, second) = session.buffer.range_slices(0, original.len());
    assert!(!first.is_empty() && !second.is_empty());
    for cap in 0..=original.len() + 1 {
        let expected = session.buffer.tail(cap);
        for payload in [
            session.attachment_payload(false, cap),
            session.restore_payload(cap),
        ] {
            assert_eq!(payload["snapshotBase64"], encode_bytes(&expected));
            assert_eq!(
                decode_bytes(payload.get("snapshotBase64")).unwrap(),
                expected
            );
        }
    }
    assert_eq!(session.buffer.to_bytes(), original);
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
    session.attach(1);
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
    let history = TerminalHostHistoryRepository::open(dir.path())
        .await
        .unwrap();
    history.queue_checkpoint(
        TerminalHostCheckpoint {
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
        },
        4,
    );
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
    let history = TerminalHostHistoryRepository::open(dir.path())
        .await
        .unwrap();
    history.queue_checkpoint(
        TerminalHostCheckpoint {
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
        },
        1024,
    );
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

#[tokio::test]
async fn cold_restored_session_keeps_binary_history_title_and_absolute_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path())
        .await
        .unwrap();
    let cap = 128 * 1024;
    let mut payload = vec![b'x'; 48 * 1024];
    payload.extend_from_slice(b"\x00\xff\x1b]2;Cold Restore\x07");
    let end = 1_000_000_u64;
    let id = "cold-offset-restore";
    history.queue_checkpoint(
        TerminalHostCheckpoint {
            session_id: id.to_string(),
            workspace_id: "workspace-1".to_string(),
            tab_id: "tab-1".to_string(),
            working_directory: "/repo".to_string(),
            running: false,
            exit_code: Some(0),
            ended_at: Some(Utc::now()),
            output_stream_bytes: end,
            updated_at: Utc::now(),
            buffer: Vec::new(),
        },
        cap,
    );
    for (sequence, chunk) in payload.chunks(16 * 1024).enumerate() {
        history.queue_output(id.to_string(), sequence as i64, chunk.to_vec(), cap);
    }
    assert!(
        history
            .flush_pending(std::time::Duration::from_secs(5))
            .await
    );
    let mut session = Session::restore_exited(
        id.to_string(),
        "workspace-1".to_string(),
        "tab-1".to_string(),
        &history,
        cap,
    )
    .await
    .unwrap();
    assert_eq!(session.buffer.to_bytes(), payload);
    assert_eq!(
        session.output_stream_range(),
        (end - payload.len() as u64, end)
    );
    assert_eq!(session.runtime_title(), Some("Cold Restore"));
    session.append_output(b"!");
    payload.push(b'!');
    assert_eq!(session.buffer.to_bytes(), payload);
    assert_eq!(session.output_stream_range().1, end + 1);
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
    let history = TerminalHostHistoryRepository::open(dir.path())
        .await
        .unwrap();
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

async fn drain_checkpoint_writes(history: &TerminalHostHistoryRepository) {
    assert!(
        history
            .flush_pending(std::time::Duration::from_secs(5))
            .await
    );
}

fn queue_session_checkpoint(
    session: &mut Session,
    history: &TerminalHostHistoryRepository,
    max_bytes: usize,
) {
    if let Some(batch) = session.flush_durable_output_batch() {
        history.queue_output_at_cursor(
            session.id.clone(),
            batch.sequence,
            batch.data,
            max_bytes,
            batch.end_cursor,
        );
    }
    session.queue_live_checkpoint(history, max_bytes);
}

#[tokio::test]
async fn unchanged_checkpoints_do_not_rewrite_sqlite_after_worker_retirement() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path())
        .await
        .unwrap();
    let audit = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new().filename(dir.path().join("terminal_history.sqlite")),
        )
        .await
        .unwrap();
    for statement in [
        "CREATE TABLE checkpoint_write_audit (kind TEXT NOT NULL)",
        "CREATE TRIGGER audit_checkpoint_insert AFTER INSERT ON checkpoints BEGIN INSERT INTO checkpoint_write_audit VALUES ('insert'); END",
        "CREATE TRIGGER audit_checkpoint_update AFTER UPDATE ON checkpoints BEGIN INSERT INTO checkpoint_write_audit VALUES ('update'); END",
    ] {
        sqlx::query(statement).execute(&audit).await.unwrap();
    }
    let mut session = test_session();
    session.set_max_bytes(4096);
    session.append_output(b"first output");
    queue_session_checkpoint(&mut session, &history, 4096);

    // The first checkpoint is already queued, but the worker has not run.
    for _ in 0..128 {
        queue_session_checkpoint(&mut session, &history, 4096);
    }
    drain_checkpoint_writes(&history).await;
    let first = history
        .read(&session.id, usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.buffer, b"first output");
    assert_eq!(first.output_stream_bytes, 12);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM checkpoint_write_audit")
        .fetch_one(&audit)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // No second worker, trim transaction, or checkpoint is necessary even
    // after the original worker has acknowledged and retired its state.
    for _ in 0..128 {
        queue_session_checkpoint(&mut session, &history, 4096);
        assert!(history.flush_pending(std::time::Duration::ZERO).await);
    }
    let unchanged = history
        .read(&session.id, usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unchanged, first);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM checkpoint_write_audit")
        .fetch_one(&audit)
        .await
        .unwrap();
    assert_eq!(count, 1);

    // Workspace transfer is a real metadata change even without PTY output.
    session.workspace_id = "moved-workspace".into();
    queue_session_checkpoint(&mut session, &history, 4096);
    drain_checkpoint_writes(&history).await;
    let moved = history
        .read(&session.id, usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(moved.workspace_id, "moved-workspace");
    assert_eq!(moved.buffer, first.buffer);

    // A cap reduction must still persist and trim the durable tail.
    session.set_max_bytes(5);
    queue_session_checkpoint(&mut session, &history, 5);
    drain_checkpoint_writes(&history).await;
    let trimmed = history
        .read(&session.id, usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(trimmed.buffer, b"utput");
    assert_eq!(trimmed.output_stream_bytes, 12);

    // The ring length remains five, but the absolute cursor has advanced.
    session.append_output(b"12345");
    queue_session_checkpoint(&mut session, &history, 5);
    drain_checkpoint_writes(&history).await;
    let changed = history
        .read(&session.id, usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(changed.buffer, b"12345");
    assert_eq!(changed.output_stream_bytes, 17);

    assert!(session.handle_exit(7).is_some());
    queue_session_checkpoint(&mut session, &history, 5);
    drain_checkpoint_writes(&history).await;
    let exited = history
        .read(&session.id, usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(exited.buffer, b"12345");
    assert!(!exited.running);
    assert_eq!(exited.exit_code, Some(7));
    assert!(exited.ended_at.is_some());

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM checkpoint_write_audit")
        .fetch_one(&audit)
        .await
        .unwrap();
    assert_eq!(count, 5);
    audit.close().await;
}

#[tokio::test]
async fn restored_running_checkpoint_is_repaired_before_noop_suppression() {
    let dir = tempfile::tempdir().unwrap();
    let history = TerminalHostHistoryRepository::open(dir.path())
        .await
        .unwrap();
    let mut original = test_session();
    original.append_output(b"crashed session");
    queue_session_checkpoint(&mut original, &history, 1024);
    drain_checkpoint_writes(&history).await;

    let mut restored = Session::restore_exited(
        original.id.clone(),
        original.workspace_id.clone(),
        original.tab_id.clone(),
        &history,
        1024,
    )
    .await
    .unwrap();
    assert!(restored.last_queued_checkpoint.is_none());
    queue_session_checkpoint(&mut restored, &history, 1024);
    drain_checkpoint_writes(&history).await;
    let repaired = history
        .read(&restored.id, usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repaired.buffer, b"crashed session");
    assert!(!repaired.running);
    assert_eq!(repaired.exit_code, Some(-1));

    queue_session_checkpoint(&mut restored, &history, 1024);
    assert!(history.flush_pending(std::time::Duration::ZERO).await);
}

#[test]
fn checkpoint_stamp_does_not_treat_saturated_cursor_as_unchanged() {
    let mut session = test_session();
    session.output_stream_bytes = u64::MAX;
    session.remember_queued_checkpoint(1024);
    assert!(!session
        .last_queued_checkpoint
        .as_ref()
        .unwrap()
        .matches(&session, 1024));
}

#[test]
fn unobserved_output_keeps_durable_bytes_without_allocating_live_batches() {
    let mut session = test_session();
    const CHUNK: usize = 16 * 1024;
    const CHUNKS: usize = 64;
    session.set_max_bytes(CHUNK * CHUNKS);
    let mut durable_bytes = Vec::new();
    let mut live_timers = 0;
    let mut durable_timers = 0;
    for index in 0..CHUNKS {
        let data = vec![index as u8; CHUNK];
        let original = data.as_ptr();
        let (live, durable, _) = session.append_owned_output(data);
        live_timers += usize::from(live.is_some());
        durable_timers += usize::from(durable.is_some());
        assert_eq!(session.output_batch.capacity(), 0);
        assert_eq!(session.output_batch_len(), 0);
        assert!(session.flush_output_batch().is_none());
        if index % 4 == 0 {
            assert_eq!(session.durable_output_batch.as_ptr(), original);
        }
        if index % 4 == 3 {
            let batch = session.flush_durable_output_batch().unwrap();
            assert_eq!(batch.sequence, (index / 4) as i64);
            assert_eq!(batch.end_cursor, ((index + 1) * CHUNK) as u64);
            durable_bytes.extend_from_slice(&batch.data);
        }
    }
    assert_eq!(live_timers, 0);
    assert_eq!(durable_timers, CHUNKS / 4);
    assert_eq!(durable_bytes.len(), CHUNK * CHUNKS);
    assert_eq!(session.buffer.to_bytes(), durable_bytes);
    assert_eq!(session.output_stream_range(), (0, (CHUNK * CHUNKS) as u64));
    for (index, chunk) in durable_bytes.chunks_exact(CHUNK).enumerate() {
        assert!(chunk.iter().all(|byte| *byte == index as u8));
    }
}

#[test]
fn live_batch_tracks_active_recipients_and_invalidates_an_abandoned_generation() {
    let mut session = test_session();
    session.attach(1);
    session.attach(2);
    session.set_output_paused(1, true);
    // Non-attached requests can leave stale pause entries. They must not
    // suppress the other viewer even when the set lengths happen to match.
    session.set_output_paused(99, true);
    let (first, _, _) = session.append_output(b"first");
    assert_eq!(first, Some(0));
    assert_eq!(session.output_clients(), vec![2]);
    assert_eq!(session.output_batch_len(), 5);

    session.set_output_paused(2, true);
    let (live, _, title) = session.append_output(b"\x1b]2;Background\x07");
    assert_eq!(live, None);
    assert_eq!(title.as_deref(), Some("Background"));
    assert_eq!(session.output_batch.capacity(), 0);
    assert!(!session.output_batch_due(first.unwrap()));
    assert_eq!(session.delivered_output_cursor(2), Some(0));
    assert!(session.output_clients().is_empty());

    // Real attach/resume first catches the recipient up from the ring.
    session.attach(2);
    let end = session.output_stream_range().1;
    let (next, _, _) = session.append_output(b"next");
    assert_eq!(next, Some(1));
    assert_eq!(session.delivered_output_cursor(2), Some(end));
    assert_eq!(session.flush_output_batch().unwrap().data, b"next");
    let durable = session.flush_durable_output_batch().unwrap();
    assert_eq!(durable.data, b"first\x1b]2;Background\x07next");
    assert_eq!(durable.end_cursor, end + 4);
}

#[test]
fn empty_output_does_not_arm_either_batch() {
    let mut session = test_session();
    session.attach(1);
    assert_eq!(session.append_owned_output(Vec::new()), (None, None, None));
    assert_eq!(session.output_batch.capacity(), 0);
    assert_eq!(session.durable_output_batch.capacity(), 0);
    assert!(!session.output_batch_due(0));
    assert!(!session.durable_output_batch_due(0));
    assert_eq!(session.output_stream_range(), (0, 0));
}
