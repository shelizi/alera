# Alera MCP Integration — fixed implementation and acceptance plan

**2026-11-04 latest production readiness / merge gate (native interactive Claude):**
A new explicit-only acceptance at
`tool/testing/mcp_managed_claude_interactive_autonomy_e2e.py` launched a real
authenticated **interactive Claude TUI** directly in a disposable
Host-owned Windows ConPTY, with private per-invocation `--settings` calling
Alera's **real Rust `agent-hook-relay`** and the private Host listener.
The real first `UserPromptSubmit` marked the Agent busy, a separate genuine
Host-minted Agent PTY queued an MCP message, and the model's first native
`Stop` caused Host paste/deferred Enter into the running Claude TUI.
Claude **autonomously answered the new message** with a nonce-derived
response not supplied by the first prompt or injected text, and emitted
another native `UserPromptSubmit` and `Stop`. The report keeps only
fixed assertions and Hook event names: **7/7 PASS, twice, exit 0; private
Host shutdown and temporary filesystem removal both confirmed**.
No Python adapter generated the second model response, no credentials were
copied, no global Claude settings were mutated, and no trust/permission
bypass was used. The first run completed 7 checks but initially failed
strict Windows cleanup; after waiting for the real ConPTY child release,
both consecutive complete reruns passed.

Separately, the bounded single-turn noninteractive real Claude
Hook→Host→ConPTY test at
`tool/testing/mcp_real_claude_native_host_delivery_e2e.py` passed
**7/7 twice**, with the receiver intentionally inert Python. Rust MCP
authorization/transport regression **25/25 PASS**. GitHub's already
authorized Desktop Builds (Windows/macOS/Linux 3/3), native Unix PTY
(2/2) and Windows PTY (1/1) all passed at `239062cdf`, but those
workflows do **not** run the dedicated native MCP macOS/Linux smoke.

**No merge / no release yet:** this proves autonomous interactive Claude
inside a Host-owned PTY, but **not** an `agentProfile.launch` production
Managed Profile automatically achieving the same lifecycle, nor an
equivalent interactive **Codex** real-model second-turn acceptance.
Managed MCP launch injection remains explicitly opt-in/default OFF.
The protected `.github/workflows/mcp-cross-platform.yml` is still
uncommitted and the dedicated native macOS/Linux MCP E2E is unexecuted.
The existing local `main` also has preexisting unrelated dirty files
which must never be reset/overwritten by an integration attempt.
Cross-workspace grants remain fail-closed; a visible second real
Flutter Settings pane spontaneous repaint is still a separate acceptance
item. No unsupported claim of a production-ready complete rollout.

**Date:** 2026-11-02

**Owner branch:** `feat/alera-mcp-control-20261102`

**Base HEAD:** `29781ab2545459a010095e700a19c7c49f952355`

**Status:** Steps 1–2 verified. Step 3 same-workspace Agent focus and durable
messaging Windows desktop **10/10 PASS**; synthetic busy→done status with
**real PTY paste/deferred Enter/persisted delivery 4/4 PASS**. Step 4 typed
settings Windows desktop **13/13 PASS**, and managed Codex/Claude MCP launch
argument injection is implemented but **opt-in only (default OFF)**.
**Real Windows Host OS-process restart + same-handle Agent capability
rotation 5/5 PASS**, installed native Codex/Claude CLI parsing and bundled
MCP stdio 3/3 PASS. **Actual authenticated Codex and Claude model →
MCP tool call → Host workspace result 2/2 PASS** using a disposable
Host-issued PTY and transient CLI configuration. **Alera Managed Profile
auto-launch and MCP argument injection on real Codex/Claude OS processes
5/5 PASS**, while managed Claude's *model execution* pauses on the normal
new-folder trust gate. **Actual Claude model → Alera MCP open_file →
Flutter applied ACK 1/1 PASS**, with the test editor tab visually verified.
**Actual Claude model → allowlisted MCP setting mutation → Flutter
SettingsController applied/readback → original value restored 1/1 PASS**.
A real Claude sender model → authenticated MCP send → durable Host PTY delivery
→ a second real Claude model reply, with **5/5 PASS** and verified disposable
Host cleanup (2026-11-03). On the actual Alera Shell startup path,
Hook receiver lifecycle and AgentStatusHostForwarder are now both watched,
and Shell dispose uses a captured RuntimeHostClient instead of ref.read after
unmount. Targeted Shell widget **1/1 PASS** and enabled-Claude Hook HTTP
UserPromptSubmit→Stop→AgentStatusController→Host working→done forwarding
**1/1 PASS**. A fresh full Shell widget run on 2026-11-04 passes
**104/104**, superseding the historical pending-timer/cleanup failures.
The recipient's idle state in the earlier collaboration E2E is supplied by a
bounded test adapter only *after* its real prior model turn returned; native
interactive Managed Claude/Codex Hook lifecycle, first-folder trust, and
autonomous Agent-to-Agent responses remain pending. Cross-window Settings
Controller + visible Editor field notification regressions are now accepted;
second native GUI visible Settings pane acceptance, explicit cross-workspace
grants, and native macOS ARM64/Linux x64 CI remain pending. The Windows **two genuine Flutter Shells +
Host-owned Agent PTY + explicit-window MCP routing, file navigation and
setting CAS cross-window readback/rollback** passed **16/16** on 2026-11-03
in a disposable E2E profile. This verifies observed setting reads, not
unsolicited Settings UI rebuild notifications.

