//! Separate, private SQLite FTS5 cache of consented/bounded Agent history.
//!
//! Provider JSONL files are never modified. Only summaries sanitized by
//! agent_session_history_scan may be stored. No ambient startup indexing.
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;

use alera_core::global_agent_search::{AgentSessionSearchQuery, AgentSessionSearchRecord};
use alera_core::runtime::{
    harden_sqlite_files, open_private_runtime_file, prepare_private_runtime_directory,
};
use anyhow::{bail, Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use crate::agent_session_history_scan::HistoryFileStamp;

pub(crate) const HISTORY_INDEX_FILE: &str = "agent-session-search.sqlite";
const INDEX_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_BATCH: usize = 256;
// Six parameters per document; 128 rows also fit older SQLite's 999 limit.
const SQL_WRITE_BATCH: usize = 128;
const MAX_RESULTS: usize = 100;
const MAX_INDEXED_RECORDS: i64 = 20_000;
const MAX_QUERY_CHARS: usize = 256;
// Bound the transient serialized batch used by the read-only warm check.
const MAX_WARM_JSON_BYTES: usize = 2 * 1024 * 1024;

pub(crate) struct AgentSessionHistoryIndex {
    pool: SqlitePool,
    path: PathBuf,
    file_stamp: Option<HistoryFileStamp>,
}

struct IndexedDocument<'a> {
    id: i64,
    record_json: &'a str,
    title: &'a str,
    goal: &'a str,
}

struct PendingDocument<'a> {
    key: &'a str,
    scope: String,
    record_json: String,
    record: &'a AgentSessionSearchRecord,
    previous: Option<IndexedDocument<'a>>,
}

impl PendingDocument<'_> {
    fn needs_fts_write(&self) -> bool {
        self.previous.as_ref().is_none_or(|previous| {
            previous.title != self.record.title || previous.goal != self.record.goal
        })
    }
}

fn document_key(record: &AgentSessionSearchRecord) -> serde_json::Result<String> {
    serde_json::to_string(&(
        &record.runtime_instance_id,
        &record.project_id,
        &record.workspace_id,
        &record.provider,
        &record.provider_account_id,
        &record.profile_id,
        &record.provider_session_id,
    ))
}

/// Immutable, bounded per-request identity keys shared by the write and
/// scoped search. Borrowing the records prevents a caller from mutating their
/// identities while the serialized keys are in use.
pub(crate) struct PreparedHistoryBatch<'a> {
    records: &'a [AgentSessionSearchRecord],
    keys: Vec<String>,
}

impl<'a> PreparedHistoryBatch<'a> {
    pub(crate) fn new(records: &'a [AgentSessionSearchRecord]) -> Result<Self> {
        if records.len() > MAX_BATCH {
            bail!("history index batch exceeds bounded limit");
        }
        let keys = records
            .iter()
            .map(document_key)
            .collect::<serde_json::Result<Vec<_>>>()?;
        Ok(Self { records, keys })
    }

