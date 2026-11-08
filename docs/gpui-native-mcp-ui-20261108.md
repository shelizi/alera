# Alera Native (GPUI): Host-authorized MCP UI bridge (2026-11-08)

## Design and ownership

The existing Alera CLI/Runtime Host implements MCP stdio tool definitions,
per-Agent-session capabilities, destination Workspace validation, local App
registration and `uiControl.complete` receipts. **Native never reads or
reimplements a Host control token, mints a per-Agent capability, or marks a UI
operation applied without changing the visible GPUI state.**

The new internal `alera native-ui-bridge --runtime-dir ... --window-id native-*`
is an authenticated *local App*, not an external MCP server or general RPC
forwarder. It uses the Host's existing `uiControl.register` and
`uiControl.unregister` APIs, and forwards only window-addressed
`mcpUiCommand` events in a bounded JSON-lines pipe. Native sends a
request-specific `uiControl.complete` acknowledgment only after executing
the operation; the CLI refuses acknowledgments for request IDs it has not
received from the Host. The Host validates that the same App connection owns
the pending request. If the Native window closes the pipe is killed; Host
keeps terminal and Agent sessions alive and rejects its now-disconnected
pending UI commands.

Both Flutter Classic and GPUI Native may register their own window IDs. The
Host correctly requires the MCP caller to specify `windowId` when multiple
Workbench windows exist. Native's Projects sidebar offers a **Copy Native
MCP window ID** action. A copied window ID is a selector, **not** a token.

## Implemented actions in the Native window

| Host-authorized action | Native GPUI operation | Success contract |
| --- | --- | --- |
| `focus_workspace` | Select existing registered workspace path | Canonical root equals Host target |
| `reveal_file` | Expand Explorer ancestors and select file | Real in-workspace file is visible in tree |
| `open_file` | Reuse existing or open file-backed NativeEditor | Correct editor file active; optional line/caret navigation verified |
| `select_tab` | Focus existing authenticated Host Terminal tab | Selected Host handle maps to requested workspace+tab ID |
| `focus_agent` | Focus existing authenticated Host Agent tab | Requested workspace, tab ID, Agent type **and Agent session handle** match |
| `open_git_graph` | Show Native Git Graph panel | Panel is open; unsupported ref/filter modes are refused |
| `compare_branches` | Display a merge-base-to-head Git diff in the Native Git preview | Background comparison is read-only, scoped, and installed in the visible pane before receipt |
| `create_workspace` | Delegate to Host-owned `workspace add` managed Git worktree workflow, then switch Native | New active Host Workspace, Project ID, branch and canonical worktree path independently verified before receipt containing `createdWorkspaceId` |
| `get_setting` | Read one of the eight Host-allowlisted Native settings from the independent state store | Return the persisted typed scalar as `settingValue` |
| `update_setting` | Compare `expectedValue`, atomically persist a writable scalar, read it back and update the Native UI owner | Applied only with readback; stale expected values or invalid/corrupt data fail closed |
| `open_settings` | Show the actual GPUI Native Settings panel and selected section | Visible section and Native settings read must succeed |

Unsupported (external editors, `openMode=new`, and unsupported Git filters) return
`applied=false`, never a made-up success. Native workbench's existing dirty
buffer guards can refuse cross-workspace switching. All file targets are
canonicalized again and required to remain within the target workspace;
Host authorization alone is not sufficient to open a symlink outside it.

## Native Settings ownership and behavior

The Native preview never reads or writes Flutter Classic's SettingsController
or preferences. Its own versioned `native-settings-v1.json` file is kept in
Native state (also honoring the isolated `ALERA_GPUI_STATE_DIR` smoke-test
override). Missing settings use Native defaults; malformed, oversized, or
future-version JSON is rejected, **not silently replaced**. Writes perform a
local exclusive compare-and-set, an atomic replacement, and full readback
before confirming the Host result. The CLI sidecar forwards `settingValue`
only for its pending successful get/update command, while Host validates the
scalar type against its separate allowlist.