**2026-11-03 rebuild gate:** the isolated Windows GUI kernel was rebuilt and
the Rust CLI was force-recompiled with `ALERA_E2E_REUSE_CLI=0` (Cargo dev
profile and isolated Debug install completed with exit 0). This exposed a
dangerous fixture mismatch: the prior `ALERA_E2E_REUSE_CLI=1` Debug bundle
contained a stale `alera.exe` that exited 64 for `mcp --help`. The isolated
build script and dual-GUI test now fail closed if the installed CLI lacks
`mcp`. With the newly built MCP CLI, `mcp_cross_platform_smoke.py` completed
`MCP_NATIVE_PLATFORM_SMOKE_PASS`, including a real Host-owned PTY, inherited
Agent bearer and unauthorized-access denial.

**2026-11-04 Windows dual-GUI recovery (same disposable profile):**
The rebuilt GUI and MCP sidecar now pass the stricter **17/17 real two-Flutter
windows / Host-owned Agent PTY / MCP E2E** twice in succession with command
exit 0, including both actual windows' open_file ACKs, ambiguous/forged window
denials, an additional pre-navigation get_setting ACK, cross-window settings
CAS and second-window readback, and restoration of the original setting.
Both runs confirmed that the isolated Runtime Host shut down. No synthetic
App registrations were counted. The earlier new-build failure had multiple
real causes: UI registration raced the E2E-only initial workspace selection;
Windows clients treated Host-advertised `configurationSyncV1` as unsupported;
and broad settings loads could silently return a stale local Drift snapshot.
The Shell now registers after initial selection; MCP scalar reads query the
Host's authoritative portable/runtime settings with a strict 8-path whitelist,
deny unavailable sources rather than returning stale values, and the client
correctly parses the advertised configuration capability. The E2E cleanup
harness waits for actual Host exit and never counts a teardown error as PASS.
Focused Host capability unit 1/1, settings repository/controller 14/14 PASS,
`git diff --check` PASS. This establishes Windows MCP readback, **not**
automatic rebuild of Settings UI in the other window, native Managed Claude
idle-hook/autonomous response, macOS/Linux native CI, or
cross-workspace grant authorization. The full Shell widget suite passed in
a subsequent 2026-11-04 run. No merge, push or release.

**2026-11-04 Windows native Claude/Codex Hook command transport:**
The actual Windows `.cmd` scripts generated by the production managed
runtime-home services were executed using `cmd.exe` and their PowerShell HTTP
forwarding, under separate disposable home directories (including spaces in
the paths). Both Claude and Codex delivered `UserPromptSubmit` and `Stop`
to a local test-only HTTP sink with the authenticated Hook header and exact
Host Agent terminal/workspace/tab identity. Focused tests **1/1 PASS per
provider**, full Claude + Codex runtime-home suites **63/63 PASS**,
`git diff --check` PASS. No global provider config, trust approvals, model
credentials or user workspaces were changed. This proves the generated native
Hook-script-to-HTTP transport by itself. The separate real Claude provider
model test below additionally verifies native provider emission. Host inbox
delivery driven by interactive Managed Agent events and autonomous Managed
Agent replies remain unverified.

