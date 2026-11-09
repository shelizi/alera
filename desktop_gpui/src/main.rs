//! Experimental Alera desktop shell powered by native GPUI (no Flutter/WebView).
mod editor_ui;
mod native_action_dialog_ui;
mod native_components;
mod native_popup_ui;
mod native_settings_ui;
mod native_ui_icons;
mod native_ui_theme;
mod quick_open_ui;

// GPUI Kit re-exports its pinned GPUI runtime. Use a single version of all
// GPUI types throughout Native instead of mixing gpui 0.2.x and gpui-pre.
extern crate gpui_kit as gpui;

use alera_gpui_desktop::agent_profiles::{
    matching_agent_handle, AgentProfileRow, LaunchedAgentTab,
};
use alera_gpui_desktop::edition::NATIVE_APP_NAME;
use alera_gpui_desktop::editor::EditorDraftStore;
use alera_gpui_desktop::editor_tabs::{EditorTabs, SaveAllReport, MAX_EDITOR_TABS};
use alera_gpui_desktop::explorer_drag::ExplorerDragBatch;
use alera_gpui_desktop::explorer_git_badges::{ExplorerGitBadge, ExplorerGitBadges};
use alera_gpui_desktop::explorer_ops::{
    ExplorerClipboardBatch, ExplorerClipboardMode, ExplorerDeleteBatch, ExplorerFiles,
};
use alera_gpui_desktop::explorer_selection::ExplorerMultiSelection;
use alera_gpui_desktop::explorer_visibility::ExplorerVisibilityOptions;
use alera_gpui_desktop::file_search::WorkspaceFileIndex;
use alera_gpui_desktop::git_actions::{GitActions, GitConflictChoice, GitConflictPlan};
use alera_gpui_desktop::git_graph_layout::{layout_git_graph, GraphCommit};
use alera_gpui_desktop::git_history::{
    compare_git_refs, compare_local_branch, preview_commit, GitHistoryOptions, GitHistorySnapshot,
};
use alera_gpui_desktop::git_inspect::{read_file_diff, GitSnapshot};
use alera_gpui_desktop::mcp_ui_bridge::{
    matching_host_tab_handle, NativeMcpUiBridge, NativeMcpUiCommand, NativeMcpUiEvent,
};
use alera_gpui_desktop::mcp_workspace_creation::HostManagedWorkspaceRequest;
use alera_gpui_desktop::model::WorkbenchModel;
use alera_gpui_desktop::native_modal::NativeModalSize;
use alera_gpui_desktop::native_settings::{NativeSettings, NativeSettingsStore};
use alera_gpui_desktop::project_search::{ContentMatch, ProjectSearch};
use alera_gpui_desktop::project_tree::{collect_project_tree, ProjectTreeGroup};
use alera_gpui_desktop::runtime_bridge::RuntimeCliBridge;
use alera_gpui_desktop::session::{PreviewSessionStore, TerminalLayout, TerminalLayoutTab};
use alera_gpui_desktop::terminal::{
    encode_terminal_key, estimate_terminal_viewport, raw_key_reply_may_drain,
    select_registered_host_workspace, terminal_ime_anchor_px, TerminalAutoResizeGate,
    TerminalCreateFlow, TerminalCreateTick, TerminalDock, TerminalLineSelection,
    TerminalPollCadence, TerminalPollGate, TerminalTabContextTarget, TerminalViewMode,
    TerminalViewportProposal, HOST_SESSION_LIST_SCOPE,
};
use alera_gpui_desktop::terminal_panes::{TerminalPaneLayout, TerminalSplitDirection};
use alera_gpui_desktop::terminal_stream_bridge::{
    NativeTerminalStreamBridge, StreamViewUpdate, TerminalStreamRetryGate,
};
use alera_gpui_desktop::visual_tokens::{
    ACTIVE, BG, BORDER, BORDER_ACTIVE, ELEVATED, MUTED, PANEL, PROJECTS_SIDEBAR_WIDTH,
    RIGHT_SIDEBAR_WIDTH, SECTION_HEADER_HEIGHT, STATUS_BAR_HEIGHT, TAB_STRIP_HEIGHT, TEXT, TOOLBAR,
    TOP_BAR_HEIGHT,
};
use alera_gpui_desktop::vt_screen::StyledRow;
use alera_gpui_desktop::workbench_layout_store::{
    SavedWorkbenchAxis, SavedWorkbenchLayoutV1, SavedWorkbenchPane, WORKBENCH_LAYOUT_VERSION,
};
use alera_gpui_desktop::workbench_panes::{PaneAxis, PaneSlot, WorkbenchPaneLayout};
use alera_gpui_desktop::workbench_tabs::{
    WorkbenchTab, WorkbenchTabIdentity, WorkbenchTabKind, WorkbenchTabs,
};
use alera_gpui_desktop::workspace_tree::WorkspaceTree;
use alera_gpui_desktop::worktree_manager::{
    matching_branch_worktree, WorktreeEntry, WorktreeManager,
};
use editor_ui::{bind_editor_keys, NativeEditor};
use gpui::{
    actions, div, prelude::*, px, rgb, size, App, Bounds, Context, Entity, Focusable, KeyBinding,
    PathPromptOptions, Window, WindowBounds, WindowOptions,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::IconName;
use gpui_kit::component::WindowExt as _;
use native_components::{icon, icon_button, UiIcon};
use native_ui_icons::explorer_file_icon;
use quick_open_ui::{bind_quick_open_keys, QuickOpenInput};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

actions!(
    alera_gpui_workbench,
    [
        ToggleFileSearch,
        ToggleContentSearch,
        ExplorerCopy,
        ExplorerCut,
        ExplorerPaste,
        ExplorerSelectAll
    ]
);

/// Deliberately small desktop shell: terminal first, basic editor on demand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum CenterView {
    #[default]
    Terminal,
    Editor,
}

#[derive(Clone)]
struct TerminalTabDrag {
    handle: String,
    workspace_generation: u64,
    /// The drag must never select a newer Host tab that recycled this handle.
    target: Option<TerminalTabContextTarget>,
}

struct TerminalTabDragPreview {
    label: String,
}

#[derive(Clone)]
struct MixedTabDrag {
    identity: WorkbenchTabIdentity,
    workspace_generation: u64,
}

impl gpui::Render for TerminalTabDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .px_3()
            .py_1()
            .text_sm()
            .child(self.label.clone())
    }
}

/// Compact Classic-style Git state indicator; rendering performs no Git I/O.
fn explorer_status_badge(badge: Option<ExplorerGitBadge>) -> gpui::Div {
    if let Some(badge) = badge {
        div()
            .ml_1()
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(badge.color()))
            .child(badge.label())
    } else {
        div().h(px(0.0))
    }
}

#[derive(Clone, Debug)]
enum GitUiAction {
    Stage(PathBuf),
    Unstage(PathBuf),
    StageMany(Vec<PathBuf>),
    UnstageMany(Vec<PathBuf>),
    Commit(String),
    SwitchBranch(String),
    MergeWithoutConflicts(String),
    Fetch,
    StashSave,
    StashApply,
    PullFfOnly,
    PushUpstream,
    MergeWithChoices(GitConflictPlan, Vec<(String, GitConflictChoice)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfirmGitAction {
    StashSave,
    StashApply,
    PullFfOnly,
    PushUpstream,
}

impl ConfirmGitAction {
    fn action(self) -> GitUiAction {
        match self {
            Self::StashSave => GitUiAction::StashSave,
            Self::StashApply => GitUiAction::StashApply,
            Self::PullFfOnly => GitUiAction::PullFfOnly,
            Self::PushUpstream => GitUiAction::PushUpstream,
        }
    }

    fn warning(self) -> &'static str {
        match self {
            Self::StashSave =>
                "Confirm Stash: move tracked index/worktree changes into a new stash; untracked files stay",
            Self::StashApply =>
                "Confirm Apply: restore the newest stash only into a clean worktree; the stash entry will remain",
            Self::PullFfOnly =>
                "Confirm Pull: fetch configured upstream and fast-forward only; never rebase or create a merge commit",
            Self::PushUpstream =>
                "Confirm Push: push current branch only to its configured upstream; never force-push",
        }
    }
}

#[derive(Clone)]
enum GitReadRequest {
    Commit(String),
    Compare(String),
}

/// Shared actions for branch context menus in the Native Git Graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitGraphBranchMenuAction {
    History,
    Compare,
    OpenWorktree,
    Switch,
    Merge,
    CopyName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExplorerAction {
    NewFile,
    NewFolder,
    Rename,
    Move,
    Copy,
    Duplicate,
    Delete,
}

#[derive(Clone, Copy)]
enum MixedContextAction {
    Focus,
    Close,
    CloseOthers,
    CloseToRight,
    MoveLeft,
    MoveRight,
    SecondPane,
    FirstPane,
}

impl ExplorerAction {
    fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New file",
            Self::NewFolder => "New folder",
            Self::Rename => "Rename",
            Self::Move => "Move into folder",
            Self::Copy => "Copy into folder",
            Self::Duplicate => "Duplicate",
            Self::Delete => "Delete permanently",
        }
    }
}

/// Secondary Explorer actions are discoverable from one text menu instead
/// of being permanently rendered as multiple rows of tiny icons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExplorerMenuAction {
    File(ExplorerAction),
    Clipboard(ExplorerClipboardMode),
    Paste,
    OpenInAlera,
    ViewCurrentGitDiff,
    CopyAbsolutePath,
    CopyRelativePath,
    CollapseFolder,
    Refresh,
    GitHistory,
}

impl ExplorerMenuAction {
    fn label(self) -> &'static str {
        match self {
            Self::File(ExplorerAction::Rename) => "Rename…",
            Self::File(ExplorerAction::Move) => "Move to folder…",
            Self::File(ExplorerAction::Copy) => "Copy into folder…",
            Self::File(ExplorerAction::Duplicate) => "Duplicate",
            Self::File(ExplorerAction::Delete) => "Delete permanently…",
            Self::File(ExplorerAction::NewFile) => "New file…",
            Self::File(ExplorerAction::NewFolder) => "New folder…",
            Self::Clipboard(ExplorerClipboardMode::Copy) => "Copy selection",
            Self::Clipboard(ExplorerClipboardMode::Cut) => "Cut selection",
            Self::Paste => "Paste",
            Self::OpenInAlera => "Open in Alera",
            Self::ViewCurrentGitDiff => "View current Git changes",
            Self::CopyAbsolutePath => "Copy path",
            Self::CopyRelativePath => "Copy relative path",
            Self::CollapseFolder => "Collapse folder",
            Self::Refresh => "Refresh",
            Self::GitHistory => "File / folder Git history",
        }
    }
}

#[derive(Clone, Debug)]
struct ExplorerContextMenu {
    /// None means the user right-clicked empty space, not a stale selection.
    path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RightSidebarView {
    Explorer,
    Git,
}

struct Workbench {
    model: WorkbenchModel,
    center_view: CenterView,
    right_sidebar_view: RightSidebarView,
    explorer_context_menu: Option<ExplorerContextMenu>,
    show_project_management: bool,
    runtime_details_open: bool,
    tree: Option<WorkspaceTree>,
    tree_mode: bool,
    explorer_visibility: ExplorerVisibilityOptions,
    explorer_selection: Option<PathBuf>,
    explorer_multi_selection: ExplorerMultiSelection,
    /// A read-only Explorer preview can cover an editor viewport without
    /// dropping its existing dirty buffer, tab identity, or undo history.
    explorer_preview_active: bool,
    explorer_keyboard_focus: gpui::FocusHandle,
    explorer_clipboard: Option<ExplorerClipboardBatch>,
    explorer_drop_busy: bool,
    explorer_drop_request: u64,
    explorer_action: Option<ExplorerAction>,
    explorer_action_input: Option<Entity<QuickOpenInput>>,
    explorer_action_modal_focus: gpui::FocusHandle,
    explorer_delete_confirmed: bool,
    explorer_delete_pending: Option<ExplorerDeleteBatch>,
    editor: Option<Entity<NativeEditor>>,
    secondary_editor: Option<Entity<NativeEditor>>,
    editor_transfer_in_progress: bool,
    editor_file_layout_snapshot: Vec<(u64, PathBuf)>,
    editor_active_layout_snapshot: Option<u64>,
    session_store: Option<PreviewSessionStore>,
    recent_workspaces: Vec<PathBuf>,
    saved_projects: Vec<PathBuf>,
    project_tree_groups: Vec<ProjectTreeGroup>,
    collapsed_project_roots: HashSet<PathBuf>,
    project_tree_loading: bool,
    project_tree_request: u64,
    show_recent_workspaces: bool,
    worktrees: Vec<WorktreeEntry>,
    worktree_error: Option<String>,
    worktree_loading: bool,
    worktree_busy: bool,
    worktree_create_input: Option<Entity<QuickOpenInput>>,
    worktree_delete_pending: Option<PathBuf>,
    picking_workspace: bool,
    quick_open: Option<Entity<QuickOpenInput>>,
    explorer_filter_input: Entity<QuickOpenInput>,
    file_index: Option<Arc<WorkspaceFileIndex>>,
    file_index_loading: bool,
    file_index_request: u64,
    content_query: Option<Entity<QuickOpenInput>>,
    content_loading: bool,
    content_matches: Vec<ContentMatch>,
    content_truncated: bool,
    content_error: Option<String>,
    content_searched_query: String,
    content_request: u64,
    git_snapshot: Option<GitSnapshot>,
    git_selected_changes: HashSet<alera_gpui_desktop::git_inspect::GitChangeRow>,
    git_selection_anchor: Option<alera_gpui_desktop::git_inspect::GitChangeRow>,
    git_history: Option<GitHistorySnapshot>,
    git_history_open: bool,
    git_history_options: GitHistoryOptions,
    git_history_loading: bool,
    git_history_error: Option<String>,
    git_history_request: u64,
    git_merge_armed: Option<String>,
    git_sensitive_armed: Option<ConfirmGitAction>,
    git_conflict_preview: Option<GitConflictPlan>,
    git_conflict_choices: HashMap<String, GitConflictChoice>,
    git_conflict_selected: Option<String>,
    git_conflict_loading: bool,
    git_conflict_request: u64,
    git_conflict_confirm_armed: bool,
    git_filter_input: Entity<QuickOpenInput>,
    git_commit_input: Entity<QuickOpenInput>,
    git_branch_input: Entity<QuickOpenInput>,
    git_action_busy: bool,
    git_action_request: u64,
    git_action_status: String,
    git_loading: bool,
    git_error: Option<String>,
    git_diff: Option<String>,
    git_diff_loading: bool,
    git_diff_request: u64,
    workspace_generation: u64,
    editor_epoch: u64,
    mixed_tabs: WorkbenchTabs,
    mixed_panes: WorkbenchPaneLayout,
    mixed_layout_restore: Option<SavedWorkbenchLayoutV1>,
    mixed_layout_read_only: bool,
    mixed_tab_context: Option<WorkbenchTabIdentity>,
    runtime_bridge: Option<RuntimeCliBridge>,
    runtime_status: String,
    runtime_loading: bool,
    mcp_ui_bridge: Option<NativeMcpUiBridge>,
    mcp_ui_window_id: String,
    mcp_ui_status: String,
    mcp_ui_ready: bool,
    mcp_host_workspaces: Vec<alera_gpui_desktop::terminal::RegisteredHostWorkspace>,
    mcp_host_workspaces_loading: bool,
    mcp_host_workspaces_request: u64,
    mcp_compare_busy: bool,
    mcp_compare_request: u64,
    mcp_workspace_create_busy: bool,
    mcp_workspace_create_request: u64,
    native_settings_store: Option<NativeSettingsStore>,
    native_settings: NativeSettings,
    native_settings_error: Option<String>,
    settings_section: Option<String>,
    settings_modal_focus: gpui::FocusHandle,
    settings_kit_dialog_open: bool,
    last_auto_git_fetch: Instant,
    agent_profiles: Vec<AgentProfileRow>,
    agent_profiles_loading: bool,
    agent_profiles_request: u64,
    agent_selected_profile: Option<String>,
    agent_prompt_input: Entity<QuickOpenInput>,
    agent_launch_busy: bool,
    agent_launch_request: u64,
    agent_pending_tab: Option<LaunchedAgentTab>,
    agent_status: String,
    terminals: TerminalDock,
    terminal_panes: TerminalPaneLayout,
    terminal_input: Entity<QuickOpenInput>,
    secondary_terminal_input: Entity<QuickOpenInput>,
    terminal_keyboard_focus: gpui::FocusHandle,
    secondary_terminal_keyboard_focus: gpui::FocusHandle,
    terminal_key_queue: VecDeque<String>,
    terminal_key_queue_target: Option<TerminalTabContextTarget>,
    terminal_key_sending: bool,
    terminal_auto_resize: TerminalAutoResizeGate,
    terminal_line_selection: Option<TerminalLineSelection>,
    terminal_create_flow: TerminalCreateFlow,
    terminal_layout_restore: Option<TerminalLayout>,
    terminal_list_loading: bool,
    terminal_read_loading: Option<String>,
    terminal_split_read_loading: bool,
    terminal_poll: TerminalPollGate,
    terminal_poll_cadence: TerminalPollCadence,
    terminal_stream: Option<NativeTerminalStreamBridge>,
    terminal_stream_retry: TerminalStreamRetryGate,
    terminal_stream_view_snapshot: Vec<(String, u64)>,
    terminal_tick: u64,
}

impl Workbench {
    /// A pending composition or ASCII duplicate marker belongs to the
    /// previous Host tab, not the next pane/Workspace selection.
    fn clear_terminal_text_inputs(&mut self, cx: &mut Context<Self>) {
        self.terminal_input.update(cx, |input, cx| input.clear(cx));
        self.secondary_terminal_input
            .update(cx, |input, cx| input.clear(cx));
    }

    /// Returns true when this was a multi-selection gesture, which must not
    /// also open or expand the clicked row. All views share visible ordering.
    fn select_explorer_row(
        &mut self,
        path: &Path,
        visible: &[PathBuf],
        ctrl: bool,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .explorer_multi_selection
            .click(path, visible, ctrl, shift)
        {
            return false;
        }
        self.explorer_selection = Some(path.to_path_buf());
        self.explorer_delete_confirmed = false;
        self.explorer_delete_pending = None;
        window.focus(&self.explorer_keyboard_focus, cx);
        if ctrl || shift {
            self.explorer_context_menu = None;
            self.model.set_status(format!(
                "Selected {} Explorer item(s)",
                self.explorer_multi_selection.len()
            ));
            cx.notify();
            return true;
        }
        false
    }

    fn click_explorer_file(
        &mut self,
        path: &Path,
        permanent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.explorer_context_menu = None;
        self.explorer_selection = Some(path.to_path_buf());
        self.explorer_delete_confirmed = false;
        self.explorer_delete_pending = None;
        if permanent {
            if self.model.open_path(path) {
                self.explorer_preview_active = false;
                self.open_editor(cx);
                if let Some(editor) = self.focused_editor() {
                    window.focus(&editor.read(cx).focus_handle(cx), cx);
                }
                self.persist_session();
            }
        } else if self.model.preview_path(path) {
            self.explorer_preview_active = true;
            self.center_view = CenterView::Editor;
            window.focus(&self.explorer_keyboard_focus, cx);
        }
        cx.notify();
    }

    fn refresh_project_tree(&mut self, cx: &mut Context<Self>) {
        let saved = self.saved_projects.clone();
        let recent = self.recent_workspaces.clone();
        let active = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.project_tree_request = self.project_tree_request.wrapping_add(1);
        let request = self.project_tree_request;
        self.project_tree_loading = true;
        cx.spawn(async move |view, cx| {
            let groups = cx
                .background_spawn(async move { collect_project_tree(&saved, &recent, &active) })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.project_tree_request != request {
                    return;
                }
                view.project_tree_groups = groups;
                view.project_tree_loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn update_explorer_visibility(
        &mut self,
        next: ExplorerVisibilityOptions,
        cx: &mut Context<Self>,
    ) {
        let previous = self.explorer_visibility;
        if previous == next {
            return;
        }
        if let Some(tree) = &mut self.tree {
            if let Err(error) = tree.set_visibility_options(next) {
                self.model
                    .set_status(format!("Explorer visibility update failed: {error}"));
                cx.notify();
                return;
            }
        }
        if let Some(store) = &self.native_settings_store {
            match store.compare_and_set_explorer_visibility(previous, next) {
                Ok(settings) => self.native_settings = settings,
                Err(error) => {
                    if let Some(tree) = &mut self.tree {
                        let _ = tree.set_visibility_options(previous);
                    }
                    self.model.set_status(format!(
                        "Explorer visibility unchanged; settings could not be saved: {error}"
                    ));
                    cx.notify();
                    return;
                }
            }
        } else {
            // A missing native state path must not write Flutter preferences.
            self.native_settings.explorer_show_hidden = next.show_hidden;
            self.native_settings.explorer_show_ignored = next.show_ignored;
        }
        self.explorer_visibility = next;
        self.explorer_multi_selection.clear();
        self.explorer_selection = None;
        self.explorer_context_menu = None;
        self.tree_mode = true;
        self.file_index = None;
        self.file_index_loading = false;
        self.file_index_request = self.file_index_request.wrapping_add(1);
        if !self
            .explorer_filter_input
            .read(cx)
            .query()
            .trim()
            .is_empty()
            || self.quick_open.is_some()
        {
            self.ensure_file_index(cx);
        }
        self.model.set_status(format!(
            "Explorer: {} ignored files, {} hidden items",
            if next.show_ignored { "show" } else { "hide" },
            if next.show_hidden { "show" } else { "hide" },
        ));
        cx.notify();
    }

    fn set_explorer_clipboard(&mut self, mode: ExplorerClipboardMode, cx: &mut Context<Self>) {
        let Some(selected) = self.explorer_selection.clone() else {
            self.model
                .set_status("Select a file or folder to Copy or Cut");
            cx.notify();
            return;
        };
        let paths = match self.explorer_multi_selection.top_level_paths() {
            selected_paths if !selected_paths.is_empty() => selected_paths,
            _ => vec![selected],
        };
        let result = ExplorerFiles::new(self.model.root())
            .and_then(|files| ExplorerClipboardBatch::capture(&files, &paths, mode));
        match result {
            Ok(clipboard) => {
                let count = clipboard.len();
                self.explorer_clipboard = Some(clipboard);
                self.model.set_status(format!(
                    "{} {count} Explorer item(s); choose destination and Paste",
                    if mode == ExplorerClipboardMode::Copy {
                        "Copied"
                    } else {
                        "Cut"
                    }
                ));
            }
            Err(error) => self
                .model
                .set_status(format!("Explorer Copy/Cut refused: {error}")),
        }
        cx.notify();
    }

    fn paste_explorer_clipboard(&mut self, cx: &mut Context<Self>) {
        if self.editor_dirty(cx) {
            self.model
                .set_status("Save unsaved editor changes before Explorer Paste");
            cx.notify();
            return;
        }
        let Some(clipboard) = self.explorer_clipboard.as_mut() else {
            self.model
                .set_status("Explorer clipboard is empty; Copy or Cut a file first");
            cx.notify();
            return;
        };
        let root = self.model.root().to_path_buf();
        let folder = self
            .explorer_selection
            .as_ref()
            .map(|path| {
                if path.is_dir() {
                    path.clone()
                } else {
                    path.parent().unwrap_or(&root).to_path_buf()
                }
            })
            .unwrap_or_else(|| root.clone());
        let result =
            ExplorerFiles::new(&root).and_then(|files| clipboard.paste_into(&files, &folder));
        let consumed = clipboard.consumed();
        if consumed {
            self.explorer_clipboard = None;
        }
        match result {
            Ok(paths) => {
                self.explorer_selection = paths.last().cloned();
                self.explorer_multi_selection.clear();
                self.model.refresh_directory();
                if let Some(tree) = &mut self.tree {
                    let _ = tree.refresh();
                }
                self.file_index = None;
                self.content_matches.clear();
                self.persist_session();
                self.model
                    .set_status(format!("Explorer Paste succeeded: {} item(s)", paths.len()));
                self.refresh_git(cx);
            }
            Err(error) => {
                // Batch preflight prevents predictable collisions, but an
                // unexpected I/O failure can still occur after one item was
                // written. Refresh the view without discarding the remaining
                // clipboard entries or claiming atomic filesystem rollback.
                self.model.refresh_directory();
                if let Some(tree) = &mut self.tree {
                    let _ = tree.refresh();
                }
                self.file_index = None;
                self.content_matches.clear();
                self.model
                    .set_status(format!("Explorer Paste refused or stopped: {error}"));
                self.refresh_git(cx);
            }
        }
        cx.notify();
    }

    fn drop_explorer_batch(
        &mut self,
        drag: &ExplorerDragBatch,
        folder: &Path,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_drop_busy || self.editor_dirty(cx) {
            self.model
                .set_status("Save dirty editor files and finish imports before dragging items");
            cx.notify();
            return;
        }
        let open_paths: Vec<PathBuf> = self
            .all_editor_files_and_active(cx)
            .0
            .into_iter()
            .map(|(_, path)| path)
            .collect();
        if drag.affects_open_files(&open_paths) {
            self.model
                .set_status("Close open Editor files before moving their paths with Explorer drag");
            cx.notify();
            return;
        }
        let result = ExplorerFiles::new(self.model.root())
            .and_then(|files| drag.move_into(&files, folder, self.workspace_generation));
        // A batch is preflighted against predictable conflicts before the
        // first mutation; unexpected disk errors may leave a partial move.
        self.model.refresh_directory();
        if let Some(tree) = &mut self.tree {
            let _ = tree.refresh();
        }
        self.file_index = None;
        self.file_index_loading = false;
        self.file_index_request = self.file_index_request.wrapping_add(1);
        self.content_matches.clear();
        match result {
            Ok(paths) => {
                self.explorer_multi_selection.clear();
                self.explorer_selection = paths.last().cloned();
                self.explorer_context_menu = None;
                self.persist_session();
                self.model
                    .set_status(format!("Moved {} Explorer item(s) to folder", paths.len()));
            }
            Err(error) => self.model.set_status(format!(
                "Explorer drop refused/stopped: {error}; refresh checked for partial disk changes"
            )),
        }
        self.refresh_git(cx);
        cx.notify();
    }

    fn import_explorer_drop(
        &mut self,
        sources: Vec<PathBuf>,
        hovered_row: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_drop_busy {
            self.model
                .set_status("Finish the previous external file import first");
            cx.notify();
            return;
        }
        if self.editor_dirty(cx) {
            self.model
                .set_status("Save unsaved editor changes before importing files");
            cx.notify();
            return;
        }
        if sources.is_empty() || sources.len() > 8 {
            self.model
                .set_status("Drop between one and eight files/folders at a time");
            cx.notify();
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.explorer_drop_request = self.explorer_drop_request.wrapping_add(1);
        let request = self.explorer_drop_request;
        self.explorer_drop_busy = true;
        self.model
            .set_status(format!("Importing {} dropped item(s)…", sources.len()));
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                let files = ExplorerFiles::new(&root)?;
                // The target comes from the actual hovered row. Selection
                // belongs to keyboard operations and is unrelated to OS drop.
                let destination = files.external_drop_folder(hovered_row.as_deref())?;
                let mut imported = Vec::new();
                for path in sources {
                    imported.push(files.import_external(&path, &destination)?);
                }
                Ok::<_, std::io::Error>(imported)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.explorer_drop_request != request {
                    return;
                }
                view.explorer_drop_busy = false;
                match result {
                    Ok(imported) => {
                        view.model.refresh_directory();
                        if let Some(tree) = &mut view.tree { let _ = tree.refresh(); }
                        view.explorer_multi_selection.clear();
                        view.explorer_selection = imported.last().cloned();
                        view.model.set_status(format!("Imported {} external file/folder item(s)", imported.len()));
                        view.persist_session();
                    }
                    Err(error) => {
                        view.model.set_status(format!(
                            "External import stopped: {error}. Earlier completed items remain; original files were not moved"
                        ));
                        view.model.refresh_directory();
                        if let Some(tree) = &mut view.tree { let _ = tree.refresh(); }
                    }
                }
                view.file_index = None;
                view.file_index_loading = false;
                view.file_index_request = view.file_index_request.wrapping_add(1);
                view.refresh_git(cx);
                cx.notify();
            }).ok();
        }).detach();
    }

    fn run_explorer_menu_action(
        &mut self,
        action: ExplorerMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(menu) = self.explorer_context_menu.take() {
            self.explorer_selection = menu.path;
        }
        match action {
            ExplorerMenuAction::File(action) => self.begin_explorer_action(action, window, cx),
            ExplorerMenuAction::Clipboard(mode) => self.set_explorer_clipboard(mode, cx),
            ExplorerMenuAction::Paste => self.paste_explorer_clipboard(cx),
            ExplorerMenuAction::OpenInAlera => {
                if let Some(path) = self.explorer_selection.clone() {
                    self.click_explorer_file(&path, true, window, cx);
                }
            }
            ExplorerMenuAction::ViewCurrentGitDiff => {
                if let Some(path) = self
                    .explorer_selection
                    .as_ref()
                    .filter(|path| path.is_file())
                {
                    if let Ok(relative) = path.strip_prefix(self.model.root()) {
                        let relative = relative.to_path_buf();
                        self.right_sidebar_view = RightSidebarView::Git;
                        self.preview_git_change(relative, cx);
                    }
                }
            }
            ExplorerMenuAction::CopyAbsolutePath | ExplorerMenuAction::CopyRelativePath => {
                let Some(path) = self
                    .explorer_selection
                    .as_ref()
                    .filter(|path| path.starts_with(self.model.root()) && path.exists())
                else {
                    self.model
                        .set_status("Choose an existing Explorer item first");
                    cx.notify();
                    return;
                };
                let text = if action == ExplorerMenuAction::CopyRelativePath {
                    path.strip_prefix(self.model.root())
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/")
                } else {
                    path.to_string_lossy().into_owned()
                };
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                self.model
                    .set_status(if action == ExplorerMenuAction::CopyRelativePath {
                        "Copied path relative to the project root"
                    } else {
                        "Copied full file/folder path"
                    });
                cx.notify();
            }
            ExplorerMenuAction::CollapseFolder => {
                if let Some(folder) = self
                    .explorer_selection
                    .as_ref()
                    .filter(|path| path.is_dir())
                {
                    if let Some(tree) = &mut self.tree {
                        if let Ok(relative) = folder.strip_prefix(self.model.root()) {
                            if let Err(error) = tree.collapse(relative) {
                                self.model
                                    .set_status(format!("Collapse folder refused: {error}"));
                            }
                        }
                    }
                }
                cx.notify();
            }
            ExplorerMenuAction::Refresh => {
                self.model.refresh_directory();
                if let Some(tree) = &mut self.tree {
                    if let Err(error) = tree.refresh() {
                        self.model
                            .set_status(format!("Explorer refresh failed: {error}"));
                    }
                }
                self.refresh_git(cx);
                self.file_index = None;
                if !self
                    .explorer_filter_input
                    .read(cx)
                    .query()
                    .trim()
                    .is_empty()
                {
                    self.ensure_file_index(cx);
                }
                cx.notify();
            }
            ExplorerMenuAction::GitHistory => self.open_explorer_selection_history(cx),
        }
    }

    fn begin_explorer_action(
        &mut self,
        action: ExplorerAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            action,
            ExplorerAction::Rename
                | ExplorerAction::Move
                | ExplorerAction::Copy
                | ExplorerAction::Duplicate
                | ExplorerAction::Delete
        ) && self.explorer_selection.is_none()
        {
            self.model
                .set_status("Select a file or folder in Explorer first");
            cx.notify();
            return;
        }
        self.explorer_action = Some(action);
        self.quick_open = None;
        // If another Kit dialog was opened by a shortcut/MCP before this
        // action, it must not remain as an empty layer beneath the new one.
        if window.has_active_dialog(cx) {
            window.close_all_dialogs(cx);
        }
        self.explorer_delete_confirmed = false;
        self.explorer_delete_pending = None;
        self.explorer_action_input =
            if matches!(action, ExplorerAction::Delete | ExplorerAction::Duplicate) {
                None
            } else {
                let placeholder = match action {
                    ExplorerAction::NewFile => "New file name",
                    ExplorerAction::NewFolder => "New folder name",
                    ExplorerAction::Rename => "New name",
                    ExplorerAction::Move => "Destination folder inside workspace",
                    ExplorerAction::Copy => "Copy into destination folder inside workspace",
                    ExplorerAction::Duplicate | ExplorerAction::Delete => unreachable!(),
                };
                let input = cx.new(|cx| QuickOpenInput::with_placeholder(placeholder, cx));
                window.focus(&input.read(cx).focus_handle(cx), cx);
                Some(input)
            };
        if self.explorer_action_input.is_none() {
            window.focus(&self.explorer_action_modal_focus, cx);
        }
        self.open_kit_explorer_action_dialog(window, cx);
        cx.notify();
    }

    fn cancel_explorer_action(&mut self, cx: &mut Context<Self>) {
        self.explorer_action = None;
        self.explorer_action_input = None;
        self.explorer_delete_confirmed = false;
        self.explorer_delete_pending = None;
        cx.notify();
    }

    fn apply_explorer_batch_delete(&mut self, cx: &mut Context<Self>) {
        if self.explorer_drop_busy || self.editor_dirty(cx) {
            self.model
                .set_status("Save editor changes and finish imports before deleting files");
            cx.notify();
            return;
        }
        let selection = self.explorer_multi_selection.top_level_paths();
        let selection = if selection.is_empty() {
            self.explorer_selection.iter().cloned().collect()
        } else {
            selection
        };
        let files = match ExplorerFiles::new(self.model.root()) {
            Ok(files) => files,
            Err(error) => {
                self.model
                    .set_status(format!("Explorer delete refused: {error}"));
                cx.notify();
                return;
            }
        };
        let open_files: Vec<PathBuf> = self
            .all_editor_files_and_active(cx)
            .0
            .into_iter()
            .map(|(_, path)| path)
            .collect();
        if !self.explorer_delete_confirmed {
            match ExplorerDeleteBatch::capture(&files, &selection, self.workspace_generation) {
                Ok(batch) if !batch.affects_open_files(&open_files) => {
                    self.model.set_status(format!(
                        "Permanently delete {} selected item(s), including folder contents? This cannot be undone. Press CONFIRM PERMANENT DELETE to proceed.",
                        batch.len(),
                    ));
                    self.explorer_delete_confirmed = true;
                    self.explorer_delete_pending = Some(batch);
                }
                Ok(_) => self.model.set_status(
                    "Close any Native Editor files inside the selection before permanent deletion",
                ),
                Err(error) => self
                    .model
                    .set_status(format!("Explorer delete preflight refused: {error}")),
            }
            cx.notify();
            return;
        }

        // The second click is a separate user gesture. Re-audit the exact
        // originally armed selection, Editor paths and workspace generation.
        self.explorer_delete_confirmed = false;
        let Some(batch) = self.explorer_delete_pending.take() else {
            self.model
                .set_status("Delete confirmation expired; choose the items again");
            cx.notify();
            return;
        };
        if !batch.matches_selection(&files, &selection, self.workspace_generation)
            || batch.affects_open_files(&open_files)
        {
            self.model
                .set_status("Delete selection or workspace changed; no files were deleted");
            cx.notify();
            return;
        }
        let result = batch.execute(&files, self.workspace_generation);
        // An unexpected I/O error can occur after a successful earlier
        // removal. Never claim rollback; reconcile actual disk state.
        for target in batch.paths() {
            if !target.exists() {
                self.model.forget_path_and_children(target);
            }
        }
        self.model.refresh_directory();
        if let Some(tree) = &mut self.tree {
            let _ = tree.refresh();
        }
        self.file_index = None;
        self.file_index_loading = false;
        self.file_index_request = self.file_index_request.wrapping_add(1);
        self.content_matches.clear();
        self.content_searched_query.clear();
        self.explorer_multi_selection.clear();
        self.explorer_selection = None;
        match result {
            Ok(count) => {
                self.cancel_explorer_action(cx);
                self.persist_session();
                self.model
                    .set_status(format!("Permanently deleted {count} Explorer item(s)"));
            }
            Err(error) => self.model.set_status(format!(
                "Explorer delete refused or partially completed: {error}. Check refreshed files."
            )),
        }
        self.refresh_git(cx);
        cx.notify();
    }

