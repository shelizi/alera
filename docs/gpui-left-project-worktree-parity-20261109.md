# Alera GPUI left sidebar parity with Classic (2026-11-09)

## Port to latest main GPUI integration baseline

- Staged on `feat/gpui-left-sidebar-port-20261109` created directly from `integrate/gpui-p0p1p2-20261108` at `0977adde0`. Do not merge the stale `fix/gpui-smoke-navigation-terminal-20261108` branch: it is 106 commits behind and contains unrelated Terminal/IME work.
- Ported Rust RuntimeStore/CLI project/workspace mutations through a conflict-free three-way merge against the original ancestor, retaining all newer main-branch CLI/settings capabilities.
- Ported Native RuntimeBridge Classic project/workspace catalog, managed Worktree lifecycle, archive/tag/section/sleep/rename/preview APIs **alongside** the existing Host-authorized shared desktop settings / Git favorites bridge. No replacement of the native settings dialog, settings Core contract or existing Terminal/editors.
- Ported standalone catalog parsing, cycle-safe nested Worktree ordering, protected WorktreeManager and preview-only UI fold data, with Native library unit tests.
- New mainline Native left project tree loads Classic catalog asynchronously with local fallback, displays registered project/workspace names, sections, tags, archive status and parent indentation; supports GPUI Kit project/workspace context menus.
- Existing global Classic/GPUI Settings stays owned by the main integration; this port does not introduce a duplicate Settings screen.
- Project context menu supports open/pin/unpin/open folder/copy path and New Worktree from a selected repository. Creation of a Classic registered Worktree uses existing Host lifecycle; local-only Git repositories can create fresh/checkout-existing branches.
- Workspace context menu offers open, folder/copy, pin/unpin, archive/restore, existing-tag and section assignment, guarded two-step Sleep and clean Host-managed remove. Additional full Classic commands already exist in RuntimeBridge; remaining UI parity is tracked as follow-up.
- Keep the old source worktree uncommitted/untouched; the mainline integration branch must receive only validated, scoped commits after checks.
- This is **code integration**, not a claim of Windows visual testing or full feature parity.

## GPUI mainline continuation: rename, workspace trees and guarded project removal

- Ported project/workspace **rename display name** actions into GPUI Kit right-click menus using existing Classic `project rename` and `workspace rename` CLI / RuntimeStore. The native input is a lightweight rename field, not an alternative global Settings UI. Neither Git branch nor project path is renamed.
- Ported parent selection, attach within one project, and detach from a workspace. Host rejects invalid/cyclic relations; the hierarchical Native tree refreshes after success.
- Added `Pin workspace tree` and `Unpin workspace tree` across registered same-project descendants, skipping already matching rows and explicitly reporting partial-update counts if a later Host call fails. These multiple writes are **not atomic**.
- Added two-stage `Forget project…` via existing `project.remove.preview` and `RuntimeCliBridge::remove_project_record`. Native refuses the active project; the bridge independently rejects linked Git Worktrees, live sessions and stale impact previews. Source files and Git branches are kept.
- The mainline shared Classic/GPUI settings Core, settings modal, Git favorites and Terminal/Editor implementation remain unchanged.
- Checks: Native GPUI `cargo check` passed after these changes; the 349/349 Native library suite passed before the final project removal menu, which is also covered by the subsequent native compilation check. A full Windows GUI acceptance remains pending.

## GPUI mainline continuation: Classic tag/section lifecycle

- Added Native GPUI Kit menu entries to create a shared Workspace tag, create a Workspace section, assign new entries immediately to the chosen Workspace, and remove the selected tag/section with **two separate confirmations**.
- The existing Classic `workspaceTag.create`, `tag remove`, `workspace section-create`, and `workspace section-remove` CLI/Host contracts remain authoritative. GPUI only holds transient edit/confirm UI state.
- Shared tag creation is two Host mutations (create then assign). If assignment fails after successful creation, an unassigned shared tag may remain; error is reported and user can retry the assignment. This remains a follow-up for transactional Host workflow.
- The global Classic/GPUI Settings feature is not duplicated or altered. Dedicated field-structured per-project settings and Windows UI acceptance remain outstanding.
- GPUI native `cargo check` passed after these additions; full library regression is recorded in the final verification result.

## Scope and baseline