**2026-11-04 reactive cross-window SettingsController notifications:**
`runtimeSettingsChangesProvider` previously emitted identical
`AsyncData<void>(null)` values on each Host `runtimeSettingsChanged` event,
so Riverpod deduplicated successive settings notifications. It now emits a
monotonic integer revision with regenerated Riverpod provider types. The
isolated negative control reproduced the bug: two identical revisions
refreshed window B's SettingsController only once; two distinct revisions
correctly refreshed it twice. A separate test verified actual Host
`runtimeSettingsChanged` events become revisions 1 and 2 while unrelated
events are ignored. Focused SettingsController + MCP CAS and concurrency
suites **24/24 PASS**. The isolated Windows Flutter GUI/Rust sidecar was
rebuilt successfully (exit 0), and the updated native two-real-window MCP
routing/settings CAS/Host cleanup regression passed **17/17** (exit 0).
These verify reactive Controller propagation and no regression in the
existing real GUI MCP actions; they do not, by themselves, prove a second
real Flutter GUI's visible Settings pane repaints without manual read.
Full native real-GUI visual-notification acceptance remains pending.

**2026-11-04 parallel acceptance streams (Windows + CI static validation):**
The full Shell widget suite passed **104/104** and SettingsDialog widget suite
passed **58/58**, including a *visible Editor Tab Size TextField* observing two
successive external Host settings events without a fresh MCP read. The Rust CLI
`cargo test -p alera-cli --bin alera mcp` suite passed **25/25**; a new
disposable Windows native MCP/Host/PTY authorization smoke completed
`MCP_NATIVE_PLATFORM_SMOKE_PASS`. Real two-Claude-model collaboration
was replayed with **5/5 PASS**, still using a test adapter to report
recipient idle and invoke its next model turn; this does **not** establish
natural interactive Agent restart. The explicit-only test at
`tool/testing/mcp_real_claude_native_hook_e2e_test.dart` executed an
**actual authenticated Claude Code non-interactive model turn** with the
real Windows Hook script installed by `ClaudeRuntimeHomeService` in a
private home. The model itself emitted `UserPromptSubmit` and `Stop`
through Alera's generated Hook command into an isolated loopback HTTP sink:
**1/1 PASS**. It did not access/write global provider settings, approve
trust, or retain provider/model transcripts. This proves real provider
event emission and Hook transport, not native interactive Managed Profile
inbox-driven autonomous response. Native macOS ARM64/Linux x64 runner matrix
and Python smoke script passed **offline/static validation only**; actual
runners have NOT been executed. The local machine has no Docker executable
or registered WSL distro, so it cannot substitute for native Linux CI.
Protected workflow changes remain uncommitted; no merge/push/release.

**2026-11-04 native Claude Hook status-to-Host acceptance (isolated):**
An authenticated real Claude `--print` model turn emitted its own
`UserPromptSubmit` and `Stop` via Alera's generated Windows command Hook.
The private HTTP sink now passes the exact event envelope through the
production `parseAgentHookRequest`, `AgentStatusController` and
`AgentStatusHostForwarder`. This observes `working → done` plus the
correct Agent terminal, workspace, tab and provider identities, without
forging a state transition; explicit real-model acceptance **1/1 PASS**
(twice). Its `RuntimeHostClient` is an in-memory recording test adapter,
**not** the real Host/PTY. Separately, a Rust `ServerActor` test now
exercises `handle_agent_hook_event` using only a disposable RuntimeStore:
disabled Claude hooks are ignored, wrong tab/workspace cannot mark an Agent
idle, the correctly scoped Claude `Stop` marks the real Host presence
`Done`/injection-ready, and disabling hooks prevents subsequent updates
(**1/1 PASS**). Rust MCP security/transport regression **25/25 PASS**;
Flutter Hook receiver + Host forwarder regression **19/19 PASS**. These
are distinct validated legs of the lifecycle, not a claim that a running
interactive Managed Profile automatically receives a queued message and
produces a second model reply.

