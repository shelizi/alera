use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::Path;

use alera_core::runtime::{
    harden_sqlite_files, open_private_runtime_file, prepare_private_runtime_directory,
};
use anyhow::Result;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use futures_util::TryStreamExt;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

pub const HISTORY_DATABASE_FILE_NAME: &str = "terminal_history.sqlite";
const HISTORY_STORE_MAX_CONNECTIONS: u32 = 2;
/// Output rows are appended to until they reach this size.
const OUTPUT_CHUNK_TARGET_BYTES: usize = 16 * 1024;
/// A session with more rows than this is compacted before it is trimmed.
const OUTPUT_COMPACTION_ROW_THRESHOLD: i64 = 2048;
/// Bound both SQLite parameters (four per row) and encoded BLOB payload.
/// An oversized individual row is still inserted intact, on its own.
const OUTPUT_INSERT_BATCH_ROWS: usize = 64;
const OUTPUT_INSERT_BATCH_BYTES: usize = 1024 * 1024;

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
pub struct TerminalHostCheckpoint<B = Vec<u8>> {
    pub session_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub working_directory: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub ended_at: Option<DateTime<Utc>>,
    pub output_stream_bytes: u64,
    pub updated_at: DateTime<Utc>,
    pub buffer: B,
}

impl<B> TerminalHostCheckpoint<B> {
    /// Preserve the checkpoint metadata while changing the buffer's ownership
    /// representation. The durable writer still uses the ordinary Vec type.
    pub(super) fn map_buffer<T>(self, map: impl FnOnce(B) -> T) -> TerminalHostCheckpoint<T> {
        TerminalHostCheckpoint {
            session_id: self.session_id,
            workspace_id: self.workspace_id,
            tab_id: self.tab_id,
            working_directory: self.working_directory,
            running: self.running,
            exit_code: self.exit_code,
            ended_at: self.ended_at,
            output_stream_bytes: self.output_stream_bytes,
            updated_at: self.updated_at,
            buffer: map(self.buffer),
        }
    }
}

pub(super) type RestoredCheckpoint = TerminalHostCheckpoint<VecDeque<u8>>;

#[derive(Clone)]
pub struct TerminalHostHistoryStore {
    pool: SqlitePool,
    blob_concat_safe: bool,
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
        // SQLite concatenation temporarily treats its result as text.
        // UTF-16 databases can discard an odd trailing byte, so only use
        // the direct BLOB append in UTF-8 databases. Encoding is fixed
        // once the database has a schema.
        let encoding: String = sqlx::query_scalar("PRAGMA encoding")
            .fetch_one(&pool)
            .await?;
        Ok(TerminalHostHistoryStore {
            pool,
            blob_concat_safe: encoding.eq_ignore_ascii_case("UTF-8"),
        })
    }

    #[cfg(test)]
    pub async fn read(
        &self,
        session_id: &str,
        max_bytes: usize,
    ) -> Result<Option<TerminalHostCheckpoint>> {
        Ok(self
            .read_with_maintenance(session_id, max_bytes)
            .await?
            .map(|(checkpoint, _)| checkpoint.map_buffer(Vec::from)))
    }

    /// Report whether the cold read could prove that no retention or row
    /// compaction is needed, using only the rows it already reads.
    pub(super) async fn read_with_maintenance(
        &self,
        session_id: &str,
        max_bytes: usize,
    ) -> Result<Option<(RestoredCheckpoint, bool)>> {
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
        let (buffer, maintenance_required) =
            read_buffer_with_maintenance(&self.pool, session_id, max_bytes).await?;
        let ended_at: Option<String> = row.try_get("endedAt")?;
        let updated_at: String = row.try_get("updatedAt")?;
        let checkpoint = TerminalHostCheckpoint {
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
        };
        Ok(Some((checkpoint, maintenance_required)))
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
        persist_session_rows(
            &mut transaction,
            session_id,
            checkpoint,
            outputs,
            self.blob_concat_safe,
        )
        .await?;
        if let Some(limit) = max_bytes {
            if let Err(trim_error) = trim_output_rows(&mut transaction, session_id, limit).await {
                // Preserve the previous durability contract on maintenance
                // failure: roll back the combined attempt, persist the output
                // without trimming, and report the trim failure for retry.
                // The same monotonically increasing output sequences make
                // that later retry idempotent.
                transaction.rollback().await?;
                let mut transaction = self.begin_write().await?;
                persist_session_rows(
                    &mut transaction,
                    session_id,
                    checkpoint,
                    outputs,
                    self.blob_concat_safe,
                )
                .await?;
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

    #[cfg(test)]
    async fn read_buffer(&self, session_id: &str, max_bytes: usize) -> Result<Vec<u8>> {
        read_buffer_from(&self.pool, session_id, max_bytes).await
    }
}

#[cfg(test)]
async fn read_buffer_from<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    session_id: &'e str,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    Ok(Vec::from(
        read_buffer_with_maintenance(executor, session_id, max_bytes)
            .await?
            .0,
    ))
}

