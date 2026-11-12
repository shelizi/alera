# GPUI New Agent detection/dropdown parity audit (2026-11-12)

## Classic reference
- `lib/src/features/workbench/presentation/workspace_workbench_new_tab_button.dart` shows `installedAgentClisProvider` (machine-local binary discovery), not just Host Agent Profiles.
- `lib/src/features/agent_profiles/application/local_agent_detection.dart` probes the canonical descriptor order, Windows PATH/PATHEXT, Classic's explicit executable overrides, and a bounded `--version` health check for AGY only.
- Classic's selection directly creates a Terminal with a configured initial local Agent launch. This is **not** the same as Native's Host profile launch mechanism.

## Defects found in Native before this patch
1. The `+` menu projected only Host Agent Profiles, so Host-offline or no-profile installations showed no Agents even when local binaries were installed.
2. It queried Host Agent Profiles only after the menu had opened, so a first popup could show a stale snapshot.
3. No independent machine-local CLI inventory was maintained.

## Implemented
- Read-only local binary detection in `desktop_gpui/src/local_agent_detection.rs`, using Rust Core `AGENT_DESCRIPTORS` for canonical names/order.
- Windows PATH/PATHEXT and quoted PATH entries, custom override argument with invalid-override fail-closed behavior, and windowless/timeout-bounded AGY `--version`.
- Startup background inventory and explicit dropdown rescan. The inventory is machine-wide and independent of the Host Profile request state. Both primary and secondary `+` menus show an **Installed Agent CLIs** group, with Host Profiles remaining in their own group.
- If a selected installed Agent has a matching current Host Agent Profile, the existing prompt/composer and guarded launch path is used. Without a matching profile, UI explicitly states that a Host Agent Profile is needed; there is no implicit profile mutation, PTY creation or arbitrary binary execution.

## TDD and real Windows verification
- RED: new detector tests initially failed to compile without the discovery implementation.
- GREEN: six new library tests covering Host-independent results, canonical order, Windows PATHEXT, quoted PATH, explicit override behavior, AGY health and safe Host profile mapping.
- Full Rust Library: **500/500 PASS**. Native binary tests: **31/31 PASS**. `cargo fmt --all --check`, `cargo check --offline --locked --bin alera-gpui-desktop`, and `git diff --check`: PASS. Workspace-local Windows build: PASS.
- Real Windows 1600x1024, with `ALERA_CLI_PATH` pointing to an intentionally missing binary and an isolated Runtime/State directory: BOTH primary/secondary dropdowns listed **Codex, Claude Code, GitHub Copilot, Antigravity, Grok Build, Devin** from the local machine. Selecting Claude with no Host profile opened the Runtime UI and displayed the explicit no-profile explanation, without starting a Host. The isolated GUI was closed after the check.

## Still open for full Classic parity
- **Direct launch semantics**: Classic starts an installed Agent in a local terminal without requiring a saved Host Agent Profile. Native does not yet have an equivalent Host-owned Agent type launch command and must not fake it via a shell command.
- **Custom paths at the UI level**: the detector supports override-path input, but Native settings do not yet expose Classic's separate `agents.agentExecutablePaths` overrides. Native must not silently read or overwrite Flutter's private preferences.
- POSIX/macOS login-shell PATH hydration, icon alignment, and live Host/PTY launching/confirmation are not yet accepted.
- Do not mark direct Agent launch parity or Native `supported=true`. Do not merge `main`, Push, deploy, or delete the nested Git fixture without the remaining gates.