**2026-11-04 native HTTP Hook -> real Host inbox -> Windows PTY (disposable):**
Two new independent acceptance layers are green. In Rust, a genuine
localhost HTTP receiver created by `start_hook_receiver` reads an
ephemeral private token, rejects 403 for invalid bearer, forwards authorized
`UserPromptSubmit` and `Stop` through its `ServerCommand` queue to the
real `ServerActor`, rejects forged Workspace/Tab identities, and verifies
`Working -> Done` / injection-ready (**1/1 PASS**, repeated). In Windows
`tool/testing/mcp_native_hook_host_delivery_e2e.py`, a fully disposable
Runtime Host owns two real Agent-scoped ConPTY sessions: a sender uses its
Host-minted MCP bearer to queue a message while a genuine Hook HTTP
`UserPromptSubmit` has marked the receiver Working; invalid bearer and
foreign Workspace/Tab Stop attempts do not deliver; a legitimate HTTP Stop
causes actual Host PTY paste and deferred Enter. A safe Python stdin
consumer confirms receipt, and the durable status stays `delivered` with
`completionConfirmed=false`. **5/5 PASS twice, exit 0, private Host stopped
and disposable filesystem cleaned**. The test-only Hook payload disables
unrelated automatic title-generation jobs so cleanup cannot be blocked by
an LLM subprocess. All Hook metadata stays local; tests do not install
or change real user agent settings. These prove the real Host network and
message-delivery boundary, but the HTTP events are sent by the isolated
test harness and the receiving PTY runs inert Python, **not** an interactive
Claude/Codex model; autonomous recipient model reply remains unverified.

**2026-11-04 GitHub fork desktop and PTY CI (existing workflows):**
The feature branch at `a9d6d2566` passed GitHub Desktop Builds on
Windows/macOS/Linux (3/3), native Terminal Worker on macOS/Linux (2/2),
and Windows PTY Job (1/1). The protected, dedicated
`.github/workflows/mcp-cross-platform.yml` remains uncommitted; these
six green jobs do **not** substitute for its unexecuted MCP-specific
macOS/Linux Host/PTY smoke. No main merge or Release.

**Cross-platform enablement follow-up (same isolated branch):** the existing
portable stdio MCP implementation and Mac/Linux packaged CLI locations are
now covered by `tool/testing/mcp_cross_platform_smoke.py` plus a dedicated
native Mac ARM64/Linux x64 workflow and post-desktop-package smoke. The shared
Host/PTY smoke, including genuine Host-minted Agent bearer inheritance and
denial after stripping/altering the bearer, passed locally on a disposable
Windows runtime. The two protected `.github/workflows/` changes are
still uncommitted due connector staging policy; **CI has not
yet run on native macOS/Linux**, and their real Flutter/Agent E2E gates remain
pending. See `docs/alera-mcp-cross-platform.md` for opt-in testing.

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
| Collaboration | `alera.send_agent_message`, `alera.get_agent_message_status` | Durable coordination; queued/delivered are distinct from Agent completion; raw PTY input is intentionally not exposed |
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
   portable configuration. Changes go through the existing Flutter
   SettingsController and underlying owner with a typed allowlist,
   per-field `expectedValue` compare-and-set and post-write verification.
   An end-to-end Runtime revision/audit contract is **not yet implemented**.
   Never output API keys, tokens or arbitrary blobs.
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
- [x] `alera.focus_agent` uses a Host-verified live same-workspace Agent
      handle, resolves its actual Terminal Tab ID and waits for Flutter's
      `applied` receipt. Repeated focus never creates or restarts a PTY.
- [x] Bind all **new Agent focus/messaging/status writes** to a Host-minted
      per-Agent-PTY capability, live handle and workspace; revoke on PTY exit.
      Do not treat original read-only env/handle checks as cryptographic proof.
- [x] Reuse Orchestration v2 durable inbox for `alera.send_agent_message`;
      require caller-scoped `clientMutationId` idempotency, same-workspace
      live Agent recipient, busy-Agent queuing and bounded input. Report
      `queued` or `delivered` separately from task completion.
- [x] Raw PTY input **not offered** by MCP. Cross-workspace writes/groups
      are denied. Host unit tests cover forged capability, stale/missing or
      foreign handles, busy queuing, idempotency, receipt privacy, and focus
      target validation. Actual Windows UI/Agent E2E **10/10 PASS**.
