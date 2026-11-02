//! Local MCP stdio server. This is a deliberately read-only initial slice;
//! Workbench actions and mutations must wait for scoped Host/UI acknowledgments.
//!
//! Keep all stdout traffic as newline-delimited JSON-RPC. In particular, do
//! not delegate to the existing human-oriented CLI printing paths.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use serde_json::{json, Map, Value};
use tokio::io::{self, AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::cli::{McpAction, McpCommand, RuntimeDirArgs};
use crate::runtime_host_client::RuntimeHostRpcClient;

const PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_LINE_BYTES: usize = 1_048_576;
const MAX_RESULT_COUNT: usize = 100;
const MAX_TERMINAL_OUTPUT_BYTES: u64 = 8_192;
const MAX_FILE_SCAN_ENTRIES: usize = 20_000;

pub(crate) async fn run(command: McpCommand) -> i32 {
    match command.action {
        McpAction::Serve => serve(&command.runtime).await,
    }
}

async fn serve(runtime: &RuntimeDirArgs) -> i32 {
    let stdin = io::stdin();
    let mut input = BufReader::new(stdin);
    let mut output = io::stdout();
    // Do not start a Host as a side effect of a read-only MCP tool.
    let mut client = None;

    loop {
        let response = match read_mcp_line(&mut input).await {
            Ok(Some(McpLine::Data(line))) => dispatch(&line, runtime, &mut client).await,
            Ok(Some(McpLine::Oversized)) => {
                Some(rpc_error(Value::Null, -32600, "MCP request is too large"))
            }
            Ok(Some(McpLine::InvalidUtf8)) => Some(rpc_error(
                Value::Null,
                -32700,
                "Invalid UTF-8 in MCP request",
            )),
            Ok(None) => break,
            Err(error) => {
                eprintln!("Alera MCP stdin closed: {error}");
                return 1;
            }
        };
        if let Some(response) = response {
            let mut frame = match serde_json::to_vec(&response) {
                Ok(bytes) => bytes,
                Err(error) => {
                    eprintln!("Alera MCP could not encode response: {error}");
                    return 1;
                }
            };
            frame.push(b'\n');
            if let Err(error) = output.write_all(&frame).await {
                eprintln!("Alera MCP stdout closed: {error}");
                return 1;
            }
            if let Err(error) = output.flush().await {
                eprintln!("Alera MCP stdout flush failed: {error}");
                return 1;
            }
        }
    }
    0
}

#[derive(Debug, PartialEq, Eq)]
enum McpLine {
    Data(String),
    Oversized,
    InvalidUtf8,
}

/// Read a newline-delimited JSON-RPC frame without buffering an unbounded
/// request. Oversized input is drained to the next newline so a subsequent
/// valid request on the same stdio channel remains usable.
async fn read_mcp_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> io::Result<Option<McpLine>> {
    let mut content = Vec::new();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if content.is_empty() && !oversized {
                return Ok(None);
            }
            break;
        }
        let separator = available.iter().position(|byte| *byte == b'\n');
        let content_len = separator.unwrap_or(available.len());
        if !oversized {
            if content.len().saturating_add(content_len) > MAX_LINE_BYTES {
                oversized = true;
                content.clear();
            } else {
                content.extend_from_slice(&available[..content_len]);
            }
        }
        reader.consume(content_len + usize::from(separator.is_some()));
        if separator.is_some() {
            break;
        }
    }
    if oversized {
        return Ok(Some(McpLine::Oversized));
    }
    if content.last() == Some(&b'\r') {
        content.pop();
    }
    Ok(Some(match String::from_utf8(content) {
        Ok(line) => McpLine::Data(line),
        Err(_) => McpLine::InvalidUtf8,
    }))
}

