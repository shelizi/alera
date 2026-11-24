//! Fail-closed, read-only fallback for native completed-turn Agent reports.
//!
//! Some Hook versions omit the final assistant text, but include an original
//! provider Session UUID + transcript path. Only read when that exact native
//! file independently proves a configured provider root and registered Host
//! Worktree. Never follow arbitrary Hook paths or expose tool/reasoning data.
use std::path::{Path, PathBuf};

use alera_core::runtime::{RuntimeStore, WorkspaceStatus};
use uuid::Uuid;

use crate::agent_history_sync::search_roots;
use crate::agent_session_history_scan::{
    discover_exact_native_history, HistoryScanPolicy, ProviderHistoryRoot,
    RegisteredHistoryWorkspace,
};
use crate::agent_status::AgentHookEvent;

fn verified_latest_report(
    provider: &str,
    native: &str,
    claimed_path: &Path,
    roots: &[ProviderHistoryRoot],
    workspace: &RegisteredHistoryWorkspace,
) -> Option<String> {
    if !matches!(provider, "claude" | "codex")
        || Uuid::parse_str(native).ok()?.to_string() != native
        || !claimed_path.is_absolute()
    {
        return None;
    }
    let canonical = std::fs::canonicalize(claimed_path).ok()?;
    let matching: Vec<ProviderHistoryRoot> = roots
        .iter()
        .filter(|root| root.provider == provider)
        .filter(|root| {
            std::fs::canonicalize(&root.path)
                .ok()
                .is_some_and(|registered| canonical.starts_with(registered))
        })
        .cloned()
        .collect();
    if matching.is_empty() {
        return None;
    }
    let proof = discover_exact_native_history(
        &matching,
        std::slice::from_ref(workspace),
        HistoryScanPolicy {
            index_user_text: false,
            max_files: 64,
        },
        native,
    );
    if !proof.source_proofs.iter().any(|source| {
        source.provider == provider
            && source.workspace_id == workspace.workspace_id
            && source.project_id == workspace.project_id
            && source.runtime_instance_id == workspace.runtime_instance_id
            && source.native_session_id == native
            && source.canonical_file == canonical
    }) {
        return None;
    }
    let page =
        super::agent_history_transcript::read_page(&canonical, provider, native, None, 60).ok()?;
    // A newer user message without an assistant reply is not a report.
    page.messages
        .last()
        .filter(|message| message.role == "assistant")
        .map(|message| message.text.clone())
}

