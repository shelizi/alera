# Windows shared-engine Terminal opt-in experiment (2026-10-23)

## Baseline

Windows Alera now starts in shared-engine mode by default for Editor tabs. Every other tab type still uses the legacy process-backed detached window. The supported Editor path and `--alera-legacy-detached-tabs` fallback remain unchanged.

## Experimental Terminal handoff

The Terminal path can be enabled **only in a separately compiled test build** with the Dart define `ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true`. Its default is `false`. The define does not override `--alera-legacy-detached-tabs`, which continues to use the original child process for every tab. Git Diff, Git History, PDF, and Markdown remain on the original child-process route even when this define is enabled.

The opt-in reuses the existing shared root ProviderScope, TerminalRuntime and durable session handle. The source tab is hidden and unmounted before the child tab surface may mount. Native child close waits for a teardown frame before restoring the source. This is intended to prevent simultaneous attachment of the same TerminalView GlobalKey, focus node and PTY. A Terminal widget's disposal releases its visibility lease; it must not call closeTab or terminate the durable PTY.

## Test and validation

Prepare the worktree with `tool/development/prepare_windows_build_storage.ps1` first. Fetch the exact checked-in `third_party/xterm` and `third_party/dart_terminal` submodules before running `flutter pub get --offline` on a new worktree.

```powershell
flutter test --no-pub test/unit/shared_engine_detached_window_lifecycle_test.dart test/unit/shared_engine_detached_backend_test.dart test/unit/detached_tab_entrypoint_compile_test.dart
flutter test --no-pub test/widget/terminal_surface_test.dart --plain-name 'Terminal view handoff keeps its PTY and output history'
```

First isolated-worktree results (Windows, 2026-10-23): shared-engine lifecycle, startup, fallback and Quit-guard unit tests **21/21 passed**; TerminalSurface widget tests **45/45 passed**; native terminal-runtime and detached-tab shortcut tests **145 passed, 2 platform skips**. Whole-project Flutter analyze exited 0 with no errors when legacy warnings and infos were nonfatal. The two pre-existing TUI resume tests initially failed because they assumed hiding an ordinary tab pauses host delivery. Their fixture now explicitly backgrounds the entire app to exercise the actual pause/resume path; both pass again. These are unit and widget validations, not native detached Terminal GUI acceptance tests.

Windows Native Runner CTest **3/3 passed**. The first isolated Windows Debug executable also compiled successfully using `build_alera_in_worktree_debug.cmd` with the experimental Terminal define and an isolated support directory; the standard `flutter build windows` frontend could generate configuration, but local CMake compiler discovery required the documented Visual Studio + Ninja fallback.

### Real Windows Terminal GUI acceptance (isolated Debug, 2026-10-23)

- Start/stop the isolated executable once to create its SQLite schema. `seed_shared_editor_test_workspace.py --configuration Debug --reset-test-file --tab-kind terminal` creates a disposable `E2E Terminal` tab in that profile. The script remains backward-compatible with its default Editor fixture.
- Windows E2E PID **64180** initially owned one top-level `Alera Dev` window, with an interactive Windows PowerShell prompt and native DPI **120 (125%)**.
- Right-click the E2E Terminal tab, choose **Move to New Window**: the OS reported two real HWNDs, `Alera Dev` and `Alera - E2E Terminal`, **both inside PID 64180**, not a second process. The child displayed the live PowerShell session and retained prior output.
- Real desktop keyboard input in the child executed `echo SHARED_TERMINAL_OK` and displayed the matching output. Injecting raw `WM_CHAR` events to the native child was unreliable and produced duplicate characters; don't equate those synthetic events with real keyboard behavior.
- Posting native `WM_CLOSE` to the exact E2E child HWND returned to one top-level window; the main Terminal tab restored the existing scrollback, including `SHARED_TERMINAL_OK`. Real desktop input then executed `echo RETURN_PTY_ALIVE` in the restored main Terminal, with matching output. Thus the PTY remained interactive after native close and reattachment in this run.
- The real GUI screenshots and temporary data are stored only in the ignored per-worktree `build/windows/shared_gui_ninja/diagnostics` and `e2e_support` directories. Other running Alera processes and regular user data were not targeted.
- **At this first one-cycle checkpoint**, repeated Terminal handoffs, large ongoing PTY output, real drag-back mouse gestures, Zhuyin IME, mixed DPI, clipboard/mouse-selection, concurrent children, packaged Release AOT, and longer-term memory/handle behavior still needed testing. The next section records subsequent 60-cycle and two-child progress; sustained output, IME/DPI, clipboard, Release and multi-hour leak analysis remain outstanding.