async fn dispatch(
    line: &str,
    runtime: &RuntimeDirArgs,
    client: &mut Option<RuntimeHostRpcClient>,
) -> Option<Value> {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return Some(rpc_error(Value::Null, -32700, "Invalid JSON")),
    };
    let Some(object) = request.as_object() else {
        return Some(rpc_error(Value::Null, -32600, "Expected a JSON-RPC object"));
    };
    let id = object.get("id").cloned();
    let Some(id) = id else {
        // MCP notifications have no response. Do not let a notification
        // accidentally invoke tools, even if it claims tools/call as a method.
        return None;
    };
    if !id.is_string() && !id.is_number() {
        return Some(rpc_error(
            Value::Null,
            -32600,
            "Invalid JSON-RPC request id",
        ));
    }
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(rpc_error(id, -32600, "Expected JSON-RPC 2.0"));
    }
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return Some(rpc_error(id, -32600, "Missing JSON-RPC method"));
    };
    let params = object.get("params").unwrap_or(&Value::Null);
    let result = match method {
        "initialize" => {
            if params.as_object().is_none() {
                return Some(rpc_error(id, -32602, "initialize params must be an object"));
            }
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "alera", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Alera MCP reads the local runtime. Tab and agent details are restricted to the current live Alera terminal/workspace. UI and write tools are not enabled in this version."
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools": tool_definitions()}),
        "tools/call" => {
            let Some(params) = params.as_object() else {
                return Some(rpc_error(id, -32602, "tools/call params must be an object"));
            };
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id, -32602, "Missing tool name"));
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !arguments.is_object() {
                return Some(rpc_error(id, -32602, "Tool arguments must be an object"));
            }
            if !supported_tool(name) {
                return Some(rpc_error(
                    id,
                    -32602,
                    "Unknown or unavailable Alera MCP tool",
                ));
            }
            let result = execute_tool(name, &arguments, runtime, client).await;
            match result {
                Ok(value) => json!({
                    "content": [{"type": "text", "text": value.to_string()}],
                    "structuredContent": value,
                    "isError": false
                }),
                Err(error) => json!({
                    "content": [{"type": "text", "text": error}],
                    "isError": true
                }),
            }
        }
        _ => return Some(rpc_error(id, -32601, "Unknown MCP method")),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "alera.list_projects",
            "description": "List visible Alera projects; read-only. Filters by name or path.",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }, "additionalProperties": false},
            "annotations": {"readOnlyHint": true}
        }),
        json!({
            "name": "alera.list_workspaces",
            "description": "List Alera workspaces, optionally including archived ones.",
            "inputSchema": {"type": "object", "properties": {
                "projectId": {"type": "string"},
                "includeArchived": {"type": "boolean", "default": false},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }, "additionalProperties": false},
            "annotations": {"readOnlyHint": true}
        }),
        json!({
            "name": "alera.list_tabs",
            "description": "List tabs in the caller's current live Alera workspace. Does not reveal tab payloads.",
            "inputSchema": {"type": "object", "properties": {
                "workspaceId": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }, "additionalProperties": false},
            "annotations": {"readOnlyHint": true}
        }),
        json!({
            "name": "alera.search_files",
            "description": "Search file names and relative paths in the caller's live workspace. Bounded and Git-ignore aware; does not open files.",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string", "minLength": 1},
                "includeHidden": {"type": "boolean", "default": false},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }, "required": ["query"], "additionalProperties": false},
            "annotations": {"readOnlyHint": true}
        }),
        json!({
            "name": "alera.search_agents",
            "description": "Search running agents inside the caller's live workspace by prompt, type, activity or state.",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }, "additionalProperties": false},
            "annotations": {"readOnlyHint": true}
        }),
        json!({
            "name": "alera.get_agent_activity",
            "description": "Read bounded structured activity for an agent in the caller's live workspace.",
            "inputSchema": {"type": "object", "properties": {
                "handle": {"type": "string"}
            }, "required": ["handle"], "additionalProperties": false},
            "annotations": {"readOnlyHint": true}
        }),
        json!({
            "name": "alera.read_agent_output",
            "description": "Read bounded, cursor-paged terminal output of a session in the caller's live workspace. Output is not a completion receipt.",
            "inputSchema": {"type": "object", "properties": {
                "handle": {"type": "string"},
                "cursor": {"type": "integer", "minimum": 0},
                "maxBytes": {"type": "integer", "minimum": 1, "maximum": 8192}
            }, "required": ["handle"], "additionalProperties": false},
            "annotations": {"readOnlyHint": true}
        }),
    ]
}

