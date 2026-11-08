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
- Explorer: the header now holds **only New File, New Folder, Search and
  Refresh**. Rename, copy, cut, paste, duplicate, move, delete and Git
  history appear as **text-labelled right-click options on the clicked
  file/folder**, not as permanent icon rows. Right-clicking blank space
  opens a current-folder menu with New File/Folder, Paste, tree navigation
  and search. A popup is clamped inside the Explorer panel; clicking away,
  switching the active file or changing workspace dismisses it. Existing
  guarded file operations and explicit delete confirmation are reused.
  File Quick Open remains available with Ctrl+P. Content Search is
  available from the blank-area menu, with Search inside its input panel.
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

The project-management collapse was subsequently compiled and checked.
The right-click Explorer menu was type-checked, with the existing
268/268 Rust model regressions passing. The contextual popup's precise
position and click routing still require acceptance: an isolated Windows
preview launched and confirmed the Explorer now has only four persistent
toolbar icons, but remote pointer injection did not activate even the
ordinary Git tab or file-opening click during that attempt. The real
right-click menu therefore **must not yet be reported as GUI-tested**.
Use a reliable interactive Windows input session to confirm it. Using a
fully authenticated Host, constrained window sizes
and accessibility acceptance are separate follow-up tasks. Icon-first
alone is not full Classic UX parity.

Avoid `main` merge, production push, release and tag until Native is
fully accepted; `supported=false` remains in force.
