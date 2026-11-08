# GPUI desktop — Flutter workbench interaction parity (2026-11-08)

Goal: **equivalent workflows and expectations**, not identical colors, geometry,
or iconography. Source of behavior: Flutter
`workspace_workbench_tab_strip.dart`,
`workspace_workbench_new_tab_button.dart`, and
`workspace_explorer.dart`.

## Implemented in this pass

| Workflow | GPUI behavior |
| --- | --- |
| New scratch from tabs | Double-clicking only the **empty area** of both the upper preview-tab strip and lower native-editor tab strip creates a new, independent unsaved scratch. Clicking an existing tab still focuses it; the tab drag/drop path is unchanged |
| New scratch when tabs overflow | Horizontal tab scrolling is separate from a permanently visible `+ New File` control; the button does not scroll away with 24 tabs |
| New scratch from center header | `+ New File` is available beside `+ New Terminal` even while Terminal is selected; it switches into Editor and uses existing scratch-draft persistence |
| Editor keyboard | Ctrl+N / Cmd+N creates a scratch; Ctrl+W / Cmd+W closes the active **clean** tab, and never silently discards a dirty draft; Ctrl+Tab / Ctrl+Shift+Tab remains in place |
| After closing last tab | Editor shows a safe empty state with `+ New File`; workspace association is retained for reopening a scratch; outer Save, Save As and Close operations do not dereference a missing document |
| Switching tabs | Selecting an existing native editor tab restores editor keyboard focus |
| Dirty background tab protection | Operations which check `NativeEditor::has_unsaved_changes` now account for **all** tabs, including unsaved changes hidden behind a clean active tab |

The existing explicit two-click Close/Confirm discard route and local scratch
draft recovery remain unchanged. The native editor still intentionally limits
the total open tabs to 24 rather than allocating unbounded state.

## Verification and known limitations

- Model TDD verifies that an inactive dirty draft blocks a workspace change,
  Ctrl+W-like clean close cannot discard it, explicit two-phase discard still
  works, and a fresh scratch can open after the last tab closes.
- Run `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`.
- Run `cargo build --manifest-path desktop_gpui/Cargo.toml --locked --offline` for the native Windows UI compilation.
- GUI double-click, keyboard focus and high-DPI interaction acceptance still
  need a **native on-screen** test. Rust tests and compilation are not proof
  of the complete Windows GUI UX.

### Remaining interaction differences (not part of this batch)

- Flutter's global unified pane/tab group model includes terminal + editor +
  agent tabs, multirow pinned tabs, detach/multiwindow flows and comprehensive
  context menus; GPUI still uses simplified center-mode and native editor tabs.
- Flutter's Explorer has preview-versus-permanent open behavior, richer
  context menus, and full i18n; GPUI currently opens a file immediately on
  click in the native editor.
- Scratch Ctrl+S / save picker should be checked against Flutter's Save As
  workflow; current GPUI has a separate Save As control.
- Projects/worktrees hierarchy, shell/Git shortcuts, close confirmations,
  focus restoration and panel resizing need live comparative scenarios.
- Windows Traditional Chinese terminal IME experimental work remains **dirty
  and separate** in `fix/gpui-smoke-navigation-terminal-20261108`.

## Follow-up: Untitled Save / Save As interaction

- Ctrl+S / Cmd+S and the workbench Save action open the native Save As
  picker when the active document is Untitled; named files still use ordinary
  conflict-safe Save.
- The workbench Save As control delegates to the same native editor dialog
  rather than maintaining an independent asynchronous save callback.
- A Save As request captures the active tab identity and structural revision.
  Choosing a destination after switching, reordering, closing or reopening
  tabs **does not save any file**, including when a reopened file has the
  same pathname as the original requester.
- Multiple concurrent Save As dialogs are blocked; Save As to an existing
  file remains refused unless overwrite was explicitly authorized (the
  current GPUI picker deliberately does not offer overwrite confirmation).
- The GPUI native editor currently restricts Save As destinations to the
  workspace via its existing checked-destination rules. Flutter's external
  save location support still requires a separate security design.
- TDD: two new tests check stale dialog responses, switch-back, reordering,
  close/reopen same path and unchanged existing destination. The complete
  headless Rust suite after this change passed **186/186**; Windows native
  executable compiled successfully. Live OS-dialog acceptance remains open.

## Follow-up: Editor tab right-click and middle-click actions

- Right-click on a native editor tab opens a compact in-workbench action bar
  for **Close**, **Close Others**, **Close to Right** and **Cancel**. This is an
  inline GPUI action bar, not yet a pointer-anchored native popup.
- The right-click target is the clicked tab, **not automatically the active
  editor**. Its revision-bound identity prevents a stale context from
  addressing another tab after an intervening reorder or close.
- Middle-click a tab to close it, but only when safe; no implicit discard of
  unsaved file edits or recovered scratch content. Contextual Close likewise
  refuses unsaved tabs. Explicit two-step discard remains available through
  the tab's Close / Confirm control.
