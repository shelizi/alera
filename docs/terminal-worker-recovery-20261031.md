# Alera Rust Terminal Worker fault isolation and recovery (2026-10-31)

## Status and safety boundary

- Development branch: `feat/terminal-worker-isolation-20261031`, based on `main` after the bounded ConPTY output / worker-thread failure fix.
- Worker isolation is **opt-in** with `ALERA_TERMINAL_WORKER_PROCESS=1`; unchanged legacy PTY Host behavior is the default.
- No push, release, feature-gate enable, or merge to `main` is authorized by this document. T2 `supported=false` remains subject to its existing Gate A/B/C rules.
- Rust Worker transport is implemented for Windows, Linux and macOS behind the same opt-in flag; Windows has native tests, Unix/macOS still require their own native CI acceptance.

## Architecture

1. Each opted-in Terminal runs its own Rust `alera __alera-terminal-pty-worker` process that owns the native PTY and Shell. On Windows it also owns a KILL_ON_JOB_CLOSE Job Object; on Unix it uses sealed process identity and PTY hangup semantics. The Host owns tab/workspace/session metadata, history, client leases, and orchestration.
2. Worker starts a loopback TCP listener. A reconnect manifest is atomically published to the runtime's `pty-workers/<sha256(session-id)>.json`. It contains a random token, session/tab IDs, original working directory, Worker PID/start-time, Shell PID, and loopback port. It does **not** contain the Shell environment or API credentials.
3. The Host authenticates with the token, checks the recorded Worker PID and start time, and retrieves raw PTY output with an absolute stream cursor. A disconnected Worker buffers a bounded tail (up to 4 MiB) until a replacement Host reconnects. Input is fenced to the newest authenticated connection.
4. On Host restart, the `createOrAttach` path tries to reclaim the live Worker **before creating another shell**. Explicit `restartTerminal` still always mints a new Shell. T2 ownership verification is performed before attach and an original transferred working directory is preserved on recovery.
5. Host-to-Worker startup handshakes time out after 15 seconds; failed startup kills and reaps the child. Windows workspace shutdown waits for Worker handles and Job trees; Unix uses sealed Worker PID and its existing shell-descendant termination. A Worker failure should not affect other Terminal processes.

## Verified checks

- `cargo check --locked --manifest-path rust/Cargo.toml -p alera-cli --bin alera` on Windows.
- `cargo build --locked --manifest-path rust/Cargo.toml -p alera-cli --bin alera` on Windows.
- `python tool/debug/test_terminal_worker_isolation.py D:\\c\\t\\debug\\alera.exe`: two Workers, one forcibly terminated and its Shell Job exited, the other Worker continued; incorrect reconnection tokens were rejected; the second Worker survived Host-socket loss and reauthenticated on a new connection with the original PID/Shell and offline output replay. Test creates an isolated runtime under its worktree and does not touch live user terminals.
- `python tool/debug/test_runtime_host_worker_recovery.py D:\\c\\t\\debug\\alera.exe`: complete isolated Windows RuntimeHost started, project/workspace/tab created, Terminal attached, **Host process forcibly killed**, replacement RuntimeHost launched with the same private runtime, original Worker PID and Shell PID preserved, and Terminal accepted/echoed input after reconnection. This passed with the Worker setting enabled only for the test; its test Worker was confirmed gone at cleanup.
- `python tool/debug/test_terminal_worker_memory_limit.py D:\\c\\t\\debug\\alera.exe`: in an isolated test Runtime, a 384 MiB Windows Job limit on one Shell rejected one child 512 MiB `VirtualAlloc(MEM_COMMIT|MEM_RESERVE)` request. The limited Shell, its Worker, and an independent uncapped sibling Terminal all stayed responsive. No machine-wide OOM and no physical page-touch stress was performed.
- `python tool/debug/test_terminal_worker_output_pressure.py D:\\c\\t\\debug\\alera.exe`: the disconnected Terminal generated 256 KiB of output with a 64 KiB maximum replay tail; reattachment returned an advanced absolute replay cursor (233275 in the successful run), preserved the final output marker and kept both Workers responsive.
- `python tool/debug/test_terminal_worker_offline_backlog.py D:\\c\\t\\debug\\alera.exe`: while Host was disconnected, one Worker emitted 6 MiB with a 64 KiB offline ring; new connection replayed only 65573 bytes (including protocol/control bytes), with an advanced cursor at 7091879, preserving the original Worker/Shell and accepting more input.
- Follow-up 2026-11-01: Worker competing-host direct IPC test passed 12 handovers; Flutter PTY and T2 roundtrip targeted tests passed 28/28. A full RuntimeHost crash/reconnect rerun preserved original Worker/Shell and successfully ran terminal input; its cleanup initially hit WinError 32 because the isolated runtime lived within a Dropbox-synced worktree. The test now uses the OS temporary directory, and the full Host-crash/recovery scenario re-ran **exit 0**, preserving original Worker/Shell and terminal input. This is backend E2E, not graphical UI acceptance.
- `python tool/debug/test_terminal_worker_competing_hosts.py D:\\c\\t\\debug\\alera.exe`: twelve authenticated replacement Host sockets took over a single live Worker/Shell, and stale TCP owners failed to execute distinct filesystem marker commands; Worker/Shell identities unchanged. This validates local IPC owner fencing, not graphical multi-window focus or UI leases.
- `python tool/debug/test_terminal_worker_resource_soak.py D:\\c\\t\\debug\\alera.exe` (optional test dependency: Python `psutil`): 60.4-second Windows Worker CPU/RSS and reconnect soak **passed**, with 2 isolated PTYs, 86 paired, shell-confirmed commands, 9 authenticated Host-socket reconnections. Worker RSS initial/peak/final in KiB: `(13220,13360,13344)` and `(13260,13308,13292)`; cumulative Worker CPU was `0.06s` and `0.03s`. Earlier rapid-loop attempt was invalid because PTY input echo alone is not command completion; the revised test waits for a fresh file marker written by the Shell for each command. 60 seconds is not evidence of multi-hour stability.
- `cargo test --locked --manifest-path rust/Cargo.toml -p alera-cli --bin alera configured_session_memory_budget_is_enforced_in_windows_job_flags`: queried Windows Job Object and verified the 384 MiB per-session memory limit flag and value.
- Windows Session regression tests: `cargo test --locked --manifest-path rust/Cargo.toml -p alera-cli --bin alera terminal_host::session::` — 40/40 passed after cross-platform Rust Worker changes.
- T2 transfer contract regression: `cargo test --locked --manifest-path rust/Cargo.toml -p alera-cli --bin alera terminal_tab_transfer` — 4 passed.
- Rustfmt direct checks and `git diff --check`.

