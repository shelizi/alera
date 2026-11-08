# Alera GPUI Native — Flutter Classic UX contract

Date: 2026-11-08. Branch: `integrate/gpui-p0p1p2-20261108`.

The **original Flutter desktop edition is the interaction specification**.
GPUI may use different layout/rendering primitives and colors, but may not
invent a second user workflow merely because implementing it is easier.
This checklist is based on the original source in this same repository,
not on a visual mockup or a collection of generic IDE assumptions.

## Source of truth

| User surface | Flutter Classic source | Native GPUI source |
| --- | --- | --- |
| File tree, filter, header and click | `lib/src/features/workbench/presentation/workspace_explorer.dart`, `workspace_explorer_widgets.dart` | `desktop_gpui/src/main.rs`, `workspace_tree.rs`, `file_search.rs`, `native_ui_icons.rs` |
| File context menu and actions | `workspace_explorer_widgets.dart` (`_ExplorerMenuDelegate`, `_ExplorerBackgroundMenu`), `workspace_explorer_actions.dart` | `desktop_gpui/src/main.rs`, `explorer_ops.rs` |
| Projects / Worktrees sidebar | `project_workbench_sidebar_tiles.dart`, `project_workbench_workspace_rows.dart` | `desktop_gpui/src/main.rs`, `project_tree.rs`, `worktree_manager.rs` |
| Source control file tree and context | `workspace_git_diff_panel_tree.dart`, `workspace_git_diff_panel_rows.dart`, `workspace_git_diff_panel_groups.dart` | `desktop_gpui/src/main.rs`, `git_actions.rs`, `git_history.rs` |
| Terminal toolbar | `terminal_surface_toolbar.dart` | `desktop_gpui/src/main.rs`, `terminal.rs` |
| Tabs / context menu | `workspace_workbench_tab_chips.dart`, `workspace_workbench_tab_menu.dart` | `desktop_gpui/src/main.rs`, `editor_tabs.rs`, `workbench_tabs.rs` |

## Explorer baseline and implemented slice

Classic's actual header is **workspace name** followed by global buttons:
New File, New Folder, Save All, toggle ignored files, toggle hidden items
(Windows), Collapse All and Refresh. A **permanent `Filter files...` input**
is directly below. File rows use a distinct chevron expander, a file-type
icon, a filename, 32-pixel row height and indentation by 16 pixels per level.
An unexpanded folder must still be discoverable through the filter. Ctrl+P
Quick Open is a separate feature, not a replacement for the Explorer filter.

The current GPUI parity slice implements the workspace-name header,
New File, New Folder, Save All, Collapse All and Refresh, plus an overflow
menu for ignored/hidden visibility to avoid overcrowding the sidebar.
The permanent IME/Unicode filter is backed by the
existing bounded fuzzy file index (now including nested folder paths),
folder/file typed Codicon icons, 32 px rows, and a separate chevron that
does not also activate the row. Filtered folder selection reveals its
ancestors. Ctrl+P Quick Open and Ctrl+Shift+F project-content search
remain separate. Source file opening stays guarded by the workbench's
existing canonical-path checks.

Classic file/folder context menu includes New File/Folder, Copy/Cut/Paste,
Copy Path/Relative Path, Duplicate, reveal in OS file manager, external
application/editor operations, file-only Git history and comparisons,
Rename, collapse folder, refresh, delete. Its blank-space menu has only
New File, New Folder and Paste. This Native slice matches the core
item-specific ordering, copy-path commands, Collapse and Refresh, with
delete confirmation retained. It does **not** offer unsupported external
editors, Git comparisons or multi-file actions under misleading names.

### Remaining Explorer UX differences (do not mark as complete)

- Classic Material Icon Theme SVGs and Git status badges have not yet been
  reproduced exactly (Native currently uses closest bundled Codicons).
- Native Save All, Gitignored and hidden visibility toggles now have actual
  file operations/filters. Their precise visual placement differs: visibility
  is grouped under one overflow icon. Real Windows pointer acceptance and
  consistency with non-Windows hidden-file settings remain unverified.
