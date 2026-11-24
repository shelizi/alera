use super::*;

fn sample(session_id: &str) -> TerminalHostCheckpoint {
    TerminalHostCheckpoint {
        session_id: session_id.to_string(),
        workspace_id: "ws".to_string(),
        tab_id: "tab".to_string(),
        working_directory: "/tmp/project".to_string(),
        running: true,
        exit_code: None,
        ended_at: None,
        output_stream_bytes: 42,
        updated_at: Utc.timestamp_opt(1_700_000_000, 123_000_000).unwrap(),
        buffer: Vec::new(),
    }
}

#[tokio::test]
async fn connection_pool_is_bounded_for_sqlite_worker_threads() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();

    assert_eq!(
        store.pool.options().get_max_connections(),
        HISTORY_STORE_MAX_CONNECTIONS
    );
}

#[tokio::test]
async fn upsert_append_then_read_round_trips_incremental_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();

    assert!(store.read("missing", 1024).await.unwrap().is_none());

    let checkpoint = sample("s1");
    store.upsert(checkpoint.clone()).await.unwrap();
    store.append_output("s1", 0, b"hello ").await.unwrap();
    store.append_output("s1", 1, b"world").await.unwrap();

    let read = store.read("s1", 1024).await.unwrap().unwrap();
    assert_eq!(read.session_id, checkpoint.session_id);
    assert_eq!(read.workspace_id, checkpoint.workspace_id);
    assert_eq!(read.output_stream_bytes, checkpoint.output_stream_bytes);
    assert_eq!(read.buffer, b"hello world");
}

#[tokio::test]
async fn read_orders_chunks_by_sequence_not_insert_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();

    store.append_output("s1", 1, b"world").await.unwrap();
    store.append_output("s1", 0, b"hello ").await.unwrap();

    let read = store.read("s1", 1024).await.unwrap().unwrap();
    assert_eq!(read.buffer, b"hello world");
}

#[tokio::test]
async fn next_output_sequence_advances_persisted_history() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();

    assert_eq!(store.next_output_sequence("s1").await.unwrap(), 0);
    store.append_output("s1", 0, b"first").await.unwrap();
    store.append_output("s1", 1, b"second").await.unwrap();

    assert_eq!(store.next_output_sequence("s1").await.unwrap(), 2);
}

#[tokio::test]
async fn append_output_trims_oldest_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();

    store.append_output("s1", 0, b"abc").await.unwrap();
    store.append_output("s1", 1, b"de").await.unwrap();
    store.append_output("s1", 2, b"fg").await.unwrap();

    let read = store.read("s1", 5).await.unwrap().unwrap();
    assert_eq!(read.buffer, b"cdefg");
}

#[tokio::test]
async fn cold_read_reports_maintenance_conservatively_without_extra_blob_reads() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.append_output("s1", 0, b"abc").await.unwrap();
    store.append_output("s1", 1, b"def").await.unwrap();

    for (cap, expected, maintenance) in [
        (0, b"".as_slice(), true),
        (3, b"def".as_slice(), true),
        (5, b"bcdef".as_slice(), true),
        (6, b"abcdef".as_slice(), true),
        (7, b"abcdef".as_slice(), false),
        (usize::MAX, b"abcdef".as_slice(), false),
    ] {
        let (checkpoint, required) = store
            .read_with_maintenance("s1", cap)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(Vec::from(checkpoint.buffer), expected);
        assert_eq!(required, maintenance, "cap={cap}");
    }

    // Row compaction is required even when the entire history fits.
    let count = OUTPUT_COMPACTION_ROW_THRESHOLD + 1;
    let mut transaction = store.begin_write().await.unwrap();
    sqlx::query(
        "WITH RECURSIVE nums(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM nums WHERE n < ?)
         INSERT INTO outputChunks(sessionId, sequence, createdAt, data)
         SELECT 's2', n, '2026-01-01', X'61' FROM nums",
    )
    .bind(count)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    store.upsert(sample("s2")).await.unwrap();
    let (checkpoint, required) = store
        .read_with_maintenance("s2", usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint.buffer.len(), count as usize);
    assert!(required);
    store.close_for_test().await;
}

#[tokio::test]
async fn bounded_read_does_not_mutate_persisted_history() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.append_output("s1", 0, b"abc").await.unwrap();
    store.append_output("s1", 1, b"de").await.unwrap();
    store.append_output("s1", 2, b"fg").await.unwrap();

    assert_eq!(store.read("s1", 5).await.unwrap().unwrap().buffer, b"cdefg");

    let total: i64 = sqlx::query(
        "SELECT COALESCE(SUM(length(data)), 0) AS total FROM outputChunks WHERE sessionId = ?",
    )
    .bind("s1")
    .fetch_one(&store.pool)
    .await
    .unwrap()
    .try_get("total")
    .unwrap();
    assert_eq!(total, 7);
}

#[tokio::test]
async fn append_output_keeps_tail_of_single_oversized_chunk() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();

    store.append_output("s1", 0, b"abcdefg").await.unwrap();

    let read = store.read("s1", 3).await.unwrap().unwrap();
    assert_eq!(read.buffer, b"efg");
}

#[test]
fn history_read_tail_matches_byte_suffix_for_arbitrary_chunk_boundaries() {
    // Includes empty rows, varying lengths, invalid UTF-8 and rows larger
    // than the normal coalescing target. Every cut remains a raw byte cut.
    let chunks: Vec<Vec<u8>> = (0..97_usize)
        .map(|index| {
            let len = if index == 41 {
                OUTPUT_CHUNK_TARGET_BYTES * 3 + 11
            } else {
                (index * 43) % 139
            };
            (0..len)
                .map(|offset| ((index * 17 + offset * 29) % 256) as u8)
                .collect()
        })
        .collect();
    let expected = chunks.concat();
    let mut limits: Vec<usize> = (0..256).collect();
    let mut boundary = 0;
    for chunk in chunks.iter().rev() {
        boundary += chunk.len();
        limits.extend([boundary.saturating_sub(1), boundary, boundary + 1]);
    }
    limits.extend([expected.len() + 1, usize::MAX]);
    for limit in limits {
        let mut tail = HistoryReadTail::new(limit);
        for chunk in chunks.iter().rev() {
            if tail.remaining() == 0 {
                break;
            }
            tail.prepend(chunk);
            assert!(tail.buffer.len() <= limit);
            assert!(tail.buffer.len() <= tail.len().saturating_mul(2));
        }
        assert_eq!(
            tail.finish(),
            expected[expected.len().saturating_sub(limit)..],
            "limit {limit}"
        );
    }
}

#[test]
fn history_read_tail_copies_only_requested_suffix_of_oversized_blob() {
    let oversized = vec![0x80; 2 * 1024 * 1024];
    let mut tail = HistoryReadTail::new(31);
    tail.prepend(&oversized);
    assert_eq!(tail.buffer.len(), 31);
    assert_eq!(tail.remaining(), 0);
    assert_eq!(tail.finish(), vec![0x80; 31]);

    // An unlimited caller must not turn a small history into a huge
    // allocation; no growth or finish copy is needed for a single row.
    let mut tail = HistoryReadTail::new(usize::MAX);
    tail.prepend(b"tiny");
    assert_eq!(tail.buffer.len(), 4);
    let ptr = tail.buffer.as_ptr();
    let result = tail.finish();
    assert_eq!(result.as_ptr(), ptr);
    assert_eq!(result, b"tiny");
}