fn supported_tool(name: &str) -> bool {
    matches!(
        name,
        "alera.list_projects"
            | "alera.list_workspaces"
            | "alera.list_tabs"
            | "alera.search_files"
            | "alera.search_agents"
            | "alera.get_agent_activity"
            | "alera.read_agent_output"
    )
}

async fn execute_tool(
    name: &str,
    args: &Value,
    runtime: &RuntimeDirArgs,
    client: &mut Option<RuntimeHostRpcClient>,
) -> Result<Value, String> {
    let result = execute_tool_connected(name, args, runtime, client).await;
    if result.is_err() {
        // A Host may have restarted. Reconnect on the next request; do not
        // automatically repeat it, even when later write tools are added.
        *client = None;
    }
    result
}

async fn execute_tool_connected(
    name: &str,
    args: &Value,
    runtime: &RuntimeDirArgs,
    client: &mut Option<RuntimeHostRpcClient>,
) -> Result<Value, String> {
    check_keys(name, args)?;
    if client.is_none() {
        *client = RuntimeHostRpcClient::connect(&crate::runtime_dir(runtime))
            .await
            .map_err(|_| "Cannot connect to the authorized Alera runtime host".to_string())?;
    }
    let client = client
        .as_mut()
        .ok_or_else(|| "Alera runtime host is not running; open Alera first".to_string())?;
    match name {
        "alera.list_projects" => {
            let limit = limit(args)?;
            let query = optional_str(args, "query")?.unwrap_or_default();
            let projects = host_array(client, "project.list", &json!({})).await?;
            let matched = projects
                .iter()
                .filter(|project| matches_query(project, &query, &["name", "repoPath"]))
                .count();
            let items = projects
                .iter()
                .filter(|project| matches_query(project, &query, &["name", "repoPath"]))
                .take(limit)
                .map(|project| project_fields(project))
                .collect::<Vec<_>>();
            Ok(
                json!({"kind": "projects", "items": items, "totalMatches": matched, "truncated": matched > limit}),
            )
        }
        "alera.list_workspaces" => {
            let limit = limit(args)?;
            let project_id = optional_str(args, "projectId")?;
            let include_archived = args
                .get("includeArchived")
                .map(|value| {
                    value
                        .as_bool()
                        .ok_or("includeArchived must be boolean".to_string())
                })
                .transpose()?
                .unwrap_or(false);
            let workspaces = host_array(client, "workspace.listAll", &json!({})).await?;
            let filtered = workspaces
                .iter()
                .filter(|w| {
                    project_id
                        .as_ref()
                        .is_none_or(|id| w["projectId"].as_str() == Some(id))
                        && (include_archived || w["status"].as_str() == Some("active"))
                })
                .collect::<Vec<_>>();
            let items = filtered
                .iter()
                .take(limit)
                .map(|item| workspace_fields(item))
                .collect::<Vec<_>>();
            Ok(
                json!({"kind": "workspaces", "items": items, "totalMatches": filtered.len(), "truncated": filtered.len() > limit}),
            )
        }
        "alera.list_tabs" => {
            let workspace = caller_workspace(client).await?;
            let requested = optional_str(args, "workspaceId")?;
            require_current_workspace(requested.as_deref(), &workspace)?;
            let limit = limit(args)?;
            let tabs = host_array(client, "tab.list", &json!({"workspaceId": workspace})).await?;
            let items = tabs.iter().take(limit).map(tab_fields).collect::<Vec<_>>();
            Ok(
                json!({"kind": "tabs", "workspaceId": workspace, "items": items, "totalMatches": tabs.len(), "truncated": tabs.len() > limit}),
            )
        }
        "alera.search_files" => {
            let workspace = caller_workspace(client).await?;
            let query = required_str(args, "query")?;
            let limit = limit(args)?;
            let include_hidden = args
                .get("includeHidden")
                .map(|value| {
                    value
                        .as_bool()
                        .ok_or("includeHidden must be boolean".to_string())
                })
                .transpose()?
                .unwrap_or(false);
            let workspaces = host_array(client, "workspace.listAll", &json!({})).await?;
            let current = workspaces
                .iter()
                .find(|item| {
                    item["id"].as_str() == Some(workspace.as_str()) && item["status"] == "active"
                })
                .ok_or("The caller's workspace is not active")?;
            let path = current["path"]
                .as_str()
                .filter(|path| !path.is_empty())
                .ok_or("Alera workspace path is missing")?;
            let root = PathBuf::from(path);
            let result = tokio::task::spawn_blocking(move || {
                search_workspace_files(&root, &query, limit, include_hidden)
            })
            .await
            .map_err(|_| "File search task failed".to_string())??;
            Ok(json!({
                "kind": "fileMatches", "workspaceId": workspace,
                "items": result.items, "totalMatches": result.matches,
                "scanned": result.scanned, "scanTruncated": result.scan_truncated,
                "truncated": result.matches > limit
            }))
        }
        "alera.search_agents" => {
            let workspace = caller_workspace(client).await?;
            let limit = limit(args)?;
            let query = optional_str(args, "query")?.unwrap_or_default();
            let agents = workspace_agents(client, &workspace).await?;
            let filtered = agents
                .iter()
                .filter(|a| {
                    matches_query(
                        a,
                        &query,
                        &[
                            "agentType",
                            "agentState",
                            "prompt",
                            "toolName",
                            "lastAssistantMessage",
                        ],
                    )
                })
                .collect::<Vec<_>>();
            let items = filtered
                .iter()
                .take(limit)
                .map(|a| agent_fields(a, false))
                .collect::<Vec<_>>();
            Ok(
                json!({"kind": "agents", "workspaceId": workspace, "items": items, "totalMatches": filtered.len(), "truncated": filtered.len() > limit}),
            )
        }
        "alera.get_agent_activity" => {
            let workspace = caller_workspace(client).await?;
            let handle = required_str(args, "handle")?;
            let agents = workspace_agents(client, &workspace).await?;
            let agent = find_agent(&agents, &handle)?;
            Ok(json!({"kind": "agentActivity", "agent": agent_fields(agent, true)}))
        }
        "alera.read_agent_output" => {
            let workspace = caller_workspace(client).await?;
            let handle = required_str(args, "handle")?;
            let agents = workspace_agents(client, &workspace).await?;
            find_agent(&agents, &handle)?;
            let max_bytes = args
                .get("maxBytes")
                .map(|value| {
                    value
                        .as_u64()
                        .ok_or("maxBytes must be a positive integer".to_string())
                })
                .transpose()?
                .unwrap_or(4096);
            if !(1..=MAX_TERMINAL_OUTPUT_BYTES).contains(&max_bytes) {
                return Err("maxBytes must be between 1 and 8192".to_string());
            }
            let cursor = args
                .get("cursor")
                .map(|value| {
                    value
                        .as_u64()
                        .ok_or("cursor must be non-negative integer".to_string())
                })
                .transpose()?;
            let output = client
                .request_value(
                    "terminal.read",
                    &json!({"sessionId": handle, "cursor": cursor, "maxBytes": max_bytes}),
                )
                .await
                .map_err(host_error)?;
            Ok(json!({
                "kind": "agentOutput",
                "handle": handle,
                "running": output["running"],
                "text": output["text"],
                "baseCursor": output["baseCursor"],
                "cursor": output["cursor"],
                "nextCursor": output["nextCursor"],
                "truncated": output["truncated"]
            }))
        }
        _ => Err("Tool is not enabled".to_string()),
    }
}

