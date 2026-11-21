//! Separate, private SQLite FTS5 cache of consented/bounded Agent history.
//!
//! Provider JSONL files are never modified. Only summaries sanitized by
//! agent_session_history_scan may be stored. No ambient startup indexing.
use std::path::Path;
use std::time::Duration;

use alera_core::global_agent_search::AgentSessionSearchRecord;
use alera_core::runtime::{
    harden_sqlite_files, open_private_runtime_file, prepare_private_runtime_directory,
};
use anyhow::{bail, Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

const MAX_BATCH: usize = 256;
const MAX_RESULTS: usize = 100;
const MAX_INDEXED_RECORDS: i64 = 20_000;
const MAX_QUERY_CHARS: usize = 256;

pub(crate) struct AgentSessionHistoryIndex {
    pool: SqlitePool,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct HistoryIndexWriteStats {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub trimmed: usize,
}

impl AgentSessionHistoryIndex {
    pub(crate) async fn open(private_runtime_dir: &Path) -> Result<Self> {
        prepare_private_runtime_directory(private_runtime_dir)?;
        let path = private_runtime_dir.join("agent-session-search.sqlite");
        open_private_runtime_file(&path)?;
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .busy_timeout(Duration::from_secs(3))
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        // SQLite and FTS shadow tables may contain user-opted-in text.
        sqlx::query("PRAGMA secure_delete = ON")
            .execute(&pool)
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
        Ok(Self { pool })
    }

    /// Incremental upsert. Unchanged serialized records cause no disk update.
    /// A partial/truncated provider scan MUST NOT delete older indexed items.
    pub(crate) async fn upsert_batch(
        &self,
        records: &[AgentSessionSearchRecord],
    ) -> Result<HistoryIndexWriteStats> {
        if records.len() > MAX_BATCH {
            bail!("history index batch exceeds bounded limit");
        }
        let mut stats = HistoryIndexWriteStats::default();
        let mut tx = self.pool.begin().await?;
        for record in records {
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
            let key = serde_json::to_string(&(
                &record.runtime_instance_id,
                &record.project_id,
                &record.workspace_id,
                &record.provider,
                &record.provider_account_id,
                &record.profile_id,
                &record.provider_session_id,
            ))?;
            let scope = serde_json::to_string(&(
                &record.runtime_instance_id,
                &record.project_id,
                &record.workspace_id,
                &record.provider,
                &record.provider_account_id,
            ))?;
            let record_json = serde_json::to_string(record)?;
            let existing = sqlx::query(
                "SELECT id, record_json FROM history_search_docs WHERE document_key = ?",
            )
            .bind(&key)
            .fetch_optional(&mut *tx)
            .await?;
            match existing {
                Some(row) => {
                    let id: i64 = row.try_get("id")?;
                    let previous: String = row.try_get("record_json")?;
                    if previous == record_json {
                        stats.unchanged += 1;
                        continue;
                    }
                    sqlx::query(
                        "UPDATE history_search_docs SET scope_id=?, record_json=?, title=?,
                         goal=?, updated_ms=? WHERE id=?",
                    )
                    .bind(&scope)
                    .bind(&record_json)
                    .bind(&record.title)
                    .bind(&record.goal)
                    .bind(record.updated_at_ms.min(i64::MAX as u64) as i64)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    sqlx::query("DELETE FROM history_search_fts WHERE rowid=?")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query(
                        "INSERT INTO history_search_fts(rowid, title, goal) VALUES (?, ?, ?)",
                    )
                    .bind(id)
                    .bind(&record.title)
                    .bind(&record.goal)
                    .execute(&mut *tx)
                    .await?;
                    stats.updated += 1;
                }
                None => {
                    let insert = sqlx::query(
                        "INSERT INTO history_search_docs(
                            document_key, scope_id, record_json, title, goal, updated_ms
                        ) VALUES (?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&key)
                    .bind(&scope)
                    .bind(&record_json)
                    .bind(&record.title)
                    .bind(&record.goal)
                    .bind(record.updated_at_ms.min(i64::MAX as u64) as i64)
                    .execute(&mut *tx)
                    .await?;
                    let id = insert.last_insert_rowid();
                    sqlx::query(
                        "INSERT INTO history_search_fts(rowid, title, goal) VALUES (?, ?, ?)",
                    )
                    .bind(id)
                    .bind(&record.title)
                    .bind(&record.goal)
                    .execute(&mut *tx)
                    .await?;
                    stats.added += 1;
                }
            }
        }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM history_search_docs")
            .fetch_one(&mut *tx)
            .await?;
        if count > MAX_INDEXED_RECORDS {
            let remove = count - MAX_INDEXED_RECORDS;
            let ids: Vec<i64> = sqlx::query_scalar(
                "SELECT id FROM history_search_docs
                 ORDER BY updated_ms ASC, id ASC LIMIT ?",
            )
            .bind(remove)
            .fetch_all(&mut *tx)
            .await?;
            for id in ids {
                sqlx::query("DELETE FROM history_search_fts WHERE rowid=?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM history_search_docs WHERE id=?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                stats.trimmed += 1;
            }
        }
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
                let raw: String = row.try_get("record_json")?;
                serde_json::from_str(&raw).map_err(Into::into)
            })
            .collect()
    }

    /// Explicit privacy switch-off removes indexed previews. Provider logs are
    /// untouched; the caller controls whether and when to reindex.
    pub(crate) async fn clear(&self) -> Result<()> {
        sqlx::query("DELETE FROM history_search_fts")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM history_search_docs")
            .execute(&self.pool)
            .await?;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await?;
        Ok(())
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
