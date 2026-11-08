# Alera GPUI — Flutter desktop UI/UX parity

This is the **first shell/visual parity pass**, not full product parity.
Reference: `lib/src/app/theme/alera_tokens.dart`,
`lib/src/app/theme/alera_dark_theme.dart`, the Flutter workbench sidebar
and native editor. All edits remain in the experimental GPUI branch.

## Applied in this pass

| Area | Flutter reference | GPUI first-pass result |
| --- | --- | --- |
| Surface palette | `bg #101010`, `surface #181818`, `surfaceVariant #202020`, `surfaceElevated #242424` | Shared `visual_tokens.rs`, shell and editor use neutral dark colors; no prototype blue-gray background |
| Text and borders | `foreground #F5F5F5`, `muted #A1A1A1`, `borderSubtle #272727` | Shared text/border tokens; Inter shell, JetBrains Mono retained for actual terminal/code |
| Shell rhythm | Top bar 52px, sidebar header 44px, status bar 30px | Top/section/Tab/status bands aligned to those dimensions |
| Left navigation | Default sidebar width 300px | Projects / worktrees rail 300px; compact inner gutters, active/hover background |
| Explorer / Git | Subtle dividers and compact file rows | 44px Explorer header, row density preserved, dark-neutral active backgrounds |
| Editor | Neutral text, cursor and theme | Removed blue prototype caret/selection/IME underline; still a limited editor |
| Tab strips | Workbench major-header alignment | 44px editor and terminal tab strips |

Two Rust regression tests verify the exact shared colors and shell dimensions
against Flutter's `AleraTokens` source, so new theme changes require deliberate
reconciliation.

## Must still be compared visually against the *running* Flutter release

- Brand row/icon, toolbar icon assets, active tab underline and hover states.
- Collapsible Projects / Worktrees sections, counts, pinned workspace layout
  and context menus; the GPUI list is not yet a complete Flutter parity view.
- Explorer / Git title/action density, tree indentation and file type icons.
- Terminal output typography, split divider/resize handles, draft/agent tabs
  and workbench navigation at 100%/125%/150% DPI.
- Light theme, settings-based appearance and persistent panel width.
- Full interaction/workflow parity: multiwindow tab drag, shortcuts, context
  menus, Agent/MCP panels, Git Graph, complex diff and accessibility.

None of those are certified by color/token tests or a compiling executable.
`supported=false` remains unchanged. The dirty Terminal IME validation branch
is intentionally separate and untouched; production `main` and existing Host
sessions are out of scope. No push, tag or release.
