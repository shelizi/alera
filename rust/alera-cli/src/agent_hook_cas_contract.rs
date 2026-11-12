//! Pure, secret-free Alera CLI contract for Native Agent Hook settings.
//!
//! This exact source file is compiled both by the full `alera` CLI and the
//! small offline test harness under scripts/agent-hook-cas-contract-tests.
//! None of these helpers connects to or mutates a Runtime Host.
//! Actual atomic write admission is owned by Host runtimeSettings.update.

use serde_json::{json, Value};

fn valid_hook_agent_id(agent: &str) -> bool {
    !agent.is_empty()
        && agent.len() <= 64
        && agent.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
}

/// Project the full Host Runtime Settings object into a strict, bounded,
/// versioned list of per-Agent booleans. Never expose any other Host fields.
pub(crate) fn native_agent_hooks_projection(record: &Value) -> Result<Value, &'static str> {
    let revision = record
        .get("revision")
        .and_then(Value::as_i64)
        .filter(|revision| *revision >= 0 && *revision < i64::MAX)
        .ok_or("Host Hook Settings revision missing or invalid")?;
    let hooks = record
        .get("agentStatusHooks")
        .and_then(Value::as_object)
        .ok_or("Host Hook Settings payload missing")?;
    if hooks.len() > 64 {
        return Err("Too many Host Hook switches");
    }
    let mut allowed = serde_json::Map::with_capacity(hooks.len());
    for (agent, enabled) in hooks {
        if !valid_hook_agent_id(agent) {
            return Err("Invalid Host Hook agent ID");
        }
        let enabled = enabled.as_bool().ok_or("Nonboolean Host Hook switch")?;
        allowed.insert(agent.to_string(), json!(enabled));
    }
    Ok(json!({
        "kind": "agentHookSettings",
        "revision": revision,
        "agentStatusHooks": allowed
    }))
}

/// Host runtimeSettings.update performs the final revision check within
/// BEGIN IMMEDIATE SQLite transaction. This first preflight additionally
/// compares the one Provider's old value without trusting Native controls.
/// Emits only expectedRevision and the validated Hook boolean map.
pub(crate) fn native_agent_hook_cas_payload(
    raw_host: &Value,
    agent: &str,
    expected_revision: i64,
    expected_enabled: bool,
    enabled: bool,
) -> Result<Option<Value>, &'static str> {
    if !valid_hook_agent_id(agent) || !(0..i64::MAX).contains(&expected_revision) {
        return Err("Invalid Agent Hook ID or expected revision");
    }
    let sanitized = native_agent_hooks_projection(raw_host)?;
    if sanitized["revision"].as_i64() != Some(expected_revision) {
        return Err("Host Hook settings changed; refresh before retrying");
    }
    let mut switches = sanitized["agentStatusHooks"]
        .as_object()
        .cloned()
        .ok_or("Host Hook switches invalid")?;
    let old = switches
        .get(agent)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if old != expected_enabled {
        return Err("Host Agent Hook changed; refresh before retrying");
    }
    if old == enabled {
        return Ok(None);
    }
    // Host currently saturates i64 revision at MAX. Native deliberately
    // rejects MAX as an invalid readback: never write MAX-1 -> MAX.
    if expected_revision >= i64::MAX - 1 {
        return Err("Host Hook revision exhausted; update refused");
    }
    if !switches.contains_key(agent) && switches.len() >= 64 {
        return Err("Host Hook settings limit reached");
    }
    switches.insert(agent.to_owned(), json!(enabled));
    Ok(Some(json!({
        "expectedRevision": expected_revision,
        "agentStatusHooks": switches
    })))
}

/// Validate the *actual* Host reply of a successful CAS before returning any
/// data to Native. An additional Provider mutation, secret-bearing readback,
/// wrong revision or missing map must never be presented as a confirmed CAS.
pub(crate) fn native_agent_hook_cas_readback(
    raw_host: &Value,
    request: &Value,
) -> Result<Value, &'static str> {
    let expected_next = request
        .get("expectedRevision")
        .and_then(Value::as_i64)
        .and_then(|revision| revision.checked_add(1))
        .filter(|revision| *revision >= 0 && *revision < i64::MAX)
        .ok_or("Invalid Agent Hook expected next revision")?;
    let projected = native_agent_hooks_projection(raw_host)?;
    if projected["revision"].as_i64() != Some(expected_next)
        || projected["agentStatusHooks"] != request["agentStatusHooks"]
    {
        return Err("Host Hook CAS readback did not confirm the update");
    }
    Ok(projected)
}
