# Alera MCP Integration — fixed implementation and acceptance plan

**Date:** 2026-11-02

**Owner branch:** `feat/alera-mcp-control-20261102`

**Base HEAD:** `29781ab2545459a010095e700a19c7c49f952355`

**Status:** Steps 1 and 2 verified. **Step 2 Windows desktop accepted** on
2026-11-02: actual Alera Agent PTY → MCP stdio → Host → mounted Flutter UI,
12/12 E2E assertions passing, actual Git Graph/branch Diff screens captured.
The two-client window-routing test uses one real Flutter window plus a test
App client; two simultaneous real Flutter windows and macOS/Linux belong to
final platform acceptance. Steps 3–4 pending. No feature released.

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
- [x] Add stdio protocol and tool schema/unknown-command tests, including
      strict 1 MiB pre-buffer bound with drain-and-recover for oversized lines
      and invalid UTF-8 recovery.
- [x] Exercise Host-backed reads with an isolated live Alera Runtime Host,
      registered Project/Workspace/PTY and an injected test-only Agent status.
      Verify successful queries, output cursor and denial conditions.
- **Exit:** valid MCP client can initialize and query data without creating
  tabs; invalid calls receive stable JSON-RPC/MCP errors.

### 2. Workbench navigation + Git
- [x] Implement one Host → Flutter Workbench UI command/ACK bridge with
      per-Agent-PTY random Host-minted capability, local registered App window,
      request ID, 12-second Host timeout and separate accepted/applied receipt.
      An ordinary Terminal receives no UI token.
- [x] Wire file search/open/reveal (including Explorer folder expansion,
      scroll-to-highlight), open Git Graph and branch compare through existing
      Workbench services. Explorer ACK comes only after its tree controller
      has actually consumed the reveal/scroll request.
- [x] Initial **read-only** workspace file-name/relative-path search with
      `ignore` walker and scan bounds; UI navigation is now implemented.
- [x] Verify missing/unknown window, two-client ambiguous routing, explicitly
      selected real window, invalid path/ref, and repeated open (reuse the
      same Tab ID) from a **running Windows Flutter desktop app**, using a
      second synthetic App connection for ambiguity tests. No second real
      Flutter desktop window was started for this phase.
- **Exit:** real UI displays selected target; receipt acknowledges the UI
  action, not just database writes.

### 3. Agent observation + collaboration
- [x] Initial **same-workspace read-only** live agent discovery, bounded
      Terminal output cursor and structured progress.
- [ ] Add explicit cross-workspace authorization and focus/continuation.
- [x] Bind **UI-only** operations to a Host-minted per-Agent-PTY capability,
      live handle and workspace; revoke on PTY exit and rotate on respawn.
- [ ] Wider Agent observation/collaboration and other writes still need their
      own scoped authorization. Existing read-only Agent handle/workspace
      checks alone are **not** proof of caller identity.
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
- 2026-11-02: Continued verification: `cargo test -p alera-cli mcp::tests`
  passed **8/8**, including oversized 1 MiB input and invalid UTF-8 / CRLF
  framing. `cargo build -p alera-cli` passed. Actual executable proved it
  rejects an oversized frame and still answers a following valid `ping`.
- 2026-11-02: Added repeatable Windows integration harness
  `rust/alera-cli/tests/mcp_live_host_smoke.ps1`. It creates only disposable
  temp Runtime Host/Project/Workspace/PTY fixtures, injects test Agent status
  through the isolated Host socket and checks all seven read-only MCP tools.
  Live checks passed: project/workspace/tab/file search; Agent search/activity;
  actual PTY output marker and incremental cursor; raw `toolInput` redaction;
  cross-Workspace, foreign Agent, stale handle and bound violations denied.
  Script enforces shutdown and cleanup: zero test runtime directories remain.
  The user's normal Alera runtime remained stopped.
- **Step 1 limitations at its original acceptance:** No real Flutter UI or
  Agent Profile injection had been exercised; no macOS/Linux smoke.
  The existing same-Workspace check is a
  useful accidental-scope guard, **not** a cryptographic authorization
  boundary, so do not enable mutating or cross-Workspace operations.
- 2026-11-02: Step 2 bridge implemented in the isolated branch:
  `alera.open_file`, `alera.reveal_file`, `alera.open_git_graph`,
  `alera.compare_branches` are now included in the 11-tool MCP catalog but
  fail closed without a Host-minted per-Agent-PTY `ALERA_MCP_UI_TOKEN` and a
  registered local Flutter window. Generic Terminal tabs receive no token.
  Host distinguishes queued/accepted from confirmed applied/error, validates
  paths and Git refs, and revokes authority on PTY exit/window disconnect.
  Flutter reuses Workbench editor, Explorer reveal, Git Graph, and Git Diff
  services; direct Workspace selection never spawns a new Terminal.
- 2026-11-02: Host MCP UI bridge tests **4/4 PASS**, including forged
  capability/workspace denial and App-only ACK (a CLI cannot confirm an
  operation). Original MCP tests **8/8 PASS** and Rust CLI build PASS.
  Disposable Windows Host integration smoke PASS with all 11 schemas,
  read-only queries and denial of non-capability UI tools. Flutter
  `flutter analyze --no-pub` found **0 issues**; actual Explorer widget test
  confirms path reveal/selection/scroll ACK and the Workbench unit test
  confirms timeout/supersession fail closed.