- Working tree: `.worktrees/fix-gpui-smoke-navigation-terminal-20261108` (no recent MCP activity at selection time).
- Classic references: `lib/src/features/workbench/presentation/project_workbench_sidebar*.dart`, `project_workbench_workspace_context_menu.dart`, `project_workbench_sidebar_actions.dart`.
- GPUI references: `desktop_gpui/src/main.rs`, `project_tree.rs`, `worktree_manager.rs`, `session.rs`.
- Do not modify Classic, the active `integrate/gpui-p0p1p2-20261108` worktree or the right-sidebar Git parity branch.
- This worktree had pre-existing, uncommitted Terminal IME edits in `main.rs`, `lib.rs` and `terminal_ime_ui.rs`. Leave them intact; stage only sidebar-specific changes if committing.

## Reconciliation with primary GPUI integration settings (2026-11-09)

Verified the active integration branch `integrate/gpui-p0p1p2-20261108` at commit `80ba44ffe` already contains:
- `native_settings_ui.rs`, `native_settings_catalog_ui.rs`, and `native_shared_settings_ui.rs`: GPUI Kit settings dialog, Classic settings catalog, and shared Rust Runtime Host readback/compare-and-set flow.
- `runtime_bridge.rs::desktop_settings_snapshot` and related migration/compare-and-set functions. Classic/GPUI shared global settings are owned by `alera_core::desktop_settings_contract` and must **not** be reimplemented or replaced by this sidebar worktree.
- The checked-in integration contract contains 28 shared setting paths; the integration worktree has an uncommitted extension adding `editor.quickOpenExcludedDirectories` as path 29. Preserve this WIP and do not overwrite it.
- The native settings catalog explicitly displays `Not available in GPUI yet` for unsupported controls; a catalog row is not proof a control can edit.

**Separate scope:** this sidebar branch's `project config-show/config-set` and lightweight ProjectConfig JSON input concern **per-project RuntimeStore configuration** (worktree setup/provider/etc.), not the shared **global desktop settings** catalog. Reuse the integration branch's GPUI Kit modal/controls when bringing project configuration into the main native shell. Do not cherry-pick this branch's standalone GPUI shell or settings implementation over the integration branch. Port only the genuinely missing left-sidebar functionality, CLI/Host project-workspace operations and tests.

## Implemented in this pass

- Add managed Git Worktree from any repository group in the left project tree, without first switching the active project.
- Explicit creation mode: create a new branch from HEAD, or check out an existing local branch into its own managed worktree.
- Validate branch existence and duplicate checkout; do not reset branches or select a remote-tracking branch implicitly.
- Right-click project rows to pin/unpin a project. Unpinning a repository group handles saved linked checkout paths as well as its primary path.
- Right-click worktree rows to switch or remove, reusing the guarded deletion flow, with double confirmation.
- Keep the former standalone Worktree section for backward compatibility until the unified tree layout and dialogs are ready.
- Native GPUI binary `cargo check` and `cargo fmt --check` passed. Targeted `worktree_manager::tests`: 6/6 passed. Full GPUI library suite (`cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib`) passed: 186/186 tests.

## 2026-11-09 continuation: RuntimeStore read-side integration

- Added bounded, read-only `runtime_catalog.rs` parsing Classic CLI `project list --json` and `workspace list --all --json`. Paths and project membership are checked, stale or removed records ignored, and Git worktree inventories cached per project.
- `RuntimeCliBridge::list_runtime_catalog` uses the installed existing Alera CLI with bounded subprocess lifetime/output, without launching a Host or accessing SQLite directly. If unavailable or invalid, GPUI retains its local preview catalog.
- Registered Classic projects appear in the left tree, preserving their human-readable names. Registered checkout rows show Classic workspace name, pin marker, tags and archived state. The sidebar reports Classic-synced vs local-preview fallback.
- Explicit workspace pin/unpin action in the Worktree right-click menu calls the Classic CLI to update RuntimeStore; the local project favorite remains a separate preview feature.
- A Classic-registered workspace is protected from GPUI's direct Git removal path, avoiding orphaned RuntimeStore records. Host-managed removal/cleanup remains outstanding.
- Validation: native GPUI binary `cargo check` passed, `cargo fmt --check` passed; full Rust library suite passed 191/191 after Host-managed create integration and per-project Git inventory caching.
- Existing Terminal IME modifications and generated terminal E2E artifacts are unrelated pre-existing work and must not be included in an isolated left-sidebar commit.

## Host-managed creation follow-up

