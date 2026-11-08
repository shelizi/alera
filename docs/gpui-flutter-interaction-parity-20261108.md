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

Production `main`, existing Host sessions, and Flutter releases are not
changed. Keep `supported=false`; no push/tag/release.