    fn iter(&self) -> impl Iterator<Item = (&AgentSessionSearchRecord, &str)> {
        self.records
            .iter()
            .zip(self.keys.iter().map(String::as_str))
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct HistoryIndexWriteStats {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub trimmed: usize,
    #[cfg(test)]
    pub(crate) write_statements: usize,
    #[cfg(test)]
    pub(crate) json_buffer_starts: usize,
    #[cfg(test)]
    pub(crate) warm_check_queries: usize,
    #[cfg(test)]
    pub(crate) warm_check_hits: usize,
    #[cfg(test)]
    pub(crate) existing_rows_loaded: usize,
    #[cfg(test)]
    retention_queries: usize,
    #[cfg(test)]
    retention_peak_ids: usize,
}

impl AgentSessionHistoryIndex {
    pub(crate) async fn open(private_runtime_dir: &Path) -> Result<Self> {
        prepare_private_runtime_directory(private_runtime_dir)?;
        let path = private_runtime_dir.join(HISTORY_INDEX_FILE);
        let file = open_private_runtime_file(&path)?;
        let before = HistoryFileStamp::read(&file);
        drop(file);
        let options = SqliteConnectOptions::new()
            .filename(&path)
            // The private file was created above; a reconnect must never silently
            // create a replacement with default file permissions.
            .create_if_missing(false)
            .busy_timeout(Duration::from_secs(3))
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            // Apply this per connection, including any pool reconnect.
            .pragma("secure_delete", "ON");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(Some(INDEX_IDLE_TIMEOUT))
            .connect_with(options)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS history_search_docs (
                id INTEGER PRIMARY KEY,
                document_key TEXT NOT NULL UNIQUE,
                scope_id TEXT NOT NULL,
                record_json TEXT NOT NULL,
                title TEXT NOT NULL,
                goal TEXT NOT NULL,
                updated_ms INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS history_search_scope ON history_search_docs(scope_id)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS history_search_recent ON history_search_docs(updated_ms)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE VIRTUAL TABLE IF NOT EXISTS history_search_fts
             USING fts5(title, goal, tokenize='unicode61')",
        )
        .execute(&pool)
        .await
        .context("SQLite build must support FTS5")?;
        harden_sqlite_files(&path)?;
        let file_stamp = before.filter(|stamp| {
            File::open(&path)
                .ok()
                .and_then(|file| HistoryFileStamp::read(&file))
                .is_some_and(|after| stamp.same_file_as(&after))
        });
        Ok(Self {
            pool,
            path,
            file_stamp,
        })
    }

    /// The index belongs to this private Host directory. Never reuse a stale
    /// handle after deletion/replacement, path changes, or idle pool shutdown.
    pub(crate) fn can_reuse(&self, private_runtime_dir: &Path) -> Result<bool> {
        if self.pool.is_closed()
            || self.pool.size() == 0
            || self.path != private_runtime_dir.join(HISTORY_INDEX_FILE)
        {
            return Ok(false);
        }
        let Some(stamp) = self.file_stamp.as_ref() else {
            return Ok(false);
        };
        let current = File::open(&self.path)
            .ok()
            .and_then(|file| HistoryFileStamp::read(&file));
        if !current.is_some_and(|current| stamp.same_file_as(&current)) {
            return Ok(false);
        }
        // Preserve the existing private-file permission checks on every search.
        prepare_private_runtime_directory(private_runtime_dir)?;
        harden_sqlite_files(&self.path)?;
        Ok(true)
    }

    pub(crate) async fn close(self) {
        self.pool.close().await;
    }

    /// Incremental upsert. Unchanged serialized records cause no disk update.
    /// A partial/truncated provider scan MUST NOT delete older indexed items.
    #[cfg(test)]
    async fn upsert_batch(
        &self,
        records: &[AgentSessionSearchRecord],
    ) -> Result<HistoryIndexWriteStats> {
        self.upsert_prepared(&PreparedHistoryBatch::new(records)?)
            .await
    }

    /// Prefer a single read-only equality check for likely unchanged previews.
    /// A miss is only a hint: the normal transaction re-reads current rows,
    /// preserves sequential duplicate statistics, and enforces retention.
    pub(crate) async fn upsert_warm_prepared(
        &self,
        batch: &PreparedHistoryBatch<'_>,
    ) -> Result<HistoryIndexWriteStats> {
        let mut checked = HistoryIndexWriteStats::default();
        if self
            .check_unchanged_batch(batch, MAX_INDEXED_RECORDS, &mut checked)
            .await?
        {
            checked.unchanged = batch.records.len();
            #[cfg(test)]
            {
                checked.warm_check_hits = 1;
            }
            return Ok(checked);
        }
        let mut stats = self.upsert_prepared(batch).await?;
        #[cfg(test)]
        {
            stats.json_buffer_starts += checked.json_buffer_starts;
            stats.warm_check_queries += checked.warm_check_queries;
            stats.retention_queries += checked.retention_queries;
        }
        Ok(stats)
    }

    async fn check_unchanged_batch(
        &self,
        batch: &PreparedHistoryBatch<'_>,
        retention_limit: i64,
        _stats: &mut HistoryIndexWriteStats,
    ) -> Result<bool> {
        if batch.records.is_empty() {
            return Ok(false);
        }
        // All records must satisfy exactly the same input rules as a write.
        for record in batch.records {
            validate_index_record(record)?;
        }
        // One bounded buffer, containing complete canonical JSON for each
        // record. Bind slices only after the buffer has finished growing.
        let mut bytes = Vec::new();
        let mut ranges = Vec::with_capacity(batch.records.len());
        #[cfg(test)]
        {
            _stats.json_buffer_starts += 1;
        }
        for record in batch.records {
            let start = bytes.len();
            serde_json::to_writer(&mut bytes, record)?;
            if bytes.len() > MAX_WARM_JSON_BYTES {
                return Ok(false);
            }
            ranges.push(start..bytes.len());
        }
        let json = std::str::from_utf8(&bytes)?;
        // Compare exact canonical text, not hashes or previously cached DB
        // state. Count retention in the same SQLite read snapshot. A missing,
        // changed, or noncanonical row forces the ordinary transaction.
        // 2 * MAX_BATCH + 1 bound values stay under the legacy 999 limit.
        let mut sql = QueryBuilder::<Sqlite>::new("WITH incoming(document_key, record_json) AS (");
        sql.push_values(batch.keys.iter().zip(ranges), |mut row, (key, range)| {
            row.push_bind(key.as_str()).push_bind(&json[range]);
        });
        sql.push(
            ") SELECT NOT EXISTS (
                SELECT 1 FROM incoming AS i
                LEFT JOIN history_search_docs AS d ON d.document_key = i.document_key
                WHERE d.id IS NULL OR typeof(d.record_json) != 'text'
                OR typeof(d.title) != 'text' OR typeof(d.goal) != 'text'
                OR d.record_json COLLATE BINARY IS NOT i.record_json
             ) AND (SELECT count(*) FROM history_search_docs) <= ",
        );
        sql.push_bind(retention_limit);
        #[cfg(test)]
        {
            _stats.warm_check_queries += 1;
            _stats.retention_queries += 1;
        }
        // One read-only statement, without an explicit transaction or any
        // materialized previous-document rows. Clear/search serialization
        // and index file/permission validation remain at the Host boundary.
        Ok(sql
            .build_query_scalar::<bool>()
            .fetch_one(&self.pool)
            .await?)
    }

    pub(crate) async fn upsert_prepared(
        &self,
        batch: &PreparedHistoryBatch<'_>,
    ) -> Result<HistoryIndexWriteStats> {
        let mut stats = HistoryIndexWriteStats::default();
        let mut tx = self.pool.begin().await?;
        let rows = if !batch.keys.is_empty() {
            let mut sql = QueryBuilder::<Sqlite>::new(
                "SELECT document_key, id, record_json, title, goal FROM history_search_docs WHERE document_key IN (",
            );
            {
                let mut values = sql.separated(", ");
                for key in &batch.keys {
                    values.push_bind(key.as_str());
                }
            }
            sql.push(")");
            sql.build().fetch_all(&mut *tx).await?
        } else {
            Vec::new()
        };
        #[cfg(test)]
        {
            stats.existing_rows_loaded = rows.len();
        }
        // SQLite rows already own the returned text. Borrow it until the
        // transaction finishes instead of copying four strings per document.
        let mut existing = HashMap::with_capacity(rows.len());
        for row in &rows {
            existing.insert(
                row.try_get::<&str, _>("document_key")?,
                IndexedDocument {
                    id: row.try_get("id")?,
                    record_json: row.try_get("record_json")?,
                    title: row.try_get("title")?,
                    goal: row.try_get("goal")?,
                },
            );
        }
        let mut pending: Vec<PendingDocument<'_>> = Vec::new();
        let mut pending_by_key: HashMap<&str, usize> = HashMap::new();
        // Most warm records are unchanged. Reuse the same serialization
        // storage rather than allocating an owned JSON string for each one.
        let mut json_scratch = Vec::new();
        for (record, key) in batch.iter() {
            validate_index_record(record)?;
            json_scratch.clear();
            #[cfg(test)]
            if json_scratch.capacity() == 0 {
                stats.json_buffer_starts += 1;
            }
            serde_json::to_writer(&mut json_scratch, record)?;
            // Preserve the logical sequential statistics and last record
            // for duplicate UUIDs, without materializing intermediate SQL
            // writes that are invisible outside this transaction.
            if let Some(&position) = pending_by_key.get(key) {
                let planned = &mut pending[position];
                if planned.record_json.as_bytes() == json_scratch.as_slice() {
                    stats.unchanged += 1;
                } else {
                    stats.updated += 1;
                    let record_json = String::from_utf8(std::mem::take(&mut json_scratch))?;
                    // A superseded pending record is no longer needed; reuse
                    // its backing allocation for the next comparison.
                    json_scratch =
                        std::mem::replace(&mut planned.record_json, record_json).into_bytes();
                    planned.record = record;
                }
                continue;
            }
            if existing
                .get(key)
                .is_some_and(|previous| previous.record_json.as_bytes() == json_scratch.as_slice())
            {
                stats.unchanged += 1;
                continue;
            }
            let previous = existing.remove(key);
            if previous.is_some() {
                stats.updated += 1;
            } else {
                stats.added += 1;
            }
            let scope = serde_json::to_string(&(
                &record.runtime_instance_id,
                &record.project_id,
                &record.workspace_id,
                &record.provider,
                &record.provider_account_id,
            ))?;
            let record_json = String::from_utf8(std::mem::take(&mut json_scratch))?;
            pending_by_key.insert(key, pending.len());
            pending.push(PendingDocument {
                key,
                scope,
                record_json,
                record,
                previous,
            });
        }
        // If two updates to an existing document cancel each other, leave its
        // original stored row/FTS text intact. Keep first-seen insertion order
        // so rowid-based retention tie-breaking remains deterministic.
        pending.retain(|planned| {
            planned
                .previous
                .as_ref()
                .is_none_or(|previous| previous.record_json != planned.record_json)
        });

        for batch in pending.chunks(SQL_WRITE_BATCH) {
            let mut sql = QueryBuilder::<Sqlite>::new(
                "INSERT INTO history_search_docs(document_key, scope_id, record_json, title, goal, updated_ms) ",
            );
            sql.push_values(batch, |mut values, planned| {
                values
                    .push_bind(planned.key)
                    .push_bind(&planned.scope)
                    .push_bind(&planned.record_json)
                    .push_bind(&planned.record.title)
                    .push_bind(&planned.record.goal)
                    .push_bind(planned.record.updated_at_ms.min(i64::MAX as u64) as i64);
            });
            sql.push(
                " ON CONFLICT(document_key) DO UPDATE SET scope_id=excluded.scope_id,
                 record_json=excluded.record_json, title=excluded.title,
                 goal=excluded.goal, updated_ms=excluded.updated_ms
                 RETURNING document_key, id",
            );
            let mut returned_ids = HashMap::with_capacity(batch.len());
            for row in sql.build().fetch_all(&mut *tx).await? {
                returned_ids.insert(
                    row.try_get::<String, _>("document_key")?,
                    row.try_get::<i64, _>("id")?,
                );
            }
            #[cfg(test)]
            {
                stats.write_statements += 1;
            }

            let mut remove_fts = Vec::new();
            let mut write_fts = Vec::new();
            for planned in batch {
                let id = returned_ids
                    .remove(planned.key)
                    .context("history upsert did not return document ID")?;
                if let Some(previous) = &planned.previous {
                    if previous.id != id {
                        bail!("history upsert changed existing document ID");
                    }
                }
                if planned.needs_fts_write() {
                    if planned.previous.is_some() {
                        remove_fts.push(id);
                    }
                    write_fts.push((id, planned.record));
                }
            }
            if !remove_fts.is_empty() {
                delete_index_rows(
                    &mut tx,
                    "DELETE FROM history_search_fts WHERE rowid IN (",
                    &remove_fts,
                )
                .await?;
                #[cfg(test)]
                {
                    stats.write_statements += 1;
                }
            }
            if !write_fts.is_empty() {
                let mut sql = QueryBuilder::<Sqlite>::new(
                    "INSERT INTO history_search_fts(rowid, title, goal) ",
                );
                sql.push_values(write_fts, |mut values, (id, record)| {
                    values
                        .push_bind(id)
                        .push_bind(&record.title)
                        .push_bind(&record.goal);
                });
                sql.build().execute(&mut *tx).await?;
                #[cfg(test)]
                {
                    stats.write_statements += 1;
                }
            }
        }
        trim_index(&mut tx, MAX_INDEXED_RECORDS, &mut stats).await?;
        tx.commit().await?;
        Ok(stats)
    }

    /// FTS input is tokenized and quoted; user input is never spliced into SQL.
    /// Prefix matching keeps Chinese technical terms searchable with unicode61.
    pub(crate) async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<AgentSessionSearchRecord>> {
        let Some(fts) = query_terms(query) else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query(
            "SELECT d.record_json FROM history_search_fts
             JOIN history_search_docs AS d ON history_search_fts.rowid=d.id
             WHERE history_search_fts MATCH ?
             ORDER BY bm25(history_search_fts), d.updated_ms DESC LIMIT ?",
        )
        .bind(fts)
        .bind(limit.clamp(1, MAX_RESULTS) as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let raw: &str = row.try_get("record_json")?;
                serde_json::from_str(raw).map_err(Into::into)
            })
            .collect()
    }