- [ ] Define and verify **explicit cross-workspace grants**, if enabled in
      the future. The currently supported scope is same-workspace only,
      fail closed.
- [x] Synthetic Agent `working`→`done` status sent through the actual Host
      event path, real recipient PTY paste/deferred Enter and persisted
      `delivered_at` E2E **4/4 PASS**. No manual DB delivery marking or raw
      forced input. `completionConfirmed=false` even after delivery.
- [x] Full **Windows OS Host process stop/restart/PTY token rotation 5/5
      PASS** with a disposable private runtime, project and same session
      handle. Host-issued token is transferred in-memory through test-only
      loopback (never printed/stored); stopped Host uses `--force` because
      a live Agent PTY correctly blocks ordinary shutdown; old bearer is
      rejected, new PTY bearer accepted, old remains revoked. The separate
      in-process Host-replacement unit test also passes.
- [x] Real authenticated Codex and Claude Code models each invoked
      `alera.list_workspaces` over their own transient MCP registration from
      an Alera Host-owned PTY. The test checked actual model tool-call events
      and **the returned disposable Workspace ID**, not model prose: **2/2
      PASS**. Both used limited, read-only test sessions.
- [x] **Actual two-Claude-model collaboration E2E 5/5 PASS** in disposable
      Host-owned sender/recipient Agent PTYs: the sender model issued exactly
      one MCP `alera.send_agent_message` tool call with correlated receipt,
      Host persisted/delivered the message into the recipient PTY, and a
      separately invoked real Claude model returned the nonce-specific ACK.
      Test cleanup confirmed; no user profile or global MCP config changed.
      `tool/testing/mcp_real_model_collaboration_e2e.py`.
- [ ] Native Managed Codex/Claude **idle hook → queued delivery → autonomous
      recipient model response** still needs E2E. The test above uses an
      adapter that reports idle *after* a real model turn and invokes another
      model turn after receiving a real PTY message; it is NOT proof that
      provider-native status hooks or automatic turn resumption work.
- **Current Step 3 exit (same-workspace only):** Agent can find another
  Agent, read bounded progress, focus its existing Tab, queue one permitted
  durable message, and inspect its delivery state. Global/cross-workspace
  authorization and actual natural Agent idle delivery/response remain
  explicitly outside this accepted subset. OS Host token rotation is verified.

### 4. Settings + injection + final acceptance
- [x] Expose a typed **eight-entry allowlisted** settings catalog via
      `alera.list_settings/get_setting/update_setting/open_settings`.
      Five harmless settings can be changed via the existing Flutter
      SettingsController. Three destructive-confirmation/archive controls are
      read-only. Unknown/secret paths and wrong scalar types fail closed.
      Require `expectedValue` CAS, serialized owner persistence and
      Host-verified Flutter scalar readback; Windows desktop **13/13 PASS**,
      including real Terminal settings dialog visibly selected.
- [x] Implement **default-off** per-Managed-Agent-Profile opt-in injection
      (`ALERA_MCP_PROFILE_INJECTION=1`) for Codex/Claude adapter-specific
      CLI arguments. No global MCP config writes; duplicate/custom config
      does not get replaced; no token in the config. Static argument tests
      **3/3 PASS**, plus regression for Claude's variadic MCP config option
      and both adapters' initial prompt terminator.
      Real installed Codex 0.165.0 discovers the transient config, Claude
      2.1.312 accepts the transient flag, bundled MCP stdio lists all 18
      tools: **3/3 PASS**. This is NOT a model/tool call E2E.
- [x] Actual Codex/Claude **model-initiated read-only MCP tool call and result
      2/2 PASS** using `tool/testing/mcp_real_model_agent_e2e.py`. The models
      ran inside the Host-owned PTY with transient MCP CLI flags, not through
      Alera Managed Profile auto-spawn.
- [x] Real Windows Host `agentProfile.launch` created **Codex and Claude
      Managed Profiles** in a disposable Workspace with the Host opt-in
      environment set. Both live PTYs and both native OS launch command lines
      contained the per-session Alera MCP config; Codex also spawned a real
      Alera MCP stdio child. **5/5 PASS** via
      `tool/testing/mcp_managed_profile_injection_inspect.py`.