#[test]
fn history_read_tail_keeps_offset_allocation_without_final_relocation() {
    let mib = 1024 * 1024;
    let cap = 4 * mib;
    let chunk_size = OUTPUT_CHUNK_TARGET_BYTES;
    let chunks = 3 * mib / chunk_size;
    let mut tail = HistoryReadTail::new(cap);
    for i in (0..chunks).rev() {
        tail.prepend(&vec![i as u8; chunk_size]);
    }
    assert_eq!(tail.len(), 3 * mib);
    assert_eq!(tail.start, mib);
    let payload_start = tail.buffer[tail.start..].as_ptr();
    let mut ring = tail.finish_ring();
    assert_eq!(ring.len(), 3 * mib);
    assert!(ring.capacity() <= cap);
    let (first, second) = ring.as_slices();
    assert!(second.is_empty());
    // Old finish copied all 3 MiB down to the allocation base.
    // The original payload pointer survives both restoration and compaction.
    assert_eq!(first.as_ptr(), payload_start);
    assert_eq!(ring.make_contiguous().as_ptr(), payload_start);
    for (i, chunk) in ring.as_slices().0.chunks(chunk_size).enumerate() {
        assert!(chunk.iter().all(|&byte| byte == i as u8));
    }
}

#[tokio::test]
async fn bounded_blob_projection_limits_sqlx_row_payload_before_tail_assembly() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.upsert(sample("other")).await.unwrap();

    let oversized: Vec<u8> = (0..8 * 1024 * 1024)
        .map(|offset| (offset % 251) as u8)
        .collect();
    let newer = [0x1b, 0xff, 0x00, 0x80, 0x07];
    store.append_output("s1", 0, &oversized).await.unwrap();
    store.append_output("s1", 1, &newer).await.unwrap();
    store.append_output("other", 2, b"unrelated").await.unwrap();

    // Inspect the SQLx row itself, not just the final Rust-side tail.
    let cap = 31;
    let rows = history_rows_query("s1", cap)
        .fetch_all(&store.pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].try_get::<&[u8], _>("data").unwrap(), &newer);
    assert_eq!(
        rows[1].try_get::<&[u8], _>("data").unwrap(),
        &oversized[oversized.len() - cap..]
    );
    let (read, maintenance_required) = store
        .read_with_maintenance("s1", cap)
        .await
        .unwrap()
        .unwrap();
    let mut expected = oversized[oversized.len() - (cap - newer.len())..].to_vec();
    expected.extend_from_slice(&newer);
    assert_eq!(Vec::from(read.buffer), expected);
    assert!(maintenance_required);

    // An unlimited reader still sees the complete unmodified persisted row.
    let rows = history_rows_query("s1", usize::MAX)
        .fetch_all(&store.pool)
        .await
        .unwrap();
    assert_eq!(rows[1].try_get::<&[u8], _>("data").unwrap(), oversized);
    store.close_for_test().await;
}

#[tokio::test]
async fn bounded_blob_projection_keeps_ordinary_and_empty_rows_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store
        .append_output("s1", 0, &[0xff, 0x00, 0x80])
        .await
        .unwrap();
    sqlx::query("INSERT INTO outputChunks (sessionId, sequence, createdAt, data) VALUES ('s1', 1, '2026-01-01', X'')")
        .execute(&store.pool)
        .await
        .unwrap();
    let rows = history_rows_query("s1", 16)
        .fetch_all(&store.pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].try_get::<&[u8], _>("data").unwrap().is_empty());
    assert_eq!(
        rows[1].try_get::<&[u8], _>("data").unwrap(),
        &[0xff, 0x00, 0x80]
    );
    let (read, maintenance_required) = store
        .read_with_maintenance("s1", 16)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(Vec::from(read.buffer), [0xff, 0x00, 0x80]);
    assert!(!maintenance_required);
    store.close_for_test().await;
}

#[tokio::test]
async fn bounded_history_read_preserves_binary_rows_and_sequence_ties() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.upsert(sample("other")).await.unwrap();

    let oversized: Vec<u8> = (0..2 * 1024 * 1024)
        .map(|offset| (offset % 256) as u8)
        .collect();
    // Out-of-order sequence values, ties ordered by row id, and raw UTF-8
    // fragments spanning rows must all retain the original byte ordering.
    store
        .append_output("s1", 4, &[0xad, 0x80, 0xff])
        .await
        .unwrap();
    store.append_output("s1", 0, &oversized).await.unwrap();
    store.append_output("s1", 3, &[0xe4, 0xb8]).await.unwrap();
    store
        .append_output("s1", 4, &[0, 0xfe, 0x61])
        .await
        .unwrap();
    store
        .append_output("other", 10, b"other history")
        .await
        .unwrap();

    let mut expected = oversized;
    expected.extend_from_slice(&[0xe4, 0xb8, 0xad, 0x80, 0xff, 0, 0xfe, 0x61]);
    for limit in [
        0,
        1,
        3,
        5,
        6,
        7,
        8,
        9,
        31,
        16_384,
        expected.len() - 1,
        usize::MAX,
    ] {
        let read = store.read("s1", limit).await.unwrap().unwrap();
        assert_eq!(
            read.buffer,
            expected[expected.len().saturating_sub(limit)..]
        );
    }
    assert_eq!(output_row_count(&store, "s1").await, 4);
    assert_eq!(
        store.read("other", 1024).await.unwrap().unwrap().buffer,
        b"other history"
    );
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), 5);
    store.close_for_test().await;
}

#[tokio::test]
async fn shrinking_the_limit_cuts_across_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.append_output("s1", 0, b"abc").await.unwrap();
    store.append_output("s1", 1, b"de").await.unwrap();
    store.append_output("s1", 2, b"fg").await.unwrap();

    // A limit above the total leaves everything alone.
    store.trim_session("s1", 9).await.unwrap();
    assert_eq!(
        store.read("s1", 100).await.unwrap().unwrap().buffer,
        b"abcdefg"
    );

    // A limit landing inside a chunk keeps that chunk's tail.
    store.trim_session("s1", 5).await.unwrap();
    assert_eq!(
        store.read("s1", 100).await.unwrap().unwrap().buffer,
        b"cdefg"
    );

    // A limit landing exactly on a boundary drops the chunk whole.
    store.trim_session("s1", 4).await.unwrap();
    assert_eq!(
        store.read("s1", 100).await.unwrap().unwrap().buffer,
        b"defg"
    );
}

