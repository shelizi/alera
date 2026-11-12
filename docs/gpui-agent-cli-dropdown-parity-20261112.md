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
- Selecting a detected local Agent now uses the existing Host-managed Terminal tab creation, **without requiring a saved Agent Profile**. It calls bounded `terminal --no-start create-agent` and authenticated `tab.upsert`, passing the concrete detected Agent executable (not shell text). For direct launches, GPUI now first validates the Host's persisted tab **Agent type, exact executable, no additional arguments and spawn flag** plus the Host tab/workspace/session identity. It then requires the exact **running** session, rejecting conflicting Hook Agent types while allowing a missing Hook status during initial PTY startup.
- Host Agent Profiles remain separate in the menu, with their task composer unchanged. Native Settings > Agent Profiles now has Agent executable path Save/Clear controls, atomic CAS/readback and inventory rescan; Classic's private settings are not modified.

## TDD and real Windows verification
- RED: new detector tests initially failed to compile without the discovery implementation.
- GREEN: six new library tests covering Host-independent results, canonical order, Windows PATHEXT, quoted PATH, explicit override behavior, AGY health and safe Host profile mapping.
- 2026-11-12 latest Native direct-launch/Hook-delay TDD: **505/505 Rust Library PASS**, **31/31 Native Binary PASS**; typed Host Agent reply corroboration and delayed-Hook PTY tests first failed as expected before implementation, then **4/4 targeted direct-Agent Library tests PASS**. GPUI Windows EXE built successfully from this exact isolated Worktree. `cargo fmt --all --check`, Native `cargo check --offline --locked --bin alera-gpui-desktop`, `git diff --check` PASS.
- Earlier 1600x1024 offline GUI inventory smoke (prior to direct-launch change): both primary/secondary dropdowns showed **Codex, Claude Code, GitHub Copilot, Antigravity, Grok Build and Devin**. A new offline GUI retest with the current direct-launch binary is **not yet accepted**: Windows Firewall displayed a separate prompt for the compiled Host CLI test executable on the interactive desktop. The modal was not dismissed, granted permission, or bypassed. No currently connected production Host/PTY was intentionally changed.

- Host CLI direct-Agent targeted tests: `direct_agent` 2/2 PASS; CLI `terminal_create_agent_requires_explicit_bounded_host_and_installed_agent_identity` 1/1 PASS; Host spawn-on-create rollback 1/1 PASS.
- Host CLI **full** suite: **1264 PASS, 4 FAIL, 8 ignored**. Individual reruns of the same four cases also failed: `agent_history_sync` (group matching, late message preservation, destination conflict) and `wire_fixture_tests::tab_upsert_round_trip_matches_shared_fixtures` (dynamic `updatedAt` differs from fixture timestamp). Both failing test source files have byte-identical Git object hashes in `main` and this integration branch, but a separate clean-main execution has not been performed; baseline status is therefore unconfirmed. Do **not** claim full Host CLI Gate PASS.

## Remaining acceptance before supported=true
- **Positive end-to-end Host/PTY launch** with the current GPUI build is **not yet verified** against an isolated live Runtime Host. The new tests verify requested Host tab payload and safe Hook-delay attachment separately. Neither simulated tests nor the earlier offline inventory smoke establish actual agent-terminal I/O, connection, reconnect, or disposal behavior.
- Native and Classic each have their own editable Agent path preferences; cross-edition synchronization is not provided.
- POSIX/macOS login-shell PATH hydration, icon parity, 150%/200% DPI and real Host/PTY reconnection remain untested.
- Do not mark Native `supported=true`. Do not merge `main`, Push, deploy, or delete the nested Git fixture without the remaining gates.