- Flutter tests initially failed to load in the isolated worktree because
  local `.dart_tool/package_config.json` and pinned `third_party/xterm`
  and `third_party/dart_terminal` submodules were absent. Those two
  submodules were checked out at pinned commits **only inside this
  worktree** and `flutter pub get --offline` succeeded. Project dependency
  manifests and the original dirty `main` were not changed.
- 2026-11-02: **Step 2 Windows desktop acceptance PASS (12/12)**. Built an
  isolated debug GUI with `ALERA_E2E_DESKTOP=1` and an explicit compile-time
  `ALERA_E2E_SUPPORT_DIR`, then initialized only its own Drift/Host SQLite.
  `tool/testing/mcp_ui_desktop_fixture.py` created a disposable workspace
  and nested Git repository with `main` / `feature/demo` commits. A real
  Host-created Agent PTY (not a manually forged environment) invoked the
  MCP server through `tool/testing/mcp_ui_agent_desktop_e2e.py`.
  All 12 cases passed: open, repeat-open Tab ID reuse, Explorer reveal,
  Git Graph, branch compare, missing file denial, traversal denial,
  invalid Git ref denial, unknown-window denial, ambiguous two-client denial,
  explicitly routed real window, and single-window routing restoration.
- The **first real E2E found a product bug**: `uiControl.request` was
  accepted but Flutter filtered `mcpUiCommand` out of
  `runtimeHostEventNames`; MCP correctly reported `ui_timeout`. Added
  `mcpUiCommand` to the event allowlist and a socket-level Flutter
  regression that verifies the event reaches `runtimeEvents` while unknown
  events remain filtered. Rebuilt the real Windows debug executable, reran
  12/12 passing. The earlier Windows kernel compiler also found and fixed
  a missing `runtimeHostClientProvider` import in `alera_shell_page.dart`.
- Visual proof captured **only from the isolated E2E PID** under ignored
  `build/windows/shared_gui_ninja/diagnostics/`:
  `mcp-desktop-12passed.png` shows the reopened `needle.dart` and Git Graph
  Tab; `mcp-git-compare-applied.png` shows the `main ... feature/demo`
  Diff and modified `README.md`; `mcp-git-graph-applied.png` shows the
  commit graph with both actual test refs and their two commits.
- Host multi-window authorization tests **5/5 PASS**, including wrong-window
  ACK denial; Flutter socket event-routing regression PASS; earlier MCP
  protocol **8/8 PASS** and Explorer Widget/reveal-controller tests PASS.
  No production profile, unrelated Alera GUI, or dirty `main` was touched.
  **Remaining final acceptance:** two simultaneous *real* Flutter windows,
  macOS/Linux/platform smoke, Agent Profile MCP auto-injection and settings
  ownership. These remain in Steps 3–4, not additional stages in Step 2.

### Reproduce Step 2 Windows desktop acceptance (disposable profile only)

1. Compile the Windows debug app in the MCP worktree with the existing
   `tool/multiview_poc/build_alera_in_worktree_debug.cmd` after generating
   Flutter `--dart-define` for `ALERA_E2E_DESKTOP=1`, the absolute
   `ALERA_E2E_SUPPORT_DIR` under this worktree's ignored
   `build/windows/shared_gui_ninja/e2e_support`, and
   `ALERA_E2E_AUTO_SELECT_TERMINAL=true`. Use the current branch's Rust
   sidecar; do **not** point it at the normal Alera runtime.
2. Start and quit the isolated `runner/Debug/alera-dev.exe` once to create
   the two test databases. Run
   `python tool/testing/mcp_ui_desktop_fixture.py prepare`.
3. Restart only the isolated app, then run
   `python tool/testing/mcp_ui_desktop_fixture.py spawn-agent`.
   Run `mcp_ui_agent_desktop_e2e.py` **inside that live test Agent PTY**,
   passing the isolated Host `--runtime`, bundled `--cli`, and a result
   `--result` under ignored `build/windows/shared_gui_ninja/`.
   The test does not print or persist the Host-issued PTY capability.
4. Expect `E2E_SUMMARY 12 passed` and the same count in the JSON result.
   Optionally replay `--focus git_graph` / `--focus branch_compare` inside
   the same test PTY to inspect the real displayed Tabs. Stop only the
   isolated test Terminal/GUI when finished; never terminate another Alera.

## Current usage (unreleased alpha)

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
automatically into Agent Profiles. The four UI navigation tools are listed
but can only act from a live Host-issued Agent-PTY token; no result reports
`applied` before Flutter explicitly confirms it. Agent collaboration, raw
input and settings mutation tools remain **unimplemented/not advertised**.

### Reproducible Windows integration smoke

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File rust/alera-cli/tests/mcp_live_host_smoke.ps1 -AleraExe <built-alera.exe>
```

The harness creates a unique temporary profile, never touches the active
profile's projects and stops its own Host/PTYs before deletion.