- Ctrl/Shift multi-selection and batch Copy/Cut/Paste now have initial
  Native implementations across tree, flat list and filter results. Native
  uses bounded, workspace-scoped validation and preflight. In-app dragging
  onto a folder now uses the same Cut batch guards, while native Shell drag
  out and exact row-target OS drag in remain. Pointer acceptance is pending.
- Single-click versus double-click now has an initial implementation in
  the Native tree, flat list, and filtered search results: one replaceable
  read-only preview, with a second click promoting into the editable Native
  Editor tab. The preview is not saved as a permanent tab. Pointer-level
  acceptance, exact Classic timing, large-file preview limits, editor reveal,
  and file-search context-menu parity remain incomplete.
- The file context menu now includes Open in Alera and a safe View current
  Git changes action using the existing read-only diff panel. Full parity
  still needs Open with Default Application, external editor, reveal in the
  system file manager, HEAD/branch comparisons and nested source-control
  root actions. The current-diff action must not be called a HEAD comparison.
- Pointer-level Windows acceptance of the menu, keyboard focus after
  selection, actual popup bounds and high DPI remains required.

## Next work packages (strictly mirror Classic)

1. **File interaction** — preview/permanent editor Tab, multi-selection,
   drag/drop, ignored/hidden settings, file Git badges, remaining context
   actions. Do not silently modify external files to simulate parity.
2. **Projects/Worktrees** — reproduce Classic's grouped Pinned/All sections,
   project header context menu, inline creation of workspace and worktree
   status indicators rather than inventing separate admin pages.
3. **Source Control** — reproduce Classic's grouped change tree, inline
   Stage/Unstage, file context, commit workflow and Git Graph behavior.
4. **Terminal and Tabs** — reproduce floating, movable corner toolbar,
   context menus, active-tab appearance, two-pane interactions and
   clipboard/selection. Keep Host ownership and session guard rules.
5. **Acceptance** — place Flutter Classic and GPUI windows side by side
   at 1200×800 and 800×600, compare visual hierarchy and the same
   mouse/keyboard sequences. A green Cargo build is not UI acceptance.

Keep Native `supported=false` until original behavioral paths are usable
with a real Host. Do not merge to `main`, push, tag or release during
this experimental migration without explicit approval.

## Verification of this first Explorer slice

- `269/269` Rust model tests passed (including file index Unicode search,
  folder search, Collapse All and safe nested directory reveal).
- Native Windows `cargo check` and `cargo build --locked --offline` passed.
  Formatting and `git diff --check` passed. Rust emitted only existing
  `proc-macro-error2` compatibility notice and intentionally retained
  unreferenced, Classic-superseded Move/Copy-Into action variants.
- Launched an isolated Windows Native preview with an intentionally missing
  CLI/Host and a disposable test repository. Actual screen verified the
  persistent **Filter files** input, separate folder chevron, distinct folder
  and file icons, and the one-line truncated workspace title after a visual
  iteration. No production workspace or Runtime Host was accessed.
- The precise mouse/keyboard interaction behavior of the new search input
  and context menu is **not signed off** by the screenshot or Cargo checks.
  Remote mouse injection on this secondary monitor did not reliably
  activate basic GPUI controls in previous isolated attempts.

## Follow-up: preview tab semantics

- Explorer file single click now uses `WorkbenchModel::preview_path`: exactly
  one transient read-only tab is replaced on the next file click. Existing
  pinned preview tabs and the underlying Native Editor sessions are retained.
- Double click explicitly promotes that path and opens or focuses the real
  file-backed Native Editor. The workbench overlays read-only content while
  retaining hidden dirty Native Editor buffers, then restores editor focus
  when the user chooses an editable tab; preview never calls Save.
- Transient previews are excluded from `PreviewSessionStore` persistence.
  Regular saved tabs and scratch-draft recovery remain separate; a failed,
  oversized or outside-workspace preview never replaces a previous good one.
- Model and state regression tests cover transient replacement, promotion,
  failed reads, workspace boundaries and no unintended startup restore.
