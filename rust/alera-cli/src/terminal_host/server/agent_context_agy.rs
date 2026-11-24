//! Exact, local Antigravity CLI work report extraction.
//! Stop reports lifecycle/identity but no final text; see
//! https://antigravity.google/docs/hooks . Only the documented transcript
//! location and a completed visible model reply qualify. Never expose
//! thinking, tool arguments/results, foreign UUIDs or another account.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use alera_core::runtime::{RuntimeStore, WorkspaceStatus};
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use super::agent_title_state::AgentTitleState;
use crate::agent_status::{agy_cli_history_roots, AgentHookEvent};

const BLOCK: usize = 16 * 1024;
const MAX_SCAN: u64 = 4 * 1024 * 1024;
const MAX_LINE: usize = 128 * 1024;
const MAX_TEXT: usize = 8000;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct AgyReport {
    pub text: String,
    pub at: Option<DateTime<Utc>>,
}
enum Step {
    Ignore,
    Boundary,
    Final(AgyReport),
}

fn parse_step(line: &[u8]) -> Step {
    let Ok(value) = serde_json::from_slice::<Value>(line) else {
        // A malformed/incompletely written tail may contain a newer event.
        // Fail closed rather than misrepresenting an earlier turn as current.
        return if line.iter().all(u8::is_ascii_whitespace) {
            Step::Ignore
        } else {
            Step::Boundary
        };
    };
    if matches!(value["source"].as_str(), Some("USER" | "USER_EXPLICIT"))
        && matches!(value["type"].as_str(), Some("USER_INPUT" | "REQUEST"))
    {
        // Even a cleared or empty user input separates turns.
        return Step::Boundary;
    }
    if value["source"] != "MODEL" || value["type"] != "PLANNER_RESPONSE" {
        return Step::Ignore;
    }
    if value["status"] != "DONE"
        || !(value["tool_calls"].is_null()
            || value["tool_calls"].as_array().is_some_and(Vec::is_empty))
    {
        return Step::Boundary;
    }
    let Some(text) = value["content"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    else {
        return Step::Boundary;
    };
    Step::Final(AgyReport {
        text: text.chars().take(MAX_TEXT).collect(),
        at: value["created_at"]
            .as_str()
            .and_then(|stamp| DateTime::parse_from_rfc3339(stamp).ok())
            .map(|at| at.with_timezone(&Utc)),
    })
}

/// Reverse block reads stop at the first final response or newer turn boundary.
/// At most 4 MiB and one bounded line are retained, even for huge logs.
fn latest_report(path: &Path) -> Option<AgyReport> {
    let mut file = File::open(path).ok()?;
    let mut end = file.metadata().ok()?.len();
    let floor = end.saturating_sub(MAX_SCAN);
    let mut block = [0u8; BLOCK];
    let mut line = Vec::new();
    while end > floor {
        let start = end.saturating_sub(BLOCK as u64).max(floor);
        let count = (end - start) as usize;
        file.seek(SeekFrom::Start(start)).ok()?;
        file.read_exact(&mut block[..count]).ok()?;
        for byte in block[..count].iter().rev().copied() {
            if byte == b'\n' {
                line.reverse();
                match parse_step(&line) {
                    Step::Final(report) => return Some(report),
                    Step::Boundary => return None,
                    Step::Ignore => line.clear(),
                }
            } else {
                if line.len() == MAX_LINE {
                    return None;
                }
                line.push(byte);
            }
        }
        end = start;
    }
    // The first line is complete only when the scan reached byte zero.
    if floor == 0 {
        line.reverse();
        if let Step::Final(report) = parse_step(&line) {
            return Some(report);
        }
    }
    None
}

fn exact_transcript(roots: &[PathBuf], native: &str, claimed: Option<&Path>) -> Option<PathBuf> {
    if Uuid::parse_str(native).ok()?.to_string() != native {
        return None;
    }
    if claimed.is_some_and(|path| !path.is_absolute()) {
        return None;
    }
    let claimed = match claimed {
        Some(path) => Some(std::fs::canonicalize(path).ok()?),
        None => None,
    };
    let mut matches = Vec::new();
    for root in roots {
        let Ok(root) = std::fs::canonicalize(root) else {
            continue;
        };
        let expected = root
            .join("brain")
            .join(native)
            .join(".system_generated")
            .join("logs")
            .join("transcript.jsonl");
        let Ok(actual) = std::fs::canonicalize(&expected) else {
            continue;
        };
        // A junction/symlink inside the root must not redirect to another
        // account or another conversation, even one under this same root.
        if actual != expected || !actual.is_file() {
            continue;
        }
        if !matches.contains(&actual) {
            matches.push(actual);
        }
    }
    // If multiple configured accounts mirror this UUID, a Hook-supplied exact
    // path disambiguates; a saved UUID without a source must fail closed.
    if let Some(claimed) = claimed {
        return matches.into_iter().find(|path| *path == claimed);
    }
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

fn completed_stop(payload: &Value) -> bool {
    (payload["fullyIdle"] == true || payload["fully_idle"] == true)
        && matches!(
            payload["terminationReason"]
                .as_str()
                .or_else(|| payload["termination_reason"].as_str()),
            Some(
                "model_stop"
                    | "TERMINATION_REASON_MODEL_STOP"
                    | "TERMINATION_REASON_NO_TOOL_CALL"
                    | "NO_TOOL_CALL"
            )
        )
        && payload["error"]
            .as_str()
            .is_none_or(|error| error.trim().is_empty())
}

fn verified_hook_report(
    roots: &[PathBuf],
    native: &str,
    path: &Path,
    workspace: &Path,
    payload: &Value,
) -> Option<AgyReport> {
    if !completed_stop(payload) {
        return None;
    }
    let workspace = std::fs::canonicalize(workspace).ok()?;
    let mounted = payload["workspacePaths"]
        .as_array()
        .or_else(|| payload["workspace_paths"].as_array())?;
    if !mounted.iter().take(32).any(|path| {
        path.as_str()
            .filter(|path| Path::new(path).is_absolute())
            .and_then(|path| std::fs::canonicalize(path).ok())
            .is_some_and(|path| path == workspace)
    }) {
        return None;
    }
    latest_report(&exact_transcript(roots, native, Some(path))?)
}

pub(super) async fn completed_agy_report(
    store: &RuntimeStore,
    event: &AgentHookEvent,
) -> Option<String> {
    let name = event
        .event_name
        .as_deref()
        .or_else(|| event.payload["hookEventName"].as_str())
        .or_else(|| event.payload["hook_event_name"].as_str())?;
    if !name.eq_ignore_ascii_case("Stop") || !completed_stop(&event.payload) {
        return None;
    }
    let native = event.payload["conversationId"]
        .as_str()
        .or_else(|| event.payload["conversation_id"].as_str())?;
    let path = event.payload["transcriptPath"]
        .as_str()
        .or_else(|| event.payload["transcript_path"].as_str())?;
    if path.is_empty() || path.len() > 4096 || Uuid::parse_str(native).ok()?.to_string() != native {
        return None;
    }
    let tab = store.find_workspace_tab(&event.tab_id).await.ok()??;
    let title = AgentTitleState::read(&tab)?;
    if tab.workspace_id != event.workspace_id
        || tab.kind != "terminal"
        || tab.payload["automationOwned"] == true
        || title.agent.as_deref() != Some("agy")
        || title.native_id.as_deref() != Some(native)
        || super::requests::terminal_session_id_from_tab(&tab).unwrap_or_else(|| tab.id.clone())
            != event.terminal_session_id
    {
        return None;
    }
    let workspace = store.find_workspace(&event.workspace_id).await.ok()??;
    if workspace.status != WorkspaceStatus::Active {
        return None;
    }
    let native = native.to_owned();
    let path = PathBuf::from(path);
    let workspace = PathBuf::from(workspace.path);
    let payload = event.payload.clone();
    tokio::task::spawn_blocking(move || {
        verified_hook_report(
            &agy_cli_history_roots(),
            &native,
            &path,
            &workspace,
            &payload,
        )
        .map(|report| report.text)
    })
    .await
    .ok()
    .flatten()
}

/// Called only after the local UI has supplied the exact registered tab
/// identity and its Host-pinned native AGY UUID. No arbitrary path or scan.
pub(super) async fn recover_saved_agy_report(native: Option<&str>) -> Option<AgyReport> {
    let native = native?.to_owned();
    if Uuid::parse_str(&native).ok()?.to_string() != native {
        return None;
    }
    tokio::task::spawn_blocking(move || {
        latest_report(&exact_transcript(&agy_cli_history_roots(), &native, None)?)
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const NATIVE: &str = "550e8400-e29b-41d4-a716-446655440001";
    fn fixture(dir: &Path, id: &str, lines: &[Value]) -> PathBuf {
        let path = dir
            .join("brain")
            .join(id)
            .join(".system_generated/logs/transcript.jsonl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            lines
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
        path
    }
    fn final_reply(text: &str) -> Value {
        json!({"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE",
               "content":text,"thinking":"PRIVATE THOUGHT","created_at":"2026-11-24T08:00:00Z"})
    }
    fn user() -> Value {
        json!({"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","content":"Fix context"})
    }

    #[test]
    fn agy_native_stop_reads_only_exact_completed_visible_reply() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("antigravity-cli");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let file = fixture(
            &root,
            NATIVE,
            &[
                user(),
                json!({"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","content":"Working","tool_calls":[{"name":"run_command","args":"SECRET TOOL"}]}),
                json!({"source":"MODEL","type":"RUN_COMMAND","status":"DONE","content":"PRIVATE RESULT"}),
                final_reply("Actual final work report"),
                json!({"source":"SYSTEM","type":"CHECKPOINT","status":"DONE"}),
            ],
        );
        let roots = [root.clone()];
        let mut payload = json!({"fullyIdle":true,"terminationReason":"TERMINATION_REASON_NO_TOOL_CALL","workspacePaths":[workspace]});
        assert_eq!(
            verified_hook_report(&roots, NATIVE, &file, &workspace, &payload)
                .unwrap()
                .text,
            "Actual final work report"
        );
        assert!(verified_hook_report(
            &roots,
            "550e8400-e29b-41d4-a716-446655440002",
            &file,
            &workspace,
            &payload
        )
        .is_none());
        payload["fullyIdle"] = json!(false);
        assert!(verified_hook_report(&roots, NATIVE, &file, &workspace, &payload).is_none());
        payload["fullyIdle"] = json!(true);
        payload["terminationReason"] = json!("error");
        assert!(verified_hook_report(&roots, NATIVE, &file, &workspace, &payload).is_none());
        payload["terminationReason"] = json!("model_stop");
        payload["workspacePaths"] = json!([dir.path()]);
        assert!(verified_hook_report(&roots, NATIVE, &file, &workspace, &payload).is_none());
        let mirrored = dir.path().join("other-account");
        fixture(&mirrored, NATIVE, &[user(), final_reply("Other account")]);
        assert!(exact_transcript(&[root, mirrored], NATIVE, None).is_none());
    }

    #[test]
    fn agy_native_stop_does_not_promote_tools_or_previous_turn() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(
            dir.path(),
            NATIVE,
            &[user(), final_reply("Previous reply"), user()],
        );
        assert!(latest_report(&path).is_none());
        let path = fixture(
            dir.path(),
            NATIVE,
            &[
                user(),
                final_reply("Previous reply"),
                json!({"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","content":"Not final","tool_calls":[{"name":"run_command"}]}),
            ],
        );
        assert!(latest_report(&path).is_none());
    }

    #[test]
    fn agy_reverse_reader_handles_unicode_across_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let text = "測試".repeat(6000);
        let path = fixture(dir.path(), NATIVE, &[user(), final_reply(&text)]);
        let report = latest_report(&path).unwrap();
        assert_eq!(report.text.chars().count(), MAX_TEXT);
        assert!(report.text.starts_with("測試"));
        assert!(report.at.is_some());
    }
}
