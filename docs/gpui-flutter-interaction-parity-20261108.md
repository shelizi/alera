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

## Follow-up: Shared pane groups and editor-only durable layout (2026-11-08)

This increment builds on the existing `integrate/gpui-p0p1p2-20261108`
worktree; **no additional worktree, branch, or changes to production `main`**.
Independent Devin SWE-2 Flash Max workers owned `workbench_panes.rs` (pane
model) and `workbench_layout_store.rs` (durable JSON), with all runtime
composition handled centrally. A third worker stalled on noninteractive
approvals and its narrow `editor_ui.rs` changes were completed by the
coordinator instead of weakening the permission policy.

- One `WorkbenchPaneLayout` now owns Primary/Secondary groups containing
  stable Host Terminal/Agent and native Editor IDs, per-pane active identity,
  keyboard focus, horizontal/vertical direction, split/move and collapse.
  Existing Host PTYs and Editor buffers are **not cloned, terminated or
  replaced** by a pane move. Workspace changes clear the projection.
- A real native GPUI split container displays both pane surfaces. The primary
  terminal renders its **own Host target**, not whichever Host was last active
  in TerminalDock. Pointer focus validates the target before forwarding raw
  keys; the secondary Host preview checks target identity before input.
  Secondary editor text is an immutable snapshot; if the primary is a Host,
  the existing live NativeEditor entity can render in the secondary pane.
- Mixed tab captions identify the owning pane and the context actions can
  move any supported tab kind into/out of the secondary pane. Dropping onto
  the second pane, toggling split axis and unsplitting update local layout.
  The redundant embedded editor tab row and terminal tab list are hidden,
  while required terminal VT, auto-poll, read-only preview and recovery
  controls remain reachable.
- Editor-only pane layout has a versioned, bounded, atomic JSON store scoped
  by the workspace's existing deterministic session key. It stores only
  relative paths of file-backed editor tabs, group order, focus and axis.
  The schema has **no Host handles, PTY contents, agent credentials or draft
  text**. On restart/switch, ordinary EditorTabs path guards revalidate
  restored files; scratch recovery stays in EditorDraftStore. A malformed or
  unknown-version layout is preserved, not overwritten.
- The Rust model tests cover pane moves, stale identities, unique ownership,
  reset, group selection, JSON roundtrip, invalid schemas, normalization and
  repeated atomic disk replacement on Windows. **215/215 library tests pass**; use
  `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`.
- The Windows GPUI native `cargo build --manifest-path desktop_gpui/Cargo.toml --locked --offline`
  passes, as does `cargo fmt --check` and `git diff --check`. The only
  build warning is the previously known `proc-macro-error2` future incompatibility.

### Remaining acceptance and explicit limitations

- **Second-pane Host output is not a separate full-color VT renderer** yet;
  this path shows a text projection with a guarded raw keyboard target.
  The old Host-only `TerminalPaneLayout` split still exists for transition.
- Two Editor tabs can share the mixed pane model, but there is only one
  mutable `NativeEditor` entity. When both pane groups contain editors,
  the second group uses a read-only content projection until independent
  editor surfaces/selection/IME focus are available.
- Legacy read-only preview tabs are not yet unified with editable tabs.
  The global mixed-strip reorder is not yet persisted as one complete
  cross-kind tab order. Terminal/Agent Host lifecycle is deliberately not
  restored from this editor-only layout.
- Confirm real Windows right/middle click, dragging, Chinese IME, two-pane
  raw PTY input, editor save/conflict, restart and cross-workspace isolation
  in the native window. No user acceptance has been claimed from compilation.
- Keep GPUI `supported=false`; do not merge, push, tag or release until
  the outstanding runtime GUI gates and core Agent/MCP parity are satisfied.

## Follow-up: Native Windows mixed-pane smoke and VT work (2026-11-08)

This phase reused the **same** `integrate/gpui-p0p1p2-20261108` worktree;
no other worktree or branch was created. We launched the native GPUI binary
using `cargo run --manifest-path desktop_gpui/Cargo.toml --locked --offline`
and actually exercised its mouse and keyboard interactions on Windows.

### Verified in the live GPUI window

- The common Editor tab's right-click inline menu opens; selecting
  **Second Pane** moves even an *inactive* file tab to the second pane without
  replacing the first pane's document. **First Pane** moves a tab back.
- A physical mouse **drag/drop** from the common tab strip to the secondary
  header also transfers the intended tab. Switching **Stack / Side by side**
  and **Unsplit** changes the visible native split layout.
- After fixing GPUI's display-style order (applying `.hidden()` after
  `.flex()`), the redundant nested NativeEditor tabs/navigation and legacy
  file-preview tab row no longer crowd editable workbench panes. The compact
  Editor toolbar clips its filename and hides redundant buttons.
- In an isolated two-file fixture, one Editor tab was moved to the secondary
  pane and edited with mouse + keyboard, then saved via **Ctrl+S**. On-disk
  verification confirmed **only the intended file changed**. Closing the
  application and relaunching restored both file-backed editable tabs,
  their primary/secondary membership, split orientation, and saved content.
