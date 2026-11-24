use std::borrow::Cow;
use std::path::Path;

use alera_core::runtime::{
    harden_sqlite_files, open_private_runtime_file, prepare_private_runtime_directory,
};
use anyhow::Result;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use futures_util::TryStreamExt;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

pub const HISTORY_DATABASE_FILE_NAME: &str = "terminal_history.sqlite";
const HISTORY_STORE_MAX_CONNECTIONS: u32 = 2;
/// Output rows are appended to until they reach this size.
const OUTPUT_CHUNK_TARGET_BYTES: usize = 16 * 1024;
/// A session with more rows than this is compacted before it is trimmed.
const OUTPUT_COMPACTION_ROW_THRESHOLD: i64 = 2048;

const CREATE_CHECKPOINTS_TABLE_SQL: &str = "\
CREATE TABLE IF NOT EXISTS checkpoints (\n\
  sessionId TEXT PRIMARY KEY,\n\
  workspaceId TEXT NOT NULL,\n\
  tabId TEXT NOT NULL,\n\
  workingDirectory TEXT NOT NULL,\n\
  running INTEGER NOT NULL,\n\
  exitCode INTEGER,\n\
  endedAt TEXT,\n\
  outputStreamBytes INTEGER NOT NULL DEFAULT 0,\n\
  updatedAt TEXT NOT NULL\n\
);";

const CREATE_OUTPUT_CHUNKS_TABLE_SQL: &str = "\
CREATE TABLE IF NOT EXISTS outputChunks (\n\
  id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
  sessionId TEXT NOT NULL,\n\
  sequence INTEGER NOT NULL,\n\
  createdAt TEXT NOT NULL,\n\
  data BLOB NOT NULL\n\
);";

const CREATE_OUTPUT_CHUNKS_SESSION_INDEX_SQL: &str = "\
CREATE INDEX IF NOT EXISTS outputChunksSessionIdSequenceIdx\n\
ON outputChunks(sessionId, sequence, id);";

#[derive(Debug, Clone, PartialEq)]
pub struct TerminalHostCheckpoint {
    pub session_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub working_directory: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub ended_at: Option<DateTime<Utc>>,
    pub output_stream_bytes: u64,
    pub updated_at: DateTime<Utc>,
    pub buffer: Vec<u8>,
}

#[derive(Clone)]
pub struct TerminalHostHistoryStore {
    pool: SqlitePool,
}

