//! Explicit opt-in pipeline: registered Worktree -> bounded provider scan ->
//! private FTS5 cache -> typed core ranking. The Host route authorizes local
//! callers and provides trusted registered Workspace and account scopes.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use alera_core::global_agent_search::{
    search_agent_sessions, AgentSessionSearchHit, AgentSessionSearchQuery,
};
use anyhow::{bail, Result};

use crate::agent_session_history_fts::{
    AgentSessionHistoryIndex, HistoryIndexWriteStats, HISTORY_INDEX_FILE,
};
use crate::agent_session_history_scan::{
    discover_history_cached, HistoryPreviewCache, HistoryScanPolicy, ProviderHistoryRoot,
    RegisteredHistoryWorkspace,
};

/// One state per Host, shared by deferred and inline search/clear requests.
pub(crate) type HistorySearchState = Arc<Mutex<HistorySearchCache>>;

#[derive(Default)]
pub(crate) struct HistorySearchCache {
    previews: HistoryPreviewCache,
    directory: Option<PathBuf>,
    index: Option<AgentSessionHistoryIndex>,
}

impl HistorySearchCache {
    async fn select_directory(&mut self, directory: &Path) {
        if self.directory.as_deref() != Some(directory) {
            self.close_index().await;
            self.previews = HistoryPreviewCache::default();
            self.directory = Some(directory.to_owned());
        }
    }

    async fn close_index(&mut self) {
        if let Some(index) = self.index.take() {
            index.close().await;
        }
    }

    async fn ensure_index(&mut self, directory: &Path) -> Result<bool> {
        let reusable = self
            .index
            .as_ref()
            .map(|index| index.can_reuse(directory))
            .transpose();
        match reusable {
            Ok(Some(true)) => return Ok(true),
            Err(error) => {
                self.close_index().await;
                return Err(error);
            }
            _ => {}
        }
        self.close_index().await;
        self.index = Some(AgentSessionHistoryIndex::open(directory).await?);
        Ok(false)
    }
}

#[derive(Debug)]
pub(crate) struct HistorySearchResponse {
    pub hits: Vec<AgentSessionSearchHit>,
    pub visited_files: usize,
    pub parsed_files: usize,
    pub reused_files: usize,
    pub index_reused: bool,
    pub scan_truncated: bool,
    pub writes: HistoryIndexWriteStats,
}

/// Never run at boot or as a background timer. The caller explicitly opts
/// into indexing the first user message, and nothing leaves the local machine.
pub(crate) async fn search_history_on_demand(
    runtime_cache_dir: &Path,
    state: &HistorySearchState,
    roots: &[ProviderHistoryRoot],
    registered_workspaces: &[RegisteredHistoryWorkspace],
    query: &AgentSessionSearchQuery,
    consent_to_index_user_text: bool,
) -> Result<HistorySearchResponse> {
    if !consent_to_index_user_text {
        bail!("history content search requires explicit local opt-in");
    }
    let mut empty = HistorySearchResponse {
        hits: Vec::new(),
        visited_files: 0,
        parsed_files: 0,
        reused_files: 0,
        index_reused: false,
        scan_truncated: false,
        writes: HistoryIndexWriteStats::default(),
    };
    if query.text.trim().is_empty() {
        return Ok(empty);
    }
    let roots = roots.to_vec();
    let registered = registered_workspaces.to_vec();
    // Hold the owned guard through indexing, so an explicit clear cannot race
    // with a search and leave an immediately repopulated private cache.
    let mut guard = state.clone().lock_owned().await;
    guard.select_directory(runtime_cache_dir).await;
    let (scan, mut guard) = tokio::task::spawn_blocking(move || {
        let scan = discover_history_cached(
            &roots,
            &registered,
            HistoryScanPolicy {
                index_user_text: true,
                max_files: 256,
            },
            &mut guard.previews,
        );
        (scan, guard)
    })
    .await?;
    empty.visited_files = scan.visited_files;
    empty.parsed_files = scan.parsed_files;
    empty.reused_files = scan.reused_files;
    empty.scan_truncated = scan.truncated;
    if scan.records.is_empty() {
        return Ok(empty);
    }

    empty.index_reused = guard.ensure_index(runtime_cache_dir).await?;
    let candidates = async {
        let index = guard.index.as_ref().expect("index opened above");
        // No partial, unverified or out-of-scope rows may become resumable solely
        // because they were present in a previous local SQLite index generation.
        for chunk in scan.records.chunks(256) {
            let write = index.upsert_batch(chunk).await?;
            empty.writes.added += write.added;
            empty.writes.updated += write.updated;
            empty.writes.unchanged += write.unchanged;
            empty.writes.trimmed += write.trimmed;
        }
        // Scope and current-file identity constrain FTS before top-N limiting;
        // otherwise other accounts can hide relevant rows from a filtered query.
        index.search_scoped(query, &scan.records).await
    }
    .await;
    let candidates = match candidates {
        Ok(candidates) => candidates,
        Err(error) => {
            // A failed connection/schema must not poison every later request.
            guard.close_index().await;
            return Err(error);
        }
    };
    let observed = scan
        .records
        .iter()
        .filter_map(|record| {
            Some((
                record.runtime_instance_id.clone(),
                record.project_id.clone(),
                record.workspace_id.clone(),
                record.provider.clone(),
                record.provider_account_id.clone(),
                record.provider_session_id.clone()?,
            ))
        })
        .collect::<HashSet<_>>();
    let candidates = candidates.into_iter().filter(|record| {
        observed.contains(&(
            record.runtime_instance_id.clone(),
            record.project_id.clone(),
            record.workspace_id.clone(),
            record.provider.clone(),
            record.provider_account_id.clone(),
            record.provider_session_id.clone().unwrap_or_default(),
        ))
    });
    empty.hits = search_agent_sessions(candidates, query);
    Ok(empty)
}