/// Cap the payload of an oversized legacy BLOB before SQLx copies the row
/// out of SQLite. Normal coalesced rows are returned unchanged. Truncating
/// any individual row to the entire requested tail cannot change the final
/// suffix, even when newer rows already consumed part of that budget.
fn history_rows_query(
    session_id: &str,
    max_bytes: usize,
) -> sqlx::query::Query<'_, Sqlite, sqlx::sqlite::SqliteArguments> {
    if max_bytes >= i32::MAX as usize {
        // SQLite cannot contain a BLOB this large; no projection is needed.
        sqlx::query(
            "SELECT data FROM outputChunks WHERE sessionId = ? ORDER BY sequence DESC, id DESC",
        )
        .bind(session_id)
    } else {
        sqlx::query(
            "SELECT CASE WHEN length(data) > ?2 THEN substr(data, -?2) ELSE data END AS data \
             FROM outputChunks WHERE sessionId = ?1 ORDER BY sequence DESC, id DESC",
        )
        .bind(session_id)
        .bind(max_bytes as i64)
    }
}

async fn read_buffer_with_maintenance<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    session_id: &'e str,
    max_bytes: usize,
) -> Result<(VecDeque<u8>, bool)> {
    if max_bytes == 0 {
        return Ok((VecDeque::new(), true));
    }

    // Read newest chunks first and stop as soon as the requested scrollback
    // tail is complete. This uses the (sessionId, sequence, id) index and
    // avoids the expensive window-function trim on the latency-sensitive
    // restore/read path.
    let mut rows = history_rows_query(session_id, max_bytes).fetch(executor);
    let mut tail = HistoryReadTail::new(max_bytes);
    let mut row_count = 0_i64;
    while tail.remaining() > 0 {
        let Some(row) = rows.try_next().await? else {
            break;
        };
        row_count = row_count.saturating_add(1);
        // Borrow the row's BLOB instead of allocating a Vec for each row.
        // Only the retained suffix is copied into the growing tail.
        let data: &[u8] = row.try_get("data")?;
        tail.prepend(data);
    }
    drop(rows);
    // A full tail can hide older rows, even if the last row fits exactly.
    // Keep maintenance in that case rather than fetching an extra BLOB.
    // Legacy tiny/empty rows may still need compaction below the byte cap.
    let maintenance_required = tail.remaining() == 0 || row_count > OUTPUT_COMPACTION_ROW_THRESHOLD;
    Ok((tail.finish_ring(), maintenance_required))
}

/// Builds a chronological tail from rows arriving newest first.
///
/// Keep the retained bytes in one allocation, right-aligned so each older row
/// can be copied into the unused prefix. Grow geometrically from actual bytes
/// read rather than allocating the requested limit (which can be usize::MAX).
/// No per-row owned BLOBs or final concatenation allocation are needed.
/// The final right-aligned allocation becomes a ring without moving its tail.
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

    fn finish_ring(self) -> VecDeque<u8> {
        let mut ring = VecDeque::from(self.buffer);
        // Draining a prefix adjusts the ring's head rather than copying the
        // remaining bytes. The restored Session can use this same allocation.
        ring.drain(..self.start);
        ring
    }

    #[cfg(test)]
    fn finish(self) -> Vec<u8> {
        Vec::from(self.finish_ring())
    }
}

async fn persist_session_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    checkpoint: Option<&TerminalHostCheckpoint>,
    outputs: &[(i64, impl AsRef<[u8]>)],
    blob_concat_safe: bool,
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
        append_output_rows(transaction, session_id, outputs, blob_concat_safe).await?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrimAction {
    None,
    Cut { excess_bytes: i64 },
    Compact,
}

