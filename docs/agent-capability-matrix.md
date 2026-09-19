# Agent Capability Matrix

## 1. Purpose and Scope

This document implements the Phase 5 agent capability matrix defined in handoff section 19 (`docs/handoff-architecture-refactor-2026-09-11.md`, section 19). It inventories every agent and provider identifier recognized in the codebase across Rust, Dart, and UI layers, mapping support status across six core operational dimensions: launch configuration and execution, hook installation and integration, status and presence reporting, usage and quota tracking, restart and session resume behavior, and model override capabilities.

## 2. Agent Inventory

The codebase currently recognizes 15 distinct agent or provider identifiers across its orchestration, hook, status, and quota registries. 12 agents are registered as spawnable workbench adapters and status entities, while 3 providers operate strictly in the quota and settings layer.

| Identifier | Display Name | Primary Code Definition / Registration | Registry Domain |
| --- | --- | --- | --- |
| `codex` | Codex | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:43` / `lib/src/features/agent_status/domain/agent_status.dart:17` | Spawnable Adapter, Hook, Status, Quota, Usage, AI Assist |
| `claude` | Claude Code | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:50` / `lib/src/features/agent_status/domain/agent_status.dart:18` | Spawnable Adapter, Hook, Status, Quota, Usage, AI Assist |
| `copilot` | GitHub Copilot | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:58` / `lib/src/features/agent_status/domain/agent_status.dart:19` | Spawnable Adapter, Hook, Status, AI Assist |
| `cursor` | Cursor | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:65` / `lib/src/features/agent_status/domain/agent_status.dart:20` | Spawnable Adapter, Hook, Status, Quota, AI Assist |
| `agy` / `antigravity` | Antigravity | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:72` / `lib/src/features/agent_status/domain/agent_status.dart:21` | Spawnable Adapter, Hook, Status, Quota (`antigravity`), AI Assist |
| `opencode` | OpenCode | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:79` / `lib/src/features/agent_status/domain/agent_status.dart:22` | Spawnable Adapter, Hook, Status, Quota, AI Assist |
| `opencode2` | OpenCode 2 | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:87` / `lib/src/features/agent_status/domain/agent_status.dart:23` | Spawnable Adapter, Hook, Status, AI Assist |
| `pi` | Pi | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:94` / `lib/src/features/agent_status/domain/agent_status.dart:24` | Spawnable Adapter, Hook, Status, AI Assist |
| `amp` | Amp | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:101` / `lib/src/features/agent_status/domain/agent_status.dart:25` | Spawnable Adapter, Hook, Status, AI Assist |
| `grok` | Grok Build | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:108` / `lib/src/features/agent_status/domain/agent_status.dart:26` | Spawnable Adapter, Hook, Status, Quota, Usage, AI Assist |
| `devin` | Devin | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:115` / `lib/src/features/agent_status/domain/agent_status.dart:27` | Spawnable Adapter, Hook, Status, Quota |
| `fx` | fx | `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:122` / `lib/src/features/agent_status/domain/agent_status.dart:28` | Spawnable Adapter, Status (Herdr Socket), AI Assist |
| `kimi` | Kimi | `rust/alera-core/src/runtime/agent_quota_settings_models.rs:40` / `lib/src/features/settings/domain/agent_quota_settings.dart:7` | Quota Provider |
| `minimax` | MiniMax | `rust/alera-core/src/runtime/agent_quota_settings_models.rs:44` / `lib/src/features/settings/domain/agent_quota_settings.dart:11` | Quota Provider |
| `zai` | Z.ai | `rust/alera-core/src/runtime/agent_quota_settings_models.rs:45` / `lib/src/features/settings/domain/agent_quota_settings.dart:12` | Quota Provider |

## 3. Capability Matrix

Values indicate verified capability status: `yes` (fully supported in production code), `partial` (supported in a restricted mode, platform subset, or single sub-feature), or `no` (no code implementation found). Every cell cites representative source files and line numbers.

| Agent | Launch | Hook | Status | Usage | Restart | Model Override |
| --- | --- | --- | --- | --- | --- | --- |
| `codex` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:79`) | yes (`rust/alera-cli/src/agent_status/integration_config_codex.rs:16`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:101`) | yes (`rust/alera-cli/src/agent_quota.rs:252`, `rust/alera-cli/src/agent_quota/usage.rs:38`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`) | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:92`) |
| `claude` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:138`) | yes (`rust/alera-cli/src/agent_status/integration_config.rs:135`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:111`) | yes (`rust/alera-cli/src/agent_quota.rs:251`, `rust/alera-cli/src/agent_quota/usage.rs:37`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`) | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:168`) |
| `copilot` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:202`) | yes (`rust/alera-cli/src/agent_status/integration_config.rs:190`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:121`) | no (`rust/alera-cli/src/agent_quota.rs:250`, `lib/src/features/settings/domain/agent_quota_settings.dart:4`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`, `rust/alera-cli/src/agent_status/normalize.rs:23`) | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:217`) |
| `cursor` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:251`) | yes (`rust/alera-cli/src/agent_status/integration_config_cursor_overlay.rs:39`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:122`) | partial (`rust/alera-cli/src/agent_quota.rs:255`; no transcripts in `rust/alera-cli/src/agent_quota/usage.rs:36`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`, `rust/alera-cli/src/agent_status/normalize.rs:24`) | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:262`) |
| `agy` / `antigravity` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:284`) | yes (`rust/alera-cli/src/agent_status/integration_config.rs:284`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:143`) | partial (`rust/alera-cli/src/agent_quota.rs:256`; no transcripts in `rust/alera-cli/src/agent_quota/usage.rs:36`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`) | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:296`) |
| `opencode` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:321`) | yes (`rust/alera-cli/src/agent_status/integration_plugins.rs:3`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:156`) | partial (`rust/alera-cli/src/agent_quota.rs:260`; no transcripts in `rust/alera-cli/src/agent_quota/usage.rs:36`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`) | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:323`) |
| `opencode2` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:328`) | yes (`rust/alera-cli/src/agent_status/integration_plugins.rs:11`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:156`) | no (`rust/alera-cli/src/agent_quota.rs:250`, `lib/src/features/settings/domain/agent_quota_settings.dart:4`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`) | partial (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:328-334`) |
| `pi` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:336`) | yes (`rust/alera-cli/src/agent_status/integration_plugins.rs:20`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:162`) | no (`rust/alera-cli/src/agent_quota.rs:250`, `lib/src/features/settings/domain/agent_quota_settings.dart:4`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`, `rust/alera-cli/src/agent_status/normalize.rs:25`) | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:338`) |
| `amp` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:359`) | yes (`rust/alera-cli/src/agent_status/integration_plugins.rs:28`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:172`) | no (`rust/alera-cli/src/agent_quota.rs:250`, `lib/src/features/settings/domain/agent_quota_settings.dart:4`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`) | no (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:360`) |
| `grok` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:371`) | yes (`rust/alera-cli/src/agent_status/integration_config.rs:226`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:179`) | yes (`rust/alera-cli/src/agent_quota.rs:254`, `rust/alera-cli/src/agent_quota/usage.rs:39`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`, `rust/alera-cli/src/agent_status/normalize.rs:26`) | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:383`) |
| `devin` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:65`) | yes (`rust/alera-cli/src/agent_status/integration_config.rs:257`) | yes (`rust/alera-cli/src/agent_status/normalize.rs:180`) | partial (`rust/alera-cli/src/agent_quota.rs:259`; no transcripts in `rust/alera-cli/src/agent_quota/usage.rs:36`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`, `rust/alera-cli/src/agent_status/normalize.rs:27`) | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:67`) |
| `fx` | yes (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:421`) | partial (`rust/alera-cli/src/agent_status/fx_herdr_receiver.rs:25`; no files in `lib/src/features/agent_status/infra/managed_agent_hook_descriptors.dart:34`) | partial (`rust/alera-cli/src/agent_status/normalize.rs:188`; Unix-only in `rust/alera-cli/src/agent_status/fx_herdr_receiver.rs:72`) | no (`rust/alera-cli/src/agent_quota.rs:250`, `lib/src/features/settings/domain/agent_quota_settings.dart:4`) | yes (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`, `rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:423`) | no (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:422`) |
| `kimi` | no (`rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:42`) | no (`rust/alera-cli/src/agent_status/integration_config.rs:40`) | no (`rust/alera-cli/src/agent_status/normalize.rs:100`) | partial (`rust/alera-cli/src/agent_quota.rs:253`, `rust/alera-cli/src/agent_quota/kimi.rs:1`; no transcripts in `rust/alera-cli/src/agent_quota/usage.rs:36`) | no (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`) | no (`lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart:157`) |
| `minimax` | no (`rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:42`) | no (`rust/alera-cli/src/agent_status/integration_config.rs:40`) | no (`rust/alera-cli/src/agent_status/normalize.rs:100`) | partial (`rust/alera-cli/src/agent_quota.rs:257`, `rust/alera-cli/src/agent_quota/plans.rs:1`; no transcripts in `rust/alera-cli/src/agent_quota/usage.rs:36`) | no (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`) | no (`lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart:157`) |
| `zai` | no (`rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:42`) | no (`rust/alera-cli/src/agent_status/integration_config.rs:40`) | no (`rust/alera-cli/src/agent_status/normalize.rs:100`) | partial (`rust/alera-cli/src/agent_quota.rs:258`, `rust/alera-cli/src/agent_quota/plans.rs:140`; no transcripts in `rust/alera-cli/src/agent_quota/usage.rs:36`) | no (`lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50`) | no (`lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart:157`) |

## 4. Evidence Notes

### 4.1 Launch Dimension
- Codex launch arguments: `rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:79-134` parses `model`, `effort`, `planModeEffort`, `sandbox`, `approvalPolicy`, `webSearch`, and `bypassApprovalsAndSandbox`. Dart mirrors this in `lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart:327-349`.
- Claude launch arguments: `rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:138-200` validates optional `ccsProfile` switcher (substituting `ccs` binary), `--model`, `--effort`, `--agent`, `--permission-mode`, and `--allow-dangerously-skip-permissions`. Dart mirror is at `lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart:350-360`.
- Startup prompt delivery strategies: `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:19-38` defines `AgentStartupPrompt`. Five agents (`codex`: line 48, `claude`: line 55, `cursor`: line 69, `grok`: line 112, `devin`: line 119) use `PositionalAfterTerminator` (`--` terminator before positional prompt). Four agents (`copilot`: line 62 with `--interactive`, `agy`: line 76 with `--prompt-interactive`, `opencode`: line 83 with `--prompt`, `opencode2`: line 91 with `--prompt`) use `LongOption`. `pi` (line 98) uses bare `Positional` because pi CLI rejects `--`.
- Amp stdin script launcher: `rust/alera-cli/src/agent_prompt_stdin_script.rs:37-68` writes `agent-prompt-<session>.cmd` (or `.sh`) and a plaintext prompt file, feeding stdin via script redirection because amp CLI has no initial prompt option and drops interactive TUI mode if stdout is redirected.
- fx startup ready gating: `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:126` declares `AgentStartupPrompt::TerminalAfterReady`. The host does not type an initial prompt at launch; instead it waits for Herdr to emit an `Idle` event before pasting.

### 4.2 Hook Dimension
- Codex runtime home isolation: `rust/alera-cli/src/agent_status/integration_config_codex.rs:16-87` builds `agent-runtime-homes/codex/home`, copies `auth.json`, symlinks auxiliary directories, writes `hooks.json`, and recalculates SHA-256 trust hashes in `config.toml` via `rust/alera-cli/src/agent_status/integration_config_codex_trust.rs:1-40`. Dart runtime home counterpart is `lib/src/features/agent_status/infra/codex_runtime_home_service.dart:56-87`.
- Claude runtime home isolation: `rust/alera-cli/src/agent_status/integration_config.rs:135-169` builds `agent-runtime-homes/claude/home` with merged `settings.json` hooks, while cleaning user-level hooks (`integration_config_user_hooks.rs:1-40`) and CCS hooks (`integration_config_ccs.rs:1-35`). Dart counterpart is `lib/src/features/agent_status/infra/claude_runtime_home_service.dart:58-100`.
- Cursor per-session overlay: `rust/alera-cli/src/agent_status/integration_config_cursor_overlay.rs:39-105` creates `.cursor-plugin/plugin.json` and a `cursor-agent` shell wrapper under a session-specific directory, passing `--plugin-dir` without touching `~/.cursor/hooks.json`.
- Plugin-backed status agents: `rust/alera-cli/src/agent_status/integration_plugins.rs:1-59` installs standalone scripts: `plugins/alera-agent-status.js` for opencode (line 3), `plugins/alera-agent-status-v2.js` for opencode2 (line 11), `extensions/alera-agent-status.ts` for pi (line 20), and `plugins/alera-agent-status.ts` for amp (line 28). Dart artifacts are defined in `lib/src/features/agent_status/infra/managed_hooks/opencode_managed_agent_hook.dart`, `opencode2_managed_agent_hook.dart`, `pi_managed_agent_hook.dart`, and `amp_managed_agent_hook.dart`.
- Antigravity hooks bundle: `rust/alera-cli/src/agent_status/integration_config.rs:284-313` writes an `alera-status` block inside `~/.gemini/config/hooks.json`. It registers `PreInvocation`, `PostInvocation`, and `Stop` as lifecycle command hooks, and `PostToolUse` as a tool hook with matcher `*`. `PreToolUse` is intentionally excluded to avoid blocking tool execution on interactive decisions.
- Devin hooks installation: `rust/alera-cli/src/agent_status/integration_config.rs` writes to Devin's `config.json`. On the Dart side, `lib/src/features/agent_status/infra/managed_hooks/devin_managed_agent_hook.dart` owns the Devin descriptor and its Windows Git-Bash-to-`cmd.exe` execution strategy; the shared installer does not branch on Devin.
- fx socket integration: fx does not use hook configuration files or wrappers (`lib/src/features/agent_status/infra/managed_agent_hook_descriptors.dart:34-42`). It connects to `fx-herdr.sock` via `rust/alera-cli/src/agent_status/fx_herdr_receiver.rs:14-22`.
- Loopback hook receivers: Rust sidecar listens at `POST /hook/{agent}` with an authentication token (`rust/alera-cli/src/agent_status/hook_receiver.rs:59-62`). Rust desktop library FRB bridge exposes static routes for 11 agents in `rust/src/api/agent_hooks.rs:104-114`.

### 4.3 Status Dimension
- Identity resolution: Both `rust/alera-cli/src/agent_status/identity.rs` and `lib/src/features/agent_status/application/agent_status_identity_resolver.dart` implement the same identity resolution contract. Dart no longer embeds the Claude exception in the shared resolver: the incoming per-agent adapter decides whether it may take over an active foreign identity, while the resolver applies the generic stale/ownership policy.
- Normalizer state mapping: `rust/alera-cli/src/agent_status/normalize.rs` maps runtime events on the Rust side. Dart routes through the string-keyed registry in `lib/src/features/agent_status/infra/normalizers/agent_hook_adapter.dart`; each `normalizers/<agent>_agent_hook_normalizer.dart` owns that agent's canonical state/event semantics, while `agent_hook_event_normalizer.dart` only runs the shared pipeline.
- Codex transcript watcher: `lib/src/features/agent_status/infra/codex_transcript_status_watcher.dart:12-81` and `codex_transcript_watch.dart:1-290` actively poll `~/.codex/sessions/` JSONL files. This watcher handles prompt completion and terminal exit cases where Codex terminates without firing an explicit `Stop` hook. No other agent currently has a dedicated status transcript watcher.
- fx platform disparity: `rust/alera-cli/src/agent_status/fx_herdr_receiver.rs:72-78` stubs out `start_fx_herdr_receiver` on non-Unix platforms (`#[cfg(not(unix))]`), rendering fx status reception inactive on Windows.

### 4.4 Usage Dimension
- Provider quota polling: `rust/alera-cli/src/agent_quota.rs:245-323` polls quota snapshots across 10 enabled providers: `claude` (line 270), `codex` (line 279), `kimi` (line 286), `grok` (line 291), `cursor` (line 294), `antigravity` (line 297), `minimax` (line 304), `zai` (line 309), `devin` (line 314), and `opencode` (line 317).
- Antigravity headless TUI quota: `rust/alera-cli/src/agent_quota/tui.rs:1-60` scrapes quota by launching `agy /usage` inside a headless ConPTY session, parsing terminal cell output instead of querying an HTTP API.
- Transcript usage and cost aggregation: `rust/alera-cli/src/agent_quota/usage.rs:36-50` and `lib/src/features/agent_usage/domain/agent_usage.dart:1` restrict transcript usage processing to three providers: `claude`, `codex`, and `grok`. Line parsers reside in `rust/alera-cli/src/agent_quota/usage/transcripts.rs` and `grok_transcripts.rs`.
- Codex rate-limit reset store: `rust/alera-cli/src/agent_quota/codex_reset_store.rs:1-80` persists and checks redeemable reset credits, mapped to `CodexResetCredits` in `lib/src/features/agent_quota/domain/agent_quota.dart:14-33`.

### 4.5 Restart Dimension
- Transparent PTY restart: `lib/src/features/workbench/application/workbench_tab_layout_owner_opening.dart:50-66` stores `initialCommand` on `WorkspaceTabRecord` for all 12 `AgentType` members, ensuring transparent PTY regeneration across sidecar restarts without losing agent context.
- Terminal restart protocol: `rust/alera-cli/src/terminal_host/server/terminal_session_requests.rs:120-165` implements the `terminal.restart` RPC request (`terminalRestartV1` protocol capability).
- Agent-specific resume flags: `fx` is the only agent with a designated managed launch resume flag (`resumeLast: true` mapping to `--continue` in `rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:423` and `lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart:418`).
- Session reset hooks: `rust/alera-cli/src/agent_status/normalize.rs:32-34` identifies Grok's `SessionStart` as a session reset event (`hook_event_resets_session`), which triggers presence entry clearing in `rust/alera-cli/src/terminal_host/server/agent_hook_events.rs:42-59`.
- Session close hooks: `rust/alera-cli/src/agent_status/normalize.rs:17-30` detects explicit session shutdown events for six agents (`copilot`, `cursor`, `pi`, `grok`, `devin`, `fx`).

### 4.6 Model Override Dimension
- Profile model support predicate: `lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart:157-159` declares that all adapters support model overrides except `amp` and `fx`.
- opencode2 launch discrepancy: While `opencode2` accepts `model` in profile JSON configuration for schema symmetry, `rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:328-334` explicitly strips model arguments during interactive launch command generation, passing only `--auto`.
- Devin model override: `rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:67` and `lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart:414` support `--model <name>` for Devin. However, Devin is excluded from AI Assist operational models (`lib/src/features/ai_assist/domain/ai_assist_settings.dart:30-42`).

## 5. Adding a New Agent Today

Adding a new agent into Alera requires updating at least 18 separate files across Rust, Dart, and asset directories. Below is the concrete inventory of touch points:

### 5.1 Rust CLI Sidecar Touch Points
1. `rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs`: Append an `AgentAdapter` entry to `AGENT_ADAPTERS` defining `agent_type`, `default_command`, `force_submit`, `interrupt_bytes`, and `startup_prompt` (`AgentStartupPrompt`).
2. `rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs`: Add a match arm in `build_managed_agent_launch` and implement `build_<agent>` to validate and map profile settings to CLI arguments.
3. `rust/alera-cli/src/agent_status/hook_receiver.rs`: Add the agent identifier to the `SUPPORTED_AGENTS` constant array.
4. `rust/alera-cli/src/agent_status/normalize.rs`: Add a match arm to `normalize_state`, and optionally update `hook_event_closes_session` and prompt/message extraction helpers.
5. `rust/alera-cli/src/agent_status/integration_config.rs`: Add conditional hook installation in `prepare_enabled_integrations` and implement `install_<agent>` or add a plugin in `integration_plugins.rs`.
6. `rust/alera-core/src/runtime/settings_models.rs`: Add the agent boolean field and update `is_enabled` and `set_enabled` in `RuntimeAgentStatusHookSettings`.
7. `rust/alera-cli/src/terminal_host/server/agent_hook_events.rs`: If the agent requires custom event preprocessing or session ID reconciliation (like fx), add handling in `handle_agent_hook_event`.

### 5.2 Rust Native Bridge Touch Points (Desktop Library)
8. `rust/src/api/agent_hooks.rs`: Register `.route("/hook/<agent>", post(handle_<agent>))` in `start_agent_hook_receiver` and add the handler function `handle_<agent>`.

### 5.3 Rust Quota and Usage Touch Points (If Quota Supported)
9. `rust/alera-core/src/runtime/agent_quota_settings_models.rs`: Add the agent identifier to `SUPPORTED` in `RuntimeAgentQuotaSettings::normalized`.
10. `rust/alera-cli/src/agent_quota.rs`: Add a match branch in `fetch_agent_quotas`, create `agent_quota/<agent>.rs`, and extend `EnvironmentNames` if custom API key variables are needed.
11. `rust/alera-cli/src/agent_quota/usage.rs`: If JSONL transcript aggregation is supported, add the agent to `UsageProvider` and implement a line parser in `usage/transcripts.rs`.

### 5.4 Dart Feature Layer Touch Points
12. `lib/src/features/agent_status/domain/agent_status.dart`: Add an enum value to `AgentType` and a display label arm to `agentDisplayName`.
13. `lib/src/features/agent_profiles/domain/agent_profile_adapters.dart`: Add the agent to `spawnableAgentProfileAdapters` and `agentProfileDefaultCommands`.
14. `lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart`: Update `agentProfileSupportsModel`, `agentProfileSupportsPersona`, `managedAgentRiskScore`, `managedAgentRiskMarkers`, `managedAgentRiskWarning`, and `managedAgentCommandPreview`.
15. `lib/src/features/agent_status/infra/managed_hooks/<agent>_managed_agent_hook.dart`: Add the agent's managed-hook adapter there, including runtime-only status, JSON descriptor/plugin artifact, Windows execution strategy, custom script/wrapper behavior, and root config policy as needed. Register only the adapter key in `_managedAgentHookAdapters`; do not add an agent branch to the shared installer/descriptors/scripts.
16. `lib/src/features/agent_status/infra/normalizers/<agent>_agent_hook_normalizer.dart`: Add the agent's status adapter there, including event-name/state mapping, lifecycle/new-turn/session-close/reset, interruption, prompt/tool/message extraction, and takeover policy as needed. Register only the adapter key in `_agentHookAdapters`; do not add an agent branch to the shared normalizer/controller/helpers.
17. `lib/src/features/settings/domain/agent_quota_settings.dart`: If quota is supported, add an enum value to `AgentQuotaProviderId` and label in `AgentQuotaProviderIdLabel`.
18. `lib/src/features/agent_usage/domain/agent_usage.dart`: If usage aggregation is supported, add the provider to `AgentUsageProvider`.

### 5.5 Dart UI and Asset Touch Points
19. `lib/src/features/agent_status/presentation/agent_identity_icon.dart`: Add a case in `_agentAsset` mapping the `AgentType` to an SVG or raster asset in `assets/agents/`.
20. `lib/src/features/agent_quota/presentation/agent_quota_provider_icon.dart`: Wire the quota provider ID mapping to `AgentType` or direct asset fallback.
21. `lib/src/features/settings/presentation/panes/agent_profile_managed_editor.dart`: Add editor controls and fields for agent-specific configuration parameters.

## 6. Adapter Seam Status And Next Convergence

The Dart hook/status seam is now implemented: every `AgentType` has a dedicated status adapter and managed-hook adapter, while the shared normalization, lifecycle, identity, installer, descriptor, and script layers are policy-agnostic. `tool/quality/agent_extension_guard.dart` requires both per-agent files and rejects explicit `AgentType.<agent>` policy in the shared hook layers. The remaining architectural problem is cross-layer duplication between the Dart adapters, the Rust runtime status/integration code, launch/profile metadata, quota/usage, and UI descriptors.

### 6.1 Unified Agent Descriptor Model
Replace hardcoded pattern matches with a unified metadata record per agent. In Rust, expand `AgentAdapter` (`rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs`) into a comprehensive descriptor:

```text
AgentDescriptor {
    id: &'static str,
    aliases: &'static [&'static str],
    display_name: &'static str,
    default_command: &'static str,
    startup_prompt: AgentStartupPrompt,
    hook_strategy: HookStrategy,
    status_strategy: StatusStrategy,
    quota_strategy: Option<QuotaStrategy>,
    usage_strategy: Option<UsageStrategy>,
    model_override: ModelOverridePolicy,
    risk_evaluation: RiskEvaluationPolicy,
}
```

### 6.2 Specific Seams to Converge
1. Converge Hook and Artifact Installation Across Languages: Dart's managed-hook installer now executes generic adapter metadata rather than agent-specific dispatch. Keep that invariant. The next convergence step is to align the Rust runtime-home/overlay/config/plugin/socket strategies with the same capability vocabulary or protocol schema instead of rebuilding a second central switch in Dart.
2. Bridge Registry to Dart over FRB: Instead of maintaining parallel `AgentType`, `AgentQuotaProviderId`, and `AiAssistAgent` enums in Dart with separate switches for commands, icons, labels, and risk scores, expose the Rust descriptor registry to Dart via Flutter Rust Bridge. Dart consumes descriptors dynamically, eliminating duplicate launch argument builders (`managedAgentCommandPreview` in Dart currently duplicates `build_managed_agent_launch` in Rust).
3. Reconcile Naming Inconsistencies: Canonicalize `agy` versus `antigravity` under a single identifier (`agy` or `antigravity`) with explicit aliases, removing translation bridges such as `AgentQuotaProviderId.antigravity => AgentType.agy` in `agent_quota_provider_icon.dart`.
4. Preserve Per-Agent Normalization Ownership: Dart now has a unified pipeline plus one adapter per agent. Simple agents may move repetitive event maps into local tables, but non-standard payload/lifecycle rules stay inside that agent's adapter. Do not replace this with a new central `switch (AgentType)`; cross-language convergence should share fixtures/schema while preserving per-agent customization seams.

## 7. Known Gaps

1. Identifier Divergence (`agy` vs `antigravity`): Antigravity uses `agy` across CLI binary naming, launch adapters, hook endpoints, and presence normalization, but uses `antigravity` in quota settings models (`rust/alera-core/src/runtime/agent_quota_settings_models.rs:43`) and Dart quota settings (`lib/src/features/settings/domain/agent_quota_settings.dart:10`). This necessitates an explicit mapping shim in `agent_quota_provider_icon.dart:22`.
2. Registry Segmentation (Quota-Only vs Spawn-Only): Three providers (`kimi`, `minimax`, `zai`) exist exclusively in the quota polling registry and cannot be launched as agents, tracked via hooks, or inspected for presence. Conversely, five spawnable agents (`copilot`, `pi`, `amp`, `fx`, `opencode2`) possess complete launch and hook infrastructure but have zero quota polling integration.
3. Windows Incompatibility for fx Status: The fx agent communicates status exclusively through a Unix domain socket (`fx-herdr.sock`). `rust/alera-cli/src/agent_status/fx_herdr_receiver.rs:72-78` disables the receiver on non-Unix platforms, making fx status reporting inoperable on Windows desktop builds.
4. Launch Argument Duplication between Rust and Dart: Managed launch arguments are constructed twice: once in Rust (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:29-63`) and once in Dart (`lib/src/features/agent_profiles/domain/managed_agent_profile_options.dart:300-426`). This creates a synchronization hazard where command previews shown in the UI can drift from the actual arguments passed by the terminal host.
5. Incomplete Model Override Support: Amp and fx completely lack model selection support in both launch builders and profile editors. OpenCode 2 accepts a model field in its profile configuration for schema symmetry but silently suppresses `--model` during interactive launch generation (`rust/alera-cli/src/terminal_host/orchestration/managed_agent_launch.rs:328-334`).
6. Transcript Watcher Exclusivity: Live transcript watching is implemented solely for Codex (`lib/src/features/agent_status/infra/codex_transcript_status_watcher.dart`). Other agents rely strictly on loopback HTTP hook events, which can leave presence stale if a process crashes or exits without triggering an explicit completion event.
7. Usage Transcript Aggregation Coverage: Cost and token aggregation from session JSONL files is limited to Claude, Codex, and Grok (`rust/alera-cli/src/agent_quota/usage.rs:36-40`). The other 12 agents have no transcript-based cost calculation.
