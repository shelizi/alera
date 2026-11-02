# Alera MCP Integration — fixed implementation and acceptance plan

**Date:** 2026-11-02

**Owner branch:** `feat/alera-mcp-control-20261102`

**Base HEAD:** `29781ab2545459a010095e700a19c7c49f952355`
**Status:** Step 1 read-only alpha implemented and unit-tested; real Host-connected
smoke and steps 2–4 remain pending. No feature has been enabled or released.

## Intent / fixed scope

Agents launched by Alera can inspect and navigate the running Alera workspace,
find and communicate with other live agents, inspect Git, and manage supported
Alera preferences through a scoped MCP stdio connection. Reuse the existing
Rust CLI, authenticated Runtime Host, Workbench services and Orchestration v2.
Never create a second Runtime Host, unprotected network endpoint, or duplicate
Git Graph/Diff UI.

### MCP tool contract (one stable `alera.*` namespace)

| Group | Tools | Intended result |
|---|---|---|
| Inventory | `alera.list_projects`, `alera.list_workspaces`, `alera.list_tabs` | Structured IDs, names, and scope |
| Files | `alera.search_files`, `alera.open_file`, `alera.reveal_file` | Find; edit or independently reveal a file in Alera Explorer |
| Git / UI | `alera.open_git_graph`, `alera.compare_branches` | Show existing Graph / branch comparison Tab |
| Agents | `alera.search_agents`, `alera.get_agent_activity`, `alera.read_agent_output`, `alera.focus_agent` | Locate live session; read bounded progress; focus correct UI |
| Collaboration | `alera.send_agent_message`, `alera.send_agent_input` | Durable coordination / separately authorized direct PTY input |
| Settings | `alera.list_settings`, `alera.get_setting`, `alera.update_setting`, `alera.open_settings` | Typed, scoped, redacted setting discovery and updates |

`alera.*` names are intentional MCP identifiers, not direct Runtime Host RPC
names. Read-only commands must not activate, create, or alter UI tabs. UI
operations report separate `accepted` and `applied` acknowledgments.

## Invariants

1. Existing dirty `main` and T2 cross-project Terminal transfer gates remain
   untouched: `supported=false`; no merge, push, release, or auto-install.
2. MCP uses **stdio JSON-RPC**, not TCP; only Alera-launched, scoped sessions
   are granted the mutating control surface. An unbound manual invocation may
   expose only deliberately allowed read-only operations.
3. Identity comes from a Host-validated per-session capability, not merely
   caller-supplied `ALERA_*_ID` variables; require project/workspace access
   before reading cross-workspace outputs or sending messages.
4. Direct Terminal input is separately gated; never infer approval from a
   successful MCP connection. Prefer existing queued Orchestration messages;
   ensure input delivery is not misreported as task completion.
5. Settings have three ownership tiers: local UI, operational Runtime Host,
   portable configuration. Changes go through their actual owner with typed
   allowlist, expected revision, audit trail, sensitive-value redaction and
   post-write verification. Never output API keys, tokens or arbitrary blobs.
6. Host-to-Flutter commands address a specific live window/workspace, carry
   request IDs and timeout, and return explicit applied/error ACK. Switching
   tabs does not recreate PTY sessions; missing UI is an error, not success.
7. File paths and Git refs are validated against the selected authorized
   workspace; avoid arbitrary disk traversal, command evaluation or shell
   interpolation. Every bounded list/search has a size limit.

## Fixed execution sequence and exit conditions

### 1. Transport + safe inventory (start here)
- [x] Implement `alera mcp serve` in existing Rust CLI with MCP stdio
      initialize, tools/list, tools/call and standardized errors.
- [x] Reuse authenticated Runtime Host RPC for project, workspace, tab and
      agent inventory; validate arguments and output bounds.