fn check_keys(tool: &str, args: &Value) -> Result<(), String> {
    let allowed: &[&str] = match tool {
        "alera.list_projects" => &["query", "limit"],
        "alera.list_workspaces" => &["projectId", "includeArchived", "limit"],
        "alera.list_tabs" => &["workspaceId", "limit"],
        "alera.search_files" => &["query", "includeHidden", "limit"],
        "alera.search_agents" => &["query", "limit"],
        "alera.get_agent_activity" => &["handle"],
        "alera.read_agent_output" => &["handle", "cursor", "maxBytes"],
        _ => return Err("Unknown tool".to_string()),
    };
    let args = args.as_object().ok_or("Tool arguments must be an object")?;
    if let Some(key) = args.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("Unsupported argument: {key}"));
    }
    Ok(())
}

fn limit(args: &Value) -> Result<usize, String> {
    let limit = args
        .get("limit")
        .map(|value| {
            value
                .as_u64()
                .ok_or("limit must be a positive integer".to_string())
        })
        .transpose()?
        .unwrap_or(50);
    if limit == 0 || limit > MAX_RESULT_COUNT as u64 {
        return Err("limit must be between 1 and 100".to_string());
    }
    Ok(limit as usize)
}

fn optional_str(args: &Value, key: &str) -> Result<Option<String>, String> {
    args.get(key)
        .map(|value| {
            let value = value
                .as_str()
                .ok_or_else(|| format!("{key} must be a string"))?;
            if value.len() > 512 {
                return Err(format!("{key} is too long"));
            }
            Ok(value.trim().to_owned())
        })
        .transpose()
}

