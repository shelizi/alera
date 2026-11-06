# Alera MCP on macOS and Linux

This is the **opt-in desktop MCP alpha**. Alera's existing Rust CLI implements
the MCP stdio protocol (now 22 tools, 11 read-only) on Windows, macOS
and Linux. An Agent launched from an Alera Managed Codex/Claude Profile can
receive its own transient MCP registration; the Host supplies a fresh, per-PTY scoped
capability through the child process environment, **not** through command-line
arguments or the user's global Agent configuration.

**2026-11-05 Windows smoke confirmation:** The native CLI at PR commit
`dc8b9864b` was rebuilt, then `mcp_cross_platform_smoke.py` passed with
`MCP_NATIVE_PLATFORM_SMOKE_PASS` (exit 0). The smoke asserts all 20 distinct
tools, including `alera.focus_workspace` and `alera.select_tab`, and verifies
Host-minted PTY capability inheritance, denial without a bearer, scoped reads,
and cleanup. This is Windows-only evidence; dedicated macOS/Linux MCP runners
remain pending authorized workflow approval.

## Project and Workspace control

Managed Agents can call `alera.get_context` to identify their Host-verified
Project and Workspace; `alera.list_projects` and `alera.list_workspaces`
list the Alera catalog, which includes non-Git workspaces and cannot be
reconstructed from `git worktree list` alone. `alera.focus_workspace`
continues to switch to an existing Workspace without relocating the caller
PTY. `alera.create_workspace` creates a new managed Git Workspace using the
same Workbench creation, setup and selection flow as the UI. The Project
defaults to the calling Agent's Project unless an explicit registered
`projectId` is supplied. Creation requires `sourceBranch` and
`newBranchName` and a real Host-issued Agent capability. The request is
complete only after the Alera App reports the new Workspace and the Host
verifies its durable Project membership and active status. A timeout is
ambiguous: list Workspaces before retrying to avoid duplicates.

The earlier 20-tool native smoke confirmation below is historical evidence
for the prior toolset, not verification of these two new tools.

## Platform support and acceptance

| Platform | Bundle location for MCP CLI | Evidence |
| --- | --- | --- |
| Windows x64 | `resources/alera/alera.exe` | Real Flutter and Codex/Claude model integration previously passed; portable native Host/PTY smoke passed locally |
| macOS Apple Silicon (arm64) | `Alera.app/Contents/Resources/alera/alera` | Platform packaging and sidecar discovery are wired; **native Mac CI and real GUI/Agent acceptance are pending** |
| Linux x64 | `/opt/alera/resources/alera/alera` (packaged app) | Linux CMake installs an executable sidecar; **native Linux CI and real GUI/Agent acceptance are pending** |

macOS Intel and Linux ARM64 desktop binaries are **not** included in this
platform enablement. The standalone runtime packager may have additional
architecture artifacts; these do not imply a supported desktop package.

## Opt in without modifying global Agent MCP configuration

By default `ALERA_MCP_PROFILE_INJECTION` is off. To test it, **close Alera
normally after saving work and finishing running Terminals**, then start the
desktop process from a shell with the flag. The desktop process must start
its own Runtime Host with this environment; merely setting the variable on
a machine with an already-running Host does not change the Host's behavior.

macOS, when the installed app is `/Applications/Alera.app`:

```sh
ALERA_MCP_PROFILE_INJECTION=1 "/Applications/Alera.app/Contents/MacOS/Alera"
```

Linux when installed from the native package:

```sh
ALERA_MCP_PROFILE_INJECTION=1 /opt/alera/alera
```

Opening the app through Finder, Dock or a desktop launcher normally does not
inherit your interactive shell variable. For this alpha, use the explicit
shell launch above. No system-wide `launchctl setenv`, security bypass or
Agent MCP configuration edits are required.

Choose a **Managed** Codex or Claude Agent Profile in Alera. Opaque Command
mode and unsupported Agent adapters never receive injected MCP arguments.
Claude may require normal folder trust and a per-tool permission prompt.
Approve only the intended trusted project/operation; Alera does not bypass
the provider's security prompts.