- [x] One actual Claude Code model (in the Host-issued disposable PTY)
      invoked only `alera.open_file` for `src/deep/needle.dart`. The exact
      correlated tool result contained `status: applied` and `tabId`;
      the Windows Flutter editor visibly selected `needle.dart`. **1/1 PASS**,
      `tool/testing/mcp_real_model_flutter_ui_e2e.py`.
- [x] One actual authenticated Claude model invoked only
      `alera.update_setting` for a test-only `editor.tabSize`, with an
      exact prior-value CAS. The correlated tool result confirmed
      `kind=setting` and `status=applied`; a separate real Flutter
      `alera.get_setting` confirmed the new value, and the harness
      **restored the original value via Host-backed CAS even on failure**.
      **1/1 PASS**, `tool/testing/mcp_real_model_settings_e2e.py`. This
      verifies the Flutter settings owner, not every other open window
      reacting to a setting notification.
- [ ] Approve managed Claude's new **disposable** Workspace trust challenge
      through its normal UI before claiming that *the Managed Profile's own
      model* completed a tool call. An empty isolated Claude config has no
      existing OAuth, so the test did not copy credentials, silently approve
      trust or rewrite the user's Claude config. Managed Codex startup and
      MCP server process were independently verified.
- [ ] Settings change notifications in a second real window, real Managed
      Profile Claude model response after trust approval, macOS/Linux, two true
      Flutter windows, cross-workspace grants and final CI/release gates.
      Do not equate startup/injection with model task completion.
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
- 2026-11-02: **Step 3 same-workspace Windows acceptance PASS**.
  `alera.focus_agent` uses the existing Host→Flutter UI command/ACK bridge;
  the Host verifies a running local Agent and resolves the existing Tab ID,
  and Flutter checks the Tab's Terminal session before focusing. Added
  `agentControl.scope/send/messageStatus` with Host-minted live Agent
  capability, local CLI role, same-workspace recipient requirement, bounded
  subject/body, mandatory caller-scoped idempotency and private message
  receipts. `send` reuses Orchestration v2 durable queued delivery; no
  direct PTY input or cross-workspace send was enabled.
- **Stage 3 tests:** Host authorization/focus/message **3/3 PASS**;
  MCP stdio **8/8 PASS** with **14 advertised tools (8 read-only, 6 guarded)**;
  disposable Windows Host integration smoke PASS including denial of all new
  controls without a Host capability; Flutter Shell compilation **6/6 PASS**
  and Host event regression **1/1 PASS**; Windows Debug isolated GUI built
  with same-branch Rust sidecar and compile-time E2E support directory.
- **Real desktop E2E 10/10 PASS:** two disposable Host-spawned PTYs, synthetic
  `working` Agent presence confined to the test profile, actual MCP stdio
  calls from the sender's capability-bearing PTY, live Agent search/activity,
  busy recipient message queued, repeat ID deduplicated, conflicting ID
  rejected, own-message status, foreign/unknown target denied, and
  focus/repeated-focus of actual recipient Terminal Tab acknowledged by
  Flutter. Report at ignored
  `build/windows/shared_gui_ninja/mcp_agent_stage3_e2e_result.json`;
  visual screenshot at ignored
  `build/windows/shared_gui_ninja/diagnostics/mcp-stage3-focus-agent-10passed.png`.
  Repro harness: `tool/testing/mcp_agent_desktop_e2e.py`.
  This proves queue + focus, **not natural real Agent idle delivery or task
  completion**. The host status unit test separately verified the stored
  delivered-state projection.
- 2026-11-02: **Stage 3 delivery follow-up:** in an isolated Alera Flutter
  GUI with two real Host-spawned PTYs, `tool/testing/mcp_agent_delivery_e2e.py`
  verified **4/4**: working recipient remains queued; `done` Agent status
  causes real Host PTY paste + deferred Enter and `delivered_at` in SQLite;
  message status changes to delivered; `completionConfirmed=false`.
  Result JSON at ignored
  `build/windows/shared_gui_ninja/mcp_stage3_delivery_e2e_result.json`.
  The hook status is **synthetic**, not a real Codex/Claude model turn.