    /// Scope FTS BEFORE applying the global result limit. Without this,
    /// unrelated account/project hits can crowd out all authorized matches.
    /// Keys come exclusively from the bounded, currently observed native scan.
    #[cfg(test)]
    async fn search_scoped(
        &self,
        query: &AgentSessionSearchQuery,
        observed: &[AgentSessionSearchRecord],
    ) -> Result<Vec<AgentSessionSearchRecord>> {
        self.search_prepared(query, &PreparedHistoryBatch::new(observed)?)
            .await
    }

    pub(crate) async fn search_prepared(
        &self,
        query: &AgentSessionSearchQuery,
        batch: &PreparedHistoryBatch<'_>,
    ) -> Result<Vec<AgentSessionSearchRecord>> {
        let Some(fts) = query_terms(&query.text) else {
            return Ok(Vec::new());
        };
        let keys = batch
            .iter()
            .filter(|(record, _)| {
                query
                    .project_id
                    .as_ref()
                    .is_none_or(|id| &record.project_id == id)
                    && query
                        .workspace_id
                        .as_ref()
                        .is_none_or(|id| &record.workspace_id == id)
                    && query
                        .provider
                        .as_ref()
                        .is_none_or(|id| &record.provider == id)
                    && query
                        .provider_account_id
                        .as_ref()
                        .is_none_or(|id| record.provider_account_id.as_ref() == Some(id))
            })
            .map(|(_, key)| key)
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut sql = QueryBuilder::<Sqlite>::new(
            "SELECT d.record_json FROM history_search_fts
             JOIN history_search_docs AS d ON history_search_fts.rowid=d.id
             WHERE history_search_fts MATCH ",
        );
        sql.push_bind(fts);
        sql.push(" AND d.document_key IN (");
        {
            let mut values = sql.separated(", ");
            for key in keys {
                values.push_bind(key);
            }
        }
        sql.push(") ORDER BY bm25(history_search_fts), d.updated_ms DESC LIMIT ");
        sql.push_bind(MAX_RESULTS as i64);
        let rows = sql.build().fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                let raw: &str = row.try_get("record_json")?;
                serde_json::from_str(raw).map_err(Into::into)
            })
            .collect()
    }

    /// Explicit privacy switch-off removes indexed previews. Provider logs are
    /// untouched; the caller controls whether and when to reindex.
    pub(crate) async fn clear(&self) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM history_search_fts")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM history_search_docs")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        let row = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(&self.pool)
            .await?;
        if row.try_get::<i64, _>(0)? != 0 {
            bail!("history index WAL checkpoint is busy");
        }
        Ok(())
    }
}