- Close Others and Close to Right operate relative to the right-clicked tab,
  skipping dirty neighbors. A headless regression tests closing to the
  right of a non-active tab with another dirty tab and unchanged disk content.
- Complete GPUI headless Rust tests: **187/187 PASS**. Windows native GPUI
  binary compiles successfully after resolving a GPUI 0.2.2 conditional
  element builder type mismatch. Live Windows mouse-event/position/focus
  testing is still required before calling this interaction accepted.

Remaining: terminal/agent unified context-menu actions, native popup
placement, drag between pane groups, and GUI acceptance on high-DPI Windows.

## Follow-up: Terminal tab context parity (2026-11-08)

- Terminal tab **right-click** shows an inline context action row mirroring
  the editor tab workflow: Focus, Close, Close Others, Close Right, Move
  Left/Right, Second Pane and Cancel. **Middle-click** closes only the local
  preview, never the Host-owned PTY.
- Actions target the **clicked tab**, not implicitly the active terminal.
  Context identities include the three Host fields (handle, workspace,
  tabId), the local preview generation and the workspace generation. A
  closed/reopened tab, recycled opaque handle or workspace switch cannot
  reactivate an old menu target.
- Close and bulk-close only alter `TerminalDock.tabs`; authenticated Host
  session inventory and PTYs remain unchanged. Any orphaned second preview
  pane is dismissed when its tab disappears.
- Second Pane requires a distinct, still-running attached session whose Host
  identity exactly matches the authenticated session list. It delegates to
  the existing pane-preview assignment, without creating, terminating or
  attaching a Host PTY.
- Existing Terminal tab drag-to-reorder and drop-to-secondary now carry that
  same context identity, not just the opaque handle. Both drag source and
  destination are revalidated at drop time, and split drop additionally
  requires a still-live Host session.
- Two Rust TDD cases cover reorder, close/reopen, handle reuse, stopped Host,
  cross-workspace stale context and preservation of the session inventory.
- Windows GPUI native `cargo build --locked --offline` passed (only the
  pre-existing `proc-macro-error2` future-incompatibility warning).
  Full Rust regression verification: **189/189 passed**. Compilation does
  **not** establish real right-click or middle-click behavior on Windows.

Still missing compared to Flutter: one mixed Terminal/Editor/Agent tab strip,
full pointer-anchored popup menus and detached-window workflows. No attempt
was made to invent a GPUI Agent tab without a working runtime agent backend.

Production `main`, existing Host sessions, and Flutter releases are not
changed. Keep `supported=false`; no push/tag/release.

## Follow-up: Mixed Terminal / Editor / Host-backed Agent tabs (2026-11-08)

This is **phase 1 of unified tab management**, not the completed Flutter
pane/group architecture. `desktop_gpui/src/workbench_tabs.rs` now projects the
existing `TerminalDock` and `EditorTabs` into a **single ordered strip**:

- `Terminal` is a Host-attached PTY preview. `Agent` is shown only if the
  authenticated Runtime Host session has a real `agent_type`; there is no fake
  or independently launched GPUI agent process.
- `Editor` retains its native buffer, undo, recovery and conflict-safe save.
  Each open tab gets a monotonically allocated per-editor-instance ID; a
  reopened path or scratch cannot borrow the identity of a closed tab.
- The common strip supports focus, middle-click close, explicit close button,
  right-click in-workbench actions, Close Others, Close Right, move left/right,
  and mixed-kind drag to another slot. Terminal right-click `Second Pane`
  delegates to the pre-existing Host-only preview split.
- Stale tab callbacks verify workspace generation and original owner identity
  before touching another tab. Closing a Host tab hides its **local preview**,
  never terminates an Alera Host PTY. Dirty editor tabs remain protected by
  the existing explicit discard / draft-recovery rules.
- Existing upper preview, native editor, and native terminal tab strips are
  retained during this transitional integration, so the new mixed strip is
  **not yet the sole tab bar**. Their editing and keyboard/IME implementation
  remain owned by existing entities.

### Verification and pending acceptance

- TDD covers the mixed-order projection, focus, stale identity handling,
  close/reopen editor IDs and forward/backward slot moves.
- Headless model regression: **194/194 PASS**. Reproduce with
  `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`.
- Windows native compilation: **PASS** (only pre-existing
  `proc-macro-error2` future-incompatibility warning). Reproduce with
  `cargo build --manifest-path desktop_gpui/Cargo.toml --locked --offline`.
- Still required: **real Windows mouse/IME drag + context-menu acceptance**;
  a unified split-pane tree that can show Editor or Agent in either pane;
  persistence of mixed tab order/panes; deduplicating transitional local
  strips; pointer-anchored native context menus; detached windows. The
  `Agent` tab classification does not establish full agent-launch/MCP parity.

No production merge, push, tag or release. GPUI remains `supported=false`.