impl TerminalHostHistoryStore {
    pub async fn open(runtime_dir: &Path) -> Result<Self> {
        prepare_private_runtime_directory(runtime_dir)?;
        let path = runtime_dir.join(HISTORY_DATABASE_FILE_NAME);
        open_private_runtime_file(&path)?;
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);
        // SQLite runs one worker thread per pooled connection. Output writes
        // are serialized by SQLite itself; a second connection lets a read or
        // trim proceed without retaining the default pool of ten threads.
        let pool = SqlitePoolOptions::new()
            .max_connections(HISTORY_STORE_MAX_CONNECTIONS)
            .connect_with(options)
            .await?;
        destroy_legacy_checkpoint_schema(&pool).await?;
        destroy_unsequenced_output_chunks_schema(&pool).await?;
        sqlx::query(CREATE_CHECKPOINTS_TABLE_SQL)
            .execute(&pool)
            .await?;
        ensure_output_stream_bytes_column(&pool).await?;
        sqlx::query(CREATE_OUTPUT_CHUNKS_TABLE_SQL)
            .execute(&pool)
            .await?;
        sqlx::query(CREATE_OUTPUT_CHUNKS_SESSION_INDEX_SQL)
            .execute(&pool)
            .await?;
        harden_sqlite_files(&path)?;
        Ok(TerminalHostHistoryStore { pool })
    }

    pub async fn read(
        &self,
        session_id: &str,
        max_bytes: usize,
    ) -> Result<Option<TerminalHostCheckpoint>> {
        let row = sqlx::query(
            "SELECT sessionId, workspaceId, tabId, workingDirectory, running, exitCode, \
             endedAt, outputStreamBytes, updatedAt FROM checkpoints WHERE sessionId = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let buffer = self.read_buffer(session_id, max_bytes).await?;
        let ended_at: Option<String> = row.try_get("endedAt")?;
        let updated_at: String = row.try_get("updatedAt")?;
        Ok(Some(TerminalHostCheckpoint {
            session_id: row.try_get("sessionId")?,
            workspace_id: row.try_get("workspaceId")?,
            tab_id: row.try_get("tabId")?,
            working_directory: row.try_get("workingDirectory")?,
            running: row.try_get::<i64, _>("running")? == 1,
            exit_code: row.try_get("exitCode")?,
            ended_at: parse_timestamp(ended_at.as_deref()),
            output_stream_bytes: row.try_get::<i64, _>("outputStreamBytes")?.max(0) as u64,
            updated_at: parse_timestamp(Some(updated_at.as_str()))
                .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("epoch is valid")),
            buffer,
        }))
    }

    #[cfg(test)]
    pub(crate) async fn close_for_test(&self) {
        self.pool.close().await;
    }

    #[cfg(test)]
    pub async fn upsert(&self, checkpoint: TerminalHostCheckpoint) -> Result<()> {
        sqlx::query(
            "INSERT INTO checkpoints \
             (sessionId, workspaceId, tabId, workingDirectory, running, exitCode, endedAt, outputStreamBytes, updatedAt) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(sessionId) DO UPDATE SET \
             workspaceId = excluded.workspaceId, \
             tabId = excluded.tabId, \
             workingDirectory = excluded.workingDirectory, \
             running = excluded.running, \
             exitCode = excluded.exitCode, \
             endedAt = excluded.endedAt, \
             outputStreamBytes = excluded.outputStreamBytes, \
             updatedAt = excluded.updatedAt",
        )
        .bind(checkpoint.session_id)
        .bind(checkpoint.workspace_id)
        .bind(checkpoint.tab_id)
        .bind(checkpoint.working_directory)
        .bind(if checkpoint.running { 1_i64 } else { 0_i64 })
        .bind(checkpoint.exit_code.map(i64::from))
        .bind(checkpoint.ended_at.map(format_timestamp))
        .bind(i64::try_from(checkpoint.output_stream_bytes).unwrap_or(i64::MAX))
        .bind(format_timestamp(checkpoint.updated_at))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[cfg(test)]
    pub async fn append_output(&self, session_id: &str, sequence: i64, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO outputChunks (sessionId, sequence, createdAt, data) \
             SELECT ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM checkpoints WHERE sessionId = ?)",
        )
        .bind(session_id)
        .bind(sequence)
        .bind(format_timestamp(Utc::now()))
        .bind(data)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Opens a transaction that takes the write lock up front.
    ///
    /// These transactions read before they write. In WAL mode a deferred
    /// transaction that upgrades while the other pooled connection holds the
    /// write lock fails at once with SQLITE_BUSY, or SQLITE_BUSY_SNAPSHOT,
    /// without consulting the busy timeout. `BEGIN IMMEDIATE` waits instead.
    async fn begin_write(&self) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>> {
        Ok(self.pool.begin_with("BEGIN IMMEDIATE").await?)
    }

    #[cfg(test)]
    pub async fn persist_session(
        &self,
        session_id: &str,
        checkpoint: Option<&TerminalHostCheckpoint>,
        outputs: &[(i64, impl AsRef<[u8]>)],
    ) -> Result<()> {
        self.persist_session_with_trim(session_id, checkpoint, outputs, None)
            .await
    }

    /// Persist a write-behind snapshot and its optional maintenance trim
    /// with a single commit. The trim sees the just-appended, uncommitted
    /// output, so no second writer admission or WAL commit is necessary.
    pub(super) async fn persist_session_with_trim(
        &self,
        session_id: &str,
        checkpoint: Option<&TerminalHostCheckpoint>,
        outputs: &[(i64, impl AsRef<[u8]>)],
        max_bytes: Option<usize>,
    ) -> Result<()> {
        let outputs = if outputs.iter().any(|(_, data)| !data.as_ref().is_empty()) {
            outputs
        } else {
            &[]
        };
        if checkpoint.is_none() && outputs.is_empty() {
            // A pure no-op trim must not acquire the writer lock.
            return match max_bytes {
                Some(limit) => self.trim_session(session_id, limit).await,
                None => Ok(()),
            };
        }

        let mut transaction = self.begin_write().await?;
        persist_session_rows(&mut transaction, session_id, checkpoint, outputs).await?;
        if let Some(limit) = max_bytes {
            if let Err(trim_error) = trim_output_rows(&mut transaction, session_id, limit).await {
                // Preserve the previous durability contract on maintenance
                // failure: roll back the combined attempt, persist the output
                // without trimming, and report the trim failure for retry.
                // The same monotonically increasing output sequences make
                // that later retry idempotent.
                transaction.rollback().await?;
                let mut transaction = self.begin_write().await?;
                persist_session_rows(&mut transaction, session_id, checkpoint, outputs).await?;
                transaction.commit().await?;
                return Err(trim_error.context("terminal history trim failed after saving output"));
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn next_output_sequence(&self, session_id: &str) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(sequence), -1) AS maxSequence \
             FROM outputChunks WHERE sessionId = ?",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        let max_sequence: i64 = row.try_get("maxSequence")?;
        Ok(max_sequence.saturating_add(1))
    }

    pub async fn trim_session(&self, session_id: &str, max_bytes: usize) -> Result<()> {
        // Standalone maintenance usually finds nothing to do. Check with a
        // read connection first so it need not queue behind output writers.
        if trim_action(&self.pool, session_id, max_bytes).await? == TrimAction::None {
            return Ok(());
        }
        let mut transaction = self.begin_write().await?;
        // Recheck after admission; concurrent writes cannot make a stale cut
        // delete data that arrived since the read-only preflight.
        trim_output_rows(&mut transaction, session_id, max_bytes).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn delete(&self, session_id: &str) -> Result<()> {
        // One transaction, so a reader never sees the chunks gone while the
        // checkpoint that would restore the session still exists.
        let mut transaction = self.begin_write().await?;
        sqlx::query("DELETE FROM outputChunks WHERE sessionId = ?")
            .bind(session_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM checkpoints WHERE sessionId = ?")
            .bind(session_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn read_buffer(&self, session_id: &str, max_bytes: usize) -> Result<Vec<u8>> {
        read_buffer_from(&self.pool, session_id, max_bytes).await
    }
}

async fn read_buffer_from<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    session_id: &'e str,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    if max_bytes == 0 {
        return Ok(Vec::new());
    }

    // Read newest chunks first and stop as soon as the requested scrollback
    // tail is complete. This uses the (sessionId, sequence, id) index and
    // avoids the expensive window-function trim on the latency-sensitive
    // restore/read path.
    let mut rows = sqlx::query(
        "SELECT data FROM outputChunks WHERE sessionId = ? ORDER BY sequence DESC, id DESC",
    )
    .bind(session_id)
    .fetch(executor);
    let mut tail = HistoryReadTail::new(max_bytes);
    while tail.remaining() > 0 {
        let Some(row) = rows.try_next().await? else {
            break;
        };
        // Borrow the row's BLOB instead of allocating a Vec for each row.
        // Only the retained suffix is copied into the growing tail.
        let data: &[u8] = row.try_get("data")?;
        tail.prepend(data);
    }
    drop(rows);
    Ok(tail.finish())
}

/// Builds a chronological tail from rows arriving newest first.
///
/// Keep the retained bytes in one allocation, right-aligned so each older row
/// can be copied into the unused prefix. Grow geometrically from actual bytes
/// read rather than allocating the requested limit (which can be usize::MAX).
/// No per-row owned BLOBs or final concatenation allocation are needed.
struct HistoryReadTail {
    max_bytes: usize,
    buffer: Vec<u8>,
    start: usize,
}

impl HistoryReadTail {
    fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            buffer: Vec::new(),
            start: 0,
        }
    }

    fn len(&self) -> usize {
        self.buffer.len() - self.start
    }

    fn remaining(&self) -> usize {
        self.max_bytes - self.len()
    }

    fn prepend(&mut self, data: &[u8]) {
        let retained = &data[data.len().saturating_sub(self.remaining())..];
        if retained.is_empty() {
            return;
        }
        if self.start >= retained.len() {
            self.start -= retained.len();
            self.buffer[self.start..self.start + retained.len()].copy_from_slice(retained);
            return;
        }

        let required = self.len() + retained.len();
        let capacity = self
            .buffer
            .len()
            .saturating_mul(2)
            .max(required)
            .min(self.max_bytes);
        let start = capacity - required;
        let mut grown = Vec::with_capacity(capacity);
        // Initialize only the unused prefix. The real bytes are copied
        // directly into their final positions, without zeroing them first.
        grown.resize(start, 0);
        grown.extend_from_slice(retained);
        grown.extend_from_slice(&self.buffer[self.start..]);
        self.buffer = grown;
        self.start = start;
    }

    fn finish(mut self) -> Vec<u8> {
        if self.start > 0 {
            let len = self.len();
            self.buffer.copy_within(self.start.., 0);
            self.buffer.truncate(len);
        }
        self.buffer
    }
}