An MCP client manually started outside an Alera Agent Terminal can list
tool metadata, but cannot impersonate the Host-issued per-PTY capability
needed for Workbench UI, setting changes, Agent focus or messaging.
Cross-workspace writes and raw PTY input remain unavailable.

## Native tests, isolated from user data

Build the CLI for the **actual target OS**, then run:

```sh
cargo build --locked --manifest-path rust/Cargo.toml -p alera-cli --bin alera
cargo test --locked --manifest-path rust/Cargo.toml -p alera-cli --bin alera mcp
python3 tool/testing/mcp_cross_platform_smoke.py --cli rust/target/debug/alera
```

To check the packaged desktop sidecar instead, pass its full path to
`--cli`. The Python smoke suite creates a temporary Runtime Host and OS
PTY, exercises MCP initialization, tools, file/Agent inventory, output
cursors and scoped authorization denial. The smoke additionally creates a
**real Host-owned Agent PTY without running a model**, then checks from inside
that PTY that the Host-minted capability is usable, removing the token is
rejected locally by MCP, and an altered token is rejected by the Host.
No Flutter window is connected, so actual UI operations still fail closed.
The suite stops/deletes only its test Runtime. It never connects to Alera's
normal runtime, launches a model, opens user files or changes a global profile.

The complete native security and PTY probe, including the stronger in-Agent
capability tests, has passed against a **Windows** test Host. Native macOS
ARM64 and Linux x64 remain pending until their own CI runners execute.

## GitHub fork native desktop CI (2026-11-04)

The MCP feature branch was pushed to `shelizi/alera` at
`ab3fdbd4`. Existing, unchanged **Desktop Builds** was manually dispatched
as [run 41749708040](https://github.com/shelizi/alera/actions/runs/41749708040)
for macOS, Linux and Windows. This executes native desktop builds and
integration suites, **not** the pending MCP-specific Host/PTY Python smoke.

The initial Linux x64 **desktop build passed**, but its
`integration_test/alera_smoke_flow_test.dart` failed because the isolated
test `ProviderScope` omitted
`terminalSessionTitleSourceOverride()` even though the real app
composition root supplies it. The resulting Resource Manager
`ProviderException` also rendered an oversized Flutter error widget in
Agent Quota status-bar layout. The integration fixture now uses the same
title-source binding as the real app. The repaired Linux integration test
has not yet passed a native Linux runner; do not mark that job green based
on this source change. The initial macOS/Windows jobs are independent of
this Linux fixture result and require their own final checks.

Only an authorized workflow reviewer can commit the protected
`.github/workflows/mcp-cross-platform.yml` and
`.github/workflows/desktop-build.yml` changes. Do not bypass tool
protection to insert them into GitHub. The native MCP-specific CI
remains **unexecuted** until authorized workflow changes are accepted.

CI wiring:

- `.github/workflows/mcp-cross-platform.yml` builds native Rust binaries,
  runs MCP regression tests and native Host/PTY smoke on **macOS ARM64 and
  Linux x64** for matching PRs or on manual dispatch.
- `.github/workflows/desktop-build.yml` additionally verifies that the
  **packaged native desktop bundle** actually contains an executable MCP CLI,
  then runs the same isolated Host/PTY smoke on macOS, Linux and Windows.
- `test/unit/alera_cli_sidecar_test.dart` checks app-side sidecar discovery
  paths on macOS and Linux as well as Windows.

The two GitHub Actions workflow changes remain **uncommitted** in the
isolated MCP worktree: the connected Git tool refuses staging protected
`.github/workflows/` paths. Do not bypass that protection through direct
`git add` or another transport. An authorized repository workflow reviewer
must stage/approve those changes before native CI can run for this branch.

These CI checks are defined but **do not count as passing until they have
actually run successfully on native runners**. Real model-initiated UI and
settings changes on the two Unix desktops, multi-window routing, and
cross-workspace authorization require separate acceptance.

No release, auto-install, push or merge is implied by this document.