### Continued Windows stress and concurrent windows (2026-10-23)

The already isolated Debug E2E application was exercised in the same process without restarting between the batches. `probe_real_alera_editor_cycles.ps1` now accepts a child title, scenario and unique diagnostics filename; the Editor defaults are unchanged. The Terminal scenario refuses a build without the experimental Dart define, and requires native `WM_CLOSE` for automated cycles because synthetic `PostMessage` clicks of the child Return icon were not reliable. A **real mouse click of Return** was tested separately and successfully docked the Terminal. Do not count the synthetic Return test failure as a product regression.

PowerShell and desktop-input tools use differently scaled coordinates on this 125% DPI remote display. Initial `0/3` synthetic gestures missed the tab/menu. After inspecting the menu and calibrating native view-relative coordinates, all 60 subsequent gestures succeeded at their **first** attempt. Those coordinates are specific to the current test layout, not portable defaults.

```powershell
pwsh -NoProfile -File tool/multiview_poc/probe_real_alera_editor_cycles.ps1 `
  -Scenario TERMINAL -ChildTitle 'Alera - E2E Terminal' `
  -Cycles 30 -NativeCloseEvery 1 -ChildReadyWaitMs 250 `
  -TabX 754 -TabY 107 -MenuX 430 -MenuY 386 `
  -ReportName real_terminal_native_cycles_30.json
```

Results for the one native Windows PID (`11396`):

| Measure | First 30 cycles | Next 30 cycles |
|---|---:|---:|
| Detach / native close success | **30/30** | **30/30** |
| Open P95 (includes 500 ms synthetic popup wait) | 999 ms | 941 ms |
| Close P95 | 66 ms | 64 ms |
| Working set at batch start / finish | 594.8 / 617.6 MiB | 597.3 / 605.2 MiB |
| Handle count at batch start / finish | 1027 / 1054 | 1025 / 1056 |
| GDI objects at batch start / finish | 23 / 23 | 23 / 23 |

Across the 60 post-close samples: RSS min/max **596.4/623.3 MiB**, handles **1043–1057**, GDI **always 23**, combined observed Open P95 **943 ms**, Close P95 **65 ms**. After returning to idle, RSS was approximately **595.5 MiB** and handles **1023**. This suggests transient allocations and no *obvious* monotonic growth in these 60 samples; it **does not prove leak-freedom**, especially for native PTY and renderer allocations over hours. Windows remained at one main HWND after each close. A real keyboard command `echo PTY_AFTER_60_CYCLES` still worked on the original shell after all 60 handoffs.

Concurrent Terminal validation: open a second Terminal tab, detach `Terminal 1`, detach the original `E2E Terminal` at the same time. Windows reported **three HWNDs under the same PID**. Closing `E2E Terminal` returned to two HWNDs and left `Terminal 1` interactive (`echo SECOND_OK` completed); closing `Terminal 1` returned to one HWND with both tab records preserved. Switching to the original tab showed its previous `PTY_AFTER_60_CYCLES` output. A first burst of synthetic desktop typing dropped the start of a command, so only the subsequent successful real-input `echo SECOND_OK` is counted as confirmation.

The JSON samples and Windows capture artifacts remain only in the ignored `build/windows/shared_gui_ninja/diagnostics/` directory. Neither `main` nor installed Alera was changed. Native close, PTY identity, and concurrent view lifecycle are substantially better verified now, but the experimental Terminal mode must **remain opt-in** pending mixed-DPI, IME, clipboard, sustained output, real drag-back, and Release validation.

The final focused Flutter suite (shared backend, lifecycle, Dart entrypoint and TerminalSurface) passed **61/61**. The updated PowerShell cycle driver parsed successfully, the Dart source was formatted, and `git diff --check` passed. The disposable E2E process was shut down through Alera's own running-PTY confirmation dialog; no test GUI remains running.