async fn persist_session_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    checkpoint: Option<&TerminalHostCheckpoint>,
    outputs: &[(i64, impl AsRef<[u8]>)],
) -> Result<()> {
    if let Some(checkpoint) = checkpoint {
        sqlx::query(
            "INSERT INTO checkpoints \
             (sessionId, workspaceId, tabId, workingDirectory, running, exitCode, endedAt, outputStreamBytes, updatedAt) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(sessionId) DO UPDATE SET \
             workspaceId = excluded.workspaceId, \
             tabId = excluded.tabId, \
             workingDirectory = excluded.workingDirectory, \
             running = excluded.running, \
             exitCode = excluded.exitCode, \
             endedAt = excluded.endedAt, \
             outputStreamBytes = excluded.outputStreamBytes, \
             updatedAt = excluded.updatedAt",
        )
        .bind(&checkpoint.session_id)
        .bind(&checkpoint.workspace_id)
        .bind(&checkpoint.tab_id)
        .bind(&checkpoint.working_directory)
        .bind(if checkpoint.running { 1_i64 } else { 0_i64 })
        .bind(checkpoint.exit_code.map(i64::from))
        .bind(
            checkpoint
                .ended_at
                .as_ref()
                .map(|value| format_timestamp(value.to_owned())),
        )
        .bind(i64::try_from(checkpoint.output_stream_bytes).unwrap_or(i64::MAX))
        .bind(format_timestamp(checkpoint.updated_at.to_owned()))
        .execute(&mut **transaction)
        .await?;
    }

    if outputs.iter().any(|(_, data)| !data.as_ref().is_empty()) {
        append_output_rows(transaction, session_id, outputs).await?;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrimAction {
    None,
    Cut,
    Compact,
}

/// An aggregate is much cheaper than the ordered running sum when there is
/// no excess history. Keep that common no-op path free of mutations.
async fn trim_action<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    session_id: &'e str,
    max_bytes: usize,
) -> Result<TrimAction> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS rowCount, COALESCE(SUM(length(data)), 0) AS totalBytes \
         FROM outputChunks WHERE sessionId = ?",
    )
    .bind(session_id)
    .fetch_one(executor)
    .await?;
    let row_count: i64 = row.try_get("rowCount")?;
    let total_bytes: i64 = row.try_get("totalBytes")?;
    if row_count > OUTPUT_COMPACTION_ROW_THRESHOLD {
        Ok(TrimAction::Compact)
    } else if total_bytes > i64::try_from(max_bytes).unwrap_or(i64::MAX) {
        Ok(TrimAction::Cut)
    } else {
        Ok(TrimAction::None)
    }
}