- This remains **source/test verified only**, not a Windows mouse/double-click
  usability sign-off. Preview limits remain 256 KiB / 600 lines; completing
  the Classic large-text-file behavior needs another slice.

## Follow-up: multi-selection and batch clipboard

- Native Explorer shares a workspace-bound visual selection model across
  filtered, tree and flat presentations. Ctrl/Cmd toggles an individual file
  or folder; Shift selects an inclusive visible range from a stable anchor.
  Modified clicks do not open, expand or edit files. Context-right-click
  retains an existing multi-selection, and selecting a new item replaces it.
- Batch Copy/Cut/Paste now captures up to 24 validated top-level entries.
  A selected folder subsumes its selected descendants. Paste preflights
  destination names, existing paths, symlink/Git metadata safety, and
  same-workspace identity before any normal file move/copy. Copy may be
  reused; successfully moved Cut entries are consumed.
- For rare mid-batch I/O failure, earlier successful writes cannot safely
  be rolled back automatically. The UI refreshes and reports the partial
  failure, retaining remaining clipboard entries for explicit retry.
- Changing filter/workspace and successful Explorer mutations invalidates
  stale row selections. This does not implicitly delete user files.
- Native Ctrl/Cmd+C/X/V and Ctrl/Cmd+A are now Explorer-context actions;
  they require the actual Explorer focus handle and do not intentionally
  consume text-editor or Terminal clipboard commands. Modified clicks and
  Explorer context right-click restore Explorer keyboard focus. Key-routing
  behavior still needs OS-level acceptance and must not be called verified.
- Remaining: native Windows Shell drag-out, real Windows pointer/drop
  acceptance, true recycle-bin (Trash) semantics for deletion, and complete
  Git badge coverage when Git's bounded snapshot truncates. Those gaps
  must not be called complete.

## Follow-up: Save All and Explorer visibility (source-level implementation)

- Save All scans every dirty native editor tab in both split panes, not just
  the selected tab. It saves file-backed documents without switching tabs;
  untitled documents are never written to a fabricated path, and the first
  one uses the existing identity-guarded Save As picker. Active IME preedit
  is skipped rather than accidentally committed; per-file errors are
  reported without discarding buffers or suppressing later file writes.
- The same ExplorerVisibility options now drive the lazy folder tree and
  the bounded fuzzy index (including the permanent filter and Ctrl+P).
  Show-ignored checks nested Gitignore rules through libgit2, while
  generated directories are excluded by default and permanently protected
  Git metadata remains excluded in either mode. On Windows the hidden-item
  toggle uses FILE_ATTRIBUTE_HIDDEN, not the presence of a leading dot.
- The Native-only settings JSON persists both visibility preferences with
  atomic compare-and-set and backward-compatible defaults; no Flutter
  preferences or Runtime Host API surface is changed. Invalidating an
  in-flight index scan also invalidates its request ticket to prevent old
  visibility options from resurfacing.
- **No GUI test performed in this slice by request.** Rust unit tests,
  Windows native typecheck and formatting are development checks only.

## Follow-up: guarded in-app drag-to-folder and Git decorations

- All three Explorer projections (filtered results, tree and flat list)
  attach a typed GPUI drag payload and accept it only on directory rows.
  An already selected dragged row moves the selected top-level group; a
  nonselected dragged row moves itself. Selected parent directories subsume
  their children; each payload carries its workspace generation.
- The drop handler rechecks target folder and workspace identity, Git
  metadata/symlink protection, folder cycles, filename collisions, and
  duplicate targets through the existing batch Cut preflight. It refuses
  moving a source containing an open Native Editor file, or any move while
  dirty buffers or an active external import are present. Unexpected
  mid-batch I/O failure can still produce a partial move; it is explicitly
  reported and the tree is refreshed rather than claiming rollback.
- Explorer file and folder rows now display compact U/A/M/R/D/! status
  labels derived from the existing read-only Git snapshot. Parent folder
  status aggregates the highest-priority changed descendant. Rendering
  does not issue per-row Git commands, and an absent/truncated snapshot
  never creates invented clean statuses. Explicit Explorer Refresh and
  paste/drag operations also refresh Git snapshot data.