### Sustained-output handoff and recovery investigation (2026-10-23)

Added `e2e_terminal_stream.ps1` as an explicitly guarded E2E-only PowerShell stream generator; it refuses to run outside a path ending with `shared_gui_ninja/e2e_workspace`. The script prints numbered terminal lines, records a progress counter every 50 lines, and writes a final JSON completion marker in the disposable test workspace. It does not access other Alera workspaces or persistent user profiles. The widget regression `Continuous PTY output survives source and child view handoffs` passes, and `Terminal follows live output after a resized view handoff` passes with a changed viewport height and continued PTY output.

Actual isolated Windows Debug run under **PID 50612**, still with the experimental Terminal Dart define and isolated support directory:

- First output run: **1,200 lines completed in 37,525 ms**. Its initial automated detach attempt correctly failed before posting any mouse message because the restored 730 x 445 E2E window was smaller than the previously calibrated test coordinates. This run did **not** validate a live detach.
- An isolated host-client disconnect was subsequently observed. The runtime-host log has one `peer_closed` at `2026-10-23T10:13:33.658Z`; the same E2E host process remained alive. The GUI displayed `Terminal host connection closed`, but clicking **Reconnect** recovered the previous shell and scrollback, including the 1,200-line completion marker. Causation is unknown: do **not** attribute it to the stream, window resize, or multiview without a repeatable reproducer.
- The E2E native HWND was then verified as belonging to PID 50612 and resized to 1460 x 890 for real GUI interaction. A fresh **1,200-line / 37,633 ms** stream was started. While the stream progress was approximately **800/1200**, right-click **Move to New Window** created a second HWND in the same process. Native `WM_CLOSE` returned the child to the primary view before the stream finished. The final progress and result marker both confirmed **1200/1200 complete**; the Terminal remained interactive and subsequently wrote an `terminal_after_handoff.txt` sentinel in the isolated workspace. No alternate PTY or app process was needed.
- The output was still present after the handoff. A captured remote viewport sometimes showed an earlier tail position rather than the final prompt without additional scrolling; because the remote capture involved resizing and manual scroll tests, this is tracked as an **unresolved UX observation**, not a confirmed regression. The resized-view tail-follow widget test passed.
- `probe_real_alera_editor_cycles.ps1` now supports `-PreflightOnly`. It validates tab and menu hit points against the actual E2E FlutterView bounds before stressing the app, avoiding the previous misleading first-cycle click error after window restoration. A valid coordinate set was accepted; an intentionally out-of-range tab point was rejected with a precise error.

**At this sustained-output checkpoint**, the remaining gates were repeatable diagnosis (or exclusion) of transient runtime-host disconnects, real multi-monitor mixed-DPI drag-back, Zhuyin IME composition, clipboard/select, streams lasting significantly longer than 38 seconds, Windows Release AOT and memory/handles during prolonged streams. The next section records the subsequent Release AOT acceptance; the other gates still remain. The shared-engine Terminal option stays **off by default**.

### Isolated Windows Release AOT acceptance (2026-10-23)

The Release build now has independent evidence, rather than only Debug Flutter tests. The standard `flutter build windows --release` frontend generated the correct Dart configuration but stopped at this machine's pre-existing CMake C++ compiler detection. Invoking the checked-in `build_alera_in_worktree_release.cmd` through Visual Studio VC + Ninja Multi-Config succeeded (**exit 0**). Its isolated Release bundle contains `runner/Release/alera-dev.exe`, `data/app.so` (28,115,848 bytes), `alera_native.dll`, `code_forge.dll`, `flutter_windows.dll`, `multiview_desktop_plugin.dll`, and `resources/alera/alera.exe` (Rust CLI sidecar). The generated Flutter config was checked against all three base64 Dart defines: `ALERA_E2E_DESKTOP=1`, `ALERA_E2E_SUPPORT_DIR=<this worktree>/build/windows/shared_release_ninja/e2e_support`, and `ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true`. These files are all under ignored `build/` and were **not installed or published**.