/// Determine and apply the cut using the same transaction as the append.
/// Compaction also reads from this connection, including uncommitted rows.
async fn trim_output_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    max_bytes: usize,
) -> Result<()> {
    match trim_action(&mut **transaction, session_id, max_bytes).await? {
        TrimAction::None => return Ok(()),
        TrimAction::Compact => {
            return compact_output_rows(transaction, session_id, max_bytes).await
        }
        TrimAction::Cut => {}
    }
    let max_bytes_i64 = i64::try_from(max_bytes).unwrap_or(i64::MAX);
    let row = sqlx::query(
        "SELECT id, sequence, byteLen, running FROM ( \
           SELECT id, sequence, length(data) AS byteLen, \
                  SUM(length(data)) OVER ( \
                    ORDER BY sequence DESC, id DESC \
                  ) AS running \
           FROM outputChunks WHERE sessionId = ? \
         ) WHERE running > ? ORDER BY sequence DESC, id DESC LIMIT 1",
    )
    .bind(session_id)
    .bind(max_bytes_i64)
    .fetch_optional(&mut **transaction)
    .await?;
    if let Some(row) = row {
        let id: i64 = row.try_get("id")?;
        let sequence: i64 = row.try_get("sequence")?;
        let byte_len: i64 = row.try_get("byteLen")?;
        let running: i64 = row.try_get("running")?;
        // Everything newer than this row, which is what stays whole.
        let kept = running.saturating_sub(byte_len).max(0) as usize;
        let remaining = max_bytes.saturating_sub(kept);
        if remaining > 0 {
            // SQLite substr operates on BLOB bytes, not Unicode characters.
            // Keep the boundary data in SQLite instead of round-tripping
            // the full chunk and copying its retained suffix through Rust.
            sqlx::query("UPDATE outputChunks SET data = substr(data, -?) WHERE id = ?")
                .bind(i64::try_from(remaining).unwrap_or(i64::MAX))
                .bind(id)
                .execute(&mut **transaction)
                .await?;
            sqlx::query(
                "DELETE FROM outputChunks \
                 WHERE sessionId = ? AND (sequence < ? OR (sequence = ? AND id < ?))",
            )
            .bind(session_id)
            .bind(sequence)
            .bind(sequence)
            .bind(id)
            .execute(&mut **transaction)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM outputChunks \
                 WHERE sessionId = ? AND (sequence < ? OR (sequence = ? AND id <= ?))",
            )
            .bind(session_id)
            .bind(sequence)
            .bind(sequence)
            .bind(id)
            .execute(&mut **transaction)
            .await?;
        }
    }
    Ok(())
}

