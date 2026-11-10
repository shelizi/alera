# GPUI Native Terminal responsiveness: 2026-11-09

Scope: experimental `integrate/gpui-p0p1p2-20261108` worktree only. This is
not Flutter Classic, a production Host change, or an approval to release Native.

## Observed bottleneck

The Native GPUI Terminal output path was tied to a 1,500 ms housekeeping
timer and requested only 8 KiB from the Host per CLI call. With continuously
available data, that is approximately 5.3 KiB/s of UI-side draining. Empty
reads also invalidated the entire GPUI Workbench despite changing no terminal
content. Per-key terminal writes and each output read still launch an
authenticated `alera.exe terminal --no-start` child.

On the connected Windows host, eight consecutive, non-mutating calls against
the existing Runtime Host took:

* `terminal list`: 20, 19, 19, 19, 19, 31, 22, 22 ms.
* `terminal read`: 26, 30, 22, 22, 21, 21, 24, 24 ms.

These are CLI round-trip samples, not a keyboard-to-screen or frame-time
benchmark. They show that the intentional 1.5-second poll gap, rather than a
1.5-second Host RPC, dominated ordinary output latency.
An additional read with `--max-bytes 65536` against the existing Host returned
4,911 text characters in 37 ms, validating the larger request shape without
mutating a PTY or restarting the Host.

## Implemented in Native

* Read window: 64 KiB, still bounded and checked by `parse_terminal_chunk`.
* One lifecycle-bound 50 ms scheduling wakeup; *actual* read cadence is
  100 ms for three seconds after input/output, 350 ms while foreground idle,
  and 1,500 ms for background/hidden Terminal panes.
* Kept 1.5-second housekeeping (Host session reconciliation, resize debounce,
  autosave, Git fetch); it no longer controls foreground terminal output.
* Successful reads without new output do not trigger expensive Workbench
  repaint. Failed/stale reads retain the existing Host identity guards and
  backoff. PTY process ownership and authentication remain with the Host/CLI.
* No new Terminal tabs, PTYs, Host launches, direct RPC token access, or
  workspace mutations are introduced by this performance change.

## Verification and follow-up

Verification completed on the connected Windows host:

* `cargo check --manifest-path desktop_gpui/Cargo.toml --offline --locked`
  passed (exit 0; the GPUI executable source type-checks).
* `cargo test --no-default-features --lib ... terminal_poll_cadence_tests`
  passed 1/1, with 299 other tests filtered out.
* Changed Rust files passed isolated `rustfmt --check`, and `git diff --check`
  passed. A workspace-wide `cargo fmt --check` still reports an unrelated,
  preexisting formatting divergence in `file_search.rs`, which was preserved.
* A supplemental 300-test library suite was started but intentionally stopped
  after many passing cases because of unusually slow Windows test execution.
  This is **not** evidence that all 300 passed.

Windows GUI preview rebuild and real keyboard-to-screen latency measurement
remain outstanding; do not state perceived latency improvement as measured
before an actual GUI smoke. The already-running Native preview was not killed
or replaced, to preserve possible unsaved editor state; Host PTYs remain intact.

Remaining optimization opportunity: each input/output operation still starts
one CLI process. A future persistent authenticated CLI-side terminal relay
could remove this overhead without moving Host credentials, PTY ownership, or
session lifecycle into GPUI. It needs bounded IPC, failure recovery,
workspace/session identity validation, and real IME/TUI acceptance before
replacing the existing safe CLI route.