#[tokio::test]
async fn trim_preserves_binary_suffix_and_other_sessions_without_extra_updates() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.upsert(sample("s2")).await.unwrap();
    store
        .append_output("s1", 0, &[0xff, 0, 0x80, 0x61])
        .await
        .unwrap();
    store.append_output("s1", 1, &[0xfe, 0]).await.unwrap();
    store.append_output("s2", 0, b"other").await.unwrap();

    // A no-op trim must not update or delete any rows.
    sqlx::query(
        "CREATE TRIGGER reject_output_update BEFORE UPDATE ON outputChunks
                 BEGIN SELECT RAISE(ABORT, 'unexpected output update'); END",
    )
    .execute(&store.pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_output_delete BEFORE DELETE ON outputChunks
                 BEGIN SELECT RAISE(ABORT, 'unexpected output delete'); END",
    )
    .execute(&store.pool)
    .await
    .unwrap();
    store.trim_session("s1", 6).await.unwrap();
    store.trim_session("s1", 100).await.unwrap();
    store.trim_session("missing", 0).await.unwrap();
    sqlx::query("DROP TRIGGER reject_output_update")
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("DROP TRIGGER reject_output_delete")
        .execute(&store.pool)
        .await
        .unwrap();

    store.trim_session("s1", 5).await.unwrap();
    assert_eq!(
        store.read("s1", 100).await.unwrap().unwrap().buffer,
        vec![0, 0x80, 0x61, 0xfe, 0]
    );
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), 2);
    store.trim_session("s1", 0).await.unwrap();
    assert!(store
        .read("s1", 100)
        .await
        .unwrap()
        .unwrap()
        .buffer
        .is_empty());
    assert_eq!(
        store.read("s2", 100).await.unwrap().unwrap().buffer,
        b"other"
    );
}

const LEGACY_TRIM_BOUNDARY_SQL: &str = "SELECT id, sequence, byteLen, running FROM ( \
       SELECT id, sequence, length(data) AS byteLen, \
              SUM(length(data)) OVER (ORDER BY sequence DESC, id DESC) AS running \
       FROM outputChunks WHERE sessionId = ? \
     ) WHERE running > ? ORDER BY sequence DESC, id DESC LIMIT 1";

#[tokio::test]
async fn indexed_trim_boundary_matches_legacy_for_ties_empty_rows_and_binary_suffixes() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.upsert(sample("other")).await.unwrap();
    store.append_output("other", 0, b"keep me").await.unwrap();

    // Deliberately insert sequences out of order, include ties and empty
    // rows that cannot be produced by a modern PTY but may exist on disk.
    let chunks: &[(i64, &[u8])] = &[
        (5, &[0xff, 0, 0x80, b'x']),
        (2, b""),
        (1, b"ab"),
        (5, b""),
        (3, &[0, 0xfe]),
        (5, b"last"),
        (0, b""),
        (6, b""),
        (3, b"c"),
        (6, b""),
        (-1, b"first"),
    ];
    for (sequence, data) in chunks {
        sqlx::query("INSERT INTO outputChunks (sessionId, sequence, createdAt, data) VALUES ('s1', ?, '', ?)")
            .bind(sequence)
            .bind(data)
            .execute(&store.pool)
            .await.unwrap();
    }
    let expected = store.read_buffer("s1", usize::MAX).await.unwrap();
    let mut transaction = store.begin_write().await.unwrap();
    for cap in 0..expected.len() {
        let row = sqlx::query(LEGACY_TRIM_BOUNDARY_SQL)
            .bind("s1")
            .bind(cap as i64)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        let len: i64 = row.try_get("byteLen").unwrap();
        let running: i64 = row.try_get("running").unwrap();
        let excess_bytes = match trim_action(&mut *transaction, "s1", cap).await.unwrap() {
            TrimAction::Cut { excess_bytes } => excess_bytes,
            other => panic!("unexpected trim action: {other:?}"),
        };
        let boundary = find_trim_boundary(&mut transaction, "s1", excess_bytes, cap)
            .await
            .unwrap();
        assert_eq!(
            boundary.id,
            row.try_get::<i64, _>("id").unwrap(),
            "cap={cap}"
        );
        assert_eq!(
            boundary.sequence,
            row.try_get::<i64, _>("sequence").unwrap(),
            "cap={cap}"
        );
        assert_eq!(
            boundary.retained_bytes,
            cap as i64 - (running - len),
            "cap={cap}"
        );
    }
    assert_eq!(
        trim_action(&mut *transaction, "s1", expected.len())
            .await
            .unwrap(),
        TrimAction::None
    );
    assert_eq!(
        trim_action(&mut *transaction, "s1", usize::MAX)
            .await
            .unwrap(),
        TrimAction::None
    );
    transaction.rollback().await.unwrap();

    // Exercise real range deletes at every smaller cap, including zero.
    for cap in (0..expected.len()).rev() {
        store.trim_session("s1", cap).await.unwrap();
        assert_eq!(
            store.read_buffer("s1", usize::MAX).await.unwrap(),
            expected[expected.len() - cap..],
            "cap={cap}"
        );
        assert_eq!(
            store.read_buffer("other", usize::MAX).await.unwrap(),
            b"keep me"
        );
    }
    store.close_for_test().await;
}

#[tokio::test]
async fn trim_uses_existing_index_and_visits_only_small_expired_or_retained_side() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    let count = OUTPUT_COMPACTION_ROW_THRESHOLD;
    let data = vec![b'x'; 256];
    let mut transaction = store.begin_write().await.unwrap();
    for sequence in 0..count {
        sqlx::query("INSERT INTO outputChunks (sessionId, sequence, createdAt, data) VALUES ('s1', ?, '', ?)")
            .bind(sequence).bind(&data).execute(&mut *transaction).await.unwrap();
    }
    let total = count as usize * data.len();
    let action = trim_action(&mut *transaction, "s1", total - 17)
        .await
        .unwrap();
    assert_eq!(action, TrimAction::Cut { excess_bytes: 17 });
    let boundary = find_trim_boundary(&mut transaction, "s1", 17, total - 17)
        .await
        .unwrap();
    assert_eq!(boundary.sequence, 0);
    assert_eq!(boundary.retained_bytes, 239);
    assert_eq!(boundary.visited_rows, 1);
    let boundary = find_trim_boundary(&mut transaction, "s1", (total - 17) as i64, 17)
        .await
        .unwrap();
    assert_eq!(boundary.sequence, count - 1);
    assert_eq!(boundary.retained_bytes, 17);
    assert_eq!(boundary.visited_rows, 1);

    for sql in [
        "EXPLAIN QUERY PLAN SELECT id, sequence, length(data) AS byteLen FROM outputChunks WHERE sessionId = ? ORDER BY sequence ASC, id ASC",
        "EXPLAIN QUERY PLAN SELECT id, sequence, length(data) AS byteLen FROM outputChunks WHERE sessionId = ? ORDER BY sequence DESC, id DESC",
    ] {
        let rows = sqlx::query(sql)
            .bind("s1")
            .fetch_all(&mut *transaction)
            .await
            .unwrap();
        let plan = rows
            .iter()
            .map(|row| row.try_get::<String, _>("detail").unwrap())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(plan.contains("outputChunksSessionIdSequenceIdx"), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
    sqlx::query(
        "INSERT INTO outputChunks (sessionId, sequence, createdAt, data) VALUES ('s1', ?, '', ?)",
    )
    .bind(count)
    .bind(&data)
    .execute(&mut *transaction)
    .await
    .unwrap();
    // Even if the byte total is below the requested cap, the row count
    // threshold still schedules compaction.
    assert_eq!(
        trim_action(&mut *transaction, "s1", usize::MAX)
            .await
            .unwrap(),
        TrimAction::Compact
    );
    transaction.commit().await.unwrap();
    store.trim_session("s1", 17).await.unwrap();
    assert_eq!(
        store.read_buffer("s1", usize::MAX).await.unwrap(),
        vec![b'x'; 17]
    );
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), count + 1);
    store.close_for_test().await;
}

