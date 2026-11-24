//! Bounded native transcript metadata and consented preview parsing.
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use serde_json::Value;
use uuid::Uuid;

const MAX_READ_BYTES: u64 = 256 * 1024;
const MAX_LINES: usize = 128;
const MAX_PREVIEW_CHARS: usize = 240;

/// Reject only events that provably cannot match the existing Value-based
/// predicates. The target string values are ASCII letters/underscore, so a
/// JSON string can spell them either literally or with a Unicode escape.
/// Escapes anywhere (and ambiguous or nested markers) take the full parser.
/// Do not skip Codex's first physical line, or bypass read/size validation.
fn may_contain_relevant_event(provider: &str, line_index: usize, line: &str) -> bool {
    match provider {
        "codex" if line_index != 0 => {
            (line.contains(r#""event_msg""#) && line.contains(r#""user_message""#))
                || line.contains("\\u")
        }
        "claude" => line.contains(r#""user""#) || line.contains("\\u"),
        _ => true,
    }
}

/// Parse the first bounded section only. Never touch tool results, assistant
/// outputs, secrets in the full transcript or platform auth files.
pub(super) fn read_provider_metadata(
    provider: &str,
    path: &Path,
    index_user_text: bool,
) -> Option<(String, PathBuf, Option<String>)> {
    let file = File::open(path).ok()?;
    // Exact resume and uncached verification retain the original parse path.
    read_provider_metadata_impl::<false>(provider, path, index_user_text, &file)
}

pub(super) fn read_provider_metadata_from_file(
    provider: &str,
    path: &Path,
    index_user_text: bool,
    file: &File,
) -> Option<(String, PathBuf, Option<String>)> {
    read_provider_metadata_impl::<true>(provider, path, index_user_text, file)
}

fn read_provider_metadata_impl<const PREFILTER: bool>(
    provider: &str,
    path: &Path,
    index_user_text: bool,
    source: impl Read,
) -> Option<(String, PathBuf, Option<String>)> {
    let mut reader = BufReader::new(source.take(MAX_READ_BYTES));
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
        if PREFILTER && !may_contain_relevant_event(provider, line_index, &line) {
            continue;
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
    use serde_json::json;

    const ID: &str = "550e8400-e29b-41d4-a716-446655440000";

    fn codex_header() -> String {
        json!({"type":"session_meta","payload":{"id":ID,"cwd":"workspace"}}).to_string()
    }

    fn codex_user(message: &str) -> String {
        json!({"type":"event_msg","payload":{"type":"user_message","message":message}}).to_string()
    }

    fn claude_user(message: serde_json::Value) -> String {
        json!({"type":"user","sessionId":ID,"cwd":"workspace","message":{"role":"user","content":message}}).to_string()
    }

    fn path(provider: &str) -> PathBuf {
        if provider == "codex" {
            PathBuf::from(format!("rollout-test-{ID}.jsonl"))
        } else {
            PathBuf::from(format!("{ID}.jsonl"))
        }
    }

    fn compare(
        provider: &str,
        bytes: &[u8],
        index_user_text: bool,
    ) -> Option<(String, PathBuf, Option<String>)> {
        let path = path(provider);
        let expected =
            read_provider_metadata_impl::<false>(provider, &path, index_user_text, bytes);
        let actual = read_provider_metadata_impl::<true>(provider, &path, index_user_text, bytes);
        assert_eq!(actual, expected, "{provider}, {} bytes", bytes.len());
        actual
    }

    fn transcript(lines: &[String]) -> String {
        let mut joined = lines.join("\n");
        joined.push('\n');
        joined
    }

    #[test]
    fn event_prefilter_is_conservative_for_escaped_types_and_ambiguous_json() {
        let irrelevant = [
            r#"{"type":"assistant","text":"ordinary output"}"#,
            r#"{"type":"response_item","payload":{"type":"function_call_output","output":"line\nwith \"quotes\" and C:\\temp"}}"#,
            r#"{"type":"event_msg","payload":{"type":"agent_message","message":"irrelevant"}}"#,
            r#"{"type":"event_msg","type":"other","payload":{"type":"other"}}"#,
            r#"{"type":"assistant","payload":{"role":"assistant","text":"not a user event"}}"#,
            "null",
            "not valid JSON",
        ];
        for line in irrelevant {
            assert!(!may_contain_relevant_event("codex", 1, line), "{line}");
            assert!(!may_contain_relevant_event("claude", 1, line), "{line}");
            assert!(
                may_contain_relevant_event("codex", 0, line),
                "first physical line must be parsed"
            );
        }
        for line in [
            r#"{"type":"event_m\u0073g","payload":{"type":"user_message","message":"real"}}"#,
            r#"{"type":"event_msg","payload":{"type":"user_\u006dessage","message":"real"}}"#,
            r#"{"type":"u\u0073er"}"#,
            r#"{"type":"assistant","other":"\u0061"}"#,
        ] {
            assert!(may_contain_relevant_event("codex", 1, line));
            assert!(may_contain_relevant_event("claude", 1, line));
        }
        assert!(may_contain_relevant_event(
            "codex",
            1,
            r#"{"nested":{"type":"event_msg","payload":{"type":"user_message"}}}"#
        ));
        assert!(may_contain_relevant_event(
            "claude",
            1,
            r#"{"type":"assistant","nested":{"type":"user"}}"#
        ));
    }

    #[test]
    fn filtered_provider_parser_matches_original_event_semantics() {
        let header = codex_header();
        let ordinary = codex_user("Real keyboard work");
        let escaped = ordinary
            .replace("\"event_msg\"", r#""event_m\u0073g""#)
            .replace("\"user_message\"", r#""user_\u006dessage""#)
            .replace("\"type\"", r#""t\u0079pe""#);
        let duplicate_last_valid = ordinary.replacen(
            "\"type\":\"event_msg\"",
            "\"type\":\"ignored\",\"type\":\"event_msg\"",
            1,
        );
        let duplicate_last_ignored = ordinary.replacen(
            "\"type\":\"event_msg\"",
            "\"type\":\"event_msg\",\"type\":\"ignored\"",
            1,
        );
        let malformed_candidate = format!("{ordinary} trailing invalid JSON");
        let fillers = [
            r#"{"type":"response_item","payload":{"type":"function_call_output","output":"line\nwith \"quotes\" and C:\\temp"}}"#.to_owned(),
            r#"{"type":"assistant","nested":{"type":"event_msg","payload":{"type":"user_message"}}}"#.to_owned(),
            r#"{"type":"event_msg","payload":{"type":"user_message","message":null}}"#.to_owned(),
            r#"{"type":"event_msg","payload":{"type":"user_message"}}"#.to_owned(),
            r#"{"type":"event_msg","payload":{"type":"user_message","message":123}}"#.to_owned(),
            r#"{"type":"event_msg","payload":{"type":"user_message","type":"ignored"}}"#.to_owned(),
            codex_user("password=synthetic-test-marker"),
            codex_user("<not indexed>"),
            duplicate_last_valid,
            duplicate_last_ignored,
            escaped.clone(),
            malformed_candidate,
            "null".to_owned(),
            "[1,2,3]".to_owned(),
            "broken JSON".to_owned(),
        ];
        for filler in &fillers {
            for consent in [false, true] {
                compare(
                    "codex",
                    transcript(&[header.clone(), filler.clone(), ordinary.clone()]).as_bytes(),
                    consent,
                );
                compare(
                    "codex",
                    transcript(&[filler.clone(), ordinary.clone()]).as_bytes(),
                    consent,
                );
            }
        }
        let actual = compare(
            "codex",
            transcript(&[header.clone(), escaped]).as_bytes(),
            true,
        )
        .unwrap();
        assert_eq!(actual.2.as_deref(), Some("Real keyboard work"));
        assert!(compare(
            "codex",
            transcript(&[
                header,
                r#"{"type":"event_msg","payload":{"type":"user_message"}}"#.into(),
                ordinary
            ])
            .as_bytes(),
            true
        )
        .is_none());

        let ordinary = claude_user(json!("Real Claude work"));
        let escaped = ordinary
            .replace("\"user\"", r#""u\u0073er""#)
            .replace("\"type\"", r#""t\u0079pe""#);
        let array_content = claude_user(
            json!([{"type":"tool_result","content":"ignored"},{"type":"text","text":"First text"}]),
        );
        let sidechain = ordinary.replacen(
            "\"type\":\"user\"",
            "\"isSidechain\":true,\"type\":\"user\"",
            1,
        );
        let meta = ordinary.replacen("\"type\":\"user\"", "\"isMeta\":true,\"type\":\"user\"", 1);
        let wrong_uuid = ordinary.replace(ID, "550e8400-e29b-41d4-a716-446655440001");
        let wrong_role = ordinary.replace("\"role\":\"user\"", "\"role\":\"assistant\"");
        let duplicate_last_ignored = ordinary.replacen(
            "\"type\":\"user\"",
            "\"type\":\"user\",\"type\":\"assistant\"",
            1,
        );
        let duplicate_last_valid = ordinary.replacen(
            "\"type\":\"user\"",
            "\"type\":\"assistant\",\"type\":\"user\"",
            1,
        );
        for filler in [
            sidechain,
            meta,
            wrong_uuid,
            wrong_role,
            duplicate_last_ignored,
            duplicate_last_valid,
            escaped.clone(),
            array_content.clone(),
            claude_user(json!("password=synthetic-test-marker")),
            r#"{"type":"user","message":{"role":"user","content":"missing identity"}}"#.into(),
            r#"{"type":"assistant","message":{"role":"user","content":"not a user event"}}"#.into(),
            "null".into(),
            "{malformed".into(),
        ] {
            for consent in [false, true] {
                compare(
                    "claude",
                    transcript(&[filler.clone(), ordinary.clone()]).as_bytes(),
                    consent,
                );
            }
        }
        let actual = compare("claude", transcript(&[escaped]).as_bytes(), true).unwrap();
        assert_eq!(actual.2.as_deref(), Some("Real Claude work"));
        let actual = compare("claude", transcript(&[array_content]).as_bytes(), true).unwrap();
        assert_eq!(actual.2.as_deref(), Some("First text"));
        let sensitive = compare(
            "claude",
            transcript(&[
                claude_user(json!("password=synthetic-test-marker")),
                ordinary,
            ])
            .as_bytes(),
            true,
        )
        .unwrap();
        assert_eq!(
            sensitive.2, None,
            "do not change first-user privacy semantics"
        );
    }

    #[test]
    fn event_prefilter_preserves_line_byte_utf8_and_oversized_rejection() {
        let header = codex_header();
        let user = codex_user("Within budget");
        let filler = r#"{"type":"assistant","text":"irrelevant"}"#;
        let within = format!(
            "{header}\n{}{user}\n",
            format!("{filler}\n").repeat(MAX_LINES - 2)
        );
        assert_eq!(
            compare("codex", within.as_bytes(), true)
                .unwrap()
                .2
                .as_deref(),
            Some("Within budget")
        );
        let beyond = format!(
            "{header}\n{}{user}\n",
            format!("{filler}\n").repeat(MAX_LINES - 1)
        );
        assert_eq!(compare("codex", beyond.as_bytes(), true).unwrap().2, None);

        // Cross the byte cap with individually permitted lines.
        let large_filler = json!({"type":"assistant","text":"x".repeat(60 * 1024)}).to_string();
        let beyond_bytes = format!(
            "{header}\n{}{user}\n",
            format!("{large_filler}\n").repeat(5)
        );
        assert!(beyond_bytes.len() as u64 > MAX_READ_BYTES);
        assert_eq!(
            compare("codex", beyond_bytes.as_bytes(), true).unwrap().2,
            None
        );
        let too_large = json!({"type":"assistant","text":"x".repeat(64 * 1024)}).to_string();
        assert!(compare(
            "codex",
            transcript(&[header.clone(), too_large.clone(), user.clone()]).as_bytes(),
            true
        )
        .is_none());
        assert!(compare(
            "claude",
            transcript(&[too_large, claude_user(json!("Within budget"))]).as_bytes(),
            true
        )
        .is_none());
        let mut invalid_utf8 = format!("{header}\n").into_bytes();
        invalid_utf8.extend_from_slice(&[0xff, b'\n']);
        invalid_utf8.extend_from_slice(user.as_bytes());
        assert!(compare("codex", &invalid_utf8, true).is_none());
    }

    #[test]
    #[ignore = "opt-in isolated before/after parser benchmark"]
    fn provider_metadata_prefilter_benchmark() {
        use std::hint::black_box;
        use std::time::Instant;

        for (label, filler_count, filler) in [
            ("light", 6, json!({"type":"assistant","text":"x".repeat(500)}).to_string()),
            ("tool_heavy", 40, json!({"type":"response_item","payload":{"type":"function_call_output","output":"a line of tool output\n".repeat(70),"metadata":{"duration":12,"exit":0,"tags":["a","b","c"]}}}).to_string()),
        ] {
            let codex = format!("{}\n{}{}\n", codex_header(), format!("{filler}\n").repeat(filler_count), codex_user("historybenchmark keyboard"));
            let claude = format!("{}{}\n", format!("{filler}\n").repeat(filler_count), claude_user(json!("historybenchmark keyboard")));
            let paths = [path("codex"), path("claude")];
            let fixtures = [("codex", codex.as_bytes()), ("claude", claude.as_bytes())];
            for (provider, bytes) in fixtures {
                assert!(compare(provider, bytes, true).is_some());
                assert_eq!(std::str::from_utf8(bytes).unwrap().lines().enumerate().filter(|(i, line)| !may_contain_relevant_event(provider, *i, line)).count(), filler_count);
            }
            let run = |filtered: bool| {
                let started = Instant::now();
                for _ in 0..256 {
                    for (i, (provider, bytes)) in fixtures.iter().enumerate() {
                        let value = if filtered {
                            read_provider_metadata_impl::<true>(provider, &paths[i], true, black_box(*bytes))
                        } else {
                            read_provider_metadata_impl::<false>(provider, &paths[i], true, black_box(*bytes))
                        };
                        assert!(black_box(value).is_some());
                    }
                }
                println!("PARSER_BENCH workload={label} version={} parsed_transcripts=512 avoided_full_json_parses={} elapsed_us={}", if filtered {"after"} else {"before"}, if filtered {512 * filler_count} else {0}, started.elapsed().as_micros());
            };
            for filtered in [false, true, true, false, false, true] {
                run(filtered);
            }
        }
    }
}