/// Only called when a native completed-turn Hook did not provide fresh text.
/// The Hook already passed the Host's live Terminal/Workspace/Tab check.
pub(super) async fn completed_hook_report(
    store: &RuntimeStore,
    event: &AgentHookEvent,
) -> Option<String> {
    let name = event
        .event_name
        .as_deref()
        .or_else(|| event.payload["hook_event_name"].as_str())
        .or_else(|| event.payload["hookEventName"].as_str());
    if !name.is_some_and(|name| name.eq_ignore_ascii_case("Stop"))
        || !matches!(event.agent_type.as_str(), "claude" | "codex")
    {
        return None;
    }
    let native = event.payload["session_id"]
        .as_str()
        .or_else(|| event.payload["sessionId"].as_str())?;
    if Uuid::parse_str(native).ok()?.to_string() != native {
        return None;
    }
    let path = event.payload["transcript_path"]
        .as_str()
        .or_else(|| event.payload["transcriptPath"].as_str())?;
    if path.is_empty() || path.len() > 4096 {
        return None;
    }
    let workspace = store.find_workspace(&event.workspace_id).await.ok()??;
    if workspace.status != WorkspaceStatus::Active
        || workspace.instance_id.is_empty()
        || workspace.project_id.is_empty()
    {
        return None;
    }
    let known = RegisteredHistoryWorkspace {
        runtime_instance_id: workspace.instance_id,
        project_id: workspace.project_id,
        workspace_id: workspace.id,
        path: PathBuf::from(workspace.path),
    };
    let settings = store.agent_quota_settings().await.ok()?;
    let roots = search_roots(&settings).await;
    let provider = event.agent_type.clone();
    let native = native.to_owned();
    let path = PathBuf::from(path);
    tokio::task::spawn_blocking(move || {
        verified_latest_report(&provider, &native, &path, &roots, &known)
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registered_codex_native_answer_filters_tool_output_and_foreign_paths() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("project");
        let sessions = dir.path().join("codex/sessions/2026/11/22");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&sessions).unwrap();
        let id = "550e8400-e29b-41d4-a716-446655440015";
        let file = sessions.join(format!("rollout-2026-11-22T01-00-00-{id}.jsonl"));
        let other = "550e8400-e29b-41d4-a716-446655440016";
        let lines = [
            json!({"type":"session_meta","payload":{"id":id,"cwd":workspace}}),
            json!({"type":"event_msg","payload":{"type":"user_message","message":"修正 Agent Context"}}),
            json!({"type":"event_msg","payload":{"type":"exec_command_output","message":"PRIVATE TOOL"}}),
            json!({"type":"event_msg","payload":{"type":"agent_message","message":"已完成修復與驗證"}}),
        ];
        std::fs::write(
            &file,
            lines
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
        let roots = [ProviderHistoryRoot {
            provider: "codex",
            account_id: "default".into(),
            path: dir.path().join("codex/sessions"),
        }];
        let registered = RegisteredHistoryWorkspace {
            runtime_instance_id: "local".into(),
            project_id: "project-1".into(),
            workspace_id: "workspace-1".into(),
            path: workspace.clone(),
        };
        assert_eq!(
            verified_latest_report("codex", id, &file, &roots, &registered).as_deref(),
            Some("已完成修復與驗證")
        );
        assert!(verified_latest_report("codex", other, &file, &roots, &registered).is_none());
        let foreign = RegisteredHistoryWorkspace {
            path: dir.path().join("other"),
            ..registered.clone()
        };
        assert!(verified_latest_report("codex", id, &file, &roots, &foreign).is_none());
        let foreign_roots = [ProviderHistoryRoot {
            provider: "codex",
            account_id: "unrelated".into(),
            path: dir.path().join("unknown/sessions"),
        }];
        assert!(verified_latest_report("codex", id, &file, &foreign_roots, &registered).is_none());
    }

    #[test]
    fn claude_sidechains_and_tool_results_never_become_progress() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("project");
        let root = dir.path().join(".claude/projects/-project");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let id = "550e8400-e29b-41d4-a716-446655440017";
        let file = root.join(format!("{id}.jsonl"));
        let lines = [
            json!({"type":"user","sessionId":id,"cwd":workspace,"message":{"role":"user","content":"測試"}}),
            json!({"type":"assistant","sessionId":id,"message":{"role":"assistant","content":[{"type":"text","text":"完成 3 項修復"},{"type":"tool_use","name":"Bash","input":"SECRET"}]}}),
            json!({"type":"user","sessionId":id,"message":{"role":"user","content":[{"type":"tool_result","content":"PRIVATE TOOL"}]}}),
        ];
        std::fs::write(
            &file,
            lines
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
        let roots = [ProviderHistoryRoot {
            provider: "claude",
            account_id: "default".into(),
            path: dir.path().join(".claude/projects"),
        }];
        let known = RegisteredHistoryWorkspace {
            runtime_instance_id: "local".into(),
            project_id: "p".into(),
            workspace_id: "w".into(),
            path: workspace,
        };
        assert_eq!(
            verified_latest_report("claude", id, &file, &roots, &known).as_deref(),
            Some("完成 3 項修復")
        );
        let wrong = root.join("different-session.jsonl");
        assert!(verified_latest_report("claude", id, &wrong, &roots, &known).is_none());
    }
}