/// Appends a flush's output to the session's newest row until it holds
/// [`OUTPUT_CHUNK_TARGET_BYTES`], then opens new rows of that size.
///
/// One row per PTY read left sessions with six figures of rows averaging under
/// 100 bytes, and the byte-retention trim has to sum every one of them.
///
/// A write-behind retry can repeat a batch after an ambiguous SQLite failure.
/// A flush commits all of its output or none of it and sequences only grow, so
/// skipping every sequence at or below the newest persisted one is the
/// idempotency rule; the newest row always carries the last sequence it holds.
async fn append_output_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    outputs: &[(i64, impl AsRef<[u8]>)],
) -> Result<()> {
    let has_checkpoint = sqlx::query("SELECT 1 FROM checkpoints WHERE sessionId = ?")
        .bind(session_id)
        .fetch_optional(&mut **transaction)
        .await?
        .is_some();
    if !has_checkpoint {
        return Ok(());
    }
    let newest = sqlx::query(
        "SELECT id, sequence, CASE WHEN length(data) < ? THEN data END AS appendableData \
         FROM outputChunks WHERE sessionId = ? ORDER BY sequence DESC, id DESC LIMIT 1",
    )
    .bind(OUTPUT_CHUNK_TARGET_BYTES as i64)
    .bind(session_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let mut rows: Vec<PendingOutputRow<'_>> = Vec::new();
    let mut persisted_sequence = None;
    if let Some(row) = newest {
        let sequence: i64 = row.try_get("sequence")?;
        persisted_sequence = Some(sequence);
        // A full or oversized newest row cannot be extended. Do not decode
        // or transfer its BLOB to Rust just to discard it.
        let data: Option<Vec<u8>> = row.try_get("appendableData")?;
        if let Some(data) = data {
            rows.push(PendingOutputRow {
                existing_id: Some(row.try_get("id")?),
                sequence,
                data: Cow::Owned(data),
                dirty: false,
            });
        }
    }

    for (sequence, data) in outputs {
        let data = data.as_ref();
        if data.is_empty() || persisted_sequence.is_some_and(|newest| *sequence <= newest) {
            continue;
        }
        match rows.last_mut() {
            Some(row) if row.data.len() + data.len() <= OUTPUT_CHUNK_TARGET_BYTES => {
                row.data.to_mut().extend_from_slice(data);
                row.sequence = *sequence;
                row.dirty = true;
            }
            _ => rows.push(PendingOutputRow {
                existing_id: None,
                sequence: *sequence,
                data: Cow::Borrowed(data),
                dirty: true,
            }),
        }
    }

    let created_at = format_timestamp(Utc::now());
    for row in rows.into_iter().filter(|row| row.dirty) {
        match row.existing_id {
            Some(id) => {
                sqlx::query(
                    "UPDATE outputChunks SET sequence = ?, createdAt = ?, data = ? WHERE id = ?",
                )
                .bind(row.sequence)
                .bind(&created_at)
                .bind(row.data.as_ref())
                .bind(id)
                .execute(&mut **transaction)
                .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO outputChunks (sessionId, sequence, createdAt, data)                      VALUES (?, ?, ?, ?)",
                )
                .bind(session_id)
                .bind(row.sequence)
                .bind(&created_at)
                .bind(row.data.as_ref())
                .execute(&mut **transaction)
                .await?;
            }
        }
    }
    Ok(())
}

