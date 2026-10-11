use super::*;
use std::collections::BTreeMap;
use std::net::TcpListener;

fn environment() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("ALERA_AGENT_TYPE".into(), "codex".into()),
        ("ALERA_AGENT_HOOK_PORT".into(), "12345".into()),
        ("ALERA_AGENT_HOOK_TOKEN".into(), "test-token".into()),
        ("ALERA_TERMINAL_SESSION_ID".into(), "terminal".into()),
        ("ALERA_WORKSPACE_ID".into(), "workspace".into()),
        ("ALERA_TAB_ID".into(), "tab".into()),
    ])
}

fn request(environment: &BTreeMap<String, String>) -> Option<HookRequest> {
    HookRequest::from_environment(|key| environment.get(key).cloned())
}

#[test]
fn hook_relay_preserves_unicode_nested_payload_and_identity_for_every_http_agent() {
    for agent in [
        "codex",
        "claude",
        "devin",
        "cursor",
        "copilot",
        "grok",
        "agy",
        "pi",
        "amp",
        "opencode",
        "opencode2",
    ] {
        let mut env = environment();
        env.insert("ALERA_AGENT_TYPE".into(), agent.into());
        env.insert("ALERA_AGENT_HOOK_EVENT".into(), "PreToolUse".into());
        env.insert("ALERA_AGENT_HOOK_VERSION".into(), "2".into());
        let payload = json!({"message": "測試 🦀", "nested": [{"null": null, "bool": true, "number": 123.25}]});
        let body = request(&env)
            .unwrap()
            .body(&serde_json::to_vec(&payload).unwrap())
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body,
            json!({"terminalSessionId": "terminal", "workspaceId": "workspace", "tabId": "tab", "hookEventName": "PreToolUse", "version": "2", "payload": payload})
        );
    }
}

#[test]
fn hook_relay_handles_empty_bom_and_invalid_input_without_inventing_events() {
    let request = request(&environment()).unwrap();
    for input in [b"".as_slice(), b" \r\n\t", b"\xef\xbb\xbf{}"] {
        let body: Value = serde_json::from_slice(&request.body(input).unwrap()).unwrap();
        assert_eq!(body["payload"], json!({}));
        assert_eq!(body["hookEventName"], Value::Null);
        assert_eq!(body["version"], Value::Null);
    }
    assert!(request.body(b"{broken").is_none());
    assert!(request.body(b"\xff").is_none());
}

#[test]
fn hook_relay_borrowed_envelope_matches_previous_wire_bytes() {
    let mut env = environment();
    env.insert("ALERA_AGENT_HOOK_EVENT".into(), "Tool\"\\\n測試".into());
    env.insert("ALERA_AGENT_HOOK_VERSION".into(), "2".into());
    let request = request(&env).unwrap();
    for input in [
        "null",
        "true",
        "123",
        "-0.0",
        "1.25e30",
        "18446744073709551615",
        "\"測試 🦀\\n\\u0000\"",
        "[1, null, {\"b\": false, \"a\": [\"x\"]}]",
        "{\"duplicate\": 1, \"duplicate\": 2}",
        "  {\"nested\": [{\"text\": \"x\"}]} \r\n",
    ] {
        let payload: Value = serde_json::from_str(input).unwrap();
        let previous = serde_json::to_vec(&json!({
            "terminalSessionId": request.terminal,
            "workspaceId": request.workspace,
            "tabId": request.tab,
            "hookEventName": request.event,
            "version": request.version,
            "payload": payload,
        }))
        .unwrap();
        assert_eq!(request.body(input.as_bytes()).unwrap(), previous, "{input}");
    }
    for input in [b"{} trailing".as_slice(), b"{} {}", b"[1,]", b"\"\\uD800\""] {
        assert!(request.body(input).is_none());
    }
}

#[test]
fn hook_relay_requires_routing_and_rejects_invalid_port_or_headers() {
    for key in environment().keys() {
        let mut env = environment();
        env.remove(key);
        assert!(request(&env).is_none(), "missing {key}");
        env.insert(key.clone(), String::new());
        assert!(request(&env).is_none(), "empty {key}");
    }
    for (key, value) in [
        ("ALERA_AGENT_HOOK_PORT", "0"),
        ("ALERA_AGENT_HOOK_PORT", "65536"),
        ("ALERA_AGENT_TYPE", "codex/../claude"),
        ("ALERA_AGENT_HOOK_TOKEN", "x\r\nHeader: y"),
    ] {
        let mut env = environment();
        env.insert(key.into(), value.into());
        assert!(request(&env).is_none());
    }
}

#[test]
fn hook_relay_retries_completion_only_with_windows_case_semantics() {
    for event in COMPLETION_EVENTS {
        let mut env = environment();
        env.insert("ALERA_AGENT_HOOK_EVENT".into(), event.to_ascii_uppercase());
        assert_eq!(request(&env).unwrap().attempts(), 3);
    }
    assert_eq!(request(&environment()).unwrap().attempts(), 1);
}

#[test]
fn hook_relay_posts_exact_http_body_and_retries_failed_completion() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut env = environment();
    env.insert(
        "ALERA_AGENT_HOOK_PORT".into(),
        listener.local_addr().unwrap().port().to_string(),
    );
    env.insert("ALERA_AGENT_HOOK_EVENT".into(), "Stop".into());
    let request = request(&env).unwrap();
    let body = request.body(br#"{"done":true}"#).unwrap();
    let expected_body = body.clone();
    let server = std::thread::spawn(move || {
        for code in [503, 503, 204] {
            let (stream, peer) = listener.accept().unwrap();
            assert!(peer.ip().is_loopback());
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                headers.push_str(&line);
            }
            assert!(headers.starts_with("POST /hook/codex HTTP/1.1\r\n"));
            assert!(headers.contains("X-Alera-Agent-Hook-Token: test-token\r\n"));
            assert!(headers.contains("Content-Type: application/json; charset=utf-8\r\n"));
            let mut received = vec![0; expected_body.len()];
            reader.read_exact(&mut received).unwrap();
            assert_eq!(received, expected_body);
            write!(
                reader.get_mut(),
                "HTTP/1.1 {code} Result\r\nContent-Length: 0\r\n\r\n"
            )
            .unwrap();
        }
    });
    request.deliver(&body);
    server.join().unwrap();
}

#[test]
fn hook_relay_response_wait_has_a_deadline() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut env = environment();
    env.insert(
        "ALERA_AGENT_HOOK_PORT".into(),
        listener.local_addr().unwrap().port().to_string(),
    );
    let request = request(&env).unwrap();
    let server = std::thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        std::thread::sleep(Duration::from_millis(1500));
    });
    let started = Instant::now();
    assert!(request.post(b"{}").is_err());
    assert!(started.elapsed() < Duration::from_millis(1400));
    server.join().unwrap();
}