/// Only histories at or below the compaction threshold need an exact byte
/// total. The bounded subquery stops visiting legacy tiny rows as soon as
/// compaction is inevitable, rather than summing their entire history.
const TRIM_STATS_SQL: &str =
    "SELECT COUNT(*) AS rowCount, COALESCE(SUM(byteLen), 0) AS totalBytes \
     FROM (SELECT length(data) AS byteLen FROM outputChunks \
           WHERE sessionId = ? LIMIT ?)";

async fn trim_action<'e>(
    executor: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    session_id: &'e str,
    max_bytes: usize,
) -> Result<TrimAction> {
    let row = sqlx::query(TRIM_STATS_SQL)
        .bind(session_id)
        .bind(OUTPUT_COMPACTION_ROW_THRESHOLD + 1)
        .fetch_one(executor)
        .await?;
    let row_count: i64 = row.try_get("rowCount")?;
    let total_bytes: i64 = row.try_get("totalBytes")?;
    let limit = i64::try_from(max_bytes).unwrap_or(i64::MAX);
    if row_count > OUTPUT_COMPACTION_ROW_THRESHOLD {
        Ok(TrimAction::Compact)
    } else if total_bytes > limit {
        Ok(TrimAction::Cut {
            excess_bytes: total_bytes - limit,
        })
    } else {
        Ok(TrimAction::None)
    }
}

const TRIM_OLDEST_SQL: &str = "SELECT id, sequence, length(data) AS byteLen FROM outputChunks \
     WHERE sessionId = ? ORDER BY sequence ASC, id ASC";
const TRIM_NEWEST_SQL: &str = "SELECT id, sequence, length(data) AS byteLen FROM outputChunks \
     WHERE sessionId = ? ORDER BY sequence DESC, id DESC";

#[derive(Debug, PartialEq, Eq)]
struct TrimBoundary {
    id: i64,
    sequence: i64,
    retained_bytes: i64,
    #[cfg(test)]
    visited_rows: usize,
}

/// Find the boundary using the existing (sessionId, sequence, id) index.
/// A normal incremental trim walks only the small expired prefix; a large
/// shrink walks the retained suffix instead. Neither path needs a window
/// function, a temporary sort, or to transfer the row's BLOB. The row stream
/// is dropped before the caller mutates the transaction.
async fn find_trim_boundary(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    excess_bytes: i64,
    max_bytes: usize,
) -> Result<TrimBoundary> {
    let limit = i64::try_from(max_bytes).unwrap_or(i64::MAX);
    let oldest_first = excess_bytes <= limit;
    let mut remaining = if oldest_first { excess_bytes } else { limit };
    let sql = if oldest_first {
        TRIM_OLDEST_SQL
    } else {
        TRIM_NEWEST_SQL
    };
    let mut rows = sqlx::query(sql).bind(session_id).fetch(&mut **transaction);
    #[cfg(test)]
    let mut visited_rows = 0;
    while let Some(row) = rows.try_next().await? {
        #[cfg(test)]
        {
            visited_rows += 1;
        }
        let byte_len: i64 = row.try_get("byteLen")?;
        // At an exact row boundary, the oldest traversal stops at the
        // last fully discarded nonempty row. The newest traversal skips
        // any empty rows on the retained side before reaching that row.
        let reached = if oldest_first {
            byte_len >= remaining
        } else {
            byte_len > remaining
        };
        if reached {
            return Ok(TrimBoundary {
                id: row.try_get("id")?,
                sequence: row.try_get("sequence")?,
                retained_bytes: if oldest_first {
                    byte_len - remaining
                } else {
                    remaining
                },
                #[cfg(test)]
                visited_rows,
            });
        }
        remaining -= byte_len;
    }
    anyhow::bail!("terminal history changed while locating retention boundary")
}

