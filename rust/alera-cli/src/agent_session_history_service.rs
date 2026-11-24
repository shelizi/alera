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

use crate::agent_session_history_fts::{AgentSessionHistoryIndex, HistoryIndexWriteStats};
use crate::agent_session_history_scan::{
    discover_history_cached, HistoryPreviewCache, HistoryScanPolicy, ProviderHistoryRoot,
    RegisteredHistoryWorkspace,
};

/// One state per Host, shared by deferred and inline search/clear requests.
pub(crate) type HistorySearchState = Arc<Mutex<HistoryPreviewCache>>;

#[derive(Debug)]
pub(crate) struct HistorySearchResponse {
    pub hits: Vec<AgentSessionSearchHit>,
    pub visited_files: usize,
    pub parsed_files: usize,
    pub reused_files: usize,
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
    let (scan, _guard) = tokio::task::spawn_blocking(move || {
        let scan = discover_history_cached(
            &roots,
            &registered,
            HistoryScanPolicy {
                index_user_text: true,
                max_files: 256,
            },
            &mut guard,
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

    let index = AgentSessionHistoryIndex::open(runtime_cache_dir).await?;
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
    let candidates = index.search_scoped(query, &scan.records).await?;
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
    *guard = HistoryPreviewCache::default();
    let path = PathBuf::from(runtime_cache_dir).join("agent-session-search.sqlite");
    if !path.exists() {
        return Ok(());
    }
    AgentSessionHistoryIndex::open(runtime_cache_dir)
        .await?
        .clear()
        .await
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
        assert_eq!(first.parsed_files, 2);
        assert_eq!(first.reused_files, 0);
        assert_eq!(first.hits[0].session.workspace_id, "ws");
        let second =
            search_history_on_demand(cache.path(), &state, &roots, &registered, &query, true)
                .await
                .unwrap();
        assert_eq!(second.hits.len(), 1);
        assert_eq!(second.writes.unchanged, 1);
        assert_eq!(
            second.parsed_files, 1,
            "unregistered history is never retained"
        );
        assert_eq!(second.reused_files, 1);
        clear_history_search_cache(cache.path(), &state)
            .await
            .unwrap();
        let third =
            search_history_on_demand(cache.path(), &state, &roots, &registered, &query, true)
                .await
                .unwrap();
        assert_eq!(third.reused_files, 0);
        assert_eq!(third.parsed_files, 2);
        assert_eq!(third.writes.added, 1);
        assert!(
            file.exists(),
            "deleting FTS cache must not change native provider JSONL"
        );
    }
}
