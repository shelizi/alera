# Alera Native (GPUI): Terminal Host bootstrap

Date: 2026-11-09. Branch: integrate/gpui-p0p1p2-20261108.
Classic desktop and its Release CLI are not modified.

## Root cause of blank Terminal

The debug Native executable originally had no sibling \`alera.exe\` CLI.
The deployed Classic CLI is older and lacks the \`terminal --no-start\`,
\`workspaces\`, \`create\`, and \`resize\` commands needed by GPUI.
Additionally, the user's default Runtime Host is initially offline and the
current Native worktree is not registered in its workspace inventory.
Native's old New Terminal flow required an already-running Host and a
registered workspace, so it could not create the first terminal.

## Fixed startup contract

- At startup, Native discovers a same-build \`alera.exe\` next to its own
  executable, or an explicitly configured absolute \`ALERA_CLI_PATH\`.
  Passive session listing, reading, input and viewport resize always use
  \`terminal --no-start\`. Opening a Native window never starts a Host.
- On explicit **+ > New Terminal** only, Native uses the existing CLI to
  start the Runtime Host when offline. The Host and the CLI retain all PTY
  ownership, authentication and session identity validation.
- It first tries to resolve the active checkout against registered Host
  workspaces. If absent, a strict CLI project lookup identifies its
  pre-existing main Git repository by canonical path/Git common directory.
  The CLI registers the **existing checkout** in Runtime inventory, with
  matching project/branch/path; no \`git worktree add\`, branch checkout, or
  mutation of the worktree itself occurs.
- The registration reply and a fresh authenticated Host workspace list must
  both match before Native may request one Host-owned Terminal. The resulting
  PTY is attached only after a fresh authenticated session list matches all
  three Host identities (handle, workspace ID, tab ID).
- Starting a persistent detached Host on Windows must not wait for stdout
  pipe EOF: the Host can inherit the pipe while the parent CLI has exited.
  The start command is therefore run with null stdio and a bounded process
  wait. Ordinary short CLI JSON queries retain bounded stdout decoding.
- New Terminal does not terminate/restart another application's Host; if
  compatible Host already exists, it reuses it. A workspace not belonging
  to any uniquely registered project is rejected safely.

## Repeatable Windows developer launch

Run \`tool/dev/run_alera_native_windows.ps1 -Offline\` from this worktree.
It builds \`rust/alera-cli\` and \`desktop_gpui\` in one dedicated Cargo target,
checks the CLI verbs, and launches Native with matching sibling executables.
The script can use \`-TargetDir\` for an existing compiled output directory.
Never substitute a Classic Release CLI with a different Host protocol.

## Verification

- Unit tests cover registered main checkout, duplicate/mismatched project
  refusal, and linked Git worktrees without creating a new checkout.
- Opt-in real isolated Windows test:
  \`ALERA_GPUI_TEST_CLI=<absolute-current-branch-alera.exe> cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --test terminal_bootstrap_host -- --ignored\`
  uses a temporary Runtime profile and Git repo, explicitly starts a
  persistent Host, registers the checkout, creates a real PTY, checks the
  listed Host identity, writes \`echo\` and reads its output; cleanup stops
  **only this temporary Host**. It does not send requests to Classic's
  Runtime profile or touch user's existing terminal sessions.

## Live Windows Native acceptance

The rebuilt Native + matched CLI sidecar were launched together on
Windows. The visible **+ > New Terminal** GPUI Kit dropdown triggered
the explicit Host bootstrap; the existing integration worktree was
registered and an authenticated PTY started. The new mixed Terminal tab
displayed a live Windows command prompt and cursor. Entering
`echo ALERA_NATIVE_UI_E2E_OK` through Native's terminal command bar and
clicking Run produced the marker and next prompt **in the GPUI Terminal
surface**. The authenticated CLI independently reported one running
Host, one running PTY, and the exact Native checkout registered.
Classic was not stopped or modified.

Still pending before supported=true: direct raw-keyboard/IME, full-screen
TUI, resize and split-pane Host session interactive acceptance, Agent
sessions, and Classic's richer terminal UX. No production merge/push/
release in this slice.
