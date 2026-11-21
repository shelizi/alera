//! Opt-in, bounded discovery of native Claude/Codex history (read-only).
//!
//! A provider transcript is evidence of a conversation, NOT authority to
//! resume it. All results are PreviewOnly until the Host separately verifies
//! provider account, registered checkout and session identity.
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use alera_core::global_agent_search::AgentSessionSearchRecord;
use serde_json::Value;
use uuid::Uuid;

const MAX_FILES: usize = 256;
const MAX_ENTRIES: usize = 4096;
const MAX_DEPTH: usize = 6;
const MAX_READ_BYTES: u64 = 256 * 1024;
const MAX_LINES: usize = 128;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PREVIEW_CHARS: usize = 240;

#[derive(Clone, Debug)]
pub(crate) struct ProviderHistoryRoot {
    pub provider: &'static str,
    pub account_id: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct RegisteredHistoryWorkspace {
    pub runtime_instance_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HistoryScanPolicy {
    /// False by default. Explicit opt-in is required to read human messages
    /// into the transient search projection; provider files are never edited.
    pub index_user_text: bool,
    pub max_files: usize,
}

impl Default for HistoryScanPolicy {
    fn default() -> Self {
        Self {
            index_user_text: false,
            max_files: 64,
        }
    }
}

#[derive(Default, Debug)]
pub(crate) struct HistoryScanReport {
    pub records: Vec<AgentSessionSearchRecord>,
    pub visited_files: usize,
    pub skipped_files: usize,
    pub truncated: bool,
}

#[derive(Clone)]
struct CanonicalWorkspace {
    runtime: String,
    project: String,
    workspace: String,
    path: PathBuf,
}

/// Scan only caller-provided provider roots, strictly match a canonical CWD
/// with a registered Alera Workspace, and preserve account provenance.
pub(crate) fn discover_history(
    roots: &[ProviderHistoryRoot],
    workspaces: &[RegisteredHistoryWorkspace],
    policy: HistoryScanPolicy,
) -> HistoryScanReport {
    let mut report = HistoryScanReport::default();
    let cap = policy.max_files.min(MAX_FILES);
    if cap == 0 {
        return report;
    }
    let known: Vec<_> = workspaces
        .iter()
        .take(512)
        .filter_map(|w| {
            if w.project_id.is_empty() || w.workspace_id.is_empty() {
                return None;
            }
            let path = fs::canonicalize(&w.path).ok()?;
            path.is_dir().then_some(CanonicalWorkspace {
                runtime: w.runtime_instance_id.clone(),
                project: w.project_id.clone(),
                workspace: w.workspace_id.clone(),
                path,
            })
        })
        .collect();
    let mut seen_files = HashSet::new();
    let mut walked = 0_usize;
    for root in roots.iter().take(24) {
        if !matches!(root.provider, "codex" | "claude") || root.account_id.is_empty() {
            continue;
        }
        let Ok(canonical_root) = fs::canonicalize(&root.path) else {
            continue;
        };
        if !canonical_root.is_dir() {
            continue;
        }
        let mut pending = vec![(canonical_root.clone(), 0_usize)];
        while let Some((directory, depth)) = pending.pop() {
            if walked >= MAX_ENTRIES || report.visited_files >= cap {
                report.truncated = true;
                return report;
            }
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries.into_iter().rev() {
                walked += 1;
                if walked >= MAX_ENTRIES {
                    report.truncated = true;
                    return report;
                }
                let path = entry.path();
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                // Reject symlinks at every level to prevent out-of-root reads.
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir() {
                    if depth < MAX_DEPTH
                        && !matches!(
                            entry.file_name().to_str(),
                            Some("subagents" | ".git" | "node_modules")
                        )
                    {
                        pending.push((path, depth + 1));
                    }
                    continue;
                }
                if !meta.is_file() || !is_provider_filename(root.provider, &path) {
                    continue;
                }
                if report.visited_files >= cap {
                    report.truncated = true;
                    return report;
                }
                let Ok(canonical_file) = fs::canonicalize(&path) else {
                    continue;
                };
                if !canonical_file.starts_with(&canonical_root)
                    || !seen_files.insert((
                        root.provider,
                        root.account_id.clone(),
                        canonical_file.clone(),
                    ))
                {
                    continue;
                }
                report.visited_files += 1;
                if meta.len() > MAX_FILE_BYTES {
                    report.skipped_files += 1;
                    continue;
                }
                let Some((native_id, cwd, preview)) =
                    read_provider_metadata(root.provider, &canonical_file, policy.index_user_text)
                else {
                    report.skipped_files += 1;
                    continue;
                };
                let Ok(verified_cwd) = fs::canonicalize(cwd) else {
                    continue;
                };
                let Some(workspace) = known.iter().find(|w| w.path == verified_cwd) else {
                    // Never attribute a transcript from an unregistered folder.
                    continue;
                };
                let updated_at_ms = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
                    .unwrap_or(0);
                let display = format!("{} {}", root.provider, &native_id[..8]);
                let title = preview
                    .as_deref()
                    .map(|p| p.chars().take(96).collect())
                    .unwrap_or(display);
                report.records.push(AgentSessionSearchRecord {
                    runtime_instance_id: workspace.runtime.clone(),
                    project_id: workspace.project.clone(),
                    workspace_id: workspace.workspace.clone(),
                    tab_id: String::new(),
                    host_session_id: String::new(),
                    provider: root.provider.to_owned(),
                    provider_account_id: Some(root.account_id.clone()),
                    profile_id: None,
                    provider_session_id: Some(native_id),
                    // A native file is NOT proof of the active provider account
                    // or agent binary. Never enable ResumeExact here.
                    provider_identity_verified: false,
                    title,
                    goal: preview.unwrap_or_default(),
                    progress: String::new(),
                    milestones: Vec::new(),
                    updated_at_ms,
                    running: false,
                    source_exists: true,
                });
            }
        }
    }
    report
}

fn is_provider_filename(provider: &str, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if !name.ends_with(".jsonl") {
        return false;
    }
    match provider {
        "codex" => name.starts_with("rollout-") && name.len() > "rollout-.jsonl".len(),
        "claude" => Uuid::parse_str(name.trim_end_matches(".jsonl")).is_ok(),
        _ => false,
    }
}

/// Parse the first bounded section only. Never touch tool results, assistant
/// outputs, secrets in the full transcript or platform auth files.
fn read_provider_metadata(
    provider: &str,
    path: &Path,
    index_user_text: bool,
) -> Option<(String, PathBuf, Option<String>)> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file.take(MAX_READ_BYTES));
    let mut line = String::new();
    let mut id: Option<String> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut preview: Option<String> = None;
    for line_index in 0..MAX_LINES {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        if line.len() > 64 * 1024 {
            return None;
        }
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if provider == "codex" {
            if line_index == 0 {
                if event["type"] != "session_meta" {
                    return None;
                }
                let native = event.pointer("/payload/id")?.as_str()?;
                if Uuid::parse_str(native).is_err() {
                    return None;
                }
                let filename = path.file_name()?.to_str()?;
                if !filename.ends_with(&format!("-{native}.jsonl")) {
                    return None;
                }
                id = Some(native.to_owned());
                cwd = Some(PathBuf::from(event.pointer("/payload/cwd")?.as_str()?));
            } else if index_user_text
                && preview.is_none()
                && event["type"] == "event_msg"
                && event.pointer("/payload/type").and_then(Value::as_str) == Some("user_message")
            {
                preview = safe_preview(event.pointer("/payload/message")?.as_str()?);
            }
        } else if provider == "claude" {
            if event["type"] != "user" || event["isSidechain"] == true || event["isMeta"] == true {
                continue;
            }
            let native = event["sessionId"].as_str()?;
            if Uuid::parse_str(native).is_err()
                || path.file_stem()?.to_str()? != native
                || event["message"]["role"] != "user"
            {
                continue;
            }
            let entry_cwd = PathBuf::from(event["cwd"].as_str()?);
            if id.is_none() {
                id = Some(native.to_owned());
                cwd = Some(entry_cwd);
            } else if cwd.as_ref() != Some(&entry_cwd) || id.as_deref() != Some(native) {
                return None;
            }
            if index_user_text && preview.is_none() {
                let content = &event["message"]["content"];
                let message = content.as_str().or_else(|| {
                    content
                        .as_array()?
                        .iter()
                        .find(|part| part["type"] == "text")
                        .and_then(|part| part["text"].as_str())
                });
                if let Some(text) = message {
                    preview = safe_preview(text);
                }
            }
        }
        if id.is_some() && (provider == "claude" || !index_user_text || preview.is_some()) {
            break;
        }
    }
    Some((id?, cwd?, preview))
}

