# Alera GPUI Native component library

## GPUI Kit adoption (source migration complete, 2026-11-09)

The dependency policy has changed: **use GPUI Kit 0.9.0 as the sole GPUI
runtime and styled component dependency**. This upgrades Alera Native from
GPUI 0.2.2 to Kit's pinned `gpui-pre 0.3.11`. Initialize with
`gpui_kit::application()`, `gpui_kit::init(cx)`, and
`gpui_kit::open_window(...)` so the Base Root can host framework overlays.
Do not add another direct `gpui` dependency.

After `gpui_kit::init(cx)`, call
`native_ui_theme::install_alera_dark_theme(cx)` **before** creating any
window. Kit 0.9.0 initializes in Light mode regardless of Alera's shell
colors; without this call its Button, TabBar, dropdown and Dialog surfaces
show bright white on the dark Alera UI. The adapter sets Kit's Dark mode and
aligns its semantic control backgrounds, borders, foregrounds and overlays
with Flutter Classic's `AleraTokens`. Do not patch individual Kit buttons
with hardcoded colors or sync to Windows Light appearance behind this policy.

Prefer `gpui_kit::component` for generic Button, Switch, TabBar, menus,
selects and dialogs; Alera owns only target identity, Git confirmation and
filesystem/Host authorization. The source migration now uses Kit Button
for action/select triggers and icon-toolbar hit targets (retaining Alera's
Codicon glyphs), Kit Switch in Native Settings, Kit TabBar
for the right Explorer/Git sidebar, and Kit DropdownMenu/PopupMenuItem
for visibility, branch selection, Git More, Terminal session attachment and
Terminal size. Explorer file rows (filtered, tree, list and blank area),
the primary mixed Editor/Terminal tab bar and the hidden legacy Terminal
tab strip now use Kit's `ContextMenuExt` with live target validation.
Explorer file operations, Quick Open and Native Settings use
`WindowExt::open_dialog` with Kit's overlay and focus lifecycle. The
home-grown modal/popup rendering and transient popup state were removed.

Windows `cargo check --offline --locked` passes with the upgraded GPUI
runtime; this does **not** constitute native mouse, keyboard, focus, IME or
accessibility acceptance. Preserve the original guarded operations, including
two-click destructive file and Git actions. Native remains `supported=false`.

The inventory below documents the *previous bespoke implementation* and
its behavior contract; it is not a recommendation to keep building new
native component primitives.

## Earlier native building blocks (historical pre-Kit implementation)

Previously pinned GPUI 0.2.2 exposed `gpui::anchored()` for viewport-fit popup
positioning, `gpui::deferred()` for popup z-order, `div()` layout and
event handling, and OS window/keyboard focus. It exposes platform application
menus, **not** a complete cross-platform anchored select, context-menu,
confirmation-dialog or dropdown field widget. This limitation no longer
applies to the current Kit dependency; do not rebuild these controls.

Classic Flutter sources remain the UX contract:

- `lib/src/design_system/layout/alera_dialog.dart` and
  `alera_dialog_header.dart` — bounded modal, shared surface, close affordance.
- `lib/src/design_system/forms/alera_dropdown_field.dart` and
  `lib/src/design_system/menus/alera_dropdown_entry.dart` — trigger/value,
  selectable entry, popover and selection semantics.
- `lib/src/features/settings/presentation/settings_dialog.dart` —
  modal Settings with left navigation and right configuration pane.

## Native modules

| Module | API and ownership |
| --- | --- |
| `desktop_gpui/src/native_components.rs` | Kit Button adapters and the feature-specific Quick Open results row; no custom popup/overlay or file-system side effects. |
| `desktop_gpui/src/native_popup_ui.rs` | GPUI Kit `ContextMenuExt` and `DropdownMenu` application callbacks only; validates the exact Explorer path, Git branch, or Host/Editor tab before action. |
| `desktop_gpui/src/native_modal.rs` | UI-agnostic responsive Settings layout thresholds; no custom modal rendering. |
| `desktop_gpui/src/native_action_dialog_ui.rs` | Explorer guarded file dialogs and Ctrl+P Quick Open rendered in Kit `WindowExt::open_dialog`. |
| `desktop_gpui/src/native_settings_ui.rs` | Kit Dialog for MCP and toolbar Settings, Kit Switch, and existing durable settings compare-and-set. |

## Integration inventory

| Native feature | Before | After |
| --- | --- | --- |
| Explorer file/folder right-click | Sidebar-relative handcrafted absolute list | Top-layer anchored context menu under the actual pointer |
| Explorer visibility | Permanent inline show/hide panel | Anchored checkable dropdown |
| Git More (Stash / Apply / conflict review) | Second row of unlabeled icon actions | Labeled dropdown; same original guarded two-click Git actions |
| Local Git branch selection | Textbox only, guess branch names | Bounded dropdown of local branch names; choice fills existing textbox, never switches without explicit Switch |
| Host terminal attach | Inline session list | Host-confirmed session dropdown |
| Mixed Editor/Terminal and Terminal tabs | Horizontally scrolling action bars | Right-click context popup under mouse, guarded by tab identity |
| Terminal sizes | Bare `80×24` and `120×36` text links | Reusable dropdown with two sizes |
| Explorer create/rename/move/copy/duplicate/delete | Inline action form occupying file list | Compact reusable modal with explicit confirmation and cancellation |
| Ctrl+P Quick Open | Inline Explorer search/results panel | Centered modal with indexed fuzzy results, selection, keyboard close/open |
| Native Settings | Standalone textual `On · change` and bare `+`/`−` | Shared switch and button; existing durable Native preferences compare-and-set unchanged |
| Right sidebar Explorer/Git | Duplicated local tab styling | Kit segmented TabBar control |

## Behavioral invariants

- Never interpret a Git branch dropdown selection as authorization to
  switch, merge, push or fetch. The existing explicit controls still own
  these actions.
- Opening a context menu never operates on a stale Explorer path, Host
  session or Editor tab. Typed callbacks revalidate their target.
- Opening a menu closes the preceding menu; opening Settings invalidates
  transient menus. Escape and clicking outside dismiss; menu keyboard
  events must not reach Editor or Terminal.
- Explorer permanent deletion remains **permanent** and explicitly says it
  is *not* Windows Recycle Bin. The original bounded path audit, blocked open
  files and two-step confirmation stay in the existing data layer.
- Native settings write only their own state directory; Classic remains
  untouched. The MCP `open_settings` command still addresses a section.
- Dynamic menu content (branch snapshot/session list) can be stale before
  refresh; an empty or loading state must not synthesize an option.
- Do not call these visual migrations production-ready until Windows
  mouse/keyboard/focus acceptance passes. Native remains `supported=false`.

## Kit integration acceptance backlog

- Windows real GUI smoke for context menus: right-click on Explorer
  file/blank/tree and the mixed tab strip, keyboard navigation, dismissal,
  nested scroll, DPI, and stale-session refusal.
- Kit Dialog real GUI smoke: open Settings via toolbar and Host MCP, switch
  sections, focus without repeated stealing, escape/backdrop close, Ctrl+P
  selection, and Explorer delete's two-click confirmation.
- Prefer Kit's long-list Select, virtualized list/tree, input and notification
  primitives when feature parity calls for them. Do not build replacements.
- Add explicit functional regression coverage for Kit overlay stacking when
  MCP opens Settings while another file/Quick Open dialog is present.

Do not merge into Classic, push or release solely because `cargo check`
passes. Actual native GUI acceptance is separately gated.
