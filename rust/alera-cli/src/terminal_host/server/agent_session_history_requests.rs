//! Local-only, consent-gated history search. This handler owns no PTYs and
//! accepts no file paths or account roots from the RPC caller.
use std::path::PathBuf;

use alera_core::global_agent_search::AgentSessionSearchQuery;
use alera_core::runtime::RuntimeStore;
use serde_json::{json, Value};

use crate::agent_history_sync::search_roots;
use crate::agent_session_history_scan::RegisteredHistoryWorkspace;
use crate::agent_session_history_service::{
    clear_history_search_cache, search_history_on_demand, HistorySearchState,
};
use crate::terminal_host::host_error::{HostError, HostResult};

pub(super) struct AgentSessionHistoryRequestHandler {
    store: RuntimeStore,
    private_dir: PathBuf,
    state: HistorySearchState,
}

impl AgentSessionHistoryRequestHandler {
    pub(super) fn new(
        store: RuntimeStore,
        runtime_dir: PathBuf,
        state: HistorySearchState,
    ) -> Self {
        Self {
            store,
            private_dir: runtime_dir.join("agent-session-search"),
            state,
        }
    }

    pub(super) async fn search(&self, payload: &Value) -> HostResult<Value> {
        let query = parse_history_search_query(payload)?;
        let workspaces = self
            .store
            .list_all_workspaces()
            .await
            .map_err(|err| HostError::state(err.to_string()))?;
        let registered: Vec<_> = workspaces
            .into_iter()
            .filter(|workspace| {
                !workspace.instance_id.is_empty()
                    && !workspace.project_id.is_empty()
                    && !workspace.id.is_empty()
                    && query
                        .project_id
                        .as_ref()
                        .is_none_or(|id| id == &workspace.project_id)
                    && query
                        .workspace_id
                        .as_ref()
                        .is_none_or(|id| id == &workspace.id)
            })
            .map(|workspace| RegisteredHistoryWorkspace {
                runtime_instance_id: workspace.instance_id,
                project_id: workspace.project_id,
                workspace_id: workspace.id,
                path: workspace.path.into(),
            })
            .collect();
        let settings = self
            .store
            .agent_quota_settings()
            .await
            .map_err(|err| HostError::state(err.to_string()))?;
        let roots = search_roots(&settings)
            .await
            .into_iter()
            .filter(|root| {
                query
                    .provider
                    .as_deref()
                    .is_none_or(|provider| root.provider == provider)
                    && query
                        .provider_account_id
                        .as_deref()
                        .is_none_or(|account| root.account_id == account)
            })
            .collect::<Vec<_>>();
        let result = search_history_on_demand(
            &self.private_dir,
            &self.state,
            &roots,
            &registered,
            &query,
            true,
        )
        .await
        .map_err(|err| HostError::state(err.to_string()))?;
        Ok(json!({
            "hits": result.hits,
            "visitedFiles": result.visited_files,
            "parsedFiles": result.parsed_files,
            "reusedFiles": result.reused_files,
            "scanTruncated": result.scan_truncated,
            "writes": {
                "added": result.writes.added,
                "updated": result.writes.updated,
                "unchanged": result.writes.unchanged,
                "trimmed": result.writes.trimmed,
            },
            "openPolicy": "preview_only"
        }))
    }

    pub(super) async fn clear(&self) -> HostResult<Value> {
        clear_history_search_cache(&self.private_dir, &self.state)
            .await
            .map_err(|err| HostError::state(err.to_string()))?;
        Ok(json!({"cleared": true, "providerFilesPreserved": true}))
    }
}

fn optional_scope(payload: &Value, key: &str, max: usize) -> HostResult<Option<String>> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value))
            if !value.trim().is_empty()
                && value.len() <= max
                && !value.chars().any(char::is_control) =>
        {
            Ok(Some(value.clone()))
        }
        _ => Err(HostError::format(format!(
            "Invalid Agent history {key} filter"
        ))),
    }
}

fn parse_history_search_query(payload: &Value) -> HostResult<AgentSessionSearchQuery> {
    if payload.get("consentToIndexUserText") != Some(&Value::Bool(true)) {
        return Err(HostError::state(
            "Explicit local consent to index bounded user messages is required",
        ));
    }
    let text = payload
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| HostError::format("Search text is required"))?;
    if text.trim().is_empty() || text.chars().count() > 256 || text.contains('\0') {
        return Err(HostError::format("Search query must be 1–256 characters"));
    }
    let provider = optional_scope(payload, "provider", 32)?;
    if provider
        .as_deref()
        .is_some_and(|provider| !matches!(provider, "claude" | "codex"))
    {
        return Err(HostError::format("Unsupported history provider filter"));
    }
    let max_results = match payload.get("maxResults") {
        None | Some(Value::Null) => 30,
        Some(Value::Number(number)) => number
            .as_u64()
            .filter(|count| (1..=100).contains(count))
            .ok_or_else(|| HostError::format("maxResults must be 1–100"))?
            as usize,
        _ => return Err(HostError::format("Invalid maxResults")),
    };
    Ok(AgentSessionSearchQuery {
        text: text.to_owned(),
        project_id: optional_scope(payload, "projectId", 256)?,
        workspace_id: optional_scope(payload, "workspaceId", 256)?,
        provider,
        provider_account_id: optional_scope(payload, "accountId", 256)?,
        max_results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_rpc_requires_opt_in_and_rejects_bad_filters() {
        let base = json!({"text": "Windows 注音", "maxResults": 30});
        assert!(parse_history_search_query(&base).is_err());
        let mut base = base;
        base["consentToIndexUserText"] = json!(true);
        assert_eq!(
            parse_history_search_query(&base).unwrap().text,
            "Windows 注音"
        );
        base["provider"] = json!("unsupported");
        assert!(parse_history_search_query(&base).is_err());
        base["provider"] = json!("claude");
        base["accountId"] = json!("secondary");
        assert_eq!(
            parse_history_search_query(&base)
                .unwrap()
                .provider_account_id
                .as_deref(),
            Some("secondary")
        );
        base["maxResults"] = json!(1000);
        assert!(parse_history_search_query(&base).is_err());
    }

    #[tokio::test]
    async fn search_without_consent_does_not_scan_or_create_cache() {
        let temp = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(temp.path()).await.unwrap();
        let handler = AgentSessionHistoryRequestHandler::new(
            store,
            temp.path().to_path_buf(),
            Default::default(),
        );
        let result = handler.search(&json!({"text":"private history"})).await;
        assert!(result.is_err());
        assert!(!temp.path().join("agent-session-search").exists());
    }

    #[test]
    fn history_rpcs_are_local_only_and_use_deferred_scheduler() {
        use super::super::request_route_policy::{request_route_policy, DeferredReadRoute};
        let search = request_route_policy("agentHistory.search");
        assert!(!search.mobile_allowed);
        assert_eq!(
            search.deferred_read,
            Some(DeferredReadRoute::AgentHistorySearch)
        );
        let clear = request_route_policy("agentHistory.clear");
        assert!(!clear.mobile_allowed);
        assert_eq!(
            clear.deferred_read,
            Some(DeferredReadRoute::AgentHistoryClear)
        );
    }
}