- Native startup now describes restored tabs as **editable** rather than
  reporting them as read-only previews.

### Implementation and safety

- Second-pane Host output now has the same **styled ANSI/VT rows** as the
  primary renderer: foreground/background, cursor, bold, italic, underline
  and text positioning are projected from the existing bounded `VtScreen`.
- Background Host polling selects the non-focused pane's exact
  `TerminalTabContextTarget`; replies are dropped if the workspace, pane
  assignment or Host ticket becomes stale. It does not poll the active ticket
  twice. Pane focus changes reset the old terminal key queue before
  switching the authenticated Host session.
- Editor tab/file identity changes (including Save As and keyboard tab
  navigation) persist pane layout; typing ordinary editor content does **not**
  trigger a layout JSON write.
- `workbench_panes` adds a regression test for independent Host background
  selection and for Editor-only panes not opening Host read targets.
- **216/216 Rust library tests pass**. The Windows GPUI native
  `cargo build --locked --offline` succeeds; only the preexisting
  `proc-macro-error2` future-incompatibility advisory remains.

### Gates still open

- No authenticated Runtime Host control file was available during this GUI
  test. Therefore true two-PTY streaming, resizing, focus and raw keyboard
  input **were not live-accepted**, despite compiling the VT/polling changes.
- The host OS input used for this smoke is not a true Chinese IME composition
  test. Preserve the separate Windows IME acceptance gate.
- Unfocused Editor panes still use a read-only snapshot from one shared
  `NativeEditor` entity; independent concurrent editable surfaces, agent
  launch/MCP, pointer-anchored native menus, detachable windows and full
  mixed tab-order persistence still require work.
- The isolated fixture was created under the existing worktree as
  `.gpui-smoke-20261108/`. Cleanup requires explicit destructive-command
  approval under the workspace security policy; do not confuse that
  temporary folder with a Git worktree or commit it.
- GPUI remains **`supported=false`**. No production merge, remote push,
  tag or release was performed.

## Follow-up: Independent NativeEditor pane owners (2026-11-08)

The earlier shared-Editor read-only second-pane limitation above has now
been **replaced in source**, but the full native mouse/IME GUI acceptance
gate remains open.

- Continued in the existing `integrate/gpui-p0p1p2-20261108` worktree.
  Devin SWE-2 Flash Max owned only `editor_tabs.rs` for the core data
  transfer and regression tests. Runtime pane/UI composition was handled
  separately; the second Devin job did not start successfully and was not
  treated as completed. No branch/worktree was added.
- `EditorTabs` allocates process-unique nonzero tab instance IDs and can
  detach/insert an opaque `DetachedEditorTab` containing the **same live
  EditorSession**, dirty document, undo/redo history, selection, discard
  state and recovery store. Insertion refusal returns the value for
  rollback; moving a tab never reloads or writes disk contents.
- Workbench owns one native `NativeEditor` entity per pane. Moving a tab
  through context actions or native drag transfers the exact session
  between owners; `Unsplit` returns the sessions to the primary editor.
  A move refused by tab capacity or duplicate path attempts to restore the
  original session. A file already edited in the other pane is focused,
  not opened as another independent buffer.
- Each view has its own GPUI focus handle, line geometry and scroll state.
  Pointer focus updates the owning Workbench pane; toolbar Save, Save As
  and Close use that pane. Dirty-buffer guards cover **both** entities on
  workspace switch and Explorer mutations. Late editor observers from a
  previous workspace generation are ignored.
- Restoration rebuilds the actual primary/secondary Editor owners from
  file-backed paths, not merely their display slots. Native Editor Ctrl+N
  and Save As additions are assigned to the real owning pane; no Host
  sessions or unsaved draft contents are serialized into the pane layout.
- `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features
  --lib --locked --offline`: **223/223 PASS** (seven new editor-transfer
  tests), `cargo check --locked --offline`: **PASS**, `cargo fmt --check`
  and `git diff --check`: **PASS**. The pre-existing
  `proc-macro-error2` advisory remains.

### Acceptance still required

- The native binary was launched for the existing isolated fixture, but the
  interactive desktop was being used by another foreground application.
  The GPUI window could not be kept in a reliable test foreground, so no
  reproducible end-to-end dual Editor mouse/IME/Save proof was obtained
  **for this new two-entity implementation**. The launched GPUI process was
  stopped through its own managed command session; no other application
  was terminated or controlled. The previous phase's Editor GUI smoke
  belongs to the old single-owner implementation and is not substituted.
- The default `~/.alera/runtime` does not provide an authenticated Runtime
  Host control file. True dual Host PTY input/output, resizing and Chinese
  IME composition remain unaccepted; do not start or take over an existing
  user's Host just to satisfy a smoke test.
- The existing untracked `.gpui-smoke-20261108/` fixture is deliberately
  **not staged** or deleted without the workspace tool's destructive action
  approval. Keep production `main` untouched and GPUI
  `supported=false`; no remote push, tag or release.