- 2026-11-02: **Stage 4 settings Windows desktop E2E PASS 13/13** from a
  Host-issued Agent PTY, using the same private Alera Flutter GUI. Tested
  catalog, rejection of secrets/unsafe switches/out-of-range values,
  Terminal font size and Git Auto Fetch frequency owner-backed updates,
  readback, stale expected value rejection and restoration of original
  settings, plus actual Settings dialog opened to the Terminal pane.
  `tool/testing/mcp_settings_desktop_e2e.py`; JSON:
  `build/windows/shared_gui_ninja/mcp_settings_stage4_e2e_result.json`,
  screenshot:
  `build/windows/shared_gui_ninja/diagnostics/mcp-stage4-settings-dialog-13passed.png`.
  Initial E2E found 16.0 from Flutter != integer 16 from MCP input;
  fixed the Rust numeric readback equivalence, retaining strict type/range
  validation, and reran all 13 cases successfully. In-memory Drift CAS/owner
  tests **2/2 PASS**; Host settings receipt/test and MCP protocol tests pass.
- 2026-11-02: The per-managed-profile MCP auto-injection implementation
  adds transient Codex `--config mcp_servers.alera-workbench...` or Claude
  `--mcp-config` only when `ALERA_MCP_PROFILE_INJECTION=1` at Host launch.
  Requires Host-managed Agent Profile, existing sidecar executable and
  runtime dir; unsupported/custom profiles fail closed. Pure adapter argument
  regression **3/3 PASS**. Real provider startup and macOS/Linux not yet
  exercised; do not enable by default. Existing global client config is
  never modified.
- 2026-11-02: **Actual Windows Host OS restart E2E 5/5 PASS** using
  `tool/testing/mcp_real_host_restart_e2e.py` against only
  `build/windows/shared_gui_ninja/e2e_host_restart/runtime`. Start a
  private Host, register a private project/workspace, spawn an Agent-style
  Host-owned PTY, verify its in-memory Host capability, then force-stop
  the **test Host only** because Alera correctly refuses an ordinary stop
  with a live Terminal. Restart actual Host executable with persistent
  SQLite and same session handle; prove stale bearer rejected, freshly
  minted bearer accepted, old bearer remains revoked. The test passes only
  ephemeral credentials over loopback within memory; result JSON at
  `build/windows/shared_gui_ninja/e2e_host_restart/mcp_real_host_restart_result.json`.
  Script also guards against reusing any existing private data.
  First harness attempt exposed two testing issues (read-only status
  initializes the SQLite profile, starter pipe inherited by background
  Host); fixed harness preflight/DEVNULL spawning. No production Alera
  process/profile was stopped. Private Host stopped after completion.
- 2026-11-02: **Installed Agent CLI compatibility 3/3 PASS**:
  `tool/testing/mcp_installed_cli_compat.py` uses isolated Codex/Claude
  CLI config homes, no copied credentials/model requests/global MCP
  modifications. Real Codex 0.165.0 parses two per-launch TOML
  `mcp_servers.alera-workbench` overrides and reports the exact stdio
  command and args. Real Claude Code 2.1.312 accepts the per-launch
  `--mcp-config` JSON flag; its varargs require the existing `--`
  initial prompt separator, now regression-tested. Bundled Alera MCP
  handshake advertises 18 tools. Report at ignored
  `build/windows/shared_gui_ninja/mcp_installed_cli_compat_result.json`.
  This verifies CLI native parsing, **not** real model-initiated tool calls,
  which remain an explicit pending gate.