/// Clearing the local search cache never modifies provider-native histories.
pub(crate) async fn clear_history_search_cache(
    runtime_cache_dir: &Path,
    state: &HistorySearchState,
) -> Result<()> {
    let mut guard = state.lock().await;
    guard.select_directory(runtime_cache_dir).await;
    guard.previews = HistoryPreviewCache::default();
    let path = runtime_cache_dir.join(HISTORY_INDEX_FILE);
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => {
            guard.close_index().await;
            return Ok(());
        }
        Err(error) => {
            guard.close_index().await;
            return Err(error.into());
        }
    }
    guard.ensure_index(runtime_cache_dir).await?;
    let result = guard
        .index
        .as_ref()
        .expect("index opened above")
        .clear()
        .await;
    // Release SQLite/WAL handles after explicit privacy clear, even on error.
    guard.close_index().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn explicit_opt_in_indexes_scoped_codex_and_clearing_preserves_provider() {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let state = HistorySearchState::default();
        let work = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let sessions = root.path().join("sessions/2026/11/21");
        std::fs::create_dir_all(&sessions).unwrap();
        let id = "550e8400-e29b-41d4-a716-446655440009";
        let file = sessions.join(format!("rollout-2026-11-21T10-00-00-{id}.jsonl"));
        std::fs::write(&file, format!("{}\n{}\n",
            json!({"type":"session_meta","payload":{"id":id,"cwd":work.path()}}),
            json!({"type":"event_msg","payload":{"type":"user_message","message":"Windows 注音搜尋除錯"}}),
        )).unwrap();
        let alien = "550e8400-e29b-41d4-a716-446655440008";
        std::fs::write(
            sessions.join(format!("rollout-day-{alien}.jsonl")),
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":alien,"cwd":other.path()}})
            ),
        )
        .unwrap();
        let roots = vec![ProviderHistoryRoot {
            provider: "codex",
            account_id: "main".into(),
            path: root.path().join("sessions"),
        }];
        let registered = vec![RegisteredHistoryWorkspace {
            runtime_instance_id: "runtime".into(),
            project_id: "alera".into(),
            workspace_id: "ws".into(),
            path: work.path().into(),
        }];
        let query = AgentSessionSearchQuery {
            text: "注音".into(),
            max_results: 10,
            ..Default::default()
        };
        assert!(
            search_history_on_demand(cache.path(), &state, &roots, &registered, &query, false)
                .await
                .is_err()
        );
        assert!(!cache.path().join("agent-session-search.sqlite").exists());
        let first =
            search_history_on_demand(cache.path(), &state, &roots, &registered, &query, true)
                .await
                .unwrap();
        assert_eq!(first.hits.len(), 1);
        assert_eq!(first.writes.added, 1);
        assert!(!first.index_reused);
        assert_eq!(first.parsed_files, 2);
        assert_eq!(first.reused_files, 0);
        assert_eq!(first.hits[0].session.workspace_id, "ws");
        let second =
            search_history_on_demand(cache.path(), &state, &roots, &registered, &query, true)
                .await
                .unwrap();
        assert_eq!(second.hits.len(), 1);
        assert_eq!(second.writes.unchanged, 1);
        assert!(second.index_reused);
        assert_eq!(
            second.parsed_files, 1,
            "unregistered history is never retained"
        );
        assert_eq!(second.reused_files, 1);
        clear_history_search_cache(cache.path(), &state)
            .await
            .unwrap();
        assert!(state.lock().await.index.is_none());
        let third =
            search_history_on_demand(cache.path(), &state, &roots, &registered, &query, true)
                .await
                .unwrap();
        assert_eq!(third.reused_files, 0);
        assert!(!third.index_reused);
        assert_eq!(third.parsed_files, 2);
        assert_eq!(third.writes.added, 1);
        assert!(
            file.exists(),
            "deleting FTS cache must not change native provider JSONL"
        );
    }
}