fn validate_index_record(record: &AgentSessionSearchRecord) -> Result<()> {
    if record.running
        || record.provider_identity_verified
        || !record.source_exists
        || !matches!(record.provider.as_str(), "claude" | "codex")
        || record.project_id.is_empty()
        || record.workspace_id.is_empty()
        || record
            .provider_account_id
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        || record
            .provider_session_id
            .as_deref()
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
            .is_none()
    {
        bail!("index only bounded, scoped and unverified Provider history");
    }
    if record.title.chars().count() > 1024 || record.goal.chars().count() > 1024 {
        bail!("history search metadata exceeds the field budget");
    }
    Ok(())
}

// The SQL prefix is a fixed internal literal, never caller input.
async fn delete_index_rows(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    prefix: &'static str,
    ids: &[i64],
) -> Result<()> {
    debug_assert!(!ids.is_empty() && ids.len() <= SQL_WRITE_BATCH);
    let mut sql = QueryBuilder::<Sqlite>::new(prefix);
    {
        let mut values = sql.separated(", ");
        for id in ids {
            values.push_bind(id);
        }
    }
    sql.push(")");
    sql.build().execute(&mut **tx).await?;
    Ok(())
}

async fn trim_index(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    limit: i64,
    stats: &mut HistoryIndexWriteStats,
) -> Result<()> {
    debug_assert!(limit >= 0);
    loop {
        // Count and select in one statement, with a sentinel to detect more
        // work. Normally a batch can add at most MAX_BATCH rows, so a healthy
        // index needs just one query even when retention removes old rows.
        // An unexpectedly oversized index never allocates all expired IDs.
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM history_search_docs ORDER BY updated_ms ASC, id ASC
             LIMIT MIN(?, MAX(0, (SELECT count(*) FROM history_search_docs) - ?))",
        )
        .bind((MAX_BATCH + 1) as i64)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?;
        #[cfg(test)]
        {
            stats.retention_queries += 1;
            stats.retention_peak_ids = stats.retention_peak_ids.max(ids.len());
        }
        let has_more = ids.len() > MAX_BATCH;
        let ids = &ids[..ids.len().min(MAX_BATCH)];
        for batch in ids.chunks(SQL_WRITE_BATCH) {
            delete_index_rows(tx, "DELETE FROM history_search_fts WHERE rowid IN (", batch).await?;
            delete_index_rows(tx, "DELETE FROM history_search_docs WHERE id IN (", batch).await?;
            stats.trimmed += batch.len();
            #[cfg(test)]
            {
                stats.write_statements += 2;
            }
        }
        if !has_more {
            return Ok(());
        }
    }
}

