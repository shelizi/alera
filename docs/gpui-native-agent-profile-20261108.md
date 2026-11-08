# GPUI Native: first usable Agent Profile workflow (2026-11-08)

Scope: one user-selected existing Host Agent Profile, in the current already-
registered Runtime Host Workspace, launched from a Native text prompt and
attached as a Host-backed Agent tab after authenticated identity confirmation.

## Native user workflow

1. Open Native's **Agent Profiles** section in the Projects sidebar; click
   **Refresh** to discover profiles declared in the existing Runtime Host.
   No CLI or Native action starts the Host automatically.
2. Select an existing Agent Profile by its unique Host ID. Only name, adapter
   and managed-vs-command mode are displayed. Managed configuration and
   commands are never displayed or retained in Native's catalog projection.
3. Enter a task in the Agent prompt field and click **Launch Agent**. The
   Native CLI bridge first verifies that the current canonical project path
   has an exactly matching registered Host Workspace. The task goes through
   the subprocess stdin pipe (`--prompt-stdin`), never argv.
4. `agent-profile --no-start --json --runtime-dir ... launch` keeps the Host
   as the sole owner of readiness, profile credentials, permissions, MCP
   injection and the actual PTY. An unsupported or old CLI fails closed.
5. Native validates the returned `profileId`, `workspaceId`, and `tabId`.
   A fresh authenticated Host terminal list must contain a **matching Agent**
   session before Native switches to that Host tab; the user can also click
   **Attach existing**. If confirmation is pending, **Dismiss tracking**
   forgets only the local expectation, never terminates a Host session.

## Safety and compatibility

- The added CLI `agent-profile --no-start` is **opt-in**. Existing Classic
  Flutter CLI calls without that flag retain their prior behavior.
- No arbitrary `agent-profile create/update/remove`, shell command, Host
  control token or MCP grant is exposed through this GPUI UI.
- An in-flight Agent launch blocks Native workspace switching. Late replies
  are guarded by the workspace generation and request ID.
- A failed/timeout reply does **not** prove that the Host performed no work;
  users are instructed to inspect existing Host sessions before retrying.
- Profile MCP server injection is controlled by the existing Host/CLI profile
  configuration. This UI does **not** yet implement MCP tool navigation,
  application-internal MCP request execution, or custom per-profile settings.
- The user requested **main feature coverage before cosmetic parity**.
  The sidebar is a functional first pass; visual and Chinese IME GUI
  acceptance remains pending.

## Verification

```powershell
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked
cargo check --manifest-path desktop_gpui/Cargo.toml --locked
cargo test --manifest-path rust/Cargo.toml -p alera-cli agent_profile_cli_supports_explicit_no_start_without_changing_legacy_default --locked
```

Run the real GUI Agent smoke only with an intentionally registered disposable
Host project and disposable Agent Profile; never launch an existing personal
Agent session just to exercise this feature. Keep Native `supported=false`.