- Focused Rust unit tests cover safe batch moves, collision/cycle/generation
  refusal, open-editor identity guards, badge mapping and ancestor priority.
  No UI/pointer testing or formal release acceptance was performed.
- Native file context now additionally exposes Open in Alera (promoting
  into a real guarded Editor tab) and View current Git changes (switches
  to the preexisting async Git preview). These reuse established
  workspace/path guards and do not open files through a new OS process.

## Follow-up: bounded multi-delete and precise external drop target

- Native Explorer Delete now operates on up to 24 selected items (tree, flat
  or filtered). Selected parent directories subsume selected children. A
  two-step UI confirmation captures the exact workspace generation and
  selected source paths, then validates the same selection before removal.
  Preflight audits *every* selected directory and rejects any protected
  Git metadata, symlink/junction, invalid path or overlarge subtree before
  deleting the first selected item. Source files open in either Native
  Editor pane and unsaved editor buffers block permanent deletion. Re-check
  all paths at confirmation to reduce time-of-check/time-of-use risks.
- The operation is explicitly labeled **Delete permanently**, not the
  Classic version's **Move to Trash**: Windows recycle-bin integration is
  pending and irreversible deletion must not be presented as recoverable.
  Unexpected mid-batch disk errors can still result in partial deletion;
  refresh the tree, tabs and file index and disclose the partial failure.
- OS file imports now resolve a *hovered* Explorer row as the target,
  independently from the last selected file: a folder row imports into
  that folder, a file row imports into its parent, and only blank Explorer
  space targets the workspace root. Missing, protected or outside-workspace
  target paths are refused, never replaced with a guessed root. A row-level
  GPUI drop handler stops event propagation to avoid duplicate root imports.
- This is source/compilation and focused Rust-model verification only.
  No Windows drag pointer or native GUI testing performed by request.

## Follow-up: reusable Alera-style Native modal and Settings

- Classic's `AleraDialog` and `AleraDialogHeader` already provide the
  canonical modal surface; Classic Settings uses 92% of the viewport up to
  1800x1280 with a 260px category sidebar and right-hand settings content.
- Native previously replaced the entire workbench with an absolute opaque
  full-screen settings panel rather than a modal. It now uses the reusable
  `AleraNativeModal` backdrop/frame/header in
  `desktop_gpui/src/native_modal_ui.rs`. The sizing policy lives in
  `desktop_gpui/src/native_modal.rs` and also supports future compact
  confirmation/picker dialogs. It keeps a margin on small windows.
- Settings now opens centered over a dimmed workbench, with a left category
  navigation and independently scrolling right panel. Narrow windows stack
  navigation horizontally. Close is icon-only; Escape and a backdrop click
  dismiss. Pointer events in the dialog do not dismiss or activate the
  underlying workbench. The modal receives its own keyboard focus and
  suppresses terminal/editor keystrokes while open.
- Existing Native-only persistent settings storage and compare-and-set
  callbacks are reused; the MCP `open_settings` command still targets
  exactly the requested section. No Classic preferences are written.
- Future Native confirm/input/selection dialogs can reuse the same shell.
  This change does not claim all Classic settings sections have been ported
  or that pointer/focus behavior has been GUI-tested.

## Follow-up: anchored dropdowns and common component library

- See `docs/gpui-native-component-library-20261108.md` for the Native
  components, GPUI 0.2.2 primitives and Classic controls used as references.
- Converted Explorer right-click, visibility, Git More, Git local-branch
  selection, existing Terminal Host attach, Terminal size, and Editor/Terminal
  right-click from inline labels/toolbars to a shared anchored popup.
- Converted Explorer file-action strips and Ctrl+P Quick Open into reusable
  compact/centered modals. Native Settings uses shared switches/buttons and
  the Explorer/Git sidebar uses shared segmented options.
- The new popup is an in-app GPUI element, not a Windows native shell menu.
  Do not claim native OS popup behavior or keyboard/pointer acceptance
  without actual Windows GUI testing. Existing Git/worktree/Editor behavior
  remains guarded by its own domain layer.