Five writable paths now have Native runtime behavior: Git auto-fetch toggle
and 1–60-minute interval; Terminal text size 8–48 px; Editor idle autosave
(only a file-backed buffer after 3 seconds without IME composition); Editor
Tab indentation width 1–16 spaces, applied to both editors. Three destructive
confirmation/archive controls remain MCP read-only. Native Settings are
also accessible from the top bar. Agent/Agent Profiles settings sections
are not implemented yet; their MCP navigation requests correctly return
`applied=false` rather than showing a placeholder and claiming success.
Native Git Auto Fetch defaults to enabled with a five-minute interval,
matching Classic's first-run behavior without sharing its preferences.

**Acceptance remains limited:** storage and bridge tests and successful
Native compilation cannot replace a real Agent→Host→Native GUI settings
get/update/open/receipt exercise. Keep `supported=false` pending the
existing full GUI/PTY/IME acceptance gates.

The Native `create_workspace` path **does not** manually call `git worktree add`:
it delegates Git setup, Project/Workspace registration, setup policies and
parent links to the existing Runtime Host managed creation API. The CLI
subprocess and Host readback are bounded, and Native blocks unrelated
worktree/switch operations during creation. If a user starts editing while
creation is running, Native refuses a destructive Workspace switch and
reports `not_visible` instead of `applied`. The Host-created Workspace remains
available; the caller must **inspect existing Host Workspaces before retrying**
to avoid duplicates. Creation does not auto-delete a Workspace or stop PTYs.

## Compatibility and operational boundaries

* Native uses the existing Host's `App` role but does not replace or close
  Flutter Classic. Ordinary CLI connections remain `clientKind=cli`.
* Native starts no Runtime Host; it attempts a background reconnect only
  while a Native window remains open.
* MCP agent tool discovery remains handled by `alera mcp serve` with existing
  Host-minted per-PTY capability. It is not redefined by GPUI.
* The new Native UI bridge requires the new Alera CLI sidecar, otherwise it
  disconnects safely without accepting or acknowledging commands.
* Native remains `supported=false` pending real Windows GUI keyboard, Zhuyin,
  two-editor and Host PTY acceptance. The MCP UI features have unit and
  isolated real-Host integration tests; there is no claim of full cross-app
  hands-on UI acceptance.

## Verification

```powershell
cargo test --manifest-path rust/Cargo.toml -p alera-cli native_ui_bridge --locked
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked
cargo build --manifest-path rust/Cargo.toml -p alera-cli --bin alera --locked
cargo build --manifest-path desktop_gpui/Cargo.toml --locked
$env:ALERA_GPUI_TEST_CLI='path\\to\\disposable-built-cli.exe'
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --test live_runtime_host -- --ignored
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --test native_mcp_settings_host --locked -- --ignored --nocapture
```

The integration test starts **its own temporary Host and workspace**, proves
Native App registration and disconnection, **builds a disposable Git repository,
then creates and reads back a real Host-managed linked worktree**, and never
needs the user's real terminal, Agent or production Host. GUI-level MCP tool
to Native receipt end-to-end remains a separate acceptance gate.

The independent `native_mcp_settings_host` integration test uses the same
branch-built CLI to launch a **real disposable Runtime Host and Host-owned
Agent PTY**, then calls actual MCP stdio inside the Agent's Host-injected
capability environment. The Native App relay is a real CLI subprocess;
an isolated headless consumer uses the actual `NativeSettingsStore` to
execute disk-backed operations and send typed receipts. On Windows
2026-11-08: **1/1 PASS**, including seven cases for read, update, readback,
stale CAS rejection, secret/range rejection, and explicit fail-closed
`open_settings` without a GUI; the test restores original settings.
**This is real Host/MCP/data-layer E2E, not Native GPUI GUI acceptance.**
