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

The current GPUI parity slice implements the workspace-name header, the
four global actions that are fully supported (New File, New Folder,
Collapse All, Refresh), the permanent IME/Unicode filter backed by the
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
- Classic toggles for ignored/hidden files and **Save All** need a true
  implementation; never add decorative buttons without a working handler.
- Ctrl/Shift multi-selection and batch Copy/Cut/Paste now have initial
  Native implementations across tree, flat list and filter results. Native
  uses bounded, workspace-scoped validation and preflight; actual mouse/
  keyboard acceptance and drag-to-folder / OS drag in/out parity remain.
- Single-click versus double-click now has an initial implementation in
  the Native tree, flat list, and filtered search results: one replaceable
  read-only preview, with a second click promoting into the editable Native
  Editor tab. The preview is not saved as a permanent tab. Pointer-level
  acceptance, exact Classic timing, large-file preview limits, editor reveal,
  and file-search context-menu parity remain incomplete.
- Full file context menu options (Open in Alera, Open with Default
  Application, external editor, reveal in system file manager, compare
  with latest revision/branch, nested Git source control root) are pending.
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
- Remaining: dedicated Explorer-scoped Ctrl+C/X/V keyboard shortcuts,
  native OS drag/drop, multi-file deletion UX, per-file Git badges and
  real Windows pointer acceptance. Those gaps must not be called complete.