fn required_str(args: &Value, key: &str) -> Result<String, String> {
    optional_str(args, key)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

fn require_current_workspace(requested: Option<&str>, current: &str) -> Result<(), String> {
    if requested.is_some_and(|value| value != current) {
        return Err("This MCP session is not authorized to read another workspace".to_string());
    }
    Ok(())
}

async fn caller_workspace(client: &mut RuntimeHostRpcClient) -> Result<String, String> {
    let handle = std::env::var("ALERA_TERMINAL_HANDLE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or("ALERA_TERMINAL_HANDLE is missing; a live Alera terminal is required")?;
    let workspace = std::env::var("ALERA_WORKSPACE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or("ALERA_WORKSPACE_ID is missing; a live Alera terminal is required")?;
    // A claimed environment variable alone is not sufficient even for
    // read-only Terminal observations: cross-check against the live Host.
    let session = client
        .request_value("orchestration.terminalShow", &json!({"handle": handle}))
        .await
        .map_err(|_| "Alera cannot validate the caller's live terminal handle".to_string())?;
    if session["workspaceId"].as_str() != Some(workspace.as_str()) || session["running"] != true {
        return Err(
            "Alera MCP session is stale or its workspace does not match the Host".to_string(),
        );
    }
    Ok(workspace)
}

async fn workspace_agents(
    client: &mut RuntimeHostRpcClient,
    workspace: &str,
) -> Result<Vec<Value>, String> {
    let envelope = client
        .request_value("orchestration.terminals", &json!({"workspace": workspace}))
        .await
        .map_err(host_error)?;
    let items = envelope["items"]
        .as_array()
        .ok_or("Invalid agent inventory response from Alera Host")?;
    Ok(items
        .iter()
        .filter(|item| {
            item["workspaceId"].as_str() == Some(workspace)
                && item["agentType"].is_string()
                && item["agentState"].is_string()
        })
        .cloned()
        .collect())
}

fn find_agent<'a>(agents: &'a [Value], handle: &str) -> Result<&'a Value, String> {
    agents
        .iter()
        .find(|agent| agent["handle"].as_str() == Some(handle))
        .ok_or_else(|| "Agent is missing or not authorized in this workspace".to_string())
}

async fn host_array(
    client: &mut RuntimeHostRpcClient,
    request: &str,
    payload: &Value,
) -> Result<Vec<Value>, String> {
    let result = client
        .request_value(request, payload)
        .await
        .map_err(host_error)?;
    result
        .as_array()
        .cloned()
        .ok_or_else(|| format!("Invalid response to {request}"))
}

fn host_error(error: anyhow::Error) -> String {
    // Do not pass raw Host error details to an agent; Host responses can
    // include filesystem paths and other privileged diagnostics.
    let _ = error;
    "Alera runtime host request failed".to_string()
}

fn matches_query(value: &Value, query: &str, keys: &[&str]) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || keys.iter().any(|key| {
            value[*key]
                .as_str()
                .is_some_and(|text| text.to_lowercase().contains(&query))
        })
}