/// Repeat the actual old window query and new indexed cut on the same
/// populated SQLite database. No writes are timed: this isolates the cost
/// of finding the retention boundary, not end-to-end persistence.
#[tokio::test]
#[ignore = "manual retention scan microbenchmark"]
async fn benchmark_indexed_retention_boundary() {
    use std::time::Instant;

    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    let count = OUTPUT_COMPACTION_ROW_THRESHOLD;
    let data = vec![b'x'; 4096];
    let mut transaction = store.begin_write().await.unwrap();
    for sequence in 0..count {
        sqlx::query("INSERT INTO outputChunks (sessionId, sequence, createdAt, data) VALUES ('s1', ?, '', ?)")
            .bind(sequence).bind(&data).execute(&mut *transaction).await.unwrap();
    }
    let total = count as usize * data.len();
    async fn legacy(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, cap: usize) -> (i64, i64, i64) {
        let row = sqlx::query(LEGACY_TRIM_BOUNDARY_SQL)
            .bind("s1")
            .bind(cap as i64)
            .fetch_one(&mut **tx)
            .await
            .unwrap();
        let running: i64 = row.try_get("running").unwrap();
        let len: i64 = row.try_get("byteLen").unwrap();
        (
            row.try_get("id").unwrap(),
            row.try_get("sequence").unwrap(),
            cap as i64 - (running - len),
        )
    }
    async fn indexed(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        total: usize,
        cap: usize,
    ) -> (i64, i64, i64) {
        let b = find_trim_boundary(tx, "s1", (total - cap) as i64, cap)
            .await
            .unwrap();
        (b.id, b.sequence, b.retained_bytes)
    }
    for (label, cap) in [
        ("steady_trim", total - 17),
        ("shrink_to_17", 17),
        ("shrink_half", total / 2),
    ] {
        let expected = legacy(&mut transaction, cap).await;
        assert_eq!(indexed(&mut transaction, total, cap).await, expected);
        for _ in 0..3 {
            assert_eq!(legacy(&mut transaction, cap).await, expected);
            assert_eq!(indexed(&mut transaction, total, cap).await, expected);
        }
        let mut before = Vec::new();
        let mut after = Vec::new();
        const CALLS: usize = 30;
        for round in 0..7 {
            for new in [round % 2 != 0, round % 2 == 0] {
                let started = Instant::now();
                for _ in 0..CALLS {
                    let result = if new {
                        indexed(&mut transaction, total, cap).await
                    } else {
                        legacy(&mut transaction, cap).await
                    };
                    assert_eq!(result, expected);
                }
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                if new {
                    after.push(elapsed_ms);
                } else {
                    before.push(elapsed_ms);
                }
            }
        }
        before.sort_by(f64::total_cmp);
        after.sort_by(f64::total_cmp);
        let boundary = find_trim_boundary(&mut transaction, "s1", (total - cap) as i64, cap)
            .await
            .unwrap();
        eprintln!("retention_bench {label} rows={count} visited={} calls={CALLS} old_ms={:.4} new_ms={:.4}",
            boundary.visited_rows,before[3],after[3]);
    }
    transaction.rollback().await.unwrap();
    store.close_for_test().await;
}

#[tokio::test]
async fn shrinking_session_limit_trims_existing_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.append_output("s1", 0, b"abcdefgh").await.unwrap();

    store.trim_session("s1", 3).await.unwrap();

    let read = store.read("s1", 3).await.unwrap().unwrap();
    assert_eq!(read.buffer, b"fgh");
}

#[tokio::test]
async fn delete_removes_metadata_and_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.append_output("s1", 0, b"hello").await.unwrap();

    store.delete("s1").await.unwrap();

    assert!(store.read("s1", 1024).await.unwrap().is_none());
}

#[tokio::test]
async fn legacy_buffer_checkpoint_schema_is_destroyed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(HISTORY_DATABASE_FILE_NAME);
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE checkpoints (sessionId TEXT PRIMARY KEY, buffer BLOB NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO checkpoints (sessionId, buffer) VALUES ('old', x'4142')")
        .execute(&pool)
        .await
        .unwrap();
    drop(pool);

    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();

    assert!(store.read("old", 1024).await.unwrap().is_none());
    let columns = sqlx::query("PRAGMA table_info(checkpoints)")
        .fetch_all(&store.pool)
        .await
        .unwrap();
    assert!(!columns
        .iter()
        .any(|row| row.try_get::<String, _>("name").unwrap() == "buffer"));
}

async fn output_row_count(store: &TerminalHostHistoryStore, session_id: &str) -> i64 {
    sqlx::query("SELECT COUNT(*) AS rowCount FROM outputChunks WHERE sessionId = ?")
        .bind(session_id)
        .fetch_one(&store.pool)
        .await
        .unwrap()
        .try_get("rowCount")
        .unwrap()
}

/// Count commit markers in a freshly truncated SQLite WAL. Each frame
/// starts with the page number and a nonzero database size on commit.
fn wal_commit_frames(runtime_dir: &Path) -> (usize, usize, usize) {
    let wal = std::fs::read(runtime_dir.join(format!("{HISTORY_DATABASE_FILE_NAME}-wal"))).unwrap();
    if wal.is_empty() {
        return (0, 0, 0);
    }
    assert!(wal.len() >= 32);
    let magic = u32::from_be_bytes(wal[..4].try_into().unwrap());
    assert!(matches!(magic, 0x377f0682 | 0x377f0683));
    let page_size = u32::from_be_bytes(wal[8..12].try_into().unwrap()) as usize;
    assert!(page_size.is_power_of_two() && page_size >= 512);
    let frames = wal[32..].chunks_exact(24 + page_size);
    assert!(frames.remainder().is_empty());
    let frame_count = frames.len();
    let commits = frames
        .filter(|frame| u32::from_be_bytes(frame[4..8].try_into().unwrap()) != 0)
        .count();
    (commits, frame_count, wal.len())
}

#[tokio::test]
async fn combined_checkpoint_and_trim_uses_one_wal_commit_instead_of_two() {
    async fn run(combined: bool) -> (usize, usize, usize) {
        let dir = tempfile::tempdir().unwrap();
        let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
        store
            .persist_session("s1", Some(&sample("s1")), &[(0, b"abcdef")])
            .await
            .unwrap();
        let reset: (i64, i64, i64) = sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(reset.0, 0);
        assert_eq!(wal_commit_frames(dir.path()), (0, 0, 0));
        let mut metadata = sample("s1");
        metadata.output_stream_bytes = 11;
        let batch = [(1, b"ghijk")];
        if combined {
            store
                .persist_session_with_trim("s1", Some(&metadata), &batch, Some(5))
                .await
                .unwrap();
        } else {
            store
                .persist_session("s1", Some(&metadata), &batch)
                .await
                .unwrap();
            store.trim_session("s1", 5).await.unwrap();
        }
        let persisted = store.read("s1", usize::MAX).await.unwrap().unwrap();
        assert_eq!(persisted.buffer, b"ghijk");
        assert_eq!(persisted.output_stream_bytes, 11);
        assert_eq!(store.next_output_sequence("s1").await.unwrap(), 2);
        let result = wal_commit_frames(dir.path());
        // An empty batch and a trim-only no-op must not create another commit.
        store
            .persist_session_with_trim("s1", None, &[(2, b"" as &[u8])], Some(5))
            .await
            .unwrap();
        assert_eq!(wal_commit_frames(dir.path()), result);
        store.close_for_test().await;
        result
    }

    let separate = run(false).await;
    let combined = run(true).await;
    eprintln!(
        "separate commits={} frames={} wal_bytes={}; combined commits={} frames={} wal_bytes={}",
        separate.0, separate.1, separate.2, combined.0, combined.1, combined.2
    );
    assert_eq!(separate.0, 2);
    assert_eq!(combined.0, 1);
}