## Cross-platform acceptance status (2026-11-01)

- The same Rust PTY Worker and authenticated reconnect transport are now wired into the Windows, macOS and Linux Terminal Session paths. Unix uses private 0700/0600 token files and sealed Worker PIDs rather than a Windows Job Object.
- Unix shell launch now applies the workspace working directory.
- Windows Rust Terminal Session regression passed 40/40 after the cross-platform changes. No regressions observed in that test scope.
- Native Linux Rust tests are available in existing `.github/workflows/pr.yml`, and macOS build checks in `.github/workflows/desktop-build.yml`. The added `tool/debug/test_terminal_worker_unix.py` must also run on both native operating systems to verify Worker crash isolation, sibling survival, original Shell PID reattachment, private token permissions and offline replay.
- Both native operating systems must pass their build/tests and the explicit Unix Worker smoke before enabling or merging. Cross-target `cargo check` cannot replace native fault injection. The protected CI workflow directory cannot be staged with the current Git tool permissions, so the new smoke is supplied as a separate script, not yet automatically wired to CI.
- Cross-target checks from Windows with Rust 1.98.0 were attempted. macOS ARM64 stopped at missing Apple-target zlib/sqlite C compiler/SDK; Linux x86_64 stopped at a missing GNU cross-compiler. These are environmental blockers, not passed native builds. Native Linux/macOS acceptance remains mandatory.

## Remaining release and acceptance checks

- Complete **graphical Alera UI acceptance** for actual Host restart, multi-window tab leases and prompt interaction on Windows. The new isolated RuntimeHost test verifies the server/PTY code path, but does not validate Flutter screen state and focus.
- Remaining load/edge coverage: full machine-wide OOM (which no application can absolutely contain), repeated Worker-native process aborts, sustained long-running CPU/memory monitoring, many rapid create/destroy cycles, Host graceful-shutdown versus crash semantics, and simultaneous competing Host attach attempts. The bounded child allocation-failure and offline-tail-overflow scenarios above have now been tested.
- Windows PTY worker reconnect registry and token files now receive a protected DACL granting only the current user, SYSTEM and Administrators access; ACL setup fails closed. The full Host crash/recovery smoke checks no inherited or generic Users/Everyone ACE. Additional security review and restricted-user deployment acceptance are still required before default enable.
- Run macOS/Linux platform tests and ensure no unintended changes to their path. Explicit cross-project T2 Gate A/B/C and full UI acceptance remain independent release blockers.

## Replay gap rule

If the Worker has already trimmed output from before the latest durable Host cursor, the resumed stream starts at the Worker's absolute replay cursor. Stale cached scrollback is discarded in that case; its old data cannot be relabeled as adjacent bytes in the new output stream. This avoids false cursor reconciliation, but lost offline output remains unrecoverable beyond the capped ring.

## Key source files

- `tool/debug/test_terminal_worker_unix.py`
- `rust/alera-cli/src/terminal_host/session/pty_worker.rs`
- `rust/alera-cli/src/terminal_host/session/pty_worker_recovery.rs`
- `rust/alera-cli/src/terminal_host/session/pty_worker_acl.rs`
- `rust/alera-cli/src/terminal_host/session/pty_spawn.rs`
- `rust/alera-cli/src/terminal_host/session.rs`
- `rust/alera-cli/src/terminal_host/server/terminal_spawn.rs`
- `rust/alera-cli/src/terminal_host/server/terminal_session_requests.rs`
- `tool/debug/test_terminal_worker_isolation.py`
- `tool/debug/test_runtime_host_worker_recovery.py`
- `tool/debug/pty_job_memory_probe.py`
- `tool/debug/test_terminal_worker_memory_limit.py`
- `tool/debug/test_terminal_worker_output_pressure.py`
- `tool/debug/pty_output_flood_probe.py`
- `tool/debug/test_terminal_worker_competing_hosts.py`
- `tool/debug/test_terminal_worker_resource_soak.py`
- `tool/debug/test_terminal_worker_offline_backlog.py`