    fn apply_explorer_action(&mut self, cx: &mut Context<Self>) {
        let Some(action) = self.explorer_action else {
            return;
        };
        if action == ExplorerAction::Delete {
            self.apply_explorer_batch_delete(cx);
            return;
        }
        let selected = self.explorer_selection.clone();
        let root = self.model.root().to_path_buf();
        let input = self
            .explorer_action_input
            .as_ref()
            .map(|input| input.read(cx).query().trim().to_owned())
            .unwrap_or_default();

        if matches!(
            action,
            ExplorerAction::Rename
                | ExplorerAction::Move
                | ExplorerAction::Copy
                | ExplorerAction::Duplicate
                | ExplorerAction::Delete
        ) && self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).switch_allowed().is_err())
        {
            self.model
                .set_status("Save or discard editor changes before file operations");
            cx.notify();
            return;
        }
        let source = selected.unwrap_or_else(|| self.model.directory().to_path_buf());
        let parent = if source.is_dir() {
            source.clone()
        } else {
            source
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| root.clone())
        };
        let result = ExplorerFiles::new(&root).and_then(|files| match action {
            ExplorerAction::NewFile => files.create_file(&parent, &input).map(Some),
            ExplorerAction::NewFolder => files.create_folder(&parent, &input).map(Some),
            ExplorerAction::Rename => files.rename(&source, &input).map(Some),
            ExplorerAction::Move => {
                let destination = root.join(&input);
                files.move_into(&source, &destination).map(Some)
            }
            ExplorerAction::Copy => {
                let destination = root.join(&input);
                files.copy_into(&source, &destination).map(Some)
            }
            ExplorerAction::Duplicate => files.duplicate(&source).map(Some),
            ExplorerAction::Delete => unreachable!("delete batch is handled above"),
        });
        match result {
            Ok(target) => {
                if matches!(
                    action,
                    ExplorerAction::Rename | ExplorerAction::Move | ExplorerAction::Delete
                ) {
                    // Preserve all unrelated editor tabs. Any document whose
                    // original file moved was clean above; its disk-identity
                    // guard will refuse a stale-path save until reopened.
                    self.model.forget_path_and_children(&source);
                }
                self.model.refresh_directory();
                if let Some(tree) = &mut self.tree {
                    if let Err(error) = tree.refresh() {
                        self.model
                            .set_status(format!("Tree refresh failed: {error}"));
                    }
                }
                self.file_index = None;
                self.content_matches.clear();
                self.content_searched_query.clear();
                self.explorer_multi_selection.clear();
                self.explorer_selection = target.clone();
                self.cancel_explorer_action(cx);
                self.persist_session();
                self.model.set_status(format!(
                    "{} succeeded{}",
                    action.label(),
                    target
                        .as_ref()
                        .map(|target| format!(": {}", target.display()))
                        .unwrap_or_default()
                ));
                if action == ExplorerAction::NewFile {
                    if let Some(path) = target {
                        if self.model.open_path(&path) {
                            self.open_editor(cx);
                        }
                    }
                }
                self.refresh_git(cx);
            }
            Err(error) => {
                self.model
                    .set_status(format!("{} refused: {error}", action.label()));
                // A failed attempt must never retain an armed delete.
                self.explorer_delete_confirmed = false;
            }
        }
        cx.notify();
    }

    /// Each pane owns a separate native viewport and its own EditorTabs.
    /// Dirty buffers are moved between owners, never cloned or reopened.
    fn editor_for_pane(&self, pane: PaneSlot) -> Option<Entity<NativeEditor>> {
        match pane {
            PaneSlot::Primary => self.editor.clone(),
            PaneSlot::Secondary => self.secondary_editor.clone(),
        }
    }

    fn editor_for_instance(
        &self,
        instance: u64,
        cx: &Context<Self>,
    ) -> Option<(PaneSlot, Entity<NativeEditor>)> {
        for slot in [PaneSlot::Primary, PaneSlot::Secondary] {
            if let Some(editor) = self.editor_for_pane(slot) {
                if editor.read(cx).contains_tab_instance(instance) {
                    return Some((slot, editor));
                }
            }
        }
        None
    }

    fn focused_editor(&self) -> Option<Entity<NativeEditor>> {
        self.editor_for_pane(self.mixed_panes.focused_pane())
            .or_else(|| self.editor.clone())
            .or_else(|| self.secondary_editor.clone())
    }

    fn all_editor_files_and_active(
        &self,
        cx: &Context<Self>,
    ) -> (Vec<(u64, PathBuf)>, Option<u64>) {
        let mut files = Vec::new();
        for slot in [PaneSlot::Primary, PaneSlot::Secondary] {
            if let Some(editor) = self.editor_for_pane(slot) {
                files.extend(editor.read(cx).file_backed_tab_instances());
            }
        }
        let active = self
            .focused_editor()
            .and_then(|editor| editor.read(cx).active_tab_instance());
        (files, active)
    }

    fn observe_native_editor(&mut self, editor: &Entity<NativeEditor>, cx: &mut Context<Self>) {
        let workspace_generation = self.workspace_generation;
        cx.observe(editor, move |workbench, _, cx| {
            if workbench.workspace_generation != workspace_generation
                || workbench.editor_transfer_in_progress
            {
                return;
            }
            if workbench
                .editor
                .as_ref()
                .is_some_and(|entity| entity.read(cx).tabs_empty())
            {
                workbench.editor = None;
            }
            if workbench
                .secondary_editor
                .as_ref()
                .is_some_and(|entity| entity.read(cx).tabs_empty())
            {
                workbench.secondary_editor = None;
            }
            let (files, active) = workbench.all_editor_files_and_active(cx);
            let changed = files != workbench.editor_file_layout_snapshot
                || active != workbench.editor_active_layout_snapshot;
            workbench.editor_file_layout_snapshot = files;
            workbench.editor_active_layout_snapshot = active;
            if changed && workbench.mixed_layout_restore.is_none() {
                workbench.sync_mixed_tabs(cx);
                workbench.save_mixed_pane_layout(cx);
            }
            cx.notify();
        })
        .detach();
    }

    fn install_editor(&mut self, tabs: EditorTabs, cx: &mut Context<Self>) {
        if self.editor.is_none() && self.secondary_editor.is_none() {
            self.editor_epoch = self.editor_epoch.wrapping_add(1);
        }
        let editor = cx.new(|cx| NativeEditor::new(tabs, cx));
        editor.update(cx, |editor, cx| {
            editor.set_local_tab_strip_visible(false, cx);
            editor.set_tab_size(self.native_settings.editor_tab_size, cx);
        });
        self.editor = Some(editor.clone());
        (
            self.editor_file_layout_snapshot,
            self.editor_active_layout_snapshot,
        ) = self.all_editor_files_and_active(cx);
        self.observe_native_editor(&editor, cx);
    }

    fn install_secondary_editor(&mut self, tabs: EditorTabs, cx: &mut Context<Self>) {
        let editor = cx.new(|cx| NativeEditor::new(tabs, cx));
        editor.update(cx, |editor, cx| {
            editor.set_local_tab_strip_visible(false, cx);
            editor.set_tab_size(self.native_settings.editor_tab_size, cx);
        });
        self.secondary_editor = Some(editor.clone());
        (
            self.editor_file_layout_snapshot,
            self.editor_active_layout_snapshot,
        ) = self.all_editor_files_and_active(cx);
        self.observe_native_editor(&editor, cx);
    }

    fn select_git_change(
        &mut self,
        row: alera_gpui_desktop::git_inspect::GitChangeRow,
        visible: &[alera_gpui_desktop::git_inspect::GitChangeRow],
        toggle: bool,
        shift: bool,
    ) {
        if !visible.contains(&row) {
            return;
        }
        if shift {
            if let Some(anchor) = self.git_selection_anchor.as_ref() {
                if anchor.area == row.area {
                    if let (Some(from), Some(to)) = (
                        visible.iter().position(|entry| entry == anchor),
                        visible.iter().position(|entry| entry == &row),
                    ) {
                        if !toggle {
                            self.git_selected_changes.clear();
                        }
                        for entry in &visible[from.min(to)..=from.max(to)] {
                            if entry.area == row.area && self.git_selected_changes.len() < 120 {
                                self.git_selected_changes.insert(entry.clone());
                            }
                        }
                        return;
                    }
                }
            }
        }
        if toggle {
            if !self.git_selected_changes.remove(&row) && self.git_selected_changes.len() < 120 {
                self.git_selected_changes.insert(row.clone());
            }
        } else {
            self.git_selected_changes.clear();
            self.git_selected_changes.insert(row.clone());
        }
        self.git_selection_anchor = Some(row);
    }

    fn refresh_git(&mut self, cx: &mut Context<Self>) {
        if self.git_loading {
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_loading = true;
        self.git_error = None;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { GitSnapshot::load(&root) })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.git_loading = false;
                match result {
                    Ok(snapshot) => {
                        view.git_snapshot = Some(snapshot);
                        view.git_error = None;
                    }
                    Err(error) => {
                        view.git_snapshot = None;
                        view.git_error = Some(error);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn refresh_git_history(&mut self, cx: &mut Context<Self>) {
        self.load_git_history(None, cx);
    }

    /// Installing the selected graph is the success boundary for MCP:
    /// a queued background read is NOT a visible Git Graph.
    fn load_git_history(
        &mut self,
        mcp_command: Option<NativeMcpUiCommand>,
        cx: &mut Context<Self>,
    ) {
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        let options = self.git_history_options.clone();
        self.git_history_request = self.git_history_request.wrapping_add(1);
        let request = self.git_history_request;
        self.git_history_loading = true;
        self.git_history_error = None;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { GitHistorySnapshot::load_filtered(&root, &options) })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_history_request != request {
                    if let Some(command) = mcp_command {
                        if let Some(bridge) = view.mcp_ui_bridge.as_mut() {
                            let _ = bridge.complete(
                                &command.request_id,
                                false,
                                None,
                                Some("not_visible"),
                            );
                        }
                    }
                    return;
                }
                view.git_history_loading = false;
                let mut applied = false;
                match result {
                    Ok(history) => {
                        view.git_history = Some(history);
                        view.git_history_error = None;
                        applied = view.git_history_open;
                    }
                    Err(error) => {
                        view.git_history = None;
                        view.git_history_error = Some(error);
                    }
                }
                if let Some(command) = mcp_command {
                    if let Some(bridge) = view.mcp_ui_bridge.as_mut() {
                        let _ = bridge.complete(
                            &command.request_id,
                            applied,
                            None,
                            if applied {
                                None
                            } else {
                                Some("ui_operation_failed")
                            },
                        );
                    }
                    view.mcp_ui_status = if applied {
                        "MCP: Native Git Graph loaded with requested filters".into()
                    } else {
                        "MCP: Native Git Graph failed or is not visible".into()
                    };
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn toggle_git_history(&mut self, cx: &mut Context<Self>) {
        self.git_history_open = !self.git_history_open;
        if self.git_history_open {
            self.refresh_git_history(cx);
        }
        cx.notify();
    }

    /// Human-facing counterpart to MCP open_git_graph.filePath/gitDiffRoot:
    /// view the selected Explorer file or folder history in the same bounded
    /// read-only Native Git Graph, without modifying the Git repository.
    fn open_explorer_selection_history(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.explorer_selection.clone() else {
            self.model
                .set_status("Select an Explorer file or folder for Git history");
            cx.notify();
            return;
        };
        let scoped = (|| -> Result<(Option<String>, Option<String>), String> {
            let root = dunce::canonicalize(self.model.root()).map_err(|error| error.to_string())?;
            let selected = dunce::canonicalize(selected)
                .map_err(|_| "Selected Explorer item no longer exists".to_string())?;
            let relative = selected
                .strip_prefix(&root)
                .map_err(|_| "Selected Explorer item is outside this Workspace".to_string())?;
            if relative.as_os_str().is_empty() {
                return Ok((None, None));
            }
            let path = relative
                .to_str()
                .ok_or("Git history cannot use a non-Unicode Explorer path")?
                .replace('\\', "/");
            if selected.is_file() {
                Ok((Some(path), None))
            } else if selected.is_dir() {
                Ok((None, Some(path)))
            } else {
                Err("Select a regular file or directory for Git history".into())
            }
        })();
        let (file_path, git_diff_root) = match scoped {
            Ok(scope) => scope,
            Err(error) => {
                self.model
                    .set_status(format!("Git history unavailable: {error}"));
                cx.notify();
                return;
            }
        };
        self.git_history_options.file_path = file_path;
        self.git_history_options.git_diff_root = git_diff_root;
        self.git_history_options.selected_ref = None;
        self.git_history_options.favorites_only = false;
        self.git_history_open = true;
        self.refresh_git_history(cx);
        self.model
            .set_status("Loading selected Explorer item Git history…");
        cx.notify();
    }

    /// A branch checked out in another linked worktree must be opened there
    /// rather than checked out in this worktree. Recheck Git HEAD on click to
    /// reject stale context-menu targets before switching the workspace.
    fn open_graph_branch_worktree(&mut self, branch: &str, cx: &mut Context<Self>) -> bool {
        let Some(target) =
            matching_branch_worktree(&self.worktrees, branch).map(|entry| entry.path.clone())
        else {
            // A locked/prunable worktree still owns this branch. Do not
            // silently try a second checkout when navigation is unavailable.
            if self
                .worktrees
                .iter()
                .any(|entry| entry.branch.as_deref() == Some(branch))
            {
                self.model.set_status(
                    "Branch is checked out in an unavailable worktree; refresh worktrees",
                );
                cx.notify();
                return true;
            }
            return false;
        };
        let actual = git2::Repository::discover(&target).ok().and_then(|repo| {
            repo.head().ok().and_then(|head| {
                if head.is_branch() {
                    head.shorthand().ok().map(str::to_owned)
                } else {
                    None
                }
            })
        });
        if actual.as_deref() != Some(branch) {
            self.model
                .set_status("Branch worktree changed; refresh worktrees before opening");
            self.refresh_worktrees(cx);
            cx.notify();
            return true;
        }
        self.switch_workspace(target, cx);
        true
    }

    fn run_git_graph_branch_menu(
        &mut self,
        branch: String,
        remote: bool,
        action: GitGraphBranchMenuAction,
        cx: &mut Context<Self>,
    ) {
        // The action belongs to the graph snapshot that rendered its menu.
        let exists = self.git_history.as_ref().is_some_and(|graph| {
            graph
                .branches
                .iter()
                .any(|row| row.name == branch && row.remote == remote)
        });
        if !exists {
            self.model
                .set_status("Git Graph branch selection is stale; refresh Graph");
            cx.notify();
            return;
        }
        let reference = if remote {
            format!("refs/remotes/{branch}")
        } else {
            branch.clone()
        };
        match action {
            GitGraphBranchMenuAction::History => self.set_git_history_ref(Some(reference), cx),
            GitGraphBranchMenuAction::CopyName => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(branch));
            }
            GitGraphBranchMenuAction::Compare => {
                if remote {
                    self.git_action_status = "Remote comparison: select ref in Git Graph; local comparison requires a local branch".into();
                    cx.notify();
                } else {
                    self.git_branch_input
                        .update(cx, |input, cx| input.set_query(&branch, cx));
                    self.preview_git_history(GitReadRequest::Compare(branch), cx);
                }
            }
            GitGraphBranchMenuAction::OpenWorktree => {
                if remote || !self.open_graph_branch_worktree(&branch, cx) {
                    self.model.set_status(
                        "No existing local worktree for this branch; refresh worktrees",
                    );
                    cx.notify();
                }
            }
            GitGraphBranchMenuAction::Switch => {
                if remote {
                    return;
                }
                if !self.open_graph_branch_worktree(&branch, cx) {
                    self.run_git_action(GitUiAction::SwitchBranch(branch), cx);
                }
            }
            GitGraphBranchMenuAction::Merge => {
                if remote {
                    return;
                }
                self.git_branch_input
                    .update(cx, |input, cx| input.set_query(&branch, cx));
                self.git_merge_armed = Some(branch.clone());
                self.git_action_status = format!(
                    "Merge prepared for {branch}. Review source branch, then press Confirm Merge in Source Control."
                );
                cx.notify();
            }
        }
    }

    fn set_git_history_ref(&mut self, selected: Option<String>, cx: &mut Context<Self>) {
        self.git_history_options.selected_ref = selected;
        self.refresh_git_history(cx);
    }

    fn toggle_git_history_favorite(&mut self, branch: String, cx: &mut Context<Self>) {
        let mut next = self.git_history_options.favorites.clone();
        if let Some(index) = next.iter().position(|value| value == &branch) {
            next.remove(index);
        } else {
            next.push(branch);
        }
        let result = self
            .session_store
            .as_ref()
            .ok_or_else(|| std::io::Error::other("Native session storage unavailable"))
            .and_then(|store| store.save_git_favorites(self.model.root(), &next));
        match result {
            Ok(()) => {
                self.git_history_options.favorites = next;
                self.refresh_git_history(cx);
            }
            Err(error) => {
                self.git_history_error = Some(format!("Cannot persist Git favorites: {error}"))
            }
        }
        cx.notify();
    }

    fn preview_git_history(&mut self, request: GitReadRequest, cx: &mut Context<Self>) {
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        let ticket = self.git_diff_request;
        self.git_diff = None;
        self.git_diff_loading = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    match request {
                        GitReadRequest::Commit(oid) => preview_commit(&root, &oid),
                        GitReadRequest::Compare(branch) => compare_local_branch(&root, &branch),
                    }
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_diff_request != ticket {
                    return;
                }
                view.git_diff_loading = false;
                view.git_diff =
                    Some(result.unwrap_or_else(|error| format!("Git preview refused: {error}")));
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn run_git_action(&mut self, action: GitUiAction, cx: &mut Context<Self>) {
        if self.git_action_busy {
            self.git_action_status = "Finish the current Git operation first".into();
            cx.notify();
            return;
        }
        // Classic routing: when another worktree already owns this branch,
        // opening that workspace replaces checkout in the current worktree.
        if let GitUiAction::SwitchBranch(branch) = &action {
            if self
                .worktrees
                .iter()
                .any(|entry| entry.branch.as_deref() == Some(branch))
            {
                self.open_graph_branch_worktree(branch, cx);
                return;
            }
        }
        if self.editor_dirty(cx) {
            self.git_action_status = "Save unsaved editor buffers before changing Git".into();
            cx.notify();
            return;
        }
        self.git_sensitive_armed = None;
        self.git_conflict_confirm_armed = false;
        self.git_conflict_loading = false;
        self.git_conflict_request = self.git_conflict_request.wrapping_add(1);
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_action_request = self.git_action_request.wrapping_add(1);
        let ticket = self.git_action_request;
        self.git_action_busy = true;
        self.git_action_status = "Running explicit Git operation…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut git = GitActions::open(&root)?;
                    match action {
                        GitUiAction::Stage(path) => {
                            git.stage_file(&path)?;
                            Ok("Staged selected file".to_string())
                        }
                        GitUiAction::Unstage(path) => {
                            git.unstage_file(&path)?;
                            Ok("Unstaged selected file".to_string())
                        }
                        GitUiAction::StageMany(paths) => git
                            .stage_files(&paths)
                            .map(|count| format!("Staged {count} selected files")),
                        GitUiAction::UnstageMany(paths) => git
                            .unstage_files(&paths)
                            .map(|count| format!("Unstaged {count} selected files")),
                        GitUiAction::Commit(message) => {
                            git.commit(&message).map(|id| format!("Committed {id}"))
                        }
                        GitUiAction::SwitchBranch(branch) => {
                            git.switch_branch(&branch)?;
                            Ok(format!("Switched to branch {branch}"))
                        }
                        GitUiAction::MergeWithoutConflicts(branch) => {
                            git.merge_without_conflicts(&branch)
                        }
                        GitUiAction::Fetch => {
                            git.fetch_current_remote()?;
                            Ok("Fetched remote refs; working tree unchanged".to_string())
                        }
                        GitUiAction::StashSave => git.stash_tracked_changes(),
                        GitUiAction::StashApply => git.apply_latest_stash(),
                        GitUiAction::PullFfOnly => git.pull_ff_only(),
                        GitUiAction::PushUpstream => git.push_current_upstream(),
                        GitUiAction::MergeWithChoices(plan, selections) => {
                            git.merge_with_conflict_choices(&plan, &selections)
                        }
                    }
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_action_request != ticket {
                    return;
                }
                view.git_action_busy = false;
                view.git_merge_armed = None;
                view.git_sensitive_armed = None;
                match result {
                    Ok(message) => {
                        view.git_action_status = message;
                        view.git_conflict_preview = None;
                        view.git_conflict_choices.clear();
                        view.git_conflict_selected = None;
                        view.git_conflict_confirm_armed = false;
                        view.git_snapshot = None;
                        view.git_selected_changes.clear();
                        view.git_selection_anchor = None;
                        view.refresh_git(cx);
                        if view.git_history_open {
                            view.git_history_loading = false;
                            view.git_history_request = view.git_history_request.wrapping_add(1);
                            view.refresh_git_history(cx);
                        }
                        view.refresh_worktrees(cx);
                    }
                    Err(error) => {
                        view.git_action_status = format!("Git operation refused: {error}")
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Explicit two-click opt-in for operations that mutate local changes
    /// (stash/pull) or contact remote endpoints (pull/push). Switching action
    /// disarms the previous one; no remote operation is automatically retried.
    fn confirm_sensitive_git_action(&mut self, action: ConfirmGitAction, cx: &mut Context<Self>) {
        if self.git_action_busy {
            self.git_action_status = "Finish the current Git operation first".into();
        } else if self.git_sensitive_armed == Some(action) {
            self.git_sensitive_armed = None;
            self.run_git_action(action.action(), cx);
        } else {
            self.git_merge_armed = None;
            self.git_conflict_confirm_armed = false;
            self.git_sensitive_armed = Some(action);
            self.git_action_status = action.warning().into();
        }
        cx.notify();
    }

    fn preview_git_merge_conflicts(&mut self, cx: &mut Context<Self>) {
        if self.git_action_busy || self.git_conflict_loading {
            self.git_action_status = "Finish the current Git operation first".into();
            cx.notify();
            return;
        }
        if self.editor_dirty(cx) {
            self.git_action_status =
                "Save unsaved editor buffers before previewing merge conflicts".into();
            cx.notify();
            return;
        }
        let branch = self.git_branch_input.read(cx).query().trim().to_owned();
        if branch.is_empty() {
            self.git_action_status =
                "Select a local source branch to inspect merge conflicts".into();
            cx.notify();
            return;
        }
        self.git_merge_armed = None;
        self.git_sensitive_armed = None;
        self.git_conflict_preview = None;
        self.git_conflict_choices.clear();
        self.git_conflict_selected = None;
        self.git_conflict_confirm_armed = false;
        self.git_conflict_request = self.git_conflict_request.wrapping_add(1);
        let ticket = self.git_conflict_request;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        self.git_conflict_loading = true;
        self.git_action_status = format!("Inspecting merge conflicts with {branch} (read-only)…");
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                GitActions::open(&root)?.preview_merge_conflicts(&branch)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.git_conflict_request != ticket
                {
                    return;
                }
                view.git_conflict_loading = false;
                match result {
                    Ok(plan) => {
                        view.git_action_status = format!(
                            "{} conflict(s) found with {}. Select Ours or Theirs for every file; choices replace the whole conflicted file.",
                            plan.conflicts.len(),
                            plan.source_branch,
                        );
                        view.git_conflict_selected = plan.conflicts.first().map(|file| file.path.clone());
                        view.git_conflict_preview = Some(plan);
                    }
                    Err(error) => view.git_action_status = format!("Cannot preview merge conflicts: {error}"),
                }
                cx.notify();
            }).ok();
        }).detach();
    }

    fn choose_git_conflict_side(
        &mut self,
        path: String,
        choice: GitConflictChoice,
        cx: &mut Context<Self>,
    ) {
        if self.git_action_busy || self.git_conflict_loading {
            return;
        }
        if self
            .git_conflict_preview
            .as_ref()
            .is_some_and(|preview| preview.conflicts.iter().any(|file| file.path == path))
        {
            self.git_conflict_selected = Some(path.clone());
            self.git_conflict_choices.insert(path, choice);
            self.git_conflict_confirm_armed = false;
            self.git_action_status =
                "Choice updated; selecting a side replaces that whole conflicted file, not just one hunk".into();
            cx.notify();
        }
    }

    fn confirm_git_conflict_merge(&mut self, cx: &mut Context<Self>) {
        if self.git_action_busy || self.git_conflict_loading {
            return;
        }
        let Some(preview) = self.git_conflict_preview.as_ref() else {
            self.git_action_status = "Preview conflicts before applying any merge choice".into();
            cx.notify();
            return;
        };
        let selections = preview
            .conflicts
            .iter()
            .map(|file| {
                self.git_conflict_choices
                    .get(&file.path)
                    .map(|choice| (file.path.clone(), *choice))
            })
            .collect::<Option<Vec<_>>>();
        let Some(selections) = selections else {
            self.git_action_status =
                "Choose Ours or Theirs for every conflicting file first".into();
            cx.notify();
            return;
        };
        if self.git_conflict_confirm_armed {
            self.git_conflict_confirm_armed = false;
            let preview = preview.clone();
            self.run_git_action(GitUiAction::MergeWithChoices(preview, selections), cx);
        } else {
            self.git_conflict_confirm_armed = true;
            self.git_sensitive_armed = None;
            self.git_merge_armed = None;
            self.git_action_status = format!(
                "Click Confirm Resolved Merge to create a two-parent merge commit from {} into {}. Each Ours/Theirs selection replaces the WHOLE file.",
                preview.source_branch, preview.current_branch,
            );
            cx.notify();
        }
    }

    fn preview_git_change(&mut self, relative: PathBuf, cx: &mut Context<Self>) {
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        let ticket = self.git_diff_request;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        self.git_diff = None;
        self.git_diff_loading = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { read_file_diff(&root, &relative) })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_diff_request != ticket {
                    return;
                }
                view.git_diff_loading = false;
                view.git_diff =
                    Some(result.unwrap_or_else(|error| format!("Diff unavailable: {error}")));
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn explorer_copy_action(
        &mut self,
        _: &ExplorerCopy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_keyboard_focus.is_focused(window) {
            self.set_explorer_clipboard(ExplorerClipboardMode::Copy, cx);
        }
    }

    fn explorer_cut_action(
        &mut self,
        _: &ExplorerCut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_keyboard_focus.is_focused(window) {
            self.set_explorer_clipboard(ExplorerClipboardMode::Cut, cx);
        }
    }

    fn explorer_paste_action(
        &mut self,
        _: &ExplorerPaste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_keyboard_focus.is_focused(window) {
            self.paste_explorer_clipboard(cx);
        }
    }

    fn explorer_select_all_action(
        &mut self,
        _: &ExplorerSelectAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.explorer_keyboard_focus.is_focused(window) {
            return;
        }
        let query = self
            .explorer_filter_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        let visible: Vec<PathBuf> = if !query.is_empty() {
            self.file_index
                .as_ref()
                .map(|index| index.search_explorer(&query, 200))
                .unwrap_or_default()
                .into_iter()
                .map(|hit| hit.path)
                .collect()
        } else if self.tree_mode {
            self.tree
                .as_ref()
                .map(|tree| {
                    tree.rows()
                        .iter()
                        .map(|row| self.model.root().join(&row.relative_path))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            self.model
                .entries()
                .iter()
                .map(|entry| entry.path.clone())
                .collect()
        };
        let count = self.explorer_multi_selection.select_visible_all(&visible);
        self.explorer_selection = self
            .explorer_multi_selection
            .top_level_paths()
            .last()
            .cloned();
        self.model.set_status(format!(
            "Selected {count} of {} visible Explorer item(s)",
            visible.len()
        ));
        cx.notify();
    }

    fn toggle_file_search_action(
        &mut self,
        _: &ToggleFileSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_file_search(window, cx);
    }

    fn toggle_content_search_action(
        &mut self,
        _: &ToggleContentSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.right_sidebar_view = RightSidebarView::Explorer;
        self.explorer_context_menu = None;
        self.toggle_content_search(window, cx);
    }

    fn toggle_file_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quick_open.is_some() {
            self.close_quick_open_dialog(window, cx);
            return;
        }
        if self.settings_section.is_some() {
            return;
        }
        if self.explorer_action.is_some() {
            self.cancel_explorer_action(cx);
            window.close_dialog(cx);
        }
        let input = cx.new(QuickOpenInput::new);
        cx.observe(&input, |_, _, cx| cx.notify()).detach();
        window.focus(&input.read(cx).focus_handle(cx), cx);
        self.quick_open = Some(input);
        self.ensure_file_index(cx);
        self.open_kit_quick_open_dialog(window, cx);
        cx.notify();
    }

    /// Shared, bounded asynchronous index for Ctrl+P and Classic Explorer's
    /// persistent fuzzy filter. The render pass never scans the filesystem.
    fn ensure_file_index(&mut self, cx: &mut Context<Self>) {
        if self
            .file_index
            .as_ref()
            .is_none_or(|index| index.root() != self.model.root())
            && !self.file_index_loading
        {
            let root = self.model.root().to_path_buf();
            let generation = self.workspace_generation;
            let options = self.explorer_visibility;
            self.file_index_request = self.file_index_request.wrapping_add(1);
            let request = self.file_index_request;
            self.file_index_loading = true;
            cx.spawn(async move |view, cx| {
                let result = cx
                    .background_spawn(async move {
                        WorkspaceFileIndex::scan_with_options(&root, options)
                    })
                    .await;
                view.update(cx, |view, cx| {
                    if view.workspace_generation != generation
                        || view.file_index_request != request
                        || view.explorer_visibility != options
                    {
                        return;
                    }
                    view.file_index_loading = false;
                    match result {
                        Ok(index) => view.file_index = Some(Arc::new(index)),
                        Err(error) => view
                            .model
                            .set_status(format!("Quick Open scan failed: {error}")),
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    fn toggle_content_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.content_query.is_some() {
            self.content_query = None;
            // A scan can finish after the panel is closed. Invalidating its
            // ticket guarantees it never restores hidden or stale results.
            self.content_request = self.content_request.wrapping_add(1);
            self.content_loading = false;
            self.content_matches.clear();
            self.content_error = None;
            self.content_searched_query.clear();
            self.content_truncated = false;
        } else {
            let input = cx.new(QuickOpenInput::new);
            window.focus(&input.read(cx).focus_handle(cx), cx);
            self.content_query = Some(input);
        }
        cx.notify();
    }

    fn search_project_contents(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self
            .content_query
            .as_ref()
            .map(|input| input.read(cx).query().trim().to_owned())
        else {
            return;
        };
        self.content_request = self.content_request.wrapping_add(1);
        let ticket = self.content_request;
        self.content_matches.clear();
        self.content_truncated = false;
        self.content_error = None;
        self.content_searched_query = query.clone();
        if query.is_empty() {
            self.content_loading = false;
            self.content_error = Some("Type text to search within workspace files".into());
            cx.notify();
            return;
        }
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        self.content_loading = true;
        cx.spawn(async move |view, cx| {
            // Directory walking, decoding and matching are strictly off the
            // GPUI UI thread. ProjectSearch never writes workspace files.
            let result = cx
                .background_spawn(async move {
                    ProjectSearch::new(&root).and_then(|search| search.search(&query))
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.content_request != ticket {
                    return;
                }
                view.content_loading = false;
                match result {
                    Ok(results) => {
                        view.content_matches = results.matches;
                        view.content_truncated = results.truncated;
                        view.content_error = None;
                    }
                    Err(error) => {
                        view.content_matches.clear();
                        view.content_truncated = false;
                        view.content_error = Some(format!("Content search failed: {error}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn open_content_search_hit(
        &mut self,
        path: &std::path::Path,
        line: usize,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.model.open_path(path) {
            self.model
                .set_status("Search result is unavailable; rerun search");
            cx.notify();
            return;
        }
        self.open_editor(cx);
        let Some(editor) = self.focused_editor() else {
            return;
        };
        match editor.update(cx, |editor, cx| {
            editor.navigate_to_content_hit(path, line, column, cx)
        }) {
            Ok(()) => window.focus(&editor.read(cx).focus_handle(cx), cx),
            Err(error) => self.model.set_status(format!(
                "Cannot navigate to search result {line}:{column}: {error}; rerun search"
            )),
        }
        self.persist_session();
        cx.notify();
    }

    fn editor_dirty(&self, cx: &mut Context<Self>) -> bool {
        [PaneSlot::Primary, PaneSlot::Secondary]
            .into_iter()
            .filter_map(|pane| self.editor_for_pane(pane))
            .any(|editor| editor.read(cx).has_unsaved_changes())
    }

    fn draft_store(&self) -> Option<EditorDraftStore> {
        self.session_store
            .as_ref()
            .map(|store| EditorDraftStore::new(store.state_dir()))
    }

    /// Restore only this workspace's unsaved scratch buffers. Corrupt draft
    /// records remain untouched on error; no workspace file is ever written.
    fn restore_scratch_tabs(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.draft_store() else {
            return;
        };
        let mut tabs = EditorTabs::new(Some(store));
        match tabs.restore_scratch_drafts(self.model.root()) {
            Ok(0) => {}
            Ok(count) => {
                self.install_editor(tabs, cx);
                self.model.set_status(format!(
                    "Recovered {count} unsaved scratch tab(s) from local state; review before saving"
                ));
            }
            Err(error) => self.model.set_status(format!(
                "Scratch recovery blocked: {error}. Local recovery files were kept."
            )),
        }
    }

    fn open_editor(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.model.selected_path().map(PathBuf::from) else {
            self.model.set_status("Choose a text file before editing");
            cx.notify();
            return;
        };
        self.explorer_preview_active = false;
        let root = self.model.root().to_path_buf();
        // Never open the same disk file as two independent mutable buffers.
        // If a pane already owns it, focus that exact existing document.
        for pane in [PaneSlot::Primary, PaneSlot::Secondary] {
            if let Some(owner) = self.editor_for_pane(pane) {
                if let Some((instance, _)) = owner
                    .read(cx)
                    .file_backed_tab_instances()
                    .into_iter()
                    .find(|(_, open)| open == &path)
                {
                    owner.update(cx, |editor, cx| {
                        editor.activate_tab_instance(instance, cx);
                    });
                    self.mixed_panes.activate(&WorkbenchTabIdentity::Editor {
                        editor_epoch: self.editor_epoch,
                        instance,
                    });
                    self.center_view = CenterView::Editor;
                    self.model
                        .set_status("Focused existing editor buffer; no duplicate opened");
                    cx.notify();
                    return;
                }
            }
        }
        if self.editor.is_none() {
            let mut tabs = EditorTabs::new(self.draft_store());
            match tabs.open_file(&root, &path) {
                Ok(effect) => {
                    self.model.set_status(effect.status());
                    self.install_editor(tabs, cx);
                    self.center_view = CenterView::Editor;
                }
                Err(error) => self.model.set_status(format!(
                    "Cannot edit {}: {error}. Recovery draft kept for investigation when one exists.",
                    path.display()
                )),
            }
            self.sync_mixed_tabs(cx);
            self.save_mixed_pane_layout(cx);
            cx.notify();
            return;
        }
        let owner = self.focused_editor().expect("primary editor installed");
        let result = owner.update(cx, |editor, cx| editor.open_in_place(&root, &path, cx));
        match result {
            Ok(effect) => {
                self.model.set_status(effect.status());
                self.center_view = CenterView::Editor;
            }
            Err(error) => self.model.set_status(format!(
                "Cannot edit {}: {error}. Recovery draft kept for investigation when one exists.",
                path.display()
            )),
        }
        self.sync_mixed_tabs(cx);
        self.save_mixed_pane_layout(cx);
        cx.notify();
    }

    fn save_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.focused_editor() else {
            return;
        };
        if editor.read(cx).tabs_empty() {
            self.model.set_status("Open or create a file before saving");
            cx.notify();
            return;
        }
        let path = editor.read(cx).document().path().map(PathBuf::from);
        if path.is_none() {
            // Align with Flutter: saving Untitled opens Save As instead of
            // refusing with "Scratch document requires Save As".
            editor.update(cx, |editor, cx| editor.prompt_save_as(cx));
            self.model
                .set_status("Choose a workspace path to save the untitled file");
            cx.notify();
            return;
        }
        let saved = editor.update(cx, |editor, cx| editor.save(cx));
        if saved {
            if self.model.selected_path() == path.as_deref() {
                self.model.reload_active();
            }
        }
        let status = editor.read(cx).status_text().to_owned();
        self.model.set_status(status);
        cx.notify();
    }

    fn save_all_editors(&mut self, cx: &mut Context<Self>) {
        let mut report = SaveAllReport::default();
        let mut first_untitled = None;
        // Each split pane owns distinct live editor documents. Never switch
        // tabs merely to save file-backed buffers in an inactive pane.
        for editor in [self.editor.clone(), self.secondary_editor.clone()]
            .into_iter()
            .flatten()
        {
            let pane_report = editor.update(cx, |editor, cx| editor.save_all(cx));
            if pane_report.needs_save_as > 0 && first_untitled.is_none() {
                first_untitled = Some(editor);
            }
            report.merge(pane_report);
        }
        if report.saved > 0 {
            self.model.reload_active();
        }
        let status = if first_untitled.is_some() {
            format!(
                "{}; opening Save As for first untitled file",
                report.status()
            )
        } else {
            report.status()
        };
        self.model.set_status(status);
        if let Some(editor) = first_untitled {
            editor.update(cx, |editor, cx| {
                editor.prompt_first_dirty_untitled_save_as(cx);
            });
            self.center_view = CenterView::Editor;
        }
        cx.notify();
    }

    fn new_editor(&mut self, cx: &mut Context<Self>) {
        self.explorer_preview_active = false;
        let root = self.model.root().to_path_buf();
        if self.editor.is_none() {
            let mut tabs = EditorTabs::new(self.draft_store());
            match tabs.open_scratch(&root) {
                Ok(effect) => {
                    self.model.set_status(effect.status());
                    self.install_editor(tabs, cx);
                    self.center_view = CenterView::Editor;
                }
                Err(error) => self
                    .model
                    .set_status(format!("Cannot create an editor: {error}")),
            }
            cx.notify();
            return;
        }
        let owner = self.focused_editor().expect("native editor installed");
        let result = owner.update(cx, |editor, cx| editor.open_scratch(&root, cx));
        match result {
            Ok(effect) => {
                self.model.set_status(effect.status());
                self.center_view = CenterView::Editor;
            }
            Err(error) => self
                .model
                .set_status(format!("Cannot create an editor: {error}")),
        }
        cx.notify();
    }

    fn pick_save_as(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.focused_editor() else {
            return;
        };
        editor.update(cx, |editor, cx| editor.prompt_save_as(cx));
        self.model
            .set_status("Save As: choose a workspace filename");
        cx.notify();
    }

    fn close_editor(&mut self, discard: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.focused_editor() else {
            return;
        };
        let secondary = self.secondary_editor.as_ref() == Some(&editor);
        if editor.read(cx).tabs_empty() {
            if secondary {
                self.secondary_editor = None;
            } else {
                self.editor = None;
            }
            self.model.set_status("Editor closed");
            cx.notify();
            return;
        }
        let path = editor.read(cx).document().path().map(PathBuf::from);
        let effect = editor.update(cx, |editor, _| editor.close_editor(discard));
        if effect.closed() {
            if self.model.selected_path() == path.as_deref() {
                self.model.reload_active();
            }
            if editor.read(cx).tabs_empty() {
                if secondary {
                    self.secondary_editor = None;
                } else {
                    self.editor = None;
                }
            }
        }
        self.model.set_status(effect.status());
        if effect.closed() {
            self.sync_mixed_tabs(cx);
            self.save_mixed_pane_layout(cx);
        }
        cx.notify();
    }

    fn persist_session(&mut self) {
        if let Some(store) = &self.session_store {
            if let Err(error) = store.save(&self.model) {
                eprintln!("Alera GPUI: unable to persist preview session: {error}");
                self.model
                    .set_status(format!("Preview state not saved: {error}"));
            } else if let Ok(recent) = store.recent_workspaces() {
                self.recent_workspaces = recent;
            }
        }
    }

    fn pin_current_project(&mut self, cx: &mut Context<Self>) {
        let Some(store) = &self.session_store else {
            self.model
                .set_status("Local project registry is unavailable");
            cx.notify();
            return;
        };
        match store
            .save_project(self.model.root())
            .and_then(|_| store.saved_projects())
        {
            Ok(projects) => {
                self.saved_projects = projects;
                self.model.set_status("Project added to saved projects");
                self.refresh_project_tree(cx);
            }
            Err(error) => self
                .model
                .set_status(format!("Cannot save project: {error}")),
        }
        cx.notify();
    }

    fn unpin_project(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let Some(store) = &self.session_store else {
            return;
        };
        match store
            .unsave_project(path)
            .and_then(|_| store.saved_projects())
        {
            Ok(projects) => {
                self.saved_projects = projects;
                self.model
                    .set_status("Project removed from saved list (files kept)");
                self.refresh_project_tree(cx);
            }
            Err(error) => self
                .model
                .set_status(format!("Cannot unpin project: {error}")),
        }
        cx.notify();
    }

    fn refresh_worktrees(&mut self, cx: &mut Context<Self>) {
        if self.worktree_loading || self.worktree_busy {
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.worktree_loading = true;
        self.worktree_error = None;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    WorktreeManager::discover(&root).and_then(|manager| manager.list())
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.worktree_loading = false;
                match result {
                    Ok(entries) => {
                        view.worktrees = entries;
                        view.worktree_error = None;
                    }
                    Err(error) => {
                        view.worktrees.clear();
                        view.worktree_error = Some(error.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_worktree_creator(&mut self, cx: &mut Context<Self>) {
        self.worktree_create_input = if self.worktree_create_input.is_some() {
            None
        } else {
            Some(cx.new(|cx| QuickOpenInput::with_placeholder("feature/new-branch", cx)))
        };
        self.worktree_delete_pending = None;
        cx.notify();
    }

    fn create_worktree(&mut self, cx: &mut Context<Self>) {
        if self.worktree_busy || self.mcp_workspace_create_busy || self.editor_dirty(cx) {
            self.model
                .set_status("Save editor changes and finish other worktree operations first");
            cx.notify();
            return;
        }
        let Some(input) = &self.worktree_create_input else {
            return;
        };
        let branch = input.read(cx).query().trim().to_owned();
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.worktree_busy = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    WorktreeManager::discover(&root).and_then(|manager| manager.create(&branch))
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.worktree_busy = false;
                match result {
                    Ok(path) => {
                        view.worktree_create_input = None;
                        view.switch_workspace(path, cx);
                        view.refresh_worktrees(cx);
                    }
                    Err(error) => {
                        view.worktree_error = Some(error.to_string());
                        view.model
                            .set_status(format!("Cannot create Worktree: {error}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remove_worktree(&mut self, target: PathBuf, cx: &mut Context<Self>) {
        if self.worktree_busy || self.mcp_workspace_create_busy {
            return;
        }
        if self.worktree_delete_pending.as_deref() != Some(target.as_path()) {
            self.worktree_delete_pending = Some(target);
            self.model.set_status(
                "Click Remove again to confirm. Only a clean, managed worktree may be removed.",
            );
            cx.notify();
            return;
        }
        self.worktree_delete_pending = None;
        if self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).switch_allowed().is_err())
            || self.model.root().starts_with(&target)
        {
            self.model
                .set_status("Cannot remove active Worktree or leave unsaved editor changes");
            cx.notify();
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.worktree_busy = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    WorktreeManager::discover(&root)
                        .and_then(|manager| manager.remove(&target, true))
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.worktree_busy = false;
                match result {
                    Ok(()) => {
                        view.model
                            .set_status("Clean managed Worktree removed (Git branch kept)");
                        view.refresh_worktrees(cx);
                        view.refresh_project_tree(cx);
                    }
                    Err(error) => {
                        view.worktree_error = Some(error.to_string());
                        view.model
                            .set_status(format!("Worktree removal refused: {error}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn switch_workspace(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.worktree_busy
            || self.mcp_workspace_create_busy
            || self.terminal_create_flow.is_creating()
            || self.git_action_busy
            || self.agent_launch_busy
            || self.mcp_compare_busy
        {
            self.model.set_status(
                "Finish current Git, Worktree or Host terminal create before switching projects",
            );
            cx.notify();
            return;
        }
        for pane in [PaneSlot::Primary, PaneSlot::Secondary] {
            if let Some(editor) = self.editor_for_pane(pane) {
                if let Err(status) = editor.read(cx).switch_allowed() {
                    self.model.set_status(status);
                    cx.notify();
                    return;
                }
            }
        }
        let canonical = match dunce::canonicalize(&root) {
            Ok(root) => root,
            Err(error) => {
                self.model
                    .set_status(format!("Cannot open workspace {}: {error}", root.display()));
                cx.notify();
                return;
            }
        };
        if canonical == self.model.root() {
            self.show_recent_workspaces = true;
            self.model.set_status("Workspace already open");
            cx.notify();
            return;
        }
        let result = if let Some(store) = &self.session_store {
            store.switch_workspace(&self.model, &canonical)
        } else {
            WorkbenchModel::new(&canonical)
        };
        let next = match result {
            Ok(next) => next,
            Err(error) => {
                self.model
                    .set_status(format!("Workspace switch failed: {error}"));
                cx.notify();
                return;
            }
        };
        // Preserve only tab identities/display mode, never PTY output or
        // credentials. Other workspaces never inherit this local layout.
        self.save_mixed_pane_layout(cx);
        self.save_terminal_layout();
        self.model = next;
        self.center_view = CenterView::Terminal;
        self.mixed_tabs
            .reset(self.workspace_generation.wrapping_add(1));
        self.mixed_panes.reset();
        self.mixed_tab_context = None;
        self.tree = WorkspaceTree::new_with_options(&canonical, self.explorer_visibility).ok();
        self.explorer_multi_selection = ExplorerMultiSelection::new(&canonical);
        self.explorer_preview_active = false;
        self.explorer_filter_input
            .update(cx, |input, cx| input.clear(cx));
        self.explorer_selection = None;
        self.explorer_delete_confirmed = false;
        self.explorer_delete_pending = None;
        self.explorer_context_menu = None;
        self.explorer_clipboard = None;
        self.explorer_drop_request = self.explorer_drop_request.wrapping_add(1);
        self.explorer_drop_busy = false;
        self.cancel_explorer_action(cx);
        self.editor = None;
        self.secondary_editor = None;
        self.editor_transfer_in_progress = false;
        self.editor_file_layout_snapshot.clear();
        self.editor_active_layout_snapshot = None;
        self.restore_scratch_tabs(cx);
        self.load_mixed_pane_layout();
        self.restore_file_tabs_from_pane_layout(cx);
        self.quick_open = None;
        self.file_index = None;
        self.file_index_loading = false;
        self.file_index_request = self.file_index_request.wrapping_add(1);
        self.content_query = None;
        self.content_loading = false;
        self.content_matches.clear();
        self.content_error = None;
        self.content_truncated = false;
        self.content_searched_query.clear();
        self.content_request = self.content_request.wrapping_add(1);
        self.git_snapshot = None;
        self.git_selected_changes.clear();
        self.git_selection_anchor = None;
        self.git_loading = false;
        self.git_history = None;
        self.git_history_loading = false;
        self.git_history_error = None;
        self.git_history_options = GitHistoryOptions::default();
        if let Some(store) = &self.session_store {
            match store.load_git_favorites(&canonical) {
                Ok(favorites) => self.git_history_options.favorites = favorites,
                Err(error) => {
                    self.git_history_error =
                        Some(format!("Native Git favorites cannot be loaded: {error}"))
                }
            }
        }
        self.git_history_request = self.git_history_request.wrapping_add(1);
        self.git_merge_armed = None;
        self.git_sensitive_armed = None;
        self.git_conflict_preview = None;
        self.git_conflict_choices.clear();
        self.git_conflict_selected = None;
        self.git_conflict_loading = false;
        self.git_conflict_confirm_armed = false;
        self.git_conflict_request = self.git_conflict_request.wrapping_add(1);
        self.git_action_request = self.git_action_request.wrapping_add(1);
        self.git_action_status.clear();
        self.git_filter_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_commit_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_branch_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_diff = None;
        self.git_diff_loading = false;
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        // Agent profiles are Host-owned, but selection and an in-flight
        // launch response must never attach to a different GPUI workspace.
        self.agent_profiles_request = self.agent_profiles_request.wrapping_add(1);
        self.agent_profiles_loading = false;
        self.agent_launch_request = self.agent_launch_request.wrapping_add(1);
        self.agent_launch_busy = false;
        self.agent_pending_tab = None;
        self.agent_prompt_input
            .update(cx, |input, cx| input.clear(cx));
        self.agent_status = "Select an Agent Profile and enter a task".into();
        self.mcp_host_workspaces.clear();
        self.mcp_host_workspaces_loading = false;
        self.mcp_host_workspaces_request = self.mcp_host_workspaces_request.wrapping_add(1);
        self.workspace_generation = self.workspace_generation.wrapping_add(1);
        self.project_tree_loading = false;
        self.project_tree_groups.clear();
        self.project_tree_request = self.project_tree_request.wrapping_add(1);
        self.worktrees.clear();
        self.worktree_error = None;
        self.worktree_loading = false;
        self.worktree_busy = false;
        self.worktree_create_input = None;
        self.worktree_delete_pending = None;
        // Terminal preview tabs are local UI state, never PTY owners. Drop
        // them on workspace changes to prevent cross-project output leakage.
        // Existing Host sessions remain alive and under Host ownership.
        self.terminals = TerminalDock::for_workspace(self.workspace_generation);
        self.terminal_auto_resize.clear();
        self.terminal_line_selection = None;
        self.terminal_panes.reset();
        self.terminal_create_flow.cancel();
        self.load_terminal_layout();
        self.terminal_key_queue.clear();
        self.terminal_key_queue_target = None;
        self.terminal_key_sending = false;
        self.clear_terminal_text_inputs(cx);
        self.terminal_poll = TerminalPollGate::default();
        self.terminal_poll_cadence = TerminalPollCadence::default();
        self.terminal_stream = None;
        self.terminal_stream_retry.recovered(Instant::now());
        self.terminal_stream_view_snapshot.clear();
        self.terminal_list_loading = false;
        self.terminal_read_loading = None;
        self.terminal_split_read_loading = false;
        self.terminal_tick = 0;
        self.show_recent_workspaces = true;
        if let Some(store) = &self.session_store {
            match store.recent_workspaces() {
                Ok(recent) => self.recent_workspaces = recent,
                Err(error) => self
                    .model
                    .set_status(format!("Recent workspace list: {error}")),
            }
        } else {
            self.recent_workspaces = vec![canonical];
        }
        self.refresh_terminals(cx);
        self.refresh_agent_profiles(cx);
        self.refresh_mcp_host_workspaces(cx);
        self.refresh_project_tree(cx);
        self.refresh_worktrees(cx);
        self.refresh_git(cx);
        cx.notify();
    }

    fn pick_workspace(&mut self, cx: &mut Context<Self>) {
        if self.picking_workspace {
            return;
        }
        self.picking_workspace = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Alera workspace".into()),
        });
        cx.spawn(async move |view, cx| {
            let result = selection.await;
            view.update(cx, |view, cx| {
                view.picking_workspace = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            view.switch_workspace(path, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => view
                        .model
                        .set_status(format!("Workspace picker failed: {error}")),
                    Err(error) => view
                        .model
                        .set_status(format!("Workspace picker canceled: {error}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Register a separately identified Native window with an already
    /// running Host. Classic remains registered as its own App window.
    fn connect_mcp_ui_bridge(&mut self, cx: &mut Context<Self>) {
        if self.mcp_ui_bridge.is_some() {
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.as_ref() else {
            self.mcp_ui_status = "MCP: existing Alera CLI unavailable".into();
            cx.notify();
            return;
        };
        match bridge.start_native_mcp_ui(self.mcp_ui_window_id.clone()) {
            Ok(connection) => {
                self.mcp_ui_bridge = Some(connection);
                self.mcp_ui_status = "MCP: registering Native window…".into();
                self.mcp_ui_ready = false;
                self.refresh_mcp_host_workspaces(cx);
            }
            Err(error) => {
                self.mcp_ui_ready = false;
                self.mcp_ui_status = format!("MCP: {error}");
            }
        }
        cx.notify();
    }

    fn refresh_mcp_host_workspaces(&mut self, cx: &mut Context<Self>) {
        if self.mcp_host_workspaces_loading {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            return;
        };
        self.mcp_host_workspaces_loading = true;
        self.mcp_host_workspaces_request = self.mcp_host_workspaces_request.wrapping_add(1);
        let request = self.mcp_host_workspaces_request;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.list_registered_workspaces() })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.mcp_host_workspaces_request != request
                {
                    return;
                }
                view.mcp_host_workspaces_loading = false;
                match result {
                    Ok(workspaces) => view.mcp_host_workspaces = workspaces,
                    Err(_) => view.mcp_host_workspaces.clear(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn install_native_settings(&mut self, settings: NativeSettings, cx: &mut Context<Self>) {
        if self.native_settings.auto_git_fetch_enabled != settings.auto_git_fetch_enabled
            || self.native_settings.auto_git_fetch_interval_minutes
                != settings.auto_git_fetch_interval_minutes
        {
            self.last_auto_git_fetch = Instant::now();
        }
        let tab_size = settings.editor_tab_size;
        self.native_settings = settings;
        self.native_settings_error = None;
        for editor in [self.editor.clone(), self.secondary_editor.clone()]
            .into_iter()
            .flatten()
        {
            editor.update(cx, |editor, cx| editor.set_tab_size(tab_size, cx));
        }
        cx.notify();
    }

    /// The Host authenticates the Agent and allowlists the path. Native
    /// independently rechecks its live Workspace registration and store,
    /// and replies only with the value actually read from its own JSON file.
    fn apply_native_mcp_setting(&mut self, command: &NativeMcpUiCommand, cx: &mut Context<Self>) {
        let result: Result<(NativeSettings, serde_json::Value), &'static str> = (|| {
            let authorized = self.mcp_host_workspaces.iter().any(|workspace| {
                workspace.id == command.workspace_id
                    && dunce::canonicalize(&workspace.path).is_ok_and(|path| path.is_dir())
            });
            if !authorized {
                return Err("workspace_missing");
            }
            let store = self.native_settings_store.as_ref().ok_or("ui_not_ready")?;
            let path = command
                .arguments
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or("ui_operation_failed")?;
            let settings = if command.action == "get_setting" {
                store.read().map_err(|_| "ui_operation_failed")?
            } else {
                let expected = command
                    .arguments
                    .get("expectedValue")
                    .ok_or("ui_operation_failed")?;
                let value = command
                    .arguments
                    .get("value")
                    .ok_or("ui_operation_failed")?;
                store
                    .compare_and_set(path, expected, value)
                    .map_err(|_| "ui_operation_failed")?
            };
            let readback = settings.get(path).ok_or("ui_operation_failed")?;
            Ok((settings, readback))
        })();
        match result {
            Ok((settings, readback)) => {
                self.install_native_settings(settings, cx);
                if let Some(bridge) = self.mcp_ui_bridge.as_mut() {
                    if bridge
                        .complete_setting(&command.request_id, &readback)
                        .is_err()
                    {
                        self.mcp_ui_status =
                            "MCP: Native setting applied, but Host receipt failed".into();
                    } else {
                        self.mcp_ui_status =
                            format!("MCP: Native {} readback confirmed", command.action);
                    }
                }
            }
            Err(code) => {
                self.mcp_ui_status =
                    "MCP: Native setting not changed or readback unavailable".into();
                if let Some(bridge) = self.mcp_ui_bridge.as_mut() {
                    let _ = bridge.complete(&command.request_id, false, None, Some(code));
                }
            }
        }
        cx.notify();
    }

    fn change_native_setting(
        &mut self,
        path: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let result = self
            .native_settings_store
            .as_ref()
            .ok_or_else(|| std::io::Error::other("Native state directory is unavailable"))
            .and_then(|store| {
                let expected = store.get(path)?;
                store.compare_and_set(path, &expected, &value)
            });
        match result {
            Ok(settings) => {
                self.install_native_settings(settings, cx);
                self.model
                    .set_status(format!("Native {path} saved and read back"));
            }
            Err(error) => {
                self.model
                    .set_status(format!("Native setting unchanged: {error}"));
            }
        }
        cx.notify();
    }

    /// Host has already authorized the target and parameters, but this window
    /// must independently confirm the target exists in its visible Workbench.
    /// Only implemented operations can produce a successful acknowledgment.
    fn apply_native_mcp_action(
        &mut self,
        command: &NativeMcpUiCommand,
        cx: &mut Context<Self>,
    ) -> Result<Option<String>, &'static str> {
        let target = self
            .mcp_host_workspaces
            .iter()
            .find(|workspace| workspace.id == command.workspace_id)
            .ok_or("workspace_missing")?;
        let target_path = dunce::canonicalize(&target.path).map_err(|_| "workspace_missing")?;
        if !target_path.is_dir() {
            return Err("workspace_missing");
        }

        if target_path != self.model.root() && command.action != "open_settings" {
            if !matches!(
                command.action.as_str(),
                "focus_workspace" | "open_file" | "reveal_file" | "select_tab" | "focus_agent"
            ) {
                return Err("not_visible");
            }
            self.switch_workspace(target_path.clone(), cx);
            if self.model.root() != target_path {
                return Err("not_visible");
            }
        }

        let args = command.arguments.as_object().ok_or("ui_not_ready")?;
        match command.action.as_str() {
            "open_settings" => {
                let section = args
                    .get("sectionId")
                    .and_then(|value| value.as_str())
                    .unwrap_or("application");
                self.show_native_settings_section(section, cx)?;
                self.model
                    .set_status(format!("Native Settings · {section}"));
                cx.notify();
                if self.settings_section.as_deref() == Some(section) {
                    Ok(None)
                } else {
                    Err("not_visible")
                }
            }
            "focus_workspace" => {
                if self.model.root() != target_path {
                    return Err("not_visible");
                }
                cx.notify();
                Ok(None)
            }
            "reveal_file" | "open_file" => {
                let relative = args
                    .get("relativePath")
                    .and_then(|path| path.as_str())
                    .ok_or("invalid_path")?;
                let relative_path = Path::new(relative);
                if !Self::valid_editor_layout_key(relative_path) || relative.len() > 1024 {
                    return Err("invalid_path");
                }
                let target_file = dunce::canonicalize(self.model.root().join(relative_path))
                    .map_err(|_| "file_missing")?;
                if !target_file.starts_with(self.model.root()) || !target_file.is_file() {
                    return Err("invalid_path");
                }
                if command.action == "reveal_file" {
                    let Some(tree) = self.tree.as_mut() else {
                        return Err("ui_not_ready");
                    };
                    if !tree.reveal(relative_path).map_err(|_| "not_visible")? {
                        return Err("not_visible");
                    }
                    self.tree_mode = true;
                    self.explorer_selection = Some(target_file);
                    cx.notify();
                    return Ok(None);
                }
                // Native cannot honor Flutter's multiple-buffer 'new' mode
                // or external editor settings yet. Fail rather than falsely
                // acknowledging an unperformed operation.
                if args.get("openMode").and_then(|mode| mode.as_str()) == Some("new")
                    || args
                        .get("editor")
                        .and_then(|editor| editor.as_str())
                        .is_some_and(|editor| editor != "alera")
                {
                    return Err("ui_not_ready");
                }
                let line = args
                    .get("line")
                    .map(|value| value.as_u64().ok_or("invalid_path"))
                    .transpose()?;
                let column = args
                    .get("column")
                    .map(|value| value.as_u64().ok_or("invalid_path"))
                    .transpose()?;
                if column.is_some() && line.is_none() {
                    return Err("invalid_path");
                }
                if !self.model.open_path(&target_file) {
                    return Err("file_missing");
                }
                self.open_editor(cx);
                let Some(editor) = self.focused_editor() else {
                    return Err("ui_not_ready");
                };
                if editor.read(cx).document().path() != Some(target_file.as_path())
                    || self.center_view != CenterView::Editor
                {
                    return Err("not_visible");
                }
                if let Some(line) = line {
                    let line: usize = line.try_into().map_err(|_| "invalid_path")?;
                    let column: usize =
                        column.unwrap_or(1).try_into().map_err(|_| "invalid_path")?;
                    if line == 0 || column == 0 {
                        return Err("invalid_path");
                    }
                    editor
                        .update(cx, |editor, cx| {
                            editor.navigate_to_content_hit(&target_file, line, column, cx)
                        })
                        .map_err(|_| "not_visible")?;
                }
                cx.notify();
                Ok(None)
            }
            "select_tab" | "focus_agent" => {
                let tab_id = args
                    .get("tabId")
                    .and_then(|id| id.as_str())
                    .ok_or("tab_missing")?;
                let handle = matching_host_tab_handle(command, &self.terminals.sessions)
                    .ok_or("tab_missing")?
                    .to_owned();
                self.select_terminal(handle.clone(), cx);
                let active = self
                    .terminals
                    .active_tab
                    .and_then(|index| self.terminals.tabs.get(index))
                    .is_some_and(|tab| tab.handle == handle);
                if !active {
                    return Err("not_visible");
                }
                Ok(Some(tab_id.to_owned()))
            }
            // Background Git Graph selection, compare and creation paths all
            // have dedicated request-ticket owners and deferred receipts.
            _ => Err("ui_not_ready"),
        }
    }

    fn start_native_mcp_git_graph(&mut self, command: NativeMcpUiCommand, cx: &mut Context<Self>) {
        let registered = self.mcp_host_workspaces.iter().any(|workspace| {
            workspace.id == command.workspace_id
                && dunce::canonicalize(&workspace.path).ok().as_deref() == Some(self.model.root())
        });
        let args = command.arguments.as_object();
        let supported = args.is_some_and(|args| {
            args.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "allBranches" | "favoritesOnly" | "selectedRef" | "filePath" | "gitDiffRoot"
                )
            }) && ["allBranches", "favoritesOnly"]
                .into_iter()
                .all(|key| args.get(key).is_none_or(serde_json::Value::is_boolean))
                && args.get("selectedRef").is_none_or(|value| {
                    value.as_str().is_some_and(|name| {
                        !name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
                    })
                })
                && ["filePath", "gitDiffRoot"].into_iter().all(|key| {
                    args.get(key).is_none_or(|value| {
                        value.as_str().is_some_and(|path| {
                            !path.is_empty()
                                && path.len() <= 1024
                                && !path.chars().any(char::is_control)
                        })
                    })
                })
        });
        if !registered || !supported {
            if let Some(bridge) = self.mcp_ui_bridge.as_mut() {
                let _ = bridge.complete(
                    &command.request_id,
                    false,
                    None,
                    Some(if !registered {
                        "workspace_missing"
                    } else {
                        "ui_not_ready"
                    }),
                );
            }
            return;
        }
        let args = args.expect("validated graph arguments");
        self.git_history_options.all_branches = args
            .get("allBranches")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(self.git_history_options.all_branches);
        self.git_history_options.favorites_only = args
            .get("favoritesOnly")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(self.git_history_options.favorites_only);
        if self.git_history_options.favorites_only {
            self.git_history_options.all_branches = true;
        }
        self.git_history_options.selected_ref = args
            .get("selectedRef")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        self.git_history_options.file_path = args
            .get("filePath")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        self.git_history_options.git_diff_root = args
            .get("gitDiffRoot")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        self.git_history_open = true;
        self.load_git_history(Some(command), cx);
    }

    /// Use the existing Host-owned managed Workspace workflow. Only one
    /// request at a time, and no success receipt until both the Host's new
    /// registered Workspace and this actual Native Workbench selection agree.
    fn start_native_mcp_workspace_create(
        &mut self,
        command: NativeMcpUiCommand,
        cx: &mut Context<Self>,
    ) {
        let valid_caller = self
            .mcp_host_workspaces
            .iter()
            .find(|entry| entry.id == command.workspace_id)
            .and_then(|entry| dunce::canonicalize(&entry.path).ok())
            .as_deref()
            == Some(self.model.root());
        let request = HostManagedWorkspaceRequest::from_host_arguments(&command.arguments);
        let busy = self.mcp_workspace_create_busy
            || self.mcp_compare_busy
            || self.worktree_busy
            || self.git_action_busy
            || self.agent_launch_busy
            || self.terminal_create_flow.is_creating()
            || self.editor_dirty(cx);
        let Some(bridge) = self.runtime_bridge.clone() else {
            if let Some(mcp) = self.mcp_ui_bridge.as_mut() {
                let _ = mcp.complete(&command.request_id, false, None, Some("ui_not_ready"));
            }
            return;
        };
        if !valid_caller || request.is_err() || busy {
            if let Some(mcp) = self.mcp_ui_bridge.as_mut() {
                let code = if !valid_caller {
                    "workspace_missing"
                } else if busy {
                    "ui_not_ready"
                } else {
                    "workspace_create_failed"
                };
                let _ = mcp.complete(&command.request_id, false, None, Some(code));
            }
            return;
        }
        let request = request.expect("Host Workspace args checked");
        self.mcp_workspace_create_busy = true;
        self.mcp_workspace_create_request = self.mcp_workspace_create_request.wrapping_add(1);
        let ticket = self.mcp_workspace_create_request;
        let generation = self.workspace_generation;
        self.model
            .set_status("Creating Host-managed Workspace for MCP Agent…");
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move { bridge.create_mcp_workspace(&request) }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.mcp_workspace_create_request != ticket {
                    return;
                }
                view.mcp_workspace_create_busy = false;
                match result {
                    Ok(created) => {
                        // This can fail when the user edited a buffer during
                        // creation: never discard it, never report success.
                        view.switch_workspace(created.path.clone(), cx);
                        let visible = view.model.root() == created.path;
                        if visible {
                            if let Some(mcp) = view.mcp_ui_bridge.as_mut() {
                                if mcp.complete_workspace(&command.request_id, &created.id).is_err() {
                                    view.model.set_status("Created Workspace is open but MCP Host completion failed; inspect Host state before retrying");
                                } else {
                                    view.model.set_status("Host-managed MCP Workspace created and selected");
                                }
                            }
                        } else {
                            view.model.set_status("Host created the Workspace, but Native did not switch. Reopen it from Projects; do not create a duplicate");
                            if let Some(mcp) = view.mcp_ui_bridge.as_mut() {
                                let _ = mcp.complete(&command.request_id, false, None, Some("not_visible"));
                            }
                        }
                    }
                    Err(error) => {
                        view.model.set_status(format!("MCP Workspace creation not confirmed: {error}; check Host workspaces before retrying"));
                        if let Some(mcp) = view.mcp_ui_bridge.as_mut() {
                            let _ = mcp.complete(&command.request_id, false, None, Some("workspace_create_failed"));
                        }
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
    }

    /// A deferred Host UI receipt. The comparison must be computed on a
    /// worker and actually installed in the visible Git pane before success.
    fn start_native_mcp_compare(&mut self, command: NativeMcpUiCommand, cx: &mut Context<Self>) {
        let authorized_root = self
            .mcp_host_workspaces
            .iter()
            .find(|entry| entry.id == command.workspace_id)
            .and_then(|entry| dunce::canonicalize(&entry.path).ok());
        let valid_workspace = authorized_root.as_deref() == Some(self.model.root());
        let arguments = command.arguments.as_object();
        let refs = arguments.and_then(|args| {
            Some((
                args.get("baseRef")?.as_str()?.to_owned(),
                args.get("headRef")?.as_str()?.to_owned(),
                args.get("gitDiffRoot")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned),
            ))
        });
        if !valid_workspace || refs.is_none() || self.mcp_compare_busy || self.git_diff_loading {
            if let Some(bridge) = self.mcp_ui_bridge.as_mut() {
                let _ = bridge.complete(
                    &command.request_id,
                    false,
                    None,
                    Some(if !valid_workspace {
                        "workspace_missing"
                    } else {
                        "ui_not_ready"
                    }),
                );
            }
            return;
        }
        let (base, head, scope) = refs.expect("refs checked");
        self.mcp_compare_busy = true;
        self.mcp_compare_request = self.mcp_compare_request.wrapping_add(1);
        let ticket = self.mcp_compare_request;
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        let diff_ticket = self.git_diff_request;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        self.git_history_open = true;
        self.git_diff_loading = true;
        self.git_diff = None;
        self.model
            .set_status(format!("Comparing {base}..{head} for MCP Agent…"));
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(
                    async move { compare_git_refs(&root, &base, &head, scope.as_deref()) },
                )
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.mcp_compare_request != ticket {
                    return;
                }
                view.mcp_compare_busy = false;
                let visible = view.git_diff_request == diff_ticket && view.git_history_open;
                let mut applied = false;
                if visible {
                    view.git_diff_loading = false;
                    match result {
                        Ok(comparison) => {
                            view.git_diff = Some(comparison);
                            applied = true;
                            view.model.set_status("MCP Git branch comparison displayed");
                        }
                        Err(error) => {
                            view.git_diff = Some(format!("MCP Git comparison refused: {error}"));
                        }
                    }
                }
                if let Some(bridge) = view.mcp_ui_bridge.as_mut() {
                    let _ = bridge.complete(
                        &command.request_id,
                        applied,
                        None,
                        if applied {
                            None
                        } else {
                            Some("git_compare_failed")
                        },
                    );
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn poll_native_mcp_commands(&mut self, cx: &mut Context<Self>) {
        let events = self
            .mcp_ui_bridge
            .as_mut()
            .map(NativeMcpUiBridge::take_events)
            .unwrap_or_default();
        if events.is_empty() {
            return;
        }
        for event in events {
            match event {
                NativeMcpUiEvent::Ready => {
                    self.mcp_ui_ready = true;
                    self.mcp_ui_status = "MCP: Native window registered with Host".into();
                    self.refresh_mcp_host_workspaces(cx);
                }
                NativeMcpUiEvent::Command(command) => {
                    if matches!(command.action.as_str(), "get_setting" | "update_setting") {
                        self.apply_native_mcp_setting(&command, cx);
                        continue;
                    }
                    if command.action == "create_workspace" {
                        self.start_native_mcp_workspace_create(command, cx);
                        continue;
                    }
                    if command.action == "compare_branches" {
                        self.start_native_mcp_compare(command, cx);
                        continue;
                    }
                    if command.action == "open_git_graph" {
                        self.start_native_mcp_git_graph(command, cx);
                        continue;
                    }
                    let result = self.apply_native_mcp_action(&command, cx);
                    let (applied, tab_id, error) = match result {
                        Ok(tab_id) => (true, tab_id, None),
                        Err(code) => (false, None, Some(code)),
                    };
                    if let Some(bridge) = self.mcp_ui_bridge.as_mut() {
                        if bridge
                            .complete(&command.request_id, applied, tab_id.as_deref(), error)
                            .is_err()
                        {
                            self.mcp_ui_status =
                                "MCP: could not acknowledge Host UI request".into();
                        } else if !applied {
                            self.mcp_ui_status =
                                "MCP: UI command refused; unsupported or invisible target".into();
                        }
                    }
                }
                NativeMcpUiEvent::Disconnected => {
                    self.mcp_ui_ready = false;
                    self.mcp_ui_bridge = None;
                    self.mcp_ui_status = "MCP: Host window relay disconnected".into();
                }
            }
        }
        cx.notify();
    }

    fn refresh_runtime(&mut self, cx: &mut Context<Self>) {
        if self.runtime_loading {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.runtime_status =
                "CLI unavailable. Set ALERA_CLI_PATH to the existing Alera CLI.".into();
            cx.notify();
            return;
        };
        self.runtime_loading = true;
        self.runtime_status = "Checking existing Runtime Host…".into();
        cx.notify();
        // CLI handles authentication and the bounded probe runs off the UI
        // thread. The experimental frontend never starts or owns a PTY.
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move { bridge.probe() }).await;
            view.update(cx, |view, cx| {
                view.runtime_loading = false;
                view.runtime_status = match result {
                    Ok(status) => status.label(),
                    Err(error) => format!("Runtime check failed: {error}"),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn save_terminal_layout(&mut self) {
        // Do not overwrite a previously saved layout with an empty view
        // while waiting for the first successful Host session listing.
        if self.terminal_layout_restore.is_some() && self.terminals.tabs.is_empty() {
            return;
        }
        // An explicit tab action supersedes deferred startup restoration.
        self.terminal_layout_restore = None;
        let Some(store) = &self.session_store else {
            return;
        };
        let layout = TerminalLayout {
            tabs: self
                .terminals
                .tabs
                .iter()
                .map(|tab| TerminalLayoutTab {
                    handle: tab.handle.clone(),
                    workspace_id: tab.workspace_id.clone(),
                    tab_id: tab.tab_id.clone(),
                })
                .collect(),
            active_handle: self.terminals.active().map(|tab| tab.handle.clone()),
            transcript_view: self.terminals.view_mode == TerminalViewMode::Transcript,
        };
        if let Err(error) = store.save_terminal_layout(self.model.root(), &layout) {
            self.terminals.status = format!("Terminal tab layout not saved: {error}");
        }
    }

    fn load_terminal_layout(&mut self) {
        self.terminal_layout_restore = self.session_store.as_ref().and_then(|store| {
            match store.load_terminal_layout(self.model.root()) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Alera GPUI terminal layout recovery refused: {error}");
                    None
                }
            }
        });
    }

    /// Restore display-only tabs only after an authenticated Host session
    /// list matches every opaque session/workspace/tab identity. A stale
    /// handle is never treated as a request to start or attach a new PTY.
    fn restore_terminal_layout_from_host(&mut self, cx: &mut Context<Self>) {
        if self.terminals.sessions.is_empty() {
            return;
        }
        let Some(layout) = self.terminal_layout_restore.take() else {
            return;
        };
        if self.terminals.restore_previews_from_host_layout(&layout) > 0 {
            self.refresh_terminal_output(cx);
        }
    }

    fn refresh_agent_profiles(&mut self, cx: &mut Context<Self>) {
        if self.agent_profiles_loading {
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_status = "Alera CLI not installed; cannot list Agent Profiles".into();
            cx.notify();
            return;
        };
        self.agent_profiles_loading = true;
        self.agent_profiles_request = self.agent_profiles_request.wrapping_add(1);
        let request = self.agent_profiles_request;
        let generation = self.workspace_generation;
        self.agent_status = "Listing Host Agent Profiles…".into();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.list_agent_profiles() })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.agent_profiles_request != request
                {
                    return;
                }
                view.agent_profiles_loading = false;
                match result {
                    Ok(profiles) => {
                        view.agent_status =
                            format!("{} Agent Profile(s) available", profiles.len());
                        if !view
                            .agent_selected_profile
                            .as_ref()
                            .is_some_and(|id| profiles.iter().any(|profile| profile.id == *id))
                        {
                            view.agent_selected_profile = view
                                .native_settings
                                .default_agent_profile_id
                                .as_ref()
                                .filter(|id| profiles.iter().any(|profile| &profile.id == *id))
                                .cloned()
                                .or_else(|| profiles.first().map(|profile| profile.id.clone()));
                        }
                        view.agent_profiles = profiles;
                    }
                    Err(error) => {
                        view.agent_profiles.clear();
                        view.agent_selected_profile = None;
                        view.agent_status = format!("Agent Profiles unavailable: {error}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn launch_selected_agent(&mut self, cx: &mut Context<Self>) {
        if self.agent_launch_busy || self.agent_pending_tab.is_some() {
            self.agent_status =
                "Finish confirming the previous Agent tab first; use Refresh or Dismiss tracking"
                    .into();
            cx.notify();
            return;
        }
        let Some(profile_id) = self
            .agent_selected_profile
            .clone()
            .filter(|id| self.agent_profiles.iter().any(|profile| &profile.id == id))
        else {
            self.agent_status = "Select an existing Agent Profile first".into();
            cx.notify();
            return;
        };
        let input = self.agent_prompt_input.read(cx);
        if input.composing() {
            self.agent_status = "Finish IME composition before launching an Agent".into();
            cx.notify();
            return;
        }
        let prompt = input.query().trim().to_owned();
        if prompt.is_empty() {
            self.agent_status = "Enter a task prompt before starting the Agent".into();
            cx.notify();
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_status = "Alera CLI not installed; cannot start Agent".into();
            cx.notify();
            return;
        };
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.agent_launch_busy = true;
        self.agent_launch_request = self.agent_launch_request.wrapping_add(1);
        let ticket = self.agent_launch_request;
        self.agent_status =
            "Checking registered Host workspace and launching selected Agent…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                let registered = bridge.list_registered_workspaces()?;
                let workspace_id = select_registered_host_workspace(&root, &registered)?;
                bridge.launch_agent_profile(&profile_id, &workspace_id, &prompt)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.agent_launch_request != ticket {
                    return;
                }
                view.agent_launch_busy = false;
                match result {
                    Ok(created) => {
                        view.agent_pending_tab = Some(created);
                        view.agent_prompt_input.update(cx, |input, cx| input.clear(cx));
                        view.agent_status = "Agent launched; confirming the exact Host tab identity…".into();
                        view.refresh_terminals(cx);
                    }
                    Err(error) => {
                        view.agent_status = format!(
                            "Agent launch not confirmed: {error}. Check Attach existing before retrying; the Host might have created the tab"
                        );
                        view.refresh_terminals(cx);
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
    }

    fn create_host_terminal(&mut self, cx: &mut Context<Self>) {
        if self.terminal_create_flow.is_active() {
            self.terminals.status =
                "A Host terminal request is pending; confirm, retry its listing, or cancel first"
                    .into();
            cx.notify();
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.status = "Alera CLI missing; cannot create Host terminal".into();
            cx.notify();
            return;
        };
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        let Some(request_id) = self.terminal_create_flow.start_request() else {
            return;
        };
        self.terminals.status =
            "Checking registered Host workspaces before creating terminal…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    // Explicit New Terminal may start the existing CLI Host
                    // and register this *already existing* checkout. Passive
                    // refresh/read/write still never start a Host or PTY.
                    bridge.create_terminal_for_workspace(&root)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                match result {
                    Ok(created) => {
                        if !view
                            .terminal_create_flow
                            .finish_request(request_id, created)
                        {
                            // Canceled Host RPCs may still have created a PTY.
                            // Never auto-attach a stale response.
                            return;
                        }
                        view.center_view = CenterView::Terminal;
                        view.terminals.status =
                            "Host terminal created; confirming session identity (30s limit)…"
                                .into();
                        view.refresh_terminals(cx);
                    }
                    Err(error) => {
                        if !view.terminal_create_flow.fail_request(request_id) {
                            return;
                        }
                        view.terminals.status = if error.kind() == std::io::ErrorKind::InvalidData {
                            // The Host RPC may already have created a PTY,
                            // even when the CLI reply failed validation.
                            // Never encourage a second create as a retry.
                            format!(
                                "Host terminal reply could not be verified: {error}. +                                 A session may already exist; use Attach existing before retrying New Terminal."
                            )
                        } else {
                            format!("Cannot create Host terminal: {error}")
                        };
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn retry_host_terminal_confirmation(&mut self, cx: &mut Context<Self>) {
        if self.terminal_create_flow.retry_confirmation() {
            self.terminals.status = "Rechecking the existing Host terminal (no new PTY)…".into();
            self.refresh_terminals(cx);
            cx.notify();
        }
    }

    fn cancel_host_terminal_confirmation(&mut self, cx: &mut Context<Self>) {
        if self.terminal_create_flow.cancel() {
            // An RPC in flight can still create a Host terminal. Dismiss
            // only the local expectation; never issue a Host kill request.
            self.terminals.status =
                "Stopped waiting. Host terminal may exist; use Attach existing to find it".into();
            cx.notify();
        }
    }

    fn refresh_terminals(&mut self, cx: &mut Context<Self>) {
        if self.terminal_list_loading {
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.note_session_list_failure();
            self.terminals.status = "Cannot list sessions: install experimental Alera CLI".into();
            cx.notify();
            return;
        };
        self.terminal_list_loading = true;
        let generation = self.workspace_generation;
        self.terminals.status = "Listing existing Host sessions…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.list_terminals() })
                .await;
            view.update(cx, |view, cx| {
                if view.terminals.workspace_generation() != generation {
                    return;
                }
                view.terminal_list_loading = false;
                match result {
                    Ok(sessions) => {
                        let was_disconnected = view.terminals.session_list_is_stale()
                            || view.terminals.sessions.is_empty();
                        if view.terminals.apply_session_list(generation, sessions) {
                            if was_disconnected && !view.terminals.sessions.is_empty() {
                                view.terminal_poll.reset();
                            }
                            view.restore_terminal_layout_from_host(cx);
                            if let Some(created) = view
                                .terminal_create_flow
                                .confirm_sessions(&view.terminals.sessions)
                            {
                                view.select_terminal(created.handle, cx);
                            }
                            // Launch reply contains a Host workspace+tab id,
                            // not a trustworthy PTY handle. Resolve through
                            // a fresh authenticated session list, and require
                            // this to be an Agent session before opening.
                            let found_agent = view.agent_pending_tab.as_ref()
                                .and_then(|pending| matching_agent_handle(pending, &view.terminals.sessions))
                                .map(str::to_owned);
                            if let Some(handle) = found_agent {
                                view.select_terminal(handle.clone(), cx);
                                let attached = view.terminals.active_tab
                                    .and_then(|index| view.terminals.tabs.get(index))
                                    .is_some_and(|tab| tab.handle == handle);
                                if attached {
                                    view.agent_pending_tab = None;
                                    view.agent_status = "Agent Host tab confirmed and attached".into();
                                } else {
                                    view.agent_status = "Host Agent exists, but Native preview tabs are full; close one and Refresh".into();
                                }
                            }
                        }
                    }
                    Err(error) => {
                        // A failed list must not mark every PTY stopped or
                        // erase the old VT preview. The Host may restart
                        // and re-expose the same running Session later.
                        view.terminals.note_session_list_failure();
                        view.terminals.status = if error.kind() == std::io::ErrorKind::NotConnected {
                            "Runtime Host is offline · use + → New Terminal to start one".into()
                        } else {
                            format!("Session refresh failed: {error}")
                        };
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select_terminal(&mut self, handle: String, cx: &mut Context<Self>) {
        if self.terminals.open(&handle) {
            self.explorer_preview_active = false;
            self.terminal_line_selection = None;
            self.center_view = CenterView::Terminal;
            self.clear_terminal_text_inputs(cx);
            self.save_terminal_layout();
            self.refresh_terminal_output(cx);
        } else {
            cx.notify();
        }
    }

    /// Native terminal content routes keys and committed IME text directly to
    /// the Host PTY; no always-visible proxy command input is needed.
    fn resize_terminal(&mut self, cols: u16, rows: u16, cx: &mut Context<Self>) {
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.status = "Host CLI unavailable for Resize".into();
            cx.notify();
            return;
        };
        let Some(tab) = self.terminals.active() else {
            self.terminals.status = "Choose a running Host terminal before Resize".into();
            cx.notify();
            return;
        };
        if !tab.running {
            self.terminals.status = "Cannot resize a stopped Host terminal".into();
            cx.notify();
            return;
        }
        let handle = tab.handle.clone();
        let tab_id = tab.tab_id.clone();
        let generation = self.workspace_generation;
        if let Some(target) = self
            .terminals
            .active_tab
            .and_then(|index| self.terminals.context_target_at(index))
        {
            self.terminal_auto_resize.suspend_until_changed(&target);
        }
        self.terminals.status = format!("Requesting Host viewport {cols}×{rows}…");
        cx.notify();
        cx.spawn(async move |view, cx| {
            let request_handle = handle.clone();
            let result = cx.background_spawn(async move {
                bridge.resize_terminal(&request_handle, cols, rows)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || !view.terminals.active().is_some_and(|tab| {
                    tab.handle == handle && tab.tab_id == tab_id
                }) {
                    return;
                }
                view.terminals.status = match result {
                    Ok(()) => format!(
                        "Requested {cols}×{rows}; Host may defer while a mobile client holds viewport control"
                    ),
                    Err(error) => format!("Host Resize refused: {error}"),
                };
                cx.notify();
            }).ok();
        }).detach();
    }

    /// Run from the existing Workbench lifecycle poll, never from render.
    /// Each pending viewport is bound to one exact still-live Host tab and
    /// is dispatched through the allowlisted, non-starting CLI resize verb.
    fn flush_auto_terminal_resizes(&mut self, cx: &mut Context<Self>) {
        let Some(bridge) = self.runtime_bridge.clone() else {
            return;
        };
        for change in self.terminal_auto_resize.take_ready(Instant::now()) {
            if !self.terminals.context_target_is_live(&change.target)
                || !self
                    .terminals
                    .tabs
                    .iter()
                    .any(|tab| tab.handle == change.target.handle && tab.running)
            {
                continue;
            }
            let handle = change.target.handle.clone();
            let target = change.target.clone();
            let generation = self.workspace_generation;
            let (cols, rows) = (change.cols, change.rows);
            let bridge = bridge.clone();
            cx.spawn(async move |view, cx| {
                let result = cx.background_spawn(async move {
                    bridge.resize_terminal(&handle, cols, rows)
                }).await;
                view.update(cx, |view, cx| {
                    if view.workspace_generation != generation
                        || !view.terminals.context_target_is_live(&target)
                    {
                        return;
                    }
                    if let Err(error) = result {
                        view.terminals.status = format!(
                            "Auto Resize {cols}×{rows} refused (no repeated retries until size changes): {error}"
                        );
                        cx.notify();
                    }
                }).ok();
            }).detach();
        }
    }

    /// Mouse selection is a bounded snapshot of primary VT screen rows;
    /// never forwards mouse/selection gestures to the running Host PTY.
    fn begin_terminal_line_selection(&mut self, row: usize, cx: &mut Context<Self>) {
        let index = if self.mixed_panes.has_secondary() {
            self.mixed_panes
                .active_in(PaneSlot::Primary)
                .and_then(|id| match id {
                    WorkbenchTabIdentity::Host(target) => self.terminals.index_for_context(target),
                    _ => None,
                })
        } else {
            self.terminals.active_tab
        };
        let Some((target, tab)) = index.and_then(|i| {
            self.terminals
                .context_target_at(i)
                .zip(self.terminals.tabs.get(i))
        }) else {
            self.terminal_line_selection = None;
            return;
        };
        self.terminal_line_selection = TerminalLineSelection::begin(
            target,
            tab.next_cursor,
            tab.screen
                .styled_rows()
                .into_iter()
                .map(|row| row.text)
                .collect(),
            row,
        );
        cx.notify();
    }

    fn extend_terminal_line_selection(&mut self, row: usize, cx: &mut Context<Self>) {
        if self
            .terminal_line_selection
            .as_mut()
            .is_some_and(|s| s.extend(row))
        {
            cx.notify();
        }
    }

    fn finish_terminal_line_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(selection) = self.terminal_line_selection.as_mut() {
            if selection.is_dragging() {
                selection.finish();
                cx.notify();
            }
        }
    }

    fn copy_primary_terminal_text(&mut self, selected_only: bool, cx: &mut Context<Self>) {
        let index = if self.mixed_panes.has_secondary() {
            self.mixed_panes
                .active_in(PaneSlot::Primary)
                .and_then(|id| match id {
                    WorkbenchTabIdentity::Host(target) => self.terminals.index_for_context(target),
                    _ => None,
                })
        } else {
            self.terminals.active_tab
        };
        let Some((target, tab)) = index.and_then(|i| {
            self.terminals
                .context_target_at(i)
                .zip(self.terminals.tabs.get(i))
        }) else {
            self.terminals.status = "No visible primary Host terminal to copy".into();
            cx.notify();
            return;
        };
        let text = if let Some(selection) = self.terminal_line_selection.as_ref().filter(|s| {
            self.terminals.view_mode == TerminalViewMode::Screen
                && s.is_current(&target, tab.next_cursor)
        }) {
            selection.selected_text()
        } else if selected_only {
            self.terminals.status =
                "Select VT rows first; old selection cannot copy after Host output changes".into();
            cx.notify();
            return;
        } else if self.terminals.view_mode == TerminalViewMode::Screen {
            tab.screen.contents()
        } else {
            tab.output.clone()
        };
        if text.is_empty() {
            self.terminals.status = "No visible terminal text to copy".into();
        } else {
            let mut text = text;
            if text.len() > 128 * 1024 {
                let mut start = text.len() - 128 * 1024;
                while !text.is_char_boundary(start) {
                    start += 1;
                }
                text = text[start..].to_owned();
            }
            let bytes = text.len();
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            self.terminals.status = format!("Copied {bytes} bytes of plain terminal text");
        }
        cx.notify();
    }

    /// Committed TextInput/IME text and raw control keys use the same
    /// identity-guarded, bounded Host queue. No duplicate local command bar.
    fn enqueue_terminal_text(&mut self, encoded: String, cx: &mut Context<Self>) -> bool {
        if self.center_view != CenterView::Terminal {
            return false;
        }
        let Some(tab) = self.terminals.active() else {
            return false;
        };
        if !tab.running {
            return false;
        }
        let Some(target) = self
            .terminals
            .active_tab
            .and_then(|index| self.terminals.context_target_at(index))
        else {
            return false;
        };
        if !self.terminals.context_target_is_live(&target) {
            return false;
        }
        if encoded.is_empty() {
            return false;
        }
        if self.terminal_key_queue.len() >= 512
            || self
                .terminal_key_queue
                .iter()
                .map(String::len)
                .sum::<usize>()
                + encoded.len()
                > 4096
        {
            self.terminals.status = "Terminal key queue full; stopped accepting input".into();
            cx.notify();
            return true;
        }
        if self.terminal_key_queue_target.as_ref() != Some(&target) {
            self.terminal_key_queue.clear();
            self.terminal_key_queue_target = Some(target);
        }
        self.terminal_key_queue.push_back(encoded);
        self.terminal_poll_cadence.record_activity(Instant::now());
        self.flush_terminal_key_queue(cx);
        true
    }

    /// Native TextInput callbacks may be delayed until after focus has moved.
    /// The focused pane and globally active Host identity must still agree.
    fn enqueue_terminal_text_for_pane(
        &mut self,
        pane: PaneSlot,
        encoded: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.mixed_panes.has_secondary() {
            let Some(active_target) = self
                .terminals
                .active_tab
                .and_then(|index| self.terminals.context_target_at(index))
            else {
                return false;
            };
            if !self
                .mixed_panes
                .authorizes_terminal_input(pane, &active_target)
                || !self.terminals.context_target_is_live(&active_target)
            {
                return false;
            }
        } else if pane != PaneSlot::Primary {
            return false;
        }
        self.enqueue_terminal_text(encoded, cx)
    }

    /// Native IME callbacks are asynchronous. The pane and Host identity
    /// can remain selected even after actual OS keyboard focus moved into
    /// Explorer, Settings, or another app. Never forward those stale commits.
    fn enqueue_terminal_ime_text_for_pane(
        &mut self,
        pane: PaneSlot,
        input_has_os_focus: bool,
        encoded: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if !input_has_os_focus {
            return false;
        }
        if self.mixed_panes.has_secondary() {
            let Some(active_target) = self
                .terminals
                .active_tab
                .and_then(|index| self.terminals.context_target_at(index))
            else {
                return false;
            };
            if !self.mixed_panes.authorizes_native_terminal_commit(
                pane,
                &active_target,
                input_has_os_focus,
            ) {
                return false;
            }
        }
        self.enqueue_terminal_text_for_pane(pane, encoded, cx)
    }

    fn send_terminal_key(
        &mut self,
        pane: PaneSlot,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(encoded) = encode_terminal_key(
            &event.keystroke.key,
            event.keystroke.key_char.as_deref(),
            event.keystroke.modifiers.control,
            event.keystroke.modifiers.alt,
            event.keystroke.modifiers.platform,
        ) else {
            return false;
        };
        let raw_ascii = !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.platform
            && encoded.len() == 1
            && (encoded.as_bytes()[0].is_ascii_graphic() || encoded == " ");
        let sent = self.enqueue_terminal_text_for_pane(pane, encoded.clone(), cx);
        if sent && raw_ascii {
            // A subsequent Windows TextInput callback may duplicate this
            // exact KeyDown. Do not suppress TextInput unless KeyDown really
            // reached the selected Host input queue.
            let input = match pane {
                PaneSlot::Primary => &self.terminal_input,
                PaneSlot::Secondary => &self.secondary_terminal_input,
            };
            input.update(cx, |input, _| input.note_terminal_ascii_key(&encoded));
        }
        sent
    }

    /// Do not let both native pane surfaces forward keys via the shared
    /// TerminalDock active index. The clicked/focused pane must still own
    /// the exact authenticated Host tab at dispatch time.
    fn send_terminal_key_for_pane(
        &mut self,
        pane: PaneSlot,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.mixed_panes.has_secondary() {
            let Some(target) = self.mixed_panes.focused_host_target_for_input(pane) else {
                return false;
            };
            if !self.terminals.context_target_is_live(target)
                || self.terminals.index_for_context(target) != self.terminals.active_tab
            {
                return false;
            }
        } else if pane != PaneSlot::Primary {
            return false;
        }
        if event.keystroke.modifiers.control
            && !event.keystroke.modifiers.shift
            && event.keystroke.key.eq_ignore_ascii_case("v")
        {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                return self.enqueue_terminal_text_for_pane(pane, text, cx);
            }
            return true;
        }
        // KeyDown and the native IME/TextInput commit are two paths to the
        // same Host. This must apply to BOTH panes, not just primary.
        if alera_gpui_desktop::terminal::is_text_input_keystroke(
            &event.keystroke.key,
            event.keystroke.key_char.as_deref(),
            event.keystroke.modifiers.control,
            event.keystroke.modifiers.alt,
            event.keystroke.modifiers.platform,
        ) {
            return false;
        }
        self.send_terminal_key(pane, event, cx)
    }

    fn flush_terminal_key_queue(&mut self, cx: &mut Context<Self>) {
        if self.terminal_key_sending || self.terminal_key_queue.is_empty() {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.status = "Cannot send raw terminal keys without CLI".into();
            self.terminal_key_queue.clear();
            self.terminal_key_queue_target = None;
            cx.notify();
            return;
        };
        let Some(tab) = self.terminals.active() else {
            self.terminal_key_queue.clear();
            self.terminal_key_queue_target = None;
            return;
        };
        let active_target = self
            .terminals
            .active_tab
            .and_then(|index| self.terminals.context_target_at(index));
        if !tab.running
            || !active_target
                .as_ref()
                .is_some_and(|target| self.terminals.context_target_is_live(target))
            || self.terminal_key_queue_target.as_ref() != active_target.as_ref()
        {
            self.terminal_key_queue.clear();
            self.terminal_key_queue_target = None;
            return;
        }
        let handle = tab.handle.clone();
        let workspace_id = tab.workspace_id.clone();
        let tab_id = tab.tab_id.clone();
        let sent_target = active_target.expect("live active terminal target exists");
        let generation = self.workspace_generation;
        let mut text = String::new();
        for _ in 0..self.terminal_key_queue.len().min(64) {
            let Some(next) = self.terminal_key_queue.front() else {
                break;
            };
            if text.len() + next.len() > 4096 {
                break;
            }
            text.push_str(&self.terminal_key_queue.pop_front().unwrap());
        }
        if text.is_empty() {
            return;
        }
        // Native's authenticated CLI stream is long-lived. Once this exact
        // Host tab is observed, raw input never needs per-key CLI spawn.
        let stream_result = self.terminal_stream.as_mut().and_then(|stream| {
            if stream.is_observing(&handle, &workspace_id, &tab_id) {
                Some(stream.write_observed(&handle, &workspace_id, &tab_id, text.as_bytes()))
            } else {
                None
            }
        });
        match stream_result {
            Some(true) => {
                self.terminal_key_sending = false;
                cx.notify();
                return;
            }
            Some(false) => {
                // Keep byte/keystroke ordering when the bounded private IPC
                // pipe is saturated; never race it with a new CLI process.
                self.terminal_key_queue.push_front(text);
                return;
            }
            None => {}
        }
        self.terminal_key_sending = true;
        cx.spawn(async move |view, cx| {
            let send_handle = handle.clone();
            let result = cx
                .background_spawn(async move { bridge.write_terminal(&send_handle, &text, false) })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.terminal_key_sending = false;
                let active_target = view
                    .terminals
                    .active_tab
                    .and_then(|index| view.terminals.context_target_at(index));
                let active_is_live = active_target
                    .as_ref()
                    .is_some_and(|target| view.terminals.context_target_is_live(target));
                let reply_is_for_active = active_target.as_ref() == Some(&sent_target);
                if raw_key_reply_may_drain(
                    &sent_target,
                    view.terminal_key_queue_target.as_ref(),
                    active_target.as_ref(),
                    active_is_live,
                    result.is_ok(),
                ) {
                    // A's late completion must let B's queued keys continue.
                    view.flush_terminal_key_queue(cx);
                } else {
                    view.terminal_key_queue.clear();
                    view.terminal_key_queue_target = None;
                    if reply_is_for_active {
                        if let Err(error) = result {
                            view.terminals.status =
                                format!("Raw terminal key send failed: {error}");
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_terminal_output(&mut self, cx: &mut Context<Self>) {
        if self.terminal_read_loading.is_some() {
            return;
        }
        if let Some(tab) = self.terminals.active() {
            if self.terminal_stream.as_ref().is_some_and(|bridge| {
                bridge.is_observing(&tab.handle, &tab.workspace_id, &tab.tab_id)
            }) {
                return;
            }
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.status = "CLI unavailable".into();
            cx.notify();
            return;
        };
        let Some(ticket) = self.terminals.active_ticket() else {
            return;
        };
        let handle = ticket.handle.clone();
        let cursor = ticket.cursor;
        let max_bytes = ticket.max_bytes;
        let generation = self.workspace_generation;
        self.terminal_read_loading = Some(handle.clone());
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    bridge.read_terminal_bounded(&handle, cursor, max_bytes)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.terminals.workspace_generation() != generation {
                    return;
                }
                view.terminal_read_loading = None;
                let mut needs_repaint = false;
                match result {
                    Ok(chunk) => {
                        if view.terminals.active_ticket().as_ref() == Some(&ticket) {
                            // Successful reads with no new output still
                            // reset previous failure backoff.
                            view.terminal_poll.reset();
                        }
                        let had_output = !chunk.text.is_empty();
                        if view
                            .terminals
                            .apply_workspace_read(generation, &ticket, chunk)
                        {
                            if had_output {
                                view.terminal_poll_cadence.record_activity(Instant::now());
                            }
                            needs_repaint = true;
                        }
                    }
                    Err(error) => {
                        if view.terminals.active_ticket().as_ref() == Some(&ticket) {
                            view.terminal_poll.failed();
                            view.terminals.status = format!("Terminal read failed: {error}");
                            needs_repaint = true;
                        }
                    }
                }
                // Empty read responses are common; painting the entire
                // Workbench at each idle poll was needless UI thread work.
                if needs_repaint {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn valid_editor_layout_key(relative: &Path) -> bool {
        relative.components().next().is_some()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
    }

    /// Only existing, file-backed editor tabs have a cross-launch identity.
    /// Keep their keys workspace-relative and treat saved strings as inert;
    /// unsaved drafts are recovered separately by EditorDraftStore.
    fn editor_layout_keys(&self, cx: &Context<Self>) -> Vec<(WorkbenchTabIdentity, String)> {
        let mut keys = Vec::new();
        for pane in [PaneSlot::Primary, PaneSlot::Secondary] {
            if let Some(editor) = self.editor_for_pane(pane) {
                for (instance, path) in editor.read(cx).file_backed_tab_instances() {
                    let Ok(relative) = path.strip_prefix(self.model.root()) else {
                        continue;
                    };
                    if Self::valid_editor_layout_key(relative) {
                        keys.push((
                            WorkbenchTabIdentity::Editor {
                                editor_epoch: self.editor_epoch,
                                instance,
                            },
                            relative.to_string_lossy().into_owned(),
                        ));
                    }
                }
            }
        }
        keys
    }

    fn load_mixed_pane_layout(&mut self) {
        self.mixed_layout_restore = None;
        self.mixed_layout_read_only = false;
        let Some(store) = self.session_store.as_ref() else {
            return;
        };
        let result = store
            .workbench_layout_path(self.model.root())
            .and_then(|path| SavedWorkbenchLayoutV1::load_from_path(&path));
        match result {
            Ok(Some(saved)) => {
                if saved.axis == SavedWorkbenchAxis::Vertical
                    && self.mixed_panes.axis() != PaneAxis::Vertical
                {
                    self.mixed_panes.toggle_axis();
                }
                self.mixed_layout_restore = Some(saved);
            }
            Ok(None) => {}
            Err(error) => {
                // Do not overwrite a malformed or unknown-version snapshot.
                self.mixed_layout_read_only = true;
                eprintln!("Alera GPUI: mixed pane layout recovery kept: {error}");
            }
        }
    }

    fn save_mixed_pane_layout(&mut self, cx: &Context<Self>) {
        if self.mixed_layout_read_only {
            return;
        }
        let Some(store) = self.session_store.as_ref() else {
            return;
        };
        let Ok(path) = store.workbench_layout_path(self.model.root()) else {
            return;
        };
        let keys = self.editor_layout_keys(cx);
        let key_for = |identity: &WorkbenchTabIdentity| {
            keys.iter()
                .find(|(id, _)| id == identity)
                .map(|(_, key)| key.clone())
        };
        let pane_keys = |slot: PaneSlot| {
            self.mixed_panes
                .tabs_in(slot)
                .iter()
                .filter_map(&key_for)
                .collect::<Vec<_>>()
        };
        let saved = SavedWorkbenchLayoutV1 {
            version: WORKBENCH_LAYOUT_VERSION,
            split_enabled: self.mixed_panes.has_secondary(),
            axis: if self.mixed_panes.axis() == PaneAxis::Horizontal {
                SavedWorkbenchAxis::Horizontal
            } else {
                SavedWorkbenchAxis::Vertical
            },
            focused_pane: if self.mixed_panes.focused_pane() == PaneSlot::Primary {
                SavedWorkbenchPane::Primary
            } else {
                SavedWorkbenchPane::Secondary
            },
            primary_editors: pane_keys(PaneSlot::Primary),
            secondary_editors: pane_keys(PaneSlot::Secondary),
            primary_active_editor: self
                .mixed_panes
                .active_in(PaneSlot::Primary)
                .and_then(&key_for),
            secondary_active_editor: self
                .mixed_panes
                .active_in(PaneSlot::Secondary)
                .and_then(&key_for),
        };
        if let Err(error) = saved.save_to_path(&path) {
            eprintln!("Alera GPUI: could not save mixed pane layout: {error}");
        } else {
            self.mixed_layout_restore = None;
        }
    }

    /// Recover only files that are validated by the ordinary EditorTabs
    /// path guard. A saved relative key never authorizes reading outside
    /// this workspace and never restores a Host session.
    fn restore_file_tabs_from_pane_layout(&mut self, cx: &mut Context<Self>) {
        let Some(saved) = self.mixed_layout_restore.clone() else {
            return;
        };
        let root = self.model.root().to_path_buf();
        let mut restored = 0_usize;
        for key in saved
            .primary_editors
            .iter()
            .chain(saved.secondary_editors.iter())
            .take(MAX_EDITOR_TABS)
        {
            let relative = Path::new(key);
            if !Self::valid_editor_layout_key(relative) {
                eprintln!("Alera GPUI: refused unsafe persisted editor path");
                continue;
            }
            let path = root.join(relative);
            if let Some(editor) = self.editor.clone() {
                if editor
                    .update(cx, |editor, cx| editor.open_in_place(&root, &path, cx))
                    .is_ok()
                {
                    restored += 1;
                }
            } else {
                let mut tabs = EditorTabs::new(self.draft_store());
                if tabs.open_file(&root, &path).is_ok() {
                    self.install_editor(tabs, cx);
                    restored += 1;
                }
            }
        }
        if restored > 0 {
            let desired = if saved.focused_pane == SavedWorkbenchPane::Secondary {
                saved
                    .secondary_active_editor
                    .as_ref()
                    .or(saved.primary_active_editor.as_ref())
            } else {
                saved
                    .primary_active_editor
                    .as_ref()
                    .or(saved.secondary_active_editor.as_ref())
            };
            if let Some((WorkbenchTabIdentity::Editor { instance, .. }, _)) = self
                .editor_layout_keys(cx)
                .into_iter()
                .find(|(_, key)| desired.is_some_and(|wanted| wanted == key))
            {
                if let Some(editor) = self.editor.clone() {
                    editor.update(cx, |editor, cx| {
                        editor.activate_tab_instance(instance, cx);
                    });
                }
            }
            self.center_view = CenterView::Editor;
            self.model.set_status(format!(
                "Restored {restored} editable file tab(s) into GPUI panes"
            ));
        }
    }

    fn apply_restored_mixed_panes(&mut self, cx: &mut Context<Self>) {
        let Some(saved) = self.mixed_layout_restore.take() else {
            return;
        };
        let keys = self.editor_layout_keys(cx);
        if keys.is_empty()
            && (!saved.primary_editors.is_empty() || !saved.secondary_editors.is_empty())
        {
            self.mixed_layout_restore = Some(saved);
            return;
        }
        for (id, key) in &keys {
            if saved.secondary_editors.contains(key) {
                self.transfer_editor_to_pane(id, PaneSlot::Secondary, cx);
            }
        }
        for wanted in [
            saved.primary_active_editor.as_ref(),
            saved.secondary_active_editor.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if let Some((id, _)) = keys.iter().find(|(_, key)| key == wanted) {
                if let WorkbenchTabIdentity::Editor { instance, .. } = id {
                    if let Some((_, owner)) = self.editor_for_instance(*instance, cx) {
                        owner.update(cx, |editor, cx| {
                            editor.activate_tab_instance(*instance, cx);
                        });
                    }
                }
                self.mixed_panes.activate(id);
            }
        }
        if !saved.split_enabled {
            self.unsplit_mixed_panes(cx);
        } else if saved.focused_pane == SavedWorkbenchPane::Primary {
            self.mixed_panes.focus_pane(PaneSlot::Primary);
        } else {
            self.mixed_panes.focus_pane(PaneSlot::Secondary);
        }
    }

    /// Project both original state owners into one stable, mixed-order tab
    /// strip. The original editor buffers and Host PTYs remain authoritative.
    fn sync_mixed_tabs(&mut self, cx: &mut Context<Self>) {
        let mut observed = Vec::new();
        let mut preferred = None;
        for (index, tab) in self.terminals.tabs.iter().enumerate() {
            let Some(target) = self.terminals.context_target_at(index) else {
                continue;
            };
            let identity = WorkbenchTabIdentity::Host(target);
            let is_agent = self.terminals.sessions.iter().any(|session| {
                session.handle == tab.handle
                    && session.workspace_id == tab.workspace_id
                    && session.tab_id == tab.tab_id
                    && session.agent_type.is_some()
            });
            observed.push(WorkbenchTab {
                identity: identity.clone(),
                kind: if is_agent {
                    WorkbenchTabKind::Agent
                } else {
                    WorkbenchTabKind::Terminal
                },
                title: tab.title.clone(),
                dirty: false,
                running: tab.running,
            });
            if self.center_view == CenterView::Terminal && self.terminals.active_tab == Some(index)
            {
                preferred = Some(identity);
            }
        }
        for slot in [PaneSlot::Primary, PaneSlot::Secondary] {
            if let Some(editor) = self.editor_for_pane(slot) {
                let editor = editor.read(cx);
                let active = editor.active_tab_instance();
                for (instance, label) in editor.tab_instances() {
                    let identity = WorkbenchTabIdentity::Editor {
                        editor_epoch: self.editor_epoch,
                        instance,
                    };
                    if self.center_view == CenterView::Editor
                        && self.mixed_panes.focused_pane() == slot
                        && active == Some(instance)
                    {
                        preferred = Some(identity.clone());
                    }
                    observed.push(WorkbenchTab {
                        identity,
                        kind: WorkbenchTabKind::Editor,
                        title: label.title,
                        dirty: label.dirty,
                        running: true,
                    });
                }
            }
        }
        self.mixed_tabs
            .reconcile(self.workspace_generation, observed, preferred.clone());
        let live_tabs: Vec<_> = self
            .mixed_tabs
            .entries()
            .iter()
            .map(|tab| tab.identity.clone())
            .collect();
        self.mixed_panes.reconcile(&live_tabs);
        // A new tab opened through NativeEditor's own Ctrl+N/Save As is
        // initially unknown to WorkbenchPaneLayout. Reconcile appends new
        // identities to Primary by default; correct that projection to
        // whichever native view *actually owns* the editable buffer.
        for slot in [PaneSlot::Primary, PaneSlot::Secondary] {
            if let Some(owner) = self.editor_for_pane(slot) {
                for (instance, _) in owner.read(cx).tab_instances() {
                    let id = WorkbenchTabIdentity::Editor {
                        editor_epoch: self.editor_epoch,
                        instance,
                    };
                    if !self.mixed_panes.tabs_in(slot).contains(&id) {
                        self.mixed_panes.move_to(&id, slot);
                    }
                }
            }
        }
        // NativeEditor can open/activate a tab through its own shortcuts or
        // async Save As callbacks. Its currently selected owner must follow
        // the actual pane holding that identity, not remain in a stale pane.
        if let Some(identity) = preferred {
            self.mixed_panes.activate(&identity);
        }
        self.apply_restored_mixed_panes(cx);
        // Classic has one active content tab per pane. In unsplit mode the
        // mixed strip is authoritative even after session restoration, when
        // the legacy center_view flag can still point at an empty Terminal.
        if !self.mixed_panes.has_secondary() {
            if let Some(active) = self.mixed_tabs.active() {
                self.mixed_panes.activate(active);
            }
        }
        if !self
            .mixed_tab_context
            .as_ref()
            .is_some_and(|id| self.mixed_tabs.contains(id))
        {
            self.mixed_tab_context = None;
        }
    }

    /// Move a live EditorSession between two real NativeEditor view owners.
    /// Unlike a plain pane-layout move this transfers its *existing*
    /// document, drafts, undo history and identity without disk reload.
    fn transfer_editor_to_pane(
        &mut self,
        id: &WorkbenchTabIdentity,
        destination: PaneSlot,
        cx: &mut Context<Self>,
    ) -> bool {
        let WorkbenchTabIdentity::Editor {
            editor_epoch,
            instance,
        } = id
        else {
            return self.mixed_panes.move_to(id, destination);
        };
        if *editor_epoch != self.editor_epoch {
            return false;
        }
        let Some((source, source_editor)) = self.editor_for_instance(*instance, cx) else {
            return false;
        };
        if source == destination {
            return false;
        }
        self.editor_transfer_in_progress = true;
        let detached =
            source_editor.update(cx, |editor, cx| editor.take_tab_for_pane(*instance, cx));
        let Some(detached) = detached else {
            self.editor_transfer_in_progress = false;
            self.model.set_status(
                "Cannot move Editor tab while Chinese IME is composing; commit or cancel composition first",
            );
            cx.notify();
            return false;
        };
        let result = if let Some(target) = self.editor_for_pane(destination) {
            target.update(cx, |editor, cx| editor.install_tab_from_pane(detached, cx))
        } else {
            let mut tabs = EditorTabs::new(self.draft_store());
            match tabs.insert_from_pane(detached) {
                Ok(()) => {
                    match destination {
                        PaneSlot::Primary => self.install_editor(tabs, cx),
                        PaneSlot::Secondary => self.install_secondary_editor(tabs, cx),
                    }
                    Ok(())
                }
                Err(rejected) => Err(rejected),
            }
        };
        let success = match result {
            Ok(()) => {
                if source_editor.read(cx).tabs_empty() {
                    match source {
                        PaneSlot::Primary => self.editor = None,
                        PaneSlot::Secondary => self.secondary_editor = None,
                    }
                }
                self.mixed_panes.move_to(id, destination)
            }
            Err(rejected) => {
                // A capacity/duplicate failure is reversible because the
                // source stayed alive until the destination accepted its tab.
                let restored = source_editor
                    .update(cx, |editor, cx| editor.install_tab_from_pane(rejected, cx))
                    .is_ok();
                self.model.set_status(if restored {
                    "Editor pane move refused; original dirty buffer was kept"
                } else {
                    "Editor pane move failed; keep this workspace open for draft recovery"
                });
                false
            }
        };
        self.editor_transfer_in_progress = false;
        if success {
            self.sync_mixed_tabs(cx);
            self.save_mixed_pane_layout(cx);
        }
        cx.notify();
        success
    }

    fn unsplit_mixed_panes(&mut self, cx: &mut Context<Self>) {
        let secondary = self.mixed_panes.tabs_in(PaneSlot::Secondary).to_vec();
        for id in secondary {
            if matches!(id, WorkbenchTabIdentity::Editor { .. }) {
                if !self.transfer_editor_to_pane(&id, PaneSlot::Primary, cx) {
                    return;
                }
            }
        }
        self.mixed_panes.close_secondary();
        self.save_mixed_pane_layout(cx);
        cx.notify();
    }

    fn mixed_tab_is_current(&self, id: &WorkbenchTabIdentity, cx: &Context<Self>) -> bool {
        match id {
            WorkbenchTabIdentity::Host(target) => {
                self.terminals.index_for_context(target).is_some()
            }
            WorkbenchTabIdentity::Editor {
                editor_epoch,
                instance,
            } => {
                *editor_epoch == self.editor_epoch
                    && self.editor_for_instance(*instance, cx).is_some()
            }
        }
    }

    fn focus_mixed_tab(
        &mut self,
        id: &WorkbenchTabIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.mixed_tab_is_current(id, cx) {
            self.mixed_tab_context = None;
            cx.notify();
            return;
        }
        self.mixed_tab_context = None;
        self.explorer_preview_active = false;
        self.mixed_tabs.select(id);
        self.mixed_panes.activate(id);
        match id {
            WorkbenchTabIdentity::Host(target) => {
                if let Some(index) = self.terminals.index_for_context(target) {
                    if self.terminals.active_tab != Some(index) {
                        self.terminal_key_queue.clear();
                        self.terminal_key_queue_target = None;
                        self.clear_terminal_text_inputs(cx);
                    }
                    self.terminals.activate(index);
                    self.center_view = CenterView::Terminal;
                    self.save_terminal_layout();
                    self.refresh_terminal_output(cx);
                    if self.mixed_panes.focused_pane() == PaneSlot::Secondary {
                        window.focus(&self.secondary_terminal_input.read(cx).focus_handle(cx), cx);
                    } else {
                        window.focus(&self.terminal_input.read(cx).focus_handle(cx), cx);
                    }
                }
            }
            WorkbenchTabIdentity::Editor { instance, .. } => {
                // A focused Editor has a different native text input owner;
                // cancel any leftover Terminal phonetic composition first.
                self.clear_terminal_text_inputs(cx);
                if let Some((_, editor)) = self.editor_for_instance(*instance, cx) {
                    if editor.update(cx, |editor, cx| editor.activate_tab_instance(*instance, cx)) {
                        self.center_view = CenterView::Editor;
                        window.focus(&editor.read(cx).focus_handle(cx), cx);
                    }
                }
            }
        }
        self.save_mixed_pane_layout(cx);
        cx.notify();
    }

    /// Pointer focus on the primary terminal must select that pane's
    /// authenticated Host target before raw keys enter TerminalDock.
    fn focus_primary_mixed_terminal(&mut self, cx: &mut Context<Self>) {
        if !self.mixed_panes.has_secondary() {
            return;
        }
        let Some(WorkbenchTabIdentity::Host(target)) =
            self.mixed_panes.active_in(PaneSlot::Primary).cloned()
        else {
            return;
        };
        if !self.terminals.context_target_is_live(&target) {
            return;
        }
        let Some(index) = self.terminals.index_for_context(&target) else {
            return;
        };
        self.mixed_panes.focus_pane(PaneSlot::Primary);
        if self.terminals.active_tab != Some(index) {
            self.terminal_key_queue.clear();
            self.terminal_key_queue_target = None;
            self.clear_terminal_text_inputs(cx);
        }
        self.terminals.activate(index);
        self.explorer_preview_active = false;
        self.center_view = CenterView::Terminal;
        self.refresh_terminal_output(cx);
        self.save_mixed_pane_layout(cx);
        cx.notify();
    }

    fn close_mixed_tab(&mut self, id: &WorkbenchTabIdentity, cx: &mut Context<Self>) {
        if !self.mixed_tab_is_current(id, cx) {
            self.mixed_tab_context = None;
            cx.notify();
            return;
        }
        self.mixed_tab_context = None;
        match id {
            WorkbenchTabIdentity::Host(target) => {
                if let Some(index) = self.terminals.index_for_context(target) {
                    self.terminals.close(index);
                    if self
                        .terminal_panes
                        .secondary_index(&self.terminals)
                        .is_none()
                    {
                        self.terminal_panes.close_secondary();
                    }
                    self.terminals.status = "Preview closed; Host PTY remains alive".into();
                    self.save_terminal_layout();
                    self.refresh_terminal_output(cx);
                }
            }
            WorkbenchTabIdentity::Editor { instance, .. } => {
                if let Some((slot, editor)) = self.editor_for_instance(*instance, cx) {
                    let closed =
                        editor.update(cx, |editor, cx| editor.close_tab_instance(*instance, cx));
                    self.model
                        .set_status(editor.read(cx).status_text().to_owned());
                    if closed && editor.read(cx).tabs_empty() {
                        match slot {
                            PaneSlot::Primary => self.editor = None,
                            PaneSlot::Secondary => self.secondary_editor = None,
                        }
                        if self.editor.is_none() && self.secondary_editor.is_none() {
                            self.center_view = CenterView::Terminal;
                        }
                    }
                }
            }
        }
        self.sync_mixed_tabs(cx);
        self.save_mixed_pane_layout(cx);
        cx.notify();
    }

    fn reorder_mixed_tab(
        &mut self,
        drag: &MixedTabDrag,
        target: &WorkbenchTabIdentity,
        cx: &mut Context<Self>,
    ) {
        if drag.workspace_generation != self.workspace_generation
            || !self.mixed_tab_is_current(&drag.identity, cx)
            || !self.mixed_tab_is_current(target, cx)
        {
            return;
        }
        if self.mixed_tabs.move_to(&drag.identity, target) {
            cx.notify();
        }
    }

    fn apply_mixed_tab_context(
        &mut self,
        action: MixedContextAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.mixed_tab_context.take() else {
            return;
        };
        if !self.mixed_tab_is_current(&id, cx) || !self.mixed_tabs.contains(&id) {
            cx.notify();
            return;
        }
        match action {
            MixedContextAction::Focus => self.focus_mixed_tab(&id, window, cx),
            MixedContextAction::Close => self.close_mixed_tab(&id, cx),
            MixedContextAction::CloseOthers | MixedContextAction::CloseToRight => {
                let entries = self.mixed_tabs.entries();
                let Some(index) = entries.iter().position(|tab| tab.identity == id) else {
                    return;
                };
                let candidates: Vec<_> = entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, item)| {
                        (i != index
                            && (matches!(action, MixedContextAction::CloseOthers) || i > index))
                            .then_some(item.identity.clone())
                    })
                    .collect();
                for target in candidates {
                    // Editor dirty-buffer protection and Host PTY ownership
                    // are enforced by the original owners for every close.
                    self.close_mixed_tab(&target, cx);
                }
            }
            MixedContextAction::MoveLeft | MixedContextAction::MoveRight => {
                let entries = self.mixed_tabs.entries();
                if let Some(index) = entries.iter().position(|tab| tab.identity == id) {
                    let other = match action {
                        MixedContextAction::MoveLeft => index.checked_sub(1),
                        _ => (index + 1 < entries.len()).then_some(index + 1),
                    }
                    .and_then(|index| entries.get(index).map(|tab| tab.identity.clone()));
                    if let Some(other) = other {
                        if matches!(action, MixedContextAction::MoveLeft) {
                            self.mixed_tabs.move_to(&id, &other);
                        } else {
                            self.mixed_tabs.move_to(&id, &other);
                        }
                    }
                }
            }
            MixedContextAction::SecondPane => {
                // Host stays in its Host-owned runtime; Editor transfers its
                // existing EditorSession into an independent native viewport.
                if self.transfer_editor_to_pane(&id, PaneSlot::Secondary, cx) {
                    self.focus_mixed_tab(&id, window, cx);
                    self.refresh_terminal_split_output(cx);
                }
            }
            MixedContextAction::FirstPane => {
                if self.transfer_editor_to_pane(&id, PaneSlot::Primary, cx) {
                    self.focus_mixed_tab(&id, window, cx);
                    self.refresh_terminal_split_output(cx);
                }
            }
        }
        self.save_mixed_pane_layout(cx);
        cx.notify();
    }

    fn toggle_terminal_split(&mut self, cx: &mut Context<Self>) {
        if self.terminal_panes.secondary.is_some() {
            self.terminal_panes.close_secondary();
            self.terminals.status =
                "Second terminal preview pane closed; Host sessions remain alive".into();
        } else if self.terminal_panes.split(&self.terminals) {
            self.terminals.status =
                "Split Terminal opened with an existing attached session".into();
            self.refresh_terminal_split_output(cx);
        } else {
            self.terminals.status =
                "Attach a second running Host session before splitting Terminal".into();
        }
        cx.notify();
    }

    fn switch_terminal_split_focus(&mut self, cx: &mut Context<Self>) {
        if self.terminal_panes.switch_focus(&mut self.terminals) {
            self.terminal_key_queue.clear();
            self.terminal_key_queue_target = None;
            self.save_terminal_layout();
            self.refresh_terminal_output(cx);
            self.refresh_terminal_split_output(cx);
        }
        cx.notify();
    }

    fn refresh_terminal_split_output(&mut self, cx: &mut Context<Self>) {
        if self.terminal_split_read_loading {
            return;
        }
        // The legacy Host-only split and the mixed Workbench split share the
        // same guarded read path. In a mixed split, poll whichever Host pane
        // is *not* focused; don't allow the global TerminalDock active tab to
        // cause one Host to be polled twice while the other stays frozen.
        let index = if self.mixed_panes.has_secondary() {
            self.mixed_panes
                .background_host_target()
                .and_then(|target| self.terminals.index_for_context(target))
        } else {
            self.terminal_panes.secondary_index(&self.terminals)
        };
        let Some(index) = index else {
            return;
        };
        let Some(ticket) = self.terminals.ticket_at(index) else {
            return;
        };
        if let Some(tab) = self.terminals.tabs.get(index) {
            if self.terminal_stream.as_ref().is_some_and(|bridge| {
                bridge.is_observing(&tab.handle, &tab.workspace_id, &tab.tab_id)
            }) {
                return;
            }
        }
        if self.terminals.active_ticket().as_ref() == Some(&ticket) {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            return;
        };
        let generation = self.workspace_generation;
        self.terminal_split_read_loading = true;
        cx.spawn(async move |view, cx| {
            let handle = ticket.handle.clone();
            let cursor = ticket.cursor;
            let max_bytes = ticket.max_bytes;
            let result = cx
                .background_spawn(async move {
                    bridge.read_terminal_bounded(&handle, cursor, max_bytes)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.terminal_split_read_loading = false;
                let still_visible = if view.mixed_panes.has_secondary() {
                    view.mixed_panes
                        .background_host_target()
                        .and_then(|target| view.terminals.index_for_context(target))
                        .and_then(|index| view.terminals.ticket_at(index))
                        .as_ref()
                        == Some(&ticket)
                } else {
                    view.terminal_panes
                        .secondary_index(&view.terminals)
                        .and_then(|index| view.terminals.ticket_at(index))
                        .as_ref()
                        == Some(&ticket)
                };
                if !still_visible {
                    return;
                }
                if let Ok(chunk) = result {
                    let had_output = !chunk.text.is_empty();
                    if view
                        .terminals
                        .apply_workspace_read(generation, &ticket, chunk)
                    {
                        if had_output {
                            view.terminal_poll_cadence.record_activity(Instant::now());
                        }
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn pump_native_terminal_stream(&mut self, cx: &mut Context<Self>) {
        let live_views: Vec<_> = self
            .terminals
            .tabs
            .iter()
            .map(|tab| (tab.handle.clone(), tab.view_generation()))
            .collect();
        if live_views != self.terminal_stream_view_snapshot {
            // Closing/reopening a preview invalidates its old output cursor
            // even if the Host later reuses the same handle. Re-observe with
            // a fresh private CLI connection; never reuse the old view lease
            // or feed a stale frame into a newly created emulator surface.
            self.terminal_stream = None;
            self.terminals.invalidate_stream_observers();
            self.terminal_stream_retry.recovered(Instant::now());
            self.terminal_stream_view_snapshot = live_views;
        }
        let mut visible = Vec::new();
        if let Some(tab) = self.terminals.active() {
            if tab.running {
                visible.push((
                    tab.handle.clone(),
                    tab.workspace_id.clone(),
                    tab.tab_id.clone(),
                ));
            }
        }
        let split_index = if self.mixed_panes.has_secondary() {
            self.mixed_panes
                .background_host_target()
                .and_then(|target| self.terminals.index_for_context(target))
        } else {
            self.terminal_panes.secondary_index(&self.terminals)
        };
        if let Some(tab) = split_index.and_then(|index| self.terminals.tabs.get(index)) {
            if tab.running && !visible.iter().any(|(id, _, _)| id == &tab.handle) {
                visible.push((
                    tab.handle.clone(),
                    tab.workspace_id.clone(),
                    tab.tab_id.clone(),
                ));
            }
        }
        // Even when a newly restarted Host temporarily reports zero live
        // sessions, display-only Native tabs may still have valid persistent
        // PTY Workers. Allow a private recover-only stream for those tabs;
        // it cannot mint a Shell or authorize input from a stale listing.
        if visible.is_empty() && self.terminals.tabs.is_empty() && self.terminal_stream.is_none() {
            return;
        }
        if (!visible.is_empty() || !self.terminals.tabs.is_empty())
            && self.terminal_stream.is_none()
            && self.terminal_stream_retry.can_attempt(Instant::now())
        {
            if let Some(cli) = self.runtime_bridge.as_ref() {
                match NativeTerminalStreamBridge::start(cli) {
                    Ok(bridge) => self.terminal_stream = Some(bridge),
                    Err(_) => self.terminal_stream_retry.failed(Instant::now()),
                }
            }
        }
        let Some(stream) = self.terminal_stream.as_mut() else {
            return;
        };
        // Restore only the Host Session map for inactive previews, without
        // subscribing background TUI output. A visible Terminal uses the
        // normal authenticated observe + VT snapshot path below.
        for tab in &self.terminals.tabs {
            if !visible.iter().any(|(id, _, _)| id == &tab.handle) {
                stream.ensure_recovering(&tab.handle, &tab.workspace_id, &tab.tab_id);
            }
        }
        for (session_id, workspace_id, tab_id) in &visible {
            stream.ensure_observing(session_id, workspace_id, tab_id);
        }
        let updates = stream.poll_updates();
        #[cfg(debug_assertions)]
        let isolated_update_count = updates.len();
        let parse_started = Instant::now();
        let mut parsed_bytes = 0;
        let mut repaint = false;
        let mut disconnected = false;
        for update in updates {
            match update {
                StreamViewUpdate::RecoveryReady {
                    session_id,
                    workspace_id,
                    tab_id,
                } => {
                    // Only this still-open, same-workspace preview can
                    // authorize an inventory refresh after Host recovery.
                    // Do not mark a tab running or send input on the basis
                    // of the private recovery reply alone.
                    if self.terminals.tabs.iter().any(|tab| {
                        tab.handle == session_id
                            && tab.workspace_id == workspace_id
                            && tab.tab_id == tab_id
                    }) {
                        self.refresh_terminals(cx);
                    }
                }
                StreamViewUpdate::Snapshot {
                    identity,
                    bytes,
                    cursor,
                    rows,
                    cols,
                } => {
                    if self.terminals.apply_stream_snapshot(
                        &identity.session_id,
                        &identity.workspace_id,
                        &identity.tab_id,
                        cursor,
                        rows,
                        cols,
                        &bytes,
                    ) {
                        self.terminal_stream_retry.recovered(Instant::now());
                        parsed_bytes += bytes.len();
                        repaint = true;
                    }
                }
                StreamViewUpdate::Output { identity, bytes } => {
                    if self.terminals.apply_stream_output(
                        &identity.session_id,
                        &identity.workspace_id,
                        &identity.tab_id,
                        &bytes,
                    ) {
                        parsed_bytes += bytes.len();
                        self.terminal_poll_cadence.record_activity(Instant::now());
                        repaint = true;
                    }
                }
                StreamViewUpdate::Failed(reason) => {
                    // A resync, failed lease, or refused input invalidates
                    // incremental output. Rehydrate from retained Host
                    // scrollback instead of keeping the broken stream alive.
                    disconnected = true;
                    self.terminals.status =
                        format!("Terminal stream: {reason}; switching to CLI read fallback");
                    repaint = true;
                }
                StreamViewUpdate::Disconnected => {
                    disconnected = true;
                    self.terminals.status =
                        "Terminal stream disconnected; using existing CLI polling".into();
                    repaint = true;
                }
            }
        }
        if let Some(stream) = self.terminal_stream.as_mut() {
            stream.note_ui_parse(parsed_bytes, parse_started.elapsed());
        }
        if disconnected {
            self.terminal_stream = None;
            self.terminals.invalidate_stream_observers();
            self.terminal_stream_retry.failed(Instant::now());
        }
        #[cfg(debug_assertions)]
        if std::env::var_os("ALERA_GPUI_ISOLATED_STREAM_DIAGNOSTICS").is_some() {
            use std::sync::atomic::{AtomicU64, Ordering};
            static LAST_REPORTED: AtomicU64 = AtomicU64::new(0);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if LAST_REPORTED.fetch_max(now, Ordering::Relaxed) < now {
                let (queued, active, pending) = self
                    .terminal_stream
                    .as_ref()
                    .map(|stream| stream.isolated_diagnostic_queues())
                    .unwrap_or((0, 0, 0));
                let tabs = self
                    .terminals
                    .tabs
                    .iter()
                    .map(|tab| (tab.next_cursor.unwrap_or(0), tab.running))
                    .collect::<Vec<_>>();
                eprintln!(
                    "GPUI_STREAM_DIAG clock={now} stream={} updates={} bytes={} q={} active={} pending={} tabs={tabs:?}",
                    self.terminal_stream.is_some(), isolated_update_count,
                    parsed_bytes, queued, active, pending,
                );
            }
        }
        if repaint {
            cx.notify();
        }
        if !self.terminal_key_sending && !self.terminal_key_queue.is_empty() {
            self.flush_terminal_key_queue(cx);
        }
    }

    fn start_terminal_polling(&mut self, cx: &mut Context<Self>) {
        // Native Host push is consumed on a 16ms visible-screen frame.
        // Legacy CLI reads retain their adaptive 100/350/1500ms rate and
        // hidden/stream-disconnected views retain the 50ms housekeeping wake.
        // One lifecycle-bound timer, never one timer per Terminal tab.
        cx.spawn(async move |view, cx| {
            let mut wake = TerminalPollCadence::WAKE_INTERVAL;
            loop {
                cx.background_executor().timer(wake).await;
                let next = view.update(cx, |view, cx| {
                    view.pump_native_terminal_stream(cx);
                    let foreground = view.center_view == CenterView::Terminal
                        || view.mixed_panes.background_host_target().is_some();
                    let next_wake = TerminalPollCadence::wake_interval(
                        view.terminal_stream.is_some(),
                        foreground,
                    );
                    if (view.terminals.active_ticket().is_none()
                        && view.mixed_panes.background_host_target().is_none())
                        || !view
                            .terminal_poll_cadence
                            .is_due(Instant::now(), foreground)
                        || !view.terminal_poll.tick()
                    {
                        return next_wake;
                    }
                    view.refresh_terminal_output(cx);
                    view.refresh_terminal_split_output(cx);
                    next_wake
                });
                match next {
                    Ok(next) => wake = next,
                    Err(_) => break,
                }
            }
        })
        .detach();
        // Housekeeping retains its existing 1.5s interval and is independent
        // of the fast output reader. Never spawn a timer per tab.
        cx.spawn(async move |view, cx| loop {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            if view
                .update(cx, |view, cx| {
                    view.terminal_tick = view.terminal_tick.wrapping_add(1);
                    view.flush_auto_terminal_resizes(cx);
                    view.poll_native_mcp_commands(cx);
                    if view.native_settings.editor_autosave_enabled && view.terminal_tick % 2 == 0 {
                        for editor in [view.editor.clone(), view.secondary_editor.clone()].into_iter().flatten() {
                            editor.update(cx, |editor, cx| editor.autosave_if_idle(cx));
                        }
                    }
                    if view.native_settings.auto_git_fetch_enabled
                        && view.last_auto_git_fetch.elapsed()
                            >= Duration::from_secs(view.native_settings.auto_git_fetch_interval_minutes * 60)
                        && !view.git_action_busy
                        && !view.mcp_workspace_create_busy
                        && !view.worktree_busy
                    {
                        view.last_auto_git_fetch = Instant::now();
                        view.run_git_action(GitUiAction::Fetch, cx);
                    }
                    if view.terminal_tick % 20 == 1 && view.mcp_ui_bridge.is_none() {
                        view.connect_mcp_ui_bridge(cx);
                    }
                    match view.terminal_create_flow.tick() {
                        TerminalCreateTick::RefreshSessions => {
                            // Retry only authenticated Host session listing.
                            // Never call terminal.create again from a timer.
                            view.refresh_terminals(cx);
                        }
                        TerminalCreateTick::TimedOut => {
                            view.terminals.status =
                                "Host confirmation timed out after 30s. Retry checking or Cancel; the PTY may still exist"
                                    .into();
                            cx.notify();
                        }
                        TerminalCreateTick::None => {}
                    }
                    if view.terminal_tick % 10 == 1 {
                        view.refresh_terminals(cx);
                    }
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
    }
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_mixed_tabs(cx);
        if self.settings_section.is_some() && !self.settings_kit_dialog_open {
            // MCP open_settings has no Window parameter. Defer Kit overlay
            // creation until after the Workbench render to avoid re-entering
            // the Root while it is painting this Workbench.
            self.settings_kit_dialog_open = true;
            let owner = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                if owner
                    .upgrade()
                    .is_some_and(|entity| entity.read(cx).settings_section.is_some())
                {
                    // MCP Settings can replace a live Quick Open / file dialog.
                    // Close the previous Kit layer so its stale content never
                    // reappears when Settings is dismissed.
                    if window.has_active_dialog(cx) {
                        window.close_all_dialogs(cx);
                    }
                    Workbench::open_kit_native_settings_dialog(owner, window, cx);
                }
            });
        }
        let terminal_font_size = self.native_settings.terminal_font_size as f32;
        let mixed_tabs = self.mixed_tabs.entries().to_vec();
        let mixed_active = self.mixed_tabs.active().cloned();
        let mixed_secondary_tab_ids = self.mixed_panes.tabs_in(PaneSlot::Secondary).to_vec();
        let mixed_secondary_active = self.mixed_panes.active_in(PaneSlot::Secondary).cloned();
        let mixed_has_secondary = self.mixed_panes.has_secondary();
        let mixed_split_horizontal = self.mixed_panes.axis() == PaneAxis::Horizontal;
        let mixed_secondary_identity = self.mixed_panes.active_in(PaneSlot::Secondary).cloned();
        let mixed_secondary_host = mixed_secondary_identity.as_ref().and_then(|id| match id {
            WorkbenchTabIdentity::Host(target) => Some(target.clone()),
            WorkbenchTabIdentity::Editor { .. } => None,
        });
        let mixed_secondary_key_target = mixed_secondary_host.clone();
        let mixed_secondary_vt_rows: Option<Vec<StyledRow>> =
            if self.terminals.view_mode == TerminalViewMode::Screen {
                mixed_secondary_host
                    .as_ref()
                    .and_then(|target| self.terminals.index_for_context(target))
                    .and_then(|index| self.terminals.tabs.get(index))
                    .map(|terminal| terminal.screen.styled_rows())
            } else {
                None
            };
        let mixed_secondary_preview = mixed_secondary_identity.as_ref().and_then(|identity| {
            let tab = self
                .mixed_tabs
                .entries()
                .iter()
                .find(|tab| &tab.identity == identity)?;
            let label = tab.title.clone();
            let preview = match identity {
                WorkbenchTabIdentity::Host(target) => {
                    let index = self.terminals.index_for_context(target)?;
                    let session = self.terminals.tabs.get(index)?;
                    match self.terminals.view_mode {
                        TerminalViewMode::Screen => session.screen.contents(),
                        TerminalViewMode::Transcript => session.output.clone(),
                    }
                }
                WorkbenchTabIdentity::Editor {
                    editor_epoch,
                    instance,
                } => {
                    if *editor_epoch != self.editor_epoch {
                        return None;
                    }
                    self.editor_for_instance(*instance, cx)
                        .and_then(|(_, editor)| editor.read(cx).tab_content_snapshot(*instance))
                        .unwrap_or_else(|| "Editor buffer no longer available".to_owned())
                }
            };
            Some((label, preview))
        });
        let mixed_drag_generation = self.workspace_generation;
        let folder_name = self
            .model
            .directory()
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| self.model.directory().display().to_string());
        let filename = self
            .model
            .selected_path()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Welcome".into());
        let content = if self.model.selected_path().is_some() {
            self.model.content().to_owned()
        } else {
            "Select a text file from Explorer on the right to preview or edit it.\n\nThis GPUI preview keeps project management on the left, Terminal in the center, and Explorer/Git on the right. Full terminal PTY ownership stays with Alera Runtime Host.".into()
        };
        let entries = self.model.entries().to_vec();
        let tree_rows = self
            .tree
            .as_ref()
            .map(|tree| tree.rows().to_vec())
            .unwrap_or_default();
        let explorer_filter_input = self.explorer_filter_input.clone();
        let explorer_filter_query = self
            .explorer_filter_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        let explorer_filter_active = !explorer_filter_query.is_empty();
        let explorer_filter_results = if explorer_filter_active {
            self.file_index
                .as_ref()
                .filter(|index| index.root() == self.model.root())
                .map(|index| index.search_explorer(&explorer_filter_query, 200))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let explorer_filter_empty = explorer_filter_results.is_empty();
        let explorer_git_badges = ExplorerGitBadges::from_snapshot(self.git_snapshot.as_ref());
        let tree_mode = self.tree_mode && self.tree.is_some();
        let tree_truncated = self.tree.as_ref().is_some_and(WorkspaceTree::truncated);
        let explorer_multi_selection = self.explorer_multi_selection.clone();
        let explorer_drag_selection: Arc<Vec<PathBuf>> =
            Arc::new(explorer_multi_selection.top_level_paths());
        let explorer_drag_generation = self.workspace_generation;
        let explorer_visible_paths: Arc<Vec<PathBuf>> = Arc::new(if explorer_filter_active {
            explorer_filter_results
                .iter()
                .map(|hit| hit.path.clone())
                .collect()
        } else if tree_mode {
            tree_rows
                .iter()
                .map(|row| self.model.root().join(&row.relative_path))
                .collect()
        } else {
            self.model
                .entries()
                .iter()
                .map(|row| row.path.clone())
                .collect()
        });
        let tabs: Vec<_> = self
            .model
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| (index, tab.title.clone(), tab.transient))
            .collect();
        let active_tab = self.model.active_tab();
        let active_primary_identity = self.mixed_panes.active_in(PaneSlot::Primary);
        let primary_is_editor = matches!(
            active_primary_identity,
            Some(WorkbenchTabIdentity::Editor { .. })
        );
        // Each pane owns an independent native editor viewport and its own
        // dirty-buffer/undo owner. No shared Entity is rendered twice.
        let mixed_editor_in_secondary = mixed_has_secondary
            && matches!(
                self.mixed_panes.active_in(PaneSlot::Secondary),
                Some(WorkbenchTabIdentity::Editor { .. })
            );
        // On restart the old read-only preview owner may restore tabs before
        // any NativeEditor/Host mixed tab exists. Classic opens the selected
        // restored file, not a misleading empty Terminal behind file chips.
        let showing_explorer_preview = (self.explorer_preview_active
            || self.mixed_tabs.entries().is_empty())
            && self.model.active_document().is_some();
        let center_is_editor = if showing_explorer_preview {
            true
        } else if active_primary_identity.is_some() {
            primary_is_editor
        } else {
            // Empty workbench fallback only; a real tab selection owns mode.
            self.center_view == CenterView::Editor
        };
        let show_editor_toolbar = center_is_editor || mixed_editor_in_secondary;
        let editor = self.editor.clone();
        let toolbar_editor = self.focused_editor();
        let has_native_editor = toolbar_editor.is_some() && !showing_explorer_preview;
        let secondary_native_editor = if mixed_editor_in_secondary {
            self.secondary_editor.clone()
        } else {
            None
        };
        let secondary_editor_live = secondary_native_editor.is_some();
        let editor_dirty = toolbar_editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).document().is_dirty());
        let discard_pending = toolbar_editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).discard_armed());
        let editor_title = toolbar_editor
            .as_ref()
            .and_then(|editor| {
                editor.read(cx).document().path().and_then(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
            })
            .unwrap_or_else(|| filename.clone());
        let status = self.model.status().to_owned();
        let branch = self.model.branch().to_owned();
        let show_recent_workspaces = self.show_recent_workspaces;
        let picking_workspace = self.picking_workspace;
        let workspace_root = self.model.root().to_path_buf();
        let recent_workspaces: Vec<_> = self
            .recent_workspaces
            .iter()
            .take(12)
            .cloned()
            .enumerate()
            .collect();
        let saved_projects: Vec<_> = self.saved_projects.iter().cloned().enumerate().collect();
        let project_tree_rows: Vec<_> = self
            .project_tree_groups
            .iter()
            .take(45)
            .cloned()
            .enumerate()
            .map(|(index, group)| {
                let collapsed = self.collapsed_project_roots.contains(&group.root);
                (index, group, collapsed)
            })
            .collect();
        let project_tree_loading = self.project_tree_loading;
        let worktree_rows: Vec<_> = self
            .worktrees
            .iter()
            .take(24)
            .cloned()
            .enumerate()
            .collect();
        let worktree_error = self.worktree_error.clone();
        let worktree_loading = self.worktree_loading;
        let worktree_busy = self.worktree_busy;
        let worktree_create_input = self.worktree_create_input.clone();
        let worktree_delete_pending = self.worktree_delete_pending.clone();
        let runtime_status = self.runtime_status.clone();
        let runtime_loading = self.runtime_loading;
        let mcp_ui_status = self.mcp_ui_status.clone();
        let mcp_ui_window_id = self.mcp_ui_window_id.clone();
        let agent_profile_rows: Vec<_> = self
            .agent_profiles
            .iter()
            .take(28)
            .cloned()
            .enumerate()
            .collect();
        let agent_profile_selected = self.agent_selected_profile.clone();
        let agent_profiles_loading = self.agent_profiles_loading;
        let agent_launch_busy = self.agent_launch_busy;
        let agent_pending = self.agent_pending_tab.is_some();
        let agent_prompt_input = self.agent_prompt_input.clone();
        let agent_status = self.agent_status.clone();
        let terminal_list_loading = self.terminal_list_loading;
        let terminal_sessions: Vec<_> = self
            .terminals
            .sessions
            .iter()
            .map(|session| {
                (
                    session.handle.clone(),
                    session.label(),
                    session.workspace_id.clone(),
                )
            })
            .collect();
        let terminal_create_busy = self.terminal_create_flow.is_active();
        let terminal_create_timed_out = self.terminal_create_flow.is_timed_out();
        let terminal_creating = self.terminal_create_flow.is_creating();
        // The primary pane renders its own Host identity, even when the
        // second pane currently owns keyboard focus. The global TerminalDock
        // selection alone cannot identify two simultaneous pane contents.
        let primary_terminal_index = if mixed_has_secondary {
            self.mixed_panes
                .active_in(PaneSlot::Primary)
                .and_then(|identity| match identity {
                    WorkbenchTabIdentity::Host(target) => self.terminals.index_for_context(target),
                    WorkbenchTabIdentity::Editor { .. } => None,
                })
        } else {
            self.terminals.active_tab
        };
        let primary_terminal =
            primary_terminal_index.and_then(|index| self.terminals.tabs.get(index));
        let terminal_output = primary_terminal
            .map(|tab| match self.terminals.view_mode {
                TerminalViewMode::Screen => tab.screen.contents(),
                TerminalViewMode::Transcript => tab.output.clone(),
            })
            .unwrap_or_default();
        let terminal_secondary_output = self
            .terminal_panes
            .secondary_index(&self.terminals)
            .and_then(|index| self.terminals.tabs.get(index))
            .map(|tab| {
                (
                    tab.title.clone(),
                    match self.terminals.view_mode {
                        TerminalViewMode::Screen => tab.screen.contents(),
                        TerminalViewMode::Transcript => tab.output.clone(),
                    },
                )
            });
        let terminal_split_exists = terminal_secondary_output.is_some();
        let terminal_split_horizontal =
            self.terminal_panes.direction == TerminalSplitDirection::Horizontal;
        let terminal_status = self.terminals.status.clone();
        let has_terminal_preview = primary_terminal.is_some();
        let terminal_input = self.terminal_input.clone();
        let secondary_terminal_input = self.secondary_terminal_input.clone();
        let terminal_keyboard_focus = self.terminal_keyboard_focus.clone();
        let secondary_terminal_keyboard_focus = self.secondary_terminal_keyboard_focus.clone();
        let terminal_is_vt = self.terminals.view_mode == TerminalViewMode::Screen;
        let terminal_screen_rows = if terminal_is_vt {
            primary_terminal
                .map(|tab| tab.screen.styled_rows())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let current_primary_host =
            primary_terminal_index.and_then(|index| self.terminals.context_target_at(index));
        let terminal_selected_rows = current_primary_host
            .as_ref()
            .zip(primary_terminal)
            .and_then(|(target, tab)| {
                self.terminal_line_selection
                    .as_ref()
                    .filter(|selection| selection.is_current(target, tab.next_cursor))
            })
            .map(|selection| {
                (0..terminal_screen_rows.len())
                    .map(|row| selection.includes_row(row))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Window geometry is only *observed* by render. The existing 1.5s
        // lifecycle poll coalesces stable changes before the Host RPC.
        // No resizing from inside GPUI's paint/layout phase.
        let viewport = window.viewport_size();
        let content_width =
            (f32::from(viewport.width) - PROJECTS_SIDEBAR_WIDTH - RIGHT_SIDEBAR_WIDTH - 14.0)
                .max(0.0);
        let content_height = (f32::from(viewport.height)
            - TOP_BAR_HEIGHT
            - STATUS_BAR_HEIGHT
            - TAB_STRIP_HEIGHT
            - 14.0)
            .max(0.0);
        let split_width = mixed_has_secondary && mixed_split_horizontal;
        let split_height = mixed_has_secondary && !mixed_split_horizontal;
        let primary_width = content_width / if split_width { 2.0 } else { 1.0 };
        let primary_height = content_height / if split_height { 2.0 } else { 1.0 };
        // Keep Windows Zhuyin candidate popups beside the visible VT cursor.
        // The 1px GPUI TextInput still captures composition; only its local
        // visual anchor moves, without altering Host PTY geometry.
        let primary_ime_anchor = primary_terminal
            .and_then(|tab| {
                terminal_ime_anchor_px(
                    tab.screen.cursor(),
                    primary_width,
                    primary_height,
                    terminal_font_size,
                )
            })
            .unwrap_or((12.0, 8.0));
        let secondary_ime_anchor = mixed_secondary_host
            .as_ref()
            .and_then(|target| self.terminals.index_for_context(target))
            .and_then(|index| self.terminals.tabs.get(index))
            .and_then(|tab| {
                terminal_ime_anchor_px(
                    tab.screen.cursor(),
                    primary_width,
                    (primary_height - TAB_STRIP_HEIGHT).max(24.0),
                    terminal_font_size,
                )
            })
            .unwrap_or((12.0, 8.0));
        let mut proposed_viewports = Vec::new();
        if self.settings_section.is_none() && !center_is_editor {
            if let Some((target, tab)) = current_primary_host.as_ref().zip(primary_terminal) {
                if tab.running && self.terminals.context_target_is_live(target) {
                    let width = primary_width
                        / if terminal_split_exists && terminal_split_horizontal {
                            2.0
                        } else {
                            1.0
                        };
                    let height = primary_height
                        / if terminal_split_exists && !terminal_split_horizontal {
                            2.0
                        } else {
                            1.0
                        };
                    if let Some((cols, rows)) =
                        estimate_terminal_viewport(width, height, terminal_font_size)
                    {
                        proposed_viewports.push(TerminalViewportProposal {
                            target: target.clone(),
                            cols,
                            rows,
                        });
                    }
                }
            }
            if terminal_split_exists {
                if let Some((index, tab)) = self
                    .terminal_panes
                    .secondary_index(&self.terminals)
                    .and_then(|index| self.terminals.tabs.get(index).map(|tab| (index, tab)))
                {
                    if let Some(target) = self.terminals.context_target_at(index).filter(|target| {
                        tab.running && self.terminals.context_target_is_live(target)
                    }) {
                        let width =
                            primary_width / if terminal_split_horizontal { 2.0 } else { 1.0 };
                        let height = primary_height
                            / if terminal_split_horizontal { 1.0 } else { 2.0 }
                            - 28.0;
                        if let Some((cols, rows)) =
                            estimate_terminal_viewport(width, height, terminal_font_size)
                        {
                            proposed_viewports.push(TerminalViewportProposal {
                                target,
                                cols,
                                rows,
                            });
                        }
                    }
                }
            }
        }
        if self.settings_section.is_none() {
            if let Some(target) = mixed_secondary_host
                .as_ref()
                .filter(|target| self.terminals.context_target_is_live(target))
            {
                if let Some(tab) = self
                    .terminals
                    .index_for_context(target)
                    .and_then(|index| self.terminals.tabs.get(index))
                {
                    if tab.running {
                        let width = content_width / if split_width { 2.0 } else { 1.0 };
                        let height = content_height / if split_height { 2.0 } else { 1.0 }
                            - TAB_STRIP_HEIGHT;
                        if let Some((cols, rows)) =
                            estimate_terminal_viewport(width, height, terminal_font_size)
                        {
                            proposed_viewports.push(TerminalViewportProposal {
                                target: target.clone(),
                                cols,
                                rows,
                            });
                        }
                    }
                }
            }
        }
        self.terminal_auto_resize
            .observe(proposed_viewports, Instant::now());
        let index_loading = self.file_index_loading;
        let content_query = self.content_query.clone();
        let content_loading = self.content_loading;
        let content_results_total = self.content_matches.len();
        let content_results: Vec<_> = self
            .content_matches
            .iter()
            .take(80)
            .cloned()
            .enumerate()
            .collect();
        let content_status = if content_loading {
            "Searching workspace text…".to_string()
        } else if let Some(error) = &self.content_error {
            error.clone()
        } else if self.content_searched_query.is_empty() {
            "Enter text and click Search".to_string()
        } else {
            format!(
                "{} match(es){}{}",
                content_results_total,
                if self.content_truncated {
                    " · partial results"
                } else {
                    ""
                },
                if content_results_total > 80 {
                    " · showing first 80"
                } else {
                    ""
                }
            )
        };
        let git_filter_input = self.git_filter_input.clone();
        let git_filter = self.git_filter_input.read(cx).query().to_owned();
        let git_changes = self
            .git_snapshot
            .as_ref()
            .map(|snapshot| snapshot.visible_rows(&git_filter))
            .unwrap_or_default();
        let git_visible_rows = Arc::new(git_changes.clone());
        let git_selected = self.git_selected_changes.clone();
        let mut git_stage_paths = Vec::new();
        let mut git_unstage_paths = Vec::new();
        for row in &git_changes {
            if !git_selected.contains(row) {
                continue;
            }
            if row.area.can_stage() {
                git_stage_paths.push(row.relative.clone());
            }
            if row.area.can_unstage() {
                git_unstage_paths.push(row.relative.clone());
            }
        }
        git_stage_paths.sort();
        git_stage_paths.dedup();
        git_unstage_paths.sort();
        git_unstage_paths.dedup();
        let git_selected_count = git_changes
            .iter()
            .filter(|row| git_selected.contains(*row))
            .count();
        let git_action_busy = self.git_action_busy;
        let git_sensitive_armed = self.git_sensitive_armed;
        let git_action_status = self.git_action_status.clone();
        let git_commit_input = self.git_commit_input.clone();
        let git_branch_input = self.git_branch_input.clone();
        let git_graph_open = self.git_history_open;
        let git_graph_all_branches = self.git_history_options.all_branches;
        let git_graph_favorites_only = self.git_history_options.favorites_only;
        let git_graph_selected_ref = self.git_history_options.selected_ref.clone();
        let git_graph_file = self.git_history_options.file_path.clone();
        let git_graph_scope = self.git_history_options.git_diff_root.clone();
        let git_history_status = if self.git_history_loading {
            "Loading commit graph…".to_owned()
        } else if let Some(history) = &self.git_history {
            format!(
                "{} branch(es), {} commit(s){}",
                history.branches.len(),
                history.commits.len(),
                if history.truncated { " (limited)" } else { "" }
            )
        } else {
            self.git_history_error
                .clone()
                .unwrap_or_else(|| "Open Git Graph to load history".into())
        };
        let git_branch_rows = self
            .git_history
            .as_ref()
            .map(|history| history.branches.clone())
            .unwrap_or_default();
        let git_commit_rows = self
            .git_history
            .as_ref()
            .map(|history| history.commits.clone())
            .unwrap_or_default();
        let git_graph_topology = layout_git_graph(
            &git_commit_rows
                .iter()
                .map(|commit| GraphCommit {
                    oid: commit.oid.clone(),
                    parents: commit.parent_ids.clone(),
                })
                .collect::<Vec<_>>(),
        );
        let git_graph_topology_note = git_graph_topology.as_ref().err().map(|error| {
            format!(
                "Topology unavailable: {} · showing commit list",
                error.message()
            )
        });
        let git_graph_topology_rows = git_graph_topology.ok().map(|layout| layout.rows);
        let git_merge_armed = self.git_merge_armed.is_some();
        let git_conflict_armed = self.git_conflict_confirm_armed;
        let git_conflict_count = self
            .git_conflict_preview
            .as_ref()
            .map(|preview| preview.conflicts.len())
            .unwrap_or(0);
        let git_conflict_choices_count = self.git_conflict_choices.len();
        let git_conflict_rows = self
            .git_conflict_preview
            .as_ref()
            .map(|preview| {
                preview
                    .conflicts
                    .iter()
                    .map(|file| {
                        (
                            file.path.clone(),
                            self.git_conflict_choices.get(&file.path).copied(),
                            self.git_conflict_selected.as_deref() == Some(file.path.as_str()),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let git_conflict_detail = self.git_conflict_preview
            .as_ref()
            .and_then(|preview| preview.conflicts.iter().find(|file| {
                self.git_conflict_selected.as_deref() == Some(file.path.as_str())
            }))
            .map(|file| {
                let excerpt = |full: &str| {
                    let first = full.chars().take(1800).collect::<String>();
                    if full.chars().count() > 1800 {
                        format!("{first}\n[Preview limited to 1,800 characters. Inspect the full file before choosing.]")
                    } else {
                        first
                    }
                };
                format!(
                    "BASE:\n{}\n\nOURS (current branch):\n{}\n\nTHEIRS (source branch):\n{}",
                    file.ancestor_preview.as_deref().map(excerpt).unwrap_or_else(|| "(new in both branches)".to_string()),
                    excerpt(&file.ours_preview),
                    excerpt(&file.theirs_preview),
                )
            });
        let git_summary = if self.git_loading {
            "Reading Git status…".to_string()
        } else if let Some(snapshot) = &self.git_snapshot {
            format!(
                "{} · {} change(s){}",
                snapshot.branch,
                snapshot.changed_files.len(),
                if snapshot.truncated { " (limited)" } else { "" }
            )
        } else {
            self.git_error
                .clone()
                .unwrap_or_else(|| "Git status not loaded".into())
        };
        let git_diff = self.git_diff.clone().unwrap_or_else(|| {
            if self.git_diff_loading {
                "Reading read-only diff…".to_string()
            } else {
                "Select a changed file to preview its patch".to_string()
            }
        });

        div()
            .size_full()
            .relative()
            .on_action(cx.listener(Self::toggle_file_search_action))
            .on_action(cx.listener(Self::toggle_content_search_action))
            .on_action(cx.listener(Self::explorer_copy_action))
            .on_action(cx.listener(Self::explorer_cut_action))
            .on_action(cx.listener(Self::explorer_paste_action))
            .on_action(cx.listener(Self::explorer_select_all_action))
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .font_family("Inter")
            .text_color(rgb(TEXT))
            .child(
                div()
                    .h(px(TOP_BAR_HEIGHT))
                    .px_4()
                    .bg(rgb(TOOLBAR))
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().font_weight(gpui::FontWeight::BOLD).child("Alera"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("NATIVE PREVIEW"),
                    )
                    .child(div().flex_1())
                    .child(div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(icon(UiIcon::Branch, 14.0, MUTED))
                        .child(branch.clone()))
                    .child(icon_button("native-settings-open", UiIcon::Settings, "Settings", false)
                        .on_click(cx.listener(|view, _, _, cx| {
                            if let Err(code) = view.show_native_settings_section("application", cx) {
                                view.model.set_status(format!("Native settings unavailable: {code}"));
                                cx.notify();
                            }
                        })))
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row_reverse()
                    .child(
                        div()
                            .id("native-right-sidebar-scroll")
                            .w(px(RIGHT_SIDEBAR_WIDTH))
                            .flex()
                            .flex_col()
                            .bg(rgb(PANEL))
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .id("right-sidebar-view-switch")
                                    .h(px(38.0))
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        TabBar::new("right-sidebar-view-tabs")
                                            .segmented()
                                            .selected_index(
                                                if self.right_sidebar_view == RightSidebarView::Explorer {
                                                    0
                                                } else {
                                                    1
                                                },
                                            )
                                            .child(Tab::new().label("Explorer").prefix(icon(UiIcon::Folder, 15.0, TEXT)))
                                            .child(Tab::new().label("Git").prefix(icon(UiIcon::Branch, 15.0, TEXT)))
                                            .on_click(cx.listener(|view, index, _, cx| {
                                                view.right_sidebar_view = if *index == 0 {
                                                    RightSidebarView::Explorer
                                                } else {
                                                    view.explorer_context_menu = None;
                                                    RightSidebarView::Git
                                                };
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("right-explorer-panel")
                                    .flex_1()
                                    .min_h_0()
                                    .track_focus(&self.explorer_keyboard_focus)
                                    .key_context("AleraGpuiExplorer")
                                    .relative()
                                    .flex()
                                    .flex_col()
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        if this.explorer_context_menu.take().is_some() { cx.notify(); }
                                    }))
                                    .when(self.right_sidebar_view != RightSidebarView::Explorer, |v| v.hidden())
                            .child(
                                div()
                                    .h(px(SECTION_HEADER_HEIGHT))
                                    .px_3()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(workspace_root.file_name()
                                            .map(|name| name.to_string_lossy().to_string())
                                            .unwrap_or_else(|| "Explorer".to_string())))
                                    .child(
                                        div()
                                            .flex()
                                            .gap_1()
                                            .child(
                                                icon_button(("explorer-file-action", 0usize), UiIcon::NewFile, "New file", false)
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.begin_explorer_action(ExplorerAction::NewFile, window, cx);
                                                    })),
                                            )
                                            .child(
                                                icon_button(("explorer-file-action", 1usize), UiIcon::NewFolder, "New folder", false)
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.begin_explorer_action(ExplorerAction::NewFolder, window, cx);
                                                    })),
                                            )
                                            .child(
                                                icon_button("explorer-save-all", UiIcon::Save, "Save all editor files", false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.save_all_editors(cx);
                                                    })),
                                            )
                                            .child({
                                                let owner = cx.entity().downgrade();
                                                let ignored = self.explorer_visibility.show_ignored;
                                                let hidden = self.explorer_visibility.show_hidden;
                                                Button::new("explorer-visibility-options")
                                                    .icon(IconName::Ellipsis)
                                                    .tooltip("Show/hide Explorer items")
                                                    .ghost()
                                                    .dropdown_menu(move |menu, _, _| {
                                                        let ignored_owner = owner.clone();
                                                        let mut menu = menu.item(
                                                            PopupMenuItem::new(if ignored {
                                                                "Hide ignored files"
                                                            } else {
                                                                "Show ignored files"
                                                            })
                                                            .on_click(move |_, _, cx| {
                                                                let _ = ignored_owner.update(cx, |view, cx| {
                                                                    let mut next = view.explorer_visibility;
                                                                    next.show_ignored = !next.show_ignored;
                                                                    view.update_explorer_visibility(next, cx);
                                                                });
                                                            }),
                                                        );
                                                        if cfg!(windows) {
                                                            let hidden_owner = owner.clone();
                                                            menu = menu.item(
                                                                PopupMenuItem::new(if hidden {
                                                                    "Hide hidden items"
                                                                } else {
                                                                    "Show hidden items"
                                                                })
                                                                .on_click(move |_, _, cx| {
                                                                    let _ = hidden_owner.update(cx, |view, cx| {
                                                                        let mut next = view.explorer_visibility;
                                                                        next.show_hidden = !next.show_hidden;
                                                                        view.update_explorer_visibility(next, cx);
                                                                    });
                                                                }),
                                                            );
                                                        }
                                                        menu
                                                    })
                                            })
                                            .child(
                                                icon_button("explorer-collapse-all", UiIcon::CollapseAll, "Collapse all folders", false)
                                                    .on_click(cx.listener(|this, _, _window, cx| {
                                                        if let Some(tree) = &mut this.tree {
                                                            if let Err(error) = tree.collapse_all() {
                                                                this.model.set_status(format!("Collapse All failed: {error}"));
                                                            }
                                                        }
                                                        this.tree_mode = true;
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                icon_button("explorer-refresh", UiIcon::Refresh, "Refresh Explorer", false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.model.refresh_directory();
                                                        if let Some(tree) = &mut this.tree {
                                                            if let Err(error) = tree.refresh() {
                                                                this.model.set_status(format!("Tree refresh failed: {error}"));
                                                            }
                                                        }
                                                        this.refresh_git(cx);
                                                        cx.notify();
                                                    })),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("explorer-classic-filter")
                                    .px_2()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(explorer_filter_input),
                            )
                            .child(
                                div()
                                    .id("content-search-panel")
                                    .px_3()
                                    .py_1()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .when(content_query.is_none(), |panel| panel.hidden())
                                    .child(if let Some(input) = content_query {
                                        div()
                                            .mt_2()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .child(div().flex_1().min_w_0().child(input))
                                                .child(icon_button("content-search-run", UiIcon::Search, "Search project file contents", content_loading)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.search_project_contents(cx);
                                                    }))))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child(content_status),
                                            )
                                            .child(
                                                div()
                                                    .id("content-search-results")
                                                    .max_h(px(230.0))
                                                    .overflow_y_scroll()
                                                    .children(content_results.into_iter().map(|(i, hit)| {
                                                        let path = workspace_root.join(&hit.path);
                                                        let label = format!(
                                                            "{}:{}:{}  {}",
                                                            hit.path.display(),
                                                            hit.line,
                                                            hit.column,
                                                            hit.snippet
                                                        );
                                                        div()
                                                            .id(("content-search-result", i))
                                                            .cursor_pointer()
                                                            .py_1()
                                                            .text_xs()
                                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                                            .child(label)
                                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                                this.open_content_search_hit(
                                                                    &path, hit.line, hit.column, window, cx,
                                                                );
                                                            }))
                                                    })),
                                            )
                                    } else {
                                        div().h(px(0.0))
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .id("explorer-scroll")
                                    .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                                        this.import_explorer_drop(paths.paths().to_vec(), None, cx);
                                    }))
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        if this.explorer_context_menu.take().is_some() { cx.notify(); }
                                    }))
                                    .overflow_y_scroll()
                                    .children(if explorer_filter_active {
                                        explorer_filter_results.into_iter().enumerate().map(|(i, hit)| {
                                            let path = hit.path;
                                            let clicked_path = path.clone();
                                            let context_path = path.clone();
                                            let visible_for_click = explorer_visible_paths.clone();
                                            let relative = path.strip_prefix(&workspace_root)
                                                .unwrap_or(&path).to_path_buf();
                                            let label = path.file_name()
                                                .map(|name| name.to_string_lossy().to_string())
                                                .unwrap_or_default();
                                            let drag_payload = ExplorerDragBatch::for_row(
                                                &workspace_root,
                                                explorer_drag_generation,
                                                &path,
                                                explorer_drag_selection.as_ref(),
                                            );
                                            let drag_caption = if drag_payload.len() > 1 {
                                                format!("Move {} Explorer items", drag_payload.len())
                                            } else {
                                                format!("Move {label}")
                                            };
                                            let drop_target = path.clone();
                                            let external_drop_target = path.clone();
                                            let selected = explorer_multi_selection.contains(&path);
                                            let row_icon = explorer_file_icon(&label, hit.is_directory, false);
                                            let status_badge = explorer_git_badges.get(&path);
                                            div()
                                                .id(("explorer-filter-result", i))
                                                .px_2()
                                                .py_1()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(icon(row_icon, 15.0, if selected { TEXT } else { MUTED }))
                                                .child(div().min_w_0().flex_1().flex().flex_col()
                                                    .child(div().text_sm().child(label))
                                                    .child(div().text_xs().text_color(rgb(MUTED))
                                                        .child(relative.display().to_string())))
                                                .child(explorer_status_badge(status_badge))
                                                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                                    this.explorer_context_menu = None;
                                                    let modifiers = event.modifiers();
                                                    if this.select_explorer_row(
                                                        &clicked_path,
                                                        visible_for_click.as_ref(),
                                                        modifiers.control || modifiers.platform,
                                                        modifiers.shift,
                                                        window,
                                                        cx,
                                                    ) {
                                                        return;
                                                    }
                                                    if hit.is_directory {
                                                        this.explorer_selection = Some(clicked_path.clone());
                                                        this.explorer_delete_confirmed = false;
                                                        this.tree_mode = true;
                                                        if let Some(tree) = &mut this.tree {
                                                            if let Err(error) = tree.reveal_directory(&relative) {
                                                                this.model.set_status(format!("Cannot reveal folder: {error}"));
                                                            }
                                                        }
                                                        this.explorer_filter_input.update(cx, |input, cx| input.clear(cx));
                                                    } else {
                                                        this.click_explorer_file(&clicked_path, event.click_count() >= 2, window, cx);
                                                    }
                                                    cx.notify();
                                                }))
                                                .on_drag(drag_payload, move |_, _, _, cx| {
                                                    cx.new(|_| TerminalTabDragPreview { label: drag_caption.clone() })
                                                })
                                                .when(hit.is_directory, |row| {
                                                    row.on_drop(cx.listener(move |this, drag: &ExplorerDragBatch, _, cx| {
                                                        this.drop_explorer_batch(drag, &drop_target, cx);
                                                    }))
                                                })
                                                .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                                                    cx.stop_propagation();
                                                    this.import_explorer_drop(paths.paths().to_vec(), Some(external_drop_target.clone()), cx);
                                                }))
                                                .context_menu(Self::kit_explorer_context_menu(
                                                    cx.entity().downgrade(),
                                                    Some(context_path),
                                                ))
                                        }).collect::<Vec<_>>()
                                    } else if tree_mode {
                                        tree_rows.into_iter().enumerate().map(|(i, item)| {
                                            let relative = item.relative_path.clone();
                                            let row_path = workspace_root.join(&relative);
                                            let context_path = row_path.clone();
                                            let drag_payload = ExplorerDragBatch::for_row(
                                                &workspace_root,
                                                explorer_drag_generation,
                                                &row_path,
                                                explorer_drag_selection.as_ref(),
                                            );
                                            let drag_caption = if drag_payload.len() > 1 {
                                                format!("Move {} Explorer items", drag_payload.len())
                                            } else {
                                                format!("Move {}", item.name)
                                            };
                                            let drop_target = row_path.clone();
                                            let external_drop_target = row_path.clone();
                                            let visible_for_click = explorer_visible_paths.clone();
                                            let selected = explorer_multi_selection.contains(&row_path);
                                            let is_directory = item.is_directory;
                                            let status_badge = explorer_git_badges.get(&row_path);
                                            let row_icon = explorer_file_icon(&item.name, is_directory, item.expanded);
                                            let row_label = item.name;
                                            div()
                                                .id(("explorer-tree-row", i))
                                                .pl(px(8.0 + item.depth as f32 * 16.0))
                                                .pr_2()
                                                .h(px(32.0))
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .cursor_pointer()
                                                .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(if is_directory {
                                                    let arrow_path = relative.clone();
                                                    div()
                                                        .id(("explorer-folder-arrow", i))
                                                        .w(px(20.0))
                                                        .h(px(24.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .child(icon(if item.expanded { UiIcon::ChevronDown } else { UiIcon::ChevronRight }, 14.0, MUTED))
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            if let Some(tree) = &mut this.tree {
                                                                if let Err(error) = tree.toggle(&arrow_path) {
                                                                    this.model.set_status(format!("Cannot expand folder: {error}"));
                                                                }
                                                            }
                                                            cx.stop_propagation();
                                                            cx.notify();
                                                        }))
                                                        .into_any_element()
                                                } else {
                                                    div().w(px(20.0)).into_any_element()
                                                })
                                                .child(icon(row_icon, 15.0, if selected { TEXT } else { MUTED }))
                                                .child(div().flex_1().min_w_0().overflow_hidden().text_sm().child(row_label))
                                                .child(explorer_status_badge(status_badge))
                                                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                                    this.explorer_context_menu = None;
                                                    let clicked = this.model.root().join(&relative);
                                                    let modifiers = event.modifiers();
                                                    if this.select_explorer_row(
                                                        &clicked,
                                                        visible_for_click.as_ref(),
                                                        modifiers.control || modifiers.platform,
                                                        modifiers.shift,
                                                        window,
                                                        cx,
                                                    ) {
                                                        return;
                                                    }
                                                    if is_directory {
                                                        this.explorer_selection = Some(clicked);
                                                        this.explorer_delete_confirmed = false;
                                                        if let Some(tree) = &mut this.tree {
                                                            if let Err(error) = tree.toggle(&relative) {
                                                                this.model.set_status(format!("Cannot expand folder: {error}"));
                                                            }
                                                        }
                                                    } else {
                                                        let path = this.model.root().join(&relative);
                                                        this.click_explorer_file(&path, event.click_count() >= 2, window, cx);
                                                    }
                                                    cx.notify();
                                                }))
                                                .on_drag(drag_payload, move |_, _, _, cx| {
                                                    cx.new(|_| TerminalTabDragPreview { label: drag_caption.clone() })
                                                })
                                                .when(is_directory, |row| {
                                                    row.on_drop(cx.listener(move |this, drag: &ExplorerDragBatch, _, cx| {
                                                        this.drop_explorer_batch(drag, &drop_target, cx);
                                                    }))
                                                })
                                                .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                                                    cx.stop_propagation();
                                                    this.import_explorer_drop(paths.paths().to_vec(), Some(external_drop_target.clone()), cx);
                                                }))
                                                .context_menu(Self::kit_explorer_context_menu(
                                                    cx.entity().downgrade(),
                                                    Some(context_path),
                                                ))
                                        }).collect::<Vec<_>>()
                                    } else {
                                        entries.into_iter().enumerate().map(|(i, item)| {
                                            let is_directory = item.is_directory;
                                            let file_path = item.path.clone();
                                            let context_path = file_path.clone();
                                            let drag_payload = ExplorerDragBatch::for_row(
                                                &workspace_root,
                                                explorer_drag_generation,
                                                &file_path,
                                                explorer_drag_selection.as_ref(),
                                            );
                                            let drag_caption = if drag_payload.len() > 1 {
                                                format!("Move {} Explorer items", drag_payload.len())
                                            } else {
                                                format!("Move {}", item.name)
                                            };
                                            let drop_target = file_path.clone();
                                            let external_drop_target = file_path.clone();
                                            let visible_for_click = explorer_visible_paths.clone();
                                            let selected = explorer_multi_selection.contains(&file_path);
                                            let status_badge = explorer_git_badges.get(&file_path);
                                            let row_icon = explorer_file_icon(&item.name, is_directory, false);
                                            div()
                                                .id(i)
                                                .pl_2()
                                                .pr_2()
                                                .h(px(32.0))
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(icon(row_icon, 15.0, if selected { TEXT } else { MUTED }))
                                                .child(div().flex_1().min_w_0().overflow_hidden().text_sm().child(item.name))
                                                .child(explorer_status_badge(status_badge))
                                                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                                    this.explorer_context_menu = None;
                                                    let modifiers = event.modifiers();
                                                    if this.select_explorer_row(
                                                        &file_path,
                                                        visible_for_click.as_ref(),
                                                        modifiers.control || modifiers.platform,
                                                        modifiers.shift,
                                                        window,
                                                        cx,
                                                    ) {
                                                        return;
                                                    }
                                                    if is_directory {
                                                        this.explorer_selection = Some(file_path.clone());
                                                        this.explorer_delete_confirmed = false;
                                                        this.model.open_entry(i);
                                                        this.persist_session();
                                                    } else {
                                                        this.click_explorer_file(&file_path, event.click_count() >= 2, window, cx);
                                                    }
                                                    cx.notify();
                                                }))
                                                .on_drag(drag_payload, move |_, _, _, cx| {
                                                    cx.new(|_| TerminalTabDragPreview { label: drag_caption.clone() })
                                                })
                                                .when(is_directory, |row| {
                                                    row.on_drop(cx.listener(move |this, drag: &ExplorerDragBatch, _, cx| {
                                                        this.drop_explorer_batch(drag, &drop_target, cx);
                                                    }))
                                                })
                                                .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                                                    cx.stop_propagation();
                                                    this.import_explorer_drop(paths.paths().to_vec(), Some(external_drop_target.clone()), cx);
                                                }))
                                                .context_menu(Self::kit_explorer_context_menu(
                                                    cx.entity().downgrade(),
                                                    Some(context_path),
                                                ))
                                        }).collect::<Vec<_>>()
                                    })
                                    .when(explorer_filter_active && explorer_filter_empty,
                                        |view| view.child(div().px_3().py_3().text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(if index_loading { "Indexing workspace files…" } else { "No matching files or folders" })))
                                    .when(tree_mode && tree_truncated, |view| {
                                        view.child(div().text_color(rgb(MUTED)).child("Tree limited to 4,000 visible items"))
                                    })
                                    .context_menu(Self::kit_explorer_context_menu(
                                        cx.entity().downgrade(),
                                        None,
                                    )),
                            )
                            )
                            .child(
                                div()
                                    .id("right-git-panel")
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .overflow_y_scroll()
                                    .when(self.right_sidebar_view != RightSidebarView::Git, |v| v.hidden())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .h(px(36.0))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("SOURCE CONTROL"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(icon_button("git-graph-toggle", UiIcon::Graph, "Toggle Git Graph", git_graph_open)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_git_history(cx);
                                                })))
                                            .child(icon_button("git-refresh", UiIcon::Refresh, "Refresh Git status", false)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.refresh_git(cx);
                                                    if this.git_history_open { this.refresh_git_history(cx); }
                                                }))),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(git_summary),
                            )
                            .child(if git_graph_open {
                                div()
                                    .id("git-graph-panel")
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().text_xs().text_color(rgb(MUTED)).child(git_history_status))
                                    .child(if git_graph_file.is_some() || git_graph_scope.is_some() {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_3()
                                            .child(div()
                                                .text_xs()
                                                .text_color(rgb(TEXT))
                                                .child(format!(
                                                    "History scope: {} · File: {}",
                                                    git_graph_scope.as_deref().unwrap_or("Workspace"),
                                                    git_graph_file.as_deref().unwrap_or("All files"),
                                                )))
                                            .child(div()
                                                .id("git-graph-clear-path-scope")
                                                .cursor_pointer()
                                                .text_xs()
                                                .text_color(rgb(TEXT))
                                                .child("Clear path filter")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.git_history_options.file_path = None;
                                                    this.git_history_options.git_diff_root = None;
                                                    this.refresh_git_history(cx);
                                                })))
                                            .into_any_element()
                                    } else {
                                        div().h(px(0.0)).into_any_element()
                                    })
                                    .child(div()
                                        .flex()
                                        .items_center()
                                        .gap_3()
                                        .child(div()
                                            .id("git-graph-toggle-remotes")
                                            .text_xs()
                                            .cursor_pointer()
                                            .text_color(rgb(TEXT))
                                            .child(if git_graph_all_branches { "Remote branches: On" } else { "Remote branches: Off" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.git_history_options.all_branches = !this.git_history_options.all_branches;
                                                if !this.git_history_options.all_branches {
                                                    this.git_history_options.favorites_only = false;
                                                }
                                                this.refresh_git_history(cx);
                                            })))
                                        .child(div()
                                            .id("git-graph-toggle-favorites")
                                            .text_xs()
                                            .cursor_pointer()
                                            .text_color(rgb(TEXT))
                                            .child(if git_graph_favorites_only { "★ Favorites only" } else { "☆ All branches" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.git_history_options.favorites_only = !this.git_history_options.favorites_only;
                                                if this.git_history_options.favorites_only {
                                                    this.git_history_options.all_branches = true;
                                                }
                                                this.refresh_git_history(cx);
                                            })))
                                        .child(div()
                                            .id("git-graph-clear-ref")
                                            .text_xs()
                                            .cursor_pointer()
                                            .text_color(rgb(TEXT))
                                            .child(if git_graph_selected_ref.is_some() { "Clear selected ref" } else { "All commits" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_git_history_ref(None, cx);
                                            }))))
                                    .child(div().text_xs().text_color(rgb(TEXT)).child("Branches · click name to select history, star to favorite"))
                                    .child(div()
                                        .id("git-graph-branches")
                                        .max_h(px(85.0))
                                        .overflow_y_scroll()
                                        .children(git_branch_rows.into_iter().enumerate().map(|(index, row)| {
                                            let name = row.name;
                                            let remote = row.remote;
                                            let favorite = row.favorite;
                                            let reference = if remote { format!("refs/remotes/{name}") } else { name.clone() };
                                            let selected = git_graph_selected_ref.as_deref() == Some(reference.as_str());
                                            let label = format!("{} {}{} · {}",
                                                if row.current { "●" } else { "○" },
                                                if remote { "☁ " } else { "" }, name, row.tip);
                                            let favorite_name = reference.clone();
                                            let context_branch = name.clone();
                                            let has_worktree = !remote && self.worktrees.iter()
                                                .any(|entry| entry.branch.as_deref() == Some(name.as_str()));
                                            div()
                                                .id(("git-graph-branch", index))
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(div()
                                                    .id(("git-graph-favorite", index))
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(rgb(TEXT))
                                                    .child(if favorite { "★" } else { "☆" })
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.toggle_git_history_favorite(favorite_name.clone(), cx);
                                                    })))
                                                .child(div()
                                                    .id(("git-graph-branch-select", index))
                                                    .flex_1()
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(rgb(if row.current || selected { TEXT } else { MUTED }))
                                                    .hover(|style| style.bg(rgb(ACTIVE)))
                                                    .child(label)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.git_merge_armed = None;
                                                        this.set_git_history_ref(Some(reference.clone()), cx);
                                                        if !remote {
                                                            this.git_branch_input.update(cx, |input, cx| { input.set_query(&name, cx); });
                                                            this.preview_git_history(GitReadRequest::Compare(name.clone()), cx);
                                                        }
                                                    }))
                                                    .context_menu(Self::kit_git_graph_branch_menu(
                                                        cx.entity().downgrade(), context_branch,
                                                        remote, row.current, has_worktree,
                                                    )))
                                        })))
                                    .child(div().text_xs().text_color(rgb(TEXT)).child("Commits · graph lanes represent actual parent commits"))
                                    .when_some(git_graph_topology_note, |panel, note| {
                                        panel.child(div().text_xs().text_color(rgb(MUTED)).child(note))
                                    })
                                    .child(div()
                                        .id("git-graph-commits")
                                        .max_h(px(220.0))
                                        .overflow_y_scroll()
                                        .children(git_commit_rows.into_iter().enumerate().map(|(index, row)| {
                                            let oid = row.oid;
                                            let menu_oid = oid.clone();
                                            let menu_subject = row.subject.clone();
                                            let marker = if row.parents > 1 { "◆" } else { "●" };
                                            let labels = if row.labels.is_empty() { String::new() }
                                                else { format!(" [{}]", row.labels.join(", ")) };
                                            let title = format!("{} {}{} · {}", row.short, row.subject, labels, row.author);
                                            let topology = git_graph_topology_rows.as_ref().and_then(|rows| rows.get(index));
                                            let node_line = topology.map(|item| item.node_line.clone())
                                                .unwrap_or_else(|| marker.to_string());
                                            let connector = topology.map(|item| item.connector_line.clone())
                                                .unwrap_or_default();
                                            let rail_width = topology.map(|item| item.node_line.chars().count())
                                                .unwrap_or(1) as f32 * 8.0;
                                            div()
                                                .id(("git-graph-commit", index))
                                                .flex()
                                                .flex_col()
                                                .cursor_pointer()
                                                .text_xs()
                                                .text_color(rgb(MUTED))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(div().flex().items_center().gap_2()
                                                    .child(div().font_family("JetBrains Mono")
                                                        .min_w(px(rail_width)).text_color(rgb(TEXT)).child(node_line))
                                                    .child(div().min_w_0().flex_1().child(title)))
                                                .when(!connector.trim().is_empty(), |item| item.child(
                                                    div().font_family("JetBrains Mono")
                                                        .text_color(rgb(MUTED)).child(connector)
                                                ))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.preview_git_history(GitReadRequest::Commit(oid.clone()), cx);
                                                }))
                                                .context_menu(Self::kit_git_graph_commit_menu(
                                                    cx.entity().downgrade(), menu_oid, menu_subject,
                                                ))
                                        })))
                                    .into_any_element()
                            } else {
                                div().h(px(0.0)).into_any_element()
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(git_commit_input)
                                    .child(icon_button("git-commit-submit", UiIcon::GitCommit, "Commit staged changes using this message", false)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            let message = this.git_commit_input.read(cx).query().trim().to_owned();
                                            this.run_git_action(GitUiAction::Commit(message), cx);
                                        }))),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(self.render_kit_branch_dropdown(
                                        "git-branch-dropdown",
                                        "Choose local branch".into(),
                                        cx,
                                    ))
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(div().flex_1().min_w_0().child(git_branch_input))
                                    .child(icon_button("git-switch-branch", UiIcon::Branch, "Switch to the specified local branch", false)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            let branch = this.git_branch_input.read(cx).query().trim().to_owned();
                                            this.run_git_action(GitUiAction::SwitchBranch(branch), cx);
                                        })))
                                    .child(icon_button("git-fetch-remote", UiIcon::GitFetch, "Fetch upstream branches (no working tree change)", false)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.run_git_action(GitUiAction::Fetch, cx);
                                        }))),
                            )
                            .child(div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(icon_button("git-compare-local", UiIcon::History, "Compare the selected branch with HEAD (read-only)", false)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let branch = this.git_branch_input.read(cx).query().trim().to_owned();
                                        this.git_merge_armed = None;
                                        this.preview_git_history(GitReadRequest::Compare(branch), cx);
                                    })))
                                .child(icon_button("git-merge-auto", if git_merge_armed { UiIcon::Check } else { UiIcon::GitMerge }, "Merge selected branch into HEAD (click twice to confirm)", git_merge_armed)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.git_action_busy { return; }
                                        let branch = this.git_branch_input.read(cx).query().trim().to_owned();
                                        if branch.is_empty() {
                                            this.git_action_status = "Select a source branch before merging".into();
                                        } else if this.git_merge_armed.as_deref() == Some(branch.as_str()) {
                                            this.git_merge_armed = None;
                                            this.run_git_action(GitUiAction::MergeWithoutConflicts(branch), cx);
                                        } else {
                                            this.git_sensitive_armed = None;
                                            this.git_conflict_confirm_armed = false;
                                            this.git_merge_armed = Some(branch.clone());
                                            this.git_action_status = format!("Click Confirm Merge to merge {branch} into the current branch; merge conflicts will be refused without changing files");
                                        }
                                        cx.notify();
                                    })))
                                .child(icon_button("git-pull-ff-only", if git_sensitive_armed == Some(ConfirmGitAction::PullFfOnly) { UiIcon::Check } else { UiIcon::GitPull }, "Pull current branch from upstream, fast-forward only (confirm twice)", git_sensitive_armed == Some(ConfirmGitAction::PullFfOnly))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_sensitive_git_action(ConfirmGitAction::PullFfOnly, cx);
                                    })))
                                .child(icon_button("git-push-upstream", if git_sensitive_armed == Some(ConfirmGitAction::PushUpstream) { UiIcon::Check } else { UiIcon::GitPush }, "Push current branch to upstream without force (confirm twice)", git_sensitive_armed == Some(ConfirmGitAction::PushUpstream))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_sensitive_git_action(ConfirmGitAction::PushUpstream, cx);
                                    })))
                                .child({
                                    let owner = cx.entity().downgrade();
                                    let armed = self.git_sensitive_armed;
                                    let has_conflict_review = self.git_conflict_preview.is_some();
                                    Button::new("git-more-actions")
                                        .icon(IconName::Ellipsis)
                                        .ghost()
                                        .tooltip("More Git actions: stash and conflict review")
                                        .dropdown_menu(move |menu, _, _| {
                                            let save_owner = owner.clone();
                                            let apply_owner = owner.clone();
                                            let inspect_owner = owner.clone();
                                            let mut menu = menu
                                                .item(
                                                    PopupMenuItem::new(
                                                        if armed == Some(ConfirmGitAction::StashSave) {
                                                            "Confirm stash tracked changes"
                                                        } else {
                                                            "Stash tracked changes"
                                                        }
                                                    )
                                                    .on_click(move |_, _, cx| {
                                                        let _ = save_owner.update(cx, |view, cx| {
                                                            view.confirm_sensitive_git_action(ConfirmGitAction::StashSave, cx);
                                                        });
                                                    }),
                                                )
                                                .item(
                                                    PopupMenuItem::new(
                                                        if armed == Some(ConfirmGitAction::StashApply) {
                                                            "Confirm apply latest stash"
                                                        } else {
                                                            "Apply latest stash"
                                                        }
                                                    )
                                                    .on_click(move |_, _, cx| {
                                                        let _ = apply_owner.update(cx, |view, cx| {
                                                            view.confirm_sensitive_git_action(ConfirmGitAction::StashApply, cx);
                                                        });
                                                    }),
                                                )
                                                .separator()
                                                .item(
                                                    PopupMenuItem::new("Inspect merge conflicts")
                                                        .on_click(move |_, _, cx| {
                                                            let _ = inspect_owner.update(cx, |view, cx| {
                                                                view.preview_git_merge_conflicts(cx);
                                                            });
                                                        }),
                                                );
                                            if has_conflict_review {
                                                let cancel_owner = owner.clone();
                                                menu = menu.item(
                                                    PopupMenuItem::new("Cancel conflict review")
                                                        .on_click(move |_, _, cx| {
                                                            let _ = cancel_owner.update(cx, |view, cx| {
                                                                view.git_conflict_preview = None;
                                                                view.git_conflict_choices.clear();
                                                                view.git_conflict_selected = None;
                                                                view.git_conflict_confirm_armed = false;
                                                                view.git_action_status = "Merge conflict review cancelled; no files changed".into();
                                                                cx.notify();
                                                            });
                                                        }),
                                                );
                                            }
                                            menu
                                        })
                                })
                            )
                            .child(if git_conflict_count > 0 {
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div()
                                        .text_xs()
                                        .text_color(rgb(TEXT))
                                        .child(format!(
                                            "Conflict choices: {git_conflict_choices_count}/{git_conflict_count} · WHOLE FILE, not individual hunks",
                                        )))
                                    .child(div()
                                        .id("git-conflict-files-scroll")
                                        .max_h(px(155.0))
                                        .overflow_y_scroll()
                                        .children(git_conflict_rows.into_iter().enumerate().map(|(index, (path, choice, selected))| {
                                            let path_for_select = path.clone();
                                            let path_for_ours = path.clone();
                                            let path_for_theirs = path.clone();
                                            div()
                                                .id(("git-conflict-row", index))
                                                .flex()
                                                .flex_col()
                                                .gap_1()
                                                .py_1()
                                                .child(div()
                                                    .id(("git-conflict-preview-path", index))
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(rgb(if selected { TEXT } else { MUTED }))
                                                    .child(format!("{} {}", if selected { "▸" } else { "·" }, path))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.git_conflict_selected = Some(path_for_select.clone());
                                                        this.git_conflict_confirm_armed = false;
                                                        cx.notify();
                                                    })))
                                                .child(div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_3()
                                                    .child(div()
                                                        .id(("git-conflict-pick-ours", index))
                                                        .cursor_pointer()
                                                        .text_xs()
                                                        .text_color(rgb(if choice == Some(GitConflictChoice::Ours) { TEXT } else { MUTED }))
                                                        .child(if choice == Some(GitConflictChoice::Ours) { "● Ours" } else { "○ Ours" })
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.choose_git_conflict_side(path_for_ours.clone(), GitConflictChoice::Ours, cx);
                                                        })))
                                                    .child(div()
                                                        .id(("git-conflict-pick-theirs", index))
                                                        .cursor_pointer()
                                                        .text_xs()
                                                        .text_color(rgb(if choice == Some(GitConflictChoice::Theirs) { TEXT } else { MUTED }))
                                                        .child(if choice == Some(GitConflictChoice::Theirs) { "● Theirs" } else { "○ Theirs" })
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.choose_git_conflict_side(path_for_theirs.clone(), GitConflictChoice::Theirs, cx);
                                                        })))
                                                )
                                        })))
                                    .child(div()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child("Preview excerpts only (up to 1,800 characters per side); inspect entire file before choosing."))
                                    .child(div()
                                        .id("git-conflict-text-preview")
                                        .max_h(px(132.0))
                                        .overflow_y_scroll()
                                        .text_xs()
                                        .text_color(rgb(TEXT))
                                        .child(git_conflict_detail.unwrap_or_else(|| "Select a conflicted file to review its base, ours and theirs".into())))
                                    .child(div()
                                        .id("git-conflict-resolved-merge")
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(rgb(if git_action_busy { MUTED } else { TEXT }))
                                        .child(if git_conflict_armed { "Confirm Resolved Merge" } else { "Merge selected sides" })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.confirm_git_conflict_merge(cx);
                                        })))
                                    .into_any_element()
                            } else {
                                div().h(px(0.0)).into_any_element()
                            })
                            .child(div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(if git_action_busy { "Git operation running…".to_owned() } else { git_action_status }))
                            .child(git_filter_input)
                            .when(git_selected_count > 0, |panel| panel.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_xs()
                                    .child(format!("{git_selected_count} selected"))
                                    .when(!git_stage_paths.is_empty(), |bar| bar.child(
                                        icon_button("git-stage-selected", UiIcon::GitStage, "Stage selected files", false)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.run_git_action(GitUiAction::StageMany(git_stage_paths.clone()), cx);
                                            }))
                                    ))
                                    .when(!git_unstage_paths.is_empty(), |bar| bar.child(
                                        icon_button("git-unstage-selected", UiIcon::GitUnstage, "Unstage selected files", false)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.run_git_action(GitUiAction::UnstageMany(git_unstage_paths.clone()), cx);
                                            }))
                                    ))
                                    .child(icon_button("git-clear-selected", UiIcon::Close, "Clear Git selection", false)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.git_selected_changes.clear();
                                            this.git_selection_anchor = None;
                                            cx.notify();
                                        })))
                            ))
                            .child(
                                div()
                                    .id("git-change-list")
                                    .max_h(px(135.0))
                                    .overflow_y_scroll()
                                    .children(git_changes.into_iter().enumerate().map(|(i, change)| {
                                        let selected = git_selected.contains(&change);
                                        let clicked_row = change.clone();
                                        let visible_for_click = Arc::clone(&git_visible_rows);
                                        let label = format!("{} {} {}", change.area.title(), change.label, change.relative.display());
                                        let can_stage = change.area.can_stage();
                                        let can_unstage = change.area.can_unstage();
                                        let relative = change.relative;
                                        let stage_path = relative.clone();
                                        let unstage_path = relative.clone();
                                        div()
                                            .id(("git-change-file", i))
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .py_1()
                                            .text_xs()
                                            .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                            .child(div()
                                                .id(("git-change-preview", i))
                                                .flex_1()
                                                .min_w_0()
                                                .cursor_pointer()
                                                .child(label)
                                                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                                                    let modifiers = event.modifiers();
                                                    let multi = modifiers.control || modifiers.platform;
                                                    this.select_git_change(clicked_row.clone(), visible_for_click.as_ref(), multi, modifiers.shift);
                                                    if !multi && !modifiers.shift {
                                                        this.preview_git_change(relative.clone(), cx);
                                                    }
                                                    cx.notify();
                                                })))
                                            .when(can_stage, |row| row.child(icon_button(("git-stage-file", i), UiIcon::GitStage, "Stage this file for commit", false)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.run_git_action(GitUiAction::Stage(stage_path.clone()), cx);
                                                }))))
                                            .when(can_unstage, |row| row.child(icon_button(("git-unstage-file", i), UiIcon::GitUnstage, "Unstage this file (keep working edits)", false)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.run_git_action(GitUiAction::Unstage(unstage_path.clone()), cx);
                                                }))))
                                    })),
                            )
                            .child(
                                div()
                                    .id("git-diff-scroll")
                                    .max_h(px(170.0))
                                    .overflow_y_scroll()
                                    .text_xs()
                                    .font_family("JetBrains Mono")
                                    .text_color(rgb(MUTED))
                                    .child(git_diff),
                            ),
                            )
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .id("mixed-workbench-tab-strip")
                                    // Flutter Classic uses ONE 44px mixed content
                                    // strip. Terminal/Editor are tab kinds, not
                                    // top-level navigation modes.
                                    .h(px(TAB_STRIP_HEIGHT))
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .bg(rgb(TOOLBAR))
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        div()
                                            .id("mixed-tabs-scroll")
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .overflow_x_scroll()
                                            .pl_2()
                                            .children(mixed_tabs.into_iter().enumerate().map(|(index, tab)| {
                                                let identity = tab.identity;
                                                let in_secondary = mixed_secondary_tab_ids.contains(&identity);
                                                let selected = !showing_explorer_preview
                                                    && (mixed_active.as_ref() == Some(&identity)
                                                        || mixed_secondary_active.as_ref() == Some(&identity));
                                                let icon_kind = match tab.kind {
                                                    WorkbenchTabKind::Terminal | WorkbenchTabKind::Agent => UiIcon::Terminal,
                                                    WorkbenchTabKind::Editor => UiIcon::File,
                                                };
                                                let title = tab.title.clone();
                                                let for_click = identity.clone();
                                                let for_middle = identity.clone();
                                                let for_right = identity.clone();
                                                let for_close = identity.clone();
                                                let for_drop = identity.clone();
                                                let drag_identity = identity.clone();
                                                let drag_caption = title.clone();
                                                div()
                                                    .id(("mixed-tab", index))
                                                    .h(px(32.0))
                                                    .mr_2()
                                                    .px_1()
                                                    .flex()
                                                    .items_center()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(rgb(if selected { BORDER_ACTIVE } else { BORDER }))
                                                    .bg(rgb(if selected { ELEVATED } else { TOOLBAR }))
                                                    .child(
                                                        div()
                                                            .id(("mixed-tab-focus", index))
                                                            .min_w_0()
                                                            .max_w(px(210.0))
                                                            .flex()
                                                            .items_center()
                                                            .gap_1()
                                                            .px_1()
                                                            .cursor_pointer()
                                                            .text_color(rgb(if selected { TEXT } else { MUTED }))
                                                            .child(icon(icon_kind, 12.0, if selected { TEXT } else { MUTED }))
                                                            .child(
                                                                div()
                                                                    .min_w_0()
                                                                    .max_w(px(if tab.kind == WorkbenchTabKind::Terminal { 92.0 } else { 180.0 }))
                                                                    .truncate()
                                                                    .text_sm()
                                                                    .child(title),
                                                            )
                                                            .when(tab.dirty, |entry| entry.child(div().text_xs().child("●")))
                                                            .when(!tab.running, |entry| entry.child(div().text_xs().text_color(rgb(MUTED)).child("○")))
                                                            .when(in_secondary, |entry| entry.child(div().text_xs().text_color(rgb(MUTED)).child("②")))
                                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                                this.focus_mixed_tab(&for_click, window, cx);
                                                            }))
                                                            .on_mouse_up(gpui::MouseButton::Middle, cx.listener(move |this, _, _, cx| {
                                                                this.close_mixed_tab(&for_middle, cx);
                                                            }))
                                                            .on_drag(MixedTabDrag {
                                                                identity: drag_identity,
                                                                workspace_generation: mixed_drag_generation,
                                                            }, move |_, _, _, cx| {
                                                                cx.new(|_| TerminalTabDragPreview { label: drag_caption.clone() })
                                                            })
                                                            .on_drop(cx.listener(move |this, drag: &MixedTabDrag, _, cx| {
                                                                this.reorder_mixed_tab(drag, &for_drop, cx);
                                                            }))
                                                            .context_menu(Self::kit_mixed_tab_context_menu(
                                                                cx.entity().downgrade(),
                                                                for_right,
                                                                in_secondary,
                                                            )),
                                                    )
                                                    .child(
                                                        icon_button(("mixed-tab-close", index), UiIcon::Close, "Close tab", false)
                                                            .w(px(22.0))
                                                            .h(px(22.0))
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.close_mixed_tab(&for_close, cx);
                                                            })),
                                                    )
                                            }))
                                            // Legacy Explorer preview tabs belong
                                            // to the same visual strip, never a
                                            // second line beneath live tabs.
                                            .children(tabs.into_iter().map(|(index, title, transient)| {
                                                let selected = showing_explorer_preview && active_tab == Some(index);
                                                div()
                                                    .id(("file-preview-tab", index))
                                                    .h(px(32.0))
                                                    .mr_2()
                                                    .px_1()
                                                    .flex()
                                                    .items_center()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(rgb(if selected { BORDER_ACTIVE } else { BORDER }))
                                                    .bg(rgb(if selected { ELEVATED } else { TOOLBAR }))
                                                    .child(
                                                        div()
                                                            .id(("file-tab", index))
                                                            .min_w_0()
                                                            .max_w(px(180.0))
                                                            .flex()
                                                            .items_center()
                                                            .gap_1()
                                                            .px_1()
                                                            .cursor_pointer()
                                                            .text_color(rgb(if selected { TEXT } else { MUTED }))
                                                            .child(icon(UiIcon::File, 12.0, if selected { TEXT } else { MUTED }))
                                                            .child(
                                                                div()
                                                                    .min_w_0()
                                                                    .max_w(px(160.0))
                                                                    .truncate()
                                                                    .text_sm()
                                                                    .when(transient, |label| label.italic())
                                                                    .child(title),
                                                            )
                                                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                                                this.model.activate_tab(index);
                                                                if event.click_count() >= 2 && transient {
                                                                    if let Some(path) = this.model.selected_path().map(PathBuf::from) {
                                                                        this.click_explorer_file(&path, true, window, cx);
                                                                    }
                                                                } else {
                                                                    this.explorer_preview_active = true;
                                                                    this.center_view = CenterView::Editor;
                                                                    window.focus(&this.explorer_keyboard_focus, cx);
                                                                }
                                                                this.persist_session();
                                                                cx.notify();
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button(("file-tab-close", index), UiIcon::Close, "Close preview", false)
                                                            .w(px(22.0))
                                                            .h(px(22.0))
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.model.close_tab(index);
                                                                if this.model.tabs().is_empty() {
                                                                    this.explorer_preview_active = false;
                                                                }
                                                                this.persist_session();
                                                                cx.notify();
                                                            })),
                                                    )
                                            }))
                                            .child({
                                                // Classic's + opens one New Tab
                                                // menu after all real content tabs.
                                                let owner = cx.entity().downgrade();
                                                let refresh_owner = owner.clone();
                                                let running = self.terminals.sessions.iter()
                                                    .filter(|session| session.running)
                                                    .map(|session| (
                                                        session.handle.clone(),
                                                        format!("{} · {}", session.label(), session.workspace_id),
                                                    ))
                                                    .collect::<Vec<_>>();
                                                icon_button("mixed-new-tab", UiIcon::Add, "New Tab", terminal_creating)
                                                    .w(px(28.0))
                                                    .h(px(28.0))
                                                    .dropdown_menu(Self::kit_new_tab_menu(
                                                        owner,
                                                        running,
                                                        terminal_create_busy,
                                                        terminal_create_timed_out,
                                                    ))
                                                    .on_open_change(move |open, _, cx| {
                                                        if *open {
                                                            let _ = refresh_owner.update(cx, |view, cx| {
                                                                view.refresh_terminals(cx);
                                                            });
                                                        }
                                                    })
                                            })
                                            .child(
                                                div()
                                                    .id("mixed-tabs-empty-area")
                                                    .flex_1()
                                                    .min_w(px(40.0))
                                                    .h(px(TAB_STRIP_HEIGHT - 1.0))
                                                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                                                        if event.click_count() == 2 {
                                                            this.new_editor(cx);
                                                        }
                                                    })),
                                            ),
                                    )
                                    .child(
                                        icon_button("mixed-pane-actions", UiIcon::More, "Pane actions", false)
                                            .dropdown_menu(Self::kit_pane_actions_menu(
                                                cx.entity().downgrade(),
                                                mixed_active.clone(),
                                                mixed_active.as_ref().is_some_and(|id| mixed_secondary_tab_ids.contains(id)),
                                                mixed_has_secondary,
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(29.0))
                                    .px_4()
                                    .bg(rgb(BG))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .when(!show_editor_toolbar, |panel| panel.hidden())
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .child(if has_native_editor {
                                                format!("Editing: {}{}", editor_title, if editor_dirty { " ●" } else { "" })
                                            } else {
                                                format!("{} · read-only preview", filename)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                icon_button("native-editor-new", UiIcon::NewFile, "Create new untitled editor", false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.new_editor(cx);
                                                    }))
                                                    .when(has_native_editor, |button| button.hidden()),
                                            )
                                            .child(
                                                icon_button("native-editor-edit", UiIcon::Edit, "Edit currently selected file", false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.open_editor(cx);
                                                    }))
                                                    .when(has_native_editor, |button| button.hidden()),
                                            )
                                            .when(editor.is_some() && !showing_explorer_preview, |toolbar| {
                                                toolbar
                                                    .child(
                                                        icon_button("native-editor-save", UiIcon::Save, "Save current editor file (Ctrl+S)", false)
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.save_editor(cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("native-editor-save-as", UiIcon::SaveAs, "Save current editor as a different file", false)
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.pick_save_as(cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("native-editor-close", UiIcon::Close, "Close current editor tab", false)
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.close_editor(false, cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("native-editor-discard", if discard_pending { UiIcon::Check } else { UiIcon::Delete }, "Discard unsaved changes (confirmation required)", discard_pending)
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.close_editor(true, cx);
                                                            }))
                                                            .when(!editor_dirty && !discard_pending, |button| button.hidden()),
                                                    )
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("mixed-pane-layout")
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
                                    .flex()
                                    .when(mixed_has_secondary && mixed_split_horizontal, |pane| pane.flex_row())
                                    .when(!mixed_has_secondary || !mixed_split_horizontal, |pane| pane.flex_col())
                                    .child(
                                        div()
                                            .id("mixed-primary-pane")
                                            .flex_1()
                                            .min_w_0()
                                            .min_h_0()
                                            .flex()
                                            .flex_col()
                            .child(
                                if let Some(editor) = editor.clone().filter(|_| !showing_explorer_preview) {
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .when(!center_is_editor, |panel| panel.hidden())
                                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                                            if this.mixed_panes.focus_pane(PaneSlot::Primary) {
                                                this.center_view = CenterView::Editor;
                                                this.save_mixed_pane_layout(cx);
                                                cx.notify();
                                            }
                                        }))
                                        .child(editor)
                                        .into_any_element()
                                } else {
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .when(!center_is_editor, |panel| panel.hidden())
                                        .id("preview-scroll")
                                        .overflow_y_scroll()
                                        .p_4()
                                        .text_sm()
                                        .font_family("JetBrains Mono")
                                        .child(content)
                                        .into_any_element()
                                },
                            )
                            .child(if has_terminal_preview {
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .when(center_is_editor, |panel| panel.hidden())
                                    .border_t_1()
                                    .border_color(rgb(BORDER))
                                    .bg(rgb(BG))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_h_0()
                                            .flex()
                                            .when(terminal_split_horizontal, |layout| layout.flex_row())
                                            .when(!terminal_split_horizontal, |layout| layout.flex_col())
                                            .child(if terminal_is_vt {
                                        div()
                                            .id("terminal-colored-grid")
                                            .relative()
                                            .key_context("AleraGpuiTerminalRaw")
                                            .track_focus(&terminal_keyboard_focus)
                                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                                                if !this.terminal_input.read(cx).focus_handle(cx).is_focused(window) {
                                                    this.clear_terminal_text_inputs(cx);
                                                }
                                                this.focus_primary_mixed_terminal(cx);
                                                let focus = this.terminal_input.read(cx).focus_handle(cx);
                                                window.focus(&focus, cx);
                                            }))
                                            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                                                this.finish_terminal_line_selection(cx);
                                                // On Windows GPUI the pointer's default focus
                                                // may win after MouseDown. Reassert the hidden
                                                // IME sink once the click/selection completes.
                                                let focus = this.terminal_input.read(cx).focus_handle(cx);
                                                window.focus(&focus, cx);
                                            }))
                                            .on_mouse_up_out(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                this.finish_terminal_line_selection(cx);
                                            }))
                                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                                if event.keystroke.modifiers.control
                                                    && event.keystroke.modifiers.shift
                                                    && event.keystroke.key.eq_ignore_ascii_case("c")
                                                {
                                                    this.copy_primary_terminal_text(false, cx);
                                                    cx.stop_propagation();
                                                } else if this.send_terminal_key_for_pane(PaneSlot::Primary, event, cx) {
                                                    cx.stop_propagation();
                                                }
                                            }))
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_x_scroll()
                                            .overflow_y_scroll()
                                            .px_3()
                                            .py_2()
                                            .font_family("JetBrains Mono")
                                            .text_size(px(terminal_font_size))
                                            .whitespace_nowrap()
                                            .child(div()
                                                .absolute()
                                                .left(px(primary_ime_anchor.0))
                                                .top(px(primary_ime_anchor.1))
                                                .w(px(1.0))
                                                .h(px(1.0))
                                                .child(terminal_input.clone()))
                                            .children(terminal_screen_rows.into_iter().enumerate().map(
                                                |(row_index, row)| {
                                                    let selected = terminal_selected_rows.get(row_index).copied().unwrap_or(false);
                                                    div()
                                                        .id(("terminal-colored-row", row_index))
                                                        .whitespace_nowrap()
                                                        .line_height(px(terminal_font_size * 1.3))
                                                        .bg(rgb(if selected { ACTIVE } else { BG }))
                                                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                            this.begin_terminal_line_selection(row_index, cx);
                                                        }))
                                                        .on_mouse_move(cx.listener(move |this, _, _, cx| {
                                                            this.extend_terminal_line_selection(row_index, cx);
                                                        }))
                                                        .child(
                                                            gpui::StyledText::new(row.text)
                                                                .with_highlights(row.spans.into_iter().map(|span| {
                                                                    let style = span.style;
                                                                    (
                                                                        span.range,
                                                                        gpui::HighlightStyle {
                                                                            color: Some(rgb(style.fg).into()),
                                                                            background_color: Some(rgb(if selected { ACTIVE } else { style.bg }).into()),
                                                                            font_weight: style.bold.then_some(gpui::FontWeight::BOLD),
                                                                            font_style: style.italic.then_some(gpui::FontStyle::Italic),
                                                                            underline: style.underline.then_some(gpui::UnderlineStyle {
                                                                                thickness: px(1.0),
                                                                                color: Some(rgb(style.fg).into()),
                                                                                wavy: false,
                                                                            }),
                                                                            ..Default::default()
                                                                        },
                                                                    )
                                                                })),
                                                        )
                                                },
                                            ))
                                    } else {
                                        div()
                                            .id("terminal-output-scroll")
                                            .key_context("AleraGpuiTerminalRaw")
                                            .track_focus(&terminal_keyboard_focus)
                                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                                                this.focus_primary_mixed_terminal(cx);
                                                window.focus(&this.terminal_input.read(cx).focus_handle(cx), cx);
                                            }))
                                            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                                                let focus = this.terminal_input.read(cx).focus_handle(cx);
                                                window.focus(&focus, cx);
                                            }))
                                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                                if event.keystroke.modifiers.control
                                                    && event.keystroke.modifiers.shift
                                                    && event.keystroke.key.eq_ignore_ascii_case("c")
                                                {
                                                    this.copy_primary_terminal_text(false, cx);
                                                    cx.stop_propagation();
                                                } else if this.send_terminal_key_for_pane(PaneSlot::Primary, event, cx) {
                                                    cx.stop_propagation();
                                                }
                                            }))
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_y_scroll()
                                            .px_3()
                                            .py_2()
                                            .font_family("JetBrains Mono")
                                            .text_size(px(terminal_font_size))
                                            .child(terminal_input)
                                            .child(if terminal_output.is_empty() {
                                                "No terminal content yet. Auto updates are enabled by default.".into()
                                            } else {
                                                terminal_output
                                            })
                                            })
                                            .child(if let Some((title, output)) = terminal_secondary_output {
                                                div()
                                                    .id("terminal-secondary-preview")
                                                    .flex_1()
                                                    .min_h_0()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .border_t_1()
                                                    .border_color(rgb(BORDER))
                                                    .child(div()
                                                        .id("terminal-secondary-focus")
                                                        .h(px(28.0))
                                                        .px_2()
                                                        .bg(rgb(PANEL))
                                                        .text_sm()
                                                        .cursor_pointer()
                                                        .child(format!("Second pane: {title} · click to focus"))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.switch_terminal_split_focus(cx);
                                                        }))
                                                        .on_drop(cx.listener(|this, drag: &TerminalTabDrag, _, cx| {
                                                            if drag.workspace_generation != this.workspace_generation {
                                                                return;
                                                            }
                                                            if !drag.target.as_ref().is_some_and(|target| {
                                                                target.handle == drag.handle &&
                                                                this.terminals.context_target_is_live(target)
                                                            }) {
                                                                return;
                                                            }
                                                            if this.terminal_panes.drop_tab_on_secondary(&mut this.terminals, &drag.handle) {
                                                                this.terminal_key_queue.clear();
                                                                this.terminal_key_queue_target = None;
                                                                this.save_terminal_layout();
                                                                this.refresh_terminal_output(cx);
                                                                this.refresh_terminal_split_output(cx);
                                                                cx.notify();
                                                            }
                                                        })))
                                                    .child(div()
                                                        .id("terminal-secondary-scroll")
                                                        .flex_1()
                                                        .min_h_0()
                                                        .overflow_x_scroll()
                                                        .overflow_y_scroll()
                                                        .px_3()
                                                        .py_2()
                                                        .font_family("JetBrains Mono")
                                                        .text_size(px(terminal_font_size))
                                                        .child(output))
                                                    .into_any_element()
                                            } else {
                                                div().h(px(0.0)).into_any_element()
                                            }),
                                    )
                                    .into_any_element()
                            } else {
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .when(center_is_editor, |panel| panel.hidden())
                                    .p_4()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .child("No Host terminal selected. Choose an existing session in Projects on the left.")
                                            .child(
                                                div()
                                                    .text_color(rgb(TEXT))
                                                    .child(terminal_status),
                                            ),
                                    )
                                    .into_any_element()
                            }),
                                    )
                                    .child(if let Some((label, preview)) = mixed_secondary_preview {
                                        div()
                                            .id("mixed-secondary-pane")
                                            .flex_1()
                                            .min_w_0()
                                            .min_h_0()
                                            .flex()
                                            .flex_col()
                                            .border_l_1()
                                            .border_color(rgb(BORDER))
                                            .child(
                                                div()
                                                    .id("mixed-secondary-toolbar")
                                                    .h(px(TAB_STRIP_HEIGHT))
                                                    .px_2()
                                                    .flex()
                                                    .items_center()
                                                    .gap_3()
                                                    .bg(rgb(PANEL))
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .text_sm()
                                                            .child(format!(
                                                                "Second · {label} · {}",
                                                                if secondary_editor_live { "Editor" } else { "Preview" }
                                                            ))
                                                            .on_drop(cx.listener(|this, drag: &MixedTabDrag, window, cx| {
                                                                if drag.workspace_generation == this.workspace_generation
                                                                    && this.mixed_tab_is_current(&drag.identity, cx)
                                                                    && this.transfer_editor_to_pane(&drag.identity, PaneSlot::Secondary, cx)
                                                                {
                                                                    this.focus_mixed_tab(&drag.identity, window, cx);
                                                                    this.refresh_terminal_split_output(cx);
                                                                }
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("mixed-split-axis")
                                                            .cursor_pointer()
                                                            .text_xs()
                                                            .child(if mixed_split_horizontal { "Stack" } else { "Side by side" })
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.mixed_panes.toggle_axis();
                                                                this.save_mixed_pane_layout(cx);
                                                                cx.notify();
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("mixed-split-close")
                                                            .cursor_pointer()
                                                            .text_xs()
                                                            .child("Unsplit")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.unsplit_mixed_panes(cx);
                                                            })),
                                                    ),
                                            )
                                            .child(if let Some(editor) = secondary_native_editor {
                                                div()
                                                    .id("mixed-secondary-editor")
                                                    .flex_1()
                                                    .min_h_0()
                                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                        if this.mixed_panes.focus_pane(PaneSlot::Secondary) {
                                                            this.center_view = CenterView::Editor;
                                                            this.save_mixed_pane_layout(cx);
                                                            cx.notify();
                                                        }
                                                    }))
                                                    .child(editor)
                                                    .into_any_element()
                                            } else {
                                                div()
                                                    .id("mixed-secondary-content")
                                                    .relative()
                                                    .flex_1()
                                                    .min_h_0()
                                                    .key_context("AleraGpuiTerminalRaw")
                                                    .track_focus(&secondary_terminal_keyboard_focus)
                                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, window, cx| {
                                                        if !this.secondary_terminal_input.read(cx).focus_handle(cx).is_focused(window) {
                                                            this.clear_terminal_text_inputs(cx);
                                                        }
                                                        if let Some(target) = mixed_secondary_host.as_ref() {
                                                            if this.terminals.context_target_is_live(target) {
                                                                if let Some(index) = this.terminals.index_for_context(target) {
                                                                if this.terminals.active_tab != Some(index) {
                                                                    this.terminal_key_queue.clear();
                                                                    this.terminal_key_queue_target = None;
                                                                    this.clear_terminal_text_inputs(cx);
                                                                }
                                                                this.mixed_panes.focus_pane(PaneSlot::Secondary);
                                                                this.terminals.activate(index);
                                                                this.center_view = CenterView::Terminal;
                                                                this.refresh_terminal_output(cx);
                                                                this.save_mixed_pane_layout(cx);
                                                                window.focus(&this.secondary_terminal_input.read(cx).focus_handle(cx), cx);
                                                                cx.notify();
                                                                }
                                                            }
                                                        }
                                                    }))
                                                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                                                        if this.mixed_panes.focused_pane() == PaneSlot::Secondary {
                                                            window.focus(&this.secondary_terminal_input.read(cx).focus_handle(cx), cx);
                                                        }
                                                    }))
                                                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                                                        let authorized = mixed_secondary_key_target.as_ref().is_some_and(|target| {
                                                            this.terminals.context_target_is_live(target)
                                                                && this.terminals.index_for_context(target) == this.terminals.active_tab
                                                        });
                                                        if authorized && this.send_terminal_key_for_pane(PaneSlot::Secondary, event, cx) {
                                                            cx.stop_propagation();
                                                        }
                                                    }))
                                                    .overflow_x_scroll()
                                                    .overflow_y_scroll()
                                                    .p_3()
                                                    .font_family("JetBrains Mono")
                                                    .text_size(px(terminal_font_size))
                                                    .child(div()
                                                        .absolute()
                                                        .left(px(secondary_ime_anchor.0))
                                                        .top(px(secondary_ime_anchor.1))
                                                        .w(px(1.0))
                                                        .h(px(1.0))
                                                        .child(secondary_terminal_input))
                                                    .child(if let Some(rows) = mixed_secondary_vt_rows {
                                                        div()
                                                            .id("mixed-secondary-colored-grid")
                                                            .flex()
                                                            .flex_col()
                                                            .whitespace_nowrap()
                                                            .children(rows.into_iter().enumerate().map(|(index, row)| {
                                                                div()
                                                                    .id(("mixed-secondary-colored-row", index))
                                                                    .whitespace_nowrap()
                                                                    .line_height(px(terminal_font_size * 1.3))
                                                                    .child(
                                                                        gpui::StyledText::new(row.text)
                                                                            .with_highlights(row.spans.into_iter().map(|span| {
                                                                                let style = span.style;
                                                                                (
                                                                                    span.range,
                                                                                    gpui::HighlightStyle {
                                                                                        color: Some(rgb(style.fg).into()),
                                                                                        background_color: Some(rgb(style.bg).into()),
                                                                                        font_weight: style.bold.then_some(gpui::FontWeight::BOLD),
                                                                                        font_style: style.italic.then_some(gpui::FontStyle::Italic),
                                                                                        underline: style.underline.then_some(gpui::UnderlineStyle {
                                                                                            thickness: px(1.0),
                                                                                            color: Some(rgb(style.fg).into()),
                                                                                            wavy: false,
                                                                                        }),
                                                                                        ..Default::default()
                                                                                    },
                                                                                )
                                                                            })),
                                                                    )
                                                            }))
                                                            .into_any_element()
                                                    } else {
                                                        div()
                                                            .id("mixed-secondary-plain-text")
                                                            .child(preview)
                                                            .into_any_element()
                                                    })
                                                    .into_any_element()
                                            })
                                            .into_any_element()
                                    } else {
                                        div().h(px(0.0)).into_any_element()
                                    }),
                            )
                    )
                    .child(
                        div()
                            .w(px(PROJECTS_SIDEBAR_WIDTH))
                            .flex()
                            .flex_col()
                            .bg(rgb(PANEL))
                            .border_r_1()
                            .border_color(rgb(BORDER))
                            .px_2()
                            .py_1()
                            .gap_2()
                            .child(
                                div()
                                    .h(px(SECTION_HEADER_HEIGHT))
                                    .px_1()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .id("projects-toggle")
                                            .cursor_pointer()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child(if show_recent_workspaces { "Projects ▾" } else { "Projects ▸" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.show_recent_workspaces = !this.show_recent_workspaces;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        icon_button("projects-open-folder", UiIcon::Open, "Open a project folder…", picking_workspace)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.pick_workspace(cx);
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(format!("Active workspace · {folder_name}")),
                            )
                            .child(
                                div()
                                    .id("project-tree-panel")
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .text_sm()
                                                    .child("Projects / Worktrees"),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(icon_button("projects-management-toggle", UiIcon::Settings, "Manage saved projects and Git worktrees", self.show_project_management)
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.show_project_management = !this.show_project_management;
                                                            cx.notify();
                                                        })))
                                                    .child(icon_button("project-tree-refresh", UiIcon::Refresh, "Refresh saved projects and worktrees", project_tree_loading)
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.refresh_project_tree(cx);
                                                        }))),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("project-tree-scroll")
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_y_scroll()
                                            .children(project_tree_rows.into_iter().map(|(index, group, collapsed)| {
                                                let root = group.root.clone();
                                                let fold_root = group.root.clone();
                                                let current = group.is_active;
                                                let pinned = group.is_saved;
                                                let title = group.label;
                                                let error = group.error;
                                                div()
                                                    .id(("project-tree-group", index))
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .py_1()
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_1()
                                                            .child(
                                                                icon_button(("project-tree-toggle", index),
                                                                    if collapsed { UiIcon::ChevronRight } else { UiIcon::ChevronDown },
                                                                    "Expand or collapse this project's worktrees",
                                                                    false)
                                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                                        if !this.collapsed_project_roots.insert(fold_root.clone()) {
                                                                            this.collapsed_project_roots.remove(&fold_root);
                                                                        }
                                                                        cx.notify();
                                                                    })),
                                                            )
                                                            .child(
                                                                div()
                                                                    .id(("project-tree-open", index))
                                                                    .flex_1()
                                                                    .min_w_0()
                                                                    .px_2()
                                                                    .py_1()
                                                                    .bg(rgb(if current { ACTIVE } else { PANEL }))
                                                                    .hover(|style| style.bg(rgb(ACTIVE)))
                                                                    .cursor_pointer()
                                                                    .text_sm()
                                                                    .text_color(rgb(if current { TEXT } else { MUTED }))
                                                                    .child(format!("{}{}", if pinned { "★ " } else { "" }, title))
                                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                                        this.switch_workspace(root.clone(), cx);
                                                                    })),
                                                            ),
                                                    )
                                                    .children(group.worktrees.into_iter().take(if collapsed { 0 } else { 14 }).enumerate().map(|(child_index, entry)| {
                                                        let target = entry.path.clone();
                                                        let active = workspace_root.starts_with(&target);
                                                        let label = entry.branch.unwrap_or_else(|| {
                                                            if entry.detached { "Detached HEAD".into() } else { "Unnamed".into() }
                                                        });
                                                        div()
                                                            .id(("project-tree-worktree", index * 16 + child_index))
                                                            .pl_4()
                                                            .py_1()
                                                            .bg(rgb(if active { ACTIVE } else { PANEL }))
                                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                                            .cursor_pointer()
                                                            .text_xs()
                                                            .text_color(rgb(if active { TEXT } else { MUTED }))
                                                            .child(format!(
                                                                "{} {} {}",
                                                                if active { "●" } else { "○" },
                                                                if entry.is_primary { "main:" } else { "worktree:" },
                                                                label
                                                            ))
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.switch_workspace(target.clone(), cx);
                                                            }))
                                                    }))
                                                    .child(if let Some(error) = error {
                                                        div().text_xs().text_color(rgb(MUTED))
                                                            .child(format!("Git unavailable: {error}"))
                                                            .into_any_element()
                                                    } else {
                                                        div().h(px(0.0)).into_any_element()
                                                    })
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("project-management-details")
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .when(!self.show_project_management, |panel| panel.hidden())
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_sm()
                                            .child("Saved projects"),
                                    )
                                    .child(
                                        icon_button("project-pin-current", UiIcon::Check, "Save / pin the current project", false)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.pin_current_project(cx);
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("saved-projects-scroll")
                                    .max_h(px(72.0))
                                    .overflow_y_scroll()
                                    .children(saved_projects.into_iter().map(|(index, path)| {
                                        let open_path = path.clone();
                                        let unpin_path = path.clone();
                                        let title = path.file_name()
                                            .map(|name| name.to_string_lossy().to_string())
                                            .unwrap_or_else(|| path.display().to_string());
                                        let active = workspace_root == path;
                                        div()
                                            .id(("saved-project-row", index))
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .id(("saved-project-open", index))
                                                    .flex_1()
                                                    .min_w_0()
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(if active { TEXT } else { MUTED }))
                                                    .child(format!("{} {title}", if active { "●" } else { "○" }))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.switch_workspace(open_path.clone(), cx);
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id(("saved-project-unpin", index))
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child("×")
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.unpin_project(&unpin_path, cx);
                                                    })),
                                            )
                                    })),
                            )
                            .child(if show_recent_workspaces {
                                div()
                                    .id("project-workspaces-scroll")
                                    .max_h(px(82.0))
                                    .overflow_y_scroll()
                                    .children(recent_workspaces.into_iter().map(|(index, path)| {
                                        let active = workspace_root == path;
                                        let title = path.file_name()
                                            .map(|name| name.to_string_lossy().to_string())
                                            .unwrap_or_else(|| path.display().to_string());
                                        div()
                                            .id(("project-workspace", index))
                                            .cursor_pointer()
                                            .px_1()
                                            .py_1()
                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(if active { TEXT } else { MUTED }))
                                                    .child(format!("{} {title}", if active { "●" } else { "○" })),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child(path.display().to_string()),
                                            )
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.switch_workspace(path.clone(), cx);
                                            }))
                                    }))
                                    .into_any_element()
                            } else {
                                div().h(px(0.0)).into_any_element()
                            })
                            .child(
                                div()
                                    .id("project-worktree-heading")
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().font_weight(gpui::FontWeight::BOLD).text_sm().child("Git Worktrees"))
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("worktree-refresh")
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(MUTED))
                                                    .child(if worktree_loading { "Loading…" } else { "↻" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.refresh_worktrees(cx);
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("worktree-add-toggle")
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(TEXT))
                                                    .child("+ New")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.toggle_worktree_creator(cx);
                                                    })),
                                            ),
                                    ),
                            )
                            .child(if let Some(input) = worktree_create_input {
                                div()
                                    .id("worktree-create-form")
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(input)
                                    .child(
                                        div()
                                            .id("worktree-create-confirm")
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(rgb(TEXT))
                                            .child(if worktree_busy { "Creating…" } else { "Create branch + worktree" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.create_worktree(cx);
                                            })),
                                    )
                                    .into_any_element()
                            } else {
                                div().h(px(0.0)).into_any_element()
                            })
                            .child(
                                div()
                                    .id("worktree-list-scroll")
                                    .max_h(px(90.0))
                                    .overflow_y_scroll()
                                    .children(worktree_rows.into_iter().map(|(index, entry)| {
                                        let target = entry.path.clone();
                                        let open_path = target.clone();
                                        let remove_path = target.clone();
                                        let is_active = workspace_root == target;
                                        let removable = !entry.is_primary && !entry.locked && !entry.prunable && !is_active;
                                        let is_confirm = worktree_delete_pending.as_deref() == Some(target.as_path());
                                        let label = entry.branch.unwrap_or_else(|| {
                                            if entry.detached { "Detached HEAD".into() } else { "(unnamed)".into() }
                                        });
                                        div()
                                            .id(("worktree-row", index))
                                            .flex()
                                            .flex_col()
                                            .py_1()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .flex()
                                                    .justify_between()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .id(("worktree-open", index))
                                                            .flex_1()
                                                            .min_w_0()
                                                            .cursor_pointer()
                                                            .text_sm()
                                                            .text_color(rgb(if is_active { TEXT } else { MUTED }))
                                                            .child(format!("{} {label}", if is_active { "●" } else { "○" }))
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.switch_workspace(open_path.clone(), cx);
                                                            })),
                                                    )
                                                    .child(if removable {
                                                        div()
                                                            .id(("worktree-remove", index))
                                                            .cursor_pointer()
                                                            .text_xs()
                                                            .text_color(rgb(if is_confirm { TEXT } else { MUTED }))
                                                            .child(if is_confirm { "Confirm" } else { "Remove" })
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.remove_worktree(remove_path.clone(), cx);
                                                            }))
                                                            .into_any_element()
                                                    } else {
                                                        div().text_xs().text_color(rgb(MUTED))
                                                            .child(if entry.is_primary { "Main" } else if is_active { "Active" } else { "Locked" })
                                                            .into_any_element()
                                                    }),
                                            )
                                            .child(div().text_xs().text_color(rgb(MUTED)).child(target.display().to_string()))
                                    })),
                            )
                            .child(if let Some(error) = worktree_error {
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(format!("Worktrees: {error}"))
                                    .into_any_element()
                            } else {
                                div().h(px(0.0)).into_any_element()
                            })
                            )
                            .child(
                                div()
                                    .id("runtime-tools-toggle")
                                    .h(px(34.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .cursor_pointer()
                                    .border_t_1()
                                    .border_color(rgb(BORDER))
                                    .child(div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child("RUNTIME / AGENTS"))
                                    .child(icon(
                                        if self.runtime_details_open { UiIcon::Close } else { UiIcon::More },
                                        16.0, MUTED,
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.runtime_details_open = !this.runtime_details_open;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("runtime-tools-details")
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .when(!self.runtime_details_open, |panel| panel.hidden())
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Runtime Host"),
                            )
                            .child(div().text_sm().text_color(rgb(MUTED)).child(runtime_status))
                            .child(
                                div()
                                    .id("runtime-status-refresh")
                                    .cursor_pointer()
                                    .text_sm()
                                    .text_color(rgb(TEXT))
                                    .child(if runtime_loading {
                                        "Checking…"
                                    } else {
                                        "Check Host"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_runtime(cx);
                                    })),
                            )
                            .child(div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(mcp_ui_status))
                            .child(div()
                                .id("native-mcp-copy-window-id")
                                .cursor_pointer()
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .child("Copy Native MCP window ID")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(mcp_ui_window_id.clone()));
                                    this.model.set_status("Native MCP window ID copied; set windowId when Classic and Native are both open");
                                    cx.notify();
                                })))
                            .child(div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(div()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_sm()
                                    .child("Agent Profiles"))
                                .child(div()
                                    .id("agent-profiles-refresh")
                                    .cursor_pointer()
                                    .text_xs()
                                    .text_color(rgb(TEXT))
                                    .child(if agent_profiles_loading { "Loading…" } else { "Refresh ↻" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_agent_profiles(cx);
                                    }))))
                            .child(div()
                                .id("agent-profiles-list")
                                .max_h(px(116.0))
                                .overflow_y_scroll()
                                .children(agent_profile_rows.into_iter().map(|(index, profile)| {
                                    let selected = agent_profile_selected.as_deref() == Some(profile.id.as_str());
                                    let id = profile.id;
                                    let kind = if profile.managed { "Managed" } else { "Command" };
                                    let label = format!("{} {} · {} ({kind})",
                                        if selected { "●" } else { "○" }, profile.name, profile.agent_type);
                                    div()
                                        .id(("agent-profile-row", index))
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(rgb(if selected { TEXT } else { MUTED }))
                                        .hover(|style| style.bg(rgb(ACTIVE)))
                                        .py_1()
                                        .child(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.agent_selected_profile = Some(id.clone());
                                            cx.notify();
                                        }))
                                })))
                            .child(agent_prompt_input)
                            .child(div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(div()
                                    .id("agent-profile-launch")
                                    .cursor_pointer()
                                    .text_sm()
                                    .text_color(rgb(if agent_launch_busy || agent_pending { MUTED } else { TEXT }))
                                    .child(if agent_launch_busy { "Starting Agent…" } else { "+ Launch Agent" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.launch_selected_agent(cx);
                                    })))
                                .child(if agent_pending {
                                    div()
                                        .id("agent-dismiss-tracking")
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child("Dismiss tracking")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.agent_pending_tab = None;
                                            this.agent_status = "Agent still belongs to Host; use Attach existing to find it".into();
                                            cx.notify();
                                        }))
                                        .into_any_element()
                                } else { div().h(px(0.0)).into_any_element() }))
                            .child(div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(agent_status))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child("Terminal sessions"),
                                    )
                                    .child(
                                        div()
                                            .id("terminal-sessions-refresh")
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(rgb(TEXT))
                                            .child(if terminal_list_loading {
                                                "Checking…"
                                            } else {
                                                "Reconnect ↻"
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.refresh_terminals(cx);
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(HOST_SESSION_LIST_SCOPE),
                            )
                            .child(
                                div()
                                    .id("terminal-sessions-scroll")
                                    .max_h(px(145.0))
                                    .overflow_y_scroll()
                                    .children(terminal_sessions.into_iter().enumerate().map(|(index, (handle, label, workspace_id))| {
                                        div()
                                            .id(("terminal-session", index))
                                            .py_1()
                                            .px_1()
                                            .cursor_pointer()
                                            .text_sm()
                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                            .child(label)
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child(format!("Host workspace: {workspace_id}")),
                                            )
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.select_terminal(handle.clone(), cx);
                                            }))
                                    })),
                            )
                            )
                    ),
            )
            .child(
                div()
                    .h(px(STATUS_BAR_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(TOOLBAR))
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(status)
                    .child(self.render_kit_branch_dropdown(
                        "status-branch-picker",
                        format!("{} · {}", branch, NATIVE_APP_NAME),
                        cx,
                    )),
            )
    }
}