#[tokio::test]
async fn combined_trim_failure_rolls_back_partial_cut_but_preserves_output_for_retry() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    store.append_output("s1", 0, b"abcdefgh").await.unwrap();
    store.append_output("s1", 1, b"ijklmnop").await.unwrap();
    store.append_output("s1", 2, b"qrst").await.unwrap();
    store.upsert(sample("other")).await.unwrap();
    store
        .append_output("other", 0, b"other history")
        .await
        .unwrap();
    // The boundary row is shortened before the older rows are deleted.
    // Reject the delete so the combined attempt must undo that partial cut.
    sqlx::query(
        "CREATE TRIGGER reject_trim_delete BEFORE DELETE ON outputChunks
                 WHEN OLD.sessionId = 's1' BEGIN
                 SELECT RAISE(ABORT, 'trim unavailable'); END",
    )
    .execute(&store.pool)
    .await
    .unwrap();
    let mut metadata = sample("s1");
    metadata.output_stream_bytes = 23;
    metadata.working_directory = "/tmp/new-location".into();
    let batch = [(3, b"uvw")];
    let error = store
        .persist_session_with_trim("s1", Some(&metadata), &batch, Some(10))
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("trim failed after saving output"));
    let persisted = store.read("s1", usize::MAX).await.unwrap().unwrap();
    assert_eq!(persisted.buffer, b"abcdefghijklmnopqrstuvw");
    assert_eq!(persisted.output_stream_bytes, 23);
    assert_eq!(persisted.working_directory, metadata.working_directory);
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), 4);

    sqlx::query("DROP TRIGGER reject_trim_delete")
        .execute(&store.pool)
        .await
        .unwrap();
    // Retry the exact same snapshot; already committed output is not doubled.
    store
        .persist_session_with_trim("s1", Some(&metadata), &batch, Some(10))
        .await
        .unwrap();
    assert_eq!(
        store.read("s1", usize::MAX).await.unwrap().unwrap().buffer,
        b"nopqrstuvw"
    );
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), 4);
    assert_eq!(
        store
            .read("other", usize::MAX)
            .await
            .unwrap()
            .unwrap()
            .buffer,
        b"other history"
    );
    store.close_for_test().await;
}

#[tokio::test]
async fn combined_compaction_sees_uncommitted_rows_and_keeps_newest_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    let rows = OUTPUT_COMPACTION_ROW_THRESHOLD + 10;
    for sequence in 0..rows {
        store
            .append_output("s1", sequence, format!("{sequence};").as_bytes())
            .await
            .unwrap();
    }
    let mut expected = store.read("s1", usize::MAX).await.unwrap().unwrap().buffer;
    let data = [0x80, 0xff, 0, b'x', b'y'];
    expected.extend_from_slice(&data);
    let mut metadata = sample("s1");
    metadata.output_stream_bytes = expected.len() as u64;
    store
        .persist_session_with_trim("s1", Some(&metadata), &[(rows, data)], Some(5000))
        .await
        .unwrap();
    let persisted = store.read("s1", usize::MAX).await.unwrap().unwrap();
    assert_eq!(persisted.buffer, expected[expected.len() - 5000..]);
    assert_eq!(persisted.output_stream_bytes, metadata.output_stream_bytes);
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), rows + 1);
    assert_eq!(output_row_count(&store, "s1").await, 1);
    store.close_for_test().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_combined_persists_keep_session_tails_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    for session in ["s1", "s2"] {
        store.upsert(sample(session)).await.unwrap();
    }
    let writer = |session: &'static str, tag: u8| {
        let store = store.clone();
        tokio::spawn(async move {
            let mut expected = Vec::new();
            for sequence in 0..100_i64 {
                let batch = [tag, sequence as u8, 0xff, 0];
                expected.extend_from_slice(&batch);
                let mut metadata = sample(session);
                metadata.output_stream_bytes = expected.len() as u64;
                store
                    .persist_session_with_trim(
                        session,
                        Some(&metadata),
                        &[(sequence, batch)],
                        Some(63),
                    )
                    .await?;
            }
            anyhow::Ok(expected)
        })
    };
    let (first, second) = tokio::join!(writer("s1", b'a'), writer("s2", b'b'));
    for (session, expected) in [
        ("s1", first.unwrap().unwrap()),
        ("s2", second.unwrap().unwrap()),
    ] {
        let persisted = store.read(session, usize::MAX).await.unwrap().unwrap();
        assert_eq!(persisted.buffer, expected[expected.len() - 63..]);
        assert_eq!(persisted.output_stream_bytes, expected.len() as u64);
        assert_eq!(store.next_output_sequence(session).await.unwrap(), 100);
    }
    store.close_for_test().await;
}

#[tokio::test]
async fn batched_insert_respects_row_and_payload_limits_and_preserves_binary_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let created_at = "2026-11-24T00:00:00.000Z";
    assert_eq!(
        insert_output_rows(&mut tx, "s1", created_at, &[])
            .await
            .unwrap(),
        0
    );

    let tiny: Vec<Vec<u8>> = (0..OUTPUT_INSERT_BATCH_ROWS * 2 + 1)
        .map(|i| vec![(i % 256) as u8, 0xff, 0])
        .collect();
    assert_eq!(
        insert_output_rows(
            &mut tx,
            "s1",
            created_at,
            &tiny
                .iter()
                .enumerate()
                .map(|(i, data)| (i as i64, data.as_slice()))
                .collect::<Vec<_>>()
        )
        .await
        .unwrap(),
        3
    );
    // Fit exactly one full payload batch, then force two independent large
    // rows and an oversized row that must stay intact. Identical sequences
    // still use insertion id as the chronological tie breaker.
    let large = vec![
        vec![0x80; OUTPUT_INSERT_BATCH_BYTES / 2],
        vec![0x81; OUTPUT_INSERT_BATCH_BYTES / 2],
        vec![0x82; OUTPUT_INSERT_BATCH_BYTES / 2 + 1],
        vec![0x83; OUTPUT_INSERT_BATCH_BYTES / 2 + 1],
        vec![0x84; OUTPUT_INSERT_BATCH_BYTES + 17],
        vec![0xff, 0, b'x'],
    ];
    let next = tiny.len() as i64;
    assert_eq!(
        insert_output_rows(
            &mut tx,
            "s1",
            created_at,
            &large
                .iter()
                .map(|data| (next, data.as_slice()))
                .collect::<Vec<_>>()
        )
        .await
        .unwrap(),
        5
    );
    tx.commit().await.unwrap();

    let expected = tiny
        .iter()
        .chain(&large)
        .flat_map(|data| data.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(store.read_buffer("s1", usize::MAX).await.unwrap(), expected);
    let lengths: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT sequence, length(data), createdAt FROM outputChunks
         WHERE sessionId = 's1' ORDER BY sequence, id",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    assert_eq!(lengths.len(), tiny.len() + large.len());
    for (i, (sequence, len, timestamp)) in lengths.iter().enumerate() {
        let expected_sequence = if i < tiny.len() { i as i64 } else { next };
        let expected_len = if i < tiny.len() {
            tiny[i].len()
        } else {
            large[i - tiny.len()].len()
        };
        assert_eq!(*sequence, expected_sequence);
        assert_eq!(*len, expected_len as i64);
        assert_eq!(timestamp, created_at);
    }
    store.close_for_test().await;
}

