use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use serde_json::Value;
use uuid::Uuid;

/// A Codex Hook's `session_id` is the resumable thread, while other UUIDs
/// (turn IDs, conversation IDs and the Alera terminal handle) are not.
/// Codex supplies an exact rollout path; use it as proof, never search for the
/// latest file under ~/.codex or guess a session from timestamps.
pub(super) struct VerifiedCodexSession<'a> {
    pub id: &'a str,
    pub transcript_path: &'a str,
}

pub(super) fn from_hook(payload: &Value) -> Option<VerifiedCodexSession<'_>> {
    let id = payload.get("session_id")?.as_str()?;
    let transcript_path = payload.get("transcript_path")?.as_str()?;
    let cwd = payload.get("cwd").and_then(Value::as_str);
    verified_transcript(id, transcript_path, cwd).then_some(VerifiedCodexSession {
        id,
        transcript_path,
    })
}

/// Verify only the explicitly named rollout's first metadata record. A
/// bounded single-file read avoids scanning session history on every startup.
pub(super) fn verified_transcript(
    session_id: &str,
    transcript_path: &str,
    expected_cwd: Option<&str>,
) -> bool {
    if session_id.len() != 36
        || Uuid::parse_str(session_id).is_err()
        || transcript_path.is_empty()
        || transcript_path.len() > 4096
    {
        return false;
    }
    let path = Path::new(transcript_path);
    if !path.is_absolute()
        || !path.components().any(|part| part.as_os_str() == "sessions")
        || !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with("rollout-") && name.ends_with(&format!("-{session_id}.jsonl"))
            })
    {
        return false;
    }
    let Ok(file) = File::open(path) else {
        return false;
    };
    const MAX_METADATA_BYTES: u64 = 256 * 1024;
    let mut first_line = String::new();
    if BufReader::new(file.take(MAX_METADATA_BYTES))
        .read_line(&mut first_line)
        .is_err()
        || !first_line.ends_with('\n')
    {
        return false;
    }
    let Ok(record) = serde_json::from_str::<Value>(&first_line) else {
        return false;
    };
    if record["type"] != "session_meta"
        || record.pointer("/payload/id").and_then(Value::as_str) != Some(session_id)
        || record
            .pointer("/payload/session_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id != session_id)
    {
        return false;
    }
    match expected_cwd {
        None => true,
        Some(cwd) => record
            .pointer("/payload/cwd")
            .and_then(Value::as_str)
            .is_some_and(|stored| same_working_directory(stored, cwd)),
    }
}

/// Codex 0.171 on Windows emits a normal path on startup, then the
/// extended-length \\?\ path on resume. Both point to the same rollout.
/// Never equate two different physical sources solely by UUID or basename.
pub(super) fn same_rollout_source(expected: &str, incoming: &str) -> bool {
    if expected == incoming {
        return true;
    }
    match (
        std::fs::canonicalize(expected),
        std::fs::canonicalize(incoming),
    ) {
        (Ok(expected), Ok(incoming)) => expected == incoming,
        _ => false,
    }
}

fn same_working_directory(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.replace('/', "\\")
            .trim_end_matches('\\')
            .eq_ignore_ascii_case(b.replace('/', "\\").trim_end_matches('\\'))
    } else {
        a.trim_end_matches('/') == b.trim_end_matches('/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_hook_uses_exact_rollout_session_and_rejects_unrelated_uuids() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions/2026/11/21");
        std::fs::create_dir_all(&sessions).unwrap();
        let native = "550e8400-e29b-41d4-a716-446655440077";
        let other = "550e8400-e29b-41d4-a716-446655440088";
        let path = sessions.join(format!("rollout-2026-11-21T10-00-00-{native}.jsonl"));
        std::fs::write(
            &path,
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":native,"session_id":native,"cwd":dir.path()}})
            ),
        )
        .unwrap();
        let path = path.to_str().unwrap();
        let canonical = std::fs::canonicalize(path).unwrap();
        assert!(same_rollout_source(path, canonical.to_str().unwrap()));
        let payload = json!({
            "session_id": native,
            "transcript_path": path,
            "conversation_id": other,
            "thread_id": other,
            "turn_id": other,
            "cwd": dir.path()
        });
        assert_eq!(from_hook(&payload).unwrap().id, native);
        assert!(verified_transcript(native, path, None));
        assert!(from_hook(&json!({"session_id":other,"transcript_path":path})).is_none());
        assert!(from_hook(&json!({"session_id":native})).is_none());
        assert!(from_hook(&json!({"thread_id":native,"transcript_path":path})).is_none());
        assert!(
            from_hook(&json!({"session_id":native,"transcript_path":path,"cwd":"unrelated"}))
                .is_none()
        );
        assert!(!verified_transcript(other, path, None));
        assert!(!verified_transcript(native, "relative/rollout.jsonl", None));
        let mismatch = sessions.join(format!("rollout-2026-11-21T10-00-00-{other}.jsonl"));
        std::fs::write(
            &mismatch,
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":native}})
            ),
        )
        .unwrap();
        assert!(!verified_transcript(
            other,
            mismatch.to_str().unwrap(),
            None
        ));
        assert!(!same_rollout_source(path, mismatch.to_str().unwrap()));
        std::fs::remove_file(path).unwrap();
        assert!(!verified_transcript(native, path, None));
    }
}