/// Conservative opt-in preview: discard lines that resemble embedded secrets
/// or instructions rather than promising reversible perfect token redaction.
fn safe_preview(value: &str) -> Option<String> {
    let line = value.lines().find(|line| !line.trim().is_empty())?.trim();
    if line.starts_with('<') || line.starts_with('{') || line.starts_with('[') {
        return None;
    }
    let lower = line.to_ascii_lowercase();
    if [
        "api_key",
        "apikey",
        "password",
        "authorization",
        "bearer ",
        "secret=",
        "sk-",
        "ghp_",
        "-----begin",
        "token=",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return None;
    }
    let preview: String = line.chars().take(MAX_PREVIEW_CHARS).collect();
    if preview.len() < 3
        || preview
            .split_whitespace()
            .any(|part| part.len() >= 64 && part.chars().all(|c| c.is_ascii_alphanumeric()))
    {
        return None;
    }
    Some(preview)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alera_core::global_agent_search::{
        search_agent_sessions, AgentSessionSearchQuery, SessionOpenAction,
    };
    use serde_json::json;

    fn workspace(dir: &Path) -> RegisteredHistoryWorkspace {
        RegisteredHistoryWorkspace {
            runtime_instance_id: "runtime".into(),
            project_id: "project".into(),
            workspace_id: "worktree".into(),
            path: dir.into(),
        }
    }
    fn root(provider: &'static str, path: &Path) -> ProviderHistoryRoot {
        ProviderHistoryRoot {
            provider,
            account_id: "default".into(),
            path: path.into(),
        }
    }

    #[test]
    fn codex_discovers_only_verified_metadata_under_registered_cwd() {
        let home = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let path = home.path().join("sessions/2026/11/21");
        fs::create_dir_all(&path).unwrap();
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let good = path.join(format!("rollout-2026-11-21T10-00-00-{id}.jsonl"));
        fs::write(&good, format!("{}\n{}\n",
            json!({"type":"session_meta","payload":{"id":id,"cwd":work.path()}}),
            json!({"type":"event_msg","payload":{"type":"user_message","message":"Fix Windows 注音候選"}}),
        )).unwrap();
        let alien = path.join(format!(
            "rollout-2026-11-21T10-00-00-{}.jsonl",
            "550e8400-e29b-41d4-a716-446655440001"
        ));
        fs::write(&alien, format!("{}\n",
            json!({"type":"session_meta","payload":{"id":"550e8400-e29b-41d4-a716-446655440001","cwd":other.path()}})
        )).unwrap();
        let roots = [root("codex", &home.path().join("sessions"))];
        let workspaces = [workspace(work.path())];
        let default = discover_history(&roots, &workspaces, HistoryScanPolicy::default());
        assert_eq!(default.records.len(), 1);
        assert!(default.records[0].goal.is_empty());
        assert_eq!(
            default.records[0].open_action(),
            SessionOpenAction::PreviewOnly
        );
        let opted = discover_history(
            &roots,
            &workspaces,
            HistoryScanPolicy {
                index_user_text: true,
                max_files: 64,
            },
        );
        assert_eq!(opted.records.len(), 1);
        assert!(opted.records[0].goal.contains("Windows"));
        let hits = search_agent_sessions(
            opted.records,
            &AgentSessionSearchQuery {
                text: "注音".into(),
                max_results: 20,
                ..Default::default()
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].open_action, SessionOpenAction::PreviewOnly);
    }

    #[test]
    fn claude_rejects_sidechains_wrong_uuid_and_sensitive_prompts() {
        let home = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let dir = home.path().join("projects/project-slug");
        fs::create_dir_all(&dir).unwrap();
        let id = "550e8400-e29b-41d4-a716-446655440002";
        let path = dir.join(format!("{id}.jsonl"));
        let user = |content: &str, sidechain| {
            json!({
                "type":"user", "sessionId":id, "cwd":work.path(),
                "isSidechain":sidechain,
                "message":{"role":"user","content":content}
            })
        };
        fs::write(
            &path,
            format!(
                "{}\n{}\n{}\n",
                user("discard this subagent", true),
                user("password=very-secret", false),
                user("Fix cross-agent dashboard", false)
            ),
        )
        .unwrap();
        let roots = [root("claude", &home.path().join("projects"))];
        let matches = discover_history(
            &roots,
            &[workspace(work.path())],
            HistoryScanPolicy {
                index_user_text: true,
                max_files: 32,
            },
        );
        assert_eq!(matches.records.len(), 1);
        assert!(
            matches.records[0].goal.is_empty(),
            "sensitive first message is not indexed"
        );
        assert_eq!(
            matches.records[0].open_action(),
            SessionOpenAction::PreviewOnly
        );
    }

    #[test]
    fn strict_budgets_and_unregistered_roots_never_scan_other_projects() {
        let home = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let dir = home.path().join("sessions/2026/11/21");
        fs::create_dir_all(&dir).unwrap();
        for index in 0..6 {
            let id = format!("550e8400-e29b-41d4-a716-{index:012}");
            fs::write(
                dir.join(format!("rollout-day-{id}.jsonl")),
                format!(
                    "{}\n",
                    json!({"type":"session_meta","payload":{"id":id,"cwd":work.path()}})
                ),
            )
            .unwrap();
        }
        let roots = [root("codex", &home.path().join("sessions"))];
        assert!(discover_history(&roots, &[], HistoryScanPolicy::default())
            .records
            .is_empty());
        let no_scan = discover_history(
            &roots,
            &[workspace(work.path())],
            HistoryScanPolicy {
                max_files: 0,
                index_user_text: false,
            },
        );
        assert_eq!(no_scan.visited_files, 0);
        let bounded = discover_history(
            &roots,
            &[workspace(work.path())],
            HistoryScanPolicy {
                max_files: 2,
                index_user_text: false,
            },
        );
        assert_eq!(bounded.visited_files, 2);
        assert!(bounded.truncated);
        assert_eq!(bounded.records.len(), 2);
    }
}