#[cfg(windows)]
fn install_native_windows_app_identity() {
    // The Flutter desktop keeps dev.leynier.alera (or .dev) as its single-
    // instance identifier. Native must use its own taskbar/shortcut grouping.
    // shell32 is part of Windows, so this adds no new Rust dependencies.
    #[link(name = "shell32")]
    extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
    }
    use alera_gpui_desktop::edition::NATIVE_WINDOWS_APP_ID;
    let utf16 = NATIVE_WINDOWS_APP_ID
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let hr = unsafe { SetCurrentProcessExplicitAppUserModelID(utf16.as_ptr()) };
    if hr < 0 {
        eprintln!("Alera Native: Windows AppUserModelID could not be set (HRESULT {hr:#x})");
    }
}

fn main() {
    #[cfg(windows)]
    install_native_windows_app_identity();
    let session_store = PreviewSessionStore::platform_default();
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .or_else(|| {
            session_store.as_ref().and_then(|store| {
                store.last_workspace().unwrap_or_else(|error| {
                    eprintln!("Alera GPUI: cannot load last workspace: {error}");
                    None
                })
            })
        })
        .unwrap_or_else(|| std::env::current_dir().expect("current working directory"));
    let mut model = match WorkbenchModel::new(root) {
        Ok(model) => model,
        Err(error) => {
            eprintln!("Alera GPUI: unable to open workspace: {error}");
            std::process::exit(1);
        }
    };
    if let Some(store) = &session_store {
        // Preserve a malformed snapshot for possible recovery instead of
        // overwriting it with an empty workspace at startup.
        match store.restore(&mut model) {
            Ok(_) => {
                if let Err(error) = store.save(&model) {
                    eprintln!("Alera GPUI: cannot save preview session: {error}");
                }
            }
            Err(error) => {
                eprintln!("Alera GPUI: cannot restore preview session: {error}");
                model.set_status(format!("Previous preview session unavailable: {error}"));
            }
        }
    }
    let runtime_bridge = RuntimeCliBridge::discover().ok();
    let runtime_status = if runtime_bridge.is_some() {
        "Not checked · use existing Alera CLI".into()
    } else {
        "CLI not found · set ALERA_CLI_PATH".into()
    };
    let recent_workspaces = session_store
        .as_ref()
        .and_then(|store| store.recent_workspaces().ok())
        .filter(|roots| !roots.is_empty())
        .unwrap_or_else(|| vec![model.root().to_path_buf()]);
    let saved_projects = session_store
        .as_ref()
        .and_then(|store| store.saved_projects().ok())
        .unwrap_or_default();
    let initial_git_favorites = match session_store
        .as_ref()
        .map(|store| store.load_git_favorites(model.root()))
    {
        Some(Ok(saved)) => saved,
        Some(Err(error)) => {
            model.set_status(format!(
                "Native Git favorites could not be restored: {error}"
            ));
            Vec::new()
        }
        None => Vec::new(),
    };
    let native_settings_store = session_store
        .as_ref()
        .map(|store| NativeSettingsStore::new(store.state_dir()));
    let (native_settings, native_settings_error) = match native_settings_store.as_ref() {
        Some(store) => match store.read() {
            Ok(settings) => (settings, None),
            Err(error) => (NativeSettings::default(), Some(error.to_string())),
        },
        None => (
            NativeSettings::default(),
            Some("Native state directory unavailable".into()),
        ),
    };
    let initial_visibility = ExplorerVisibilityOptions {
        show_ignored: native_settings.explorer_show_ignored,
        show_hidden: native_settings.explorer_show_hidden,
    };

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            // Kit initializes its global palette in Light mode. Alera Native
            // has a deliberately dark shell regardless of Windows appearance.
            // Configure Kit once at the root so buttons, menus, tabs and
            // dialogs share the same Classic-derived semantic colors.
            native_ui_theme::install_alera_dark_theme(cx);
            // Flutter embeds these licensed typefaces. Without explicitly
            // registering them GPUI silently falls back to a Windows font even
            // if font_family("Inter") is set on the native workbench root.
            if let Err(error) = cx.text_system().add_fonts(vec![
                Cow::Borrowed(include_bytes!("../../assets/fonts/Inter-Variable.ttf").as_slice()),
                Cow::Borrowed(
                    include_bytes!("../../assets/fonts/JetBrainsMono-Variable.ttf").as_slice(),
                ),
                Cow::Borrowed(
                    include_bytes!("../../assets/fonts/VSCodeCodicons-0.0.46-24.ttf").as_slice(),
                ),
            ]) {
                eprintln!("Alera GPUI: cannot load bundled desktop fonts: {error}");
            }
            bind_editor_keys(cx);
            bind_quick_open_keys(cx);
            cx.bind_keys([
                KeyBinding::new("ctrl-p", ToggleFileSearch, None),
                KeyBinding::new("cmd-p", ToggleFileSearch, None),
                KeyBinding::new("ctrl-c", ExplorerCopy, Some("AleraGpuiExplorer")),
                KeyBinding::new("cmd-c", ExplorerCopy, Some("AleraGpuiExplorer")),
                KeyBinding::new("ctrl-x", ExplorerCut, Some("AleraGpuiExplorer")),
                KeyBinding::new("cmd-x", ExplorerCut, Some("AleraGpuiExplorer")),
                KeyBinding::new("ctrl-v", ExplorerPaste, Some("AleraGpuiExplorer")),
                KeyBinding::new("cmd-v", ExplorerPaste, Some("AleraGpuiExplorer")),
                KeyBinding::new("ctrl-a", ExplorerSelectAll, Some("AleraGpuiExplorer")),
                KeyBinding::new("cmd-a", ExplorerSelectAll, Some("AleraGpuiExplorer")),
                KeyBinding::new("ctrl-shift-f", ToggleContentSearch, None),
                KeyBinding::new("cmd-shift-f", ToggleContentSearch, None),
            ]);
            let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let view = cx.new(|cx| {
                        let terminal_input = cx.new(QuickOpenInput::terminal);
                        let secondary_terminal_input = cx.new(QuickOpenInput::terminal);
                        cx.observe_in(
                            &terminal_input,
                            window,
                            |view: &mut Workbench, input, window, cx| {
                                // GPUI TextInput delivers ASCII and finalized IME
                                // characters here. Composing text never enters a
                                // Host until its native input has committed while
                                // still owning the actual window keyboard focus.
                                let has_os_focus =
                                    input.read(cx).focus_handle(cx).is_focused(window);
                                let committed =
                                    input.update(cx, |input, _| input.take_terminal_committed());
                                for text in committed {
                                    view.enqueue_terminal_ime_text_for_pane(
                                        PaneSlot::Primary,
                                        has_os_focus,
                                        text,
                                        cx,
                                    );
                                }
                            },
                        )
                        .detach();
                        cx.observe_in(
                            &secondary_terminal_input,
                            window,
                            |view: &mut Workbench, input, window, cx| {
                                let has_os_focus =
                                    input.read(cx).focus_handle(cx).is_focused(window);
                                let committed =
                                    input.update(cx, |input, _| input.take_terminal_committed());
                                for text in committed {
                                    view.enqueue_terminal_ime_text_for_pane(
                                        PaneSlot::Secondary,
                                        has_os_focus,
                                        text,
                                        cx,
                                    );
                                }
                            },
                        )
                        .detach();
                        let explorer_filter_input =
                            cx.new(|cx| QuickOpenInput::with_placeholder("Filter files…", cx));
                        cx.observe(&explorer_filter_input, |view: &mut Workbench, _, cx| {
                            // The visual result set has changed; never let hidden
                            // stale selections enter a later batch mutation.
                            view.explorer_multi_selection.clear();
                            view.explorer_selection = None;
                            if !view
                                .explorer_filter_input
                                .read(cx)
                                .query()
                                .trim()
                                .is_empty()
                            {
                                view.ensure_file_index(cx);
                            }
                            cx.notify();
                        })
                        .detach();
                        let mut workbench = Workbench {
                            tree: WorkspaceTree::new_with_options(model.root(), initial_visibility)
                                .ok(),
                            tree_mode: true,
                            explorer_visibility: initial_visibility,
                            explorer_selection: None,
                            explorer_multi_selection: ExplorerMultiSelection::new(model.root()),
                            explorer_preview_active: false,
                            explorer_keyboard_focus: cx.focus_handle(),
                            explorer_clipboard: None,
                            explorer_drop_busy: false,
                            explorer_drop_request: 0,
                            explorer_action: None,
                            explorer_action_input: None,
                            explorer_action_modal_focus: cx.focus_handle(),
                            explorer_delete_confirmed: false,
                            explorer_delete_pending: None,
                            model,
                            center_view: CenterView::Terminal,
                            right_sidebar_view: RightSidebarView::Explorer,
                            explorer_context_menu: None,
                            show_project_management: false,
                            runtime_details_open: false,
                            editor: None,
                            secondary_editor: None,
                            editor_transfer_in_progress: false,
                            editor_file_layout_snapshot: Vec::new(),
                            editor_active_layout_snapshot: None,
                            session_store,
                            recent_workspaces,
                            saved_projects,
                            project_tree_groups: Vec::new(),
                            project_tree_loading: false,
                            project_tree_request: 0,
                            collapsed_project_roots: HashSet::new(),
                            show_recent_workspaces: true,
                            worktrees: Vec::new(),
                            worktree_error: None,
                            worktree_loading: false,
                            worktree_busy: false,
                            worktree_create_input: None,
                            worktree_delete_pending: None,
                            picking_workspace: false,
                            quick_open: None,
                            explorer_filter_input,
                            file_index: None,
                            file_index_loading: false,
                            file_index_request: 0,
                            content_query: None,
                            content_loading: false,
                            content_matches: Vec::new(),
                            content_truncated: false,
                            content_error: None,
                            content_searched_query: String::new(),
                            content_request: 0,
                            git_snapshot: None,
                            git_selected_changes: HashSet::new(),
                            git_selection_anchor: None,
                            git_history: None,
                            git_history_open: false,
                            git_history_options: GitHistoryOptions {
                                favorites: initial_git_favorites,
                                ..Default::default()
                            },
                            git_history_loading: false,
                            git_history_error: None,
                            git_history_request: 0,
                            git_merge_armed: None,
                            git_sensitive_armed: None,
                            git_conflict_preview: None,
                            git_conflict_choices: HashMap::new(),
                            git_conflict_selected: None,
                            git_conflict_loading: false,
                            git_conflict_request: 0,
                            git_conflict_confirm_armed: false,
                            git_filter_input: cx
                                .new(|cx| QuickOpenInput::with_placeholder("Filter files…", cx)),
                            git_commit_input: cx
                                .new(|cx| QuickOpenInput::with_placeholder("Commit message", cx)),
                            git_branch_input: cx
                                .new(|cx| QuickOpenInput::with_placeholder("Existing branch", cx)),
                            git_action_busy: false,
                            git_action_request: 0,
                            git_action_status: String::new(),
                            git_loading: false,
                            git_error: None,
                            git_diff: None,
                            git_diff_loading: false,
                            git_diff_request: 0,
                            workspace_generation: 0,
                            editor_epoch: 0,
                            mixed_tabs: WorkbenchTabs::for_workspace(0),
                            mixed_panes: WorkbenchPaneLayout::default(),
                            mixed_layout_restore: None,
                            mixed_layout_read_only: false,
                            mixed_tab_context: None,
                            runtime_bridge,
                            runtime_status,
                            runtime_loading: false,
                            mcp_ui_bridge: None,
                            mcp_ui_window_id: NativeMcpUiBridge::new_window_id(),
                            mcp_ui_status: "Waiting for existing Runtime Host".into(),
                            mcp_ui_ready: false,
                            mcp_host_workspaces: Vec::new(),
                            mcp_host_workspaces_loading: false,
                            mcp_host_workspaces_request: 0,
                            mcp_compare_busy: false,
                            mcp_compare_request: 0,
                            mcp_workspace_create_busy: false,
                            mcp_workspace_create_request: 0,
                            native_settings_store,
                            native_settings,
                            native_settings_error,
                            settings_section: None,
                            settings_modal_focus: cx.focus_handle(),
                            settings_kit_dialog_open: false,
                            last_auto_git_fetch: Instant::now(),
                            agent_profiles: Vec::new(),
                            agent_profiles_loading: false,
                            agent_profiles_request: 0,
                            agent_selected_profile: None,
                            agent_prompt_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("Task for selected Agent…", cx)
                            }),
                            agent_launch_busy: false,
                            agent_launch_request: 0,
                            agent_pending_tab: None,
                            agent_status: "Select an Agent Profile and enter a task".into(),
                            terminals: TerminalDock::for_workspace(0),
                            terminal_panes: TerminalPaneLayout::default(),
                            terminal_input,
                            secondary_terminal_input,
                            terminal_keyboard_focus: cx.focus_handle(),
                            secondary_terminal_keyboard_focus: cx.focus_handle(),
                            terminal_key_queue: VecDeque::new(),
                            terminal_key_queue_target: None,
                            terminal_key_sending: false,
                            terminal_auto_resize: TerminalAutoResizeGate::default(),
                            terminal_line_selection: None,
                            terminal_create_flow: TerminalCreateFlow::default(),
                            terminal_layout_restore: None,
                            terminal_list_loading: false,
                            terminal_read_loading: None,
                            terminal_split_read_loading: false,
                            terminal_poll: TerminalPollGate::default(),
                            terminal_poll_cadence: TerminalPollCadence::default(),
                            terminal_stream: None,
                            terminal_stream_retry: TerminalStreamRetryGate::default(),
                            terminal_stream_view_snapshot: Vec::new(),
                            terminal_tick: 0,
                        };
                        workbench.restore_scratch_tabs(cx);
                        workbench.load_terminal_layout();
                        workbench.load_mixed_pane_layout();
                        workbench.restore_file_tabs_from_pane_layout(cx);
                        workbench
                    });
                    view.update(cx, |view, cx| {
                        view.start_terminal_polling(cx);
                        // Restore saved Host tab previews immediately after
                        // Native window startup, rather than waiting for the
                        // first 1.5s housekeeping session-list tick.
                        // Uses authenticated CLI --no-start; no PTY is minted.
                        view.refresh_terminals(cx);
                        view.refresh_project_tree(cx);
                        view.refresh_worktrees(cx);
                        view.refresh_git(cx);
                        view.refresh_agent_profiles(cx);
                        view.connect_mcp_ui_bridge(cx);
                    });
                    view
                },
            )
            .expect("create native GPUI desktop window");
            cx.activate(true);
        });
}
