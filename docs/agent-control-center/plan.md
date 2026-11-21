# Alera Global Agent Control Center and Session Search

Status: IN PROGRESS
Updated: 2026-11-21
Worktree: `.worktrees/agent-context-overview-20261121`
Branch: `feat/global-agent-control-session-search-20261121`
Base: `main` at `c7c3b2faa66b54eda3b767a5ee84b478ced45930`
Owners: feature worktree only; do not mutate `main` or other active worktrees.

## Spec

Deliver one cross-project Agent control center for both Classic Flutter and Native GPUI. It must enumerate all current Runtime Host Agent sessions regardless of the selected project, show the authenticated Agent state and Hook-derived overall objective/progress/timeline, accept explicit user input targeted to a verified session, create agents in a selected registered workspace, and distinguish hiding a view from terminating a PTY. The same control center must find historical Claude/Codex sessions by exact terms or natural-language task description, preview a result, and safely resume only the exact provider conversation.

User workflows:
1. Open Global Agents from either UI and see all open agents grouped by project/workspace; active workspace is not a filter.
2. Inspect goal, recent report, activity, and a drill-down timeline without opening a terminal.
3. Target one session and send a command only after checking the immutable Host session/workspace/tab triple.
4. Add an Agent with an explicit project, worktree, Provider, Profile and initial goal; close a view without killing a background task.
5. Search `Windows IME candidate window`, `IMM API`, `git diff`, or a function name across supported history; preview the original session with provenance; resume exact Claude/Codex UUID from the original registered checkout.
6. Fall back to preview or explicit new-session handoff when the Provider cannot resume, its native ID is absent, or its checkout is missing.

Success criteria:
- Classic and GPUI consume the same Runtime Host API and search records; no secondary PTY owner, no separate third-party background daemon.
- Global scope is explicit, not silently restricted to active project. No unverified/outdated Hook event shows a working state.
- Never confuse Alera Terminal handle with provider-native conversation UUID. Never use `--last` as a global restore action.
- No provider credentials, raw terminal output, unrestricted file contents, or secret-bearing transcripts enter logs or cloud services.
- Every result is labeled `Running / Exact resume available / Preview only / Source missing`; fresh live sessions have priority over duplicated historical records.
- Index increments only on changed sources and can be disabled or rebuilt. Search input never triggers process startup or writes to PTY.

Non-goals for v1: automatic cross-provider native transcript import, silent multi-agent broadcasting, arbitrary shell execution from search hits, guessing task completion percentages, background embeddings on every terminal byte, auto-close/terminate on UI dismissal.

## Design and ownership

- Shared Rust Core: normalized Agent session metadata, result classification, exact-match and token ranking, search query/ranking contract. A bounded index and FTS5 API are built here after pure functions are validated.
- Runtime Host: authoritative current-session/project/worktree IDs; authenticated inventory, status, context and lifecycle actions; provider-native ID map; persistent historical record and background incremental scan with no-start reads.
- Providers: Claude and Codex history adapters first. Other providers expose their capabilities (`history`, `resume`, `steer`) and return `Preview only` until verified.
- Classic Flutter: a global screen/dialog reachable without changing workspace, using existing Riverpod workbench/host APIs and design tokens.
- GPUI: native global center panel using existing TerminalDock host inventory and bounded context; no duplicate PTY or sidecar.
- Search: use SQLite FTS5 exact search first, then opt-in local embeddings for semantic reranking; retrieve per-session chunks (objective + milestones + redacted user/assistant content) rather than indexing entire raw PTY streams.
- Identity: (runtime instance, projectId, workspaceId, tabId, host sessionId, provider, provider UUID, account/profile context) with guarded comparison at every action.
- Ordering: (exact phrase, technical term and path match, semantic score, objective relevance, recency), dedupe by provider-native ID plus workspace.
- Security: allowlist sources, per-account scope and local-only encrypted/OS-protected storage where available; ignore credentials and tool payloads, cap chunk size and scan I/O. Reconcile deletion and revocation.
- Operations: event-driven status updates and bounded refresh, no full transcript reread per render. Search and embedding work off UI threads; auto-stop when idle.
- Backward compatibility: only additive Host fields/API, preserve old CLI and Classic host clients. Do not bump strict Host protocol version.

## Workstreams and dependencies

### P0 - Global operation center