struct PendingOutputRow<'a> {
    existing_id: Option<i64>,
    sequence: i64,
    data: Cow<'a, [u8]>,
    dirty: bool,
}

/// Rewrites a session's retained tail as [`OUTPUT_CHUNK_TARGET_BYTES`] rows,
/// ending at the newest sequence so later appends still order after it.
///
/// Histories written before rows were coalesced can hold six figures of tiny
/// rows, and the trim's running sum over them took seconds on every
/// maintenance pass.
async fn compact_output_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    max_bytes: usize,
) -> Result<()> {
    let tail = read_buffer_from(&mut **transaction, session_id, max_bytes).await?;
    let newest: Option<i64> =
        sqlx::query("SELECT MAX(sequence) AS maxSequence FROM outputChunks WHERE sessionId = ?")
            .bind(session_id)
            .fetch_one(&mut **transaction)
            .await?
            .try_get("maxSequence")?;
    let Some(newest) = newest else {
        return Ok(());
    };
    sqlx::query("DELETE FROM outputChunks WHERE sessionId = ?")
        .bind(session_id)
        .execute(&mut **transaction)
        .await?;
    let chunks = tail.chunks(OUTPUT_CHUNK_TARGET_BYTES);
    let first_sequence = newest.saturating_sub(chunks.len() as i64 - 1);
    let created_at = format_timestamp(Utc::now());
    for (index, chunk) in chunks.enumerate() {
        sqlx::query(
            "INSERT INTO outputChunks (sessionId, sequence, createdAt, data) VALUES (?, ?, ?, ?)",
        )
        .bind(session_id)
        .bind(first_sequence.saturating_add(index as i64))
        .bind(&created_at)
        .bind(chunk)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn destroy_legacy_checkpoint_schema(pool: &SqlitePool) -> Result<()> {
    let rows = sqlx::query("PRAGMA table_info(checkpoints)")
        .fetch_all(pool)
        .await?;
    let has_legacy_buffer = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .is_ok_and(|name| name == "buffer")
    });
    if has_legacy_buffer {
        sqlx::query("DROP TABLE IF EXISTS checkpoints")
            .execute(pool)
            .await?;
        sqlx::query("DROP TABLE IF EXISTS outputChunks")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn destroy_unsequenced_output_chunks_schema(pool: &SqlitePool) -> Result<()> {
    let rows = sqlx::query("PRAGMA table_info(outputChunks)")
        .fetch_all(pool)
        .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let has_sequence = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .is_ok_and(|name| name == "sequence")
    });
    if !has_sequence {
        sqlx::query("DROP TABLE IF EXISTS outputChunks")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn ensure_output_stream_bytes_column(pool: &SqlitePool) -> Result<()> {
    let rows = sqlx::query("PRAGMA table_info(checkpoints)")
        .fetch_all(pool)
        .await?;
    let has_column = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .is_ok_and(|name| name == "outputStreamBytes")
    });
    if !has_column {
        sqlx::query(
            "ALTER TABLE checkpoints ADD COLUMN outputStreamBytes INTEGER NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

fn format_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?;
    if value.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests;