/// Determine and apply the cut using the same transaction as the append.
/// Compaction also reads from this connection, including uncommitted rows.
async fn trim_output_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    max_bytes: usize,
) -> Result<()> {
    let excess_bytes = match trim_action(&mut **transaction, session_id, max_bytes).await? {
        TrimAction::None => return Ok(()),
        TrimAction::Compact => {
            return compact_output_rows(transaction, session_id, max_bytes).await
        }
        TrimAction::Cut { excess_bytes } => excess_bytes,
    };
    let boundary = find_trim_boundary(transaction, session_id, excess_bytes, max_bytes).await?;
    if boundary.retained_bytes > 0 {
        // SQLite substr operates on BLOB bytes, not Unicode characters.
        // Keep the boundary data in SQLite instead of round-tripping
        // the full chunk and copying its retained suffix through Rust.
        sqlx::query("UPDATE outputChunks SET data = substr(data, -?) WHERE id = ?")
            .bind(boundary.retained_bytes)
            .bind(boundary.id)
            .execute(&mut **transaction)
            .await?;
        sqlx::query(
            "DELETE FROM outputChunks \
             WHERE sessionId = ? AND (sequence, id) < (?, ?)",
        )
        .bind(session_id)
        .bind(boundary.sequence)
        .bind(boundary.id)
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            "DELETE FROM outputChunks \
             WHERE sessionId = ? AND (sequence, id) <= (?, ?)",
        )
        .bind(session_id)
        .bind(boundary.sequence)
        .bind(boundary.id)
        .execute(&mut **transaction)
        .await?;
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
    blob_concat_safe: bool,
) -> Result<()> {
    let has_checkpoint = sqlx::query("SELECT 1 FROM checkpoints WHERE sessionId = ?")
        .bind(session_id)
        .fetch_optional(&mut **transaction)
        .await?
        .is_some();
    if !has_checkpoint {
        return Ok(());
    }

    // Read only the row identity and its byte length. Even a small append
    // or a replayed flush previously decoded the entire appendable BLOB.
    let newest: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT id, sequence, length(data) FROM outputChunks \
         WHERE sessionId = ? ORDER BY sequence DESC, id DESC LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let persisted_sequence = newest.map(|(_, sequence, _)| sequence);
    let appendable = newest.and_then(|(id, _, len)| {
        let capacity = OUTPUT_CHUNK_TARGET_BYTES.checked_sub(usize::try_from(len).ok()?)?;
        (capacity > 0).then_some((id, capacity))
    });
    // The extension contains only new bytes, never the existing BLOB.
    let mut extension: Option<PendingOutputRow<'_>> = None;
    let mut rows: Vec<PendingOutputRow<'_>> = Vec::new();
    for (sequence, data) in outputs {
        let data = data.as_ref();
        if data.is_empty() || persisted_sequence.is_some_and(|newest| *sequence <= newest) {
            continue;
        }
        if rows.is_empty() {
            if let Some((_, capacity)) = appendable {
                let appended = extension.as_ref().map_or(0, |row| row.data.len());
                if data.len() <= capacity - appended {
                    if let Some(row) = extension.as_mut() {
                        row.append(*sequence, data);
                    } else {
                        extension = Some(PendingOutputRow::new(*sequence, data));
                    }
                    continue;
                }
            }
        }
        // Once a new row has started, never move later smaller chunks back
        // into the previous row, even when its unused capacity would fit.
        match rows.last_mut() {
            Some(row) if row.data.len().saturating_add(data.len()) <= OUTPUT_CHUNK_TARGET_BYTES => {
                row.append(*sequence, data);
            }
            _ => rows.push(PendingOutputRow::new(*sequence, data)),
        }
    }
    if extension.is_none() && rows.is_empty() {
        return Ok(());
    }
    let created_at = format_timestamp(Utc::now());
    if let (Some((id, _)), Some(row)) = (appendable, extension) {
        if blob_concat_safe {
            // Both operands are BLOBs. CAST prevents SQLite's intermediate
            // text result from becoming stored text (length/substr would
            // otherwise interpret NUL and non-UTF-8 bytes incorrectly).
            sqlx::query(
                "UPDATE outputChunks SET sequence = ?, createdAt = ?, \
                 data = CAST(data || ? AS BLOB) WHERE id = ?",
            )
            .bind(row.sequence)
            .bind(&created_at)
            .bind(row.data.as_ref())
            .bind(id)
            .execute(&mut **transaction)
            .await?;
        } else {
            // Preserve raw bytes even in an externally created UTF-16 DB.
            // Only fetch the prefix when we actually extend that row.
            let mut data: Vec<u8> =
                sqlx::query_scalar("SELECT data FROM outputChunks WHERE id = ?")
                    .bind(id)
                    .fetch_one(&mut **transaction)
                    .await?;
            data.extend_from_slice(row.data.as_ref());
            sqlx::query(
                "UPDATE outputChunks SET sequence = ?, createdAt = ?, data = ? WHERE id = ?",
            )
            .bind(row.sequence)
            .bind(&created_at)
            .bind(&data)
            .bind(id)
            .execute(&mut **transaction)
            .await?;
        }
    }
    match rows.as_slice() {
        [] => {}
        [row] => {
            insert_output_rows(
                transaction,
                session_id,
                &created_at,
                &[(row.sequence, row.data.as_ref())],
            )
            .await?;
        }
        new_rows => {
            let inserts: Vec<_> = new_rows
                .iter()
                .map(|row| (row.sequence, row.data.as_ref()))
                .collect();
            insert_output_rows(transaction, session_id, &created_at, &inserts).await?;
        }
    }
    Ok(())
}