#[tokio::test]
async fn failed_later_insert_batch_rolls_back_earlier_inserts_and_checkpoint_update() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    let original = sample("s1");
    store
        .persist_session("s1", Some(&original), &[(0, b"seed")])
        .await
        .unwrap();
    store.upsert(sample("other")).await.unwrap();
    store
        .append_output("other", 0, b"other history")
        .await
        .unwrap();

    let mut outputs = vec![(1, b"+".to_vec())];
    for sequence in 2..(OUTPUT_INSERT_BATCH_ROWS as i64 + 8) {
        let mut data = vec![sequence as u8; OUTPUT_CHUNK_TARGET_BYTES];
        data[0] = 0xff;
        data[1] = 0;
        outputs.push((sequence, data));
    }
    // The first full batch (sequences 2 through 65) succeeds, then the
    // second fails. The existing-row update and metadata must roll back too.
    let fail_at = OUTPUT_INSERT_BATCH_ROWS as i64 + 2;
    sqlx::query("CREATE TABLE rejectedSequence (sequence INTEGER NOT NULL)")
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO rejectedSequence VALUES (?)")
        .bind(fail_at)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_later_batch BEFORE INSERT ON outputChunks
         WHEN NEW.sessionId = 's1' AND NEW.sequence = (SELECT sequence FROM rejectedSequence)
         BEGIN SELECT RAISE(ABORT, 'later output batch rejected'); END",
    )
    .execute(&store.pool)
    .await
    .unwrap();
    let mut updated = original.clone();
    updated.working_directory = "/tmp/updated".into();
    let mut expected = b"seed".to_vec();
    for (_, data) in &outputs {
        expected.extend_from_slice(data);
    }
    updated.output_stream_bytes = expected.len() as u64;
    assert!(store
        .persist_session("s1", Some(&updated), &outputs)
        .await
        .is_err());

    let persisted = store.read("s1", usize::MAX).await.unwrap().unwrap();
    assert_eq!(persisted.buffer, b"seed");
    assert_eq!(persisted.working_directory, original.working_directory);
    assert_eq!(persisted.output_stream_bytes, original.output_stream_bytes);
    assert_eq!(output_row_count(&store, "s1").await, 1);
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), 1);

    sqlx::query("DROP TRIGGER reject_later_batch")
        .execute(&store.pool)
        .await
        .unwrap();
    for _ in 0..2 {
        store
            .persist_session("s1", Some(&updated), &outputs)
            .await
            .unwrap();
        let persisted = store.read("s1", usize::MAX).await.unwrap().unwrap();
        assert_eq!(persisted.buffer, expected);
        assert_eq!(persisted.working_directory, updated.working_directory);
        assert_eq!(persisted.output_stream_bytes, updated.output_stream_bytes);
        assert_eq!(
            store.next_output_sequence("s1").await.unwrap(),
            outputs.last().unwrap().0 + 1
        );
    }
    assert_eq!(output_row_count(&store, "s1").await, outputs.len() as i64);
    assert_eq!(
        store.read_buffer("other", usize::MAX).await.unwrap(),
        b"other history"
    );
    store.close_for_test().await;
}

#[tokio::test]
async fn compaction_rewrites_multiple_insert_batches_without_losing_the_retained_suffix() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    let count = OUTPUT_COMPACTION_ROW_THRESHOLD + 7;
    let data: Vec<Vec<u8>> = (0..count)
        .map(|sequence| {
            let mut chunk = vec![sequence as u8; 1024];
            chunk[..8].copy_from_slice(&sequence.to_le_bytes());
            chunk
        })
        .collect();
    let mut tx = store.begin_write().await.unwrap();
    insert_output_rows(
        &mut tx,
        "s1",
        "",
        &data
            .iter()
            .enumerate()
            .map(|(sequence, bytes)| (sequence as i64, bytes.as_slice()))
            .collect::<Vec<_>>(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(output_row_count(&store, "s1").await, count);
    let mut expected = data.concat();
    let latest = [0xff, 0, 0x80, b'x'];
    expected.extend_from_slice(&latest);
    let cap = OUTPUT_CHUNK_TARGET_BYTES * (OUTPUT_INSERT_BATCH_ROWS + 9) + 13;
    assert!(expected.len() > cap);
    store
        .persist_session_with_trim("s1", None, &[(count, latest)], Some(cap))
        .await
        .unwrap();
    assert_eq!(
        store.read_buffer("s1", usize::MAX).await.unwrap(),
        expected[expected.len() - cap..]
    );
    assert_eq!(
        output_row_count(&store, "s1").await,
        cap.div_ceil(OUTPUT_CHUNK_TARGET_BYTES) as i64
    );
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), count + 1);
    store.close_for_test().await;
}

/// Measure the actual SQLite insertion statements, excluding transaction
/// admission and rollback so the result isolates database worker round trips.
#[tokio::test]
#[ignore = "manual output insert microbenchmark"]
async fn benchmark_batched_history_insert() {
    use std::time::Instant;

    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    let data = vec![b'x'; OUTPUT_CHUNK_TARGET_BYTES];
    async fn run(
        store: &TerminalHostHistoryStore,
        data: &[u8],
        rows: usize,
        calls: usize,
        batched: bool,
    ) -> (f64, usize) {
        let mut elapsed = std::time::Duration::ZERO;
        let mut statements = 0;
        for _ in 0..calls {
            let mut tx = store.begin_write().await.unwrap();
            let started = Instant::now();
            if batched {
                statements += insert_output_rows(
                    &mut tx,
                    "s1",
                    "",
                    &(0..rows)
                        .map(|sequence| (sequence as i64, data))
                        .collect::<Vec<_>>(),
                )
                .await
                .unwrap();
            } else {
                for sequence in 0..rows {
                    sqlx::query(
                        "INSERT INTO outputChunks (sessionId, sequence, createdAt, data) VALUES (?, ?, ?, ?)")
                        .bind("s1").bind(sequence as i64).bind("").bind(data)
                        .execute(&mut *tx).await.unwrap();
                    statements += 1;
                }
            }
            elapsed += started.elapsed();
            let (actual_rows, actual_bytes, newest): (i64, i64, i64) = sqlx::query_as(
                "SELECT COUNT(*), SUM(length(data)), MAX(sequence) FROM outputChunks WHERE sessionId = 's1'")
                .fetch_one(&mut *tx).await.unwrap();
            assert_eq!(actual_rows, rows as i64);
            assert_eq!(actual_bytes, (rows * data.len()) as i64);
            assert_eq!(newest, rows as i64 - 1);
            tx.rollback().await.unwrap();
        }
        (elapsed.as_secs_f64() * 1000.0, statements)
    }
    for (rows, calls) in [(1, 128), (64, 16), (512, 4)] {
        for _ in 0..2 {
            run(&store, &data, rows, calls, false).await;
            run(&store, &data, rows, calls, true).await;
        }
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut old_statements = 0;
        let mut new_statements = 0;
        for round in 0..7 {
            for batched in [round % 2 != 0, round % 2 == 0] {
                let (ms, statements) = run(&store, &data, rows, calls, batched).await;
                if batched {
                    new_statements = statements;
                    after.push(ms);
                } else {
                    old_statements = statements;
                    before.push(ms);
                }
            }
        }
        before.sort_by(f64::total_cmp);
        after.sort_by(f64::total_cmp);
        assert_eq!(old_statements, rows * calls);
        assert_eq!(
            new_statements,
            rows.div_ceil(OUTPUT_INSERT_BATCH_ROWS) * calls
        );
        eprintln!("insert_bench rows={rows} calls={calls} old_statements={old_statements} new_statements={new_statements} old_ms={:.4} new_ms={:.4}",
            before[3],after[3]);
    }
    store.close_for_test().await;
}