- [x] Add stdio protocol and tool schema/unknown-command tests.
- [ ] Exercise Host-backed reads with a live Alera Runtime Host. The transport
      smoke confirms an expected `isError` when no Host is available; this is
      not equivalent to a successful live query.
- **Exit:** valid MCP client can initialize and query data without creating
  tabs; invalid calls receive stable JSON-RPC/MCP errors.

### 2. Workbench navigation + Git
- [ ] Introduce one authorized Host → Flutter Workbench UI command/ACK bridge.
- [ ] Wire file search/open/reveal (including Explorer folder expansion,
      scroll-to-highlight), open Git Graph and branch compare through existing
      Workbench services.
- [x] Initial **read-only** workspace file-name/relative-path search with
      `ignore` walker and scan bounds; full UI navigation remains pending.
- [ ] Verify multi-window target, missing window, invalid path/ref and repeated
      open (reuse existing Tab).
- **Exit:** real UI displays selected target; receipt acknowledges the UI
  action, not just database writes.

### 3. Agent observation + collaboration
- [x] Initial **same-workspace read-only** live agent discovery, bounded
      Terminal output cursor and structured progress.
- [ ] Add explicit cross-workspace authorization and focus/continuation.
- [ ] Reuse Orchestration v2 messaging and dispatch; enforce session scope,
      idle delivery, idempotency, and an explicit gate for raw PTY input.
- [ ] Validate stop/restart/stale handle, busy agent, dedupe and unauthorized
      cross-workspace access.
- **Exit:** an agent can locate another agent, read progress, send a permitted
  message, and distinguish queued, delivered and completed.

### 4. Settings + injection + final acceptance
- [ ] Expose typed settings catalog and ownership-aware getter/updater; never
      return secret plaintext; block privileged or unsupported mutations.
- [ ] Inject MCP into supported Alera Agent Profile launches with adapter-aware
      config, no rewriting of unrelated global agent MCP configuration.
- [ ] Test MCP over real agent/stdio, UI behavior, settings notifications,
      auth scope, platform smoke tests and CI build; update usage docs.
- **Exit:** all supported tools have tests, feature-flag or opt-in policy is
  clear, integration passes; only then consider opt-in enablement/merge.

## Work log

- 2026-11-02: Plan created in isolated worktree before implementation; all
  steps initially pending. Start step 1 without expanding the scope.
- 2026-11-02: Added `alera mcp serve` to Rust CLI with MCP 2025-06-18
  line-oriented stdio JSON-RPC, bounded args and read-only tools:
  `alera.list_projects`, `alera.list_workspaces`, `alera.list_tabs`,
  `alera.search_files`, `alera.search_agents`,
  `alera.get_agent_activity`, `alera.read_agent_output`.
  Cross-workspace tab/agent observations fail closed; Host validates caller
  handle and workspace before these operations. No write tools advertised.
- 2026-11-02: `cargo fmt -p alera-cli` completed; `cargo test -p alera-cli
  mcp::tests` passed 6/6; `cargo build -p alera-cli` passed (existing
  unrelated `next_client_line` dead-code warning); `git diff --check`
  passed. Built binary stdio smoke: initialize and seven tools/list entries
  passed; Host-backed list_projects appropriately returned `isError=true`
  because a live Host was not available on the tested runtime profile.

## Current usage (read-only alpha)

```bash
alera mcp serve
# Optional runtime profile override; flags belong to the mcp command group:
alera mcp --runtime-dir /path/to/alera/runtime serve
```

The MCP client connects to the executable over stdio. No network listener is
opened and no Runtime Host is auto-started. Current workspace file/Tab/Agent
tools require `ALERA_TERMINAL_HANDLE` and `ALERA_WORKSPACE_ID` from an
active Alera-managed Terminal and an existing authenticated Host. Until the
later adapter-aware launch integration is implemented, this is not injected
automatically into Agent Profiles. Tool names for UI navigation, messaging,
raw input and settings are intentionally **not** advertised yet.