- 2026-11-02: **Actual Codex/Claude model-initiated MCP read-only calls 2/2
  PASS**. Reused only the private, already-registered
  `build/windows/shared_gui_ninja/e2e_host_restart` Runtime and spawned
  Host-managed Agent PTY. Actual authenticated Codex 0.165.0 ran an
  ephemeral read-only `codex exec` with transient MCP overrides. Actual
  authenticated Claude Code 2.1.312 ran a nonpersistent bounded
  `--print` session, `--strict-mcp-config`, disabled built-in tools and
  pre-approved **only** the read-only MCP tool being tested. Both models
  called `alera.list_workspaces`. The E2E parser correlated actual tool
  call events with the real tool result and checked the isolated Workspace ID;
  it does not trust the model's final text as proof.
  Reproduction: `tool/testing/mcp_real_model_agent_e2e.py` invoked from
  the same disposable Host-issued PTY. Sanitized reports (no auth or tool
  transcript) at ignored
  `build/windows/shared_gui_ninja/e2e_host_restart/mcp_real_model_codex_result.json`
  and `mcp_real_model_claude_result.json`.
  Initial Claude probe using `--bare` could not use existing OAuth because
  that flag intentionally disables OAuth/Keychain access; a subsequent
  attempt also needed explicit approval of the one read-only MCP tool.
  Both issues were confined to test invocation parameters, not production
  Alera MCP source. These tests do **not** yet verify Alera Managed Profile
  opt-in auto-injection, mutating UI controls, or natural inter-Agent
  conversation completion.
- 2026-11-02: **Real Windows Managed Profile launch + MCP injection 5/5
  PASS**. Launched a private Host with
  `ALERA_MCP_PROFILE_INJECTION=1` under ignored
  `build/windows/shared_gui_ninja/e2e_host_restart`, declared two
  `launchMode=managed` Agent Profiles (Codex `sandbox=read-only`;
  Claude `permissionMode=plan`) and invoked the existing
  `agentProfile.launch` RPC, not custom or manual Agent CLI commands.
  Both PTYs ran in the disposable Workspace; Windows native process
  inspection verified transient Alera MCP arguments for both and an actual
  MCP stdio child spawned by Codex. Test-only inspection script:
  `tool/testing/mcp_managed_profile_injection_inspect.py`; sanitized
  report in ignored
  `build/windows/shared_gui_ninja/e2e_host_restart/mcp_managed_profile_injection_result.json`.
  Managed Claude halted at its normal new-folder trust confirmation; the
  test **did not** approve it, alter global Claude trust settings or copy
  OAuth into an isolated config. The managed Host was stopped afterward.
- 2026-11-02: **Actual Claude model-driven Windows Flutter UI open 1/1
  PASS**. Started only the isolated Alera Dev GUI using the existing
  `e2e_support` Host and its Host-minted test Agent PTY. A real bounded
  Claude session was allowed exactly the
  `mcp__alera-workbench__alera_open_file` operation targeting
  `src/deep/needle.dart`. The model emitted the real MCP tool_use and a
  correlated successful tool_result containing **status applied + tabId**;
  a screenshot verified `needle.dart` selected in Flutter. The direct
  MCP-from-PTY preflight also passed. Repro:
  `tool/testing/mcp_real_model_flutter_ui_e2e.py`; sanitized result in
  `build/windows/shared_gui_ninja/mcp_real_model_flutter_ui_result.json`,
  screenshot in
  `build/windows/shared_gui_ninja/diagnostics/mcp-real-claude-flutter-open-file-applied.png`.
  Initial false negatives came from test code JSON-encoding an already
  JSON-formatted tool_result string; corrected and reran successfully.
  Only a disposable test editor tab was opened; no files were modified.

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
reads check a live Host handle and matching `ALERA_TERMINAL_HANDLE` /
`ALERA_WORKSPACE_ID`, but that read-only check alone is not a cryptographic
identity proof. All **five UI navigation/focus tools**, plus same-workspace
durable Agent send, own-message status, and typed settings read/write/open
tools additionally require a Host-issued per-PTY capability. No UI response
reports `applied` before Flutter confirms it; settings updates validate
allowed scalar types, an expected prior value and returned actual value.
`send_agent_message` returns queued/delivered, never task completed.
Managed Codex/Claude Agent Profile MCP injection is currently **opt-in only**:
set `ALERA_MCP_PROFILE_INJECTION=1` for the *isolated Host launch* to enable
per-launch adapter config. Never modify global Codex/Claude MCP files.
Unverified real-provider startup, privileged cross-workspace grants, raw
Terminal input and arbitrary settings mutation remain unavailable.

### Reproducible Windows integration smoke

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File rust/alera-cli/tests/mcp_live_host_smoke.ps1 -AleraExe <built-alera.exe>
```

The harness creates a unique temporary profile, never touches the active
profile's projects and stops its own Host/PTYs before deletion.
