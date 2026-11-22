//! Bounded read-only restored Claude/Codex conversation reader.
//! The Host chooses a verified provider file; never merge this text into PTY.
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const PAGE_BYTES: u64 = 1024 * 1024;
const MAX_LINE_BYTES: usize = 128 * 1024;
const MAX_TEXT_CHARS: usize = 8000;
pub(super) const MAX_MESSAGES: usize = 60;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Message {
    pub role: &'static str,
    pub text: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Page {
    pub messages: Vec<Message>,
    pub has_more: bool,
    pub next_before: Option<u64>,
    pub source: &'static str,
    pub read_only: bool,
}

fn visible_text(content: &Value) -> Option<String> {
    let text = if let Some(s) = content.as_str() {
        s.to_owned()
    } else {
        let texts: Vec<_> = content
            .as_array()?
            .iter()
            .filter(|v| matches!(v["type"].as_str(), Some("text" | "output_text")))
            .filter_map(|v| v["text"].as_str())
            .collect();
        if texts.is_empty() {
            return None;
        }
        texts.join("\n")
    };
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.chars().take(MAX_TEXT_CHARS).collect())
    }
}

/// Never include tool inputs/results, hidden thinking, reasoning or sidechains.
pub(super) fn parse_message(provider: &str, line: &str, native: &str) -> Option<Message> {
    if line.len() > MAX_LINE_BYTES {
        return None;
    }
    let event: Value = serde_json::from_str(line).ok()?;
    let (role, text) = match provider {
        "claude" => {
            if event["sessionId"] != native
                || event["isSidechain"] == true
                || event["isMeta"] == true
            {
                return None;
            }
            let role = match (event["type"].as_str()?, event["message"]["role"].as_str()?) {
                ("user", "user") => "user",
                ("assistant", "assistant") => "assistant",
                _ => return None,
            };
            (role, visible_text(&event["message"]["content"])?)
        }
        "codex" => {
            if event["type"] != "event_msg" {
                return None;
            }
            match event.pointer("/payload/type")?.as_str()? {
                "user_message" => ("user", visible_text(event.pointer("/payload/message")?)?),
                "agent_message" => (
                    "assistant",
                    visible_text(event.pointer("/payload/message")?)?,
                ),
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(Message {
        role,
        text,
        timestamp: event["timestamp"].as_str().map(str::to_owned),
    })
}

/// Paginate backward using file byte offsets, at most 1 MiB per user click.
/// The caller cannot pass file paths and never learns the provider config root.
pub(super) fn read_page(
    path: &Path,
    provider: &str,
    native: &str,
    before: Option<u64>,
    limit: usize,
) -> std::io::Result<Page> {
    if !matches!(provider, "claude" | "codex") || !(1..=MAX_MESSAGES).contains(&limit) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid conversation limit",
        ));
    }
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let end = before.unwrap_or(file_len);
    if end > file_len {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Stale history cursor",
        ));
    }
    if end == 0 {
        return Ok(Page {
            messages: vec![],
            has_more: false,
            next_before: None,
            source: "nativeProvider",
            read_only: true,
        });
    }
    let start = end.saturating_sub(PAGE_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0; (end - start) as usize];
    file.read_exact(&mut bytes)?;
    let skipped = if start > 0 {
        bytes
            .iter()
            .position(|v| *v == b'\n')
            .map_or(bytes.len(), |i| i + 1)
    } else {
        0
    };
    let mut events = Vec::new();
    let mut pos = skipped;
    while pos < bytes.len() {
        let line_end = bytes[pos..]
            .iter()
            .position(|v| *v == b'\n')
            .map_or(bytes.len(), |offset| pos + offset + 1);
        if line_end - pos <= MAX_LINE_BYTES {
            if let Ok(line) = std::str::from_utf8(&bytes[pos..line_end]) {
                if let Some(message) = parse_message(provider, line.trim(), native) {
                    events.push((start + pos as u64, message));
                }
            }
        }
        pos = line_end;
    }
    let first = events.len().saturating_sub(limit);
    let next = if first > 0 {
        Some(events[first].0)
    } else if start > 0 {
        // A single line can exceed PAGE_BYTES (e.g. giant tool payload).
        // Make cursor progress even when there was no newline in this slice.
        Some(if skipped == bytes.len() {
            start
        } else {
            start + skipped as u64
        })
    } else {
        None
    };
    Ok(Page {
        messages: events
            .into_iter()
            .skip(first)
            .map(|(_, message)| message)
            .collect(),
        has_more: next.is_some_and(|offset| offset > 0),
        next_before: next.filter(|offset| *offset > 0),
        source: "nativeProvider",
        read_only: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_rejects_tool_sidechain_and_foreign_session() {
        let id = "550e8400-e29b-41d4-a716-446655440001";
        let assistant = json!({"type":"assistant","sessionId":id,
            "message":{"role":"assistant","content":[
                {"type":"text","text":"assistant reply"},
                {"type":"tool_use","input":{"command":"PRIVATE"}}]}});
        assert_eq!(
            parse_message("claude", &assistant.to_string(), id)
                .unwrap()
                .text,
            "assistant reply"
        );
        for bad in [
            json!({"type":"user","sessionId":id,"message":{"role":"user","content":[{"type":"tool_result","content":"SECRET"}]}}),
            json!({"type":"user","sessionId":id,"isSidechain":true,"message":{"role":"user","content":"HIDDEN"}}),
            json!({"type":"assistant","sessionId":"foreign","message":{"role":"assistant","content":"WRONG"}}),
        ] {
            assert!(parse_message("claude", &bad.to_string(), id).is_none());
        }
    }

    #[test]
    fn codex_only_user_and_agent_messages() {
        let id = "550e8400-e29b-41d4-a716-446655440001";
        let good =
            json!({"type":"event_msg","payload":{"type":"agent_message","message":"response"}});
        assert_eq!(
            parse_message("codex", &good.to_string(), id).unwrap().role,
            "assistant"
        );
        for typ in ["reasoning", "tool_result", "exec_command_begin"] {
            assert!(parse_message(
                "codex",
                &json!({"type":"event_msg",
                "payload":{"type":typ,"message":"INTERNAL"}})
                .to_string(),
                id
            )
            .is_none());
        }
        assert!(parse_message(
            "codex",
            &json!({"type":"response_item",
            "payload":{"type":"message","content":"duplicate"}})
            .to_string(),
            id
        )
        .is_none());
    }

    #[test]
    fn giant_tool_line_cannot_trap_reverse_page_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("verified.jsonl");
        let first =
            json!({"type":"event_msg","payload":{"type":"user_message","message":"earliest"}})
                .to_string();
        let huge = "x".repeat(PAGE_BYTES as usize + 128);
        std::fs::write(&path, format!("{first}\n{huge}\n")).unwrap();
        let page = read_page(&path, "codex", "native", None, 30).unwrap();
        assert!(page.messages.is_empty());
        let next = page.next_before.unwrap();
        assert!(next < std::fs::metadata(&path).unwrap().len());
        let older = read_page(&path, "codex", "native", Some(next), 30).unwrap();
        assert!(
            older
                .messages
                .iter()
                .any(|message| message.text == "earliest")
                || older.has_more
        );
    }

    #[test]
    fn pages_read_backward_without_duplicates_or_tool_leakage() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("verified.jsonl");
        let mut lines = Vec::new();
        for n in 0..13 {
            lines.push(json!({"type":"event_msg","payload":{"type":"user_message","message":format!("q{n}")}}).to_string());
            lines.push(
                json!({"type":"event_msg","payload":{"type":"reasoning","message":"INTERNAL"}})
                    .to_string(),
            );
        }
        std::fs::write(&f, format!("{}\n", lines.join("\n"))).unwrap();
        let mut next = None;
        let mut seen = Vec::new();
        loop {
            let page = read_page(&f, "codex", "native", next, 5).unwrap();
            // The UI prepends each older page above newer history.
            seen.splice(0..0, page.messages.iter().map(|m| m.text.clone()));
            if !page.has_more {
                break;
            }
            let cursor = page.next_before.unwrap();
            assert!(next.is_none_or(|previous| cursor < previous));
            next = Some(cursor);
        }
        assert_eq!(seen.len(), 13);
        assert_eq!(seen.first().map(String::as_str), Some("q0"));
        assert_eq!(seen.last().map(String::as_str), Some("q12"));
        assert_eq!(
            seen.len(),
            (0..13).collect::<std::collections::HashSet<_>>().len()
        );
        assert!(read_page(&f, "codex", "native", Some(u64::MAX), 5).is_err());
        assert!(read_page(&f, "codex", "native", None, 61).is_err());
    }
}