#[tokio::test]
async fn sqlite_blob_append_preserves_binary_values_boundaries_and_replay() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    assert!(store.blob_concat_safe);
    let seed: Vec<u8> = (0..OUTPUT_CHUNK_TARGET_BYTES - 5)
        .map(|i| i as u8)
        .collect();
    store
        .persist_session("s1", Some(&sample("s1")), &[(0, seed.clone())])
        .await
        .unwrap();
    let outputs = [
        (0, b"replayed seed".to_vec()),
        (1, Vec::new()),
        (2, vec![0xff, 0, 0x80]),
        (3, vec![0xfe]),
        // This no longer fits in the first row. Subsequent smaller
        // output must stay with the new row, not jump backwards.
        (4, vec![0, 0xfd, b'x', 0xfc]),
        (5, vec![0]),
        (6, vec![0x81; OUTPUT_CHUNK_TARGET_BYTES + 3]),
        (7, vec![0xff]),
    ];
    store.persist_session("s1", None, &outputs).await.unwrap();
    let mut expected_rows = vec![seed, Vec::new(), outputs[6].1.clone(), outputs[7].1.clone()];
    expected_rows[0].extend_from_slice(&outputs[2].1);
    expected_rows[0].extend_from_slice(&outputs[3].1);
    expected_rows[1].extend_from_slice(&outputs[4].1);
    expected_rows[1].extend_from_slice(&outputs[5].1);
    let rows: Vec<(i64, String, i64, Vec<u8>)> = sqlx::query_as(
        "SELECT sequence, typeof(data), length(data), data FROM outputChunks
         WHERE sessionId = 's1' ORDER BY sequence, id",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), expected_rows.len());
    for ((sequence, storage_class, len, data), (expected_sequence, expected)) in rows
        .iter()
        .zip([3, 5, 6, 7].into_iter().zip(&expected_rows))
    {
        assert_eq!(*sequence, expected_sequence);
        assert_eq!(storage_class, "blob");
        assert_eq!(*len, expected.len() as i64);
        assert_eq!(data, expected);
    }
    sqlx::query(
        "CREATE TRIGGER reject_replay_update BEFORE UPDATE ON outputChunks
                 BEGIN SELECT RAISE(ABORT, 'replay must not update'); END",
    )
    .execute(&store.pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_replay_insert BEFORE INSERT ON outputChunks
                 BEGIN SELECT RAISE(ABORT, 'replay must not insert'); END",
    )
    .execute(&store.pool)
    .await
    .unwrap();
    store.persist_session("s1", None, &outputs).await.unwrap();
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), 8);
    assert_eq!(
        store.read_buffer("s1", usize::MAX).await.unwrap(),
        expected_rows.concat()
    );
    store.close_for_test().await;
}

