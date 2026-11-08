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
- Ctrl/Shift multiselection, copy/cut/paste on multiple selected items,
  drag-to-folder and drag into/out of the OS need targeted parity testing.
- Classic file single-click preview versus second-click permanent tab
  behavior, editor reveal, and file-search result context menu parity
  remain separate work.
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