fn query_terms(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .chars()
        .take(MAX_QUERY_CHARS)
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(8)
        .filter(|term| !term.is_empty() && term.chars().count() <= 64)
        .map(|term| format!("\"{term}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" OR "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alera_core::global_agent_search::SessionOpenAction;

    fn record(native: &str, title: &str, goal: &str) -> AgentSessionSearchRecord {
        AgentSessionSearchRecord {
            runtime_instance_id: "runtime".into(),
            project_id: "alera".into(),
            workspace_id: "ws1".into(),
            tab_id: String::new(),
            host_session_id: String::new(),
            provider: "codex".into(),
            provider_account_id: Some("primary".into()),
            profile_id: None,
            provider_session_id: Some(native.into()),
            provider_identity_verified: false,
            title: title.into(),
            goal: goal.into(),
            progress: String::new(),
            milestones: vec![],
            updated_at_ms: 42,
            running: false,
            source_exists: true,
        }
    }

    #[tokio::test]
    async fn index_queries_cjk_prefix_and_never_promotes_native_resume() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let r = record(id, "Windows 注音候選修正", "修正 Windows 注音候選窗");
        let stats = index.upsert_batch(&[r.clone()]).await.unwrap();
        assert_eq!(stats.added, 1);
        assert_eq!(index.upsert_batch(&[r]).await.unwrap().unchanged, 1);
        let hits = index.search("注音", 30).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].open_action(), SessionOpenAction::PreviewOnly);
        assert!(index
            .search("'; DROP TABLE history_search_docs; --", 30)
            .await
            .is_ok());
        index.clear().await.unwrap();
        assert!(index.search("注音", 30).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reusable_index_preserves_secure_delete_after_reconnect() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        assert!(index.can_reuse(dir.path()).unwrap());
        assert!(!index.can_reuse(other.path()).unwrap());
        let connection = index.pool.acquire().await.unwrap();
        connection.close().await.unwrap();
        let secure_delete: i64 = sqlx::query_scalar("PRAGMA secure_delete")
            .fetch_one(&index.pool)
            .await
            .unwrap();
        assert_eq!(secure_delete, 1);
        index.clear().await.unwrap();
        index.pool.close().await;
        assert!(!index.can_reuse(dir.path()).unwrap());
    }

    #[tokio::test]
    async fn metadata_only_updates_do_not_rewrite_fts() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let mut r = record(
            "550e8400-e29b-41d4-a716-446655440000",
            "Windows",
            "keyboard",
        );
        index.upsert_batch(&[r.clone()]).await.unwrap();
        let changes_before: i64 = sqlx::query_scalar("SELECT total_changes()")
            .fetch_one(&index.pool)
            .await
            .unwrap();
        r.updated_at_ms += 1000;
        let stats = index.upsert_batch(&[r.clone()]).await.unwrap();
        assert_eq!(stats.updated, 1);
        let changes_after: i64 = sqlx::query_scalar("SELECT total_changes()")
            .fetch_one(&index.pool)
            .await
            .unwrap();
        assert_eq!(
            changes_after - changes_before,
            1,
            "only the document row changes"
        );
        let hits = index.search("keyboard", 30).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].updated_at_ms, r.updated_at_ms);

        r.goal = "replacement".into();
        index.upsert_batch(&[r.clone()]).await.unwrap();
        assert!(index.search("keyboard", 30).await.unwrap().is_empty());
        assert_eq!(index.search("replacement", 30).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn batched_lookup_preserves_duplicate_key_order_and_account_scope() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let first = record("550e8400-e29b-41d4-a716-446655440000", "first", "original");
        let mut second = first.clone();
        second.goal = "replacement".into();
        let mut other = first.clone();
        other.provider_account_id = Some("secondary".into());
        let stats = index
            .upsert_batch(&[first.clone(), second.clone(), second.clone(), other])
            .await
            .unwrap();
        assert_eq!(stats.added, 2);
        assert_eq!(stats.updated, 1);
        assert_eq!(stats.unchanged, 1);
        let hits = index.search("replacement", 30).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].provider_account_id.as_deref(), Some("primary"));
        let hits = index.search("original", 30).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].provider_account_id.as_deref(), Some("secondary"));
        let stats = index.upsert_batch(&[second, first]).await.unwrap();
        assert_eq!(stats.unchanged, 1);
        assert_eq!(stats.updated, 1);
        assert!(index.search("replacement", 30).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn separate_account_and_provider_records_are_never_merged() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let first = record(uuid, "Dashboard migration", "Agent overview");
        let mut second = first.clone();
        second.provider_account_id = Some("secondary".into());
        second.title = "Dashboard second account".into();
        let mut third = first.clone();
        third.provider = "claude".into();
        third.title = "Dashboard Claude".into();
        assert_eq!(
            index
                .upsert_batch(&[first, second, third])
                .await
                .unwrap()
                .added,
            3
        );
        assert_eq!(index.search("Dashboard", 20).await.unwrap().len(), 3);
        assert!(index
            .upsert_batch(&[record(uuid, "invalid", "")])
            .await
            .is_ok());
        let after = index.search("Dashboard", 20).await.unwrap();
        assert_eq!(
            after.len(),
            2,
            "updating one account must not overwrite others"
        );
    }

    #[tokio::test]
    async fn scoped_fts_filters_current_account_before_result_limit() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let primary = record(
            "550e8400-e29b-41d4-a716-446655440001",
            "Windows Windows Windows troubleshooting",
            "Windows desktop",
        );
        let mut secondary = record(
            "550e8400-e29b-41d4-a716-446655440002",
            "Windows IME",
            "native keyboard",
        );
        secondary.provider_account_id = Some("secondary".into());
        index
            .upsert_batch(&[primary.clone(), secondary.clone()])
            .await
            .unwrap();
        let query = AgentSessionSearchQuery {
            text: "Windows".into(),
            provider_account_id: Some("secondary".into()),
            max_results: 1,
            ..Default::default()
        };
        let results = index
            .search_scoped(&query, &[primary.clone(), secondary.clone()])
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider_account_id.as_deref(), Some("secondary"));
        // Cached secondary history is never returned after it disappears
        // from the currently authorized native scan.
        assert!(index
            .search_scoped(&query, &[primary])
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn prepared_batch_reuses_keys_and_preserves_scoped_results() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let mut first = record(
            "550e8400-e29b-41d4-a716-446655440001",
            "keyboard first",
            "keyboard",
        );
        first.project_id = "project\\with\"quotes".to_owned();
        first.profile_id = Some("profile-a".to_owned());
        let mut second = first.clone();
        second.profile_id = Some("profile-b".to_owned());
        second.title = "keyboard second".to_owned();
        let mut other = first.clone();
        other.provider_account_id = Some("secondary".to_owned());
        let records = vec![first, second, other];
        let batch = PreparedHistoryBatch::new(&records).unwrap();
        let key_ptrs = batch
            .keys
            .iter()
            .map(|key| key.as_ptr())
            .collect::<Vec<_>>();
        assert_ne!(batch.keys[0], batch.keys[1]);
        assert_ne!(batch.keys[0], batch.keys[2]);
        for (i, (record, key)) in batch.iter().enumerate() {
            assert!(std::ptr::eq(record, &records[i]));
            assert_eq!(key.as_ptr(), key_ptrs[i]);
            assert_eq!(key, document_key(record).unwrap());
        }
        assert_eq!(index.upsert_prepared(&batch).await.unwrap().added, 3);
        let warm = index.upsert_prepared(&batch).await.unwrap();
        assert_eq!(warm.unchanged, 3);
        assert_eq!(warm.write_statements, 0);
        let query = AgentSessionSearchQuery {
            text: "keyboard".to_owned(),
            project_id: Some(records[0].project_id.clone()),
            provider_account_id: Some("primary".to_owned()),
            max_results: 10,
            ..Default::default()
        };
        let hits = index.search_prepared(&query, &batch).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.contains(&records[0]));
        assert!(hits.contains(&records[1]));
        assert!(!hits.contains(&records[2]));
        assert_eq!(
            key_ptrs,
            batch
                .keys
                .iter()
                .map(|key| key.as_ptr())
                .collect::<Vec<_>>()
        );
        let subset = PreparedHistoryBatch::new(&records[1..2]).unwrap();
        assert_eq!(
            index.search_prepared(&query, &subset).await.unwrap(),
            vec![records[1].clone()]
        );
    }

    #[tokio::test]
    async fn bulk_writes_are_bounded_and_leave_unchanged_or_metadata_only_fts_intact() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let mut records = (0..MAX_BATCH)
            .map(|i| {
                record(
                    &format!("550e8400-e29b-41d4-a716-{i:012}"),
                    "original",
                    "keyboard",
                )
            })
            .collect::<Vec<_>>();
        let first = index.upsert_batch(&records).await.unwrap();
        assert_eq!(first.added, MAX_BATCH);
        assert_eq!(first.json_buffer_starts, MAX_BATCH);
        assert_eq!(first.retention_queries, 1);
        assert_eq!(
            first.write_statements, 4,
            "two document batches and two FTS batches"
        );
        let repeated = index.upsert_batch(&records).await.unwrap();
        assert_eq!(repeated.unchanged, MAX_BATCH);
        assert_eq!(repeated.write_statements, 0);
        assert_eq!(repeated.json_buffer_starts, 1);
        assert_eq!(repeated.retention_queries, 1);
        assert_eq!(repeated.retention_peak_ids, 0);
        let warm = index
            .upsert_warm_prepared(&PreparedHistoryBatch::new(&records).unwrap())
            .await
            .unwrap();
        assert_eq!(warm.unchanged, MAX_BATCH);
        assert_eq!(warm.warm_check_queries, 1);
        assert_eq!(warm.warm_check_hits, 1);
        assert_eq!(warm.existing_rows_loaded, 0);
        assert_eq!(warm.write_statements, 0);
        assert_eq!(warm.retention_queries, 1);
        for record in &mut records {
            record.updated_at_ms += 1;
        }
        let metadata = index.upsert_batch(&records).await.unwrap();
        assert_eq!(metadata.updated, MAX_BATCH);
        assert_eq!(
            metadata.write_statements, 2,
            "no FTS rebuild for metadata-only changes"
        );
        for record in &mut records {
            record.goal = "replacement".into();
        }
        let changed = index.upsert_batch(&records).await.unwrap();
        assert_eq!(changed.updated, MAX_BATCH);
        assert_eq!(
            changed.write_statements, 6,
            "bounded document, FTS delete, FTS insert batches"
        );
        assert!(index.search("keyboard", 100).await.unwrap().is_empty());
        let matches: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM history_search_fts WHERE history_search_fts MATCH 'replacement'",
        )
        .fetch_one(&index.pool)
        .await
        .unwrap();
        assert_eq!(matches, MAX_BATCH as i64);
    }

    #[tokio::test]
    async fn warm_check_avoids_old_rows_and_preserves_exact_fallback_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let first = record(
            "550e8400-e29b-41d4-a716-446655440000",
            "keyboard \"quoted\" \\ path",
            "line\n\u{6b77}\u{53f2}\t\u{1f680}",
        );
        let mut other = first.clone();
        other.provider_account_id = Some("secondary".to_owned());
        let mut records = vec![first.clone(), other.clone(), first.clone()];
        index.upsert_batch(&records).await.unwrap();
        let batch = PreparedHistoryBatch::new(&records).unwrap();
        let warm = index.upsert_warm_prepared(&batch).await.unwrap();
        assert_eq!(warm.unchanged, 3);
        assert_eq!(warm.warm_check_queries, 1);
        assert_eq!(warm.warm_check_hits, 1);
        assert_eq!(warm.existing_rows_loaded, 0);
        assert_eq!(warm.json_buffer_starts, 1);
        assert_eq!(warm.retention_queries, 1);
        assert_eq!(warm.write_statements, 0);

        // Even an identical batch cannot bypass retention if the index grew.
        let mut checked = HistoryIndexWriteStats::default();
        assert!(!index
            .check_unchanged_batch(&batch, 1, &mut checked)
            .await
            .unwrap());
        assert_eq!(checked.warm_check_queries, 1);

        // A same-file external write must be observed, not hidden by a
        // remembered upsert. Noncanonical JSON still requires repair.
        let key = document_key(&first).unwrap();
        sqlx::query("UPDATE history_search_docs SET record_json = ? WHERE document_key = ?")
            .bind(serde_json::to_string_pretty(&first).unwrap())
            .bind(&key)
            .execute(&index.pool)
            .await
            .unwrap();
        let repaired = index.upsert_warm_prepared(&batch).await.unwrap();
        assert_eq!(repaired.updated, 1);
        assert_eq!(repaired.unchanged, 2);
        assert_eq!(repaired.warm_check_hits, 0);
        assert_eq!(repaired.existing_rows_loaded, 2);
        assert_eq!(repaired.write_statements, 1);
        let stored: String = sqlx::query_scalar(
            "SELECT record_json FROM history_search_docs WHERE document_key = ?",
        )
        .bind(&key)
        .fetch_one(&index.pool)
        .await
        .unwrap();
        assert_eq!(stored, serde_json::to_string(&first).unwrap());

        // Conflicting duplicates must still execute the logical sequential
        // update path, including the case where they revert to the first row.
        records[0].goal = "temporary".to_owned();
        let batch = PreparedHistoryBatch::new(&records).unwrap();
        let reverted = index.upsert_warm_prepared(&batch).await.unwrap();
        assert_eq!(reverted.updated, 2);
        assert_eq!(reverted.unchanged, 1);
        assert_eq!(reverted.warm_check_hits, 0);
        assert_eq!(reverted.write_statements, 0);
        assert!(index.search("temporary", 10).await.unwrap().is_empty());

        // Missing rows (including after clear) need the original insert path.
        index.clear().await.unwrap();
        let records = [first, other];
        let batch = PreparedHistoryBatch::new(&records).unwrap();
        let recreated = index.upsert_warm_prepared(&batch).await.unwrap();
        assert_eq!(recreated.added, 2);
        assert_eq!(recreated.warm_check_hits, 0);
        assert_eq!(
            index
                .upsert_warm_prepared(&batch)
                .await
                .unwrap()
                .warm_check_hits,
            1
        );

        // No early-return path may accept an otherwise invalid record.
        let mut invalid = records[0].clone();
        invalid.provider_identity_verified = true;
        let invalid = [invalid];
        assert!(index
            .upsert_warm_prepared(&PreparedHistoryBatch::new(&invalid).unwrap())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn duplicate_updates_that_revert_to_original_do_not_touch_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let first = record("550e8400-e29b-41d4-a716-446655440000", "first", "original");
        index.upsert_batch(&[first.clone()]).await.unwrap();
        let mut changed = first.clone();
        changed.goal = "temporary".into();
        let stats = index
            .upsert_batch(&[changed, first.clone(), first])
            .await
            .unwrap();
        assert_eq!(stats.updated, 2);
        assert_eq!(stats.unchanged, 1);
        assert_eq!(stats.json_buffer_starts, 2);
        assert_eq!(stats.write_statements, 0);
        assert!(index.search("temporary", 30).await.unwrap().is_empty());
        assert_eq!(index.search("original", 30).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn batched_retention_deletes_matching_fts_rows_in_stable_id_order() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let rows = (0..MAX_BATCH)
            .map(|i| {
                record(
                    &format!("550e8400-e29b-41d4-a716-{i:012}"),
                    "retention",
                    "keyboard",
                )
            })
            .collect::<Vec<_>>();
        index.upsert_batch(&rows).await.unwrap();
        let mut tx = index.pool.begin().await.unwrap();
        let mut stats = HistoryIndexWriteStats::default();
        trim_index(&mut tx, 4, &mut stats).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(stats.trimmed, MAX_BATCH - 4);
        assert_eq!(stats.write_statements, 4);
        assert_eq!(stats.retention_queries, 1);
        assert_eq!(stats.retention_peak_ids, MAX_BATCH - 4);
        let remaining: Vec<String> =
            sqlx::query_scalar("SELECT document_key FROM history_search_docs ORDER BY id ASC")
                .fetch_all(&index.pool)
                .await
                .unwrap();
        assert_eq!(
            remaining,
            rows[MAX_BATCH - 4..]
                .iter()
                .map(|row| document_key(row).unwrap())
                .collect::<Vec<_>>()
        );
        let fts_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM history_search_fts WHERE history_search_fts MATCH 'retention'",
        )
        .fetch_one(&index.pool)
        .await
        .unwrap();
        assert_eq!(fts_count, 4);
    }

    #[tokio::test]
    async fn warm_json_comparison_preserves_exact_bytes_and_repairs_noncanonical_rows() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let mut first = record(
            "550e8400-e29b-41d4-a716-446655440000",
            "keyboard \"quoted\" \\ path",
            "line one\n\u{6b77}\u{53f2}\tline two",
        );
        first.milestones = vec!["\\\"\u{1f680}".to_owned()];
        index
            .upsert_batch(std::slice::from_ref(&first))
            .await
            .unwrap();
        let canonical = serde_json::to_string(&first).unwrap();
        let key = document_key(&first).unwrap();
        let stored: String = sqlx::query_scalar(
            "SELECT record_json FROM history_search_docs WHERE document_key = ?",
        )
        .bind(&key)
        .fetch_one(&index.pool)
        .await
        .unwrap();
        assert_eq!(stored, canonical);
        let warm = index
            .upsert_batch(std::slice::from_ref(&first))
            .await
            .unwrap();
        assert_eq!(warm.unchanged, 1);
        assert_eq!(warm.write_statements, 0);

        // Even semantically equivalent but noncanonical JSON is rewritten,
        // exactly as before this optimization; FTS text is unchanged.
        let pretty = serde_json::to_string_pretty(&first).unwrap();
        assert_ne!(pretty, canonical);
        sqlx::query("UPDATE history_search_docs SET record_json = ? WHERE document_key = ?")
            .bind(pretty)
            .bind(&key)
            .execute(&index.pool)
            .await
            .unwrap();
        let repaired = index
            .upsert_batch(std::slice::from_ref(&first))
            .await
            .unwrap();
        assert_eq!(repaired.updated, 1);
        assert_eq!(repaired.write_statements, 1);
        let stored: String = sqlx::query_scalar(
            "SELECT record_json FROM history_search_docs WHERE document_key = ?",
        )
        .bind(key)
        .fetch_one(&index.pool)
        .await
        .unwrap();
        assert_eq!(stored, canonical);
        assert_eq!(index.search("keyboard", 10).await.unwrap(), vec![first]);
    }

    #[tokio::test]
    async fn retention_pages_bound_ids_and_keep_newest_ties() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let rows = (0..MAX_BATCH * 2)
            .map(|i| {
                record(
                    &format!("550e8400-e29b-41d4-a716-{i:012}"),
                    "retention",
                    "keyboard",
                )
            })
            .collect::<Vec<_>>();
        for batch in rows.chunks(MAX_BATCH) {
            index.upsert_batch(batch).await.unwrap();
        }
        let mut tx = index.pool.begin().await.unwrap();
        let mut stats = HistoryIndexWriteStats::default();
        trim_index(&mut tx, 4, &mut stats).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(stats.trimmed, MAX_BATCH * 2 - 4);
        assert_eq!(stats.write_statements, 8);
        assert_eq!(stats.retention_queries, 2);
        assert_eq!(stats.retention_peak_ids, MAX_BATCH + 1);
        let remaining: Vec<String> =
            sqlx::query_scalar("SELECT document_key FROM history_search_docs ORDER BY id ASC")
                .fetch_all(&index.pool)
                .await
                .unwrap();
        assert_eq!(
            remaining,
            rows[rows.len() - 4..]
                .iter()
                .map(|row| document_key(row).unwrap())
                .collect::<Vec<_>>()
        );
        let fts_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM history_search_fts WHERE history_search_fts MATCH 'retention'",
        )
        .fetch_one(&index.pool)
        .await
        .unwrap();
        assert_eq!(fts_count, 4);
    }

    #[tokio::test]
    async fn batch_rejects_unverified_scope_or_oversized_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let index = AgentSessionHistoryIndex::open(dir.path()).await.unwrap();
        let mut record = record("550e8400-e29b-41d4-a716-446655440000", "Fix", "Goal");
        record.provider_identity_verified = true;
        assert!(index.upsert_batch(&[record.clone()]).await.is_err());
        record.provider_identity_verified = false;
        record.goal = "a".repeat(1025);
        assert!(index.upsert_batch(&[record.clone()]).await.is_err());
        record.goal = "safe".into();
        assert!(index
            .upsert_batch(&vec![record; MAX_BATCH + 1])
            .await
            .is_err());
        assert!(index.search("", 30).await.unwrap().is_empty());
    }
}