- Initial isolated Release GUI started with `Responding=True` and created its own SQLite support data. After a normal quit and `seed_shared_editor_test_workspace.py --configuration Release --tab-kind terminal`, restarted the isolated Release executable as PID **18504**. Its test workspace and PowerShell PTY were visible and interactive.
- The real Terminal tab's **Move to New Window** opened a second native HWND (`Alera Dev`, `Alera - E2E Terminal`) under the **same PID 18504**, at DPI **120**. OS sampling: `WINDOWS=2`, approximately **191.4 MiB RSS**, `HANDLE_COUNT=1015` at that instant. This is one smoke snapshot, not a comparative benchmark.
- Typing `echo AOT_SHARED_TERMINAL_OK` into the child ultimately returned the matching PowerShell output. The first synthetic desktop type lost its initial `e` immediately after focusing the view; the second, correctly focused command succeeded. Do not claim this is a proven IME issue or a universal typing success rate.
- Posting `WM_CLOSE` only to the exact E2E child returned to **one native HWND** with the same PID. The main Terminal preserved the existing `AOT_SHARED_TERMINAL_OK` scrollback and successfully executed a new `echo RELEASE_RETURN_OK` command; output matched. The isolated Release GUI was closed through Alera's regular **running terminal environment** confirmation dialog, and PID 18504 exited.

Result: **Release AOT native same-engine Terminal detach, interactive child, WM_CLOSE and interactive return smoke all passed**. This does **not** address multi-hour stability, double-DPI monitor handoff, native Zhuyin composition, mouse text selection, clipboard, real drag-back, or the one earlier transient Debug host `peer_closed`. Leave the opt-in off by default and do not promote it as a public release yet.

### Terminal Host socket error categorization and input bounds (2026-10-23, next isolated batch)

Investigation of the earlier `2026-10-23T10:13:33.658Z` `peer_closed`: the old host log only proves that its TCP reader saw EOF/read failure, **not why the client closed**. No causal evidence ties it to the multiview detach or the earlier 1,200-line stream. However, code inspection and red-first tests uncovered a reproducible transport defect: a binary frame announcing `0x7fffffff` payload bytes was silently buffered rather than rejected, and no classified reader fault reached the session. This can strand message delivery if the wire framing is corrupted; it is **not proof** that the historical disconnect had this cause.

Implemented in the isolated experiment:

- Hard-cap newline-delimited hello/legacy control frames and length-prefixed binary frames at **32 MiB** (comfortably beyond the current 10 MiB raw scrollback snapshot allowance plus base64 overhead). Reject oversized declared lengths immediately with `FormatException`, without storing the raw payload.
- Catch parser/socket/write faults in the reader isolate and emit **only** a bounded stage (`connect`, `read`, `write`, `protocol`); watch unexpected isolate exits and uncaught exceptions so a dead reader cannot silently strand terminal output. The fallback main-isolate socket reader now also catches malformed-frame errors instead of throwing in an asynchronous listener.
- Complete the reader's typed error future **before** its control stream closes, so `protocol` is not mistaken for normal EOF. A live terminal client can reconnect to the still-running host and resume the PTY through the existing path; no automatic PTY restart was added.
- Record one app WARNING per unexpected connected-host close with a bounded `origin` (`socket_eof`, `socket_reader_error`, `message_dispatch_error`, `control_stream_error`, `request_write_error`, `heartbeat_exhausted`, etc.) and, when available, `reader_stage`. **No socket error string, authentication token, raw RPC payload, terminal content or unredacted stack** is logged by this new diagnostic path. Routine intentional application quit does not emit the warning.
- RED-first tests demonstrated that the oversized-frame parser previously returned an empty list and that the socket isolate previously hung instead of reporting the malformed frame. After the repair, the focused codec, reader-isolate and SocketTerminalHostClient tests passed **75/75**. A larger PTY resume, binary fixture and shared-window lifecycle regression passed **163/163** after fixing a pre-existing fixture that compared only legacy `snapshot` bytes even though the reader isolate legitimately fills `snapshotText`. A new 1,500-output-frame single-socket ordered-burst test passed on its own; no reconnect was needed.