#[tokio::test]
async fn sqlite_blob_append_preserves_odd_byte_lengths_in_legacy_utf16_databases() {
    for pragma in [
        "PRAGMA encoding = 'UTF-16le'",
        "PRAGMA encoding = 'UTF-16be'",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let options = SqliteConnectOptions::new()
            .filename(dir.path().join(HISTORY_DATABASE_FILE_NAME))
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query(pragma).execute(&pool).await.unwrap();
        // Freeze the encoding before the history store creates its tables.
        sqlx::query("CREATE TABLE encoding_fixture (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
        assert!(!store.blob_concat_safe);
        let encoding: String = sqlx::query_scalar("PRAGMA encoding")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert!(encoding.starts_with("UTF-16"));
        store
            .persist_session("s1", Some(&sample("s1")), &[(0, &[0xff, 0][..])])
            .await
            .unwrap();
        // The old prefix plus the new bytes has odd length. The legacy
        // fallback must not allow SQLite concat to truncate the last byte.
        store
            .persist_session("s1", None, &[(1, &[0x80][..])])
            .await
            .unwrap();
        store
            .persist_session("s1", None, &[(2, &[0, 0xfe][..])])
            .await
            .unwrap();
        let row: (String, i64, Vec<u8>) = sqlx::query_as(
            "SELECT typeof(data), length(data), data FROM outputChunks WHERE sessionId = 's1'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(row, ("blob".to_string(), 5, vec![0xff, 0, 0x80, 0, 0xfe]));
        assert_eq!(store.next_output_sequence("s1").await.unwrap(), 3);
        store.trim_session("s1", 3).await.unwrap();
        assert_eq!(
            store.read_buffer("s1", usize::MAX).await.unwrap(),
            vec![0x80, 0, 0xfe]
        );
        store.close_for_test().await;
    }
}

/// Benchmark the previous actual SELECT/decode/extend/UPDATE against
/// production append_output_rows. Both execute inside the same rolled-back
/// transactions; WAL commits and fsync are deliberately outside timing.
#[tokio::test]
#[ignore = "manual BLOB append microbenchmark"]
async fn benchmark_sqlite_blob_append() {
    use std::time::Instant;

    async fn legacy_append(
        transaction: &mut sqlx::Transaction<'_, Sqlite>,
        sequence: i64,
        suffix: &[u8],
    ) -> (usize, usize) {
        let has_checkpoint = sqlx::query("SELECT 1 FROM checkpoints WHERE sessionId = ?")
            .bind("s1")
            .fetch_optional(&mut **transaction)
            .await
            .unwrap()
            .is_some();
        assert!(has_checkpoint);
        let row = sqlx::query(
            "SELECT id, sequence, CASE WHEN length(data) < ? THEN data END AS appendableData \
             FROM outputChunks WHERE sessionId = ? ORDER BY sequence DESC, id DESC LIMIT 1",
        )
        .bind(OUTPUT_CHUNK_TARGET_BYTES as i64)
        .bind("s1")
        .fetch_one(&mut **transaction)
        .await
        .unwrap();
        let newest: i64 = row.try_get("sequence").unwrap();
        let mut data: Vec<u8> = row
            .try_get::<Option<Vec<u8>>, _>("appendableData")
            .unwrap()
            .unwrap();
        let read_bytes = data.len();
        let mut write_bytes = 0;
        let created_at = format_timestamp(Utc::now());
        if sequence > newest {
            assert!(data.len() + suffix.len() <= OUTPUT_CHUNK_TARGET_BYTES);
            data.extend_from_slice(suffix);
            write_bytes = data.len();
            sqlx::query(
                "UPDATE outputChunks SET sequence = ?, createdAt = ?, data = ? WHERE id = ?",
            )
            .bind(sequence)
            .bind(&created_at)
            .bind(&data)
            .bind(row.try_get::<i64, _>("id").unwrap())
            .execute(&mut **transaction)
            .await
            .unwrap();
        }
        (read_bytes, write_bytes)
    }

    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    assert!(store.blob_concat_safe);
    let seed: Vec<u8> = (0..8192).map(|i| i as u8).collect();
    let suffix: Vec<u8> = (0..32).map(|i| (255 - i) as u8).collect();
    store
        .persist_session("s1", Some(&sample("s1")), &[(0, &seed)])
        .await
        .unwrap();
    const CALLS: usize = 128;
    for replay in [false, true] {
        let mut expected = seed.clone();
        if !replay {
            for _ in 0..CALLS {
                expected.extend_from_slice(&suffix);
            }
        }
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut old_read = 0;
        let mut old_write = 0;
        for round in 0..9 {
            for optimized in [round % 2 != 0, round % 2 == 0] {
                let mut transaction = store.begin_write().await.unwrap();
                let started = Instant::now();
                let mut read_bytes = 0;
                let mut write_bytes = 0;
                for index in 0..CALLS {
                    let sequence = if replay { 0 } else { index as i64 + 1 };
                    if optimized {
                        append_output_rows(
                            &mut transaction,
                            "s1",
                            &[(sequence, suffix.as_slice())],
                            store.blob_concat_safe,
                        )
                        .await
                        .unwrap();
                    } else {
                        let (read, written) =
                            legacy_append(&mut transaction, sequence, &suffix).await;
                        read_bytes += read;
                        write_bytes += written;
                    }
                }
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(
                    read_buffer_from(&mut *transaction, "s1", usize::MAX)
                        .await
                        .unwrap(),
                    expected
                );
                transaction.rollback().await.unwrap();
                if round >= 2 {
                    if optimized {
                        after.push(ms);
                    } else {
                        before.push(ms);
                        old_read = read_bytes;
                        old_write = write_bytes;
                    }
                }
            }
        }
        before.sort_by(f64::total_cmp);
        after.sort_by(f64::total_cmp);
        let new_write = if replay { 0 } else { CALLS * suffix.len() };
        eprintln!("blob_append_bench replay={replay} calls={CALLS} old_read_bytes={old_read} old_bound_bytes={old_write} new_read_bytes=0 new_bound_bytes={new_write} old_ms={:.4} new_ms={:.4}", before[3], after[3]);
    }
    store.close_for_test().await;
}

#[tokio::test]
async fn persist_session_coalesces_small_outputs_into_one_row() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    let checkpoint = sample("s1");
    store
        .persist_session(
            "s1",
            Some(&checkpoint),
            &[(0, b"a".to_vec()), (1, b"b".to_vec())],
        )
        .await
        .unwrap();
    for sequence in 2..200_i64 {
        store
            .persist_session("s1", None, &[(sequence, vec![b'c'])])
            .await
            .unwrap();
    }

    assert_eq!(output_row_count(&store, "s1").await, 1);
    let read = store.read("s1", 1024).await.unwrap().unwrap();
    assert_eq!(read.buffer.len(), 200);
    assert!(read.buffer.starts_with(b"abc"));
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), 200);
}

#[tokio::test]
async fn persist_session_opens_a_new_row_once_the_newest_is_full() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    let checkpoint = sample("s1");
    let big = vec![b'x'; OUTPUT_CHUNK_TARGET_BYTES - 1];
    store
        .persist_session("s1", Some(&checkpoint), &[(0, big.clone())])
        .await
        .unwrap();
    store
        .persist_session("s1", None, &[(1, b"yz".to_vec()), (2, b"w".to_vec())])
        .await
        .unwrap();

    assert_eq!(output_row_count(&store, "s1").await, 2);
    let mut expected = big;
    expected.extend_from_slice(b"yzw");
    let read = store.read("s1", usize::MAX).await.unwrap().unwrap();
    assert_eq!(read.buffer, expected);
}

#[tokio::test]
async fn retried_persist_does_not_duplicate_committed_output() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    let checkpoint = sample("s1");
    let batch = [(0, b"hello ".to_vec()), (1, b"world".to_vec())];
    store
        .persist_session("s1", Some(&checkpoint), &batch)
        .await
        .unwrap();

    // The same batch again, now followed by output that arrived meanwhile.
    let mut retried = batch.to_vec();
    retried.push((2, b"!".to_vec()));
    store.persist_session("s1", None, &retried).await.unwrap();

    let read = store.read("s1", 1024).await.unwrap().unwrap();
    assert_eq!(read.buffer, b"hello world!");
}

#[tokio::test]
async fn trim_compacts_a_history_of_tiny_rows() {
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    store.upsert(sample("s1")).await.unwrap();
    let rows = OUTPUT_COMPACTION_ROW_THRESHOLD + 10;
    for sequence in 0..rows {
        store
            .append_output("s1", sequence, format!("{sequence};").as_bytes())
            .await
            .unwrap();
    }
    let before = store.read("s1", usize::MAX).await.unwrap().unwrap().buffer;

    store.trim_session("s1", 5_000).await.unwrap();

    assert!(output_row_count(&store, "s1").await <= 1);
    let after = store.read("s1", usize::MAX).await.unwrap().unwrap().buffer;
    assert_eq!(after, before[before.len() - 5_000..]);
    assert_eq!(store.next_output_sequence("s1").await.unwrap(), rows);

    store
        .persist_session("s1", None, &[(rows, b"next".to_vec())])
        .await
        .unwrap();
    let tail = store.read("s1", usize::MAX).await.unwrap().unwrap().buffer;
    assert!(tail.ends_with(b"next"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_session_writers_wait_for_the_lock_instead_of_failing() {
    // Each output-only persist reads before it writes. In WAL a deferred
    // transaction that upgrades while the other pooled connection holds the
    // write lock fails at once with SQLITE_BUSY (or SQLITE_BUSY_SNAPSHOT)
    // without consulting the busy timeout, which left live sessions retrying
    // every five seconds.
    let dir = tempfile::tempdir().unwrap();
    let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
    for session in ["s1", "s2"] {
        store
            .persist_session(session, Some(&sample(session)), &[(0, b"x".to_vec())])
            .await
            .unwrap();
    }

    let writer = |session: &'static str| {
        let store = store.clone();
        tokio::spawn(async move {
            for sequence in 1..200_i64 {
                store
                    .persist_session(session, None, &[(sequence, vec![b'y'; 64])])
                    .await?;
            }
            anyhow::Ok(())
        })
    };
    let (first, second) = tokio::join!(writer("s1"), writer("s2"));
    first.unwrap().unwrap();
    second.unwrap().unwrap();

    for session in ["s1", "s2"] {
        assert_eq!(store.next_output_sequence(session).await.unwrap(), 200);
    }
}
