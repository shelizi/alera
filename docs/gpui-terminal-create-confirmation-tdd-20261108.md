# Alera GPUI Terminal create confirmation — TDD closure (2026-11-08)

Base: `feat/gpui-desktop-20261107` at `4a4dbc98d7b2aa74bf47d0b6a4e5723226445848`.

## RED → GREEN

The initial failing test for `TerminalCreateFlow` deliberately failed to compile because the bounded creation-confirmation state machine, timeout and retry transitions did not exist. The pure Rust implementation and three initial model tests then went GREEN. Further cases cover failed RPC recovery, stale failure response, and no automatic attach after timeout without an explicit retry.

The controller has **Idle**, **Creating**, **Awaiting Host confirmation** and **Timed out** states. A create operation gets a unique local request ID. Repeated clicks cannot create multiple Host PTYs while a prior operation is pending, and late callbacks after cancel/replacement are ignored. `finish_request` only accepts a response for the current request ID, and `confirm_sessions` only completes if the authenticated Host list confirms matching handle, workspace ID and tab ID.

The single existing GPUI 1.5-second polling timer drives the state. While awaiting confirmation, it relists Host sessions every two ticks (3 seconds); after 20 ticks (approximately 30 seconds) it reports timeout and stops auto-attach. **Retry confirmation** relists existing sessions only; it never calls `terminal.create` again. **Cancel waiting** drops local UI state; it cannot guarantee cancellation of an already-submitted RPC or kill a Host-owned PTY. Users can inspect any orphaned session via **Attach existing**.

The project-switch path cancels confirmation when leaving the workspace. An actively in-flight create still blocks workspace switching until the user cancels waiting, preventing an unacknowledged active request from silently moving across projects. All Host I/O remains in background requests with workspace-generation and request-ID guards; no new timer, Host lifecycle bypass or new dependency was added.

## Validation gates

- TDD focused GREEN: 3 initial TerminalCreateFlow model cases.
- Windows `cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline` passed.
- Final full headless `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`: **163 passed, 0 failed, 0 ignored**, including both final edge cases.
- Complete `rustfmt --check` and `git diff --check` before commit. The only known compiler notice is an existing third-party future-incompatibility warning for `proc-macro-error2 v2.0.1`.

No actual GUI interaction, live PTY creation, Host mutations, remote push, tag, release or production-main merge was performed. Runtime Host PTY ownership remains unchanged.