- Creating a Worktree for a Classic-registered Git project now delegates to the existing `alera workspace add` CLI / Host-managed `workspace.createManaged` lifecycle, including workspace registry updates and project setup behavior.
- New vs existing branch maps to the CLI `--reuse-existing-branch` switch. The returned workspace path is accepted only if Git confirms it is a linked checkout of the selected project.
- The GPUI does not silently fall back to Git-only worktree creation when the Classic Host rejects creation. For unregistered local projects, the existing guarded `WorktreeManager` remains available. When the CLI exists but registry load failed, creation pauses for refresh rather than risk orphaning a registered workspace.
- Host-managed removal is intentionally not implemented yet, so registered workspaces remain protected from the native Git-only removal path.

## 2026-11-09 continuation: Classic sidebar mutations

- Exposed missing `alera project rename --id ... --name ...` and `alera workspace rename --id ... --name ...` commands in the existing Rust CLI, preserving RuntimeStore/Host request authority. No Git branch/path rename is attempted.
- Added GPUI inline rename input and right-click project/workspace Rename actions. Names are validated, persisted via existing CLI, and refreshed only on success.
- Read Classic `tag list` and attached `tagIds` to the Runtime catalog. A registered Worktree menu can assign/remove existing tags using `workspace tag` / `untag`, with max six quick actions shown.
- Added a two-step parent selection: right-click a registered workspace and choose "Use as parent", then right-click another workspace within the same project and select "Attach to selected parent"; existing links can be detached. These operations invoke Classic `workspace link` / `unlink`. Parent names appear on child rows, while the full nested tree rendering is still pending.
- Managed registered Worktree removal now uses the existing `workspace remove --keep-branch` Host lifecycle, after explicit double-click confirmation and the shared non-destructive Git clean/safety preflight. The native Git-only route remains for unregistered Worktrees. The branch is retained and the Host owns the RuntimeStore record cleanup.
- Added tests for invalid rename/tag/parent inputs, untrusted IDs, and non-destructive Worktree preflight including dirty-worktree refusal.
- CLI `cargo check` passed after adding rename subcommands; GPUI binary `cargo check` passed after parent action changes; full GPUI Rust library suite 197/197 passed; GPUI/CLI `cargo fmt --check` and `git diff --check` passed.

## 2026-11-09 extended left-sidebar parity implementation

- Unified visual project/Worktree tree: order primary, pinned, custom Classic workspace sections, and actual parent-child indentation; orphaned/cyclic relationships cannot drop Worktrees. The old standalone Saved Projects / Recent / Git Worktrees lists are hidden (the Worktree creation form remains).
- Native project tree fold state is now stored as a bounded, validated **view preference only** in `project-folds.json`, separately from Classic's canonical RuntimeStore registry.
- Migrated this isolated GPUI worktree from standalone GPUI 0.2.2 to GPUI Kit 0.9 (same as ongoing Native integration), including API updates to the Terminal IME/editor focus, timer and list-scroll calls, and installed Alera dark semantic color overrides for Kit widgets. This migration changes the isolated Native shell; it has not been cherry-picked to the active integration tree.
- Both project and Worktree right-click surfaces now use real GPUI Kit context menus. Project actions include open, rename, pin, settings, create Worktree, open folder, copy path, and Forget Project. Worktree actions include switch, open folder, copy path, rename, pin, archive/restore, tag create/assign/unassign/global delete, section create/assign/clear/delete, parent link/unlink, sleep, and remove managed checkout. Legacy inline contexts are logically disabled, but their previous implementation remains to remove during integration cleanup.
- Classic `workspace archive/restore` uses a new atomic `RuntimeStore::set_workspace_archived` and Host `workspace.setArchived` request to preserve pin, tags, branch, and workspace relations while changing archive status. Added CLI lifecycle commands plus section-list/create/set/remove.
- Added Classic project settings read/save through `project config-show/config-set`, using the original `ProjectConfig` schema and Host `projectConfig.upsert`. The Native editor currently offers a lightweight single-line JSON input, **not** a fully field-structured settings panel. Leave configuration unchanged on invalid JSON.
- Project removal uses existing authenticated Host `project.remove.preview` with workspace, tab, live-session and config counts, then a second explicit confirmation. It fails closed for active project, any active Host sessions, still-linked Git Worktrees or changes between impact previews; it removes metadata and native recent/pin hints but never deletes source or Git branches.
- Workspace Sleep now exposes the existing Host `workspace.sleep` action with two-stage confirmation, limited to **inactive** workspace rows in Native. It closes Host tabs/sessions, not Git worktrees or project files.
- Shared tag creation delegates to Host `workspaceTag.create` to enforce case-insensitive name uniqueness, then assigns to the selected Workspace; global tag deletion requires an additional explicit confirmation and clears Classic assignments.
- Section deletion also requires two-step confirmation and returns its members to Others. Other native context actions use validated IDs and existing Classic CLI contracts.
- Compilation and test results must be refreshed after final latest changes; no GUI visual acceptance or actual destructive operation test has been claimed.
- Warning: the worktree contains preexisting Terminal IME edits, some new GPUI Kit migration changes and the left-sidebar implementation. Keep Terminal IME/other concurrent feature commits isolated; **do not commit or merge the whole dirty tree**.