This instrumentation is deliberately narrow. If `peer_closed` recurs during future Windows Debug/Release runs, correlate the new `SocketTerminalHostClient` `origin=...` line with the host `cleanup-completion` timestamp before proposing additional automatic recovery. Still outstanding: native IME, mixed-DPI drag-back, clipboard/selection, multi-hour output and leak analysis. The experimental shared Terminal define stays **off by default**.

#### Recompiled Debug GUI smoke after transport patch

Re-generated the isolated Debug Flutter config and checked the exact `ALERA_E2E_DESKTOP=1`, `ALERA_E2E_SUPPORT_DIR=<worktree>/build/windows/shared_gui_ninja/e2e_support` and `ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true` defines. Native `flutter build windows --debug` again encountered this machine's CMake compiler-discovery issue; the checked-in `build_alera_in_worktree_debug.cmd` VC+Ninja fallback completed with exit **0** and `ISOLATED_DEBUG_BUNDLE_READY`. The rebuilt `alera-dev.exe` started as isolated PID **47788**, owned a single `Alera Dev` HWND and was `Responding=True`.

During this session the remote desktop focus/coordinate mapping was unreliable with another running Alera window visible, so the rebuilt GUI **was not** put through another interactive long-output detach/return test. The script sent the normal quit command only to the exact isolated E2E PID and verified **E2E_PROCESS_EXITED=47788**. Do **not** count this as a new PTY handoff acceptance result; the previous 1,200-line and Release AOT native results predate the socket-reader changes. The new code's 1,500-frame single-connection validation was an in-process loopback test, not a native GUI test.

#### Deterministic isolated fixture repair

Investigation found `seed_shared_editor_test_workspace.py` populated only the test Runtime Host `runtime.sqlite`; the disposable app's local Drift `alera.sqlite` still had **0 projects / 0 workspaces / 0 tabs**, even though Runtime had **1 / 1 / 2**. The seed now writes matching stable project/workspace/tab IDs to both databases, using Drift's **Unix seconds** timestamps and Runtime's **ISO timestamps**, only after verifying both DB paths stay within the ignored E2E build. The new in-memory SQLite test checks Terminal payload, idempotency/editor switch and fail-closed missing-schema handling (**3/3 passed**). Running the new seed while E2E was shut down produced `SEED_OK` and `SEED_LOCAL_OK`; after restart, the local cache remained **1 / 1 / 1** and Runtime still contained the fixture. This removes one fixture inconsistency, **not** proof that the live GUI auto-selects that workspace; Alera normally starts without an active workspace and may show Welcome until selection.

On the later isolated Debug process, native Win32 enumeration confirmed an actual `FLUTTERVIEW` child even when the earlier `probe_alera_gui_input.ps1` reported none. Therefore the initial theory that the FlutterView was missing is **not established**. Remote desktop screenshot coordinates and native HWND coordinates must not be mixed; input sent to global desktop coordinates can hit an unrelated process when the E2E window is on a negative-origin monitor. Use HWND-owned native hit tests and a selectable E2E workspace for future GUI acceptance. This subsequent Debug GUI has **not** yet passed a post-patch live detach/return test.

For an isolated GUI build, keep the E2E App ID and support directory settings from `ALERA_PRODUCTION_INTEGRATION_HANDOFF.md` and additionally supply `--dart-define=ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true` during the Flutter compilation step. Merely passing a command-line argument to an already built EXE does not enable Terminal mode. Do not run experimental GUI tests against the real Alera user profile or modify an installed build.

## Safety limitations before promoting

- This opt-in is **not** production-ready and must not be merged into a production default without further validation.
- Basic command input and scrollback preservation across one real Windows ConPTY/terminal-host detach and native close have passed. Continue checking resize, Ctrl+C, Ctrl+S, background output, mouse selection and clipboard before promotion.
- Verify Taiwanese Zhuyin IME composition, mixed-DPI cursor hit tests, drag-back from two screens, native close while the shell is producing output, and multiple simultaneous Terminal windows.
- Monitor sustained terminal output and native handles/memory over repeated cycles; short unit tests cannot establish leak freedom.
- The `window_manager` plugin is main-HWND-oriented; never call global `windowManager` for child view title or close. Use the existing view-scoped native multiview controller.
- Native plugin and Flutter lifecycle integration tests, packaged Release AOT GUI, and a fallback acceptance run are required before enabling this path for users.
