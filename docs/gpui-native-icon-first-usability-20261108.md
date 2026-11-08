# Alera GPUI Native — icon-first workbench usability pass

**Branch**: `integrate/gpui-p0p1p2-20261108`. This is a Native UI
experiment. Flutter Classic and Runtime Host behavior remain unchanged.

## Motivation

The previous UI rendered most actions as separate English text buttons,
often in rows that wrapped or disappeared in narrow sidebars. Explorer
actions, Git status and Git action controls appeared simultaneously. The
left side duplicated saved projects and worktree entries while Runtime and
Agent troubleshooting controls consumed significant vertical space.

## Changes

- Reuse the bundled **VS Code Codicons** font (already used by Flutter
  Classic), loaded through GPUI's text system. Semantic `UiIcon` names
  keep a single mapping to the bundled version's actual codepoints.
- Create one consistent **30×30 icon button** with hover highlight and
  descriptive GPUI tooltip. Do not replace meaningful labels with mystery
  icons: the selected Explorer/Git page, source branch, file paths and
  confirmation/status text remain visible.
- Right sidebar: **Explorer / Git tabs** share the same width but do not
  display their long panels simultaneously. Git scrolls independently.
- Explorer: compact folder view, parent, refresh, new file/folder, rename,
  delete, copy/cut/paste/history actions; infrequent move/copy-into/
  duplicate in a **More** menu. Quick Open and project-content search now
  share one action strip; empty search controls do not waste height.
- Git: icon-based Graph, Refresh, Commit, Switch, Fetch, Compare, Merge,
  Pull, Push and More. Less-frequent Stash and conflict inspection are
  behind More. The underlying non-force and 2-click confirmation logic
  is unchanged; armed states visibly switch to a check icon and preserve
  explicit warning text.
- Center workspace: Terminal and Editor navigation remain labeled;
  new Terminal, untitled file, and attach become icons. Editor Save,
  Save As, Close and confirmed Discard share a compact action strip;
  existing PTY/Editor functionality is unchanged.
- Terminal: VT/Text view, Auto read, Refresh, Copy selected and Copy
  visible use icons; explicit numeric viewport presets remain text.
- Projects: inline project expand chevron next to the project name,
  with Open/Refresh/Pin controls as icons. Saved Project and managed Git
  worktree administration are available behind the Projects settings icon,
  collapsed by default. Runtime/Agent developer controls are behind an
  independently collapsible Runtime/Agents section.

## Verification and limitations

The first resulting Windows Native build was launched on a **disposable
local Git preview repository**, with isolated Native state and a deliberately
missing CLI/Runtime Host. The real window screenshot confirmed Codicons
render as icons (not missing-glyph boxes), Explorer/Git navigation tabs
appear, the compact Explorer toolbar is visible, and Runtime/Agents
starts collapsed.

The subsequent project-management collapse must be independently rebuilt
and checked. Clicking across all new hover tooltips, using a fully
authenticated Host, constrained window sizes, and interaction accessibility
remain follow-up tasks; icon-first alone is not full Classic UX parity.

Avoid `main` merge, production push, release and tag until Native is
fully accepted; `supported=false` remains in force.