- [ ] D01 Global dashboard for every project/worktree in Classic and GPUI.
- [ ] D02 Live working/waiting/blocked/complete/offline with stale guard.
- [ ] D03 Task-level goal; [ ] D04 latest summary; [ ] D05 bounded timeline.
- [ ] D06 Direct targeted input with result/ack (implementation wired in both UIs; GUI smoke pending); [ ] D07 queue/steer with capability guard.
- [ ] D08 Launch Agent from explicit workspace/profile (Classic registered Worktree/Provider; GPUI all Host-registered Worktrees + Profile + task with no workspace switch, durable recovery and confirmation; Windows GUI smoke pending).
- [ ] D09 Hide, interrupt, end, force-stop semantics and confirmation (Hide Card is view-only; verified Interrupt Turn and verified Terminate Session use two-click confirmation in Classic/GPUI; a distinct graceful end vs force-stop, transferred owner recovery and real GUI smoke still pending); [ ] D10 return to original Terminal.
- [ ] D11 Needs You pending attention queue; [ ] D12 native exact Claude/Codex resume.

### P1 - Session search

- [ ] S01 Provider history discovery across registered projects/worktrees.
- [ ] S02 Stable metadata/provenance and native provider conversation ID.
- [ ] S03 Local keyword full-text index and ranking; [ ] S04 opt-in local semantic embeddings; [ ] S05 hybrid rerank.
- [ ] S06 Goal and progress search; [ ] S07 in-session topic chunks and deep links.
- [ ] S08 Project/Provider/date/status/worktree filters.
- [ ] S09 Safe preview and match explanation; [ ] S10 exact one-click provider resume; [ ] S11 running-session focus.
- [ ] S12 Incremental indexing, deletion reconciliation, bounded resources.

### P2 - Advanced usability

- [ ] A01 Group/sort, [ ] A02 card/list density, [ ] A03 parent/subagent tree, [ ] A04 Kanban.
- [ ] A05 Targeted multiple-agent command dispatch with confirmation, [ ] A06 explicit cross-provider handoff.
- [ ] A07 health/stall metrics, [ ] A08 cost/quota per scope, [ ] A09 favorites/tags, [ ] A10 detached dashboard window.

### Cross-cutting

- [ ] C01 Global registry projection, [ ] C02 strict identities, [ ] C03 Provider Adapter capability matrix.
- [ ] C04 SQLite FTS5/vector extension, [ ] C05 local embedding, [ ] C06 resource limits.
- [ ] C07 privacy/sensitive-text policy, [ ] C08 reconnect and mutation idempotency.
- [ ] C09 Rust Core shared contract for Classic/GPUI, [ ] C10 tests and Windows smoke acceptance.

## Implementation slices and progress journal

| Slice | Acceptance | Status | Evidence |
| --- | --- | --- | --- |
| 0. Workspace isolation and tracking | Clean idle worktree, feature branch and persisted plan | DONE | Worktree inspected clean; independent branch from main |
| 1. Search model and deterministic ranker | Bounded, cross-project session projection, explainable exact/token matching, resume classification, unit tests | DONE | `rust/alera-core/src/global_agent_search.rs`; 9/9 Rust tests passed |
| 2. Existing Host global inventory API and both UI entries | Classic and GPUI can open global list, inspect goal and focus exact live session | IN PROGRESS | Classic dialog and sidebar/welcome entries, 2/2 projection tests; GPUI global panel and cross-workspace focus, cargo check passed; full GUI smoke pending |
| 3. Targeted commands and creation/lifecycle | Permission-checked routing, no premature PTY termination | IN PROGRESS | `agent.writeVerified` plus local-only `agent.interruptVerified` and `agent.terminateVerified`; exact Host Session/Workspace/Tab and Agent Hook identity checks, transferred PTY fence, other-viewer termination rejection, two-click confirmation in both UIs; Classic/GPUI hide card is view-only. GUI smoke and separate graceful end/force semantics pending |
| 4. Claude/Codex historical adapters and FTS index | Index real provider records, privacy/retention, dedupe and incremental updates | TODO | |
| 5. Exact resume from search result | Correct provider UUID, account, registered checkout and safe fallback | TODO | |
| 6. Local semantic reranker and topic chunks | Natural-language retrieval plus code/error exact-match priority | TODO | |
| 7. Cross-version UI and system acceptance | Classic and GPUI parity, long-running/lost-host/restart/privacy tests | TODO | |