fn selected(value: &Value, keys: &[&str]) -> Value {
    let mut fields = Map::new();
    for key in keys {
        if let Some(value) = value.get(key).filter(|value| !value.is_null()) {
            fields.insert((*key).to_string(), value.clone());
        }
    }
    Value::Object(fields)
}

fn project_fields(value: &Value) -> Value {
    selected(value, &["id", "name", "kind", "repoPath"])
}

fn workspace_fields(value: &Value) -> Value {
    selected(
        value,
        &[
            "id",
            "projectId",
            "name",
            "path",
            "branch",
            "kind",
            "status",
        ],
    )
}

fn tab_fields(value: &Value) -> Value {
    selected(value, &["id", "workspaceId", "kind", "title"])
}

fn agent_fields(value: &Value, details: bool) -> Value {
    let mut fields = selected(
        value,
        &[
            "handle",
            "workspaceId",
            "tabId",
            "running",
            "agentType",
            "agentState",
            "toolName",
            "updatedAt",
        ],
    );
    let extra = if details {
        [("prompt", 512usize), ("lastAssistantMessage", 512usize)]
    } else {
        [("prompt", 128usize), ("lastAssistantMessage", 128usize)]
    };
    if let Some(fields) = fields.as_object_mut() {
        for (key, max_chars) in extra {
            if let Some(text) = value[key].as_str() {
                fields.insert(
                    key.to_owned(),
                    json!(text.chars().take(max_chars).collect::<String>()),
                );
            }
        }
    }
    fields
}

struct FileSearchResult {
    items: Vec<Value>,
    scanned: usize,
    matches: usize,
    scan_truncated: bool,
}