/// Insert new rows in chronological order, without a database-worker
/// round trip for every row. Only bound values enter the SQL builder;
/// the batch stores borrowed slices, not another copy of the history.
/// Return the number of INSERT statements for regression/benchmark checks.
async fn insert_output_rows(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    session_id: &str,
    created_at: &str,
    mut rows: &[(i64, &[u8])],
) -> Result<usize> {
    let mut statements = 0;
    while let Some(first) = rows.first() {
        let mut end = 1;
        let mut payload_bytes = first.1.len();
        while end < rows.len().min(OUTPUT_INSERT_BATCH_ROWS) {
            let next_len = rows[end].1.len();
            if payload_bytes.saturating_add(next_len) > OUTPUT_INSERT_BATCH_BYTES {
                break;
            }
            payload_bytes += next_len;
            end += 1;
        }
        let (batch, rest) = rows.split_at(end);
        rows = rest;
        if let [(sequence, data)] = batch {
            // Most periodic flushes create zero or one row. Keep their
            // existing static cached statement and avoid a batch allocation.
            sqlx::query(
                "INSERT INTO outputChunks (sessionId, sequence, createdAt, data) VALUES (?, ?, ?, ?)",
            )
            .bind(session_id)
            .bind(*sequence)
            .bind(created_at)
            .bind(*data)
            .execute(&mut **transaction)
            .await?;
        } else {
            let mut sql = QueryBuilder::<Sqlite>::new(
                "INSERT INTO outputChunks (sessionId, sequence, createdAt, data) ",
            );
            sql.push_values(batch.iter().copied(), |mut values, (sequence, data)| {
                values
                    .push_bind(session_id)
                    .push_bind(sequence)
                    .push_bind(created_at)
                    .push_bind(data);
            });
            sql.build().execute(&mut **transaction).await?;
        }
        statements += 1;
    }
    Ok(statements)
}

struct PendingOutputRow<'a> {
    sequence: i64,
    data: Cow<'a, [u8]>,
}

impl<'a> PendingOutputRow<'a> {
    fn new(sequence: i64, data: &'a [u8]) -> Self {
        Self {
            sequence,
            data: Cow::Borrowed(data),
        }
    }

    fn append(&mut self, sequence: i64, data: &[u8]) {
        self.data.to_mut().extend_from_slice(data);
        self.sequence = sequence;
    }
}

/// Rewrites a session's retained tail as [`OUTPUT_CHUNK_TARGET_BYTES`] rows,
/// ending at the newest sequence so later appends still order after it.
///
/// Histories written before rows were coalesced can hold six figures of tiny
/// rows. Compact them once so subsequent maintenance visits fewer rows.
async fn compact_output_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    max_bytes: usize,
) -> Result<()> {
    let (mut tail, _) =
        read_buffer_with_maintenance(&mut **transaction, session_id, max_bytes).await?;
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
    // The cold reader returns one contiguous (possibly offset) slice.
    // make_contiguous does not move a contiguous ring to the allocation base.
    let chunks = tail.make_contiguous().chunks(OUTPUT_CHUNK_TARGET_BYTES);
    let first_sequence = newest.saturating_sub(chunks.len() as i64 - 1);
    let created_at = format_timestamp(Utc::now());
    let inserts: Vec<_> = chunks
        .enumerate()
        .map(|(index, chunk)| (first_sequence.saturating_add(index as i64), chunk))
        .collect();
    insert_output_rows(transaction, session_id, &created_at, &inserts).await?;
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