### Progress log

- 2026-11-21: verified active workspaces; selected clean `agent-context-overview-20261121` worktree, original merged branch no longer needed, created feature branch from `main`. Existing GPUI and Classic context-overview code verified. Observed native terminal Host list is global and exact Claude/Codex resume already exists.
- 2026-11-21: implemented `rust/alera-core/src/global_agent_search.rs`: bounded exact/token matching, filters, typed account+profile-safe identity, live-over-history deduplication, and guarded exact Claude/Codex resume classification. `cargo test --manifest-path rust/Cargo.toml -p alera-core --lib global_agent_search` passed 9/9.
- 2026-11-21: implemented Classic `global_agent_live_projection.dart` and `global_agent_dashboard_dialog.dart`, accessible from expanded/collapsed sidebar and Welcome quick start. Projects and worktrees are enumerated globally; verified workspace/tab/terminal-session matching, search, goal/progress and focus. `flutter test --no-pub test/unit/global_agent_dashboard_projection_test.dart` passed 2/2.
- 2026-11-21: implemented Native GPUI `native_global_agents_ui.rs` in the existing right Agent Context view. Live Host inventory all workspaces, compact goal/progress cards and revalidated cross-workspace focus reuse the existing draft-safe Agent notice navigation. `cargo check --manifest-path desktop_gpui/Cargo.toml --bin alera-gpui-desktop` passed (2 preexisting dead-code warnings).
- 2026-11-21: Classic full-widget analysis is not validated: `dart analyze` exceeded 180 seconds. An initial UI-dependent Flutter test was blocked on existing unrelated `TerminalTheme` and `GhosttyTerminalShellLaunch` compile issues; extracting the pure projection allowed its tests to pass. Do not mark full Classic GUI compile/desktop smoke complete.
- 2026-11-21 continuation: added fail-closed `agent.writeVerified` RPC and CLI `--expected-workspace-id` / `--expected-tab-id` + `--submit` to route instruction bytes on stdin to the existing PTY only when Host verifies live Session/Workspace/Tab atomically. Older Host rejects the new RPC; missing or mismatched IDs are never downgraded to ordinary write. Existing Terminal `write` remains unchanged. Classic Global Dashboard supports selecting a live Agent, entering a bounded prompt, receiving Host ACK and creating Codex/Claude in an explicitly selected registered worktree. GPUI supports selecting a live Agent and guarded prompting inside its global panel, with current-worktree entry to existing launcher. No auto-retry on ambiguous write ACK.
- 2026-11-21 continuation verification: Classic pure projection 3/3 passed (`flutter test --no-pub test/unit/global_agent_dashboard_projection_test.dart`); GPUI `cargo check --manifest-path desktop_gpui/Cargo.toml --bin alera-gpui-desktop` passed (two existing dead-code warnings); GPUI terminal CLI command test 1/1 passed; Host `write_reports_missing_exited_and_writerless_sessions_and_empty_writes_noop` 1/1 passed using dedicated RPC; CLI guarded-flag parser 1/1 passed. Host route policy 1/1 passed (`cargo test --manifest-path rust/Cargo.toml -p alera-cli --bin alera terminal_and_agent_skill_routes_are_typed_deferred_jobs`). Full Classic GUI smoke, true cross-project Native launch, lifecycle controls, history FTS and embeddings remain pending.

