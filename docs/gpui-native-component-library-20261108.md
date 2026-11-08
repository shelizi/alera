# Alera GPUI Native component library

## GPUI Kit adoption (in progress, 2026-11-08)

The dependency policy has changed: **use GPUI Kit 0.9.0 as the sole GPUI
runtime and styled component dependency**. This upgrades Alera Native from
GPUI 0.2.2 to Kit's pinned `gpui-pre 0.3.11`. Initialize with
`gpui_kit::application()`, `gpui_kit::init(cx)`, and
`gpui_kit::open_window(...)` so the Base Root can host framework overlays.
Do not add another direct `gpui` dependency.

Prefer `gpui_kit::component` for generic Button, Switch, TabBar, menus,
selects and dialogs; Alera owns only target identity, Git confirmation and
filesystem/Host authorization. The source migration now uses Kit Button
for action/select triggers and icon-toolbar hit targets (retaining Alera's
Codicon glyphs), Kit Switch in Native Settings, Kit TabBar
for the right Explorer/Git sidebar, and Kit DropdownMenu/PopupMenuItem
for visibility, branch selection, Git More, Terminal session attachment and
Terminal size. Windows `cargo check --offline --locked` passed after
updating GPUI 0.3 focus, timer and scroll APIs; this does **not** constitute
native UI acceptance.

Still to migrate: the remaining Explorer/Tab right-click paths in
`native_popup_ui.rs`, `native_modal_ui.rs` and Action/Quick Open modal.
Those are **temporary compatibility code**, not a new reusable design
system to maintain or extend. Preserve their guarded actions until Kit
PopupMenu / WindowExt dialog hosting replaces them with equivalent
keyboard, dismissal and focus behavior. Never disable the existing safe
two-click destructive actions just to simplify the UI.

The inventory below documents the *previous bespoke implementation* and
its behavior contract; it is not a recommendation to keep building new
native component primitives.

## Earlier native building blocks (historical pre-Kit implementation)

Previously pinned GPUI 0.2.2 exposed `gpui::anchored()` for viewport-fit popup
positioning, `gpui::deferred()` for popup z-order, `div()` layout and
event handling, and OS window/keyboard focus. It exposes platform application
menus, **not** a complete cross-platform anchored select, context-menu,
confirmation-dialog or dropdown field widget. Reuse its low-level primitives;
do not render a toolbar full of fake options to simulate a dropdown.

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
| `desktop_gpui/src/native_components.rs` | Reusable `menu_surface`, `menu_item`, `menu_separator`, `select_trigger`, `action_button`, `toggle_row`, `segmented_option`, `popup_at`; re-exports Native icon and modal components. No filesystem or Git side effects. |
| `desktop_gpui/src/native_popup.rs` | Pure typed `NativePopupState`, `NativePopupKind` and menu keyboard selection model; one transient popup at a time. |
| `desktop_gpui/src/native_popup_ui.rs` | One popup presentation layer and typed application callbacks. Uses GPUI `anchored`/`deferred`, click-outside dismissal, Escape, ArrowUp/ArrowDown and Enter; validates live target on action. |
| `desktop_gpui/src/native_modal.rs` | Pure viewport bounded Settings/Compact dialog sizing, with small-window layout policies. |
| `desktop_gpui/src/native_modal_ui.rs` | Shared `AleraNativeModal` dimmed backdrop, frame and header. |
| `desktop_gpui/src/native_action_dialog_ui.rs` | Explorer file create/rename/move/copy/duplicate/permanent-delete dialogs and Ctrl+P Quick Open dialog, reusing the shared modal shell. |

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
| Right sidebar Explorer/Git | Duplicated local tab styling | Reusable segmented option control |

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

## Future component-library backlog

- Reusable native dropdown **search/filter** for long lists, with paging and
  advanced keyboard selection. Initial dropdowns are finite/labeled only.
- General-purpose confirmation binding that owns explicit arm/confirm state,
  then migrate remaining Git/worktree sensitive inline confirmation affordances.
- Popup dynamic positioning against nontrivial scrolling parents, multi-window
  DPI and IME, and focus-trap acceptance on Windows/macOS.
- Standard text input and validation, toast/status message, virtualized
  list and tree controls; these require feature-specific semantics and should
  not be replaced by decorative abstractions.

Do not merge into Classic, push or release solely because `cargo check`
passes. Actual native GUI acceptance is separately gated.