### Follow-up: complete tree coverage and Classic bulk pinning

- Verified new `RuntimeStore::set_workspace_archived` regression with `alera-core --features runtime`: **1/1 passing**; Host sleep idempotency regression **1/1 passing**.
- Fixed Classic ProjectConfig editor's 512-byte search-text limit: dedicated 32 KiB input cap, UTF-8 byte selection offsets (especially Chinese/emoji), and bounded settings viewport. It is still a JSON field rather than a structured form.
- The unified project tree now synthesizes a **view-only primary row for registered non-Git folder workspaces**; never creates a Git Worktree for folders. Git projects show up to the WorktreeManager's full 96-row bound instead of silently truncating to 40.
- Added `workspace_family_ids` bounded, same-project descendant traversal, including cycle and orphan guards. Native Worktree context menu offers Classic-equivalent **Pin workspace tree / Unpin workspace tree**. These operations apply each workspace pin individually via Classic CLI, skip already matching rows and explicitly report partial update errors. They are not atomic across multiple Host requests.
- Added focused tests for a registered plain folder, same-project descendant traversal, cross-project rejection and cycles; GPUI library tests **207/207 passing** at this revision. Native compilation was rerun after fixing a borrowed `ProjectTreeGroup` partial move; use the final verification result below, not the previous pre-fix failure.
- Worktree contains unrelated active Terminal/IME modifications and an existing untracked terminal e2e fixture; do not stage or commit these as sidebar-only changes.

### Unfinished UX / acceptance
- Classic's highly structured project settings modal is represented by JSON input only. It still needs dedicated GPUI Kit form controls, multiline editing and field-level schema UX.
- Native currently does not expose Classic's full project-level bulk Sleep or descendant batch-pin workflows. Workspace-level Sleep and pin are available; bulk actions need a more complete Host-aware preflight.
- Manual section drag reorder, additional external editor/browser integrations, and full visual polish/localization are not yet equivalent to Classic.
- Verify on a Windows GUI with multiple projects, parent/child Worktrees, invalid/corrupt RuntimeStore records, one dirty active workspace, an offline Host and a restarting Host. In particular, verify menu positioning, keyboard accessibility, sleep and removal confirmations and display/theme contrast.
- New CLI subcommands require a matching rebuilt Alera CLI binary; otherwise the Native UI must show refusal, not silently mutate any Git-only fallback.

## Remaining for functional parity

1. **Single source of truth.** Classic RuntimeStore project/workspace reads and explicit workspace pin/unpin are now connected via CLI. GPUI still retains a separate `PreviewSessionStore` for local favorites/history; creation of Classic-registered Worktrees now uses Host-managed lifecycle, while local-only projects use the guarded Git flow. Migrate mutations to Host-managed project/workspace contracts and subscribe to changed events before treating both models as equivalent.
2. **Project management.** Display alias/rename now persists in Classic; safe project removal preview/cleanup, project settings, project ordering/sections, and project-level favorite/visibility lifecycle remain.
3. **Workspace metadata.** Rename, individual pin/unpin, existing-tag assignment and parent link/unlink now persist through Classic. Still needed: descendant batch pin, creating/deleting tags, section management, true parent-child nesting, archived/restore and sleep states. Keep data changes in RuntimeStore, not GPUI preview JSON.
4. **Classic contextual actions.** Open folder, copy path, open external editor/browser, and cursor-anchored GPUI Kit context menu with native confirmation modal (current GPUI context actions remain inline).
5. **UX parity.** Collapse the duplicate Saved Projects and Git Worktrees sections into a single unified tree only after their operations have migrated, add contextual menus and localized compact labels, persist expansion state, and support branch selection instead of typing only.
6. **Acceptance checks.** Exercise Windows GUI with two projects, primary + linked worktrees, dirty/locked/active removal guards, restart persistence and Host workspace/PTY continuity. No UI acceptance is claimed yet.

## Safety

- Do not auto-delete, force-remove, clean, reset, stash or override existing worktrees.
- New worktrees must use a validated branch name and managed `.worktrees` path. Removal requires explicit confirmation and a clean working directory; Git branch remains.
- Keep work in an unused GPUI worktree and verify before cherry-picking into the active integration branch.