fn search_workspace_files(
    root: &Path,
    query: &str,
    limit: usize,
    include_hidden: bool,
) -> Result<FileSearchResult, String> {
    if !root.is_dir() {
        return Err("Alera workspace directory is unavailable".to_string());
    }
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(!include_hidden)
        .git_ignore(true)
        .git_exclude(true)
        .follow_links(false)
        .max_depth(Some(64));
    let needle = query.to_lowercase();
    let mut result = FileSearchResult {
        items: Vec::new(),
        scanned: 0,
        matches: 0,
        scan_truncated: false,
    };
    for entry in walker.build() {
        result.scanned += 1;
        if result.scanned > MAX_FILE_SCAN_ENTRIES {
            result.scan_truncated = true;
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let name = relative
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        if name.to_lowercase().contains(&needle) {
            result.matches += 1;
            if result.items.len() < limit {
                result.items.push(json!({"relativePath": name}));
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    fn runtime() -> RuntimeDirArgs {
        RuntimeDirArgs { runtime_dir: None }
    }

    #[tokio::test]
    async fn oversized_stdio_frame_is_bounded_drained_and_followed_by_valid_frame() {
        let oversized = vec![b'x'; MAX_LINE_BYTES + 1];
        let valid = br#"{"jsonrpc":"2.0","id":42,"method":"ping"}"#;
        let mut frames = oversized;
        frames.push(b'\n');
        frames.extend_from_slice(valid);
        frames.push(b'\n');
        let mut reader = BufReader::with_capacity(7, std::io::Cursor::new(frames));
        assert_eq!(
            read_mcp_line(&mut reader).await.unwrap(),
            Some(McpLine::Oversized)
        );
        assert_eq!(
            read_mcp_line(&mut reader).await.unwrap(),
            Some(McpLine::Data(
                std::str::from_utf8(valid).unwrap().to_string()
            ))
        );
        assert_eq!(read_mcp_line(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn invalid_utf8_and_crlf_frames_do_not_break_stdio_reader() {
        let mut reader = BufReader::with_capacity(
            2,
            std::io::Cursor::new(b"\xff\n{\"jsonrpc\":\"2.0\"}\r\n".to_vec()),
        );
        assert_eq!(
            read_mcp_line(&mut reader).await.unwrap(),
            Some(McpLine::InvalidUtf8)
        );
        assert_eq!(
            read_mcp_line(&mut reader).await.unwrap(),
            Some(McpLine::Data(r#"{"jsonrpc":"2.0"}"#.to_string()))
        );
        assert_eq!(read_mcp_line(&mut reader).await.unwrap(), None);
    }

    #[test]
    fn clap_accepts_mcp_stdio_command() {
        assert!(matches!(
            Cli::try_parse_from(["alera", "mcp", "serve"])
                .unwrap()
                .command,
            crate::cli::Command::Mcp(_)
        ));
        assert!(Cli::try_parse_from(["alera", "mcp", "--runtime-dir", "custom", "serve"]).is_ok());
    }

    #[tokio::test]
    async fn stdio_initialize_and_tools_listing_are_valid_jsonrpc() {
        let mut host = None;
        let response = dispatch(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
            &runtime(),
            &mut host,
        )
        .await
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(response["id"], 1);
        let listed = dispatch(
            r#"{"jsonrpc":"2.0","id":"list","method":"tools/list"}"#,
            &runtime(),
            &mut host,
        )
        .await
        .unwrap();
        assert_eq!(listed["id"], "list");
        let tools = listed["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);
        assert!(tools
            .iter()
            .all(|tool| tool["annotations"]["readOnlyHint"] == true));
        assert!(tools
            .iter()
            .all(|tool| supported_tool(tool["name"].as_str().unwrap())));
    }

    #[tokio::test]
    async fn notifications_are_ignored_and_invalid_requests_return_errors() {
        let mut host = None;
        assert!(dispatch(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            &runtime(),
            &mut host
        )
        .await
        .is_none());
        assert!(dispatch(
            r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"alera.list_projects"}}"#,
            &runtime(),
            &mut host
        )
        .await
        .is_none());
        let parse_error = dispatch("{", &runtime(), &mut host).await.unwrap();
        assert_eq!(parse_error["error"]["code"], -32700);
        let unknown = dispatch(
            r#"{"jsonrpc":"2.0","id":2,"method":"unknown"}"#,
            &runtime(),
            &mut host,
        )
        .await
        .unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
        let unknown_tool = dispatch(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"alera.send_agent_input"}}"#, &runtime(), &mut host)
            .await
            .unwrap();
        assert_eq!(unknown_tool["error"]["code"], -32602);
    }

    #[test]
    fn tool_arguments_fail_closed_on_unknown_or_unbounded_values() {
        assert!(check_keys("alera.list_projects", &json!({"secret": "x"})).is_err());
        assert!(limit(&json!({"limit": 0})).is_err());
        assert!(limit(&json!({"limit": 101})).is_err());
        assert!(limit(&json!({"limit": -1})).is_err());
        assert!(require_current_workspace(Some("foreign"), "current").is_err());
        assert!(require_current_workspace(Some("current"), "current").is_ok());
    }

    #[test]
    fn tab_and_agent_projection_do_not_disclose_internal_payload() {
        let tab = tab_fields(&json!({
            "id": "tab1", "workspaceId": "w", "kind": "terminal", "title": "Terminal",
            "payload": {"token": "do-not-leak"}
        }));
        assert!(tab.get("payload").is_none());
        let agent = agent_fields(
            &json!({
                "handle": "s", "workspaceId": "w", "agentType": "codex", "agentState": "working",
                "toolInput": {"apiKey": "do-not-leak"}, "prompt": "p".repeat(1000)
            }),
            false,
        );
        assert!(agent.get("toolInput").is_none());
        assert!(agent["prompt"].as_str().unwrap().len() <= 128);
    }

    #[test]
    fn file_search_is_bounded_and_returns_only_relative_paths() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src").join("terminal_session.rs"), b"test").unwrap();
        std::fs::write(root.path().join("src").join("other.rs"), b"test").unwrap();
        let results = search_workspace_files(root.path(), "session", 5, false).unwrap();
        assert_eq!(results.matches, 1);
        assert_eq!(
            results.items,
            vec![json!({"relativePath": "src/terminal_session.rs"})]
        );
        assert!(!results.scan_truncated);
    }
}