- 2026-11-21 continuation (third slice): GPUI global panel now offers an actual registered Host Worktree picker, Host-owned Agent Profile catalog and initial task. A per-worktree durable create_new/fsync journal is reserved BEFORE the Host Profile launch; fresh Host Worktree ID+canonical path are revalidated in the background. Launch does not change the active Workbench, create a second PTY owner or attach to the wrong pane. A fresh authenticated Host session snapshot verifies the launched tab before clearing the journal; missing/ambiguous replies leave the marker and require explicit two-step acknowledgement after inspecting a fresh Host inventory. Marker origin distinguishes Dashboard Profile launches from ordinary Terminal creates and supports restart recovery. Classic and GPUI now support Hide Card without terminating a Host PTY and clear hidden input targets. GPUI hides are pruned after authoritative session refresh.
- 2026-11-21 third-slice verification: GPUI `cargo check --manifest-path desktop_gpui/Cargo.toml --bin alera-gpui-desktop` passed after the launcher and recovery changes (2 unchanged dead-code warnings); `cargo test --manifest-path desktop_gpui/Cargo.toml --lib launch_target_requires_the_same_unique_registered_workspace` 1/1 passed (unique Host ID and canonical path, duplicate/recycled ID and path); `cargo test --manifest-path desktop_gpui/Cargo.toml --lib dashboard_profile_launch_marker_is_distinct_and_survives_restart` 1/1 passed; Classic projection `flutter test --no-pub test/unit/global_agent_dashboard_projection_test.dart` 3/3 passed. Targeted `dart analyze lib/src/features/workbench/presentation/global_agent_dashboard_dialog.dart` and `dart analyze lib/src/features/workbench/presentation/global_agent_launch_dialog.dart` both passed with no issues after correcting Host interface narrowing and replacing deprecated dropdown `value` with `initialValue`. Existing single-worktree `unconfirmed_host_create_survives_restart_and_blocks_duplicate_shell` 1/1 passed after journal schema change. Neither UI has full real Agent GUI smoke acceptance. Explicit Host termination is intentionally not bundled with Hide Card.

- 2026-11-21 fourth slice: added **Agent lifecycle** controls in both UIs without changing Host ownership. `agent.interruptVerified` is a local-only deferred input operation using the verified Agent adapter's interrupt bytes (not a paste/Enter or PTY termination); `agent.terminateVerified` is a separate local-only RPC that reuses Host terminate's transfer-owner checks and cleanup. Both require fresh live Session/Workspace/Tab identity, valid Hook Agent type, and reject transferred/ABA PTYs for global control; termination additionally refuses attached viewers on other Host clients. Old Hosts fail closed. GPUI CLI `terminal --no-start control-agent-verified --action interrupt|terminate` requires complete IDs, and termination demands `--confirm-terminate`. Classic Host client opts into a dedicated lifecycle interface; both dashboards arm exact identity/action on first click, execute only on matching second click, and never auto-repeat ambiguous ACKs.
- 2026-11-21 fourth-slice tests: GPUI `cargo check` passed (same two existing dead-code warnings); Host lifecycle negative test `global_agent_lifecycle_rejects_wrong_owner_plain_terminal_and_moved_pty` 1/1 passed; `global_agent_control_parses_fixed_action_and_explicit_host_identity`, `terminal_and_agent_skill_routes_are_typed_deferred_jobs`, and Native `terminal_writes_are_explicit_bounded_and_never_start_host` each 1/1 passed. Classic pure projection 3/3 passed. `dart analyze` of the Dashboard itself passed without issues; full Client+Dashboard analysis remains blocked by preexisting missing `ghostty_vte_flutter` and `GhosttyTerminalShellLaunch` resolution. Wire test `global_agent_terminate_refuses_other_viewers_then_removes_only_exact_session` 1/1 passed: refuses an Agent attached in another Host client and removes exactly the verified Host Session + Tab when no other client remains. Full Windows dual-UI execution/smoke not yet performed.

## Test plan

1. Unit: token normalization (CJK/ASCII), ranking, technical identifiers, escaping, empty/oversized text, privacy-negative fields.
2. Unit: provider UUID validation, mismatched workspace/account, running session duplicate, preview-only cases.
3. Host: across two registered projects and linked worktrees, current inventory and stale Hook state do not cross-contaminate.
4. Host: restart, missing checkout, deleted Provider log, schema migration, index rebuild, resource budget, no accidental start.
5. Classic: Dashboard open under arbitrary active project, correct focus/send/stop and Session history preview.
6. GPUI: same workflows with dual pane, modal/focus and Host disconnection.
7. Windows: Claude and Codex real resume with two simultaneous Agent tabs; must restore exact conversation, never `--last`.

## Assumptions and handoff rules

- Provider transcript locations and exact history formats must be verified against installed versions before adapters read them; no unsupported Provider declared resumable merely because a CLI can launch.
- New Agent control actions must use the same existing CLI/Host permissions, trust and confirmation flow.
- A historical Session match must never auto-switch Git branches or create a worktree; only explicit user actions can start/restore.
- All scope/completion checks are tracked here. Do not mark a phase complete until both Classic and GPUI have tested user-reachable entry points.
