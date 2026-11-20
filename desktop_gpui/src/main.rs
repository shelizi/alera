//! Experimental Alera desktop shell powered by native GPUI (no Flutter/WebView).
mod editor_ui;
mod native_action_dialog_ui;
mod native_agent_quota_ui;
mod native_center_git_diff_ui;
mod native_components;
mod native_file_icons;
mod native_git_favorites_ui;
mod native_git_file_compare_ui;
mod native_git_graph_center_ui;
mod native_git_graph_compare_ui;
mod native_git_graph_rows_ui;
mod native_git_graph_ui;
mod native_history_sync_ui;
mod native_host_pins_ui;
mod native_popup_ui;
mod native_project_sidebar_ui;
mod native_review_links;
mod native_settings_catalog_ui;
mod native_settings_ui;
mod native_shared_settings_ui;
mod native_text_references;
mod native_ui_icons;
mod native_ui_theme;
mod quick_open_ui;
mod win_ime;
use native_history_sync_ui::AgyAccountEdit;

// GPUI Kit re-exports its pinned GPUI runtime. Use a single version of all
// GPUI types throughout Native instead of mixing gpui 0.2.x and gpui-pre.
extern crate gpui_kit as gpui;

use alera_core::git::{
    prepare_staged_commit_prompt, verify_staged_commit_prompt, GitStagedCommitPrompt,
};
use alera_gpui_desktop::agent_profiles::{
    matching_agent_handle, AgentProfileContentFields, AgentProfileManagedSelectionFields,
    AgentProfileRemovalImpact, AgentProfileRow, LaunchedAgentTab,
};
use alera_gpui_desktop::agent_quota::{AgentUsageSummary, QuotaSnapshot};
use alera_gpui_desktop::agent_status_notifications::{
    enabled_notice_sessions, validated_notice_pane_target, validated_notice_target, AgentNotice,
    AgentNotificationDelivery, AgentNotificationPolicy,
};
#[cfg(windows)]
use alera_gpui_desktop::agent_windows_notifications::WindowsAgentToastPresenter;
use alera_gpui_desktop::ai_commit_bridge::GeneratedAiCommit;
use alera_gpui_desktop::edition::NATIVE_APP_NAME;
use alera_gpui_desktop::editor::EditorDraftStore;
use alera_gpui_desktop::editor_tabs::{EditorTabs, SaveAllReport, MAX_EDITOR_TABS};
use alera_gpui_desktop::explorer_drag::ExplorerDragBatch;
use alera_gpui_desktop::explorer_git_badges::{ExplorerGitBadge, ExplorerGitBadges};
use alera_gpui_desktop::explorer_ops::{
    affects_open_editor_path, ExplorerClipboardBatch, ExplorerClipboardMode, ExplorerDeleteBatch,
    ExplorerFiles,
};
use alera_gpui_desktop::explorer_selection::{ExplorerMultiSelection, ExplorerNavigation};
use alera_gpui_desktop::explorer_visibility::ExplorerVisibilityOptions;
use alera_gpui_desktop::file_search::WorkspaceFileIndex;
use alera_gpui_desktop::git_actions::{
    GitActions, GitAmendPreview, GitCommitApplyKind, GitCommitApplyPreview, GitCommitRefKind,
    GitConflictChoice, GitConflictPlan, GitRebasePreview, GitSoftResetPreview,
    GitUntrackedTrashCandidate,
};
use alera_gpui_desktop::git_branch_compare::{
    compare_branch_files, compare_branch_trees, diff_compared_file, open_compared_original_snapshot,
    original_compared_file, BranchFileComparison, CompareDirection,
};
use alera_gpui_desktop::git_file_interop::selected_worktree_paths;
use alera_gpui_desktop::git_graph_layout::{layout_git_graph, GraphCommit};
use alera_gpui_desktop::git_graph_render::{PreparedGitGraph, PreparedGitGraphRow};
use alera_gpui_desktop::git_graph_search::{GitGraphRefTab, GitGraphSearch};
use alera_gpui_desktop::git_history::{
    compare_git_refs, filter_git_branch_picker, preview_commit, GitBranchRow, GitHistoryOptions, GitHistorySnapshot, GitMergedBranchVisibility, HISTORY_PAGE_STEP, MAX_BRANCHES,
    MAX_HISTORY_PAGE,
};
use alera_gpui_desktop::git_inspect::{
    git_history_file_filter, read_file_diff, read_file_diff_for_area, read_git_original_for_area, GitChangeArea,
    GitRefreshGate, GitSnapshot,
};
use alera_gpui_desktop::git_inspect::{read_file_diff_page, read_file_diff_page_for_area};
use alera_gpui_desktop::git_original_tabs::GitOriginalTabs;
use alera_gpui_desktop::git_sidebar_layout::{
    layout_git_sidebar, section_actions, section_keys, FolderKey, GitSidebarEntry, GitSidebarGroupMode,
    GitSidebarOptions, GitSidebarSelectionItem, GitSidebarViewMode, SectionKey,
};
use alera_gpui_desktop::git_watch::GitWatchService;
use alera_gpui_desktop::history_sync_bridge::{
    AgyAccountsSnapshot, HistorySyncAccount, HistorySyncMode, HistorySyncSnapshot,
};
use alera_gpui_desktop::host_tab_pins::{
    host_pin_owner_coverage, host_pin_query_workspaces, HostPinOwnerCoverage, HostTabPinSnapshot,
};
use alera_gpui_desktop::local_agent_detection::{
    classify_agent_profile_launch_failure, detect_installed_agent_clis, InstalledAgentCli,
};
use alera_gpui_desktop::managed_choice_discovery::{
    discover_choices, discovery_arguments, ManagedChoiceKind,
};
use alera_gpui_desktop::mcp_ui_bridge::{
    matching_host_tab_handle, native_mcp_file_open_requires_new, NativeMcpUiBridge,
    NativeMcpUiCommand, NativeMcpUiEvent,
};
use alera_gpui_desktop::mcp_workspace_creation::HostManagedWorkspaceRequest;
use alera_gpui_desktop::model::WorkbenchModel;
use alera_gpui_desktop::native_i18n::{tr, UiLocale};
use alera_gpui_desktop::agent_status_visual::{AgentStateSummary, AgentVisualState};
use alera_gpui_desktop::native_modal::NativeModalSize;
use alera_gpui_desktop::native_settings::{NativeSettings, NativeSettingsStore, NativeSidebarLayout};
use alera_gpui_desktop::project_tree::{
    collect_project_tree_with_registered, project_group_matches_search,
    project_title_matches_search, sidebar_workspaces_for_group, worktree_matches_search,
    ProjectTreeGroup,
};
use alera_gpui_desktop::runtime_bridge::{
    AgentHookSettingsSnapshot, HostProjectConfigOrigin, ProjectRemovalPreview, RuntimeCliBridge,
};
use alera_gpui_desktop::runtime_catalog::RuntimeCatalog;
use alera_gpui_desktop::session::{
    PendingHostCreate, PreviewSessionStore, TerminalLayout, TerminalLayoutTab,
};
use alera_gpui_desktop::terminal::{
    encode_terminal_key, estimate_terminal_viewport_with_line_height, raw_key_reply_may_drain,
    resolve_registered_host_workspace_path, select_registered_host_workspace,
    terminal_clipboard_payload, terminal_ime_anchor_with_line_height, terminal_key_dispatch,
    TerminalAutoResizeGate, TerminalClipboardShortcut, TerminalCreateFlow,
    TerminalCreateTick, TerminalDock, TerminalKeyDispatch, TerminalLineSelection,
    TerminalPollCadence, TerminalPollGate, TerminalSession, TerminalTabContextTarget,
    TerminalViewMode, TerminalViewportProposal, HOST_SESSION_LIST_SCOPE,
};
use alera_gpui_desktop::terminal_panes::{TerminalPaneLayout, TerminalSplitDirection};
use alera_gpui_desktop::terminal_stream_bridge::{
    NativeTerminalStreamBridge, StreamViewUpdate, TerminalStreamRetryGate,
};
use alera_gpui_desktop::visual_tokens::{
    ACTIVE, AGENT_BLOCKED, AGENT_DONE, AGENT_WAITING, BG, BORDER, BORDER_ACTIVE,
    clamp_context_sidebar_width, clamp_projects_sidebar_width, ELEVATED, MUTED, PANEL,
    PROJECTS_COLLAPSED_WIDTH, RESIZE_CHROME,
    SECTION_HEADER_HEIGHT, SIDEBAR_RESIZE_HANDLE_WIDTH,
    STATUS_BAR_HEIGHT, TAB_STRIP_HEIGHT, TAB_STRIP_LEADING_PADDING, TAB_ATTENTION_DOT_SIZE, TAB_CHIP_HORIZONTAL_PADDING, TEXT, TOOLBAR, TOP_BAR_HEIGHT,
    EDITOR_TOOLBAR_HEIGHT, usable_workbench_height, secondary_terminal_render_height,
    primary_inner_terminal_render_size, inner_secondary_terminal_render_height,
    TERMINAL_INNER_SPLIT_HEADER_HEIGHT,
};
use alera_gpui_desktop::vt_screen::{StyledRow, TERMINAL_DEFAULT_BG};
use alera_gpui_desktop::workbench_layout_store::{
    scratch_layout_key, SavedWorkbenchAxis, SavedWorkbenchHostTab, SavedWorkbenchLayoutV1,
    SavedWorkbenchPane, SCRATCH_EDITOR_KEY_PREFIX, WORKBENCH_LAYOUT_VERSION,
};
use alera_gpui_desktop::workbench_panes::{
    primary_pinned_row_visible, PaneAttachGuard, PaneAxis, PaneSlot, WorkbenchPaneLayout,
};
use alera_gpui_desktop::workbench_tabs::{
    WorkbenchTab, WorkbenchTabIdentity, WorkbenchTabKind, WorkbenchTabs,
};
use alera_gpui_desktop::workspace_sidebar_order::order_worktree_rows;
use alera_gpui_desktop::workspace_tree::WorkspaceTree;
use alera_gpui_desktop::worktree_content_search::{
    search_worktree_contents, worktree_hit_requires_switch, WorktreeContentMatch,
    WorktreeContentScope,
};
use alera_gpui_desktop::worktree_manager::{
    matching_branch_worktree, WorktreeEntry, WorktreeManager,
};
use editor_ui::{bind_editor_keys, NativeEditor};
use gpui::{
    actions, div, prelude::*, px, rgb, size, App, Bounds, Context, Entity, Focusable, KeyBinding,
    PathPromptOptions, ScrollHandle, UniformListScrollHandle, Window, WindowBounds, WindowOptions,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::IconName;
use gpui_kit::component::WindowExt as _;
use native_components::{icon, icon_button, UiIcon};
use native_review_links::repository_pull_requests_url;
use native_text_references::{identifier_at_cursor, is_whole_identifier_hit};
use native_ui_icons::{explorer_file_icon_element, sidebar_icon_button, tab_close_button};
use quick_open_ui::{bind_quick_open_keys, QuickOpenInput};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// Share the exact 16px Classic brand mark across native re-renders without
/// allocating or decoding the bundled PNG anew on every workbench update.
fn classic_brand_logo() -> Arc<gpui::Image> {
    static LOGO: OnceLock<Arc<gpui::Image>> = OnceLock::new();
    LOGO.get_or_init(|| {
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            include_bytes!("../../assets/logo/alera-logo-white.png").to_vec(),
        ))
    })
    .clone()
}

actions!(
    alera_gpui_workbench,
    [
        ToggleFileSearch,
        ToggleContentSearch,
        RunContentSearch,
        FindTextReferences,
        ExplorerCopy,
        ExplorerCut,
        ExplorerPaste,
        GitCopyFiles,
        GitCutFiles,
        GitPasteFiles,
        ExplorerSelectAll,
        ExplorerRename,
        ExplorerDelete,
        ExplorerRefresh,
        ExplorerUp,
        ExplorerDown,
        ExplorerFirst,
        ExplorerLast,
        ExplorerOpen,
        ExplorerPreview,
        ExplorerNewFile,
        ExplorerNewFolder
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
    DiscardTracked(Vec<PathBuf>),
    TrashUntracked(Vec<GitUntrackedTrashCandidate>),
    Commit(String),
    Amend(GitAmendPreview),
    SwitchBranch(String),
    MergeWithoutConflicts(String),
    Fetch,
    RenameInactiveBranch(String, String),
    RenameOwnedBranch(String, String),
    DeleteMergedBranch(String),
    CreateCommitRef(GitCommitRefKind, String, String),
    ApplyCommitPreview(GitCommitApplyPreview),
    ApplySoftReset(GitSoftResetPreview),
    ApplyMixedReset(GitSoftResetPreview),
    ApplyRebase(GitRebasePreview),
    CreateWorktreeAtCommit(String, String),
    PullSelected(String),
    PushSelected(String),
    StashSave,
    StashApply,
    PullFfOnly,
    PushUpstream,
    MergeWithChoices(GitConflictPlan, Vec<(String, GitConflictChoice)>),
}

#[cfg(test)]
mod g01_source_control_action_tests {
    use super::GitUiAction;
    use std::path::PathBuf;

    #[test]
    fn stage_and_unstage_are_index_only_but_discard_checkout_and_merge_are_not() {
        assert!(GitUiAction::Stage(PathBuf::from("a.txt")).is_index_only());
        assert!(GitUiAction::Unstage(PathBuf::from("a.txt")).is_index_only());
        assert!(GitUiAction::StageMany(vec![PathBuf::from("a.txt")]).is_index_only());
        assert!(GitUiAction::UnstageMany(vec![PathBuf::from("a.txt")]).is_index_only());
        assert!(!GitUiAction::DiscardTracked(vec![PathBuf::from("a.txt")]).is_index_only());
        assert!(!GitUiAction::SwitchBranch("feature".into()).is_index_only());
        assert!(!GitUiAction::MergeWithoutConflicts("feature".into()).is_index_only());
        assert!(!GitUiAction::Commit("message".into()).is_index_only());
    }
}
#[cfg(test)]
mod g05_branch_remote_ui_tests {
    use super::GitUiAction;

    #[test]
    fn selected_branch_remote_actions_do_not_require_saved_editor_buffers() {
        assert!(GitUiAction::Fetch.is_worktree_independent());
        assert!(GitUiAction::PullSelected("topic".into()).is_worktree_independent());
        assert!(GitUiAction::PushSelected("topic".into()).is_worktree_independent());
        assert!(GitUiAction::PushUpstream.is_worktree_independent());
        assert!(GitUiAction::Stage("a.txt".into()).is_worktree_independent());
        assert!(GitUiAction::Unstage("a.txt".into()).is_worktree_independent());
        assert!(!GitUiAction::MergeWithoutConflicts("topic".into()).is_worktree_independent());
        assert!(!GitUiAction::SwitchBranch("topic".into()).is_worktree_independent());
        assert!(!GitUiAction::PullFfOnly.is_worktree_independent());
        assert!(!GitUiAction::StashSave.is_worktree_independent());
    }
}
#[test]
fn g06_create_commit_worktree_may_proceed_with_unsaved_editor_but_merge_cannot() {
    let creation = GitUiAction::CreateWorktreeAtCommit("a".repeat(40), "feat/g06-checkout".into());
    assert!(creation.is_worktree_independent());
    assert!(!GitUiAction::SwitchBranch("feat/g06-checkout".into()).is_worktree_independent());
    assert!(
        !GitUiAction::MergeWithoutConflicts("feat/g06-checkout".into()).is_worktree_independent()
    );
}
impl GitUiAction {
    /// Index-only Git actions may proceed with a dirty Native Editor.
    /// They stage saved disk bytes, never the unsaved editor buffer.
    fn is_index_only(&self) -> bool {
        matches!(
            self,
            Self::Stage(_) | Self::Unstage(_) | Self::StageMany(_) | Self::UnstageMany(_)
        )
    }

    /// These actions leave the currently checked-out Workspace/Editor alone:
    /// Fetch updates remote refs, selected Pull changes an inactive branch,
    /// Push transfers committed objects, and commit-based Worktree creation
    /// checks out into an entirely separate managed directory.
    fn is_worktree_independent(&self) -> bool {
        self.is_index_only()
            || matches!(
                self,
                Self::Fetch
                    | Self::PullSelected(_)
                    | Self::PushSelected(_)
                    | Self::PushUpstream
                    | Self::CreateWorktreeAtCommit(_, _)
            )
    }
}

/// Clear only a successfully submitted Commit or Amend message; keep edits
/// typed while the background Git operation was running.
fn should_clear_git_commit_input(submitted: Option<&str>, current: &str) -> bool {
    submitted.is_some_and(|message| current.trim() == message)
}

#[cfg(test)]
mod g03_commit_input_tests {
    use super::should_clear_git_commit_input;

    #[test]
    fn commit_success_clears_only_the_unmodified_submitted_message() {
        assert!(should_clear_git_commit_input(
            Some("create project"),
            " create project "
        ));
        assert!(!should_clear_git_commit_input(
            Some("create project"),
            "new pending message"
        ));
        assert!(!should_clear_git_commit_input(None, "create project"));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfirmGitAction {
    StashSave,
    StashApply,
    PullFfOnly,
    PushUpstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeResetMode {
    Soft,
    Mixed,
}

impl NativeResetMode {
    fn label(self) -> &'static str {
        match self {
            Self::Soft => "Soft",
            Self::Mixed => "Mixed",
        }
    }
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
}

/// Explicit, inert selectors for Classic's History Tab branch perspective.
/// A ref is accepted only when it still exists in the current Git snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
enum GitGraphCenterPerspective {
    Current,
    AllBranches,
    Favorites,
    Branch { name: String, remote: bool },
}

/// Inert search selector: typed text only filters enumerated Git refs. It is
/// never interpreted as a Git command, revspec, checkout or branch writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitGraphBranchPickerMode {
    Perspective,
    MergeTarget,
    Compare,
}

/// Shared actions for branch context menus in the Native Git Graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitGraphBranchMenuAction {
    History,
    Compare,
    CompareReverse,
    PreviewRebase,
    PrepareRename,
    DeleteMerged,
    PullBranch,
    PushBranch,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitChangeMenuAction {
    Preview,
    History,
    CompareLatestRevision,
    CompareWithBranch,
    AddToGitIgnore,
    Original,
    OpenWorkingFile,
    OpenExternal,
    CopyFiles,
    CutFiles,
    PasteFiles,
    RevealFile,
    Stage,
    Unstage,
    Discard,
    TrashUntracked,
    CopyPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitFolderMenuAction {
    Copy,
    Cut,
    Paste,
    Reveal,
    CopyPath,
}

#[derive(Clone, Copy)]
enum MixedContextAction {
    Focus,
    TogglePin,
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
    OpenDefaultApplication,
    RevealInFileManager,
    ViewCurrentGitDiff,
    CopyAbsolutePath,
    CopyRelativePath,
    CollapseFolder,
    Refresh,
    GitHistory,
    CompareLatestRevision,
    CompareWithBranch,
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
            Self::OpenDefaultApplication => "Open with Default Application",
            Self::RevealInFileManager => "Show in file manager",
            Self::ViewCurrentGitDiff => "View current Git changes",
            Self::CopyAbsolutePath => "Copy path",
            Self::CopyRelativePath => "Copy relative path",
            Self::CollapseFolder => "Collapse folder",
            Self::Refresh => "Refresh",
            Self::GitHistory => "File / folder Git history",
            Self::CompareLatestRevision => "Compare With Latest Git Revision",
            Self::CompareWithBranch => "Compare With Branch...",
        }
    }
}

#[derive(Clone, Debug)]
struct ExplorerContextMenu {
    /// None means the user right-clicked empty space, not a stale selection.
    path: Option<PathBuf>,
}

/// Correlate a pending Host create RPC with the pane that initiated it.
/// The Host always owns the PTY; this only controls Native preview placement.
#[derive(Clone, Debug)]
struct PendingTerminalPane {
    request_id: u64,
    target: PaneAttachGuard,
    expected_agent_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RightSidebarView {
    Explorer,
    Search,
    References,
    Git,
    PullRequests,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidebarResizeSide {
    Projects,
    Context,
}

#[derive(Clone, Copy, Debug)]
struct SidebarResizeDrag {
    side: SidebarResizeSide,
    start_x: f32,
    start_width: f32,
}

/// File-patch-only navigation. History/branch compare previews must never
/// inherit a file's paging cursor, and pages are replaced, not concatenated.
#[derive(Clone)]
struct GitFileDiffPaging {
    relative: PathBuf,
    area: Option<GitChangeArea>,
    /// Stable for this exact file AND status area, even as each request increments
    /// git_diff_request. Re-selecting the same filename gets a fresh identity.
    source_ticket: u64,
    start_line: usize,
    next_line: Option<usize>,
    previous_starts: Vec<usize>,
}

/// Editable Worktree copy row owned by the explicit Project Settings form.
/// Input contents remain private to this entity and are dropped on Cancel,
/// confirmed Save or Workspace switch, never shown in Settings search results.
struct ProjectCopyRowInputs {
    from: Entity<QuickOpenInput>,
    to: Entity<QuickOpenInput>,
    overwrite: bool,
}

struct Workbench {
    model: WorkbenchModel,
    center_view: CenterView,
    right_sidebar_view: RightSidebarView,
    projects_sidebar_collapsed: bool,
    projects_sidebar_width: f32,
    right_sidebar_visible: bool,
    right_sidebar_width: f32,
    sidebar_resize_drag: Option<SidebarResizeDrag>,
    sidebar_save_busy: bool,
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
    /// Persistent GPUI viewport handle; keyboard movement reveals its real
    /// variable-height row in the current filtered/tree/flat presentation.
    explorer_scroll: ScrollHandle,
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
    runtime_catalog: Option<RuntimeCatalog>,
    runtime_catalog_error: Option<String>,
    project_settings_project_id: Option<String>,
    /// Explicit Settings-only selection; never changes the active workspace.
    project_settings_selected_project_id: Option<String>,
    project_settings_rows: Vec<(String, String)>,
    project_settings_loading: bool,
    project_settings_error: Option<String>,
    project_settings_request: u64,
    project_settings_config: Option<serde_json::Value>,
    project_settings_source: Option<HostProjectConfigOrigin>,
    project_settings_repo_error: bool,
    project_settings_use_repo_confirmed: bool,
    project_settings_editing: bool,
    project_settings_draft_provider: Option<String>,
    project_settings_prompt_input: Entity<QuickOpenInput>,
    project_settings_copy_inputs: Vec<ProjectCopyRowInputs>,
    project_settings_setup_inputs: Vec<Entity<QuickOpenInput>>,
    project_settings_save_busy: bool,
    project_settings_save_status: String,
    project_settings_mutation_request: u64,
    sidebar_pending_action: Option<String>,
    sidebar_rename_target: Option<(String, String)>,
    sidebar_rename_input: Option<Entity<QuickOpenInput>>,
    sidebar_selected_parent: Option<(String, String)>,
    sidebar_project_remove_pending: Option<(String, ProjectRemovalPreview)>,
    sidebar_create_target: Option<(String, String)>,
    sidebar_create_input: Option<Entity<QuickOpenInput>>,
    collapsed_project_roots: HashSet<PathBuf>,
    project_tree_loading: bool,
    project_tree_request: u64,
    show_recent_workspaces: bool,
    sidebar_search_input: Entity<QuickOpenInput>,
    worktrees: Vec<WorktreeEntry>,
    worktree_error: Option<String>,
    worktree_loading: bool,
    worktree_busy: bool,
    worktree_create_input: Option<Entity<QuickOpenInput>>,
    worktree_create_root: Option<PathBuf>,
    worktree_create_from_existing: bool,
    worktree_delete_pending: Option<PathBuf>,
    picking_workspace: bool,
    quick_open: Option<Entity<QuickOpenInput>>,
    quick_open_selected_index: usize,
    quick_open_selection_query: String,
    quick_open_results_scroll: ScrollHandle,
    explorer_filter_input: Entity<QuickOpenInput>,
    file_index: Option<Arc<WorkspaceFileIndex>>,
    file_index_loading: bool,
    file_index_request: u64,
    content_query: Option<Entity<QuickOpenInput>>,
    content_search_scope: WorktreeContentScope,
    content_worktrees_searched: usize,
    content_worktrees_skipped: usize,
    content_loading: bool,
    content_matches: Vec<WorktreeContentMatch>,
    content_truncated: bool,
    content_error: Option<String>,
    content_searched_query: String,
    content_request: u64,
    /// Textual identifier occurrences, intentionally NOT LSP semantic references.
    references_symbol: Option<String>,
    references_matches: Vec<WorktreeContentMatch>,
    references_loading: bool,
    references_error: Option<String>,
    references_truncated: bool,
    references_request: u64,
    review_browser_url: Option<String>,
    review_browser_error: Option<String>,
    git_snapshot: Option<GitSnapshot>,
    git_original_tabs: GitOriginalTabs,
    git_original_active: bool,
    git_original_request: u64,
    git_selected_changes: HashSet<alera_gpui_desktop::git_inspect::GitChangeRow>,
    git_selected_folders: HashSet<FolderKey>,
    git_keyboard_focus: gpui::FocusHandle,
    git_selection_anchor: Option<GitSidebarSelectionItem>,
    git_discard_armed: Option<Vec<PathBuf>>,
    git_trash_preview: Option<Vec<GitUntrackedTrashCandidate>>,
    git_trash_loading: bool,
    git_trash_request: u64,
    git_sidebar_options: GitSidebarOptions,
    /// Classic shows changed files first; side patch text is an optional detail.
    git_sidebar_patch_expanded: bool,
    /// Keep infrequent Git/remote operations out of the primary Source Control workflow.
    git_advanced_actions_open: bool,
    git_history: Option<GitHistorySnapshot>,
    /// Cached bounded topology and refs for the central Graph viewport.
    git_graph_render_cache: Option<native_git_graph_rows_ui::GitGraphViewCache>,
    git_center_graph_scroll: UniformListScrollHandle,
    git_center_graph_filter: String,
    git_history_open: bool,
    /// Sidebar Graph visibility is independent of the mixed Git History tab.
    git_history_sidebar_open: bool,
    /// Temporary full-height Graph projection; mixed Terminal/Editor tabs stay untouched.
    /// Whether the real, workspace-local mixed-pane Git History tab exists.
    git_history_tab_open: bool,
    /// Whether the Git History tab currently has foreground focus.
    git_graph_center_open: bool,
    git_graph_ref_tab: GitGraphRefTab,
    git_history_options: GitHistoryOptions,
    git_favorites_busy: bool,
    git_favorites_request: u64,
    git_favorites_last_refresh: Instant,
    git_history_loading: bool,
    git_history_watch_pending: bool,
    git_history_error: Option<String>,
    git_history_request: u64,
    /// Cancels the previous background Graph scan when a newer view wins.
    git_history_cancel: Arc<AtomicBool>,
    git_merge_armed: Option<String>,
    git_graph_remote_armed: Option<(String, bool)>, // true = Pull, false = Push
    git_branch_delete_armed: Option<String>,
    git_commit_ref_target: Option<(String, GitCommitRefKind)>,
    git_commit_ref_armed: Option<(String, GitCommitRefKind, String)>,
    git_commit_apply_preview: Option<GitCommitApplyPreview>,
    git_commit_apply_armed: bool,
    git_commit_apply_loading: bool,
    git_commit_apply_request: u64,
    git_soft_reset_preview: Option<GitSoftResetPreview>,
    git_soft_reset_armed: bool,
    git_soft_reset_mode: NativeResetMode,
    git_soft_reset_loading: bool,
    git_soft_reset_request: u64,
    git_rebase_preview: Option<GitRebasePreview>,
    git_rebase_armed: bool,
    git_rebase_loading: bool,
    git_rebase_request: u64,
    git_worktree_commit_target: Option<String>,
    git_worktree_commit_armed: Option<(String, String)>,
    git_worktree_commit_input: Entity<QuickOpenInput>,
    git_rename_source: Option<String>,
    git_rename_armed: Option<(String, String)>,
    git_sensitive_armed: Option<ConfirmGitAction>,
    git_conflict_preview: Option<GitConflictPlan>,
    git_conflict_choices: HashMap<String, GitConflictChoice>,
    git_conflict_selected: Option<String>,
    git_conflict_loading: bool,
    git_conflict_request: u64,
    git_conflict_confirm_armed: bool,
    git_filter_input: Entity<QuickOpenInput>,
    git_graph_search_input: Entity<QuickOpenInput>,
    git_branch_picker_input: Entity<QuickOpenInput>,
    git_branch_picker_mode: Option<GitGraphBranchPickerMode>,
    git_commit_input: Entity<QuickOpenInput>,
    git_commit_draft_loading: bool,
    git_commit_draft_request: u64,
    git_ai_prompt_preview: Option<GitStagedCommitPrompt>,
    git_ai_prompt_loading: bool,
    git_ai_prompt_request: u64,
    git_ai_generate_loading: bool,
    git_ai_generate_request: u64,
    git_ai_generate_cancel: Arc<AtomicBool>,
    git_amend_preview: Option<GitAmendPreview>,
    git_amend_loading: bool,
    git_amend_request: u64,
    git_branch_input: Entity<QuickOpenInput>,
    git_rename_input: Entity<QuickOpenInput>,
    git_commit_ref_input: Entity<QuickOpenInput>,
    git_action_busy: bool,
    git_action_request: u64,
    git_action_status: String,
    git_loading: bool,
    git_refresh_gate: GitRefreshGate,
    git_watch_service: Option<GitWatchService>,
    git_error: Option<String>,
    git_diff: Option<String>,
    git_branch_comparison: Option<BranchFileComparison>,
    git_branch_compare_loading: bool,
    /// Central Graph compares immutable branch tips; sidebar preserves merge-base mode.
    git_graph_compare_center: bool,
    git_graph_compare_selected: Option<usize>,
    git_graph_compare_rows: Arc<Vec<native_git_graph_compare_ui::CenterBranchPatchRow>>,
    git_graph_compare_hunk: Option<usize>,
    git_graph_compare_scroll: UniformListScrollHandle,
    git_diff_loading: bool,
    git_diff_request: u64,
    git_file_diff_paging: Option<GitFileDiffPaging>,
    git_file_compare_ticket: u64,
    git_file_compare_result: Option<(u64, Result<alera_gpui_desktop::git_file_revision_compare::GitFileRevisionComparison, String>)>,
    git_file_compare_split: bool,
    git_file_compare_rows: Arc<Vec<alera_gpui_desktop::git_file_split_diff::GitSplitDiffRow>>,
    git_file_compare_additions: usize,
    git_file_compare_deletions: usize,
    git_file_compare_clipped: bool,
    /// A temporary, read-only center projection; never owns or replaces an
    /// Editor buffer, mixed tab or Host Terminal session.
    center_git_diff_source: Option<native_action_dialog_ui::GitDialogFileSource>,
    center_git_diff_scroll: UniformListScrollHandle,
    center_git_diff_hunk: Option<usize>,
    /// Parsed once on each bounded Git page response, shared with visible
    /// virtual-list renders while Terminal/Host UI events keep arriving.
    center_git_diff_rows: Arc<Vec<native_center_git_diff_ui::CenterGitDiffRow>>,
    workspace_generation: u64,
    editor_epoch: u64,
    mixed_tabs: WorkbenchTabs,
    host_pin_snapshots: HashMap<(String, String), HostTabPinSnapshot>,
    host_pins_loading: bool,
    host_pins_busy: bool,
    host_pins_request: u64,
    host_pins_error: Option<String>,
    mixed_panes: WorkbenchPaneLayout,
    mixed_layout_restore: Option<SavedWorkbenchLayoutV1>,
    mixed_layout_read_only: bool,
    mixed_tab_context: Option<WorkbenchTabIdentity>,
    runtime_bridge: Option<RuntimeCliBridge>,
    runtime_status: String,
    runtime_loading: bool,
    quota_snapshots: Vec<QuotaSnapshot>,
    quota_loading: bool,
    quota_error: Option<String>,
    quota_last_refresh: Instant,
    quota_request: u64,
    quota_enabled_order: Option<Vec<String>>,
    quota_popover_open: bool,
    quota_popover_account: Option<String>,
    quota_usage_open: bool,
    quota_usage_days: u8,
    quota_usage_loading: bool,
    quota_usage_error: Option<String>,
    quota_usage_summary: Option<AgentUsageSummary>,
    quota_usage_request: u64,
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
    native_scrollback_busy: bool,
    native_scrollback_status: String,
    history_sync_snapshot: Option<HistorySyncSnapshot>,
    history_sync_loading: bool,
    history_sync_busy: bool,
    history_sync_request: u64,
    history_sync_status: String,
    history_sync_edit: Option<HistorySyncAccount>,
    history_sync_input: Entity<QuickOpenInput>,
    agy_accounts_snapshot: Option<AgyAccountsSnapshot>,
    agy_account_edit: Option<AgyAccountEdit>,
    agy_account_remove_confirm: Option<String>,
    agy_account_id_input: Entity<QuickOpenInput>,
    agy_account_name_input: Entity<QuickOpenInput>,
    settings_section: Option<String>,
    settings_modal_focus: gpui::FocusHandle,
    settings_kit_dialog_open: bool,
    /// The preview language changes only visible text, never host settings.
    settings_menu_locale: UiLocale,
    settings_menu_language: String,
    /// Validated read-only model projection from Rust Host. No local JSON.
    settings_extra_values: serde_json::Map<String, serde_json::Value>,
    settings_shared_busy: bool,
    settings_native_profile_migrated: bool,
    settings_shared_request: u64,
    settings_shared_last_refresh: Instant,
    settings_shared_error: Option<String>,
    settings_menu_search: Entity<QuickOpenInput>,
    settings_shortcut_search: Entity<QuickOpenInput>,
    settings_exclusion_input: Entity<QuickOpenInput>,
    last_auto_git_fetch: Instant,
    agent_profiles: Vec<AgentProfileRow>,
    /// Local CLI availability is separate from the Host Agent Profile catalog.
    installed_agent_clis: Vec<InstalledAgentCli>,
    installed_agent_clis_loading: bool,
    installed_agent_clis_request: u64,
    agent_profiles_loading: bool,
    agent_profiles_request: u64,
    agent_selected_profile: Option<String>,
    agent_profile_name_input: Entity<QuickOpenInput>,
    agent_profile_description_input: Entity<QuickOpenInput>,
    agent_profile_quota_group_input: Entity<QuickOpenInput>,
    /// Loaded Host identity/revision; no routing values in shared settings.
    agent_profile_routing_loaded: Option<(String, i64)>,
    /// Temporary edit-only values; never persisted in shared desktop settings.
    agent_profile_content_loaded: Option<AgentProfileContentFields>,
    /// Transient Host-owned Model/Persona selection; no other Managed config.
    agent_profile_managed_loaded: Option<AgentProfileManagedSelectionFields>,
    agent_profile_model_input: Entity<QuickOpenInput>,
    agent_profile_persona_input: Entity<QuickOpenInput>,
    agent_profile_discovered_models: Vec<String>,
    agent_profile_discovered_personas: Vec<String>,
    agent_profile_discovery_busy: bool,
    agent_profile_discovery_request: u64,
    agent_profile_discovery_status: String,
    agent_profile_command_input: Entity<QuickOpenInput>,
    agent_profile_custom_prompt_input: Entity<QuickOpenInput>,
    agent_profile_new_adapter: String,
    agent_profile_mutation_busy: bool,
    agent_profile_mutation_request: u64,
    agent_profile_mutation_status: String,
    agent_profile_removal_preview: Option<AgentProfileRemovalImpact>,
    agent_prompt_input: Entity<QuickOpenInput>,
    agent_override_selected_id: Option<String>,
    agent_override_input: Entity<QuickOpenInput>,
    agent_override_busy: bool,
    agent_override_request: u64,
    agent_override_status: String,
    agent_launch_busy: bool,
    agent_launch_request: u64,
    agent_pending_tab: Option<LaunchedAgentTab>,
    /// Keep + → Agent's original pane across prompt entry and Host launch
    /// confirmation; never attach to whichever tab happens to be focused.
    agent_launch_pane: Option<PaneAttachGuard>,
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
    terminal_create_journal: Option<PendingHostCreate>,
    terminal_create_inventory_verified: bool,
    terminal_create_pane: Option<PendingTerminalPane>,
    terminal_layout_restore: Option<TerminalLayout>,
    terminal_list_loading: bool,
    /// Prevent an async inventory started before a status push from
    /// resurrecting stale Agent Hook state after the push arrives.
    agent_presence_event_generation: u64,
    /// Host events and inventories share one delivery rule engine. No OS
    /// toast is emitted until explicit Native-only ownership is confirmed.
    agent_notification_delivery: AgentNotificationDelivery,
    agent_notification_clock: Instant,
    /// Separate Host-owned operational Hook settings; unlike the portable
    /// notification toggle, these live in runtimeSettings.agentStatusHooks.
    agent_hook_switches: Option<AgentHookSettingsSnapshot>,
    agent_hook_write_loading: bool,
    agent_hook_ui_status: String,
    agent_hook_switches_loading: bool,
    agent_hook_switches_request: u64,
    agent_hook_switches_last_refresh: Instant,
    #[cfg(windows)]
    agent_windows_toast: WindowsAgentToastPresenter,
    agent_toast_activation: Option<AgentNotice>,
    agent_toast_activation_retries: u8,
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

    /// A user-selected primary Editor/Explorer tab replaces the transient
    /// center Git projection. The read-only Original tab and dirty documents
    /// retain ownership of their data; secondary tabs keep the primary view.
    fn release_primary_center_overlays(&mut self, pane: PaneSlot) {
        native_center_git_diff_ui::release_primary_center_overlays_for_navigation(
            pane,
            &mut self.center_git_diff_source,
            &mut self.center_git_diff_hunk,
            &mut self.git_original_active,
        );
    }

    fn click_explorer_file(
        &mut self,
        path: &Path,
        permanent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.explorer_context_menu = None;
        self.git_original_active = false;
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
        } else if let Some((pane, instance)) = self.editor_instance_for_path(path, cx) {
            // A file already owned by a live editor buffer is reused, never
            // shadowed by a second read-only preview chip for the same path.
            self.focus_editor_instance(pane, instance, cx);
            self.release_primary_center_overlays(pane);
            self.explorer_preview_active = false;
            self.model
                .set_status("Focused existing editor buffer; no duplicate opened");
        } else if self.model.preview_path(path) {
            self.release_primary_center_overlays(PaneSlot::Primary);
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
        let bridge = self.runtime_bridge.clone();
        let generation = self.workspace_generation;
        self.project_tree_request = self.project_tree_request.wrapping_add(1);
        let request = self.project_tree_request;
        self.project_tree_loading = true;
        cx.spawn(async move |view, cx| {
            let (groups, runtime_result) = cx
                .background_spawn(async move {
                    let runtime_result = bridge.map(|bridge| bridge.list_runtime_catalog());
                    let registered = runtime_result
                        .as_ref()
                        .and_then(|result| result.as_ref().ok())
                        .map(|catalog| {
                            catalog
                                .projects
                                .iter()
                                .map(|project| project.root.clone())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let groups =
                        collect_project_tree_with_registered(&saved, &recent, &registered, &active);
                    (groups, runtime_result)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.project_tree_request != request {
                    return;
                }
                view.project_tree_groups = groups;
                match runtime_result {
                    Some(Ok(catalog)) => {
                        view.runtime_catalog = Some(catalog);
                        view.runtime_catalog_error = None;
                    }
                    Some(Err(error)) => {
                        view.runtime_catalog = None;
                        view.runtime_catalog_error = Some(error.to_string());
                    }
                    None => {
                        view.runtime_catalog = None;
                        view.runtime_catalog_error = Some("Classic Alera CLI unavailable".into());
                    }
                }
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
        // Explorer preferences are shared with Classic. Never optimistically
        // mutate the tree or write Native JSON before Host CAS acknowledges.
        if previous.show_hidden != next.show_hidden && previous.show_ignored != next.show_ignored {
            self.model
                .set_status(tr(self.settings_menu_locale, "Change one Explorer visibility option at a time"));
            cx.notify();
            return;
        }
        let (path, value) = if previous.show_hidden != next.show_hidden {
            ("explorer.showHiddenFiles", next.show_hidden)
        } else {
            ("explorer.showIgnoredFiles", next.show_ignored)
        };
        self.change_shared_desktop_setting(path, serde_json::json!(value), cx);
    }

    fn set_explorer_clipboard(&mut self, mode: ExplorerClipboardMode, cx: &mut Context<Self>) {
        let Some(selected) = self.explorer_selection.clone() else {
            self.model
                .set_status(tr(self.settings_menu_locale, "Select a file or folder to Copy or Cut"));
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
                let source = if mode == ExplorerClipboardMode::Copy {
                    "Copied {count} Explorer item(s); choose destination and Paste"
                } else {
                    "Cut {count} Explorer item(s); choose destination and Paste"
                };
                self.model.set_status(
                    tr(self.settings_menu_locale, source).replace("{count}", &count.to_string()),
                );
            }
            Err(error) => self
                .model
                .set_status(tr(self.settings_menu_locale, "Explorer Copy/Cut refused: {error}").replace("{error}", &error.to_string())),
        }
        cx.notify();
    }

    fn paste_explorer_clipboard(&mut self, cx: &mut Context<Self>) {
        let root = self.model.root().to_path_buf();
        let folder = self
            .explorer_selection
            .as_ref()
            .map(|path| if path.is_dir() { path.clone() } else {
                path.parent().unwrap_or(&root).to_path_buf()
            })
            .unwrap_or(root);
        self.paste_shared_file_clipboard(folder, cx);
    }

    fn paste_git_file_clipboard(&mut self, hovered: Option<PathBuf>, cx: &mut Context<Self>) {
        // The workspace root is a valid destination, but not a selectable
        // Explorer row; native and in-app paste both represent it as None.
        let hovered = hovered.filter(|path| path != self.model.root());
        // Windows Explorer exposes a CF_HDROP file list. Distinguish a newly
        // copied external selection from Alera's own guarded clipboard and
        // import it as COPY. OS Cut markers are not trusted to delete outside
        // files on an inbound paste.
        let mut outside_paths = cx.read_from_clipboard().into_iter()
            .flat_map(|item| item.into_entries())
            .filter_map(|entry| match entry {
                gpui::ClipboardEntry::ExternalPaths(paths) => Some(paths.paths().to_vec()),
                _ => None,
            })
            .flatten().collect::<Vec<_>>();
        if !outside_paths.is_empty() {
            outside_paths.sort();
            outside_paths.dedup();
            let is_ours = self.explorer_clipboard.as_ref()
                .is_some_and(|clip| clip.source_paths() == outside_paths);
            if !is_ours {
                self.import_explorer_drop(outside_paths, hovered, cx);
                return;
            }
        }
        let result = ExplorerFiles::new(self.model.root())
            .and_then(|files| {
                let row = hovered.as_deref().filter(|row| *row != files.root());
                files.external_drop_folder(row)
            });
        match result {
            Ok(folder) => self.paste_shared_file_clipboard(folder, cx),
            Err(error) => {
                self.model.set_status(format!("Git Paste refused: {error}"));
                cx.notify();
            }
        }
    }

    fn paste_shared_file_clipboard(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        // Do not block ordinary Copy because an unrelated Editor is dirty.
        // Cut is checked against the exact open files at paste time, so an
        // existing Editor tab cannot silently keep a stale moved path.
        let open_paths: Vec<PathBuf> = self
            .all_editor_files_and_active(cx)
            .0
            .into_iter()
            .map(|(_, path)| path)
            .collect();
        let Some(clipboard) = self.explorer_clipboard.as_mut() else {
            self.model
                .set_status(tr(self.settings_menu_locale, "Explorer clipboard is empty; Copy or Cut a file first"));
            cx.notify();
            return;
        };
        let root = self.model.root().to_path_buf();
        let result = ExplorerFiles::new(&root)
            .and_then(|files| clipboard.paste_into_with_open_files(&files, &folder, &open_paths));
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
                    .set_status(tr(self.settings_menu_locale, "Explorer Paste succeeded: {count} item(s)").replace("{count}", &paths.len().to_string()));
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
                    .set_status(tr(self.settings_menu_locale, "Explorer Paste refused or stopped: {error}").replace("{error}", &error.to_string()));
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
            ExplorerMenuAction::OpenDefaultApplication => {
                let Some(path) = self.explorer_selection.as_ref()
                    .filter(|path| path.is_file())
                    .and_then(|path| path.strip_prefix(self.model.root()).ok())
                    .map(Path::to_path_buf)
                else {
                    self.model.set_status("Select an existing file to open externally");
                    cx.notify();
                    return;
                };
                let root = self.model.root().to_path_buf();
                let generation = self.workspace_generation;
                cx.spawn(async move |view, cx| {
                    let result = cx.background_spawn(async move {
                        alera_gpui_desktop::git_external_open::launch_default_application_file(
                            &root, &path,
                        )
                    }).await;
                    view.update(cx, |view, cx| {
                        if view.workspace_generation != generation { return; }
                        view.model.set_status(match result {
                            Ok(()) => "Opened file with system default application".into(),
                            Err(error) => format!("Default application refused: {error}"),
                        });
                        cx.notify();
                    }).ok();
                }).detach();
            }
            ExplorerMenuAction::RevealInFileManager => {
                let result = self
                    .explorer_selection
                    .as_ref()
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            tr(self.settings_menu_locale, "Select a file or folder first"),
                        )
                    })
                    .and_then(|path| {
                        ExplorerFiles::new(self.model.root())?.reveal_in_file_manager(path)
                    });
                self.model.set_status(match result {
                    Ok(()) => tr(self.settings_menu_locale, "Opened system file manager"),
                    Err(error) => tr(self.settings_menu_locale, "Cannot reveal selected item: {error}")
                        .replace("{error}", &error.to_string()),
                });
                cx.notify();
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
                        self.save_sidebar_layout(cx);
                        self.preview_git_change(relative, None, false, cx);
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
                        .set_status(tr(self.settings_menu_locale, "Choose an existing Explorer item first"));
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
                    .set_status(tr(self.settings_menu_locale, if action == ExplorerMenuAction::CopyRelativePath {
                        "Copied path relative to the project root"
                    } else {
                        "Copied full file/folder path"
                    }));
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
                                    .set_status(tr(self.settings_menu_locale, "Collapse folder refused: {error}").replace("{error}", &error.to_string()));
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
                            .set_status(tr(self.settings_menu_locale, "Explorer refresh failed: {error}").replace("{error}", &error.to_string()));
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
            ExplorerMenuAction::CompareLatestRevision | ExplorerMenuAction::CompareWithBranch => {
                let relative = self.explorer_selection.as_ref()
                    .filter(|path| path.is_file())
                    .and_then(|path| path.strip_prefix(self.model.root()).ok())
                    .map(Path::to_path_buf);
                if let Some(relative) = relative {
                    if action == ExplorerMenuAction::CompareLatestRevision {
                        self.open_git_file_revision_comparison(relative, None, window, cx);
                    } else {
                        self.open_git_file_branch_picker(relative, window, cx);
                    }
                }
            }
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
                .set_status(tr(self.settings_menu_locale, "Select a file or folder in Explorer first"));
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
                let placeholder = tr(self.settings_menu_locale, placeholder);
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
                .set_status(tr(self.settings_menu_locale, "Save editor changes and finish imports before deleting files"));
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
                    .set_status(tr(self.settings_menu_locale, "Explorer delete refused: {error}").replace("{error}", &error.to_string()));
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
                    self.model.set_status(
                        tr(self.settings_menu_locale, "Permanently delete {count} selected item(s), including folder contents? This cannot be undone. Press CONFIRM PERMANENT DELETE to proceed.")
                            .replace("{count}", &batch.len().to_string()),
                    );
                    self.explorer_delete_confirmed = true;
                    self.explorer_delete_pending = Some(batch);
                }
                Ok(_) => self.model.set_status(tr(self.settings_menu_locale,
                    "Close any Native Editor files inside the selection before permanent deletion",
                )),
                Err(error) => self
                    .model
                    .set_status(tr(self.settings_menu_locale, "Explorer delete preflight refused: {error}").replace("{error}", &error.to_string())),
            }
            cx.notify();
            return;
        }

        // The second click is a separate user gesture. Re-audit the exact
        // originally armed selection, Editor paths and workspace generation.
        self.explorer_delete_confirmed = false;
        let Some(batch) = self.explorer_delete_pending.take() else {
            self.model
                .set_status(tr(self.settings_menu_locale, "Delete confirmation expired; choose the items again"));
            cx.notify();
            return;
        };
        if !batch.matches_selection(&files, &selection, self.workspace_generation)
            || batch.affects_open_files(&open_files)
        {
            self.model
                .set_status(tr(self.settings_menu_locale, "Delete selection or workspace changed; no files were deleted"));
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
                    .set_status(tr(self.settings_menu_locale, "Permanently deleted {count} Explorer item(s)").replace("{count}", &count.to_string()));
            }
            Err(error) => self.model.set_status(tr(self.settings_menu_locale,
                "Explorer delete refused or partially completed: {error}. Check refreshed files.")
                .replace("{error}", &error.to_string())),
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

        let source = selected.unwrap_or_else(|| self.model.directory().to_path_buf());
        // Copy/Duplicate only read saved disk bytes, so an unrelated dirty
        // Editor must not block them. Rename/Move instead refuse *only* when
        // the selected source (including a directory ancestor) is still held
        // by a live primary or secondary Editor tab.
        if matches!(action, ExplorerAction::Rename | ExplorerAction::Move) {
            let open_paths: Vec<PathBuf> = self
                .all_editor_files_and_active(cx)
                .0
                .into_iter()
                .map(|(_, path)| path)
                .collect();
            if affects_open_editor_path(&source, &open_paths) {
                self.model.set_status(tr(self.settings_menu_locale,
                    "Close Editor tabs for the selected file or folder before Rename/Move",
                ));
                cx.notify();
                return;
            }
        }
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
                self.model.set_status(
                    tr(self.settings_menu_locale, "Explorer operation succeeded: {action}{target}")
                        .replace("{action}", &tr(self.settings_menu_locale, action.label()))
                        .replace("{target}", &target.as_ref()
                            .map(|target| format!(": {}", target.display()))
                            .unwrap_or_default()),
                );
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
                    .set_status(tr(self.settings_menu_locale, "Explorer operation refused: {action}: {error}")
                        .replace("{action}", &tr(self.settings_menu_locale, action.label()))
                        .replace("{error}", &error.to_string()));
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

    /// The pane and opaque tab instance already editing `path`, if any.
    /// Canonical-path comparison lets preview/promotion reuse the live
    /// buffer instead of opening a second tab for the same file.
    fn editor_instance_for_path(&self, path: &Path, cx: &Context<Self>) -> Option<(PaneSlot, u64)> {
        let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        [PaneSlot::Primary, PaneSlot::Secondary]
            .into_iter()
            .find_map(|pane| {
                let owner = self.editor_for_pane(pane)?;
                owner
                    .read(cx)
                    .file_backed_tab_instances()
                    .into_iter()
                    .find(|(_, open)| open == &canonical)
                    .map(|(instance, _)| (pane, instance))
            })
    }

    /// Focus the exact live editor tab `instance` in `pane` and make it the
    /// workbench's active mixed tab.
    fn focus_editor_instance(&mut self, pane: PaneSlot, instance: u64, cx: &mut Context<Self>) {
        if let Some(owner) = self.editor_for_pane(pane) {
            owner.update(cx, |editor, cx| {
                editor.activate_tab_instance(instance, cx);
            });
        }
        self.mixed_panes.activate(&WorkbenchTabIdentity::Editor {
            editor_epoch: self.editor_epoch,
            instance,
        });
        self.center_view = CenterView::Editor;
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
            let files_changed = files != workbench.editor_file_layout_snapshot;
            let changed = files_changed || active != workbench.editor_active_layout_snapshot;
            workbench.editor_file_layout_snapshot = files;
            workbench.editor_active_layout_snapshot = active;
            if changed && workbench.mixed_layout_restore.is_none() {
                if files_changed {
                    // A live editor buffer is canonical for its path; drop
                    // any read-only preview chip that would duplicate it.
                    for (_, path) in &workbench.editor_file_layout_snapshot {
                        workbench.model.release_path(path);
                    }
                }
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
        // The first Editor might be created directly in an existing Host-only
        // second pane. It still needs a fresh generation-bound document ID.
        if self.editor.is_none() && self.secondary_editor.is_none() {
            self.editor_epoch = self.editor_epoch.wrapping_add(1);
        }
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

    /// Collapse never leaves a selected-but-invisible file available for
    /// accidental batch Stage/Unstage/Discard.
    fn clear_git_change_selection(&mut self) {
        self.git_selected_changes.clear();
        self.git_selected_folders.clear();
        self.git_selection_anchor = None;
        self.git_discard_armed = None;
    }

    fn toggle_git_sidebar_section(&mut self, section: SectionKey, cx: &mut Context<Self>) {
        if !self.git_sidebar_options.collapsed_sections.insert(section) {
            self.git_sidebar_options.collapsed_sections.remove(&section);
        }
        self.clear_git_change_selection();
        cx.notify();
    }

    fn toggle_git_sidebar_folder(&mut self, folder: FolderKey, cx: &mut Context<Self>) {
        if !self
            .git_sidebar_options
            .collapsed_folders
            .insert(folder.clone())
        {
            self.git_sidebar_options.collapsed_folders.remove(&folder);
        }
        self.clear_git_change_selection();
        cx.notify();
    }

    fn toggle_git_sidebar_collapse_all(&mut self, cx: &mut Context<Self>) {
        let all = self
            .git_snapshot
            .as_ref()
            .map(|snapshot| snapshot.visible_rows(""))
            .unwrap_or_default();
        let sections = section_keys(self.git_sidebar_options.grouping, &all);
        if sections
            .iter()
            .all(|key| self.git_sidebar_options.collapsed_sections.contains(key))
        {
            self.git_sidebar_options.collapsed_sections.clear();
            self.git_sidebar_options.collapsed_folders.clear();
        } else {
            self.git_sidebar_options.collapsed_sections.extend(sections);
        }
        self.clear_git_change_selection();
        cx.notify();
    }

    fn toggle_git_sidebar_view(&mut self, cx: &mut Context<Self>) {
        self.git_sidebar_options.view = match self.git_sidebar_options.view {
            GitSidebarViewMode::Flat => GitSidebarViewMode::Tree,
            GitSidebarViewMode::Tree => GitSidebarViewMode::Flat,
        };
        self.clear_git_change_selection();
        cx.notify();
    }

    fn toggle_git_sidebar_group(&mut self, cx: &mut Context<Self>) {
        self.git_sidebar_options.grouping = match self.git_sidebar_options.grouping {
            GitSidebarGroupMode::ByArea => GitSidebarGroupMode::Unified,
            GitSidebarGroupMode::Unified => GitSidebarGroupMode::ByArea,
        };
        self.clear_git_change_selection();
        cx.notify();
    }

    fn git_sidebar_item_selected(&self, item: &GitSidebarSelectionItem) -> bool {
        match item {
            GitSidebarSelectionItem::File(row) => self.git_selected_changes.contains(row),
            GitSidebarSelectionItem::Folder(key) => self.git_selected_folders.contains(key),
        }
    }

    fn select_git_sidebar_item(
        &mut self,
        row: GitSidebarSelectionItem,
        visible: &[GitSidebarSelectionItem],
        toggle: bool,
        shift: bool,
    ) {
        if !visible.contains(&row) {
            return;
        }
        self.git_discard_armed = None;
        if shift {
            if let Some(anchor) = self.git_selection_anchor.as_ref().filter(|anchor| {
                self.git_sidebar_options.grouping == GitSidebarGroupMode::Unified
                    || match (*anchor, &row) {
                        (GitSidebarSelectionItem::File(a), GitSidebarSelectionItem::File(b)) => a.area == b.area,
                        (GitSidebarSelectionItem::Folder((a, _)), GitSidebarSelectionItem::Folder((b, _))) => a == b,
                        (GitSidebarSelectionItem::File(a), GitSidebarSelectionItem::Folder((b, _)))
                        | (GitSidebarSelectionItem::Folder((b, _)), GitSidebarSelectionItem::File(a)) => *b == Some(a.area),
                    }
            }) {
                if let (Some(from), Some(to)) = (
                    visible.iter().position(|entry| entry == anchor),
                    visible.iter().position(|entry| entry == &row),
                ) {
                    if !toggle {
                        self.git_selected_changes.clear();
                        self.git_selected_folders.clear();
                    }
                    for entry in &visible[from.min(to)..=from.max(to)] {
                        if self.git_selected_changes.len() + self.git_selected_folders.len() >= 120 {
                            break;
                        }
                        match entry {
                            GitSidebarSelectionItem::File(file) => {
                                self.git_selected_changes.insert(file.clone());
                            }
                            GitSidebarSelectionItem::Folder(folder) => {
                                self.git_selected_folders.insert(folder.clone());
                            }
                        }
                    }
                    return;
                }
            }
        }
        if !toggle {
            self.git_selected_changes.clear();
            self.git_selected_folders.clear();
        }
        let was_selected = self.git_sidebar_item_selected(&row);
        if toggle && was_selected {
            match &row {
                GitSidebarSelectionItem::File(file) => { self.git_selected_changes.remove(file); }
                GitSidebarSelectionItem::Folder(folder) => { self.git_selected_folders.remove(folder); }
            }
        } else if self.git_selected_changes.len() + self.git_selected_folders.len() < 120 {
            match &row {
                GitSidebarSelectionItem::File(file) => { self.git_selected_changes.insert(file.clone()); }
                GitSidebarSelectionItem::Folder(folder) => { self.git_selected_folders.insert(folder.clone()); }
            }
        }
        self.git_selection_anchor = Some(row);
    }

    /// Git status rows reference worktree files, not immutable staged blobs.
    /// Recompute visibility and revalidate paths before sharing Explorer's
    /// non-overwriting, workspace-bound Copy/Cut clipboard.
    fn set_git_file_clipboard(
        &mut self,
        clicked: Option<GitSidebarSelectionItem>,
        mode: ExplorerClipboardMode,
        cx: &mut Context<Self>,
    ) {
        let mut options = self.git_sidebar_options.clone();
        options.filter = self.git_filter_input.read(cx).query().to_owned();
        let visible = self.git_snapshot.as_ref()
            .map(|snapshot| layout_git_sidebar(&snapshot.visible_rows(""), &options).visible_items())
            .unwrap_or_default();
        let row = clicked.filter(|item| visible.contains(item))
            .or_else(|| self.git_selection_anchor.clone().filter(|item| visible.contains(item) && self.git_sidebar_item_selected(item)))
            .or_else(|| visible.iter().find(|item| self.git_sidebar_item_selected(item)).cloned());
        let Some(row) = row else {
            self.model.set_status("Select a visible Git file or folder first");
            cx.notify();
            return;
        };
        let paths = selected_worktree_paths(self.model.root(), &row,
            &self.git_selected_changes, &self.git_selected_folders, &visible);
        // An OS file-manager Cut can move the file without involving Alera.
        // Therefore refuse to expose a live Editor tab to native Cut at all.
        if mode == ExplorerClipboardMode::Cut {
            let open_paths = self.all_editor_files_and_active(cx).0.into_iter()
                .map(|(_, path)| path).collect::<Vec<_>>();
            if paths.iter().any(|source| open_paths.iter().any(|open| open.starts_with(source))) {
                self.model.set_status("Close Editor tabs for these files before cutting them outside Alera");
                cx.notify();
                return;
            }
        }
        let result = ExplorerFiles::new(self.model.root()).and_then(|files| {
            let clipboard = ExplorerClipboardBatch::capture(&files, &paths, mode)?;
            for source in clipboard.source_paths() {
                files.native_transfer_target(&source)?;
            }
            Ok(clipboard)
        });
        match result {
            Ok(clipboard) => {
                let count = clipboard.len();
                // The batch removes children already covered by a selected
                // directory. The OS clipboard must receive the same paths.
                let native_paths = clipboard.source_paths();
                self.explorer_clipboard = Some(clipboard);
                #[cfg(windows)]
                let native_message = match alera_gpui_desktop::native_file_clipboard::write_native_files(&native_paths, mode) {
                    Ok(()) => "; Windows Explorer clipboard ready",
                    Err(error) => {
                        self.model.set_status(format!("{count} Git item(s) in Alera clipboard, but Windows clipboard failed: {error}"));
                        cx.notify();
                        return;
                    }
                };
                #[cfg(not(windows))]
                let _ = native_paths;
                #[cfg(not(windows))]
                let native_message = "; internal Alera clipboard ready";
                self.model.set_status(format!("{} {count} worktree item(s){native_message}",
                    if mode == ExplorerClipboardMode::Copy { "Copied" } else { "Cut" }));
            }
            Err(error) => self.model.set_status(format!(
                "Git file/folder Copy/Cut refused: {error} (missing items cannot be exported)"
            )),
        }
        cx.notify();
    }

    fn paste_git_at_selection(&mut self, cx: &mut Context<Self>) {
        let hovered = self.git_selection_anchor.as_ref()
            .filter(|row| self.git_sidebar_item_selected(row))
            .map(|row| match row {
                GitSidebarSelectionItem::Folder((_, path)) => self.model.root().join(path),
                GitSidebarSelectionItem::File(file) => {
                    self.model.root().join(&file.relative).parent()
                        .unwrap_or(self.model.root()).to_path_buf()
                }
        });
        self.paste_git_file_clipboard(hovered, cx);
    }

    /// The selected exact paths must be confirmed a second time. A change
    /// in selection disarms the previous confirmation, and the writer still
    /// validates status, paths and index before touching the working tree.
    fn confirm_discard_git_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut paths = paths;
        paths.sort();
        paths.dedup();
        if self.git_action_busy || paths.is_empty() || paths.len() > 120 {
            self.git_action_status =
                "Select one or more tracked unstaged changes to discard".into();
            cx.notify();
            return;
        }
        if self.git_discard_armed.as_ref() == Some(&paths) {
            self.git_discard_armed = None;
            self.run_git_action(GitUiAction::DiscardTracked(paths), cx);
        } else {
            self.git_sensitive_armed = None;
            self.git_graph_remote_armed = None;
            self.git_discard_armed = Some(paths.clone());
            self.git_action_status = format!(
                "Discard {} tracked unstaged file(s)? Click Confirm Discard again. Staged changes and untracked files remain. This cannot be undone.",
                paths.len()
            );
            cx.notify();
        }
    }

    fn confirm_trash_git_untracked(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut paths = paths;
        paths.sort();
        paths.dedup();
        if self.git_action_busy || self.git_trash_loading || paths.is_empty() || paths.len() > 80 {
            self.git_action_status = "Select 1–80 untracked regular files for Recycle Bin".into();
            cx.notify();
            return;
        }
        if let Some(plan) = self.git_trash_preview.take() {
            let planned = plan
                .iter()
                .map(|candidate| candidate.relative.clone())
                .collect::<Vec<_>>();
            if paths == planned {
                self.run_git_action(GitUiAction::TrashUntracked(plan), cx);
                return;
            }
        }
        self.git_trash_request = self.git_trash_request.wrapping_add(1);
        let request = self.git_trash_request;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        self.git_trash_loading = true;
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                GitActions::open(&root)?.preview_untracked_trash(&paths)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_trash_request != request {
                    return;
                }
                view.git_trash_loading = false;
                match result {
                    Ok(plan) => {
                        let total = plan.iter().map(|item| item.size).sum::<u64>();
                        view.git_action_status = format!(
                            "Move {} untracked file(s), {} bytes, to the OS Recycle Bin? Confirm Trash again. Original content fingerprints will be verified again.",
                            plan.len(), total);
                        view.git_trash_preview = Some(plan);
                    }
                    Err(error) => {
                        view.git_trash_preview = None;
                        view.git_action_status = format!("Trash preview refused: {error}");
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    fn run_git_folder_menu_action(
        &mut self,
        key: FolderKey,
        action: GitFolderMenuAction,
        cx: &mut Context<Self>,
    ) {
        // The menu can outlive a filter/section transition; never operate
        // on an invisible Git folder or infer a different target.
        let mut options = self.git_sidebar_options.clone();
        options.filter = self.git_filter_input.read(cx).query().to_owned();
        let visible = self.git_snapshot.as_ref()
            .map(|snapshot| layout_git_sidebar(&snapshot.visible_rows(""), &options).visible_items())
            .unwrap_or_default();
        let item = GitSidebarSelectionItem::Folder(key.clone());
        if !visible.contains(&item) {
            self.model.set_status("Git folder changed; refresh before continuing");
            cx.notify();
            return;
        }
        let folder = self.model.root().join(&key.1);
        match action {
            GitFolderMenuAction::Copy => self.set_git_file_clipboard(Some(item), ExplorerClipboardMode::Copy, cx),
            GitFolderMenuAction::Cut => self.set_git_file_clipboard(Some(item), ExplorerClipboardMode::Cut, cx),
            GitFolderMenuAction::Paste => self.paste_git_file_clipboard(Some(folder), cx),
            GitFolderMenuAction::Reveal => {
                let result = ExplorerFiles::new(self.model.root())
                    .and_then(|files| files.reveal_in_file_manager(&folder));
                self.model.set_status(match result {
                    Ok(()) => "Revealed Git worktree folder in system file manager".into(),
                    Err(error) => format!("Cannot reveal worktree folder: {error}"),
                });
                cx.notify();
            }
            GitFolderMenuAction::CopyPath => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                    key.1.to_string_lossy().into_owned(),
                ));
            }
        }
    }

    fn run_git_change_menu_action(
        &mut self,
        row: alera_gpui_desktop::git_inspect::GitChangeRow,
        action: GitChangeMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Recompute the current visible set at click time. A menu opened
        // before collapsing a folder or changing the filter cannot operate
        // on a now-hidden or stale row, even when it remains in selection.
        let mut options = self.git_sidebar_options.clone();
        options.filter = self.git_filter_input.read(cx).query().to_owned();
        let visible = self
            .git_snapshot
            .as_ref()
            .map(|snapshot| {
                layout_git_sidebar(&snapshot.visible_rows(""), &options).visible_files()
            })
            .unwrap_or_default();
        let valid = visible.contains(&row);
        if !valid {
            self.git_discard_armed = None;
            self.git_action_status = "Git change list changed; refresh before continuing".into();
            cx.notify();
            return;
        }
        let multi = self.git_selected_changes.contains(&row);
        let selected = if multi {
            let mut rows = self
                .git_selected_changes
                .iter()
                .filter(|entry| entry.area == row.area && visible.contains(entry))
                .map(|entry| entry.relative.clone())
                .collect::<Vec<_>>();
            rows.sort();
            rows.dedup();
            rows
        } else {
            vec![row.relative.clone()]
        };
        match action {
            GitChangeMenuAction::Preview => {
                self.preview_git_change(row.relative, Some(row.area), false, cx)
            }
            GitChangeMenuAction::History => {
                self.open_git_change_file_history(&row.relative, cx);
            }
            GitChangeMenuAction::CompareLatestRevision => {
                self.open_git_file_revision_comparison(row.relative, None, window, cx);
            }
            GitChangeMenuAction::CompareWithBranch => {
                self.open_git_file_branch_picker(row.relative, window, cx);
            }
            GitChangeMenuAction::AddToGitIgnore if row.area == GitChangeArea::Untracked => {
                if self.git_action_busy {
                    self.git_action_status = "Finish the current Git operation first".into();
                    cx.notify();
                    return;
                }
                let root = self.model.root().to_path_buf();
                let relative = row.relative.clone();
                let generation = self.workspace_generation;
                cx.spawn(async move |view, cx| {
                    let result = cx.background_spawn(async move {
                        alera_gpui_desktop::git_ignore::add_untracked_file_to_gitignore(
                            &root, &relative,
                        )
                    }).await;
                    view.update(cx, |view, cx| {
                        if view.workspace_generation != generation { return; }
                        view.git_action_status = match result {
                            Ok(true) => "Added file to .gitignore".into(),
                            Ok(false) => "Already in .gitignore".into(),
                            Err(error) => format!("Add to .gitignore refused: {error}"),
                        };
                        view.refresh_git(cx);
                        cx.notify();
                    }).ok();
                }).detach();
            }
            GitChangeMenuAction::Original
                if matches!(row.area, GitChangeArea::Staged | GitChangeArea::Changes) =>
            {
                self.open_git_original_tab(row.relative, row.area, cx);
            }
            GitChangeMenuAction::OpenWorkingFile => {
                let path = self.model.root().join(&row.relative);
                if self.model.open_path(&path) {
                    self.git_original_active = false;
                    self.open_editor(cx);
                    self.model
                        .set_status(format!("Opened worktree file {}", row.relative.display()));
                } else {
                    self.model.set_status("Worktree file is missing or cannot be opened; no Git original was modified");
                }
                cx.notify();
            }
            GitChangeMenuAction::OpenExternal => {
                let root = self.model.root().to_path_buf();
                let relative = row.relative;
                let generation = self.workspace_generation;
                cx.spawn(async move |view, cx| {
                    let result = cx
                        .background_spawn(async move {
                            alera_gpui_desktop::git_external_open::launch_external_file(
                                &root, &relative,
                            )
                        })
                        .await;
                    view.update(cx, |view, cx| {
                        if view.workspace_generation != generation {
                            return;
                        }
                        view.model.set_status(match result {
                            Ok(()) => {
                                "Opened worktree file with external editor or default application"
                                    .into()
                            }
                            Err(error) => format!("External application refused: {error}"),
                        });
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            GitChangeMenuAction::CopyPath => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                    row.relative.to_string_lossy().into_owned(),
                ));
            }
            GitChangeMenuAction::CopyFiles => {
                self.set_git_file_clipboard(Some(GitSidebarSelectionItem::File(row)), ExplorerClipboardMode::Copy, cx);
            }
            GitChangeMenuAction::CutFiles => {
                self.set_git_file_clipboard(Some(GitSidebarSelectionItem::File(row)), ExplorerClipboardMode::Cut, cx);
            }
            GitChangeMenuAction::PasteFiles => {
                let path = self.model.root().join(&row.relative);
                // Deleted Git entries have no working file; their parent may
                // still be a valid destination for a paste.
                let folder = path.parent().unwrap_or(self.model.root()).to_path_buf();
                self.paste_git_file_clipboard(Some(folder), cx);
            }
            GitChangeMenuAction::RevealFile => {
                let path = self.model.root().join(&row.relative);
                let result = ExplorerFiles::new(self.model.root())
                    .and_then(|files| files.reveal_in_file_manager(&path));
                self.model.set_status(match result {
                    Ok(()) => "Revealed worktree file in system file manager".into(),
                    Err(error) => format!("Cannot reveal worktree file: {error}"),
                });
                cx.notify();
            }
            GitChangeMenuAction::Stage if row.area.can_stage() => {
                self.run_git_action(GitUiAction::StageMany(selected), cx);
            }
            GitChangeMenuAction::Unstage if row.area.can_unstage() => {
                self.run_git_action(GitUiAction::UnstageMany(selected), cx);
            }
            GitChangeMenuAction::TrashUntracked if row.area == GitChangeArea::Untracked => {
                self.confirm_trash_git_untracked(selected, cx);
            }
            GitChangeMenuAction::Discard
                if row.area == alera_gpui_desktop::git_inspect::GitChangeArea::Changes =>
            {
                self.confirm_discard_git_paths(selected, cx);
            }
            _ => {
                self.git_action_status = "Action is not supported for this Git status".into();
                cx.notify();
            }
        }
    }

    fn start_git_watch(&mut self, cx: &mut Context<Self>) {
        // Drop the old watcher first: events from a different workspace must
        // never invalidate the new workspace's Git snapshot.
        self.git_watch_service = None;
        match GitWatchService::start(self.model.root()) {
            Ok(service) => self.git_watch_service = Some(service),
            Err(error) => {
                self.git_action_status = format!("{error}; manual Refresh remains available");
                cx.notify();
            }
        }
    }

    fn pump_git_watch(&mut self, cx: &mut Context<Self>) {
        let changes = self
            .git_watch_service
            .as_mut()
            .map(|watch| watch.poll(Instant::now()))
            .unwrap_or_default();
        if changes.status {
            self.refresh_git(cx);
        }
        if changes.graph && self.git_history_open {
            if self.git_history_loading {
                // A ref changed during an existing Graph read. The callback
                // runs one final read so an older snapshot cannot win.
                self.git_history_watch_pending = true;
            } else {
                self.refresh_git_history(cx);
            }
        }
    }

    fn refresh_git(&mut self, cx: &mut Context<Self>) {
        // A second manual click during a running refresh must not disappear.
        // Coalesce repeated requests and reread after the first snapshot.
        if !self.git_refresh_gate.request() {
            return;
        }
        self.spawn_git_refresh(cx);
    }

    fn spawn_git_refresh(&mut self, cx: &mut Context<Self>) {
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
                if view.git_refresh_gate.complete() {
                    // Suppress an obsolete result; the queued refresh must be
                    // the visible state after any in-flight file change.
                    view.spawn_git_refresh(cx);
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
        let cancelled = Arc::new(AtomicBool::new(false));
        self.git_history_cancel.store(true, Ordering::Release);
        self.git_history_cancel = Arc::clone(&cancelled);
        self.git_history_request = self.git_history_request.wrapping_add(1);
        let request = self.git_history_request;
        self.git_history_loading = true;
        self.git_history_error = None;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    GitHistorySnapshot::load_filtered_cancellable(&root, &options, || {
                        cancelled.load(Ordering::Acquire)
                    })
                })
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
                let refresh_after = std::mem::take(&mut view.git_history_watch_pending);
                let mut applied = false;
                match result {
                    Ok(history) => {
                        view.git_history = Some(history);
                        view.git_graph_render_cache = None;
                        view.git_history_error = None;
                        applied = view.git_history_open;
                    }
                    Err(error) => {
                        view.git_history = None;
                        view.git_graph_render_cache = None;
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
                if refresh_after && view.git_history_open {
                    view.refresh_git_history(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn load_more_git_history(&mut self, cx: &mut Context<Self>) {
        if self.git_history_loading
            || !self.git_history_open
            || !self
                .git_history
                .as_ref()
                .is_some_and(|history| history.truncated)
        {
            return;
        }
        let current = self.git_history_options.max_commits.max(HISTORY_PAGE_STEP);
        if current >= MAX_HISTORY_PAGE {
            self.git_action_status = "Maximum bounded Git Graph history reached".into();
            cx.notify();
            return;
        }
        self.git_history_options.max_commits = (current + HISTORY_PAGE_STEP).min(MAX_HISTORY_PAGE);
        self.refresh_git_history(cx);
    }

    fn search_all_git_history(&mut self, cx: &mut Context<Self>) {
        let input = self.git_graph_search_input.read(cx);
        if input.composing() {
            self.git_action_status = "Finish IME composition before searching Git history".into();
            cx.notify();
            return;
        }
        let query = input.query().trim().to_owned();
        if query.len() > 512 || !GitGraphSearch::parse(&query).can_search_commits() {
            self.git_action_status =
                "Enter commit, author:, sha: or message: terms for full-history search".into();
            cx.notify();
            return;
        }
        self.git_history_options.search_query = Some(query);
        self.git_history_options.max_commits = 0;
        self.refresh_git_history(cx);
    }

    fn exit_git_history_search(&mut self, cx: &mut Context<Self>) {
        if self.git_history_options.search_query.take().is_some() {
            self.git_history_options.max_commits = 0;
            self.refresh_git_history(cx);
        }
    }

    fn select_git_graph_ref_tab(&mut self, tab: GitGraphRefTab, cx: &mut Context<Self>) {
        if self.git_graph_ref_tab == tab {
            return;
        }
        self.git_graph_ref_tab = tab;
        if tab.needs_remote_history() && !self.git_history_options.all_branches {
            self.git_history_options.all_branches = true;
            if self.git_history_tab_open { self.save_mixed_pane_layout(cx); }
            self.refresh_git_history(cx);
        }
        cx.notify();
    }

    fn toggle_git_history(&mut self, cx: &mut Context<Self>) {
        self.git_history_sidebar_open = !self.git_history_sidebar_open;
        let was_open = self.git_history_open;
        self.git_history_open = self.git_history_sidebar_open || self.git_history_tab_open;
        if self.git_history_open {
            if !was_open || self.git_history.is_none() { self.refresh_git_history(cx); }
        } else {
            self.git_history_cancel.store(true, Ordering::Release);
            self.git_history_request = self.git_history_request.wrapping_add(1);
            self.git_history_loading = false;
            self.git_history_watch_pending = false;
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
        self.git_history_sidebar_open = true;
        self.refresh_git_history(cx);
        self.model
            .set_status("Loading selected Explorer item Git history…");
        cx.notify();
    }

    /// A right-side Git row is still a valid History target when its worktree
    /// file has been deleted. Only normal relative pathspecs are accepted.
    fn open_git_change_file_history(&mut self, relative: &Path, cx: &mut Context<Self>) {
        let path = match git_history_file_filter(relative) {
            Ok(path) => path,
            Err(error) => {
                self.git_action_status = format!("Git file history refused: {error}");
                cx.notify();
                return;
            }
        };
        self.git_history_options.file_path = Some(path);
        self.git_history_options.git_diff_root = None;
        self.git_history_options.selected_ref = None;
        self.git_history_options.favorites_only = false;
        self.git_history_open = true;
        self.git_history_sidebar_open = true;
        self.refresh_git_history(cx);
        self.model.set_status("Loading selected Git file history…");
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
        if !matches!(
            action,
            GitGraphBranchMenuAction::PullBranch | GitGraphBranchMenuAction::PushBranch
        ) {
            self.git_graph_remote_armed = None;
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
                if !remote {
                    self.compare_git_branch_files(branch, CompareDirection::CurrentToSelected, cx);
                }
            }
            GitGraphBranchMenuAction::CompareReverse => {
                if !remote {
                    self.compare_git_branch_files(branch, CompareDirection::SelectedToCurrent, cx);
                }
            }
            GitGraphBranchMenuAction::PreviewRebase => {
                if !remote {
                    self.preview_git_rebase(branch, cx);
                }
            }
            GitGraphBranchMenuAction::PrepareRename => {
                let owning_worktree = self
                    .worktrees
                    .iter()
                    .find(|entry| entry.branch.as_deref() == Some(branch.as_str()));
                let current_owner = owning_worktree.is_some_and(|entry| {
                    entry.path == self.model.root()
                        && !entry.locked
                        && !entry.prunable
                        && !entry.detached
                });
                if remote || self.git_action_busy || (owning_worktree.is_some() && !current_owner) {
                    self.git_action_status =
                        "A checked-out branch can only be renamed from its own live worktree"
                            .into();
                    cx.notify();
                    return;
                }
                self.git_commit_ref_target = None;
                self.git_commit_ref_armed = None;
                self.git_rename_source = Some(branch.clone());
                self.git_rename_armed = None;
                self.git_rename_input
                    .update(cx, |input, cx| input.clear(cx));
                self.git_action_status = format!(
                    "Rename {branch}: enter the new branch name below Git Graph, then confirm twice."
                );
                cx.notify();
            }
            GitGraphBranchMenuAction::DeleteMerged => {
                if remote
                    || self.git_action_busy
                    || self
                        .worktrees
                        .iter()
                        .any(|entry| entry.branch.as_deref() == Some(branch.as_str()))
                {
                    self.git_branch_delete_armed = None;
                    self.git_action_status =
                        "Delete only an inactive, merged local branch with no worktree owner"
                            .into();
                    cx.notify();
                    return;
                }
                if self.git_branch_delete_armed.as_deref() == Some(branch.as_str()) {
                    self.git_branch_delete_armed = None;
                    self.run_git_action(GitUiAction::DeleteMergedBranch(branch), cx);
                } else {
                    self.git_branch_delete_armed = Some(branch.clone());
                    self.git_action_status = format!(
                        "Delete local branch '{branch}'? Only a branch already merged into current HEAD can be deleted. Reopen its Git Graph menu and choose Confirm Delete. Remote refs and working files stay unchanged.");
                    cx.notify();
                }
            }
            GitGraphBranchMenuAction::PullBranch | GitGraphBranchMenuAction::PushBranch => {
                if remote || self.git_action_busy {
                    return;
                }
                let pull = action == GitGraphBranchMenuAction::PullBranch;
                let pending = (branch.clone(), pull);
                if self.git_graph_remote_armed.as_ref() == Some(&pending) {
                    self.git_graph_remote_armed = None;
                    self.run_git_action(
                        if pull {
                            GitUiAction::PullSelected(branch)
                        } else {
                            GitUiAction::PushSelected(branch)
                        },
                        cx,
                    );
                } else {
                    self.git_graph_remote_armed = Some(pending);
                    self.git_action_status = format!(
                        "Confirm {} for {branch}: reopen its Git Graph menu and select Confirm {}. Only its configured upstream is used; no checkout, force push or merge.",
                        if pull { "Pull" } else { "Push" },
                        if pull { "Pull" } else { "Push" },
                    );
                    cx.notify();
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
        if selected.is_some() {
            // A branch ref selected from Source Control must switch the
            // History Tab to that branch, not leave All branches active.
            self.git_history_options.all_branches = false;
            self.git_history_options.favorites_only = false;
        }
        self.git_history_options.selected_ref = selected;
        if self.git_history_tab_open {
            self.save_mixed_pane_layout(cx);
        }
        self.refresh_git_history(cx);
    }

    fn prepare_git_worktree_at_commit(&mut self, oid: String, cx: &mut Context<Self>) {
        if self.git_action_busy
            || !self
                .git_history
                .as_ref()
                .is_some_and(|history| history.commits.iter().any(|row| row.oid == oid))
        {
            self.git_action_status = "Commit is stale or Git is busy; refresh Graph".into();
            cx.notify();
            return;
        }
        self.git_commit_apply_preview = None;
        self.git_commit_apply_armed = false;
        self.git_commit_apply_loading = false;
        self.git_commit_apply_request = self.git_commit_apply_request.wrapping_add(1);
        self.git_commit_ref_target = None;
        self.git_commit_ref_armed = None;
        self.git_rename_source = None;
        self.git_rename_armed = None;
        self.git_worktree_commit_target = Some(oid.clone());
        self.git_worktree_commit_armed = None;
        self.git_worktree_commit_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_action_status = format!(
            "New Worktree at {}: enter a new branch name and confirm twice.",
            &oid[..8]
        );
        cx.notify();
    }

    fn confirm_git_worktree_at_commit(&mut self, cx: &mut Context<Self>) {
        let Some(oid) = self.git_worktree_commit_target.clone() else {
            return;
        };
        let branch = self
            .git_worktree_commit_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        if self.git_action_busy || branch.is_empty() {
            self.git_action_status = "Enter a new Worktree branch name before confirming".into();
            cx.notify();
            return;
        }
        if !self
            .git_history
            .as_ref()
            .is_some_and(|history| history.commits.iter().any(|row| row.oid == oid))
        {
            self.git_worktree_commit_armed = None;
            self.git_action_status = "Commit is stale; refresh and select again".into();
            cx.notify();
            return;
        }
        let pending = (oid.clone(), branch.clone());
        if self.git_worktree_commit_armed.as_ref() != Some(&pending) {
            self.git_worktree_commit_armed = Some(pending);
            self.git_action_status = format!("Confirm creating managed Worktree '{branch}' at {}. Click Confirm again; current workspace is untouched.", &oid[..8]);
            cx.notify();
            return;
        }
        self.git_worktree_commit_armed = None;
        self.run_git_action(GitUiAction::CreateWorktreeAtCommit(oid, branch), cx);
    }

    fn cancel_git_worktree_at_commit(&mut self, cx: &mut Context<Self>) {
        self.git_worktree_commit_target = None;
        self.git_worktree_commit_armed = None;
        self.git_worktree_commit_input
            .update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn preview_git_commit_apply(
        &mut self,
        oid: String,
        kind: GitCommitApplyKind,
        mainline: u32,
        cx: &mut Context<Self>,
    ) {
        if self.git_action_busy
            || self.git_commit_apply_loading
            || !self
                .git_history
                .as_ref()
                .is_some_and(|history| history.commits.iter().any(|row| row.oid == oid))
        {
            self.git_action_status = "Git Graph commit selection is stale or Git is busy".into();
            cx.notify();
            return;
        }
        if self.editor_dirty(cx) {
            self.git_action_status =
                "Save unsaved editor buffers before previewing a commit operation".into();
            cx.notify();
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_commit_apply_request = self.git_commit_apply_request.wrapping_add(1);
        let ticket = self.git_commit_apply_request;
        self.git_commit_apply_preview = None;
        self.git_commit_apply_armed = false;
        self.git_commit_apply_loading = true;
        self.git_soft_reset_request = self.git_soft_reset_request.wrapping_add(1);
        self.git_soft_reset_preview = None;
        self.git_soft_reset_armed = false;
        self.git_soft_reset_loading = false;
        self.git_rebase_request = self.git_rebase_request.wrapping_add(1);
        self.git_rebase_preview = None;
        self.git_rebase_armed = false;
        self.git_rebase_loading = false;
        self.git_commit_ref_target = None;
        self.git_commit_ref_armed = None;
        self.git_worktree_commit_target = None;
        self.git_worktree_commit_armed = None;
        self.git_rename_source = None;
        self.git_rename_armed = None;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    let git = GitActions::open(&root)?;
                    git.preview_commit_apply_with_mainline(&oid, kind, mainline)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.git_commit_apply_request != ticket
                {
                    return;
                }
                view.git_commit_apply_loading = false;
                match result {
                    Ok(plan) => {
                        view.git_action_status = format!(
                            "{} preview: {} file(s) on {}. Review paths then confirm twice.",
                            kind.label(),
                            plan.changed_paths.len(),
                            plan.current_branch
                        );
                        view.git_commit_apply_preview = Some(plan);
                    }
                    Err(error) => {
                        view.git_commit_apply_preview = None;
                        view.git_action_status =
                            format!("{} preview refused: {error}", kind.label());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn confirm_git_commit_apply(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.git_commit_apply_preview.clone() else {
            return;
        };
        if self.git_action_busy || self.git_commit_apply_loading {
            self.git_action_status = "Finish the current Git operation first".into();
            cx.notify();
            return;
        }
        if !self.git_history.as_ref().is_some_and(|history| {
            history
                .commits
                .iter()
                .any(|row| row.oid == preview.source_oid)
        }) {
            self.git_commit_apply_preview = None;
            self.git_commit_apply_armed = false;
            self.git_action_status = "Git Graph commit is stale; refresh and preview again".into();
            cx.notify();
            return;
        }
        if !self.git_commit_apply_armed {
            self.git_commit_apply_armed = true;
            self.git_action_status = format!(
                "Confirm {} of {} onto {} ({} changed files). Click Confirm again to create a commit.",
                preview.kind.label(), &preview.source_oid[..8],
                preview.current_branch, preview.changed_paths.len());
            cx.notify();
            return;
        }
        self.git_commit_apply_armed = false;
        self.run_git_action(GitUiAction::ApplyCommitPreview(preview), cx);
    }

    fn cancel_git_commit_apply(&mut self, cx: &mut Context<Self>) {
        self.git_commit_apply_request = self.git_commit_apply_request.wrapping_add(1);
        self.git_commit_apply_preview = None;
        self.git_commit_apply_loading = false;
        self.git_commit_apply_armed = false;
        cx.notify();
    }

    fn preview_git_soft_reset(
        &mut self,
        oid: String,
        mode: NativeResetMode,
        cx: &mut Context<Self>,
    ) {
        if self.git_action_busy
            || !self
                .git_history
                .as_ref()
                .is_some_and(|history| history.commits.iter().any(|row| row.oid == oid))
        {
            self.git_action_status = "Reset target is stale or Git is busy".into();
            cx.notify();
            return;
        }
        if self.editor_dirty(cx) {
            self.git_action_status = "Save unsaved editor buffers before reset preview".into();
            cx.notify();
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_soft_reset_request = self.git_soft_reset_request.wrapping_add(1);
        let request = self.git_soft_reset_request;
        self.git_soft_reset_preview = None;
        self.git_soft_reset_armed = false;
        self.git_soft_reset_mode = mode;
        self.git_soft_reset_loading = true;
        self.git_commit_apply_preview = None;
        self.git_commit_apply_armed = false;
        self.git_commit_apply_request = self.git_commit_apply_request.wrapping_add(1);
        self.git_rebase_request = self.git_rebase_request.wrapping_add(1);
        self.git_rebase_preview = None;
        self.git_rebase_armed = false;
        self.git_rebase_loading = false;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    GitActions::open(&root)?.preview_soft_reset(&oid)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.git_soft_reset_request != request
                {
                    return;
                }
                view.git_soft_reset_loading = false;
                match result {
                    Ok(preview) => {
                        view.git_action_status = format!(
                            "{} Reset preview: {} unpublished commit(s) on {}. Review and confirm twice.",
                            mode.label(), preview.removed_commits, preview.current_branch
                        );
                        view.git_soft_reset_preview = Some(preview);
                    }
                    Err(error) => {
                        view.git_soft_reset_preview = None;
                        view.git_action_status = format!("{} Reset preview refused: {error}", mode.label());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn confirm_git_soft_reset(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.git_soft_reset_preview.clone() else {
            return;
        };
        if self.git_action_busy || self.git_soft_reset_loading {
            self.git_action_status = "Finish the current Git operation first".into();
            cx.notify();
            return;
        }
        if !self.git_history.as_ref().is_some_and(|history| {
            history
                .commits
                .iter()
                .any(|row| row.oid == preview.target_oid)
        }) {
            self.cancel_git_soft_reset(cx);
            self.git_action_status = "Git Graph changed; refresh and preview reset again".into();
            cx.notify();
            return;
        }
        if !self.git_soft_reset_armed {
            self.git_soft_reset_armed = true;
            self.git_action_status = format!(
                "Confirm {} Reset {} from {} to {} ({} commit(s)). {} Click Confirm again.",
                self.git_soft_reset_mode.label(),
                preview.current_branch,
                &preview.current_oid[..8],
                &preview.target_oid[..8],
                preview.removed_commits,
                match self.git_soft_reset_mode {
                    NativeResetMode::Soft => "Index and files remain unchanged.",
                    NativeResetMode::Mixed => "Index changes, working files remain unchanged.",
                },
            );
            cx.notify();
            return;
        }
        self.git_soft_reset_armed = false;
        self.run_git_action(
            match self.git_soft_reset_mode {
                NativeResetMode::Soft => GitUiAction::ApplySoftReset(preview),
                NativeResetMode::Mixed => GitUiAction::ApplyMixedReset(preview),
            },
            cx,
        );
    }

    fn cancel_git_soft_reset(&mut self, cx: &mut Context<Self>) {
        self.git_soft_reset_request = self.git_soft_reset_request.wrapping_add(1);
        self.git_soft_reset_preview = None;
        self.git_soft_reset_loading = false;
        self.git_soft_reset_armed = false;
        cx.notify();
    }

    fn preview_git_rebase(&mut self, branch: String, cx: &mut Context<Self>) {
        if self.git_action_busy {
            self.git_action_status = "Finish the current Git operation first".into();
            cx.notify();
            return;
        }
        if self.editor_dirty(cx) {
            self.git_action_status = "Save unsaved editor buffers before Rebase preview".into();
            cx.notify();
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_rebase_request = self.git_rebase_request.wrapping_add(1);
        let request = self.git_rebase_request;
        self.git_rebase_preview = None;
        self.git_rebase_armed = false;
        self.git_rebase_loading = true;
        self.git_soft_reset_request = self.git_soft_reset_request.wrapping_add(1);
        self.git_soft_reset_preview = None;
        self.git_soft_reset_armed = false;
        self.git_commit_apply_request = self.git_commit_apply_request.wrapping_add(1);
        self.git_commit_apply_preview = None;
        self.git_commit_apply_armed = false;
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                GitActions::open(&root)?.preview_rebase_onto_branch(&branch)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.git_rebase_request != request
                {
                    return;
                }
                view.git_rebase_loading = false;
                match result {
                    Ok(plan) => {
                        view.git_action_status = format!(
                            "Rebase preview: replay {} unpublished commit(s) from {} onto {}. Review and confirm twice; conflicts abort.",
                            plan.commits.len(), plan.current_branch, plan.onto_branch
                        );
                        view.git_rebase_preview = Some(plan);
                    }
                    Err(error) => {
                        view.git_rebase_preview = None;
                        view.git_action_status = format!("Rebase preview refused: {error}");
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    fn confirm_git_rebase(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self.git_rebase_preview.clone() else {
            return;
        };
        if self.git_action_busy || self.git_rebase_loading {
            self.git_action_status = "Finish the current Git operation first".into();
            cx.notify();
            return;
        }
        if self.editor_dirty(cx) {
            self.git_action_status = "Save unsaved editor buffers before running Rebase".into();
            cx.notify();
            return;
        }
        if !self.git_history.as_ref().is_some_and(|history| {
            history
                .branches
                .iter()
                .any(|row| !row.remote && row.name == plan.onto_branch)
        }) {
            self.cancel_git_rebase(cx);
            self.git_action_status =
                "Git Graph target is stale; refresh and preview Rebase again".into();
            cx.notify();
            return;
        }
        if !self.git_rebase_armed {
            self.git_rebase_armed = true;
            self.git_action_status = format!(
                "Confirm Rebase {} at {} onto {} at {} ({} commit(s)). Conflicts will abort; a recovery ref is created. Click Confirm again.",
                plan.current_branch, &plan.current_oid[..8],
                plan.onto_branch, &plan.onto_oid[..8], plan.commits.len()
            );
            cx.notify();
            return;
        }
        self.git_rebase_armed = false;
        self.run_git_action(GitUiAction::ApplyRebase(plan), cx);
    }

    fn cancel_git_rebase(&mut self, cx: &mut Context<Self>) {
        self.git_rebase_request = self.git_rebase_request.wrapping_add(1);
        self.git_rebase_preview = None;
        self.git_rebase_loading = false;
        self.git_rebase_armed = false;
        cx.notify();
    }

    fn prepare_git_commit_ref(
        &mut self,
        oid: String,
        kind: GitCommitRefKind,
        cx: &mut Context<Self>,
    ) {
        if self.git_action_busy
            || !self
                .git_history
                .as_ref()
                .is_some_and(|history| history.commits.iter().any(|row| row.oid == oid))
        {
            self.git_action_status = "Commit is stale or Git is busy; refresh Git Graph".into();
            cx.notify();
            return;
        }
        self.git_rename_source = None;
        self.git_rename_armed = None;
        self.git_graph_remote_armed = None;
        self.git_worktree_commit_target = None;
        self.git_worktree_commit_armed = None;
        self.git_commit_apply_preview = None;
        self.git_commit_apply_armed = false;
        self.git_commit_ref_target = Some((oid.clone(), kind));
        self.git_commit_ref_armed = None;
        self.git_commit_ref_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_action_status = format!(
            "Create {} at {}: enter a new name, then confirm twice.",
            kind.label(),
            &oid[..8]
        );
        cx.notify();
    }

    fn confirm_git_commit_ref(&mut self, cx: &mut Context<Self>) {
        let Some((oid, kind)) = self.git_commit_ref_target.clone() else {
            return;
        };
        let name = self.git_commit_ref_input.read(cx).query().trim().to_owned();
        if self.git_action_busy || name.is_empty() {
            self.git_action_status = "Enter a new branch or tag name before confirming".into();
            cx.notify();
            return;
        }
        let pending = (oid.clone(), kind, name.clone());
        if self.git_commit_ref_armed.as_ref() != Some(&pending) {
            self.git_commit_ref_armed = Some(pending);
            self.git_action_status = format!("Confirm creating {} '{name}' at {}. Click Confirm Create again. HEAD and worktree will not change.", kind.label(), &oid[..8]);
            cx.notify();
            return;
        }
        if !self
            .git_history
            .as_ref()
            .is_some_and(|history| history.commits.iter().any(|row| row.oid == oid))
        {
            self.git_commit_ref_armed = None;
            self.git_action_status =
                "Commit disappeared from Git Graph; refresh and select again".into();
            cx.notify();
            return;
        }
        self.git_commit_ref_armed = None;
        self.run_git_action(GitUiAction::CreateCommitRef(kind, oid, name), cx);
    }

    fn cancel_git_commit_ref(&mut self, cx: &mut Context<Self>) {
        self.git_commit_ref_target = None;
        self.git_commit_ref_armed = None;
        self.git_commit_ref_input
            .update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn confirm_graph_branch_rename(&mut self, cx: &mut Context<Self>) {
        let Some(old) = self.git_rename_source.clone() else {
            return;
        };
        let new = self.git_rename_input.read(cx).query().trim().to_owned();
        if self.git_action_busy || new.is_empty() || old == new {
            self.git_action_status = "Enter a different, non-empty branch name to rename".into();
            cx.notify();
            return;
        }
        let pending = (old.clone(), new.clone());
        if self.git_rename_armed.as_ref() != Some(&pending) {
            self.git_rename_armed = Some(pending);
            let owned = self.worktrees.iter().any(|entry| {
                entry.path == self.model.root() && entry.branch.as_deref() == Some(old.as_str())
            });
            self.git_action_status = if owned {
                format!("Confirm renaming current branch {old} to {new}. Git will update this worktree's HEAD; no checkout, remote rename or file changes. Click Confirm Rename again.")
            } else {
                format!("Confirm renaming inactive branch {old} to {new}. Remote branches and worktrees stay unchanged. Click Confirm Rename again.")
            };
            cx.notify();
            return;
        }
        self.git_rename_armed = None;
        let current = self.worktrees.iter().any(|entry| {
            entry.path == self.model.root() && entry.branch.as_deref() == Some(old.as_str())
        });
        self.run_git_action(
            if current {
                GitUiAction::RenameOwnedBranch(old, new)
            } else {
                GitUiAction::RenameInactiveBranch(old, new)
            },
            cx,
        );
    }

    fn cancel_graph_branch_rename(&mut self, cx: &mut Context<Self>) {
        self.git_rename_source = None;
        self.git_rename_armed = None;
        self.git_rename_input
            .update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn confirm_selected_branch_remote(&mut self, pull: bool, cx: &mut Context<Self>) {
        if self.git_action_busy {
            self.git_action_status = "Finish the current Git operation first".into();
            cx.notify();
            return;
        }
        let branch = self.git_branch_input.read(cx).query().trim().to_owned();
        if !self
            .native_branch_choices()
            .iter()
            .any(|name| name == &branch)
        {
            self.git_graph_remote_armed = None;
            self.git_action_status = "Select an existing local branch to Pull or Push".into();
            cx.notify();
            return;
        }
        let pending = (branch.clone(), pull);
        if self.git_graph_remote_armed.as_ref() == Some(&pending) {
            self.git_graph_remote_armed = None;
            self.run_git_action(
                if pull {
                    GitUiAction::PullSelected(branch)
                } else {
                    GitUiAction::PushSelected(branch)
                },
                cx,
            );
        } else {
            self.git_graph_remote_armed = Some(pending);
            self.git_sensitive_armed = None;
            self.git_action_status = format!(
                "Confirm {} of {branch} (no checkout or force). Click the selected-branch {} button again.",
                if pull { "Pull" } else { "Push" },
                if pull { "Pull" } else { "Push" }
            );
            cx.notify();
        }
    }

    fn toggle_git_history_favorite(&mut self, branch: String, cx: &mut Context<Self>) {
        self.persist_shared_git_favorite(branch, cx);
    }

    fn compare_git_branch_files(
        &mut self,
        selected: String,
        direction: CompareDirection,
        cx: &mut Context<Self>,
    ) {
        self.start_git_branch_comparison(selected, direction, false, cx);
    }

    /// Central Git Graph compares complete committed tips in either direction.
    /// The existing Source Control comparator keeps its merge-base semantics.
    fn compare_git_center_branch_files(
        &mut self,
        selected: String,
        direction: CompareDirection,
        cx: &mut Context<Self>,
    ) {
        self.start_git_branch_comparison(selected, direction, true, cx);
    }

    fn start_git_branch_comparison(
        &mut self,
        selected: String,
        direction: CompareDirection,
        center: bool,
        cx: &mut Context<Self>,
    ) {
        if self.git_action_busy {
            self.git_action_status = "Finish current Git action before comparing branches".into();
            cx.notify();
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        let ticket = self.git_diff_request;
        self.git_branch_comparison = None;
        self.git_branch_compare_loading = true;
        self.git_graph_compare_center = center;
        self.git_graph_compare_selected = None;
        self.git_graph_compare_rows = Arc::new(Vec::new());
        self.git_graph_compare_hunk = None;
        self.git_graph_compare_scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
        self.git_diff = None;
        self.git_diff_loading = true;
        self.git_history_open = true;
        if !center {
            self.git_history_sidebar_open = true;
        }
        self.model.set_status(format!(
            "Comparing HEAD and {selected} (read-only); see {}",
            if center { "Git Graph" } else { "Source Control" }
        ));
        let report_branch = selected.clone();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    if center {
                        compare_branch_trees(&root, &selected, direction)
                    } else {
                        compare_branch_files(&root, &selected, direction)
                    }
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_diff_request != ticket {
                    return;
                }
                view.git_branch_compare_loading = false;
                view.git_diff_loading = false;
                match result {
                    Ok(comparison) => {
                        let preview_first = center && !comparison.files.is_empty();
                        view.model.set_status(format!(
                            "Compared HEAD with {report_branch}: {} file(s) - {}",
                            comparison.files.len(),
                            if center { "Git Graph" } else { "Source Control" },
                        ));
                        view.git_diff = Some(comparison.summary());
                        view.git_branch_comparison = Some(comparison);
                        if preview_first {
                            view.preview_git_compared_file(0, false, cx);
                        }
                    }
                    Err(error) => {
                        view.model.set_status(format!("Branch comparison refused: {error}"));
                        view.git_diff = Some(format!("Branch comparison refused: {error}"));
                        view.git_branch_comparison = None;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Open the exact merge-base file in an immutable Git Original tab.
    /// A changed comparison or Workspace cannot install an obsolete result.
    fn open_git_compared_original_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(comparison) = self.git_branch_comparison.clone() else {
            self.model
                .set_status("Select a Git branch comparison first");
            cx.notify();
            return;
        };
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_original_request = self.git_original_request.wrapping_add(1);
        let request = self.git_original_request;
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                open_compared_original_snapshot(&root, &comparison, index)
                    .map(|snapshot| (comparison, snapshot))
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_original_request != request {
                    return;
                }
                let result = result.and_then(|(comparison, snapshot)| {
                    let current = view.git_branch_comparison.as_ref().ok_or_else(|| {
                        "Git comparison changed; select a file again".to_owned()
                    })?;
                    if current.selected_branch != comparison.selected_branch
                        || current.source_oid != comparison.source_oid
                        || current.merge_base_oid != comparison.merge_base_oid
                        || current.target_oid != comparison.target_oid
                        || current.direction != comparison.direction
                    {
                        return Err("Git comparison changed; select a file again".into());
                    }
                    view.git_original_tabs.open(snapshot)
                });
                match result {
                    Ok(_) => {
                        view.git_original_active = true;
                        view.explorer_preview_active = false;
                        view.center_view = CenterView::Editor;
                        view.model.set_status(
                            "Opened immutable comparison-source Git original tab; editing and saving are disabled",
                        );
                    }
                    Err(error) => {
                        view.model.set_status(format!("Cannot open compared original: {error}"));
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    fn preview_git_compared_file(&mut self, index: usize, original: bool, cx: &mut Context<Self>) {
        let Some(comparison) = self.git_branch_comparison.clone() else {
            return;
        };
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        let ticket = self.git_diff_request;
        self.git_diff_loading = true;
        self.git_graph_compare_selected = Some(index);
        self.git_graph_compare_rows = Arc::new(Vec::new());
        self.git_graph_compare_hunk = None;
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                let text = if original {
                    original_compared_file(&root, &comparison, index)
                } else {
                    diff_compared_file(&root, &comparison, index)
                }?;
                // Parse old/new line positions and hunk targets only once in
                // the background. Virtual UI renders cached bounded rows.
                let rows = native_git_graph_compare_ui::parse_center_branch_patch_rows(&text);
                Ok::<_, String>((text, rows))
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_diff_request != ticket {
                    return;
                }
                view.git_diff_loading = false;
                match result {
                    Ok((text, rows)) => {
                        view.git_diff = Some(text);
                        if view.git_graph_compare_center {
                            view.git_graph_compare_selected = Some(index);
                            view.git_graph_compare_rows = Arc::new(rows);
                            view.git_graph_compare_scroll
                                .scroll_to_item(0, gpui::ScrollStrategy::Top);
                        }
                    }
                    Err(error) => {
                        let message = format!("Git comparison file preview refused: {error}");
                        view.git_diff = Some(message.clone());
                        if view.git_graph_compare_center {
                            view.git_graph_compare_rows = Arc::new(
                                native_git_graph_compare_ui::parse_center_branch_patch_rows(&message),
                            );
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn preview_git_history(&mut self, request: GitReadRequest, cx: &mut Context<Self>) {
        self.git_graph_compare_center = false;
        self.git_graph_compare_selected = None;
        self.git_graph_compare_rows = Arc::new(Vec::new());
        self.git_graph_compare_hunk = None;
        self.git_branch_comparison = None;
        self.git_branch_compare_loading = false;
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        let ticket = self.git_diff_request;
        self.git_file_diff_paging = None;
        self.center_git_diff_source = None;
        self.center_git_diff_hunk = None;
        self.center_git_diff_rows = Arc::new(Vec::new());
        self.git_diff = None;
        self.git_diff_loading = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    match request {
                        GitReadRequest::Commit(oid) => preview_commit(&root, &oid),
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
        let unsaved_editors = self.editor_dirty(cx);
        let index_only = action.is_index_only();
        let worktree_independent = action.is_worktree_independent();
        let submitted_commit_message = match &action {
            GitUiAction::Commit(message) => Some(message.clone()),
            GitUiAction::Amend(preview) => Some(preview.replacement_message.clone()),
            _ => None,
        };
        if unsaved_editors && !worktree_independent {
            self.git_action_status = "Save unsaved editor buffers before changing Git".into();
            cx.notify();
            return;
        }
        self.git_sensitive_armed = None;
        self.git_discard_armed = None;
        self.git_soft_reset_request = self.git_soft_reset_request.wrapping_add(1);
        self.git_soft_reset_preview = None;
        self.git_soft_reset_loading = false;
        self.git_soft_reset_armed = false;
        self.git_rebase_request = self.git_rebase_request.wrapping_add(1);
        self.git_rebase_preview = None;
        self.git_rebase_loading = false;
        self.git_rebase_armed = false;
        self.git_conflict_confirm_armed = false;
        self.git_conflict_loading = false;
        self.git_conflict_request = self.git_conflict_request.wrapping_add(1);
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_action_request = self.git_action_request.wrapping_add(1);
        let ticket = self.git_action_request;
        self.git_commit_draft_request = self.git_commit_draft_request.wrapping_add(1);
        self.git_commit_draft_loading = false;
        self.git_ai_prompt_request = self.git_ai_prompt_request.wrapping_add(1);
        self.git_ai_prompt_preview = None;
        self.git_ai_prompt_loading = false;
        self.cancel_ai_commit_generation();
        self.git_action_busy = true;
        self.git_graph_remote_armed = None;
        self.git_branch_comparison = None;
        self.git_branch_compare_loading = false;
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
                        GitUiAction::DiscardTracked(paths) => git
                            .discard_unstaged_tracked(&paths)
                            .map(|count| format!("Restored {count} tracked file(s) from index; staged changes preserved")),
                        GitUiAction::TrashUntracked(plan) => git.apply_untracked_trash(&plan)
                            .map(|count| format!("Moved {count} untracked file(s) to OS Trash / Recycle Bin")),
                        GitUiAction::Commit(message) => {
                            git.commit(&message).map(|id| format!("Committed {id}"))
                        }
                        GitUiAction::Amend(plan) => git.apply_amend_preview(&plan),
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
                        GitUiAction::RenameInactiveBranch(old, new) => {
                            git.rename_inactive_branch(&old, &new)
                        }
                        GitUiAction::RenameOwnedBranch(old, new) => {
                            git.rename_owned_branch(&old, &new)
                        }
                        GitUiAction::DeleteMergedBranch(name) => git.delete_merged_inactive_branch(&name),
                        GitUiAction::CreateCommitRef(kind, oid, name) => {
                            git.create_ref_at_commit(kind, &oid, &name)
                        }
                        GitUiAction::ApplyCommitPreview(preview) => git.apply_commit_preview(&preview),
                        GitUiAction::ApplySoftReset(plan) => git.apply_soft_reset(&plan),
                        GitUiAction::ApplyMixedReset(plan) => git.apply_mixed_reset(&plan),
                        GitUiAction::ApplyRebase(plan) => git.apply_rebase_preview(&plan),
                        GitUiAction::CreateWorktreeAtCommit(oid, branch) => {
                            let manager = alera_gpui_desktop::worktree_manager::WorktreeManager::discover(&root)
                                .map_err(|error| error.to_string())?;
                            let created = manager.create_at_commit(&branch, &oid)
                                .map_err(|error| error.to_string())?;
                            Ok(format!("Created Worktree {} at {}. Select it in Projects.", created.display(), &oid[..8]))
                        }
                        GitUiAction::PullSelected(branch) => {
                            git.pull_branch_without_switch(&branch)
                        }
                        GitUiAction::PushSelected(branch) => {
                            git.push_branch_without_switch(&branch)
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
                        view.git_action_status = if index_only && unsaved_editors {
                            format!(
                                "{message}. Only saved disk bytes changed in Git; unsaved Editor text was not staged."
                            )
                        } else if worktree_independent && unsaved_editors {
                            format!(
                                "{message}. Unsaved Editor text and working files remain untouched."
                            )
                        } else {
                            message
                        };
                        if should_clear_git_commit_input(
                            submitted_commit_message.as_deref(),
                            view.git_commit_input.read(cx).query(),
                        ) {
                            view.git_commit_input.update(cx, |input, cx| input.clear(cx));
                        }
                        view.git_branch_delete_armed = None;
                        view.git_amend_preview = None;
                        view.git_commit_ref_target = None;
                        view.git_commit_ref_armed = None;
                        view.git_commit_apply_preview = None;
                        view.git_commit_apply_armed = false;
                        view.git_soft_reset_preview = None;
                        view.git_soft_reset_armed = false;
                        view.git_rebase_preview = None;
                        view.git_rebase_armed = false;
                        view.git_worktree_commit_target = None;
                        view.git_worktree_commit_armed = None;
                        view.git_worktree_commit_input.update(cx, |input, cx| input.clear(cx));
                        view.git_commit_ref_input.update(cx, |input, cx| input.clear(cx));
                        view.git_rename_source = None;
                        view.git_rename_armed = None;
                        view.git_rename_input.update(cx, |input, cx| input.clear(cx));
                        view.git_conflict_preview = None;
                        view.git_conflict_choices.clear();
                        view.git_conflict_selected = None;
                        view.git_conflict_confirm_armed = false;
                        view.git_snapshot = None;
                        view.git_selected_changes.clear();
                        view.git_selected_folders.clear();
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

    /// A deliberately offline fallback. Classic's AI Assist is configured in
    /// Flutter and must not be silently replaced with an unapproved agent.
    /// This only suggests an editable draft from staged Git metadata.
    fn cancel_ai_commit_generation(&mut self) {
        self.git_ai_generate_cancel.store(true, Ordering::Release);
        self.git_ai_generate_request = self.git_ai_generate_request.wrapping_add(1);
        self.git_ai_generate_loading = false;
    }

    /// Explicit user-triggered AI generation. Host owns the settings; CLI
    /// enforces read-only Agent policy and never auto-commits anything.
    fn generate_ai_commit_message(&mut self, cx: &mut Context<Self>) {
        if self.git_ai_generate_loading {
            self.cancel_ai_commit_generation();
            self.git_action_status =
                "AI Commit generation cancelled; waiting for Agent shutdown".into();
            cx.notify();
            return;
        }
        if self.git_action_busy || self.git_ai_prompt_loading {
            self.git_action_status = "Finish the current Git action or prompt preview first".into();
            cx.notify();
            return;
        }
        let Some(preview) = self.git_ai_prompt_preview.clone() else {
            self.git_action_status = "Preview staged AI prompt before generating".into();
            cx.notify();
            return;
        };
        let initial_message = self.git_commit_input.read(cx).query().to_owned();
        let workspace = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        let current = self.git_ai_generate_request.wrapping_add(1);
        self.git_ai_generate_cancel.store(true, Ordering::Release);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.git_ai_generate_cancel = Arc::clone(&cancelled);
        self.git_ai_generate_request = current;
        self.git_ai_generate_loading = true;
        self.git_action_status =
            "Generating AI Commit with Classic settings; read-only Agent, no automatic commit..."
                .into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                let bridge = RuntimeCliBridge::discover().map_err(|e| e.to_string())?;
                bridge.generate_ai_commit(&workspace, &preview, &cancelled)
                    .map_err(|e| e.to_string())
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.git_ai_generate_request != current
                {
                    return;
                }
                view.git_ai_generate_loading = false;
                match result {
                    Ok(GeneratedAiCommit { text, agent_label, .. })
                        if view.git_commit_input.read(cx).query() == initial_message => {
                        view.git_commit_input.update(cx, |input, cx| {
                            input.set_query(&text, cx);
                        });
                        view.git_action_status = format!(
                            "AI Commit suggestion generated by {agent_label} using Classic settings. Review before committing."
                        );
                    }
                    Ok(_) => {
                        view.git_action_status =
                            "AI suggestion not applied: commit message was edited during generation".into();
                    }
                    Err(error) => {
                        view.git_action_status = format!(
                            "AI Commit generation unavailable: {error}"
                        );
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    fn prepare_staged_ai_prompt(&mut self, cx: &mut Context<Self>) {
        self.cancel_ai_commit_generation();
        if self.git_ai_prompt_loading {
            self.git_ai_prompt_request = self.git_ai_prompt_request.wrapping_add(1);
            self.git_ai_prompt_loading = false;
            self.git_ai_prompt_preview = None;
            self.git_action_status = "Cancelled pending staged AI prompt preview".into();
            cx.notify();
            return;
        }
        if self.git_action_busy {
            self.git_action_status = "Finish the current Git operation first".into();
            cx.notify();
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_ai_prompt_request = self.git_ai_prompt_request.wrapping_add(1);
        let request = self.git_ai_prompt_request;
        self.git_ai_prompt_preview = None;
        self.git_ai_prompt_loading = true;
        self.git_action_status =
            "Preparing read-only staged AI prompt using shared Rust Core...".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                prepare_staged_commit_prompt(&root, "").map_err(|error| error.to_string())
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_ai_prompt_request != request {
                    return;
                }
                view.git_ai_prompt_loading = false;
                match result {
                    Ok(plan) => {
                        view.git_action_status = format!(
                            "AI prompt prepared from {} staged file(s). No agent was run; review and copy explicitly.",
                            plan.staged_files
                        );
                        view.git_ai_prompt_preview = Some(plan);
                    }
                    Err(error) => {
                        view.git_ai_prompt_preview = None;
                        view.git_action_status = format!("AI prompt preview unavailable: {error}");
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    fn copy_staged_ai_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self.git_ai_prompt_preview.clone() else {
            return;
        };
        if self.git_action_busy || self.git_ai_prompt_loading {
            self.git_action_status =
                "Finish the current Git operation before copying the AI prompt".into();
            cx.notify();
            return;
        }
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        let request = self.git_ai_prompt_request;
        self.git_ai_prompt_loading = true;
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                verify_staged_commit_prompt(&root, &plan)
                    .map_err(|error| error.to_string())
                    .map(|_| plan.prompt)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_ai_prompt_request != request {
                    return;
                }
                view.git_ai_prompt_loading = false;
                match result {
                    Ok(prompt) => {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(prompt));
                        view.git_action_status = "Copied staged AI prompt. Paste into your approved Agent; Native did not run AI or alter Git.".into();
                    }
                    Err(error) => {
                        view.git_ai_prompt_preview = None;
                        view.git_action_status = format!("AI prompt was not copied: {error}. Regenerate the preview.");
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    fn close_staged_ai_prompt(&mut self, cx: &mut Context<Self>) {
        self.cancel_ai_commit_generation();
        self.git_ai_prompt_request = self.git_ai_prompt_request.wrapping_add(1);
        self.git_ai_prompt_preview = None;
        self.git_ai_prompt_loading = false;
        cx.notify();
    }

    fn prepare_offline_commit_draft(&mut self, cx: &mut Context<Self>) {
        if self.git_action_busy {
            self.git_action_status = "Finish the current Git operation first".into();
            cx.notify();
            return;
        }
        if self.git_commit_draft_loading {
            self.git_commit_draft_request = self.git_commit_draft_request.wrapping_add(1);
            self.git_commit_draft_loading = false;
            self.git_action_status = "Discarded pending offline commit draft".into();
            cx.notify();
            return;
        }
        let initial = self.git_commit_input.read(cx).query().to_owned();
        let root = self.model.root().to_path_buf();
        let generation = self.workspace_generation;
        self.git_commit_draft_request = self.git_commit_draft_request.wrapping_add(1);
        let request = self.git_commit_draft_request;
        self.git_commit_draft_loading = true;
        self.git_action_status = "Preparing offline commit draft from staged files only...".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                GitActions::open(&root)?.draft_commit_message()
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.git_commit_draft_request != request
                {
                    return;
                }
                view.git_commit_draft_loading = false;
                match result {
                    Ok(draft) if view.git_commit_input.read(cx).query() == initial => {
                        view.git_commit_input.update(cx, |input, cx| {
                            input.set_query(&draft.text, cx);
                        });
                        view.git_action_status = format!(
                            "Offline draft based on {} staged file(s). Review and edit it before committing; no AI was used.",
                            draft.staged_files
                        );
                    }
                    Ok(_) => {
                        view.git_action_status =
                            "Draft not applied because the commit message field changed".into();
                    }
                    Err(error) => {
                        view.git_action_status = format!("Offline commit draft unavailable: {error}");
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    fn prepare_or_confirm_git_amend(&mut self, cx: &mut Context<Self>) {
        let message = self.git_commit_input.read(cx).query().trim().to_owned();
        if self.git_action_busy || self.git_amend_loading {
            self.git_action_status = "Wait for the active Git operation or amend preview".into();
            cx.notify();
            return;
        }
        if let Some(plan) = self.git_amend_preview.clone() {
            if plan.replacement_message == message {
                if self.editor_dirty(cx) {
                    self.git_action_status =
                        "Save unsaved editor buffers before amending HEAD".into();
                    cx.notify();
                    return;
                }
                self.git_amend_preview = None;
                self.run_git_action(GitUiAction::Amend(plan), cx);
                return;
            }
            self.git_amend_preview = None;
        }
        if message.is_empty() {
            self.git_action_status = "Enter a commit message before preparing Amend".into();
            cx.notify();
            return;
        }
        if self.editor_dirty(cx) {
            self.git_action_status = "Save unsaved editor buffers before preparing Amend".into();
            cx.notify();
            return;
        }
        self.git_amend_request = self.git_amend_request.wrapping_add(1);
        let request = self.git_amend_request;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        self.git_amend_loading = true;
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                GitActions::open(&root)?.preview_amend_commit(&message)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_amend_request != request {
                    return;
                }
                view.git_amend_loading = false;
                match result {
                    Ok(plan) => {
                        let impact = if plan.staged_changes {
                            "including the current staged file contents"
                        } else {
                            "message only (staged tree unchanged)"
                        };
                        view.git_action_status = format!(
                            "Amend warning: replace local HEAD {} ({impact}) with message '{}'. Click Amend again to confirm. Never force-push automatically.",
                            &plan.head_oid[..8], plan.replacement_message);
                        view.git_amend_preview = Some(plan);
                    }
                    Err(error) => {
                        view.git_amend_preview = None;
                        view.git_action_status = format!("Amend preview refused: {error}");
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
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

    fn open_git_original_tab(
        &mut self,
        relative: PathBuf,
        area: GitChangeArea,
        cx: &mut Context<Self>,
    ) {
        self.git_original_request = self.git_original_request.wrapping_add(1);
        let request = self.git_original_request;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    alera_gpui_desktop::git_inspect::read_git_original_snapshot(
                        &root, &relative, area,
                    )
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_original_request != request {
                    return;
                }
                match result.and_then(|snapshot| view.git_original_tabs.open(snapshot)) {
                    Ok(_) => {
                        // Read-only Git versions do not enter NativeEditor at
                        // all and cannot become Save/Save As, draft or PTY tabs.
                        view.git_original_active = true;
                        view.explorer_preview_active = false;
                        view.center_view = CenterView::Editor;
                        view.model.set_status(
                            "Opened immutable Git original; editing and saving are disabled",
                        );
                    }
                    Err(error) => view
                        .model
                        .set_status(format!("Cannot open Git original: {error}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn copy_git_original_to_scratch(&mut self, cx: &mut Context<Self>) {
        let Some(original) = self.git_original_tabs.active().cloned() else {
            self.model
                .set_status("Select a Git original read-only tab first");
            cx.notify();
            return;
        };
        if !self.git_original_active {
            self.model
                .set_status("Focus the Git original tab before copying to scratch");
            cx.notify();
            return;
        }
        let text = original.snapshot.content;
        let root = self.model.root().to_path_buf();
        let result = if let Some(owner) = self.focused_editor() {
            owner
                .update(cx, |editor, cx| {
                    editor.open_scratch_with_text(&root, &text, cx)
                })
                .map(|_| ())
        } else {
            let mut tabs = EditorTabs::new(self.draft_store());
            tabs.open_scratch(&root)
                .and_then(|_| {
                    tabs.document_mut()
                        .ok_or_else(|| std::io::Error::other("New scratch document unavailable"))?
                        .replace(None, &text)
                })
                .map(|_| {
                    self.install_editor(tabs, cx);
                })
        };
        match result {
            Ok(()) => {
                self.git_original_active = false;
                self.explorer_preview_active = false;
                self.center_view = CenterView::Editor;
                self.model.set_status(
                    "Git original copied to editable scratch; Save As to create a worktree file",
                );
            }
            Err(error) => self
                .model
                .set_status(format!("Cannot copy Git revision into scratch: {error}")),
        }
        cx.notify();
    }

    fn focus_git_original_tab(&mut self, instance: u64, cx: &mut Context<Self>) {
        if self.git_original_tabs.activate(instance) {
            self.git_original_active = true;
            self.explorer_preview_active = false;
            self.center_view = CenterView::Editor;
            cx.notify();
        }
    }

    fn close_git_original_tab(&mut self, instance: u64, cx: &mut Context<Self>) {
        if self.git_original_tabs.close(instance) {
            if self.git_original_tabs.active().is_none() {
                self.git_original_active = false;
            }
            cx.notify();
        }
    }

    fn preview_git_change(
        &mut self,
        relative: PathBuf,
        area: Option<GitChangeArea>,
        original: bool,
        cx: &mut Context<Self>,
    ) {
        // A general Git preview can use the independently bounded patch
        // pager; exact Staged/Changes/Original previews retain their area.
        if native_center_git_diff_ui::uses_paged_git_preview(area, original) {
            self.preview_git_change_paged(relative, area, cx);
            return;
        }
        self.git_file_diff_paging = None;
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        let ticket = self.git_diff_request;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        self.git_diff = None;
        self.git_diff_loading = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    match (area, original) {
                        (Some(area), true) => read_git_original_for_area(&root, &relative, area),
                        (Some(area), false) => read_file_diff_for_area(&root, &relative, area),
                        (None, false) => read_file_diff(&root, &relative),
                        (None, true) => {
                            Err("Select a Git change row to view its exact original".into())
                        }
                    }
                })
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

    fn preview_git_change_paged(
        &mut self,
        relative: PathBuf,
        area: Option<GitChangeArea>,
        cx: &mut Context<Self>,
    ) {
        // A new click is a new dialog source even if it chooses the same file.
        // Paging preserves its source ticket, but re-selection invalidates it.
        self.center_git_diff_source = None;
        self.center_git_diff_hunk = None;
        self.center_git_diff_rows = Arc::new(Vec::new());
        self.git_file_diff_paging = None;
        self.load_git_file_diff_page(relative, area, 0, Vec::new(), cx);
    }

    /// Classic opens a selected Source Control file's diff in the workbench,
    /// rather than forcing users to scroll through a tiny right-side excerpt.
    /// The source ticket is recorded synchronously before the bounded page
    /// finishes loading, so a stale response cannot replace a newer click.
    fn open_git_sidebar_file_diff(
        &mut self,
        relative: PathBuf,
        area: GitChangeArea,
        cx: &mut Context<Self>,
    ) {
        self.preview_git_change(relative, Some(area), false, cx);
        if let Some(paging) = &self.git_file_diff_paging {
            self.center_git_diff_source = Some(native_action_dialog_ui::GitDialogFileSource {
                workspace_generation: self.workspace_generation,
                source_ticket: paging.source_ticket,
                relative: paging.relative.clone(),
                area: paging.area,
            });
            self.git_original_active = false;
            self.center_git_diff_hunk = None;
            self.center_git_diff_scroll
                .scroll_to_item(0, gpui::ScrollStrategy::Top);
        } else {
            // Conflict resolution keeps the existing exact-area side preview.
            self.center_git_diff_source = None;
            self.git_sidebar_patch_expanded = true;
        }
        cx.notify();
    }

    fn change_git_file_diff_page(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.git_diff_loading {
            return;
        }
        let Some(state) = self.git_file_diff_paging.as_ref() else {
            return;
        };
        let mut previous = state.previous_starts.clone();
        let next_start = if forward {
            if previous.len() >= 31 {
                return;
            }
            let Some(next) = state.next_line else { return };
            previous.push(state.start_line);
            next
        } else {
            let Some(previous_start) = previous.pop() else {
                return;
            };
            previous_start
        };
        let relative = state.relative.clone();
        let area = state.area;
        self.load_git_file_diff_page(relative, area, next_start, previous, cx);
    }

    fn load_git_file_diff_page(
        &mut self,
        relative: PathBuf,
        area: Option<GitChangeArea>,
        start_line: usize,
        previous_starts: Vec<usize>,
        cx: &mut Context<Self>,
    ) {
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        let ticket = self.git_diff_request;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        if self.center_git_diff_is_current() {
            self.center_git_diff_hunk = None;
            self.center_git_diff_scroll
                .scroll_to_item(0, gpui::ScrollStrategy::Top);
        }
        let source_ticket = self
            .git_file_diff_paging
            .as_ref()
            .filter(|current| current.relative == relative && current.area == area)
            .map(|current| current.source_ticket)
            .unwrap_or(ticket);
        self.git_file_diff_paging = Some(GitFileDiffPaging {
            relative: relative.clone(),
            area,
            source_ticket,
            start_line,
            next_line: None,
            previous_starts,
        });
        self.git_diff = None;
        self.git_diff_loading = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    match area {
                        Some(area) => {
                            read_file_diff_page_for_area(&root, &relative, area, start_line)
                        }
                        None => read_file_diff_page(&root, &relative, start_line),
                    }
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_diff_request != ticket {
                    return;
                }
                view.git_diff_loading = false;
                match result {
                    Ok(page) => {
                        view.center_git_diff_rows =
                            Arc::new(native_center_git_diff_ui::center_git_diff_rows(&page.text));
                        if let Some(paging) = view.git_file_diff_paging.as_mut() {
                            paging.next_line = page.next_line;
                        }
                        view.git_diff = Some(page.text);
                    }
                    Err(error) => {
                        view.center_git_diff_rows = Arc::new(Vec::new());
                        view.git_file_diff_paging = None;
                        view.git_diff = Some(format!("Diff unavailable: {error}"));
                    }
                }
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

    fn git_copy_files_action(&mut self, _: &GitCopyFiles, window: &mut Window, cx: &mut Context<Self>) {
        if self.right_sidebar_view == RightSidebarView::Git && self.git_keyboard_focus.is_focused(window) {
            self.set_git_file_clipboard(None, ExplorerClipboardMode::Copy, cx);
        }
    }

    fn git_cut_files_action(&mut self, _: &GitCutFiles, window: &mut Window, cx: &mut Context<Self>) {
        if self.right_sidebar_view == RightSidebarView::Git && self.git_keyboard_focus.is_focused(window) {
            self.set_git_file_clipboard(None, ExplorerClipboardMode::Cut, cx);
        }
    }

    fn git_paste_files_action(&mut self, _: &GitPasteFiles, window: &mut Window, cx: &mut Context<Self>) {
        if self.right_sidebar_view == RightSidebarView::Git && self.git_keyboard_focus.is_focused(window) {
            self.paste_git_at_selection(cx);
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

    fn explorer_rename_action(
        &mut self,
        _: &ExplorerRename,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_keyboard_focus.is_focused(window) {
            self.begin_explorer_action(ExplorerAction::Rename, window, cx);
        }
    }

    fn explorer_delete_action(
        &mut self,
        _: &ExplorerDelete,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_keyboard_focus.is_focused(window) {
            // Existing explicit confirmation still protects deletion.
            self.begin_explorer_action(ExplorerAction::Delete, window, cx);
        }
    }

    fn explorer_refresh_action(
        &mut self,
        _: &ExplorerRefresh,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_keyboard_focus.is_focused(window) {
            self.run_explorer_menu_action(ExplorerMenuAction::Refresh, window, cx);
        }
    }

    /// Exactly the rows rendered by the active Explorer view, so keyboard
    /// navigation cannot select a filtered-out or collapsed file.
    fn visible_explorer_paths(&self, cx: &Context<Self>) -> Vec<PathBuf> {
        let query = self
            .explorer_filter_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        if !query.is_empty() {
            return self
                .file_index
                .as_ref()
                .filter(|index| index.root() == self.model.root())
                .map(|index| index.search_explorer(&query, 200))
                .unwrap_or_default()
                .into_iter()
                .map(|hit| hit.path)
                .collect();
        }
        if self.tree_mode {
            if let Some(tree) = &self.tree {
                return tree
                    .rows()
                    .iter()
                    .map(|row| self.model.root().join(&row.relative_path))
                    .collect();
            }
        }
        self.model
            .entries()
            .iter()
            .map(|entry| entry.path.clone())
            .collect()
    }

    fn navigate_explorer_keyboard(
        &mut self,
        direction: ExplorerNavigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let visible = self.visible_explorer_paths(cx);
        if visible.is_empty() {
            // Empty filter results keep keyboard focus in the editable
            // search field so the user can immediately correct the query.
            return;
        }
        if !self.explorer_keyboard_focus.is_focused(window) {
            // A permanent Classic-style filter must be usable without a
            // mouse: Down leaves the search box for the matching rows.
            // Never steal an unfinished Windows IME composition.
            let from_filter = {
                let input = self.explorer_filter_input.read(cx);
                input.focus_handle(cx).is_focused(window) && !input.composing()
            };
            if direction != ExplorerNavigation::Next || !from_filter {
                return;
            }
            window.focus(&self.explorer_keyboard_focus, cx);
        }
        if let Some(path) = self.explorer_multi_selection.navigate(
            self.explorer_selection.as_deref(),
            &visible,
            direction,
        ) {
            if let Some(index) = self
                .explorer_multi_selection
                .visible_row_index(&path, &visible)
            {
                // GPUI resolves this against actual painted children. Do not
                // guess 32px rows: filtered results can have a second line.
                self.explorer_scroll.scroll_to_item(index);
            }
            self.explorer_selection = Some(path);
            self.explorer_delete_confirmed = false;
            self.explorer_delete_pending = None;
            self.explorer_context_menu = None;
            cx.notify();
        }
    }

    fn explorer_up_action(&mut self, _: &ExplorerUp, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_explorer_keyboard(ExplorerNavigation::Previous, window, cx);
    }

    fn explorer_down_action(
        &mut self,
        _: &ExplorerDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_explorer_keyboard(ExplorerNavigation::Next, window, cx);
    }

    fn explorer_first_action(
        &mut self,
        _: &ExplorerFirst,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_explorer_keyboard(ExplorerNavigation::First, window, cx);
    }

    fn explorer_last_action(
        &mut self,
        _: &ExplorerLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_explorer_keyboard(ExplorerNavigation::Last, window, cx);
    }

    fn explorer_open_action(
        &mut self,
        _: &ExplorerOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.explorer_keyboard_focus.is_focused(window) {
            return;
        }
        let Some(path) = self.explorer_selection.clone() else {
            return;
        };
        if !self.visible_explorer_paths(cx).contains(&path) {
            self.model
                .set_status("Selected Explorer row is no longer visible");
            cx.notify();
            return;
        }
        let target =
            ExplorerFiles::new(self.model.root()).and_then(|files| files.reveal_target(&path));
        match target {
            Ok(path) if path.is_file() => self.click_explorer_file(&path, true, window, cx),
            Ok(path) if path.is_dir() => {
                let filtered = !self
                    .explorer_filter_input
                    .read(cx)
                    .query()
                    .trim()
                    .is_empty();
                if filtered {
                    self.tree_mode = true;
                    if let Some(tree) = &mut self.tree {
                        if let Ok(relative) = path.strip_prefix(self.model.root()) {
                            if let Err(error) = tree.reveal_directory(relative) {
                                self.model
                                    .set_status(format!("Cannot reveal folder: {error}"));
                            }
                        }
                    }
                    self.explorer_filter_input
                        .update(cx, |input, cx| input.clear(cx));
                } else if self.tree_mode {
                    if let Some(tree) = &mut self.tree {
                        if let Ok(relative) = path.strip_prefix(self.model.root()) {
                            if let Err(error) = tree.toggle(relative) {
                                self.model
                                    .set_status(format!("Cannot open folder: {error}"));
                            }
                        }
                    }
                } else if let Some(index) = self
                    .model
                    .entries()
                    .iter()
                    .position(|entry| entry.path == path)
                {
                    self.model.open_entry(index);
                    self.persist_session();
                }
                cx.notify();
            }
            Ok(_) => {
                self.model
                    .set_status("Selected Explorer item is unsupported");
                cx.notify();
            }
            Err(error) => {
                self.model
                    .set_status(format!("Cannot open Explorer item: {error}"));
                cx.notify();
            }
        }
    }

    fn explorer_preview_action(
        &mut self,
        _: &ExplorerPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.explorer_keyboard_focus.is_focused(window) {
            return;
        }
        let Some(path) = self.explorer_selection.clone() else {
            return;
        };
        if !self.visible_explorer_paths(cx).contains(&path) {
            self.model
                .set_status("Selected Explorer row is no longer visible");
            cx.notify();
            return;
        }
        match ExplorerFiles::new(self.model.root()).and_then(|files| files.reveal_target(&path)) {
            Ok(path) if path.is_file() => self.click_explorer_file(&path, false, window, cx),
            Ok(_) => {}
            Err(error) => {
                self.model
                    .set_status(format!("Cannot preview Explorer item: {error}"));
                cx.notify();
            }
        }
    }

    fn explorer_new_file_action(
        &mut self,
        _: &ExplorerNewFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_keyboard_focus.is_focused(window) {
            self.begin_explorer_action(ExplorerAction::NewFile, window, cx);
        }
    }

    fn explorer_new_folder_action(
        &mut self,
        _: &ExplorerNewFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.explorer_keyboard_focus.is_focused(window) {
            self.begin_explorer_action(ExplorerAction::NewFolder, window, cx);
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
        let visible = self.visible_explorer_paths(cx);
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
        self.open_content_search_panel(window, cx);
    }

    fn run_content_search_action(
        &mut self,
        _: &RunContentSearch,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.right_sidebar_view == RightSidebarView::Search {
            self.search_project_contents(cx);
        }
    }

    fn find_text_references_action(
        &mut self,
        _: &FindTextReferences,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.find_text_references(cx);
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
        self.quick_open_selected_index = 0;
        self.quick_open_selection_query.clear();
        self.quick_open_results_scroll.scroll_to_item(0);
        let observed = input.clone();
        cx.observe(&input, move |view, _, cx| {
            let query = observed.read(cx).query().to_owned();
            if view.quick_open_selection_query != query {
                view.quick_open_selection_query = query;
                view.quick_open_selected_index = 0;
                view.quick_open_results_scroll.scroll_to_item(0);
            }
            cx.notify();
        })
        .detach();
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

    /// Open Classic's dedicated Search panel using the existing safe
    /// cross-worktree content-search engine. Switching tabs retains results;
    /// changing workspaces still invalidates in-flight searches.
    fn open_content_search_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.right_sidebar_view = RightSidebarView::Search;
        self.right_sidebar_visible = true;
        self.explorer_context_menu = None;
        if self.content_query.is_none() {
            self.content_query = Some(cx.new(|cx| {
                QuickOpenInput::with_placeholder("Search in files…", cx)
            }));
        }
        if let Some(input) = &self.content_query {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
        self.save_sidebar_layout(cx);
        cx.notify();
    }

    fn open_references_panel(&mut self, cx: &mut Context<Self>) {
        self.right_sidebar_view = RightSidebarView::References;
        self.right_sidebar_visible = true;
        self.explorer_context_menu = None;
        self.save_sidebar_layout(cx);
        cx.notify();
    }

    /// Read-only review entry until Native's provider-authenticated API
    /// controller is connected. This never stores credentials or fetches a PR.
    fn refresh_review_browser_url(&mut self) {
        let root = self.model.root().to_string_lossy().into_owned();
        match alera_core::git::repository_remote_url(&root) {
            Ok(Some(remote)) => {
                self.review_browser_url = repository_pull_requests_url(&remote);
                self.review_browser_error = if self.review_browser_url.is_none() {
                    Some("This Git remote cannot be mapped safely to a GitHub, Gitea or GitLab review URL".into())
                } else {
                    None
                };
            }
            Ok(None) => {
                self.review_browser_url = None;
                self.review_browser_error = Some("This workspace has no Git remote".into());
            }
            Err(error) => {
                self.review_browser_url = None;
                self.review_browser_error = Some(format!("Cannot inspect Git remote: {error}"));
            }
        }
    }

    fn open_pull_requests_panel(&mut self, cx: &mut Context<Self>) {
        self.right_sidebar_view = RightSidebarView::PullRequests;
        self.right_sidebar_visible = true;
        self.explorer_context_menu = None;
        self.refresh_review_browser_url();
        self.save_sidebar_layout(cx);
        cx.notify();
    }

    fn open_pull_requests_in_browser(&mut self, cx: &mut Context<Self>) {
        self.refresh_review_browser_url();
        if let Some(url) = &self.review_browser_url {
            cx.open_url(url);
        } else if let Some(error) = &self.review_browser_error {
            self.model.set_status(format!("Cannot open Pull Requests: {error}"));
        }
        cx.notify();
    }

    /// Find whole-identifier text occurrences in saved workspace files.
    /// No LSP provider is connected yet; this cannot distinguish definitions,
    /// comments, strings, shadowing or language-specific symbols.
    fn find_text_references(&mut self, cx: &mut Context<Self>) {
        self.open_references_panel(cx);
        let Some(owner) = self.focused_editor() else {
            self.references_error = Some("Open an Editor file, select a symbol, then run Find Text References".into());
            cx.notify();
            return;
        };
        let (path, symbol) = {
            let editor = owner.read(cx);
            let document = editor.document();
            (
                document.path().map(Path::to_path_buf),
                identifier_at_cursor(document.content(), document.selection(), document.cursor()),
            )
        };
        let Some(path) = path.filter(|path| path.starts_with(self.model.root())) else {
            self.references_error = Some("Find Text References requires a saved file inside this workspace".into());
            cx.notify();
            return;
        };
        let _ = path; // The root and identifier, not a user's absolute path, define the scan.
        let Some(symbol) = symbol else {
            self.references_error = Some("Select a single identifier or place the Editor caret on a symbol".into());
            cx.notify();
            return;
        };
        self.references_request = self.references_request.wrapping_add(1);
        let ticket = self.references_request;
        let workspace_generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        self.references_symbol = Some(symbol.clone());
        self.references_matches.clear();
        self.references_truncated = false;
        self.references_error = None;
        self.references_loading = true;
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                let mut result = search_worktree_contents(
                    &root, &symbol, WorktreeContentScope::Current,
                )?;
                result.matches.retain(|hit| {
                    is_whole_identifier_hit(&hit.hit.snippet, hit.hit.column, &symbol)
                });
                Ok::<_, std::io::Error>(result)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != workspace_generation
                    || view.references_request != ticket {
                    return;
                }
                view.references_loading = false;
                match result {
                    Ok(found) => {
                        view.references_matches = found.matches;
                        view.references_truncated = found.truncated;
                        view.references_error = None;
                    }
                    Err(error) => {
                        view.references_matches.clear();
                        view.references_truncated = false;
                        view.references_error = Some(format!("Text reference scan failed: {error}"));
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    fn clear_text_references(&mut self, cx: &mut Context<Self>) {
        self.references_request = self.references_request.wrapping_add(1);
        self.references_symbol = None;
        self.references_matches.clear();
        self.references_loading = false;
        self.references_error = None;
        self.references_truncated = false;
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
        self.content_worktrees_searched = 0;
        self.content_worktrees_skipped = 0;
        self.content_searched_query = query.clone();
        if query.is_empty() {
            self.content_loading = false;
            self.content_error = Some("Type text to search within workspace files".into());
            cx.notify();
            return;
        }
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        let scope = self.content_search_scope;
        self.content_loading = true;
        cx.spawn(async move |view, cx| {
            // Git worktree discovery, directory walking, decoding and content
            // matching all run off the GPUI UI thread. Results retain the
            // owning checkout so same-relative-path hits never get swapped.
            let result = cx
                .background_spawn(async move { search_worktree_contents(&root, &query, scope) })
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
                        view.content_worktrees_searched = results.searched_worktrees;
                        view.content_worktrees_skipped = results.skipped_worktrees;
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
        hit: &WorktreeContentMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A UI event can arrive after a new search has replaced the results.
        // Never switch worktrees or open a stale result from an old snapshot.
        if !self.content_matches.iter().any(|item| item == hit) {
            self.model.set_status("Search result changed; rerun search");
            cx.notify();
            return;
        }
        self.open_checked_search_hit(hit, window, cx);
    }

    fn open_reference_hit(
        &mut self,
        hit: &WorktreeContentMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.references_matches.iter().any(|item| item == hit) {
            self.model.set_status("Text references changed; run Find Text References again");
            cx.notify();
            return;
        }
        self.open_checked_search_hit(hit, window, cx);
    }

    /// Shared path validation, safe cross-worktree routing and Editor cursor
    /// navigation used by both the Search and Text References panels.
    fn open_checked_search_hit(
        &mut self,
        hit: &WorktreeContentMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = match hit.checked_path() {
            Ok(path) => path,
            Err(error) => {
                self.model.set_status(format!(
                    "Search result is unavailable: {error}; rerun search"
                ));
                cx.notify();
                return;
            }
        };
        let owner = hit.root.clone();
        let line = hit.hit.line;
        let column = hit.hit.column;
        if worktree_hit_requires_switch(self.model.root(), hit) {
            // Keep the existing dirty-buffer, running Host and Git safety
            // gates. Do not fabricate a success if workspace switch refused.
            self.switch_workspace(owner.clone(), cx);
            if self.model.root() != owner {
                cx.notify();
                return;
            }
        }
        if !self.model.open_path(&path) {
            self.model.set_status(
                "Search result belongs to this worktree but could not be opened; rerun search",
            );
            cx.notify();
            return;
        }
        self.open_editor(cx);
        let Some(editor) = self.focused_editor() else {
            self.model
                .set_status("Search result could not activate a Native Editor");
            cx.notify();
            return;
        };
        match editor.update(cx, |editor, cx| {
            editor.navigate_to_content_hit(&path, line, column, cx)
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
        self.git_original_active = false;
        self.git_graph_center_open = false;
        let Some(path) = self.model.selected_path().map(PathBuf::from) else {
            self.model.set_status("Choose a text file before editing");
            cx.notify();
            return;
        };
        self.explorer_preview_active = false;
        let root = self.model.root().to_path_buf();
        // Never open the same disk file as two independent mutable buffers.
        // If a pane already owns it, focus that exact existing document.
        if let Some((pane, instance)) = self.editor_instance_for_path(&path, cx) {
            self.focus_editor_instance(pane, instance, cx);
            self.release_primary_center_overlays(pane);
            // The editable buffer is the single canonical tab for this
            // path; release any read-only preview chip that would duplicate it.
            self.model.release_path(&path);
            self.model
                .set_status("Focused existing editor buffer; no duplicate opened");
            cx.notify();
            return;
        }
        if self.editor.is_none() {
            let mut tabs = EditorTabs::new(self.draft_store());
            match tabs.open_file(&root, &path) {
                Ok(effect) => {
                    self.model.set_status(effect.status());
                    self.install_editor(tabs, cx);
                    self.model.release_path(&path);
                    self.center_view = CenterView::Editor;
                    self.release_primary_center_overlays(PaneSlot::Primary);
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
                self.model.release_path(&path);
                self.center_view = CenterView::Editor;
                self.release_primary_center_overlays(self.mixed_panes.focused_pane());
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

    fn new_editor_in_pane(&mut self, pane: PaneSlot, cx: &mut Context<Self>) {
        if pane == PaneSlot::Secondary && !self.mixed_panes.has_secondary() {
            self.model
                .set_status("Split a tab before creating a file in the second pane");
            cx.notify();
            return;
        }
        self.explorer_preview_active = false;
        let root = self.model.root().to_path_buf();
        let mut opened = false;
        if self.editor_for_pane(pane).is_none() {
            let mut tabs = EditorTabs::new(self.draft_store());
            match tabs.open_scratch(&root) {
                Ok(effect) => {
                    self.model.set_status(effect.status());
                    match pane {
                        PaneSlot::Primary => self.install_editor(tabs, cx),
                        PaneSlot::Secondary => self.install_secondary_editor(tabs, cx),
                    }
                    self.center_view = CenterView::Editor;
                    opened = true;
                }
                Err(error) => self
                    .model
                    .set_status(format!("Cannot create an editor: {error}")),
            }
        } else if let Some(owner) = self.editor_for_pane(pane) {
            let result = owner.update(cx, |editor, cx| editor.open_scratch(&root, cx));
            match result {
                Ok(effect) => {
                    self.model.set_status(effect.status());
                    self.center_view = CenterView::Editor;
                    opened = true;
                }
                Err(error) => self
                    .model
                    .set_status(format!("Cannot create an editor: {error}")),
            }
        }
        if opened {
            self.release_primary_center_overlays(pane);
        }
        self.mixed_panes.focus_pane(pane);
        self.sync_mixed_tabs(cx);
        self.save_mixed_pane_layout(cx);
        cx.notify();
    }

    fn new_editor(&mut self, cx: &mut Context<Self>) {
        self.new_editor_in_pane(self.mixed_panes.focused_pane(), cx);
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
        if self.worktree_create_input.is_some() {
            self.worktree_create_input = None;
            self.worktree_create_root = None;
            self.worktree_create_from_existing = false;
        } else {
            self.begin_worktree_for_project(self.model.root().to_path_buf(), cx);
        }
        self.worktree_delete_pending = None;
        cx.notify();
    }

    fn begin_worktree_for_project(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.worktree_busy || self.mcp_workspace_create_busy {
            return;
        }
        let project_root = self
            .project_tree_groups
            .iter()
            .find(|group| {
                group.root == root || group.worktrees.iter().any(|entry| entry.path == root)
            })
            .map(|group| group.root.clone())
            .unwrap_or(root);
        if WorktreeManager::discover(&project_root).is_err() {
            self.model
                .set_status("Only Git projects can create a Worktree");
            cx.notify();
            return;
        }
        self.worktree_create_root = Some(project_root);
        self.worktree_create_from_existing = false;
        self.worktree_create_input =
            Some(cx.new(|cx| QuickOpenInput::with_placeholder("feature/new-branch", cx)));
        self.show_project_management = true;
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
        let root = self
            .worktree_create_root
            .clone()
            .unwrap_or_else(|| self.model.root().to_path_buf());
        let use_existing = self.worktree_create_from_existing;
        let registered = self
            .runtime_catalog
            .as_ref()
            .and_then(|catalog| catalog.projects.iter().find(|project| project.root == root))
            .map(|project| project.id.clone());
        if self.runtime_bridge.is_some() && self.runtime_catalog.is_none() {
            self.model.set_status(
                "Cannot verify Classic registry; refresh projects before creating a Worktree",
            );
            cx.notify();
            return;
        }
        let bridge = self.runtime_bridge.clone();
        let generation = self.workspace_generation;
        self.worktree_busy = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    if let Some(project_id) = registered {
                        let bridge = bridge.ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotConnected,
                                "Classic Host required for managed worktree",
                            )
                        })?;
                        bridge.create_managed_workspace(&project_id, &root, &branch, use_existing)
                    } else {
                        WorktreeManager::discover(&root).and_then(|manager| {
                            if use_existing {
                                manager.create_existing(&branch)
                            } else {
                                manager.create(&branch)
                            }
                        })
                    }
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
                        view.worktree_create_root = None;
                        view.worktree_create_from_existing = false;
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
        // Removal affects only the selected, inactive managed checkout.
        // The Git WorktreeManager independently refuses a dirty TARGET,
        // primary/active worktree, locked checkout or unmanaged path.
        // Dirty buffers in this unrelated current Workspace are not a
        // reason to refuse removal of a clean, inactive worktree.
        if self.model.root().starts_with(&target) {
            self.model
                .set_status("Cannot remove the active Worktree; switch Workspace first");
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
        self.refresh_review_browser_url();
        self.center_view = CenterView::Terminal;
        self.git_history_tab_open = false;
        self.git_history_sidebar_open = false;
        self.git_history_open = false;
        self.git_graph_center_open = false;
        self.git_branch_picker_mode = None;
        self.git_branch_picker_input.update(cx, |input, cx| input.clear(cx));
        self.mixed_tabs
            .reset(self.workspace_generation.wrapping_add(1));
        self.mixed_panes.reset();
        self.mixed_tab_context = None;
        self.tree = WorkspaceTree::new_with_options(&canonical, self.explorer_visibility).ok();
        self.explorer_multi_selection = ExplorerMultiSelection::new(&canonical);
        // Never inherit a remote worktree's vertical list position.
        self.explorer_scroll.scroll_to_item(0);
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
        self.content_query = Some(cx.new(|cx| {
            QuickOpenInput::with_placeholder("Search in files…", cx)
        }));
        self.content_search_scope = WorktreeContentScope::Current;
        self.content_worktrees_searched = 0;
        self.content_worktrees_skipped = 0;
        self.content_loading = false;
        self.content_matches.clear();
        self.content_error = None;
        self.content_truncated = false;
        self.content_searched_query.clear();
        self.content_request = self.content_request.wrapping_add(1);
        self.references_request = self.references_request.wrapping_add(1);
        self.references_symbol = None;
        self.references_matches.clear();
        self.references_loading = false;
        self.references_error = None;
        self.references_truncated = false;
        self.git_snapshot = None;
        self.git_original_tabs.reset();
        self.git_original_active = false;
        self.git_original_request = self.git_original_request.wrapping_add(1);
        self.git_selected_changes.clear();
        self.git_selected_folders.clear();
        self.git_selection_anchor = None;
        self.git_discard_armed = None;
        self.git_branch_delete_armed = None;
        self.git_trash_preview = None;
        self.git_trash_loading = false;
        self.git_trash_request = self.git_trash_request.wrapping_add(1);
        self.git_sidebar_options.collapsed_sections.clear();
        self.git_sidebar_options.collapsed_folders.clear();
        self.git_loading = false;
        self.git_refresh_gate.reset();
        self.git_history = None;
        self.git_graph_render_cache = None;
        self.git_history_loading = false;
        self.git_history_watch_pending = false;
        self.git_history_error = None;
        self.git_history_options = GitHistoryOptions::default();
        self.git_graph_ref_tab = GitGraphRefTab::Local;
        if let Some(store) = &self.session_store {
            match store.load_git_favorites(&canonical) {
                Ok(favorites) => self.git_history_options.favorites = favorites,
                Err(error) => {
                    self.git_history_error =
                        Some(format!("Native Git favorites cannot be loaded: {error}"))
                }
            }
        }
        self.git_favorites_busy = false;
        self.git_favorites_request = self.git_favorites_request.wrapping_add(1);
        self.git_favorites_last_refresh = Instant::now()
            .checked_sub(Duration::from_secs(20))
            .unwrap_or_else(Instant::now);
        self.git_history_request = self.git_history_request.wrapping_add(1);
        self.git_history_cancel.store(true, Ordering::Release);
        self.git_amend_preview = None;
        self.git_amend_loading = false;
        self.git_amend_request = self.git_amend_request.wrapping_add(1);
        self.git_merge_armed = None;
        self.git_graph_remote_armed = None;
        self.git_commit_ref_target = None;
        self.git_commit_ref_armed = None;
        self.git_commit_apply_preview = None;
        self.git_commit_apply_armed = false;
        self.git_commit_apply_loading = false;
        self.git_commit_apply_request = self.git_commit_apply_request.wrapping_add(1);
        self.git_soft_reset_preview = None;
        self.git_soft_reset_armed = false;
        self.git_soft_reset_loading = false;
        self.git_soft_reset_request = self.git_soft_reset_request.wrapping_add(1);
        self.git_rebase_preview = None;
        self.git_rebase_armed = false;
        self.git_rebase_loading = false;
        self.git_rebase_request = self.git_rebase_request.wrapping_add(1);
        self.git_worktree_commit_target = None;
        self.git_worktree_commit_armed = None;
        self.git_worktree_commit_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_rename_source = None;
        self.git_rename_armed = None;
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
        self.git_graph_search_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_commit_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_commit_draft_request = self.git_commit_draft_request.wrapping_add(1);
        self.git_commit_draft_loading = false;
        self.git_ai_prompt_request = self.git_ai_prompt_request.wrapping_add(1);
        self.git_ai_prompt_preview = None;
        self.git_ai_prompt_loading = false;
        self.cancel_ai_commit_generation();
        self.git_branch_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_rename_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_commit_ref_input
            .update(cx, |input, cx| input.clear(cx));
        self.git_diff = None;
        self.git_branch_comparison = None;
        self.git_branch_compare_loading = false;
        self.git_diff_loading = false;
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        self.git_file_diff_paging = None;
        self.center_git_diff_source = None;
        self.center_git_diff_hunk = None;
        self.center_git_diff_rows = Arc::new(Vec::new());
        // Agent profiles are Host-owned, but selection and an in-flight
        // launch response must never attach to a different GPUI workspace.
        self.agent_profiles_request = self.agent_profiles_request.wrapping_add(1);
        self.agent_profiles_loading = false;
        self.agent_launch_request = self.agent_launch_request.wrapping_add(1);
        self.agent_launch_busy = false;
        self.agent_pending_tab = None;
        self.agent_launch_pane = None;
        self.agent_prompt_input
            .update(cx, |input, cx| input.clear(cx));
        self.agent_status = "Select an Agent Profile and enter a task".into();
        self.mcp_host_workspaces.clear();
        self.mcp_host_workspaces_loading = false;
        self.mcp_host_workspaces_request = self.mcp_host_workspaces_request.wrapping_add(1);
        self.workspace_generation = self.workspace_generation.wrapping_add(1);
        self.project_settings_request = self.project_settings_request.wrapping_add(1);
        self.project_settings_mutation_request =
            self.project_settings_mutation_request.wrapping_add(1);
        self.project_settings_project_id = None;
        self.project_settings_config = None;
        self.project_settings_selected_project_id = None;

        self.project_settings_rows.clear();
        self.project_settings_source = None;
        self.project_settings_repo_error = false;
        self.project_settings_use_repo_confirmed = false;

        self.project_settings_editing = false;
        self.project_settings_loading = false;
        self.project_settings_save_busy = false;
        self.project_settings_save_status.clear();
        self.project_settings_error = None;
        self.project_settings_draft_provider = None;
        self.project_settings_copy_inputs.clear();
        self.project_settings_setup_inputs.clear();
        self.project_settings_prompt_input
            .update(cx, |input, cx| input.clear(cx));
        self.host_pin_snapshots.clear();
        self.host_pins_loading = false;
        self.host_pins_busy = false;
        self.host_pins_request = self.host_pins_request.wrapping_add(1);
        self.host_pins_error = None;
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
        self.terminals
            .set_scrollback_lines(self.native_settings.terminal_scrollback_lines as usize);
        self.terminal_auto_resize.clear();
        self.terminal_line_selection = None;
        self.terminal_panes.reset();
        self.terminal_create_flow.cancel();
        self.terminal_create_pane = None;
        self.restore_host_create_journal();
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
        self.start_git_watch(cx);
        self.refresh_git(cx);
        self.refresh_shared_git_favorites(cx);
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

    fn sidebar_layout_snapshot(&self) -> NativeSidebarLayout {
        NativeSidebarLayout {
            projects_collapsed: self.projects_sidebar_collapsed,
            context_visible: self.right_sidebar_visible,
            projects_width: self.projects_sidebar_width,
            context_width: self.right_sidebar_width,
            context_is_git: self.right_sidebar_view == RightSidebarView::Git,
            context_is_search: self.right_sidebar_view == RightSidebarView::Search,
            context_is_references: self.right_sidebar_view == RightSidebarView::References,
            context_is_pull_requests: self.right_sidebar_view == RightSidebarView::PullRequests,
        }
    }

    /// Persist on final drag or click, never on every mouse-move frame.
    /// While a write is in flight, the latest UI state is coalesced and
    /// saved after that write returns. All unrelated settings stay intact.
    fn save_sidebar_layout(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_save_busy { return; }
        let next = self.sidebar_layout_snapshot();
        let expected = self.native_settings.sidebar_layout.clone();
        if next == expected { return; }
        let Some(store) = self.native_settings_store.clone() else {
            self.model.set_status("Native sidebar state cannot be persisted: no settings store");
            return;
        };
        self.sidebar_save_busy = true;
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                store.compare_and_set_sidebar_layout(&expected, &next)
            }).await;
            view.update(cx, |view, cx| {
                view.sidebar_save_busy = false;
                match result {
                    Ok(settings) => {
                        view.native_settings.sidebar_layout = settings.sidebar_layout;
                        // A newer click/drag may have arrived during the write.
                        if view.sidebar_layout_snapshot() != view.native_settings.sidebar_layout {
                            view.save_sidebar_layout(cx);
                        }
                    }
                    Err(error) => view.model.set_status(format!(
                        "Native sidebar settings not saved: {error}; restart to reload another window's state"
                    )),
                }
                cx.notify();
            }).ok();
        }).detach();
    }

    fn install_native_settings(&mut self, settings: NativeSettings, cx: &mut Context<Self>) {
        if self.sidebar_resize_drag.is_none() && !self.sidebar_save_busy {
            self.projects_sidebar_collapsed = settings.sidebar_layout.projects_collapsed;
            self.projects_sidebar_width = settings.sidebar_layout.projects_width;
            self.right_sidebar_visible = settings.sidebar_layout.context_visible;
            self.right_sidebar_width = settings.sidebar_layout.context_width;
            self.right_sidebar_view = if settings.sidebar_layout.context_is_pull_requests {
                RightSidebarView::PullRequests
            } else if settings.sidebar_layout.context_is_references {
                RightSidebarView::References
            } else if settings.sidebar_layout.context_is_search {
                RightSidebarView::Search
            } else if settings.sidebar_layout.context_is_git {
                RightSidebarView::Git
            } else {
                RightSidebarView::Explorer
            };
        }
        let visibility = ExplorerVisibilityOptions {
            show_hidden: settings.explorer_show_hidden,
            show_ignored: settings.explorer_show_ignored,
        };
        if self.explorer_visibility != visibility {
            if let Some(tree) = &mut self.tree {
                if let Err(error) = tree.set_visibility_options(visibility) {
                    self.native_settings_error = Some(format!(
                        "Explorer could not apply Rust Host preferences: {error}"
                    ));
                    cx.notify();
                    return;
                }
            }
            self.explorer_visibility = visibility;
            self.explorer_multi_selection.clear();
            self.explorer_selection = None;
            self.explorer_context_menu = None;
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
        }
        if self.native_settings.auto_git_fetch_enabled != settings.auto_git_fetch_enabled
            || self.native_settings.auto_git_fetch_interval_minutes
                != settings.auto_git_fetch_interval_minutes
        {
            self.last_auto_git_fetch = Instant::now();
        }
        let tab_size = settings.editor_tab_size;
        self.terminals
            .set_scrollback_lines(settings.terminal_scrollback_lines as usize);
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
    /// and acknowledges shared scalar settings only after Rust Host readback.
    /// Shared settings have one owner: Rust Runtime Host. The historical
    /// Native JSON store is migration-only and must never answer MCP get or
    /// update requests, including read-only safety and extended preferences.
    fn apply_native_mcp_setting(&mut self, command: &NativeMcpUiCommand, cx: &mut Context<Self>) {
        self.apply_shared_mcp_setting(command.clone(), cx);
    }

    fn change_native_setting(
        &mut self,
        path: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        self.change_shared_desktop_setting(path, value, cx);
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
                // Reuse the Host's already-authorized external-editor choice,
                // but independently enforce the workspace boundary and line/
                // column arguments before acknowledging a process spawn.
                let mode = args
                    .get("openMode")
                    .and_then(|value| value.as_str())
                    .unwrap_or("reuse");
                let editor_choice = args
                    .get("editor")
                    .and_then(|value| value.as_str())
                    .unwrap_or("alera");
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
                if !matches!(mode, "reuse" | "new") || (editor_choice != "alera" && mode != "reuse")
                {
                    return Err("ui_not_ready");
                }
                if matches!(editor_choice, "zed" | "vscode") {
                    match alera_gpui_desktop::git_external_open::launch_mcp_external_editor(
                        self.model.root(),
                        relative_path,
                        editor_choice,
                        line,
                        column,
                    ) {
                        Ok(()) => {
                            self.model.set_status(format!(
                                "MCP: opened {} in {}",
                                relative_path.display(),
                                if editor_choice == "zed" {
                                    "Zed"
                                } else {
                                    "VS Code"
                                },
                            ));
                            cx.notify();
                            return Ok(None);
                        }
                        Err(error) => {
                            self.model
                                .set_status(format!("MCP external editor unavailable: {error}"));
                            cx.notify();
                            return Err("ui_not_ready");
                        }
                    }
                }
                if editor_choice != "alera" {
                    return Err("ui_not_ready");
                }
                let already_open = self.editor_instance_for_path(&target_file, cx).is_some();
                // "new" is a real new file-backed Tab when this path has no
                // editor. Reopening an already-editable path as another
                // mutable buffer is not safe in Native's two-pane document
                // ownership model; reject rather than fake a new Tab.
                let requires_new = match native_mcp_file_open_requires_new(mode, already_open) {
                    Ok(create) => create,
                    Err(code) => {
                        if mode == "new" && already_open {
                            self.model.set_status(
                                "This file already has a writable Editor tab. Native cannot create a second independent writable buffer; use openMode=reuse.",
                            );
                            cx.notify();
                        }
                        return Err(code);
                    }
                };
                if !self.model.open_path(&target_file) {
                    return Err("file_missing");
                }
                self.open_editor(cx);
                let Some(editor) = self.focused_editor() else {
                    return Err("ui_not_ready");
                };
                if editor.read(cx).document().path() != Some(target_file.as_path())
                    || self.center_view != CenterView::Editor
                    || (requires_new && self.editor_instance_for_path(&target_file, cx).is_none())
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
        self.git_history_sidebar_open = true;
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
        self.git_history_sidebar_open = true;
        self.git_file_diff_paging = None;
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

    /// Classic's New Tab menu lists installed Agent CLIs from PATH without
    /// needing a Host connection. Keep this slow scan off the UI thread.
    /// Inventory is machine-wide. Request IDs reject stale scan results.
    fn refresh_installed_agent_clis(&mut self, cx: &mut Context<Self>) {
        if self.installed_agent_clis_loading {
            return;
        }
        self.installed_agent_clis_loading = true;
        self.installed_agent_clis_request = self.installed_agent_clis_request.wrapping_add(1);
        let request = self.installed_agent_clis_request;
        let paths = self
            .native_settings
            .agent_executable_paths
            .clone()
            .into_iter()
            .collect::<HashMap<_, _>>();
        cx.spawn(async move |view, cx| {
            let found = cx
                .background_spawn(async move { detect_installed_agent_clis(&paths) })
                .await;
            view.update(cx, |view, cx| {
                if view.installed_agent_clis_request != request {
                    return;
                }
                view.installed_agent_clis_loading = false;
                view.installed_agent_clis = found;
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn refresh_agent_profiles(&mut self, cx: &mut Context<Self>) {
        if self.agent_profiles_loading {
            return;
        }
        self.agent_profile_removal_preview = None;
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
                        let routing_is_current =
                            view.agent_profile_routing_loaded.as_ref().is_some_and(
                                |(id, revision)| {
                                    view.agent_selected_profile.as_deref() == Some(id.as_str())
                                        && view.agent_profiles.iter().any(|row| {
                                            row.id == *id && row.revision == Some(*revision)
                                        })
                                },
                            );
                        if !routing_is_current {
                            view.agent_profile_routing_loaded = None;
                            view.agent_profile_description_input
                                .update(cx, |input, cx| input.clear(cx));
                            view.agent_profile_quota_group_input
                                .update(cx, |input, cx| input.clear(cx));
                            if view.settings_section.as_deref() == Some("agentProfiles")
                                && !view.agent_profile_mutation_busy
                            {
                                view.load_selected_agent_profile_routing(cx);
                            }
                        }
                        let content_is_current = view
                            .agent_profile_content_loaded
                            .as_ref()
                            .is_some_and(|fields| {
                                view.agent_selected_profile.as_deref() == Some(fields.id.as_str())
                                    && view.agent_profiles.iter().any(|row| {
                                        row.id == fields.id
                                            && row.revision == Some(fields.revision)
                                            && row.managed == fields.managed
                                    })
                            });
                        let managed_is_current = view
                            .agent_profile_managed_loaded
                            .as_ref()
                            .is_some_and(|fields| {
                                view.agent_selected_profile.as_deref() == Some(fields.id.as_str())
                                    && view.agent_profiles.iter().any(|row| {
                                        row.id == fields.id
                                            && row.revision == Some(fields.revision)
                                            && row.managed
                                    })
                            });
                        if !managed_is_current {
                            view.agent_profile_discovery_request =
                                view.agent_profile_discovery_request.wrapping_add(1);
                            view.agent_profile_discovery_busy = false;
                            view.agent_profile_discovered_models.clear();
                            view.agent_profile_discovered_personas.clear();
                            view.agent_profile_discovery_status.clear();
                            view.agent_profile_managed_loaded = None;
                            view.agent_profile_model_input
                                .update(cx, |input, cx| input.clear(cx));
                            view.agent_profile_persona_input
                                .update(cx, |input, cx| input.clear(cx));
                        }
                        if !content_is_current {
                            view.agent_profile_content_loaded = None;
                            view.agent_profile_command_input
                                .update(cx, |input, cx| input.clear(cx));
                            view.agent_profile_custom_prompt_input
                                .update(cx, |input, cx| input.clear(cx));
                        }
                    }
                    Err(error) => {
                        view.agent_profiles.clear();
                        view.agent_selected_profile = None;
                        view.agent_profile_routing_loaded = None;
                        view.agent_profile_content_loaded = None;
                        view.agent_profile_managed_loaded = None;
                        view.agent_profile_discovery_request =
                            view.agent_profile_discovery_request.wrapping_add(1);
                        view.agent_profile_discovery_busy = false;
                        view.agent_profile_discovered_models.clear();
                        view.agent_profile_discovered_personas.clear();
                        view.agent_profile_discovery_status.clear();
                        view.agent_profile_model_input
                            .update(cx, |input, cx| input.clear(cx));
                        view.agent_profile_persona_input
                            .update(cx, |input, cx| input.clear(cx));
                        view.agent_profile_command_input
                            .update(cx, |input, cx| input.clear(cx));
                        view.agent_profile_custom_prompt_input
                            .update(cx, |input, cx| input.clear(cx));
                        view.agent_profile_description_input
                            .update(cx, |input, cx| input.clear(cx));
                        view.agent_profile_quota_group_input
                            .update(cx, |input, cx| input.clear(cx));
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

    /// Classic's installed Agent menu starts a Host-owned PTY for the chosen
    /// adapter immediately. Configured Host Profiles remain a distinct entry.
    fn prepare_detected_agent_in_pane(
        &mut self,
        agent_id: &str,
        target: &PaneAttachGuard,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !target.still_valid(self.workspace_generation, &self.mixed_panes) {
            self.model.set_status("Pane changed; reopen New Agent");
            cx.notify();
            return;
        }
        if !self
            .installed_agent_clis
            .iter()
            .any(|agent| agent.id == agent_id)
        {
            self.model
                .set_status("Agent availability changed; rescan installed Agents");
            cx.notify();
            return;
        }
        self.create_host_agent_in_pane(target.clone(), agent_id.to_owned(), cx);
    }

    fn prepare_agent_in_pane(
        &mut self,
        profile_id: &str,
        target: &PaneAttachGuard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !target.still_valid(self.workspace_generation, &self.mixed_panes)
            || !self
                .agent_profiles
                .iter()
                .any(|profile| profile.id == profile_id)
        {
            self.agent_status = "Pane or Agent Profile changed; reopen + → Agent".into();
            cx.notify();
            return;
        }
        self.agent_launch_pane = Some(target.clone());
        self.agent_selected_profile = Some(profile_id.to_owned());
        self.runtime_details_open = true;
        self.agent_status = "Enter an Agent task, then choose Launch Agent".into();
        window.focus(&self.agent_prompt_input.read(cx).focus_handle(cx), cx);
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
        let selected_pane = self.agent_launch_pane.clone().or_else(|| {
            PaneAttachGuard::capture(
                self.workspace_generation,
                self.mixed_panes.focused_pane(),
                &self.mixed_panes,
            )
        });
        let Some(selected_pane) = selected_pane else {
            self.agent_status = "Original pane changed; reopen + → Agent".into();
            cx.notify();
            return;
        };
        if !selected_pane.still_valid(self.workspace_generation, &self.mixed_panes) {
            self.agent_status = "Agent's original pane closed; reopen + → Agent".into();
            cx.notify();
            return;
        }
        self.agent_launch_pane = Some(selected_pane);
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
                let workspace_id = bridge.ensure_host_workspace_for_explicit_profile_launch(&root)?;
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
                        view.agent_launch_pane = None;
                        let guidance = classify_agent_profile_launch_failure(&error).guidance();
                        view.agent_status = format!(
                            "Agent launch not confirmed: {guidance} Check Attach existing before retrying; the Host might have created the tab."
                        );
                        view.refresh_terminals(cx);
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
    }

    /// Read-only GUI recovery: an unconfirmed Host create survives application
    /// restart and workspace switching. Never infer "not created" from a
    /// missing CLI reply or use a saved marker to auto-attach a session.
    fn restore_host_create_journal(&mut self) {
        self.terminal_create_journal = None;
        self.terminal_create_inventory_verified = false;
        let Some(store) = self.session_store.as_ref() else {
            return;
        };
        match store.pending_host_create(self.model.root()) {
            Ok(Some(record)) => {
                if self.terminal_create_flow.restore_outcome_unknown() {
                    self.terminal_create_journal = Some(record);
                    self.terminals.status =
                        "Previous Host create unresolved; inspect existing sessions before acknowledging (no new PTY)"
                            .into();
                    self.model
                        .set_status("Host create interrupted; inspect + → Attach existing");
                }
            }
            Err(error) => {
                self.terminal_create_flow.restore_outcome_unknown();
                self.terminals.status = format!(
                    "Host create recovery record unavailable: {error}. Do not retry; preserve the file"
                );
                self.model
                    .set_status("Host create recovery blocked; inspect local journal");
            }
            Ok(None) => {}
        }
    }

    /// A validated exact Host session proves the create completed. Only then
    /// retire the matching persistent marker; a failed retire blocks retries.
    fn resolve_confirmed_host_create_journal(&mut self) {
        let Some(record) = self.terminal_create_journal.as_ref() else {
            return;
        };
        let Some(store) = self.session_store.as_ref() else {
            self.terminal_create_flow.restore_outcome_unknown();
            self.terminals.status =
                "Host create journal store unavailable; manual recovery required".into();
            return;
        };
        match store.resolve_host_create(self.model.root(), record) {
            Ok(()) => {
                self.terminal_create_journal = None;
                self.terminal_create_inventory_verified = false;
            }
            Err(error) => {
                self.terminal_create_flow.restore_outcome_unknown();
                self.terminals.status =
                    format!("Host created a PTY but recovery marker could not be cleared: {error}");
            }
        }
    }

    fn create_host_terminal_in_pane(&mut self, target: PaneAttachGuard, cx: &mut Context<Self>) {
        self.create_host_terminal_targeted(target, None, cx);
    }

    fn create_host_agent_in_pane(
        &mut self,
        target: PaneAttachGuard,
        agent_id: String,
        cx: &mut Context<Self>,
    ) {
        self.create_host_terminal_targeted(target, Some(agent_id), cx);
    }

    fn create_host_terminal_targeted(
        &mut self,
        target: PaneAttachGuard,
        agent_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !target.still_valid(self.workspace_generation, &self.mixed_panes) {
            self.terminals.status = "Workbench pane changed; reopen the New Tab menu".into();
            cx.notify();
            return;
        }
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
            // A Diff or Editor can hide the terminal-only status panel. The
            // primary status bar must still explain why explicit New Terminal
            // could not create a tab; never start a Host during smoke tests.
            self.terminals.status = "Alera CLI missing; cannot create Host terminal".into();
            self.model.set_status(if agent_id.is_some() {
                "New Agent unavailable: Alera CLI not found"
            } else {
                "New Terminal unavailable: Alera CLI not found"
            });
            cx.notify();
            return;
        };
        let root = self.model.root().to_path_buf();
        let agent_executable = agent_id.as_deref().and_then(|id| {
            self.installed_agent_clis
                .iter()
                .find(|agent| agent.id == id)
                .map(|agent| agent.executable_path.to_string_lossy().into_owned())
        });
        if agent_id.is_some() && agent_executable.is_none() {
            self.model
                .set_status("Installed Agent executable is no longer available; rescan");
            cx.notify();
            return;
        }
        let is_agent = agent_id.is_some();
        let generation = self.workspace_generation;
        // Commit a per-workspace, create_new + fsync journal BEFORE any
        // potentially side-effecting Host startup/create RPC. Even if this
        // window crashes or a second GPUI window races, no blind retry.
        let Some(store) = self.session_store.as_ref() else {
            self.terminals.status =
                "Cannot create Host PTY without durable Native recovery storage".into();
            self.model
                .set_status("Host create refused: persistent recovery storage unavailable");
            cx.notify();
            return;
        };
        let journal = match store.reserve_host_create(&root, agent_id.as_deref()) {
            Ok(record) => record,
            Err(error) => {
                self.restore_host_create_journal();
                self.terminals.status = format!(
                    "Host create blocked by recovery journal: {error}. Check + → Attach existing; no new PTY issued"
                );
                self.model
                    .set_status("Unresolved Host create: inspect existing sessions before retry");
                self.refresh_terminals(cx);
                cx.notify();
                return;
            }
        };

        let Some(request_id) = self.terminal_create_flow.start_request() else {
            return;
        };
        self.terminal_create_journal = Some(journal);
        self.terminal_create_inventory_verified = false;
        self.terminal_create_pane = Some(PendingTerminalPane {
            request_id,
            target,
            expected_agent_id: agent_id.clone(),
        });
        self.terminals.status = if is_agent {
            "Checking registered Host workspace before launching Agent…"
        } else {
            "Checking registered Host workspaces before creating terminal…"
        }
        .into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    // Explicit New Terminal may start the existing CLI Host
                    // and register this *already existing* checkout. Passive
                    // refresh/read/write still never start a Host or PTY.
                    if let Some(agent_id) = agent_id {
                        bridge.create_agent_terminal_for_workspace(
                            &root,
                            &agent_id,
                            agent_executable.as_deref().expect("checked installed executable before creating Host Agent"),
                        )
                    } else {
                        bridge.create_terminal_for_workspace(&root)
                    }
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
                        view.terminals.status = if is_agent {
                            "Host Agent started; confirming session identity (30s limit)…"
                        } else {
                            "Host terminal created; confirming session identity (30s limit)…"
                        }.into();
                        if is_agent {
                            view.model.set_status("Agent starting; confirming Host session identity");
                        }
                        view.refresh_terminals(cx);
                    }
                    Err(error) => {
                        // The pending pane intent must belong to this exact
                        // request, not a stale callback after pane reuse.
                        if !view.terminal_create_pane.as_ref().is_some_and(|pending| {
                            pending.request_id == request_id
                        }) {
                            return;
                        }

                        if !view.terminal_create_flow.mark_request_outcome_unknown(request_id) {
                            return;
                        }
                        view.terminals.status = if error.kind() == std::io::ErrorKind::InvalidData {
                            // The Host RPC may already have created a PTY,
                            // even when the CLI reply failed validation.
                            // Never encourage a second create as a retry.
                            format!(
                                "Host reply unverified: {error}. Inspect Attach running Host session, then dismiss the pending request before another New Terminal."
                            )
                        } else {
                            format!("Host create outcome unknown: {error}. Inspect existing sessions, then dismiss the pending request before another PTY.")
                        };
                        if is_agent {
                            view.model.set_status(
                                "Agent outcome unknown; inspect Host sessions before dismissing request",
                            );
                        }
                        // Re-list existing Host sessions, never create a second PTY.
                        view.refresh_terminals(cx);
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
        if !self.terminal_create_flow.is_active() {
            return;
        }
        if self.terminal_create_flow.is_creating() {
            self.terminals.status =
                "Host create still in progress; wait for its result, do not create a duplicate"
                    .into();
            cx.notify();
            return;
        }
        if !self.terminal_create_inventory_verified {
            self.terminals.status =
                "Cannot dismiss until a fresh authenticated Host session list succeeds".into();
            self.refresh_terminals(cx);
            cx.notify();
            return;
        }
        let (Some(store), Some(record)) = (
            self.session_store.as_ref(),
            self.terminal_create_journal.as_ref(),
        ) else {
            self.terminals.status =
                "Recovery record unavailable; preserve it and inspect manually".into();
            cx.notify();
            return;
        };
        if let Err(error) = store.resolve_host_create(self.model.root(), record) {
            self.terminals.status =
                format!("Cannot clear recovery record: {error}; no new PTY allowed");
            cx.notify();
            return;
        }

        if self.terminal_create_flow.cancel() {
            self.terminal_create_journal = None;
            self.terminal_create_inventory_verified = false;
            self.terminal_create_pane = None;
            // Authenticated Host inventory was checked; acknowledgement
            // retires only the UI marker and never stops a Host PTY.
            self.terminals.status =
                "Acknowledged after Host inventory; existing PTYs remain available via Attach"
                    .into();
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
        let presence_generation = self.agent_presence_event_generation;
        // A list begun BEFORE the Host create reply (still Creating) cannot
        // verify the eventual result, even if its callback arrives later.
        // Bind this read to the exact outstanding marker, not a newer one.
        let create_journal_at_listing = (!self.terminal_create_flow.is_creating())
            .then(|| self.terminal_create_journal.clone())
            .flatten();
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
                // The Host status stream is newer than this async list
                // request; discard its result and read a fresh snapshot.
                if view.agent_presence_event_generation != presence_generation {
                    view.refresh_terminals(cx);
                    return;
                }
                match result {
                    Ok(sessions) => {
                        let was_disconnected = view.terminals.session_list_is_stale()
                            || view.terminals.sessions.is_empty();
                        if view.terminals.apply_session_list(generation, sessions) {
                            // Only an authenticated post-request inventory can
                            // authorize the user's explicit unknown-create acknowledgement.
                            if create_journal_at_listing.as_ref().is_some_and(|journal| {
                                view.terminal_create_flow.is_active()
                                    && view.terminal_create_journal.as_ref() == Some(journal)
                            }) {
                                view.terminal_create_inventory_verified = true;
                            }
                            if was_disconnected && !view.terminals.sessions.is_empty() {
                                view.terminal_poll.reset();
                                view.agent_notification_delivery.on_host_reconnected();
                            }
                            // Never fully reconcile/focus tabs on a periodic
                            // Host status refresh. Change only the Hook chip
                            // metadata; pane order, pins and user focus stay.
                            view.mixed_tabs
                                .refresh_agent_statuses(&view.terminals.sessions);
                            view.observe_agent_notification_status();
                            view.restore_terminal_layout_from_host(cx);
                            view.refresh_host_tab_pins(cx);
                            // Direct Agent PTYs may start before Hooks report
                            // presence. Their typed Host create reply was
                            // corroborated before entering Awaiting, so only
                            // the exact live triple is needed; conflicting
                            // Hook agent types still fail closed. Plain
                            // terminals retain the original identity gate.
                            let direct_agent = view.terminal_create_pane.as_ref()
                                .and_then(|pending| pending.expected_agent_id.as_deref());
                            let confirmed = if let Some(agent_id) = direct_agent {
                                view.terminal_create_flow.confirm_sessions_with_host_verified_agent(
                                    &view.terminals.sessions,
                                    agent_id,
                                )
                            } else {
                                view.terminal_create_flow.confirm_sessions(
                                    &view.terminals.sessions,
                                )
                            };
                            if let Some(created) = confirmed {
                                view.resolve_confirmed_host_create_journal();
                                // Never redirect a delayed Host create result to
                                // whichever pane currently has global focus.
                                // Only the exact authenticated Host triple may
                                // be previewed in the still-existing origin.
                                let pending = view.terminal_create_pane.take();
                                let session = view.terminals.sessions.iter()
                                    .find(|session| created.matches_live_session(session))
                                    .cloned();
                                if let (Some(pending), Some(session)) = (pending, session) {
                                    if !pending.target.still_valid(view.workspace_generation, &view.mixed_panes) {
                                        view.terminals.status =
                                            "Host terminal created, but its pane changed; use + → Attach existing (PTY kept alive)".into();
                                    } else if view.attach_live_terminal_to_pane(
                                        &session, &pending.target, cx,
                                    ).is_none() {
                                        view.terminals.status =
                                            "Host terminal created but Native attach was refused; use + → Attach existing".into();
                                    } else if let Some(agent_id) = pending.expected_agent_id.as_deref() {
                                        // Host PTY input can be ready before
                                        // the provider Hook reports an Agent.
                                        // Update the stale "Agent starting"
                                        // banner only after verified attachment
                                        // to the original pane succeeded.
                                        if let Some(feedback) =
                                            alera_gpui_desktop::terminal::direct_agent_ready_feedback(
                                                &session,
                                                agent_id,
                                            )
                                        {
                                            view.model.set_status(feedback);
                                        }
                                    }
                                } else {
                                    view.terminals.status =
                                        "Host terminal confirmed without a current pane request; use + → Attach existing".into();
                                }
                            }
                            // Launch reply contains a Host workspace+tab id,
                            // not a trustworthy PTY handle. Resolve through
                            // a fresh authenticated session list and check
                            // the selected Profile adapter. Hook status can
                            // remain absent until after provider sign-in.
                            let found_agent = view.agent_pending_tab.as_ref()
                                .and_then(|pending| {
                                    view.agent_profiles.iter()
                                        .find(|profile| profile.id == pending.profile_id)
                                        .and_then(|profile| matching_agent_handle(
                                            pending, &view.terminals.sessions, &profile.agent_type,
                                        ))
                                })
                                .and_then(|handle| view.terminals.sessions.iter()
                                    .find(|session| session.handle == handle && session.running)
                                    .cloned());
                            if let Some(session) = found_agent {
                                let target = view.agent_launch_pane.clone();
                                if let Some(target) = target.filter(|pane| {
                                    pane.still_valid(view.workspace_generation, &view.mixed_panes)
                                }) {
                                    if view.attach_live_terminal_to_pane(&session, &target, cx).is_some() {
                                        view.agent_pending_tab = None;
                                        view.agent_launch_pane = None;
                                        view.agent_status = if session.agent_type.is_some() {
                                            "Host Agent Profile PTY attached; Hook recognized. Provider sign-in not verified."
                                        } else {
                                            "Host Agent Profile PTY attached; Hook not detected. Check Terminal for sign-in instructions."
                                        }.into();
                                        // A running shell/PTY or Hook status is
                                        // not proof of provider authentication.
                                        view.model.set_status(
                                            "Agent Profile terminal ready; provider sign-in not verified"
                                        );
                                    } else {
                                        view.agent_status =
                                            "Agent exists, but preview could not attach; close another tab and Refresh, or use + → Attach existing".into();
                                    }
                                } else {
                                    // The Agent still belongs to Runtime Host.
                                    // Never redirect a stale async reply to
                                    // whichever pane now has keyboard focus.
                                    view.agent_pending_tab = None;
                                    view.agent_launch_pane = None;
                                    view.agent_status =
                                        "Agent created; origin pane closed. Use + → Attach existing to place it manually".into();
                                }
                            }
                        }
                    }
                    Err(error) => {
                        // A failed list must not mark every PTY stopped or
                        // erase the old VT preview. The Host may restart
                        // and re-expose the same running Session later.
                        view.terminals.note_session_list_failure();
                        view.agent_notification_delivery.on_host_reconnected();
                        // A stale Host list cannot substantiate a live
                        // Working/Waiting/Blocked/Done status dot.
                        view.mixed_tabs.refresh_agent_statuses(&[]);
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

    /// Explicit + → Attach and confirmed + → New Terminal share this path.
    /// A menu snapshot with only a recycled handle is never authorization
    /// to attach an unrelated Host tab or steal focus from another pane.
    fn attach_live_terminal_to_pane(
        &mut self,
        expected: &TerminalSession,
        target: &PaneAttachGuard,
        cx: &mut Context<Self>,
    ) -> Option<WorkbenchTabIdentity> {
        if !target.still_valid(self.workspace_generation, &self.mixed_panes) {
            self.terminals.status = "Pane closed or moved; reopen its New Tab menu".into();
            cx.notify();
            return None;
        }
        // An authenticated Host refresh may have invalidated an old pane
        // anchor without painting another GPUI frame yet. Reconcile *before*
        // opening the new local preview so a stale secondary cannot cause
        // that preview to be implicitly appended to Primary.
        self.sync_mixed_tabs(cx);
        if !target.still_valid(self.workspace_generation, &self.mixed_panes) {
            self.terminals.status =
                "Original pane changed; Host session remains available to Attach again".into();
            cx.notify();
            return None;
        }
        if self.terminals.session_list_is_stale()
            || !self
                .terminals
                .sessions
                .iter()
                .any(|current| expected.matches_running_identity(current))
        {
            self.terminals.status =
                "Host session changed or stopped; refresh and choose the exact session again"
                    .into();
            cx.notify();
            return None;
        }
        self.select_terminal(expected.handle.clone(), cx);
        let active_is_expected = self.terminals.active().is_some_and(|tab| {
            tab.handle == expected.handle
                && tab.workspace_id == expected.workspace_id
                && tab.tab_id == expected.tab_id
        });
        if !active_is_expected {
            self.terminals.status =
                "Cannot attach Host preview: terminal limit or session changed".into();
            cx.notify();
            return None;
        }
        self.sync_mixed_tabs(cx);
        if !target.still_valid(self.workspace_generation, &self.mixed_panes) {
            self.terminals.status = "Pane changed before Host attachment completed".into();
            cx.notify();
            return None;
        }
        let active = self
            .terminals
            .active_tab
            .and_then(|index| self.terminals.context_target_at(index))?;
        if !self.terminals.context_target_is_live(&active) {
            self.terminals.status = "Host session is no longer live".into();
            cx.notify();
            return None;
        }
        let identity = WorkbenchTabIdentity::Host(active);
        let pane = target.pane();
        let positioned = if self.mixed_panes.tabs_in(pane).contains(&identity) {
            self.mixed_panes.activate(&identity);
            true
        } else {
            self.mixed_panes.move_to(&identity, pane)
        };
        if !positioned {
            self.terminals.status =
                "Host preview opened, but pane placement failed; choose Attach again".into();
            cx.notify();
            return None;
        }
        self.mixed_tabs.select(&identity);
        self.save_mixed_pane_layout(cx);
        self.refresh_terminal_split_output(cx);
        // Explicit Attach can happen just after the last Host pin read.
        // Query its authoritative state now rather than showing a default
        // unpinned chip until the next periodic session poll.
        self.refresh_host_tab_pins(cx);
        self.terminals.status = format!(
            "Attached Host session in {} pane",
            if pane == PaneSlot::Secondary {
                "second"
            } else {
                "first"
            }
        );
        cx.notify();
        Some(identity)
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
                    Ok(()) => {
                        if let Some(target) = view
                            .terminals
                            .active_tab
                            .and_then(|index| view.terminals.context_target_at(index))
                        {
                            view.terminals.apply_host_accepted_viewport(&target, cols, rows);
                        }
                        format!("Requested {cols}×{rows}; Host may defer while a mobile client holds viewport control")
                    }
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
                    match result {
                        Ok(()) => {
                            // The Host accepted the resize. Expand the local
                            // VT grid as well; otherwise a 24x80 preview keeps
                            // letterboxing in a much larger GPUI pane.
                            if view.terminals.apply_host_accepted_viewport(&target, cols, rows) {
                                cx.notify();
                            }
                        }
                        Err(error) => {
                            view.terminals.status = format!(
                                "Auto Resize {cols}×{rows} refused (no repeated retries until size changes): {error}"
                            );
                            cx.notify();
                        }
                    }
                }).ok();
            }).detach();
        }
    }

    /// Mouse selection is a bounded snapshot of primary VT screen rows;
    /// never forwards mouse/selection gestures to the running Host PTY.
    /// A mixed split owns one terminal surface per Pane; never choose the
    /// globally focused TerminalDock tab for a copy/selection in the other Pane.
    fn terminal_index_in_pane(&self, pane: PaneSlot) -> Option<usize> {
        if self.mixed_panes.has_secondary() {
            self.mixed_panes.active_in(pane).and_then(|id| match id {
                WorkbenchTabIdentity::Host(target) => self.terminals.index_for_context(target),
                _ => None,
            })
        } else if pane == PaneSlot::Primary {
            self.terminals.active_tab
        } else {
            None
        }
    }

    fn begin_terminal_line_selection(&mut self, row: usize, cx: &mut Context<Self>) {
        self.begin_terminal_line_selection_for_pane(PaneSlot::Primary, row, cx);
    }

    fn begin_terminal_line_selection_for_pane(
        &mut self,
        pane: PaneSlot,
        row: usize,
        cx: &mut Context<Self>,
    ) {
        let index = self.terminal_index_in_pane(pane);
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
        let mut completed = false;
        if let Some(selection) = self.terminal_line_selection.as_mut() {
            if selection.is_dragging() {
                selection.finish();
                completed = true;
            }
        }
        if completed {
            // Same opt-in as Classic. Identity/stale-output checks remain in
            // copy_primary_terminal_text, so a changed Host frame cannot
            // leak an old selection into the OS clipboard.
            if self
                .settings_extra_values
                .get("terminal.clipboardOnSelect")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                // The selection can belong to either split Pane. Use the
                // exact Host identity/cursor instead of copying primary by default.
                let pane = self.terminal_line_selection.as_ref().and_then(|selection| {
                    [PaneSlot::Primary, PaneSlot::Secondary]
                        .into_iter()
                        .find(|pane| {
                            self.terminal_index_in_pane(*pane)
                                .and_then(|i| {
                                    self.terminals
                                        .context_target_at(i)
                                        .zip(self.terminals.tabs.get(i))
                                })
                                .is_some_and(|(target, tab)| {
                                    selection.is_current(&target, tab.next_cursor)
                                })
                        })
                });
                if let Some(pane) = pane {
                    self.copy_terminal_text_for_pane(pane, true, cx);
                }
            } else {
                cx.notify();
            }
        }
    }

    fn copy_terminal_text_for_pane(
        &mut self,
        pane: PaneSlot,
        selected_only: bool,
        cx: &mut Context<Self>,
    ) {
        let index = self.terminal_index_in_pane(pane);
        let Some((target, tab)) = index.and_then(|i| {
            self.terminals
                .context_target_at(i)
                .zip(self.terminals.tabs.get(i))
        }) else {
            self.terminals.status = "No visible Host terminal in this Pane to copy".into();
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
    /// The Native VT owns the scrollback viewport; a full-screen TUI on
    /// Alacritty's alternate screen keeps receiving its own PageUp/PageDown.
    fn scroll_native_terminal_page(
        &mut self,
        pane: PaneSlot,
        toward_older: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.terminals.view_mode != TerminalViewMode::Screen {
            return false;
        }
        let Some(index) = self.terminal_index_in_pane(pane) else {
            return false;
        };
        let Some(tab) = self.terminals.tabs.get_mut(index) else {
            return false;
        };
        if tab.screen.alternate_screen() {
            return false;
        }
        let changed = tab.screen.scroll_history_page(toward_older);
        let offset = tab.screen.scrollback_offset();
        if changed {
            self.terminals.status = if offset == 0 {
                "Terminal returned to live output".to_owned()
            } else {
                format!("Terminal viewing history: {offset} lines above live output")
            };
            self.terminal_line_selection = None;
            cx.notify();
        }
        true
    }

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
        // The focused Native IME sink owns provisional Zhuyin text.
        // Both panes must block raw PTY keys and clipboard shortcuts while
        // candidate selection is composing; committed text comes through TSF.
        let composing = match pane {
            PaneSlot::Primary => self.terminal_input.read(cx).composing(),
            PaneSlot::Secondary => self.secondary_terminal_input.read(cx).composing(),
        };
        if !composing
            && !win_ime::focused_native_text_mode()
            && event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.platform
            && matches!(event.keystroke.key.as_str(), "pageup" | "pagedown")
            && self.scroll_native_terminal_page(pane, event.keystroke.key == "pageup", cx)
        {
            return true;
        }
        match terminal_key_dispatch(
            &event.keystroke.key,
            event.keystroke.key_char.as_deref(),
            event.keystroke.modifiers.control,
            event.keystroke.modifiers.shift,
            event.keystroke.modifiers.alt,
            event.keystroke.modifiers.platform,
            composing,
            win_ime::focused_native_text_mode(),
        ) {
            TerminalKeyDispatch::Composing => return true,
            TerminalKeyDispatch::NativeText => return false,
            TerminalKeyDispatch::Clipboard(TerminalClipboardShortcut::Copy) => {
                self.copy_terminal_text_for_pane(pane, false, cx);
                return true;
            }
            TerminalKeyDispatch::Clipboard(TerminalClipboardShortcut::Paste) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    // Respect the active Alacritty TUI's bracketed paste mode.
                    let bracketed = self
                        .terminals
                        .active()
                        .is_some_and(|tab| tab.screen.bracketed_paste());
                    if let Some(payload) = terminal_clipboard_payload(&text, bracketed) {
                        self.enqueue_terminal_text_for_pane(pane, payload, cx);
                    } else {
                        self.terminals.status =
                            "Clipboard text empty, NUL-containing or exceeds 4 KiB Host limit"
                                .into();
                        cx.notify();
                    }
                }
                return true;
            }
            TerminalKeyDispatch::Pty => {}
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

    /// File-backed tabs use checked workspace-relative paths. Unsaved scratch
    /// tabs use a bounded opaque EditorDraftStore recovery ID; the layout
    /// contains no raw scratch text or Host/PTy identity.
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
                for (instance, scratch_id) in editor.read(cx).scratch_tab_instances() {
                    if let Some(key) = scratch_layout_key(&scratch_id) {
                        keys.push((
                            WorkbenchTabIdentity::Editor {
                                editor_epoch: self.editor_epoch,
                                instance,
                            },
                            key,
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
                self.git_history_tab_open = saved.git_history_tab_open;
                self.git_graph_center_open = saved.git_history_tab_active;
                if saved.git_history_tab_open {
                    self.git_history_options.all_branches = saved.git_history_tab_all_branches.unwrap_or(true);
                    self.git_history_options.favorites_only = saved.git_history_tab_favorites_only;
                    self.git_history_options.selected_ref = saved.git_history_tab_selected_ref.clone();
                    self.git_history_options.merged_visibility = saved.git_history_tab_merged_visibility
                        .as_deref().and_then(GitMergedBranchVisibility::from_key).unwrap_or_default();
                    self.git_history_options.merged_target_ref = saved.git_history_tab_merged_target_ref.clone();
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
        // Do not overwrite saved Host/Editor pane assignments before the
        // first authenticated Host inventory restores their previews.
        if self.mixed_layout_read_only || self.mixed_layout_restore.is_some() {
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
        let host_for = |id: &WorkbenchTabIdentity| match id {
            WorkbenchTabIdentity::Host(target) => Some(SavedWorkbenchHostTab::new(
                target.handle.clone(),
                target.workspace_id(),
                target.tab_id(),
            )),
            _ => None,
        };
        let host_keys = |slot: PaneSlot| {
            self.mixed_panes
                .tabs_in(slot)
                .iter()
                .filter_map(&host_for)
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
            pinned_editors: self
                .mixed_tabs
                .pinned()
                .iter()
                .filter_map(&key_for)
                .collect(),
            pinned_hosts: self
                .mixed_tabs
                .pinned()
                .iter()
                .filter_map(&host_for)
                .collect(),
            primary_hosts: host_keys(PaneSlot::Primary),
            secondary_hosts: host_keys(PaneSlot::Secondary),
            primary_active_host: self
                .mixed_panes
                .active_in(PaneSlot::Primary)
                .and_then(&host_for),
            secondary_active_host: self
                .mixed_panes
                .active_in(PaneSlot::Secondary)
                .and_then(&host_for),
            git_history_tab_open: self.git_history_tab_open,
            git_history_tab_secondary: self.mixed_panes.tabs_in(PaneSlot::Secondary)
                .contains(&WorkbenchTabIdentity::GitHistory),
            git_history_tab_pinned: self.mixed_tabs.is_pinned(&WorkbenchTabIdentity::GitHistory),
            git_history_tab_active: self.git_graph_center_open,
            git_history_tab_all_branches: self.git_history_tab_open.then_some(self.git_history_options.all_branches),
            git_history_tab_favorites_only: self.git_history_options.favorites_only,
            git_history_tab_selected_ref: self.git_history_options.selected_ref.clone(),
            git_history_tab_merged_visibility: (self.git_history_options.merged_visibility
                != GitMergedBranchVisibility::ShowAll)
                .then(|| self.git_history_options.merged_visibility.key().to_owned()),
            git_history_tab_merged_target_ref: self.git_history_options.merged_target_ref.clone(),
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
        let mut restored_files = Vec::new();
        for key in saved
            .primary_editors
            .iter()
            .chain(saved.secondary_editors.iter())
            .take(MAX_EDITOR_TABS)
        {
            // Scratch draft documents were independently recovered by
            // EditorDraftStore. Their opaque layout keys are never paths
            // and must not trigger filesystem reads or duplicate buffers.
            if key.starts_with(SCRATCH_EDITOR_KEY_PREFIX) {
                continue;
            }
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
                    restored_files.push(path);
                }
            } else {
                let mut tabs = EditorTabs::new(self.draft_store());
                if tabs.open_file(&root, &path).is_ok() {
                    self.install_editor(tabs, cx);
                    restored += 1;
                    restored_files.push(path);
                }
            }
        }
        // A file restored into a real editor buffer is owned by that tab;
        // drop any read-only preview tab that would render as a duplicate.
        for path in restored_files {
            self.model.release_path(&path);
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
        // The first render happens before the asynchronous authenticated Host
        // list. Wait for TerminalLayout to restore its preview identities;
        // otherwise this one-shot layout would discard secondary Host slots.
        if self.terminal_layout_restore.is_some()
            && (!saved.primary_hosts.is_empty() || !saved.secondary_hosts.is_empty())
        {
            self.mixed_layout_restore = Some(saved);
            return;
        }
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
        // Host hints can only reposition already-open tabs verified against
        // the current authenticated Host inventory. They cannot attach or
        // spawn terminals, nor can a recycled handle claim the old pane.
        if !self.terminals.session_list_is_stale() {
            self.mixed_panes.restore_live_host_positions(
                &saved.primary_hosts,
                &saved.secondary_hosts,
                &self.terminals.sessions,
            );
        }
        // Scratch drafts are discovered before file-backed Editors. Reorder
        // both the visible strip and the native Ctrl+Tab sequence only after
        // transferring the actual document owners to their saved panes.
        for (slot, saved_keys) in [
            (PaneSlot::Primary, &saved.primary_editors),
            (PaneSlot::Secondary, &saved.secondary_editors),
        ] {
            let desired = saved_keys
                .iter()
                .filter_map(|wanted| {
                    keys.iter()
                        .find(|(_, key)| key == wanted)
                        .map(|(id, _)| id.clone())
                })
                .collect::<Vec<_>>();
            self.mixed_panes.restore_editor_order(slot, &desired);
            let instances = desired
                .iter()
                .filter_map(|id| match id {
                    WorkbenchTabIdentity::Editor { instance, .. } => Some(*instance),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if let Some(owner) = self.editor_for_pane(slot) {
                owner.update(cx, |editor, cx| {
                    editor.restore_tab_order_by_instances(&instances, cx);
                });
            }
        }
        // Restore Editor pins only for tabs that actually exist in the
        // recovered NativeEditor instances.
        for (id, key) in &keys {
            if saved.pinned_editors.contains(key) {
                self.mixed_tabs.set_pinned(id, true);
            }
        }
        // A Host pin is only a display preference. Validate all three
        // identities against the currently authenticated and running Host
        // inventory before pinning an already-visible preview; never use a
        // saved hint to create or attach a PTY.
        if !self.terminals.session_list_is_stale() {
            for hint in &saved.pinned_hosts {
                let verified = self.terminals.sessions.iter().any(|session| {
                    session.running
                        && session.handle == hint.handle
                        && session.workspace_id == hint.workspace_id
                        && session.tab_id == hint.tab_id
                });
                if !verified {
                    continue;
                }
                let existing = self
                    .mixed_panes
                    .tabs_in(PaneSlot::Primary)
                    .iter()
                    .chain(self.mixed_panes.tabs_in(PaneSlot::Secondary).iter())
                    .find(|id| {
                        matches!(id, WorkbenchTabIdentity::Host(target)
                            if target.handle == hint.handle
                            && target.workspace_id() == hint.workspace_id
                            && target.tab_id() == hint.tab_id)
                    })
                    .cloned();
                if let Some(id) = existing {
                    self.mixed_tabs.set_pinned(&id, true);
                }
            }
        }
        // Restore a view-only Git History tab after reconciling real Host/Editor
        // owners. Its saved identity cannot start a PTY or read editor buffers.
        if saved.git_history_tab_open {
            let graph = WorkbenchTabIdentity::GitHistory;
            if saved.git_history_tab_secondary && saved.split_enabled {
                self.mixed_panes.move_to(&graph, PaneSlot::Secondary);
            }
            if saved.git_history_tab_pinned {
                self.mixed_tabs.set_pinned(&graph, true);
            }
            self.git_history_open = true;
            if self.git_history.is_none() {
                self.refresh_git_history(cx);
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
        } else {
            // Restore each pane's active Host (when it is still authenticated)
            // before the saved focused pane. No Host is created or attached.
            for (slot, wanted) in [
                (PaneSlot::Primary, saved.primary_active_host.as_ref()),
                (PaneSlot::Secondary, saved.secondary_active_host.as_ref()),
            ] {
                if let Some(wanted) = wanted {
                    if let Some(identity) = self
                        .mixed_panes
                        .tabs_in(slot)
                        .iter()
                        .find(|id| {
                            matches!(id, WorkbenchTabIdentity::Host(target)
                            if target.handle == wanted.handle
                            && target.workspace_id() == wanted.workspace_id
                            && target.tab_id() == wanted.tab_id
                            && self.terminals.context_target_is_live(target))
                        })
                        .cloned()
                    {
                        self.mixed_panes.activate(&identity);
                    }
                }
            }
            if saved.focused_pane == SavedWorkbenchPane::Primary {
                self.mixed_panes.focus_pane(PaneSlot::Primary);
            } else {
                self.mixed_panes.focus_pane(PaneSlot::Secondary);
            }
        }
        // Restore the focused Graph only after legacy Host/Editor active-tab
        // restoration, or those later activations can steal its saved pane.
        if saved.git_history_tab_open && saved.git_history_tab_active {
            let graph = WorkbenchTabIdentity::GitHistory;
            self.mixed_panes.activate(&graph);
            self.mixed_tabs.select(&graph);
            self.git_graph_center_open = true;
        }
        self.save_mixed_pane_layout(cx);
    }

    /// Both Classic workbench panes use this same chip behavior. Only
    /// WorkbenchPaneLayout decides which pane renders an identity; this
    /// presentation never owns, clones or terminates Host PTYs or Editors.
    fn render_mixed_tab_chip(
        index: usize,
        tab: WorkbenchTab,
        pane: PaneSlot,
        selected: bool,
        pinned: bool,
        drag_generation: u64,
        host_sessions: &[TerminalSession],
        locale: UiLocale,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let secondary = pane == PaneSlot::Secondary;
        let (chip_id, focus_id, close_id) = if secondary {
            (
                "mixed-secondary-tab",
                "mixed-secondary-tab-focus",
                "mixed-secondary-tab-close",
            )
        } else {
            ("mixed-tab", "mixed-tab-focus", "mixed-tab-close")
        };
        let stopped_label = tab.stopped_label();
        // Classic tooltips describe the live Agent state in the selected
        // language; raw Host IDs are still available in Session details.
        let agent_tooltip = if tab.kind == WorkbenchTabKind::Agent && tab.running {
            tab.agent_state.and_then(|state| {
                tab.verified_agent_brand(host_sessions).map(|agent_type| {
                    AgentVisualState::from_hook(state, tab.interrupted)
                        .tooltip(agent_type, locale)
                })
            })
        } else {
            None
        };
        // Only the exact, still-running Host identity may choose a brand.
        // Generic Terminal/Editor tabs retain their own icon; the separate
        // 6px Hook status dot and stopped label never move or get concealed.
        let tab_icon: gpui::AnyElement = if tab.kind == WorkbenchTabKind::GitHistory {
            icon(UiIcon::Graph, 12.0, if selected { TEXT } else { MUTED }).into_any_element()
        } else if tab.kind == WorkbenchTabKind::Editor {
            explorer_file_icon_element(
                &tab.title, false, false, 12.0,
                if selected { TEXT } else { MUTED },
            ).into_any_element()
        } else if let Some(agent_type) = tab.verified_agent_brand(host_sessions) {
            native_agent_quota_ui::quota_provider_icon(
                &agent_type.trim().to_ascii_lowercase(), 12.0,
            )
        } else {
            icon(UiIcon::Terminal, 12.0, if selected { TEXT } else { MUTED })
                .into_any_element()
        };
        let identity = tab.identity;
        let status_color = if tab.kind == WorkbenchTabKind::Agent && tab.running {
            tab.agent_state
                .map(|state| AgentVisualState::from_hook(state, tab.interrupted).color())
        } else {
            None
        };
        let title = tab.title;
        // The title has its own hover target. Show the same Agent state
        // tooltip there, rather than masking it with a title-only bubble.
        let tab_tooltip_title = agent_tooltip.clone().unwrap_or_else(|| title.clone());
        let for_click = identity.clone();
        let for_middle = identity.clone();
        let for_right = identity.clone();
        let for_close = identity.clone();
        let for_drop = identity.clone();
        let drag_caption = title.clone();
        div()
            .id((chip_id, index))
            .h(px(32.0))
            .mr_2()
            .px(px(TAB_CHIP_HORIZONTAL_PADDING))
            .flex()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(rgb(if selected { BORDER_ACTIVE } else { BORDER }))
            .bg(rgb(if selected { ELEVATED } else { TOOLBAR }))
            .child(
                div()
                    .id((focus_id, index))
                    .when(agent_tooltip.is_some(), |chip| {
                        let text = agent_tooltip.clone().unwrap_or_default();
                        chip.tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(text.clone())
                                .build(window, cx)
                        })
                    })
                    .min_w_0()
                    .max_w(px(210.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .text_color(rgb(if selected { TEXT } else { MUTED }))
                    .child(tab_icon)
                    // Classic reserves the 6px status slot even while idle;
                    // changing Agent states cannot shift the title or X.
                    .child(div().w(px(TAB_ATTENTION_DOT_SIZE)).h(px(TAB_ATTENTION_DOT_SIZE))
                        .when(status_color.is_some(), |dot| {
                            dot.rounded_full().bg(rgb(status_color.unwrap_or(MUTED)))
                        }))
                    .child(
                        div()
                            .id(("mixed-tab-title", index))
                            .min_w_0()
                            .max_w(px(if tab.kind == WorkbenchTabKind::Terminal {
                                92.0
                            } else {
                                180.0
                            }))
                            .truncate()
                            .text_sm()
                            .tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(
                                    tab_tooltip_title.clone(),
                                )
                                .build(window, cx)
                            })
                            .child(title),
                    )
                    .when(tab.dirty, |entry| entry.child(div().text_xs().child("●")))
                    .when(pinned, |entry| entry.child(icon(UiIcon::Pin, 12.0, MUTED)))
                    .when(stopped_label.is_some(), |entry| {
                        entry.child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(stopped_label.unwrap_or("")),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus_mixed_tab(&for_click, window, cx);
                    }))
                    .on_mouse_up(
                        gpui::MouseButton::Middle,
                        cx.listener(move |this, _, _, cx| {
                            this.close_mixed_tab(&for_middle, cx);
                        }),
                    )
                    .on_drag(
                        MixedTabDrag {
                            identity,
                            workspace_generation: drag_generation,
                        },
                        move |_, _, _, cx| {
                            cx.new(|_| TerminalTabDragPreview {
                                label: drag_caption.clone(),
                            })
                        },
                    )
                    .on_drop(cx.listener(move |this, drag: &MixedTabDrag, window, cx| {
                        this.reorder_mixed_tab(drag, &for_drop, window, cx);
                    }))
                    .context_menu(Self::kit_mixed_tab_context_menu(
                        cx.entity().downgrade(),
                        for_right,
                        secondary,
                        pinned,
                    )),
            )
            .child(
                tab_close_button((close_id, index), tr(locale, "Close tab"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_mixed_tab(&for_close, cx);
                    })),
            )
            .into_any_element()
    }

    /// Project both original state owners into one stable, mixed-order tab
    /// strip. The original editor buffers and Host PTYs remain authoritative.
    fn sync_mixed_tabs(&mut self, cx: &mut Context<Self>) {
        let mut observed = Vec::new();
        let mut preferred = None;
        // A stale Host inventory may retain the last VT preview, but must
        // never resurrect working/waiting/done Hook markers on a repaint.
        let inventory_current = !self.terminals.session_list_is_stale();
        for (index, tab) in self.terminals.tabs.iter().enumerate() {
            let Some(target) = self.terminals.context_target_at(index) else {
                continue;
            };
            let identity = WorkbenchTabIdentity::Host(target);
            let agent_session = self.terminals.sessions.iter().find(|session| {
                session.handle == tab.handle
                    && session.workspace_id == tab.workspace_id
                    && session.tab_id == tab.tab_id
            });
            let is_agent = agent_session.is_some_and(|session| session.agent_type.is_some());
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
                agent_state: if is_agent && inventory_current && tab.running {
                    agent_session
                        .filter(|session| session.running)
                        .and_then(|session| session.agent_state)
                } else {
                    None
                },
                interrupted: is_agent
                    && inventory_current
                    && tab.running
                    && agent_session.is_some_and(|session| session.running && session.interrupted),
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
                        agent_state: None,
                        interrupted: false,
                    });
                }
            }
        }
        if self.git_history_tab_open {
            observed.push(WorkbenchTab {
                identity: WorkbenchTabIdentity::GitHistory,
                kind: WorkbenchTabKind::GitHistory,
                title: "Git Graph".into(),
                dirty: false,
                running: true,
                agent_state: None,
                interrupted: false,
            });
            if self.git_graph_center_open {
                preferred = Some(WorkbenchTabIdentity::GitHistory);
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
            WorkbenchTabIdentity::GitHistory => self.git_history_tab_open,
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
        // The temporary read-only Diff only covers the primary viewport.
        // Selecting a primary Editor/Terminal tab restores it immediately,
        // while interactions with the secondary pane can continue alongside.
        if self.mixed_panes.tabs_in(PaneSlot::Primary).contains(id) {
            self.center_git_diff_source = None;
            self.center_git_diff_hunk = None;
        }
        self.mixed_tab_context = None;
        self.git_graph_center_open = false;
        self.explorer_preview_active = false;
        self.git_original_active = false;
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
            WorkbenchTabIdentity::GitHistory => {
                self.git_graph_center_open = true;
                if !self.git_history_open {
                    self.git_history_open = true;
                    self.refresh_git_history(cx);
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
            WorkbenchTabIdentity::GitHistory => {
                self.git_history_tab_open = false;
                self.git_graph_center_open = false;
                self.git_history_open = self.git_history_sidebar_open;
                if !self.git_history_open {
                    self.git_history_cancel.store(true, Ordering::Release);
                    self.git_history_request = self.git_history_request.wrapping_add(1);
                    self.git_history_loading = false;
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

    /// Match Classic's trailing Tab-strip drop target. A same-pane drop
    /// appends the existing identity; a cross-pane drop transfers the real
    /// Editor owner (or moves the read-only Graph/Host tab identity) first.
    /// Never duplicate PTYs, discard drafts, or apply stale-workspace drags.
    fn drop_mixed_tab_on_strip_tail(
        &mut self,
        drag: &MixedTabDrag,
        destination: PaneSlot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if drag.workspace_generation != self.workspace_generation
            || !self.mixed_tab_is_current(&drag.identity, cx)
            || !self.mixed_tabs.contains(&drag.identity)
        {
            return;
        }
        let already_here = self.mixed_panes.tabs_in(destination).contains(&drag.identity);
        if !already_here && !self.transfer_editor_to_pane(&drag.identity, destination, cx) {
            return;
        }
        let pane_reordered = self.mixed_panes.move_to_end(&drag.identity, destination);
        let global_reordered = self.mixed_tabs.move_to_end(&drag.identity);
        self.focus_mixed_tab(&drag.identity, window, cx);
        if !already_here {
            self.refresh_terminal_split_output(cx);
        }
        if pane_reordered || global_reordered || !already_here {
            self.save_mixed_pane_layout(cx);
            cx.notify();
        }
    }

    fn reorder_mixed_tab(
        &mut self,
        drag: &MixedTabDrag,
        target: &WorkbenchTabIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if drag.workspace_generation != self.workspace_generation
            || !self.mixed_tab_is_current(&drag.identity, cx)
            || !self.mixed_tab_is_current(target, cx)
        {
            return;
        }
        let destination = if self
            .mixed_panes
            .tabs_in(PaneSlot::Secondary)
            .contains(target)
        {
            PaneSlot::Secondary
        } else if self.mixed_panes.tabs_in(PaneSlot::Primary).contains(target) {
            PaneSlot::Primary
        } else {
            return;
        };
        let source_in_destination = self
            .mixed_panes
            .tabs_in(destination)
            .contains(&drag.identity);
        if !source_in_destination {
            let moved = if drag.identity == WorkbenchTabIdentity::GitHistory {
                self.mixed_panes.move_to(&drag.identity, destination)
            } else {
                self.transfer_editor_to_pane(&drag.identity, destination, cx)
            };
            if !moved { return; }
        }
        let pane_reordered = self.mixed_panes.reorder_within(&drag.identity, target);
        let global_reordered = self.mixed_tabs.move_to(&drag.identity, target);
        if pane_reordered || global_reordered || !source_in_destination {
            // Pane membership alone does not change TerminalDock.active_tab.
            // If a Host Agent is dragged across panes, synchronize the real
            // TUI output/input owner immediately, just like clicking its tab.
            // Otherwise the old Terminal can remain on screen under the
            // moved Agent chip until the next manual focus change.
            let dropped_focus = self
                .mixed_panes
                .post_cross_pane_drop_focus(&drag.identity, !source_in_destination);
            if let Some(identity) = dropped_focus {
                self.focus_mixed_tab(&identity, window, cx);
            } else {
                self.save_mixed_pane_layout(cx);
            }
            if !source_in_destination {
                self.refresh_terminal_split_output(cx);
            }
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
            MixedContextAction::TogglePin => match &id {
                WorkbenchTabIdentity::Host(target) => {
                    self.toggle_host_tab_pinned(target.clone(), cx);
                }
                WorkbenchTabIdentity::Editor { .. } | WorkbenchTabIdentity::GitHistory => {
                    let next = !self.mixed_tabs.is_pinned(&id);
                    if self.mixed_tabs.set_pinned(&id, next) {
                        self.save_mixed_pane_layout(cx);
                    }
                }
            },
            MixedContextAction::Close => self.close_mixed_tab(&id, cx),
            MixedContextAction::CloseOthers | MixedContextAction::CloseToRight => {
                for target in self.mixed_panes.close_unpinned_candidates(
                    &id,
                    matches!(action, MixedContextAction::CloseToRight),
                    self.mixed_tabs.pinned(),
                ) {
                    // Editor dirty-buffer protection and Host PTY ownership
                    // are enforced by the original owners for every close.
                    self.close_mixed_tab(&target, cx);
                }
            }
            MixedContextAction::MoveLeft | MixedContextAction::MoveRight => {
                if let Some(other) = self
                    .mixed_panes
                    .adjacent_tab(&id, matches!(action, MixedContextAction::MoveRight))
                {
                    self.mixed_panes.reorder_within(&id, &other);
                    self.mixed_tabs.move_to(&id, &other);
                }
            }
            MixedContextAction::SecondPane => {
                // Host stays in its Host-owned runtime; Editor transfers its
                // existing EditorSession into an independent native viewport.
                let moved = if id == WorkbenchTabIdentity::GitHistory {
                    self.mixed_panes.move_to(&id, PaneSlot::Secondary)
                } else {
                    self.transfer_editor_to_pane(&id, PaneSlot::Secondary, cx)
                };
                if moved {
                    self.focus_mixed_tab(&id, window, cx);
                    self.refresh_terminal_split_output(cx);
                }
            }
            MixedContextAction::FirstPane => {
                let moved = if id == WorkbenchTabIdentity::GitHistory {
                    self.mixed_panes.move_to(&id, PaneSlot::Primary)
                } else {
                    self.transfer_editor_to_pane(&id, PaneSlot::Primary, cx)
                };
                if moved {
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

    fn refresh_agent_hook_switches(&mut self, cx: &mut Context<Self>) {
        if self.agent_hook_switches_loading || self.agent_hook_write_loading {
            return;
        }
        self.agent_hook_switches_last_refresh = Instant::now();
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_hook_switches = None;
            self.agent_notification_delivery.on_host_reconnected();
            return;
        };
        self.agent_hook_switches_loading = true;
        self.agent_hook_switches_request = self.agent_hook_switches_request.wrapping_add(1);
        let request = self.agent_hook_switches_request;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.agent_hook_settings_versioned_snapshot() })
                .await;
            view.update(cx, |view, cx| {
                if view.agent_hook_switches_request != request {
                    return;
                }
                view.agent_hook_switches_loading = false;
                match result {
                    Ok(switches) => {
                        if view
                            .agent_hook_switches
                            .as_ref()
                            .map(|state| &state.switches)
                            != Some(&switches.switches)
                        {
                            view.agent_notification_delivery.on_host_reconnected();
                        }
                        view.agent_hook_switches = Some(switches);
                    }
                    Err(_) => {
                        // Older Host/CLI, malformed readback or disconnected:
                        // never infer which hooks were enabled.
                        view.agent_hook_switches = None;
                        view.agent_notification_delivery.on_host_reconnected();
                    }
                }
                view.observe_agent_notification_status();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A Provider toggle is never optimistic. Require a Host-authenticated
    /// versioned snapshot and exactly one in-flight CAS; any conflict or CLI
    /// incompatibility invalidates Native's local permission cache.
    fn change_agent_hook_setting(
        &mut self,
        agent: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if !matches!(agent, "codex" | "claude") || self.agent_hook_write_loading {
            return;
        }
        let Some(observed) = self.agent_hook_switches.clone() else {
            self.agent_hook_ui_status = "Host Hook settings unavailable; refresh required".into();
            cx.notify();
            return;
        };
        if observed.switches.get(agent).copied().unwrap_or(false) == enabled {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_hook_ui_status = "Existing Runtime Host unavailable".into();
            cx.notify();
            return;
        };
        self.agent_hook_write_loading = true;
        self.agent_hook_ui_status = format!("Saving {agent} Hook through Runtime Host…");
        // Discard any earlier in-flight read from before this CAS.
        self.agent_hook_switches_request = self.agent_hook_switches_request.wrapping_add(1);
        self.agent_hook_switches_loading = false;
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    bridge.agent_hook_settings_compare_and_set(&observed, agent, enabled)
                })
                .await;
            view.update(cx, |view, cx| {
                view.agent_hook_write_loading = false;
                match result {
                    Ok(snapshot) => {
                        view.agent_hook_switches = Some(snapshot);
                        view.agent_hook_ui_status =
                            format!("Runtime Host confirmed {agent} Hook setting");
                    }
                    Err(_) => {
                        view.agent_hook_switches = None;
                        view.agent_hook_ui_status =
                            "Host Hook CAS rejected or unavailable; refresh and retry".into();
                    }
                }
                // No stale Waiting/Blocked notice may survive an operational
                // Hook toggle, even when the Host changes the provider set.
                view.agent_notification_delivery.on_host_reconnected();
                view.observe_agent_notification_status();
                view.agent_hook_switches_last_refresh = Instant::now()
                    .checked_sub(Duration::from_secs(30))
                    .unwrap_or_else(Instant::now);
                view.refresh_agent_hook_switches(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn native_agent_notification_policy(&self) -> AgentNotificationPolicy {
        // The runtime Host is the only settings owner. Missing, stale or
        // incompatible settings fail closed, never invent an opt-in.
        let enabled = self.settings_shared_error.is_none()
            && self
                .settings_extra_values
                .get("agents.agentStatusNotificationsEnabled")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
        let include_finished = self
            .settings_extra_values
            .get("agents.agentStatusFinishedNotificationsEnabled")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let enabled_sessions = enabled_notice_sessions(
            &self.terminals.sessions,
            self.agent_hook_switches
                .as_ref()
                .map(|state| &state.switches),
        );
        let has_hook = !self.terminals.session_list_is_stale()
            && enabled_sessions
                .iter()
                .any(|s| s.running && s.agent_type.is_some() && s.agent_state.is_some());
        // Classic currently owns production Toast delivery. Only an
        // explicitly isolated *debug* process may exercise the Windows
        // presenter; release builds can never silently duplicate Classic.
        let manual_test_owner = cfg!(all(windows, debug_assertions))
            && std::env::var_os("ALERA_GPUI_AGENT_TOAST_EXCLUSIVE_TEST")
                == Some(std::ffi::OsString::from("1"));
        AgentNotificationPolicy {
            hooks_enabled: has_hook,
            notifications_enabled: enabled,
            include_finished,
            native_owns_delivery: manual_test_owner,
        }
    }

    fn observe_agent_notification_status(&mut self) {
        let policy = self.native_agent_notification_policy();
        let now_ms = self.agent_notification_clock.elapsed().as_millis() as u64;
        let enabled_sessions = enabled_notice_sessions(
            &self.terminals.sessions,
            self.agent_hook_switches
                .as_ref()
                .map(|state| &state.switches),
        );
        self.agent_notification_delivery
            .observe(&enabled_sessions, now_ms, policy);
    }

    fn flush_agent_notification_delivery(&mut self, cx: &mut Context<Self>) {
        self.observe_agent_notification_status();
        let policy = self.native_agent_notification_policy();
        let now_ms = self.agent_notification_clock.elapsed().as_millis() as u64;
        let enabled_sessions = enabled_notice_sessions(
            &self.terminals.sessions,
            self.agent_hook_switches
                .as_ref()
                .map(|state| &state.switches),
        );
        let notices = self
            .agent_notification_delivery
            .drain(&enabled_sessions, now_ms, policy);
        if !notices.is_empty() {
            #[cfg(windows)]
            if let Err(error) = self.agent_windows_toast.show(&notices) {
                // Do not report a toast as delivered when Windows rejects
                // Native AUMID registration/notification permission.
                self.agent_status = format!("Windows Agent notice unavailable: {error}");
                cx.notify();
            }
        }
        // Toast.Show() can succeed before Windows reports an asynchronous
        // delivery error. Never silently present that as a delivered notice.
        #[cfg(windows)]
        if let Some(error) = self.agent_windows_toast.take_failure() {
            self.agent_status = format!("Windows Agent notice failed: {error}");
            cx.notify();
        }
        #[cfg(windows)]
        if let Some(clicked) = self.agent_windows_toast.take_activation() {
            self.agent_toast_activation = Some(clicked);
            self.agent_toast_activation_retries = 0;
            cx.notify();
        }
    }

    fn activate_agent_notice(
        &mut self,
        notice: AgentNotice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Clicking a WinRT notification never gives the Host permission to
        // start/terminate a PTY. Cross-workspace changes must use the normal
        // confirmed selection path; fail closed until it is implemented.
        // The COM-launched Native process starts with no Host inventory.
        // An empty list is not proof the clicked session has gone away.
        // Wait for the authenticated async read, counting actual attempts,
        // not GPUI render frames (which can complete before the CLI read).
        if self.terminal_list_loading {
            self.agent_toast_activation = Some(notice);
            return;
        }
        if self.terminals.session_list_is_stale() || self.terminals.sessions.is_empty() {
            if self.agent_toast_activation_retries < 3 {
                self.agent_toast_activation_retries += 1;
                self.agent_toast_activation = Some(notice);
                self.refresh_terminals(cx);
            } else {
                self.terminals.status =
                    "Cannot verify Agent notice: Host session list is unavailable".into();
                cx.notify();
            }
            return;
        }
        if validated_notice_target(&notice, &self.terminals.sessions).is_none() {
            self.terminals.status = "Agent notice is stale; Host session no longer exists".into();
            cx.notify();
            return;
        }
        let current_workspace =
            select_registered_host_workspace(self.model.root(), &self.mcp_host_workspaces).ok();
        if current_workspace.as_deref() != Some(notice.workspace_id.as_str()) {
            // An OS toast click is an explicit request to navigate to the
            // Agent. Use the exact authenticated Host workspace ID, never
            // guess from the repo name or toast payload, and reuse the normal
            // draft-safe workspace switch.
            if self.mcp_host_workspaces_loading {
                self.agent_toast_activation = Some(notice);
                return;
            }
            match resolve_registered_host_workspace_path(
                &notice.workspace_id,
                &self.mcp_host_workspaces,
            ) {
                Ok(target_root) => {
                    if self.model.root() != target_root.as_path() {
                        self.switch_workspace(target_root.clone(), cx);
                        if self.model.root() != target_root.as_path() {
                            self.terminals.status =
                                "Agent notice cannot switch workspace; save your draft first"
                                    .into();
                            cx.notify();
                            return;
                        }
                    }
                    // The new workspace reads Host sessions and restores pane
                    // layout asynchronously. Focus only after those are fresh.
                    self.agent_toast_activation = Some(notice);
                    self.agent_toast_activation_retries = 0;
                    cx.notify();
                }
                Err(_)
                    if self.mcp_host_workspaces.is_empty()
                        && self.agent_toast_activation_retries < 3 =>
                {
                    if self.runtime_bridge.is_none() {
                        self.runtime_bridge = RuntimeCliBridge::discover().ok();
                    }
                    if self.runtime_bridge.is_some() {
                        self.agent_toast_activation_retries += 1;
                        self.agent_toast_activation = Some(notice);
                        self.refresh_mcp_host_workspaces(cx);
                    } else {
                        self.terminals.status =
                            "Cannot navigate Agent notice: Alera Host CLI unavailable".into();
                        cx.notify();
                    }
                }
                Err(error) => {
                    self.terminals.status =
                        format!("Cannot navigate Agent notice to registered workspace: {error}");
                    cx.notify();
                }
            }
            return;
        }
        if !self.terminals.tabs.iter().any(|tab| {
            tab.handle == notice.session_id
                && tab.workspace_id == notice.workspace_id
                && tab.tab_id == notice.tab_id
        }) {
            // Only opens a local view of a previously verified running Host
            // session. Native never creates/starts a Shell here.
            self.select_terminal(notice.session_id.clone(), cx);
            self.sync_mixed_tabs(cx);
        }
        let target = validated_notice_pane_target(
            &notice,
            &self.terminals.sessions,
            &self.mixed_tabs,
            &self.mixed_panes,
        );
        if let Some((id, _pane)) = target {
            self.agent_toast_activation_retries = 0;
            self.focus_mixed_tab(&id, window, cx);
        } else {
            self.terminals.status = "Agent Host tab is no longer available".into();
            cx.notify();
        }
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
                StreamViewUpdate::AgentPresenceChanged { changes } => {
                    // Strict redacted Host status event, not a PTY update.
                    self.agent_presence_event_generation =
                        self.agent_presence_event_generation.wrapping_add(1);
                    if let Some(changes) = changes {
                        if self.terminals.apply_agent_presence_changes(&changes) {
                            self.mixed_tabs
                                .refresh_agent_statuses(&self.terminals.sessions);
                            self.observe_agent_notification_status();
                            repaint = true;
                        }
                        // Never auto-attach or create any Host session on
                        // account of an unrecognized/recycled event.
                    } else {
                        // Older Hosts publish invalidation without deltas.
                        self.refresh_terminals(cx);
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
            self.agent_notification_delivery.on_host_reconnected();
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
        // One bounded Git invalidation timer for the entire Workbench,
        // independent of Terminal sessions and invisible when idle.
        cx.spawn(async move |view, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            if view.update(cx, |view, cx| view.pump_git_watch(cx)).is_err() {
                break;
            }
        })
        .detach();
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
        // Auto-resize gets its own single per-Workbench debounce poll. Don't
        // make users see the old 80x24 VT grid until the unrelated 1.5s
        // housekeeping timer fires (a common normal new-Tab UX gap).
        cx.spawn(async move |view, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            if view
                .update(cx, |view, cx| view.flush_auto_terminal_resizes(cx))
                .is_err()
            {
                break;
            }
        })
        .detach();
        // Housekeeping retains its existing 1.5s interval and is independent
        // of Terminal resize and the fast output reader.
        cx.spawn(async move |view, cx| loop {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            if view
                .update(cx, |view, cx| {
                    view.terminal_tick = view.terminal_tick.wrapping_add(1);
                    view.poll_native_mcp_commands(cx);
                    // Classic and GPUI may remain open simultaneously.
                    // Poll a narrow six-field Host snapshot off the UI
                    // thread: fast while Settings is open, bounded otherwise.
                    let settings_refresh_secs =
                        alera_gpui_desktop::agent_quota::shared_quota_pin_poll_seconds(
                            view.settings_section.is_some(), view.quota_popover_open,
                        );
                    if view.settings_shared_last_refresh.elapsed()
                        >= Duration::from_secs(settings_refresh_secs)
                    {
                        view.refresh_shared_desktop_settings(cx);
                    }
                    let hook_refresh_secs =
                        if view.settings_section.is_some() { 3 } else { 15 };
                    if !view.agent_hook_switches_loading
                        && view.agent_hook_switches_last_refresh.elapsed()
                            >= Duration::from_secs(hook_refresh_secs)
                    {
                        view.refresh_agent_hook_switches(cx);
                    }
                    let quota_interval = if view.quota_error.is_some() { 45 }
                        else { alera_gpui_desktop::agent_quota::REFRESH_INTERVAL_SECS };
                    if !view.quota_loading
                        && view.quota_last_refresh.elapsed() >= Duration::from_secs(quota_interval)
                    {
                        view.refresh_agent_quotas(false, None, cx);
                    }
                    let favorites_refresh_secs = if view.git_history_open { 5 } else { 15 };
                    if view.git_favorites_last_refresh.elapsed()
                        >= Duration::from_secs(favorites_refresh_secs)
                    {
                        view.refresh_shared_git_favorites(cx);
                    }
                    if view.native_settings.editor_autosave_enabled {
                        let debounce = view.settings_extra_values
                            .get("editor.autosaveDelaySeconds")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(1);
                        for editor in [view.editor.clone(), view.secondary_editor.clone()].into_iter().flatten() {
                            editor.update(cx, |editor, cx| editor.autosave_if_idle(debounce, cx));
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
                    if alera_gpui_desktop::terminal::agent_presence_refresh_due(
                        view.terminal_tick,
                        &view.terminals.sessions,
                    ) {
                        view.refresh_terminals(cx);
                    }
                    view.flush_agent_notification_delivery(cx);
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
        if let Some(notice) = self.agent_toast_activation.take() {
            // Toast clicks arrive on a WinRT callback thread. Resolve them
            // on GPUI's actual Window context, after layout is available;
            // verify Host session identity again before focusing anything.
            let owner = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                if let Some(workbench) = owner.upgrade() {
                    workbench.update(cx, |view, cx| {
                        view.activate_agent_notice(notice, window, cx);
                    });
                }
            });
        }
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
        let terminal_font_family = self
            .settings_extra_values
            .get("terminal.fontFamily")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("JetBrains Mono")
            .to_owned();
        let terminal_line_height = self
            .settings_extra_values
            .get("terminal.lineHeight")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(1.3) as f32;
        let mixed_tabs = self.mixed_tabs.entries().to_vec();
        let git_original_tabs = self.git_original_tabs.entries().to_vec();
        let active_git_original = if self.git_original_active {
            self.git_original_tabs.active().cloned()
        } else {
            None
        };
        let git_original_is_active = active_git_original.is_some();
        let mixed_secondary_active = self.mixed_panes.active_in(PaneSlot::Secondary).cloned();
        let mixed_primary_active = self.mixed_panes.active_in(PaneSlot::Primary).cloned();
        let graph_in_primary = mixed_primary_active == Some(WorkbenchTabIdentity::GitHistory);
        let graph_in_secondary = mixed_secondary_active == Some(WorkbenchTabIdentity::GitHistory);
        let graph_surface_visible = graph_in_primary || graph_in_secondary;
        let mixed_has_secondary = self.mixed_panes.has_secondary();
        // Classic shows one independent mixed Tab strip per visible pane.
        // The old single shared strip remains the unsplit fallback only.
        let mixed_primary_tabs = if mixed_has_secondary {
            self.mixed_panes
                .visible_tabs_in(PaneSlot::Primary, &mixed_tabs)
        } else {
            mixed_tabs.clone()
        };
        let mixed_secondary_tabs = self
            .mixed_panes
            .visible_tabs_in(PaneSlot::Secondary, &mixed_tabs);
        let pinned_ids = self.mixed_tabs.pinned().to_vec();
        let (primary_pinned_tabs, primary_regular_tabs) =
            self.mixed_tabs.partition_pins(mixed_primary_tabs);
        let (secondary_pinned_tabs, secondary_regular_tabs) =
            self.mixed_tabs.partition_pins(mixed_secondary_tabs);
        let primary_two_rows =
            self.native_settings.editor_two_row_tabs && !primary_pinned_tabs.is_empty();
        let secondary_two_rows =
            self.native_settings.editor_two_row_tabs && !secondary_pinned_tabs.is_empty();
        let shared_pinned_row = primary_pinned_row_visible(
            primary_two_rows,
            secondary_two_rows,
            mixed_has_secondary,
            self.mixed_panes.axis(),
        );
        // Classic only creates a second row when that pane has pinned tabs.
        // In one-row mode keep the pins ahead of the regular scrolling tabs.
        let mixed_primary_tabs = if primary_two_rows {
            primary_regular_tabs
        } else {
            primary_pinned_tabs
                .iter()
                .cloned()
                .chain(primary_regular_tabs)
                .collect()
        };
        let mixed_secondary_tabs = if secondary_two_rows {
            secondary_regular_tabs
        } else {
            secondary_pinned_tabs
                .iter()
                .cloned()
                .chain(secondary_regular_tabs)
                .collect()
        };
        let mixed_split_horizontal = self.mixed_panes.axis() == PaneAxis::Horizontal;
        let mixed_secondary_identity = self.mixed_panes.active_in(PaneSlot::Secondary).cloned();
        let mixed_secondary_host = mixed_secondary_identity.as_ref().and_then(|id| match id {
            WorkbenchTabIdentity::Host(target) => Some(target.clone()),
            WorkbenchTabIdentity::Editor { .. } | WorkbenchTabIdentity::GitHistory => None,
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
                WorkbenchTabIdentity::GitHistory => String::new(),
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
        let showing_explorer_preview = !git_original_is_active
            && (self.explorer_preview_active || self.mixed_tabs.entries().is_empty())
            && self.model.active_document().is_some();
        let center_is_editor = if git_original_is_active {
            true
        } else if showing_explorer_preview {
            true
        } else if active_primary_identity.is_some() {
            primary_is_editor
        } else {
            // Empty workbench fallback only; a real tab selection owns mode.
            self.center_view == CenterView::Editor
        };
        let show_editor_toolbar =
            (center_is_editor || mixed_editor_in_secondary) && !git_original_is_active;
        let editor = self.editor.clone();
        let toolbar_editor = self.focused_editor();
        let has_native_editor =
            toolbar_editor.is_some() && !showing_explorer_preview && !git_original_is_active;
        let secondary_native_editor = if mixed_editor_in_secondary {
            self.secondary_editor.clone()
        } else {
            None
        };
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
        let sidebar_search_input = self.sidebar_search_input.clone();
        let sidebar_search_query = self.sidebar_search_input.read(cx).query().trim().to_owned();
        let sidebar_search_active = !sidebar_search_query.is_empty();
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
            .enumerate()
            .filter(|(_, group)| {
                project_group_matches_search(
                    group,
                    self.runtime_catalog.as_ref(),
                    &sidebar_search_query,
                )
            })
            .map(|(index, group)| {
                // Searching temporarily expands matching descendants without
                // modifying the user's persisted collapse preference.
                let collapsed =
                    !sidebar_search_active && self.collapsed_project_roots.contains(&group.root);
                (index, group.clone(), collapsed)
            })
            .collect();
        let sidebar_search_empty = sidebar_search_active && project_tree_rows.is_empty();
        let project_tree_loading = self.project_tree_loading;
        let runtime_catalog = self.runtime_catalog.clone();
        let runtime_catalog_error = self.runtime_catalog_error.clone();
        let sidebar_pending_action = self.sidebar_pending_action.clone();
        let sidebar_rename_input = self.sidebar_rename_input.clone();
        let sidebar_create_input = self.sidebar_create_input.clone();
        let selected_sidebar_parent = self.sidebar_selected_parent.clone();
        let sidebar_project_remove_id = self
            .sidebar_project_remove_pending
            .as_ref()
            .map(|(id, _)| id.clone());
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
        let worktree_create_root = self.worktree_create_root.clone();
        let worktree_create_from_existing = self.worktree_create_from_existing;
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
        let agent_inventory_current = !self.terminals.session_list_is_stale();
        let agent_state_summary =
            AgentStateSummary::from_sessions(&self.terminals.sessions, agent_inventory_current);
        let agent_ui_locale = self.settings_menu_locale;
        let terminal_sessions: Vec<_> = self
            .terminals
            .sessions
            .iter()
            .map(|session| {
                (
                    session.handle.clone(),
                    // Process presence and Hook state are independent. The
                    // optional colored badge below owns Hook presentation;
                    // the title itself never repeats a stale Hook result.
                    format!(
                        "{} {} · {}",
                        if session.running { "●" } else { "○" },
                        session.agent_type.as_deref().unwrap_or("Terminal"),
                        session.handle,
                    ),
                    session.workspace_id.clone(),
                    // Do not advertise a stale branded session when its
                    // Host PTY is no longer running.
                    session.running.then(|| session.agent_type.clone()).flatten(),
                    AgentVisualState::for_session(session, agent_inventory_current),
                )
            })
            .collect();
        let terminal_create_busy = self.terminal_create_flow.is_active();
        let terminal_create_unknown = self.terminal_create_flow.is_outcome_unknown();
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
                    WorkbenchTabIdentity::Editor { .. } | WorkbenchTabIdentity::GitHistory => None,
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
                    match self.terminals.view_mode {
                        TerminalViewMode::Screen => Some(tab.screen.styled_rows()),
                        TerminalViewMode::Transcript => None,
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
        let terminal_secondary_selected_rows = mixed_secondary_host
            .as_ref()
            .and_then(|target| {
                self.terminals
                    .index_for_context(target)
                    .and_then(|index| self.terminals.tabs.get(index))
                    .and_then(|tab| {
                        self.terminal_line_selection
                            .as_ref()
                            .filter(|selection| selection.is_current(target, tab.next_cursor))
                    })
            })
            .map(|selection| {
                let count = mixed_secondary_vt_rows.as_ref().map(Vec::len).unwrap_or(0);
                (0..count)
                    .map(|row| selection.includes_row(row))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Window geometry is only *observed* by render. The existing 1.5s
        // lifecycle poll coalesces stable changes before the Host RPC.
        // No resizing from inside GPUI's paint/layout phase.
        let viewport = window.viewport_size();
        let window_width = f32::from(viewport.width);
        let initial_context_width = if self.right_sidebar_visible {
            self.right_sidebar_width
        } else {
            PROJECTS_COLLAPSED_WIDTH
        };
        let projects_width = if self.projects_sidebar_collapsed {
            PROJECTS_COLLAPSED_WIDTH
        } else {
            clamp_projects_sidebar_width(self.projects_sidebar_width, window_width, initial_context_width)
        };
        let context_width = if self.right_sidebar_visible {
            clamp_context_sidebar_width(self.right_sidebar_width, window_width, projects_width)
        } else {
            PROJECTS_COLLAPSED_WIDTH
        };
        let content_width =
            (f32::from(viewport.width) - projects_width - context_width - RESIZE_CHROME).max(0.0);
        let content_height = usable_workbench_height(
            f32::from(viewport.height),
            shared_pinned_row,
            show_editor_toolbar
                && !native_center_git_diff_ui::should_show_center_git_diff_view(
                    git_original_is_active,
                    self.center_git_diff_is_current(),
                ),
        );
        let split_width = mixed_has_secondary && mixed_split_horizontal;
        let split_height = mixed_has_secondary && !mixed_split_horizontal;
        let primary_width = content_width / if split_width { 2.0 } else { 1.0 };
        let primary_height = content_height / if split_height { 2.0 } else { 1.0 };
        // The inner Terminal split further reduces the primary VT viewport.
        // Reuse these *visible* dimensions for both PTY rows and the Windows
        // IME cursor anchor; otherwise a candidate popup can cross the seam.
        let (primary_terminal_width, primary_terminal_height) = primary_inner_terminal_render_size(
            primary_width, primary_height, terminal_split_exists, terminal_split_horizontal,
        );
        let secondary_workbench_height = secondary_terminal_render_height(
            primary_height,
            split_height,
            secondary_two_rows,
        );
        // Keep Windows Zhuyin candidate popups beside the visible VT cursor.
        // The 1px GPUI TextInput still captures composition; only its local
        // visual anchor moves, without altering Host PTY geometry.
        let primary_ime_anchor = primary_terminal
            .and_then(|tab| {
                terminal_ime_anchor_with_line_height(
                    tab.screen.cursor(),
                    primary_terminal_width,
                    primary_terminal_height,
                    terminal_font_size,
                    terminal_line_height,
                )
            })
            .unwrap_or((12.0, 12.0));
        let secondary_ime_anchor = mixed_secondary_host
            .as_ref()
            .and_then(|target| self.terminals.index_for_context(target))
            .and_then(|index| self.terminals.tabs.get(index))
            .and_then(|tab| {
                terminal_ime_anchor_with_line_height(
                    tab.screen.cursor(),
                    primary_width,
                    secondary_workbench_height,
                    terminal_font_size,
                    terminal_line_height,
                )
            })
            .unwrap_or((12.0, 12.0));
        let mut proposed_viewports = Vec::new();
        if self.settings_section.is_none() && !center_is_editor {
            if let Some((target, tab)) = current_primary_host.as_ref().zip(primary_terminal) {
                if tab.running && self.terminals.context_target_is_live(target) {
                    let width = primary_terminal_width;
                    let height = primary_terminal_height;
                    if let Some((cols, rows)) = estimate_terminal_viewport_with_line_height(
                        width,
                        height,
                        terminal_font_size,
                        terminal_line_height,
                    ) {
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
                        let height = inner_secondary_terminal_render_height(
                            primary_height, terminal_split_horizontal,
                        );
                        if let Some((cols, rows)) = estimate_terminal_viewport_with_line_height(
                            width,
                            height,
                            terminal_font_size,
                            terminal_line_height,
                        ) {
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
                        let height = secondary_workbench_height;
                        if let Some((cols, rows)) = estimate_terminal_viewport_with_line_height(
                            width,
                            height,
                            terminal_font_size,
                            terminal_line_height,
                        ) {
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
        let content_search_all = self.content_search_scope == WorktreeContentScope::AllWorktrees;
        let content_worktrees_searched = self.content_worktrees_searched;
        let content_worktrees_skipped = self.content_worktrees_skipped;
        let content_results_total = self.content_matches.len();
        let content_no_results = content_results_total == 0
            && !content_loading
            && self.content_error.is_none()
            && !self.content_searched_query.is_empty();
        let content_results: Vec<_> = self
            .content_matches
            .iter()
            .take(80)
            .cloned()
            .enumerate()
            .collect();
        let content_status = if content_loading {
            if content_search_all {
                "Searching registered Git worktrees…".to_string()
            } else {
                "Searching current workspace text…".to_string()
            }
        } else if let Some(error) = &self.content_error {
            error.clone()
        } else if self.content_searched_query.is_empty() {
            "Enter text and click Search".to_string()
        } else {
            format!(
                "{} match(es) · {} worktree(s){}{}{}",
                content_results_total,
                content_worktrees_searched,
                if self.content_truncated {
                    " · partial results"
                } else {
                    ""
                },
                if content_worktrees_skipped > 0 {
                    " · some worktrees skipped"
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
        let reference_count = self.references_matches.len();
        let reference_symbol = self.references_symbol.clone();
        let reference_truncated = self.references_truncated;
        let references_loading = self.references_loading;
        let references_error = self.references_error.clone();
        let reference_status = if references_loading {
            "Scanning saved workspace files for whole-word text matches…".to_owned()
        } else if let Some(error) = references_error {
            error
        } else if reference_symbol.is_none() {
            "Select a symbol in an Editor and choose Find Text References (Shift+F12).".to_owned()
        } else if reference_count == 0 {
            "No textual occurrences found in saved workspace files.".to_owned()
        } else {
            format!(
                "{reference_count} text match(es){}{}",
                if reference_truncated { " · partial results" } else { "" },
                if reference_count > 80 { " · showing first 80" } else { "" },
            )
        };
        let mut prior_reference_path: Option<PathBuf> = None;
        let reference_rows: Vec<_> = self.references_matches.iter()
            .take(80).cloned().enumerate()
            .map(|(i, hit)| {
                let new_file = prior_reference_path.as_ref() != Some(&hit.hit.path);
                prior_reference_path = Some(hit.hit.path.clone());
                (i, hit, new_file)
            })
            .collect();
        let git_filter_input = self.git_filter_input.clone();
        let git_filter = self.git_filter_input.read(cx).query().to_owned();
        let git_all_changes = self
            .git_snapshot
            .as_ref()
            .map(|snapshot| snapshot.visible_rows(""))
            .unwrap_or_default();
        let mut git_sidebar_options = self.git_sidebar_options.clone();
        git_sidebar_options.filter = git_filter;
        let git_sidebar_layout = layout_git_sidebar(&git_all_changes, &git_sidebar_options);
        let git_sidebar_matches = git_sidebar_layout.matches;
        let git_sidebar_total = git_sidebar_layout.total;
        let git_visible_items = Arc::new(git_sidebar_layout.visible_items());
        let git_sidebar_entries = git_sidebar_layout.entries;
        let git_changes = git_sidebar_entries
            .iter()
            .filter_map(|entry| entry.file().cloned())
            .collect::<Vec<_>>();
        let git_tree_mode = self.git_sidebar_options.view == GitSidebarViewMode::Tree;
        let git_group_by_area = self.git_sidebar_options.grouping == GitSidebarGroupMode::ByArea;
        let git_selected = self.git_selected_changes.clone();
        let git_selected_folders = self.git_selected_folders.clone();
        let git_drag_root = workspace_root.clone();
        let git_drag_generation = self.workspace_generation;
        let git_drag_selection = Arc::new(git_visible_items.iter()
            .find(|item| match item {
                GitSidebarSelectionItem::File(file) => git_selected.contains(file),
                GitSidebarSelectionItem::Folder(folder) => git_selected_folders.contains(folder),
            })
            .map(|item| selected_worktree_paths(&git_drag_root, item,
                &git_selected, &git_selected_folders, git_visible_items.as_ref()))
            .unwrap_or_default());
        let mut git_stage_paths = Vec::new();
        let mut git_unstage_paths = Vec::new();
        let mut git_discard_paths = Vec::new();
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
            if row.area == alera_gpui_desktop::git_inspect::GitChangeArea::Changes {
                git_discard_paths.push(row.relative.clone());
            }
        }
        git_stage_paths.sort();
        git_stage_paths.dedup();
        git_unstage_paths.sort();
        git_unstage_paths.dedup();
        git_discard_paths.sort();
        git_discard_paths.dedup();
        let git_discard_confirm = !git_discard_paths.is_empty()
            && self.git_discard_armed.as_ref() == Some(&git_discard_paths);
        let git_selected_count = git_visible_items
            .iter()
            .filter(|item| match item {
                GitSidebarSelectionItem::File(file) => git_selected.contains(file),
                GitSidebarSelectionItem::Folder(folder) => git_selected_folders.contains(folder),
            })
            .count();
        let git_action_busy = self.git_action_busy;
        let git_sensitive_armed = self.git_sensitive_armed;
        let git_action_status = self.git_action_status.clone();
        let git_commit_input = self.git_commit_input.clone();
        let git_commit_draft_loading = self.git_commit_draft_loading;
        let git_ai_prompt_preview = self.git_ai_prompt_preview.clone();
        let git_ai_prompt_loading = self.git_ai_prompt_loading;
        let git_ai_generate_loading = self.git_ai_generate_loading;
        let git_amend_ready = self.git_amend_preview.is_some();
        let git_amend_loading = self.git_amend_loading;
        let git_branch_input = self.git_branch_input.clone();
        let git_rename_source = self.git_rename_source.clone();
        let git_rename_input = self.git_rename_input.clone();
        let git_commit_ref_target = self.git_commit_ref_target.clone();
        let git_commit_ref_input = self.git_commit_ref_input.clone();
        let git_commit_ref_armed = self.git_commit_ref_armed.is_some();
        let git_commit_apply_preview = self.git_commit_apply_preview.clone();
        let git_commit_apply_armed = self.git_commit_apply_armed;
        let git_commit_apply_loading = self.git_commit_apply_loading;
        let git_soft_reset_preview = self.git_soft_reset_preview.clone();
        let git_soft_reset_armed = self.git_soft_reset_armed;
        let git_soft_reset_mode = self.git_soft_reset_mode;
        let git_soft_reset_loading = self.git_soft_reset_loading;
        let git_rebase_preview = self.git_rebase_preview.clone();
        let git_rebase_armed = self.git_rebase_armed;
        let git_rebase_loading = self.git_rebase_loading;
        let git_worktree_commit_target = self.git_worktree_commit_target.clone();
        let git_worktree_commit_input = self.git_worktree_commit_input.clone();
        let git_worktree_commit_armed = self.git_worktree_commit_armed.is_some();
        let git_rename_armed = self.git_rename_armed.is_some();
        let git_graph_open = self.git_history_sidebar_open;
        let git_graph_ref_tab = self.git_graph_ref_tab;
        let git_graph_all_branches = self.git_history_options.all_branches;
        let git_graph_favorites_only = self.git_history_options.favorites_only;
        let git_graph_selected_ref = self.git_history_options.selected_ref.clone();
        let git_graph_hide_merged_lanes = git_graph_all_branches
            && !git_graph_favorites_only
            && git_graph_selected_ref.is_none()
            && self.git_history_options.merged_visibility == GitMergedBranchVisibility::HideMergedGraph;
        let git_graph_file = self.git_history_options.file_path.clone();
        let git_graph_scope = self.git_history_options.git_diff_root.clone();
        let git_graph_full_search = self.git_history_options.search_query.clone();
        let git_graph_full_search_active = git_graph_full_search.is_some();
        let git_graph_search_input = self.git_graph_search_input.clone();
        let git_graph_current_query = self
            .git_graph_search_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        let git_graph_search_pending = git_graph_full_search
            .as_ref()
            .is_some_and(|query| query != &git_graph_current_query);
        let git_branch_compare = self.git_branch_comparison.clone();
        let git_branch_compare_loading = self.git_branch_compare_loading;
        let git_history_more_available = !self.git_history_loading
            && self
                .git_history
                .as_ref()
                .is_some_and(|history| history.truncated && !history.scan_limit_reached)
            && self.git_history_options.max_commits.max(HISTORY_PAGE_STEP) < MAX_HISTORY_PAGE;
        let git_history_status = if self.git_history_loading {
            if git_graph_full_search_active {
                "Searching reachable Git history in background…".to_owned()
            } else {
                "Loading commit graph…".to_owned()
            }
        } else if let Some(history) = &self.git_history {
            if git_graph_full_search_active {
                format!(
                    "{} result(s) · {} commits scanned{}{}",
                    history.commits.len(),
                    history.scanned_commits,
                    if history.scan_limit_reached {
                        " (scan limit reached)"
                    } else if history.truncated {
                        " (more matches available)"
                    } else {
                        ""
                    },
                    if git_graph_search_pending {
                        " · query changed, search again"
                    } else {
                        ""
                    },
                )
            } else {
                format!(
                    "{} branch(es), {} commit(s){}",
                    history.branches.len(),
                    history.commits.len(),
                    if history.truncated { " (limited)" } else { "" }
                )
            }
        } else {
            self.git_history_error
                .clone()
                .unwrap_or_else(|| "Open Git Graph to load history".into())
        };
        let graph_query = GitGraphSearch::parse(
            git_graph_full_search
                .as_deref()
                .unwrap_or(&git_graph_current_query),
        );
        let git_branch_rows = self
            .git_history
            .as_ref()
            .map(|history| {
                history
                    .branches
                    .iter()
                    .filter(|row| git_graph_ref_tab.shows_branch(row) && graph_query.branch(row))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let git_tag_rows = self
            .git_history
            .as_ref()
            .map(|history| {
                history
                    .tags
                    .iter()
                    .filter(|row| git_graph_ref_tab.shows_tag() && graph_query.tag(row))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let git_ref_results_empty =
            self.git_history.is_some() && git_branch_rows.is_empty() && git_tag_rows.is_empty();
        let git_commit_rows = if self.git_history_loading && git_graph_full_search_active {
            Vec::new()
        } else {
            self.git_history
                .as_ref()
                .map(|history| history.display_commits(self.git_history_options.merged_visibility))
                .unwrap_or_default()
        };
        // A sparse historical search is not a contiguous Git Graph. Drawing
        // rails through missing commits would imply nonexistent edges.
        let git_graph_topology = (!git_graph_full_search_active).then(|| {
            layout_git_graph(
                &git_commit_rows
                    .iter()
                    .map(|commit| GraphCommit {
                        oid: commit.oid.clone(),
                        parents: commit.parent_ids.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
        });
        let git_graph_topology_note = if git_graph_full_search_active {
            Some("Search results omit graph rails between skipped commits".to_owned())
        } else {
            git_graph_topology
                .as_ref()
                .and_then(|graph| graph.as_ref().err())
                .map(|error| {
                    format!(
                        "Topology unavailable: {} · showing commit list",
                        error.message()
                    )
                })
        };
        let (git_graph_topology_rows, git_graph_lane_count) = match git_graph_topology
            .and_then(Result::ok)
        {
            Some(layout) => (Some(layout.rows), layout.max_lanes),
            None => (None, 1),
        };
        let git_graph_current_branch = self.git_history.as_ref()
            .map(|history| history.current.clone())
            .unwrap_or_default();
        let git_graph_head_tip = self.git_history.as_ref()
            .and_then(|history| history.branches.iter().find(|branch| branch.current))
            .map(|branch| branch.tip.clone());
        let mut git_graph_tags_by_oid: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(history) = self.git_history.as_ref() {
            // Commit labels currently hold branch tips only. Project real
            // lightweight/annotated tags into the UI without changing Git.
            for tag in &history.tags {
                git_graph_tags_by_oid.entry(tag.oid.clone())
                    .or_default().push(format!("refs/tags/{}", tag.name));
            }
        }
        // Build topology on the full bounded graph before filtering. Filtering
        // first would invent edges/lanes between unrelated commits.
        let git_commit_rows = git_commit_rows
            .into_iter()
            .enumerate()
            // Background search already matched the entire message body and
            // untruncated author. Do not refilter the shortened display row.
            .filter(|(_, row)| git_graph_full_search_active || graph_query.commit(row))
            .collect::<Vec<_>>();
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
        let git_diff_expand_available = native_action_dialog_ui::can_expand_read_only_diff(
            self.git_diff.as_deref(),
            self.git_diff_loading,
        );
        let git_file_page = self.git_file_diff_paging.as_ref().map(|page| {
            (
                page.previous_starts.len() + 1,
                !page.previous_starts.is_empty() && !self.git_diff_loading,
                page.next_line.is_some()
                    && page.previous_starts.len() < 31
                    && !self.git_diff_loading,
            )
        });
        let show_center_git_diff = native_center_git_diff_ui::should_show_center_git_diff_view(
            git_original_is_active,
            self.center_git_diff_is_current(),
        );

        div()
            .size_full()
            .relative()
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                let Some(drag) = this.sidebar_resize_drag else { return; };
                if event.pressed_button != Some(gpui::MouseButton::Left) {
                    this.sidebar_resize_drag = None;
                    this.save_sidebar_layout(cx);
                    cx.notify();
                    return;
                }
                let delta = f32::from(event.position.x) - drag.start_x;
                let viewport = f32::from(window.viewport_size().width);
                match drag.side {
                    SidebarResizeSide::Projects => {
                        let context = if this.right_sidebar_visible {
                            this.right_sidebar_width
                        } else {
                            PROJECTS_COLLAPSED_WIDTH
                        };
                        this.projects_sidebar_width = clamp_projects_sidebar_width(
                            drag.start_width + delta, viewport, context,
                        );
                    }
                    SidebarResizeSide::Context => {
                        let projects = if this.projects_sidebar_collapsed {
                            PROJECTS_COLLAPSED_WIDTH
                        } else {
                            this.projects_sidebar_width
                        };
                        this.right_sidebar_width = clamp_context_sidebar_width(
                            drag.start_width - delta, viewport, projects,
                        );
                    }
                }
                cx.notify();
            }))
            .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.sidebar_resize_drag.take().is_some() {
                    this.save_sidebar_layout(cx);
                    cx.notify();
                }
            }))
            .on_mouse_up_out(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.sidebar_resize_drag.take().is_some() {
                    this.save_sidebar_layout(cx);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(Self::toggle_file_search_action))
            .on_action(cx.listener(Self::toggle_content_search_action))
            .on_action(cx.listener(Self::run_content_search_action))
            .on_action(cx.listener(Self::find_text_references_action))
            .on_action(cx.listener(Self::explorer_copy_action))
            .on_action(cx.listener(Self::explorer_cut_action))
            .on_action(cx.listener(Self::explorer_paste_action))
            .on_action(cx.listener(Self::git_copy_files_action))
            .on_action(cx.listener(Self::git_cut_files_action))
            .on_action(cx.listener(Self::git_paste_files_action))
            .on_action(cx.listener(Self::explorer_select_all_action))
            .on_action(cx.listener(Self::explorer_rename_action))
            .on_action(cx.listener(Self::explorer_delete_action))
            .on_action(cx.listener(Self::explorer_refresh_action))
            .on_action(cx.listener(Self::explorer_up_action))
            .on_action(cx.listener(Self::explorer_down_action))
            .on_action(cx.listener(Self::explorer_first_action))
            .on_action(cx.listener(Self::explorer_last_action))
            .on_action(cx.listener(Self::explorer_open_action))
            .on_action(cx.listener(Self::explorer_preview_action))
            .on_action(cx.listener(Self::explorer_new_file_action))
            .on_action(cx.listener(Self::explorer_new_folder_action))
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
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(gpui::img(classic_brand_logo()).w(px(16.0)).h(px(16.0)))
                            .child(div().font_weight(gpui::FontWeight::BOLD).child("Alera")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("Native"),
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
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_row_reverse()
                    .child(
                        div()
                            .id("native-right-sidebar-scroll")
                            .w(px(context_width))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .when(!self.right_sidebar_visible, |panel| panel.hidden())
                            .bg(rgb(PANEL))
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .id("right-sidebar-view-switch")
                                    // Use the same 44px header baseline as Classic.
                                    .h(px(SECTION_HEADER_HEIGHT))
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .bg(rgb(PANEL))
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    // Classic uses compact, tooltip-backed icon tabs,
                                    // not a large segmented Explorer/Git text control.
                                    .child(icon_button(
                                        "right-sidebar-explorer", UiIcon::Files, "Explorer",
                                        self.right_sidebar_view == RightSidebarView::Explorer,
                                    ).on_click(cx.listener(|view, _, _, cx| {
                                        view.right_sidebar_view = RightSidebarView::Explorer;
                                        view.save_sidebar_layout(cx);
                                        cx.notify();
                                    })))
                                    .child(icon_button(
                                        "right-sidebar-search", UiIcon::Search, "Search",
                                        self.right_sidebar_view == RightSidebarView::Search,
                                    ).on_click(cx.listener(|view, _, window, cx| {
                                        view.open_content_search_panel(window, cx);
                                    })))
                                    .child(icon_button(
                                        "right-sidebar-references", UiIcon::List, "References (textual)",
                                        self.right_sidebar_view == RightSidebarView::References,
                                    ).on_click(cx.listener(|view, _, _, cx| {
                                        view.open_references_panel(cx);
                                    })))
                                    .child(icon_button(
                                        "right-sidebar-git", UiIcon::Branch, "Source Control",
                                        self.right_sidebar_view == RightSidebarView::Git,
                                    ).on_click(cx.listener(|view, _, _, cx| {
                                        view.explorer_context_menu = None;
                                        view.right_sidebar_view = RightSidebarView::Git;
                                        view.save_sidebar_layout(cx);
                                        cx.notify();
                                    })))
                                    .child(icon_button(
                                        "right-sidebar-pr", UiIcon::PullRequest, "Pull Requests (browser)",
                                        self.right_sidebar_view == RightSidebarView::PullRequests,
                                    ).on_click(cx.listener(|view, _, _, cx| {
                                        view.open_pull_requests_panel(cx);
                                    })))
                                    .child(div().flex_1())
                                    .child(icon_button(
                                        "right-sidebar-collapse", UiIcon::ChevronsRight, "Collapse panel", false,
                                    ).on_click(cx.listener(|this, _, _, cx| {
                                        this.right_sidebar_visible = false;
                                        this.save_sidebar_layout(cx);
                                        cx.notify();
                                    }))),
                            )
                            .child(
                                div()
                                    .id("right-explorer-panel")
                                    .flex_1()
                                    .min_h_0()
                                    .track_focus(&self.explorer_keyboard_focus)
                                    // Search input must never inherit Explorer's
                                    // Enter, Delete, or Ctrl+A file actions.
                                    .when(self.right_sidebar_view == RightSidebarView::Explorer, |v| {
                                        v.key_context("AleraGpuiExplorer")
                                    })
                                    .relative()
                                    .flex()
                                    .flex_col()
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                                        if this.explorer_context_menu.take().is_some() { cx.notify(); }
                                    }))
                                    .when(matches!(self.right_sidebar_view, RightSidebarView::Git | RightSidebarView::References | RightSidebarView::PullRequests), |v| v.hidden())
                            .child(
                                div()
                                    .id("right-explorer-header")
                                    .h(px(SECTION_HEADER_HEIGHT))
                                    .px_3()
                                    .flex()
                                    .when(self.right_sidebar_view != RightSidebarView::Explorer, |v| v.hidden())
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
                                                sidebar_icon_button(("explorer-file-action", 0usize), UiIcon::NewFile, tr(self.settings_menu_locale, "New file"), false)
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.begin_explorer_action(ExplorerAction::NewFile, window, cx);
                                                    })),
                                            )
                                            .child(
                                                sidebar_icon_button(("explorer-file-action", 1usize), UiIcon::NewFolder, tr(self.settings_menu_locale, "New folder"), false)
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.begin_explorer_action(ExplorerAction::NewFolder, window, cx);
                                                    })),
                                            )
                                            // Default 315px width: expose Classic's seven icon
                                            // controls in the same order, with the same actions.
                                            // Narrower panels retain all functions in More.
                                            .when(context_width >= 315.0, |toolbar| toolbar
                                                .child(sidebar_icon_button("explorer-save-all", UiIcon::Save,
                                                    tr(self.settings_menu_locale, "Save all editor files"), false)
                                                    .on_click(cx.listener(|this, _, _, cx| this.save_all_editors(cx))))
                                                .child(sidebar_icon_button("explorer-toggle-ignored",
                                                    if self.explorer_visibility.show_ignored { UiIcon::Visible } else { UiIcon::Hidden },
                                                    tr(self.settings_menu_locale, if self.explorer_visibility.show_ignored { "Hide ignored files" } else { "Show ignored files" }),
                                                    self.explorer_visibility.show_ignored)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        let mut next = this.explorer_visibility;
                                                        next.show_ignored = !next.show_ignored;
                                                        this.update_explorer_visibility(next, cx);
                                                    })))
                                                .when(cfg!(windows), |toolbar| toolbar.child(
                                                    sidebar_icon_button("explorer-toggle-hidden",
                                                        if self.explorer_visibility.show_hidden { UiIcon::Visible } else { UiIcon::Hidden },
                                                        tr(self.settings_menu_locale, if self.explorer_visibility.show_hidden { "Hide hidden items" } else { "Show hidden items" }),
                                                        self.explorer_visibility.show_hidden)
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            let mut next = this.explorer_visibility;
                                                            next.show_hidden = !next.show_hidden;
                                                            this.update_explorer_visibility(next, cx);
                                                        }))
                                                ))
                                                .child(sidebar_icon_button("explorer-collapse-all", UiIcon::CollapseAll,
                                                    tr(self.settings_menu_locale, "Collapse all folders"), false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        if let Some(tree) = &mut this.tree {
                                                            if let Err(error) = tree.collapse_all() {
                                                                this.model.set_status(tr(this.settings_menu_locale, "Collapse All failed: {error}").replace("{error}", &error.to_string()));
                                                            }
                                                        }
                                                        this.tree_mode = true;
                                                        cx.notify();
                                                    })))
                                            )
                                            .child(if context_width < 315.0 {
                                                let owner = cx.entity().downgrade();
                                                let ignored = self.explorer_visibility.show_ignored;
                                                let hidden = self.explorer_visibility.show_hidden;
                                                Button::new("explorer-visibility-options")
                                                    .icon(IconName::Ellipsis)
                                                    .tooltip(tr(self.settings_menu_locale, "Show/hide Explorer items"))
                                                    .ghost()
                                                    .dropdown_menu(move |menu, _, cx| {
                                                        let locale = owner
                                                            .upgrade()
                                                            .map(|entity| entity.read(cx).settings_menu_locale)
                                                            .unwrap_or_else(|| UiLocale::from_preference("system"));
                                                        let ignored_owner = owner.clone();
                                                        let mut menu = menu.item(
                                                            PopupMenuItem::new(tr(locale, if ignored {
                                                                "Hide ignored files"
                                                            } else {
                                                                "Show ignored files"
                                                            }))
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
                                                                PopupMenuItem::new(tr(locale, if hidden {
                                                                    "Hide hidden items"
                                                                } else {
                                                                    "Show hidden items"
                                                                }))
                                                                .on_click(move |_, _, cx| {
                                                                    let _ = hidden_owner.update(cx, |view, cx| {
                                                                        let mut next = view.explorer_visibility;
                                                                        next.show_hidden = !next.show_hidden;
                                                                        view.update_explorer_visibility(next, cx);
                                                                    });
                                                                }),
                                                            );
                                                        }
                                                        let save_owner = owner.clone();
                                                        menu = menu.item(PopupMenuItem::new(tr(locale, "Save all editor files"))
                                                            .on_click(move |_, _, cx| {
                                                                let _ = save_owner.update(cx, |view, cx| view.save_all_editors(cx));
                                                            }));
                                                        let collapse_owner = owner.clone();
                                                        menu.item(PopupMenuItem::new(tr(locale, "Collapse all folders"))
                                                            .on_click(move |_, _, cx| {
                                                                let _ = collapse_owner.update(cx, |view, cx| {
                                                                    if let Some(tree) = &mut view.tree {
                                                                        if let Err(error) = tree.collapse_all() {
                                                                            view.model.set_status(tr(view.settings_menu_locale, "Collapse All failed: {error}").replace("{error}", &error.to_string()));
                                                                        }
                                                                    }
                                                                    view.tree_mode = true;
                                                                    cx.notify();
                                                                });
                                                            }))
                                                    }).into_any_element()
                                            } else {
                                                div().h(px(0.0)).into_any_element()
                                            })
                                            .child(
                                                sidebar_icon_button("explorer-refresh", UiIcon::Refresh, tr(self.settings_menu_locale, "Refresh Explorer"), false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.model.refresh_directory();
                                                        if let Some(tree) = &mut this.tree {
                                                            if let Err(error) = tree.refresh() {
                                                                this.model.set_status(tr(this.settings_menu_locale, "Tree refresh failed: {error}").replace("{error}", &error.to_string()));
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
                                    .when(self.right_sidebar_view != RightSidebarView::Explorer, |v| v.hidden())
                                    .px_2()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(explorer_filter_input),
                            )
                            .child(
                                div()
                                    .id("right-search-header")
                                    .h(px(SECTION_HEADER_HEIGHT))
                                    .px_3()
                                    .flex()
                                    .when(self.right_sidebar_view != RightSidebarView::Search, |v| v.hidden())
                                    .items_center()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(div().text_xs().font_weight(gpui::FontWeight::SEMIBOLD).child("SEARCH")),
                            )
                            .child(
                                div()
                                    .id("content-search-panel")
                                    .key_context("AleraGpuiSearch")
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .px_3()
                                    .py_1()
                                    .when(self.right_sidebar_view != RightSidebarView::Search, |panel| panel.hidden())
                                    .child(if let Some(input) = content_query {
                                        div()
                                            .flex_1()
                                            .min_h_0()
                                            .mt_2()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .child(div().flex_1().min_w_0().child(input))
                                                .child(icon_button("content-search-run", UiIcon::Search, "Search selected content scope (Ctrl/Cmd+Enter)", content_loading)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.search_project_contents(cx);
                                                    }))))
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .id("content-scope-current")
                                                            .cursor_pointer()
                                                            .px_2()
                                                            .py_1()
                                                            .text_xs()
                                                            .bg(rgb(if content_search_all { PANEL } else { ACTIVE }))
                                                            .child("Current worktree")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                if this.content_search_scope != WorktreeContentScope::Current {
                                                                    this.content_search_scope = WorktreeContentScope::Current;
                                                                    this.search_project_contents(cx);
                                                                }
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("content-scope-all-worktrees")
                                                            .cursor_pointer()
                                                            .px_2()
                                                            .py_1()
                                                            .text_xs()
                                                            .bg(rgb(if content_search_all { ACTIVE } else { PANEL }))
                                                            .child("All Git worktrees")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                if this.content_search_scope != WorktreeContentScope::AllWorktrees {
                                                                    this.content_search_scope = WorktreeContentScope::AllWorktrees;
                                                                    this.search_project_contents(cx);
                                                                }
                                                            })),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child(content_status),
                                            )
                                            .child(
                                                div()
                                                    .id("content-search-results")
                                                    .flex_1()
                                                    .min_h_0()
                                                    .overflow_y_scroll()
                                                    .children(content_results.into_iter().map(|(i, hit)| {
                                                        let file_name = hit.hit.path.file_name()
                                                            .map(|part| part.to_string_lossy().into_owned())
                                                            .unwrap_or_else(|| hit.hit.path.display().to_string());
                                                        let position = format!("{}:{}", hit.hit.line, hit.hit.column);
                                                        let snippet = hit.hit.snippet.clone();
                                                        let owner_label = hit.label();
                                                        let full_path = format!(
                                                            "{}:{}:{} · {}",
                                                            hit.hit.path.display(), hit.hit.line, hit.hit.column, owner_label
                                                        );
                                                        div()
                                                            .id(("content-search-result", i))
                                                            .cursor_pointer()
                                                            .min_w_0()
                                                            .px_1()
                                                            .py_2()
                                                            .border_b_1()
                                                            .border_color(rgb(BORDER))
                                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                                            .tooltip(move |window, cx| {
                                                                gpui_kit::component::tooltip::Tooltip::new(full_path.clone())
                                                                    .build(window, cx)
                                                            })
                                                            .child(
                                                                div()
                                                                    .min_w_0()
                                                                    .flex()
                                                                    .flex_col()
                                                                    .gap_1()
                                                                    .child(div()
                                                                        .min_w_0()
                                                                        .flex()
                                                                        .items_center()
                                                                        .gap_2()
                                                                        .child(explorer_file_icon_element(&file_name, false, false, 15.0, MUTED))
                                                                        .child(div().min_w_0().flex_1().truncate().text_sm().child(file_name))
                                                                        .child(div().text_xs().text_color(rgb(MUTED)).child(position)))
                                                                    .child(div().min_w_0().truncate().text_xs().child(snippet))
                                                                    .child(div().min_w_0().truncate().text_xs()
                                                                        .text_color(rgb(MUTED)).child(owner_label)),
                                                            )
                                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                                this.open_content_search_hit(&hit, window, cx);
                                                            }))
                                                    }))
                                                    .when(content_no_results, |results| results.child(
                                                        div()
                                                            .px_2()
                                                            .py_3()
                                                            .text_xs()
                                                            .text_color(rgb(MUTED))
                                                            .child("No matching lines in the selected worktree scope"),
                                                    )),
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
                                    .when(self.right_sidebar_view != RightSidebarView::Explorer, |v| v.hidden())
                                    .track_scroll(&self.explorer_scroll)
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
                                                tr(self.settings_menu_locale, "Move {count} Explorer items").replace("{count}", &drag_payload.len().to_string())
                                            } else {
                                                format!("Move {label}")
                                            };
                                            let drop_target = path.clone();
                                            let external_drop_target = path.clone();
                                            let selected = explorer_multi_selection.contains(&path);
                                            let row_icon = explorer_file_icon_element(&label, hit.is_directory, false, 15.0, if selected { TEXT } else { MUTED });
                                            let status_badge = explorer_git_badges.get(&path);
                                            let full_relative = relative.display().to_string();
                                            let parent_label = relative
                                                .parent()
                                                .filter(|p| !p.as_os_str().is_empty())
                                                .map(|p| p.display().to_string())
                                                .unwrap_or_default();
                                            div()
                                                .id(("explorer-filter-result", i))
                                                .px_2()
                                                .h(px(28.0))
                                                .min_w_0()
                                                .tooltip(move |window, cx| {
                                                    gpui_kit::component::tooltip::Tooltip::new(full_relative.clone())
                                                        .build(window, cx)
                                                })
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(row_icon)
                                                .child(div().min_w_0().flex_1().flex().items_center().gap_2()
                                                    .child(div().min_w_0().flex_1().truncate().text_sm().child(label))
                                                    .when(!parent_label.is_empty(), |row| {
                                                        row.child(div().min_w_0().max_w(px(180.0)).truncate()
                                                            .text_xs().text_color(rgb(MUTED)).child(parent_label))
                                                    }))
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
                                                tr(self.settings_menu_locale, "Move {count} Explorer items").replace("{count}", &drag_payload.len().to_string())
                                            } else {
                                                format!("Move {}", item.name)
                                            };
                                            let drop_target = row_path.clone();
                                            let external_drop_target = row_path.clone();
                                            let visible_for_click = explorer_visible_paths.clone();
                                            let selected = explorer_multi_selection.contains(&row_path);
                                            let is_directory = item.is_directory;
                                            let status_badge = explorer_git_badges.get(&row_path);
                                            let row_icon = explorer_file_icon_element(&item.name, is_directory, item.expanded, 15.0, if selected { TEXT } else { MUTED });
                                            let row_label = item.name;
                                            div()
                                                .id(("explorer-tree-row", i))
                                                .pl(px(8.0 + item.depth as f32 * 16.0))
                                                .pr_2()
                                                .h(px(28.0))
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
                                                .child(row_icon)
                                                .child(div().flex_1().min_w_0().truncate().text_sm().child(row_label))
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
                                                tr(self.settings_menu_locale, "Move {count} Explorer items").replace("{count}", &drag_payload.len().to_string())
                                            } else {
                                                format!("Move {}", item.name)
                                            };
                                            let drop_target = file_path.clone();
                                            let external_drop_target = file_path.clone();
                                            let visible_for_click = explorer_visible_paths.clone();
                                            let selected = explorer_multi_selection.contains(&file_path);
                                            let status_badge = explorer_git_badges.get(&file_path);
                                            let row_icon = explorer_file_icon_element(&item.name, is_directory, false, 15.0, if selected { TEXT } else { MUTED });
                                            div()
                                                .id(i)
                                                .pl_2()
                                                .pr_2()
                                                .h(px(28.0))
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(row_icon)
                                                .child(div().flex_1().min_w_0().truncate().text_sm().child(item.name))
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
                                            .child(tr(self.settings_menu_locale, if index_loading { "Indexing workspace files…" } else { "No matching files or folders" }))))
                                    .when(tree_mode && tree_truncated, |view| {
                                        view.child(div().text_color(rgb(MUTED)).child(tr(self.settings_menu_locale, "Tree limited to 4,000 visible items")))
                                    })
                                    .context_menu(Self::kit_explorer_context_menu(
                                        cx.entity().downgrade(),
                                        None,
                                    )),
                            )
                            )
                            .child(
                                div()
                                    .id("right-references-panel")
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .when(self.right_sidebar_view != RightSidebarView::References, |panel| panel.hidden())
                                    .child(
                                        div()
                                            .h(px(SECTION_HEADER_HEIGHT))
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .border_b_1()
                                            .border_color(rgb(BORDER))
                                            .child(div()
                                                .min_w_0()
                                                .flex_1()
                                                .truncate()
                                                .text_xs()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .child(match &reference_symbol {
                                                    Some(symbol) => format!("TEXT REFERENCES · {symbol}"),
                                                    None => "TEXT REFERENCES".into(),
                                                }))
                                            .child(icon_button(
                                                "references-run", UiIcon::Search,
                                                "Find text references at Editor cursor (Shift+F12)", references_loading,
                                            ).on_click(cx.listener(|this, _, _, cx| {
                                                this.find_text_references(cx);
                                            })))
                                            .child(icon_button(
                                                "references-clear", UiIcon::Close, "Clear References", false,
                                            ).on_click(cx.listener(|this, _, _, cx| {
                                                this.clear_text_references(cx);
                                            }))),
                                    )
                                    .child(div().px_3().py_2().text_xs().text_color(rgb(MUTED))
                                        .child("Textual word matches, not language-server references. Saved files only."))
                                    .child(div().px_3().py_2().text_xs().text_color(rgb(MUTED))
                                        .child(reference_status))
                                    .child(
                                        div()
                                            .id("reference-results-scroll")
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_y_scroll()
                                            .children(reference_rows.into_iter().map(|(i, hit, file_header)| {
                                                let path = hit.hit.path.clone();
                                                let file_name = path.file_name()
                                                    .map(|value| value.to_string_lossy().to_string())
                                                    .unwrap_or_else(|| path.display().to_string());
                                                let path_hint = path.display().to_string();
                                                let snippet = hit.hit.snippet.clone();
                                                let position = format!("{}:{}", hit.hit.line, hit.hit.column);
                                                div()
                                                    .id(("reference-row", i))
                                                    .min_w_0()
                                                    .when(file_header, |row| row.child(
                                                        div()
                                                            .id(("reference-file-header", i))
                                                            .px_2().py_2()
                                                            .bg(rgb(PANEL))
                                                            .border_b_1()
                                                            .border_color(rgb(BORDER))
                                                            .flex()
                                                            .items_center()
                                                            .gap_2()
                                                            .child(explorer_file_icon_element(&file_name, false, false, 15.0, MUTED))
                                                            .child(div().min_w_0().flex_1().truncate().text_sm()
                                                                .font_weight(gpui::FontWeight::SEMIBOLD).child(file_name))
                                                            .tooltip(move |window, cx| {
                                                                gpui_kit::component::tooltip::Tooltip::new(path_hint.clone())
                                                                    .build(window, cx)
                                                            })
                                                    ))
                                                    .child(div()
                                                        .id(("reference-match", i))
                                                        .px_3().py_2()
                                                        .cursor_pointer()
                                                        .hover(|style| style.bg(rgb(ACTIVE)))
                                                        .flex()
                                                        .flex_col()
                                                        .gap_1()
                                                        .child(div().text_xs().text_color(rgb(MUTED)).child(position))
                                                        .child(div().min_w_0().truncate().text_xs().child(snippet))
                                                        .on_click(cx.listener(move |this, _, window, cx| {
                                                            this.open_reference_hit(&hit, window, cx);
                                                        })))
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("right-pr-browser-panel")
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .when(self.right_sidebar_view != RightSidebarView::PullRequests, |panel| panel.hidden())
                                    .child(
                                        div()
                                            .h(px(SECTION_HEADER_HEIGHT))
                                            .px_2()
                                            .border_b_1()
                                            .border_color(rgb(BORDER))
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(div().min_w_0().flex_1().text_xs()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .child("PULL REQUESTS"))
                                            .child(icon_button(
                                                "pr-refresh-remote", UiIcon::Refresh, "Refresh Git remote", false,
                                            ).on_click(cx.listener(|this, _, _, cx| {
                                                this.refresh_review_browser_url();
                                                cx.notify();
                                            }))),
                                    )
                                    .child(div().px_3().py_3().text_xs().text_color(rgb(MUTED))
                                        .child("Open the repository's PR/MR page in your browser. Authentication stays in your browser."))
                                    .child(if let Some(url) = self.review_browser_url.clone() {
                                        div()
                                            .px_3().py_2()
                                            .flex().flex_col().gap_2()
                                            .child(div().text_xs().text_color(rgb(MUTED)).child(url))
                                            .child(
                                                div()
                                                    .id("pr-open-browser")
                                                    .cursor_pointer()
                                                    .rounded_md()
                                                    .bg(rgb(ACTIVE))
                                                    .hover(|style| style.bg(rgb(ELEVATED)))
                                                    .px_3().py_2()
                                                    .text_sm()
                                                    .child("Open Pull Requests in browser")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.open_pull_requests_in_browser(cx);
                                                    })),
                                            )
                                            .into_any_element()
                                    } else {
                                        div()
                                            .px_3().py_2()
                                            .text_xs().text_color(rgb(MUTED))
                                            .child(self.review_browser_error.clone().unwrap_or_else(|| {
                                                "No supported remote found; check Git remote settings and refresh".into()
                                            }))
                                            .into_any_element()
                                    })
                                    .child(div().px_3().py_3().text_xs().text_color(rgb(MUTED))
                                        .child("In-app PR listing, review, comments and creation require the Native forge provider controller and are not connected yet.")),
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
                                    .h(px(SECTION_HEADER_HEIGHT))
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(tr(self.settings_menu_locale, "Source Control")),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(icon_button("git-sidebar-flat-tree",
                                                if git_tree_mode { UiIcon::File } else { UiIcon::Folder },
                                                if git_tree_mode { "Show flat Git changes list" } else { "Show Git changes tree" },
                                                git_tree_mode)
                                                .on_click(cx.listener(|this, _, _, cx| this.toggle_git_sidebar_view(cx))))
                                            .child(icon_button("git-sidebar-group-toggle", UiIcon::Graph,
                                                if git_group_by_area { "Show all Git changes together" } else { "Group Git changes by stage" },
                                                git_group_by_area)
                                                .on_click(cx.listener(|this, _, _, cx| this.toggle_git_sidebar_group(cx))))
                                            .child(icon_button("git-sidebar-collapse-all", UiIcon::CollapseAll,
                                                "Collapse or expand all Git change groups", false)
                                                .on_click(cx.listener(|this, _, _, cx| this.toggle_git_sidebar_collapse_all(cx))))
                                            .child(icon_button("git-refresh", UiIcon::GitRefresh, "Refresh Git status", false)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.refresh_git(cx);
                                                    if this.git_history_open { this.refresh_git_history(cx); }
                                                }))),
                                    ),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(git_summary),
                            )
                            // Classic puts the commit form before history and file groups.
                            // Keep the main action labelled, not an icon-only mystery button.
                            .child(
                                div()
                                    .id("git-commit-controls")
                                    .px_2()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().w_full().min_w_0().child(git_commit_input))
                                    .child(div().flex().items_center().gap_1()
                                        .child(div().flex_1().min_w_0().child(Button::new("git-commit-submit")
                                            .label(tr(self.settings_menu_locale, "Commit"))
                                            .primary()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let message = this.git_commit_input.read(cx).query().trim().to_owned();
                                                this.run_git_action(GitUiAction::Commit(message), cx);
                                            }))))
                                        .child(icon_button(
                                            "git-commit-offline-draft",
                                            if git_commit_draft_loading { UiIcon::Close } else { UiIcon::Edit },
                                            if git_commit_draft_loading {
                                                "Discard pending local commit draft"
                                            } else {
                                                "Draft commit message from staged file names (offline, not AI)"
                                            },
                                            git_commit_draft_loading,
                                        ).on_click(cx.listener(|this, _, _, cx| {
                                            this.prepare_offline_commit_draft(cx);
                                        })))
                                        .child(icon_button("git-amend-confirm",
                                            if git_amend_ready { UiIcon::Check } else { UiIcon::Edit },
                                            if git_amend_loading { "Calculating amend preview…" } else if git_amend_ready { "Confirm replacing last local commit" } else { "Prepare Amend: preview current HEAD/index; confirm twice" },
                                            git_amend_ready)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.prepare_or_confirm_git_amend(cx);
                                            }))))
                            )
                            .when(git_ai_generate_loading, |panel| panel.child(
                                div().text_xs().text_color(rgb(MUTED))
                                    .child("Generating with the Host-configured read-only Agent... Click Cancel to stop.")
                            ))
                            .when(git_ai_prompt_loading, |panel| panel.child(
                                div().text_xs().text_color(rgb(MUTED))
                                    .child("Preparing or checking staged AI prompt (no Agent started)...")
                            ))
                            .when_some(git_ai_prompt_preview, |panel, preview| {
                                let excerpt: String = preview.prompt.chars().take(550).collect();
                                let preview_note = format!(
                                    "Staged AI prompt · {} file(s) · HEAD {} · {}",
                                    preview.staged_files,
                                    &preview.head_oid[..8],
                                    if preview.patch_truncated { "patch truncated to budget" }
                                    else { "bounded read-only diff" }
                                );
                                panel.child(div().flex().flex_col().gap_1()
                                    .child(div().text_xs().text_color(rgb(TEXT)).child(preview_note))
                                    .child(div().id("git-ai-prompt-excerpt")
                                        .max_h(px(110.0)).overflow_y_scroll()
                                        .text_xs().text_color(rgb(MUTED))
                                        .child(format!("{excerpt}…")))
                                    .child(div().flex().items_center().gap_1()
                                        .child(icon_button("git-ai-generate",
                                            if git_ai_generate_loading { UiIcon::Close } else { UiIcon::Edit },
                                            if git_ai_generate_loading {
                                                "Cancel AI Commit generation and stop the Agent"
                                            } else {
                                                "Generate AI Commit with Classic Host settings (read-only Codex/Claude; does not commit)"
                                            },
                                            git_ai_generate_loading)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.generate_ai_commit_message(cx);
                                            })))
                                        .child(icon_button("git-ai-prompt-copy", UiIcon::Save,
                                            "Verify staged index and copy full AI prompt to clipboard (no AI execution)", false)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.copy_staged_ai_prompt(cx);
                                            })))
                                        .child(icon_button("git-ai-prompt-close", UiIcon::Close,
                                            "Dismiss staged AI prompt without copying", false)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.close_staged_ai_prompt(cx);
                                            })))))
                            })
                            .child(
                                div()
                                    .id("git-advanced-actions-toggle")
                                    .h(px(30.0))
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(ACTIVE)))
                                    .text_xs()
                                    .text_color(rgb(TEXT))
                                    .child(icon(if self.git_advanced_actions_open { UiIcon::ChevronDown } else { UiIcon::ChevronRight }, 14.0, MUTED))
                                    .child(tr(self.settings_menu_locale, "Branch & remote actions"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.git_advanced_actions_open = !this.git_advanced_actions_open;
                                        cx.notify();
                                    })),
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
                                    .when(!self.git_advanced_actions_open, |panel| panel.hidden())
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
                                        })))
                                    .when(!self.git_advanced_actions_open, |panel| panel.hidden()),
                            )
                            .child(div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(icon_button("git-compare-local", UiIcon::History, "Compare the selected branch with HEAD (read-only)", false)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let branch = this.git_branch_input.read(cx).query().trim().to_owned();
                                        this.git_merge_armed = None;
                                        this.compare_git_branch_files(branch, CompareDirection::CurrentToSelected, cx);
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
                                .child(icon_button("git-pull-selected-branch", UiIcon::GitPull,
                                    "Pull selected branch without checkout (fast-forward only; confirm twice)", false)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_selected_branch_remote(true, cx);
                                    })))
                                .child(icon_button("git-push-selected-branch", UiIcon::GitPush,
                                    "Push selected branch without checkout (no force; confirm twice)", false)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_selected_branch_remote(false, cx);
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
                                            let ai_owner = owner.clone();
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
                                                )
                                                .separator()
                                                .item(
                                                    PopupMenuItem::new("Preview staged AI Commit prompt (no AI run)")
                                                        .on_click(move |_, _, cx| {
                                                            let _ = ai_owner.update(cx, |view, cx| {
                                                                view.prepare_staged_ai_prompt(cx);
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
                                .when(!self.git_advanced_actions_open, |panel| panel.hidden())
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
                            .child(div().px_2().py_1().text_xs().text_color(rgb(MUTED))
                                .child(format!("{git_sidebar_matches} / {git_sidebar_total} changes")))
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
                                    .when(!git_discard_paths.is_empty(), |bar| bar.child(
                                        icon_button("git-discard-selected", if git_discard_confirm { UiIcon::Check } else { UiIcon::GitDiscard },
                                            if git_discard_confirm { "Confirm Discard selected unstaged tracked files" } else { "Discard selected unstaged tracked changes (confirm)" },
                                            git_discard_confirm)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.confirm_discard_git_paths(git_discard_paths.clone(), cx);
                                            }))
                                    ))
                                    .child(icon_button("git-clear-selected", UiIcon::Close, "Clear Git selection", false)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.git_selected_changes.clear();
                                            this.git_selected_folders.clear();
                                            this.git_selection_anchor = None;
                                            cx.notify();
                                        })))
                            ))
                            .child(
                                div()
                                    .id("git-change-list")
                                    .key_context("AleraGpuiGit")
                                    .track_focus(&self.git_keyboard_focus)
                                    // A 135px file list hid nearly all changes behind a nested
                                    // scrollbar. Let Source Control use the available sidebar
                                    // height and keep the patch preview below the files.
                                    .flex_1()
                                    .min_h(px(240.0))
                                    .max_h(px(640.0))
                                    .overflow_y_scroll()
                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| {
                                        window.focus(&this.git_keyboard_focus, cx);
                                    }))
                                    .on_drop(cx.listener(|this, drag: &ExplorerDragBatch, _, cx| {
                                        let root = this.model.root().to_path_buf();
                                        this.drop_explorer_batch(drag, &root, cx);
                                    }))
                                    .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                                        this.import_explorer_drop(paths.paths().to_vec(), None, cx);
                                    }))
                                    .when(git_sidebar_matches == 0, |list| list.child(
                                        div().px_2().py_3().text_xs().text_color(rgb(MUTED))
                                            .child(tr(self.settings_menu_locale, if git_sidebar_total == 0 {
                                                "No changes"
                                            } else {
                                                "No matching files"
                                            }))
                                    ))
                                    .children(git_sidebar_entries.into_iter().enumerate().map(|(i, entry)| {
                                        let (change, depth) = match entry {
                                            GitSidebarEntry::Section { key, total, matching, expanded } => {
                                                let label = if let Some(area) = key { area.title() } else { "ALL CHANGES" };
                                                let caption = if matching == total {
                                                    format!("{label} ({total})")
                                                } else {
                                                    format!("{label} ({matching}/{total})")
                                                };
                                                let actions = section_actions(&git_all_changes, key);
                                                let stage_paths = actions.stage;
                                                let unstage_paths = actions.unstage;
                                                return div().id(("git-group-section", i))
                                                    .flex().items_center().gap_2().h(px(30.0)).px_2().text_xs()
                                                    .cursor_pointer().text_color(rgb(TEXT))
                                                    .hover(|style| style.bg(rgb(ACTIVE)))
                                                    .child(icon(if expanded { UiIcon::ChevronDown } else { UiIcon::ChevronRight }, 14.0, MUTED))
                                                    .child(div().flex_1().min_w_0().truncate().child(caption))
                                                    .when(!stage_paths.is_empty(), |row| row.child(
                                                        sidebar_icon_button(("git-section-stage-all", i), UiIcon::GitStage,
                                                            format!("Stage all {label} changes"), false)
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                cx.stop_propagation();
                                                                this.run_git_action(GitUiAction::StageMany(stage_paths.clone()), cx);
                                                            }))
                                                    ))
                                                    .when(!unstage_paths.is_empty(), |row| row.child(
                                                        sidebar_icon_button(("git-section-unstage-all", i), UiIcon::GitUnstage,
                                                            format!("Unstage all {label} changes"), false)
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                cx.stop_propagation();
                                                                this.run_git_action(GitUiAction::UnstageMany(unstage_paths.clone()), cx);
                                                            }))
                                                    ))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.toggle_git_sidebar_section(key, cx);
                                                    }))
                                                    .into_any_element();
                                            }
                                            GitSidebarEntry::Folder { key, depth, total, matching, expanded } => {
                                                let name = key.1.file_name().unwrap_or_default().to_string_lossy().into_owned();
                                                let folder_drop_target = git_drag_root.join(&key.1);
                                                let external_folder_target = folder_drop_target.clone();
                                                let folder_selected = git_selected_folders.contains(&key);
                                                let folder_for_click = key.clone();
                                                let folder_for_toggle = key.clone();
                                                let folder_for_menu = key.clone();
                                                let folder_visible = Arc::clone(&git_visible_items);
                                                let folder_drag = ExplorerDragBatch::for_row(
                                                    &git_drag_root, git_drag_generation,
                                                    &folder_drop_target, git_drag_selection.as_ref(),
                                                );
                                                let folder_drag_caption = if folder_drag.len() > 1 {
                                                    format!("Copy {} worktree items", folder_drag.len())
                                                } else {
                                                    format!("Copy folder {name}")
                                                };
                                                let folder_native_root = git_drag_root.clone();
                                                let folder_icon = explorer_file_icon_element(
                                                    &name, true, expanded, 16.0, MUTED,
                                                );
                                                let caption = if total == matching {
                                                    format!("{name} ({total})")
                                                } else {
                                                    format!("{name} ({matching}/{total})")
                                                };
                                                return div().id(("git-group-folder", i))
                                                    .flex().items_center().gap_2().h(px(28.0)).text_xs()
                                                    .pl(px(8.0 + depth as f32 * 14.0))
                                                    .cursor_pointer().text_color(rgb(MUTED))
                                                    .bg(rgb(if folder_selected { ACTIVE } else { PANEL }))
                                                    .hover(|style| style.bg(rgb(ACTIVE)))
                                                    .child(div().id(("git-folder-toggle", i))
                                                        .cursor_pointer()
                                                        .child(icon(if expanded { UiIcon::ChevronDown } else { UiIcon::ChevronRight }, 14.0, MUTED))
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            this.toggle_git_sidebar_folder(folder_for_toggle.clone(), cx);
                                                        })))
                                                    .child(folder_icon)
                                                    .child(div().flex_1().min_w_0().truncate().child(caption))
                                                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                                        let modifiers = event.modifiers();
                                                        this.select_git_sidebar_item(
                                                            GitSidebarSelectionItem::Folder(folder_for_click.clone()),
                                                            folder_visible.as_ref(),
                                                            modifiers.control || modifiers.platform, modifiers.shift,
                                                        );
                                                        window.focus(&this.git_keyboard_focus, cx);
                                                        cx.notify();
                                                    }))
                                                    .on_drag(folder_drag, move |_, _, _, cx| {
                                                        cx.new(|_| TerminalTabDragPreview { label: folder_drag_caption.clone() })
                                                    })
                                                    .external_drag_payload(move |drag: &ExplorerDragBatch, _, _| {
                                                        let files = ExplorerFiles::new(&folder_native_root).ok()?;
                                                        let entries = drag.external_entries(&files, git_drag_generation).ok()?;
                                                        Some(gpui::ExternalDragPayload::Files(gpui::FileDragPaths::new(entries)))
                                                    })
                                                    .on_drop(cx.listener(move |this, drag: &ExplorerDragBatch, _, cx| {
                                                        cx.stop_propagation();
                                                        this.drop_explorer_batch(drag, &folder_drop_target, cx);
                                                    }))
                                                    .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                                                        cx.stop_propagation();
                                                        this.import_explorer_drop(paths.paths().to_vec(), Some(external_folder_target.clone()), cx);
                                                    }))
                                                    .context_menu(Self::kit_git_folder_menu(cx.entity().downgrade(), folder_for_menu))
                                                    .into_any_element();
                                            }
                                            GitSidebarEntry::File { change, depth } => (change, depth),
                                        };
                                        let selected = git_selected.contains(&change);
                                        let clicked_row = change.clone();
                                        let preview_area = change.area;
                                        let menu_row = change.clone();
                                        let discard_armed = change.area == alera_gpui_desktop::git_inspect::GitChangeArea::Changes &&
                                            self.git_discard_armed.as_ref().is_some_and(|paths| paths.contains(&change.relative));
                                        let visible_for_click = Arc::clone(&git_visible_items);
                                        let display_name = if git_tree_mode {
                                            change.relative.file_name().unwrap_or_default().to_string_lossy().into_owned()
                                        } else {
                                            change.relative.display().to_string()
                                        };
                                        let file_icon = explorer_file_icon_element(
                                            &display_name, false, false, 16.0, MUTED,
                                        );
                                        let status_color = match change.label {
                                            "A" | "?" | "U" => 0x77c79b,
                                            "D" | "CONFLICT" => 0xe58d8d,
                                            _ => 0xe7b96a,
                                        };
                                        let status_label = change.label;
                                        let area_label = change.area.title();
                                        let can_stage = change.area.can_stage();
                                        let can_unstage = change.area.can_unstage();
                                        let tooltip_path = change.relative.display().to_string();
                                        let relative = change.relative;
                                        let git_file_path = git_drag_root.join(&relative);
                                        let git_drag_payload = ExplorerDragBatch::for_row(
                                            &git_drag_root,
                                            git_drag_generation,
                                            &git_file_path,
                                            git_drag_selection.as_ref(),
                                        );
                                        let git_drag_caption = if git_drag_payload.len() > 1 {
                                            format!("Copy {} worktree items", git_drag_payload.len())
                                        } else {
                                            format!("Copy {}", display_name)
                                        };
                                        let native_drag_root = git_drag_root.clone();
                                        let file_drop_parent = git_file_path.parent()
                                            .unwrap_or(&git_drag_root).to_path_buf();
                                        let external_file_drop_parent = file_drop_parent.clone();
                                        let stage_path = relative.clone();
                                        let unstage_path = relative.clone();
                                        div()
                                            .id(("git-change-file", i))
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .h(px(28.0))
                                            .pl(px(8.0 + depth as f32 * 14.0))
                                            .pr_1()
                                            .text_xs()
                                            .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                            .child(div()
                                                .id(("git-change-preview", i))
                                                .flex_1()
                                                .min_w_0()
                                                .cursor_pointer()
                                                .tooltip(move |window, cx| {
                                                    gpui_kit::component::tooltip::Tooltip::new(tooltip_path.clone())
                                                        .build(window, cx)
                                                })
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .child(file_icon)
                                                .child(div().flex_1().min_w_0().truncate().text_sm().child(display_name))
                                                .when(!git_group_by_area, |row| row.child(
                                                    div().text_xs().text_color(rgb(MUTED)).child(area_label)
                                                ))
                                                .child(div().min_w(px(14.0)).text_xs()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(rgb(status_color)).child(status_label))
                                                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                                    let modifiers = event.modifiers();
                                                    let multi = modifiers.control || modifiers.platform;
                                                    this.select_git_sidebar_item(GitSidebarSelectionItem::File(clicked_row.clone()), visible_for_click.as_ref(), multi, modifiers.shift);
                                                    window.focus(&this.git_keyboard_focus, cx);
                                                    if !multi && !modifiers.shift {
                                                        this.open_git_sidebar_file_diff(relative.clone(), preview_area, cx);
                                                    }
                                                    cx.notify();
                                                }))
                                                .on_drag(git_drag_payload, move |_, _, _, cx| {
                                                    cx.new(|_| TerminalTabDragPreview { label: git_drag_caption.clone() })
                                                })
                                                .external_drag_payload(move |drag: &ExplorerDragBatch, _, _| {
                                                    let files = ExplorerFiles::new(&native_drag_root).ok()?;
                                                    let entries = drag.external_entries(&files, git_drag_generation).ok()?;
                                                    Some(gpui::ExternalDragPayload::Files(gpui::FileDragPaths::new(entries)))
                                                })
                                                .on_drop(cx.listener(move |this, drag: &ExplorerDragBatch, _, cx| {
                                                    cx.stop_propagation();
                                                    this.drop_explorer_batch(drag, &file_drop_parent, cx);
                                                }))
                                                .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                                                    cx.stop_propagation();
                                                    this.import_explorer_drop(paths.paths().to_vec(), Some(external_file_drop_parent.clone()), cx);
                                                }))
                                                .context_menu(Self::kit_git_change_menu(cx.entity().downgrade(), menu_row, discard_armed)))
                                            .when(can_stage, |row| row.child(sidebar_icon_button(("git-stage-file", i), UiIcon::GitStage, "Stage this file for commit", false)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.run_git_action(GitUiAction::Stage(stage_path.clone()), cx);
                                                }))))
                                            .when(can_unstage, |row| row.child(sidebar_icon_button(("git-unstage-file", i), UiIcon::GitUnstage, "Unstage this file (keep working edits)", false)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.run_git_action(GitUiAction::Unstage(unstage_path.clone()), cx);
                                                }))))
                                            .into_any_element()
                                    })),
                            )
                            .child(
                                div()
                                    .id("git-patch-preview-toggle")
                                    .h(px(30.0))
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .border_t_1()
                                    .border_color(rgb(BORDER))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(ACTIVE)))
                                    .child(icon(if self.git_sidebar_patch_expanded { UiIcon::ChevronDown } else { UiIcon::ChevronRight }, 14.0, MUTED))
                                    .child(div().text_xs().font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(tr(self.settings_menu_locale, "Patch Preview")))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.git_sidebar_patch_expanded = !this.git_sidebar_patch_expanded;
                                        cx.notify();
                                    })),
                            )
                            .when(self.git_sidebar_patch_expanded, |panel| panel.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(div().text_xs().text_color(rgb(MUTED))
                                        .child(tr(self.settings_menu_locale, "Patch preview · read-only")))
                                    .child(if git_diff_expand_available {
                                        div()
                                            .id("git-diff-expand")
                                            .cursor_pointer()
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .child("Expand diff")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_kit_git_diff_dialog(window, cx);
                                            }))
                                            .into_any_element()
                                    } else {
                                        div().text_xs().text_color(rgb(MUTED))
                                            .child("Expand diff").into_any_element()
                                    })
                                    .child(if git_diff_expand_available && git_file_page.is_some() {
                                        div()
                                            .id("git-diff-open-center")
                                            .cursor_pointer()
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .child("Center")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.open_center_git_diff(cx);
                                            }))
                                            .into_any_element()
                                    } else {
                                        div().into_any_element()
                                    }),
                            )
                            .child(if let Some((page, can_previous, can_next)) = git_file_page {
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(format!("Page {page} · live Git · 32 KiB/page")),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(if can_previous {
                                                div()
                                                    .id("git-diff-previous-page")
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(rgb(TEXT))
                                                    .child("Previous")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.change_git_file_diff_page(false, cx);
                                                    }))
                                                    .into_any_element()
                                            } else {
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child("Previous")
                                                    .into_any_element()
                                            })
                                            .child(if can_next {
                                                div()
                                                    .id("git-diff-next-page")
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(rgb(TEXT))
                                                    .child("Next")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.change_git_file_diff_page(true, cx);
                                                    }))
                                                    .into_any_element()
                                            } else {
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child("Next")
                                                    .into_any_element()
                                            }),
                                    )
                                    .into_any_element()
                            } else {
                                div().into_any_element()
                            })
                            .child(
                                div()
                                    .id("git-diff-scroll")
                                    .max_h(px(170.0))
                                    .overflow_y_scroll()
                                    .text_xs()
                                    .font_family(terminal_font_family.clone())
                                    .text_color(rgb(MUTED))
                                    .child(git_diff),
                            ))
                            // Classic docks Git History after the changed-file list.
                            .child(
                                div()
                                    .id("git-history-footer-toggle")
                                    .h(px(SECTION_HEADER_HEIGHT))
                                    .mt_2()
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .border_t_1()
                                    .border_color(rgb(BORDER))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(ACTIVE)))
                                    .child(icon(if git_graph_open { UiIcon::ChevronDown } else { UiIcon::ChevronRight }, 14.0, MUTED))
                                    .child(div().text_xs().font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(tr(self.settings_menu_locale, "Git History")))
                                    .child(div().flex_1())
                                    .child(icon_button("git-graph-expand-center", UiIcon::Open,
                                        "Open persistent Git History tab in workbench", false)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.open_center_git_graph(cx);
                                        })))
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_git_history(cx))),
                            )
                            .child(if git_graph_open {
                                div()
                                    .id("git-graph-panel")
                                    .flex()
                                    .flex_col()
                                    .max_h(px(460.0))
                                    .overflow_y_scroll()
                                    .gap_1()
                                    .child(div().text_xs().text_color(rgb(MUTED)).child(git_history_status))
                                    .child(div().flex().items_center().gap_1()
                                        .child(if graph_surface_visible {
                                            div().flex_1().text_xs().text_color(rgb(MUTED))
                                                .child("Search in full-height Git Graph").into_any_element()
                                        } else {
                                            div().flex_1().min_w_0()
                                                .child(git_graph_search_input).into_any_element()
                                        })
                                        .child(icon_button("git-graph-search-all", UiIcon::Search,
                                            "Search reachable Git history in background (up to 50,000 commits; path scopes remain limited)", false)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.search_all_git_history(cx);
                                            })))
                                        .when(git_graph_full_search_active, |row| row.child(
                                            icon_button("git-graph-exit-full-search", UiIcon::History,
                                                if self.git_history_loading {
                                                    "Stop active full-history scan and return to paged Git Graph"
                                                } else {
                                                    "Return to paged Git Graph"
                                                }, false)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.exit_git_history_search(cx);
                                                }))
                                        ))
                                        .child(icon_button("git-graph-clear-search", UiIcon::Close,
                                            "Clear Git Graph search", false)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.git_graph_search_input.update(cx, |input, cx| input.clear(cx));
                                                if this.git_history_options.search_query.is_some() {
                                                    this.exit_git_history_search(cx);
                                                } else {
                                                    cx.notify();
                                                }
                                            }))))
                                    .when_some(git_worktree_commit_target, |panel, oid| panel.child(
                                        div().flex().flex_col().gap_1()
                                            .child(div().text_xs().text_color(rgb(TEXT))
                                                .child(format!("New Worktree at {}", &oid[..8])))
                                            .child(div().flex().items_center().gap_1()
                                                .child(div().flex_1().min_w_0().child(git_worktree_commit_input))
                                                .child(icon_button("git-worktree-commit-confirm",
                                                    if git_worktree_commit_armed { UiIcon::Check } else { UiIcon::Add },
                                                    if git_worktree_commit_armed { "Confirm creating managed Worktree" } else { "Prepare Worktree creation (confirm twice)" },
                                                    git_worktree_commit_armed)
                                                    .on_click(cx.listener(|this, _, _, cx| this.confirm_git_worktree_at_commit(cx))))
                                                .child(icon_button("git-worktree-commit-cancel", UiIcon::Close,
                                                    "Cancel creating Worktree", false)
                                                    .on_click(cx.listener(|this, _, _, cx| this.cancel_git_worktree_at_commit(cx))))
                                            )
                                    ))
                                    .when(git_commit_apply_loading, |panel| panel.child(
                                        div().text_xs().text_color(rgb(MUTED))
                                            .child("Calculating Cherry-pick/Revert preview (no files changed)…")
                                    ))
                                    .when_some(git_commit_apply_preview, |panel, preview| panel.child(
                                        div().flex().flex_col().gap_1()
                                            .child(div().text_xs().text_color(rgb(TEXT))
                                                .child(format!("{} {} → {} · {} file(s)",
                                                    preview.kind.label(),
                                                    &preview.source_oid[..8],
                                                    preview.current_branch,
                                                    preview.changed_paths.len())))
                                            .child(div().id("git-commit-apply-files").max_h(px(92.0))
                                                .overflow_y_scroll().flex().flex_col()
                                                .children(preview.changed_paths.into_iter().enumerate().map(|(i, path)| {
                                                    div().id(("git-commit-apply-file", i))
                                                        .text_xs().text_color(rgb(MUTED)).child(path)
                                                })))
                                            .child(div().flex().items_center().gap_1()
                                                .child(icon_button("git-commit-apply-confirm",
                                                    if git_commit_apply_armed { UiIcon::Check } else { UiIcon::Edit },
                                                    if git_commit_apply_armed { "Confirm commit apply now" } else { "Prepare Cherry-pick/Revert (confirm twice)" },
                                                    git_commit_apply_armed)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.confirm_git_commit_apply(cx);
                                                    })))
                                                .child(icon_button("git-commit-apply-cancel", UiIcon::Close,
                                                    "Cancel commit preview; nothing changed", false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.cancel_git_commit_apply(cx);
                                                    }))))
                                    ))
                                    .when(git_soft_reset_loading, |panel| panel.child(
                                        div().text_xs().text_color(rgb(MUTED))
                                            .child(format!("Checking {} Reset safety (no Git refs changed)...", git_soft_reset_mode.label()))
                                    ))
                                    .when_some(git_soft_reset_preview, |panel, preview| panel.child(
                                        div().flex().flex_col().gap_1()
                                            .child(div().text_xs().text_color(rgb(TEXT))
                                                .child(format!(
                                                    "{} Reset {}: {} to {} ({} commit(s))",
                                                    git_soft_reset_mode.label(),
                                                    preview.current_branch,
                                                    &preview.current_oid[..8],
                                                    &preview.target_oid[..8],
                                                    preview.removed_commits,
                                                )))
                                            .child(div().text_xs().text_color(rgb(MUTED))
                                                .child(format!(
                                                    "{} · {}",
                                                    preview.target_subject,
                                                    match git_soft_reset_mode {
                                                        NativeResetMode::Soft => "Index and work files stay unchanged; original HEAD in reflog.",
                                                        NativeResetMode::Mixed => "Index resets to target, work files stay unchanged; original HEAD is backed up.",
                                                    },
                                                )))
                                            .child(div().flex().items_center().gap_1()
                                                .child(icon_button("git-soft-reset-confirm",
                                                    if git_soft_reset_armed { UiIcon::Check } else { UiIcon::History },
                                                    if git_soft_reset_armed { "Confirm Reset now" } else { "Review Reset and confirm twice" },
                                                    git_soft_reset_armed)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.confirm_git_soft_reset(cx);
                                                    })))
                                                .child(icon_button("git-soft-reset-cancel", UiIcon::Close,
                                                    "Cancel Soft Reset preview; nothing changed", false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.cancel_git_soft_reset(cx);
                                                    }))))
                                    ))
                                    .when(git_rebase_loading, |panel| panel.child(
                                        div().text_xs().text_color(rgb(MUTED))
                                            .child("Checking local Rebase history and upstream safety...")
                                    ))
                                    .when_some(git_rebase_preview, |panel, preview| {
                                        let count = preview.commits.len();
                                        panel.child(div().flex().flex_col().gap_1()
                                            .child(div().text_xs().text_color(rgb(TEXT))
                                                .child(format!(
                                                    "Rebase {} ({}) onto {} ({})",
                                                    preview.current_branch, &preview.current_oid[..8],
                                                    preview.onto_branch, &preview.onto_oid[..8]
                                                )))
                                            .child(div().text_xs().text_color(rgb(MUTED))
                                                .child(format!(
                                                    "Replay {} unpublished commit(s). Conflicts abort; original HEAD is backed up before execution.",
                                                    count
                                                )))
                                            .child(div().id("git-rebase-commit-list")
                                                .max_h(px(92.0)).overflow_y_scroll().flex().flex_col()
                                                .children(preview.commits.into_iter().take(10).enumerate().map(|(i, (oid, subject))| {
                                                    div().id(("git-rebase-commit", i))
                                                        .text_xs().text_color(rgb(MUTED))
                                                        .child(format!("{} {}", &oid[..8], subject))
                                                })))
                                            .child(div().flex().items_center().gap_1()
                                                .child(icon_button("git-rebase-confirm",
                                                    if git_rebase_armed { UiIcon::Check } else { UiIcon::History },
                                                    if git_rebase_armed { "Confirm Rebase now" } else { "Review Rebase and confirm twice" },
                                                    git_rebase_armed)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.confirm_git_rebase(cx);
                                                    })))
                                                .child(icon_button("git-rebase-cancel", UiIcon::Close,
                                                    "Cancel Rebase preview without changing Git", false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.cancel_git_rebase(cx);
                                                    }))))
                                        )
                                    })
                                    .when_some(git_commit_ref_target, |panel, (oid, kind)| panel.child(
                                        div().flex().flex_col().gap_1()
                                            .child(div().text_xs().text_color(rgb(TEXT))
                                                .child(format!("Create {} at {}", kind.label(), &oid[..8])))
                                            .child(div().flex().items_center().gap_1()
                                                .child(div().flex_1().min_w_0().child(git_commit_ref_input))
                                                .child(icon_button("git-commit-ref-confirm",
                                                    if git_commit_ref_armed { UiIcon::Check } else { UiIcon::Add },
                                                    if git_commit_ref_armed { "Confirm creation at this commit" } else { "Prepare new branch or tag (confirm twice)" },
                                                    git_commit_ref_armed)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.confirm_git_commit_ref(cx);
                                                    })))
                                                .child(icon_button("git-commit-ref-cancel", UiIcon::Close,
                                                    "Cancel creating reference", false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.cancel_git_commit_ref(cx);
                                                    }))))
                                    ))
                                    .when_some(git_rename_source, |panel, source| panel.child(
                                        div().flex().flex_col().gap_1()
                                            .child(div().text_xs().text_color(rgb(TEXT))
                                                .child(format!("Rename local branch: {source}")))
                                            .child(div().flex().items_center().gap_1()
                                                .child(div().flex_1().min_w_0().child(git_rename_input))
                                                .child(icon_button("git-rename-confirm", if git_rename_armed { UiIcon::Check } else { UiIcon::Edit },
                                                    if git_rename_armed { "Confirm branch rename" } else { "Prepare branch rename (confirm twice)" },
                                                    git_rename_armed)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.confirm_graph_branch_rename(cx);
                                                    })))
                                                .child(icon_button("git-rename-cancel", UiIcon::Close, "Cancel rename", false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.cancel_graph_branch_rename(cx);
                                                    }))))
                                    ))
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
                                                    if matches!(this.git_graph_ref_tab, GitGraphRefTab::All | GitGraphRefTab::Remote) {
                                                        this.git_graph_ref_tab = GitGraphRefTab::Local;
                                                    }
                                                }
                                                if this.git_history_tab_open { this.save_mixed_pane_layout(cx); }
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
                                                if this.git_history_tab_open { this.save_mixed_pane_layout(cx); }
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
                                    .child(div().flex().items_center().gap_1()
                                        .children([
                                            GitGraphRefTab::All,
                                            GitGraphRefTab::Local,
                                            GitGraphRefTab::Remote,
                                            GitGraphRefTab::Tags,
                                        ].into_iter().enumerate().map(|(index, tab)| {
                                            div().id(("git-graph-ref-tab", index))
                                                .px_2().py_1().text_xs()
                                                .cursor_pointer()
                                                .bg(rgb(if tab == git_graph_ref_tab { ACTIVE } else { PANEL }))
                                                .text_color(rgb(if tab == git_graph_ref_tab { TEXT } else { MUTED }))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(tab.label())
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.select_git_graph_ref_tab(tab, cx);
                                                }))
                                        })))
                                    .when(git_ref_results_empty, |panel| panel.child(
                                        div().text_xs().text_color(rgb(MUTED))
                                            .child("No matching refs in this tab. Try another tab or clear search.")
                                    ))
                                    .when(git_graph_ref_tab != GitGraphRefTab::Tags, |panel| panel.child(
                                        div().text_xs().text_color(rgb(TEXT)).child("Branches · first 96 shown; search Graph with branch: to narrow, star to favorite")
                                    ))
                                    .child(div()
                                        .id("git-graph-branches")
                                        .max_h(px(85.0))
                                        .overflow_y_scroll()
                                        .children(git_branch_rows.into_iter().take(96).enumerate().map(|(index, row)| {
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
                                                            this.compare_git_branch_files(name.clone(), CompareDirection::CurrentToSelected, cx);
                                                        }
                                                    }))
                                                    .context_menu(Self::kit_git_graph_branch_menu(
                                                        cx.entity().downgrade(), context_branch,
                                                        remote, row.current, has_worktree,
                                                        self.git_graph_remote_armed.clone(),
                                                        self.git_branch_delete_armed.clone(),
                                                    )))
                                        })))
                                    .when(!git_tag_rows.is_empty(), |panel| panel.child(
                                        div().flex().flex_col().gap_1()
                                            .child(div().text_xs().text_color(rgb(TEXT)).child("Tags · select to view commit history"))
                                            .child(div().id("git-graph-tags")
                                                .max_h(px(90.0))
                                                .overflow_y_scroll()
                                                .children(git_tag_rows.into_iter().enumerate().map(|(index, tag)| {
                                                    let reference = format!("refs/tags/{}", tag.name);
                                                    let selected = git_graph_selected_ref.as_deref() == Some(reference.as_str());
                                                    div().id(("git-graph-tag", index))
                                                        .cursor_pointer().text_xs()
                                                        .text_color(rgb(if selected { TEXT } else { MUTED }))
                                                        .hover(|style| style.bg(rgb(ACTIVE)))
                                                        .child(format!("◇ {} · {}", tag.name, tag.tip))
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.set_git_history_ref(Some(reference.clone()), cx);
                                                        }))
                                                })))
                                    ))
                                    .when(git_branch_compare_loading, |panel| panel.child(
                                        div().text_xs().text_color(rgb(MUTED)).child("Comparing branch files…")
                                    ))
                                    .when_some(git_branch_compare, |panel, comparison| {
                                        let target = comparison.selected_branch.clone();
                                        let reverse = comparison.direction == CompareDirection::SelectedToCurrent;
                                        panel.child(div()
                                            .flex().flex_col().gap_1()
                                            .child(div().flex().items_center().gap_2()
                                                .child(div().flex_1().min_w_0().text_xs()
                                                    .text_color(rgb(TEXT)).child(comparison.title()))
                                                .child(icon_button("git-compare-flip", UiIcon::History,
                                                    "Reverse comparison direction (merge-base to other branch)", false)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        let next = if reverse {
                                                            CompareDirection::CurrentToSelected
                                                        } else {
                                                            CompareDirection::SelectedToCurrent
                                                        };
                                                        this.compare_git_branch_files(target.clone(), next, cx);
                                                    }))))
                                            .child(div().id("git-compare-files").max_h(px(150.0))
                                                .overflow_y_scroll().flex().flex_col()
                                                .children(comparison.files.into_iter().enumerate().map(|(index, file)| {
                                                    div().id(("git-compare-file", index))
                                                        .flex().items_center().gap_1().text_xs()
                                                        .child(div().flex_1().min_w_0()
                                                            .text_color(rgb(MUTED))
                                                            .child(format!("{} {}", file.status, file.display_path())))
                                                        .child(icon_button(("git-compare-preview", index), UiIcon::Edit,
                                                            "Preview committed file differences", false)
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.preview_git_compared_file(index, false, cx);
                                                            })))
                                                        .child(icon_button(("git-compare-original", index), UiIcon::Open,
                                                            "Open merge-base original as a read-only Git tab", false)
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.open_git_compared_original_tab(index, cx);
                                                            })))
                                                }))))
                                    })
                                    .child(div().flex().items_center().gap_2()
                                        .child(div().flex_1().text_xs().text_color(rgb(TEXT))
                                            .child(if git_graph_full_search_active {
                                                "Commit matches · parent rails hidden across skipped history"
                                            } else if git_graph_hide_merged_lanes {
                                                "Commits · merged lanes collapsed for display only"
                                            } else {
                                                "Commits · graph lanes represent actual parent commits"
                                            }))
                                        .when(git_history_more_available, |row| row.child(
                                            icon_button("git-graph-load-more", UiIcon::Add,
                                                "Load 96 more commits (bounded maximum 512)", false)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.load_more_git_history(cx);
                                                }))
                                        )))
                                    .when_some(git_graph_topology_note, |panel, note| {
                                        panel.child(div().text_xs().text_color(rgb(MUTED)).child(note))
                                    })
                                    .child(div()
                                        .id("git-graph-commits")
                                        .max_h(px(220.0))
                                        .overflow_y_scroll()
                                        .children(git_commit_rows.into_iter().map(|(index, row)| {
                                            let oid = row.oid;
                                            let menu_oid = oid.clone();
                                            let menu_subject = row.subject.clone();
                                            let is_head = git_graph_head_tip.as_ref()
                                                .is_some_and(|tip| tip == &row.short);
                                            let is_merge = row.parent_ids.len() > 1;
                                            let topology = git_graph_topology_rows.as_ref()
                                                .and_then(|rows| rows.get(index)).cloned();
                                            let subject = row.subject;
                                            let short = row.short;
                                            let author = row.author;
                                            let mut refs = row.labels;
                                            if let Some(tags) = git_graph_tags_by_oid.get(&oid) {
                                                refs.extend(tags.iter().cloned());
                                            }
                                            refs.sort_by_key(|label| {
                                                if label == &git_graph_current_branch { 0u8 }
                                                else if label.starts_with("refs/tags/") { 1 }
                                                else if label.starts_with("refs/remotes/") { 3 }
                                                else { 2 }
                                            });
                                            let hidden_refs = refs.len().saturating_sub(2);
                                            div()
                                                .id(("git-graph-commit", index))
                                                .cursor_pointer()
                                                .text_xs()
                                                .text_color(rgb(MUTED))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(div().flex().items_center().gap_2()
                                                    .child(native_git_graph_ui::graph_lane(
                                                        topology, git_graph_lane_count, is_merge, is_head,
                                                    ))
                                                    .child(div().min_w_0().flex_1().flex().flex_col().gap_1()
                                                        .child(div().flex().items_center().gap_1()
                                                            .child(div().min_w_0().flex_1()
                                                                .text_color(rgb(TEXT)).child(subject))
                                                            .children(refs.into_iter().take(2).map(|label| {
                                                                let color = if label.starts_with("refs/remotes/") {
                                                                    native_git_graph_ui::REF_REMOTE_COLOR
                                                                } else if label.starts_with("refs/tags/") {
                                                                    native_git_graph_ui::REF_TAG_COLOR
                                                                } else {
                                                                    native_git_graph_ui::REF_LOCAL_COLOR
                                                                };
                                                                let display = label.strip_prefix("refs/tags/")
                                                                    .or_else(|| label.strip_prefix("refs/remotes/"))
                                                                    .unwrap_or(&label);
                                                                let shown = if display.chars().count() > 22 {
                                                                    format!("{}…", display.chars().take(21).collect::<String>())
                                                                } else { display.to_owned() };
                                                                div().px(px(4.0)).py(px(1.0)).rounded_sm()
                                                                    .bg(rgb(ELEVATED)).text_color(rgb(color))
                                                                    .child(shown)
                                                            }))
                                                            .when(hidden_refs > 0, |item| item.child(
                                                                div().text_color(rgb(MUTED))
                                                                    .child(format!("+{hidden_refs}"))
                                                            )))
                                                        .child(div().flex().items_center().gap_2()
                                                            .text_color(rgb(MUTED))
                                                            .child(short)
                                                            .child(author))))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.preview_git_history(GitReadRequest::Commit(oid.clone()), cx);
                                                }))
                                                .context_menu(Self::kit_git_graph_commit_menu(
                                                    cx.entity().downgrade(), menu_oid, menu_subject, row.parents,
                                                ))
                                        })))
                                    .into_any_element()
                            } else {
                                div().h(px(0.0)).into_any_element()
                            })
                            )
                    )
                    .child(
                        div()
                            .id("right-sidebar-rail")
                            .w(px(PROJECTS_COLLAPSED_WIDTH))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .when(self.right_sidebar_visible, |rail| rail.hidden())
                            .items_center()
                            .bg(rgb(PANEL))
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .child(div().h(px(8.0)))
                            .child(icon_button("right-rail-explorer", UiIcon::Files, "Explorer", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.right_sidebar_view = RightSidebarView::Explorer;
                                    this.right_sidebar_visible = true;
                                    this.save_sidebar_layout(cx);
                                    cx.notify();
                                })))
                            .child(icon_button("right-rail-search", UiIcon::Search, "Search", false)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_content_search_panel(window, cx);
                                })))
                            .child(icon_button("right-rail-references", UiIcon::List, "References", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_references_panel(cx);
                                })))
                            .child(icon_button("right-rail-git", UiIcon::Branch, "Source Control", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.right_sidebar_view = RightSidebarView::Git;
                                    this.explorer_context_menu = None;
                                    this.right_sidebar_visible = true;
                                    this.save_sidebar_layout(cx);
                                    cx.notify();
                                })))
                            .child(icon_button("right-rail-pr", UiIcon::PullRequest, "Pull Requests (browser)", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_pull_requests_panel(cx);
                                })))
                            .child(div().flex_1())
                            .child(icon_button("right-rail-expand", UiIcon::ChevronsLeft, "Expand panel", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.right_sidebar_visible = true;
                                    this.save_sidebar_layout(cx);
                                    cx.notify();
                                }))),
                    )
                    .child(
                        div()
                            .id("right-sidebar-resize-handle")
                            .when(!self.right_sidebar_visible, |handle| handle.hidden())
                            .w(px(SIDEBAR_RESIZE_HANDLE_WIDTH))
                            .flex_shrink_0()
                            .h_full()
                            .bg(rgb(BORDER))
                            .cursor(gpui::CursorStyle::ResizeLeftRight)
                            .hover(|style| style.bg(rgb(BORDER_ACTIVE)))
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                this.sidebar_resize_drag = Some(SidebarResizeDrag {
                                    side: SidebarResizeSide::Context,
                                    start_x: f32::from(event.position.x),
                                    start_width: this.right_sidebar_width,
                                });
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            // One header layout owns both split Pane Tab bars.  The secondary
                            // pane no longer starts a whole Tab-row below the primary.
                            .child(
                                div()
                                    .id("mixed-pane-aligned-headers")
                                    .flex()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .id("mixed-primary-header-column")
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                            .child(
                                div()
                                    .id("mixed-primary-pinned-tab-strip")
                                    .when(shared_pinned_row, |row| {
                                        row.h(px(TAB_STRIP_HEIGHT))
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .overflow_x_scroll()
                                            .pl(px(TAB_STRIP_LEADING_PADDING))
                                            .bg(rgb(TOOLBAR))
                                            .border_b_1()
                                            .border_color(rgb(BORDER))
                                            .children((if primary_two_rows { primary_pinned_tabs } else { Vec::new() }).into_iter().enumerate().map(|(index, tab)| {
                                                let selected = mixed_primary_active.as_ref() == Some(&tab.identity);
                                                Self::render_mixed_tab_chip(
                                                    index + 512,
                                                    tab,
                                                    PaneSlot::Primary,
                                                    selected,
                                                    true,
                                                    mixed_drag_generation,
                                                    &self.terminals.sessions,
                                                    self.settings_menu_locale,
                                                    cx,
                                                )
                                            }))
                                    }),
                            )
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
                                            .pl(px(TAB_STRIP_LEADING_PADDING))
                                            .children(mixed_primary_tabs.into_iter().enumerate().map(|(index, tab)| {
                                                let selected = !showing_explorer_preview
                                                    && mixed_primary_active.as_ref() == Some(&tab.identity);
                                                let pinned = pinned_ids.contains(&tab.identity);
                                                Self::render_mixed_tab_chip(
                                                    index, tab, PaneSlot::Primary, selected, pinned,
                                                    mixed_drag_generation, &self.terminals.sessions, self.settings_menu_locale, cx,
                                                )
                                            }))
                                            .children(git_original_tabs.into_iter().enumerate().map(|(index, tab)| {
                                                let id = tab.instance;
                                                let active = active_git_original.as_ref().is_some_and(|selected| selected.instance == id);
                                                let title = tab.title();
                                                div()
                                                    .id(("git-original-tab", index))
                                                    .h(px(32.0)).mr_2().px(px(TAB_CHIP_HORIZONTAL_PADDING))
                                                    .flex().items_center().rounded_md().border_1()
                                                    .border_color(rgb(if active { BORDER_ACTIVE } else { BORDER }))
                                                    .bg(rgb(if active { ELEVATED } else { TOOLBAR }))
                                                    .child(div()
                                                        .id(("git-original-tab-select", index))
                                                        .min_w_0().max_w(px(210.0)).flex()
                                                        .items_center().gap_1()
                                                        .cursor_pointer().text_sm()
                                                        .text_color(rgb(if active { TEXT } else { MUTED }))
                                                        .child(icon(UiIcon::File, 12.0, MUTED))
                                                        .child(div().truncate().child(title))
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.focus_git_original_tab(id, cx);
                                                        })))
                                                    .child(tab_close_button(("git-original-tab-close", index),
                                                        tr(self.settings_menu_locale, "Close immutable Git original"))
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.close_git_original_tab(id, cx);
                                                        })))
                                            }))
                                            // Legacy Explorer preview tabs belong
                                            // to the same visual strip, never a
                                            // second line beneath live tabs.
                                            .children(tabs.into_iter().map(|(index, title, transient)| {
                                                let selected = showing_explorer_preview && active_tab == Some(index);
                                                let file_icon = explorer_file_icon_element(&title, false, false, 12.0, if selected { TEXT } else { MUTED });
                                                div()
                                                    .id(("file-preview-tab", index))
                                                    .h(px(32.0))
                                                    .mr_2()
                                                    .px(px(TAB_CHIP_HORIZONTAL_PADDING))
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
                                                            .cursor_pointer()
                                                            .text_color(rgb(if selected { TEXT } else { MUTED }))
                                                            .child(file_icon)
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
                                                                this.release_primary_center_overlays(PaneSlot::Primary);
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
                                                        tab_close_button(("file-tab-close", index), tr(self.settings_menu_locale, "Close preview"))
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
                                                        session.clone(),
                                                        format!("{} · {}", session.label(), session.workspace_id),
                                                    ))
                                                    .collect::<Vec<_>>();
                                                let target = PaneAttachGuard::capture(
                                                    self.workspace_generation,
                                                    PaneSlot::Primary,
                                                    &self.mixed_panes,
                                                );
                                                let profiles = self.agent_profiles.clone();
                                                let installed = self.installed_agent_clis.clone();
                                                let installed_loading = self.installed_agent_clis_loading;
                                                icon_button("mixed-new-tab", UiIcon::Add, tr(self.settings_menu_locale, "New Tab"), terminal_creating
                                                    && self.terminal_create_pane.as_ref().is_some_and(|pending| pending.target.pane() == PaneSlot::Primary))
                                                    .w(px(28.0))
                                                    .h(px(28.0))
                                                    .dropdown_menu(Self::kit_new_tab_menu(
                                                        owner,
                                                        target,
                                                        running,
                                                        profiles,
                                                        installed,
                                                        installed_loading,
                                                        terminal_create_busy,
                                                        terminal_create_timed_out,
                                                        terminal_create_unknown,
                                                    ))
                                                    .on_open_change(move |open, _, cx| {
                                                        if *open {
                                                            let _ = refresh_owner.update(cx, |view, cx| {
                                                                view.refresh_terminals(cx);
                                                                view.refresh_agent_profiles(cx);
                                                                view.refresh_installed_agent_clis(cx);
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
                                                            this.new_editor_in_pane(PaneSlot::Primary, cx);
                                                        }
                                                    }))
                                                    .on_drop(cx.listener(|this, drag: &MixedTabDrag, window, cx| {
                                                        this.drop_mixed_tab_on_strip_tail(drag, PaneSlot::Primary, window, cx);
                                                    })),
                                            ),
                                    )
                                    .child(
                                        icon_button("native-terminal-history-up-primary", UiIcon::ChevronUp,
                                            tr(self.settings_menu_locale, "Scroll terminal history up one page (Shift+PageUp)"), false)
                                            .when(!terminal_is_vt || !mixed_primary_active.as_ref().is_some_and(|id|
                                                matches!(id, WorkbenchTabIdentity::Host(_))), |button| button.hidden())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.scroll_native_terminal_page(PaneSlot::Primary, true, cx);
                                            })),
                                    )
                                    .child(
                                        icon_button("native-terminal-history-down-primary", UiIcon::ChevronDown,
                                            tr(self.settings_menu_locale, "Scroll terminal history down one page (Shift+PageDown)"), false)
                                            .when(!terminal_is_vt || !mixed_primary_active.as_ref().is_some_and(|id|
                                                matches!(id, WorkbenchTabIdentity::Host(_))), |button| button.hidden())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.scroll_native_terminal_page(PaneSlot::Primary, false, cx);
                                            })),
                                    )
                                    .child(
                                        icon_button("mixed-pane-actions", UiIcon::More, tr(self.settings_menu_locale, "Pane actions"), false)
                                            .dropdown_menu(Self::kit_pane_actions_menu(
                                                cx.entity().downgrade(),
                                                mixed_primary_active.clone(),
                                                false,
                                                mixed_has_secondary,
                                            )),
                                    ),
                            )
                                    )
                                    .child(if mixed_has_secondary && mixed_split_horizontal {
                                        div()
                                            .id("mixed-secondary-header-column")
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .border_l_1()
                                            .border_color(rgb(BORDER))
                                            .child(
                                                div()
                                                    .id("mixed-secondary-pinned-tab-strip")
                                                    .when(shared_pinned_row, |row| {
                                                        row.h(px(TAB_STRIP_HEIGHT))
                                                            .min_w_0()
                                                            .flex()
                                                            .items_center()
                                                            .overflow_x_scroll()
                                                            .pl(px(TAB_STRIP_LEADING_PADDING))
                                                            .bg(rgb(TOOLBAR))
                                                            .border_b_1()
                                                            .border_color(rgb(BORDER))
                                                            .children((if secondary_two_rows { secondary_pinned_tabs.clone() } else { Vec::new() }).into_iter().enumerate().map(|(index, tab)| {
                                                                let selected = mixed_secondary_active.as_ref() == Some(&tab.identity);
                                                                Self::render_mixed_tab_chip(
                                                                    index + 512,
                                                                    tab,
                                                                    PaneSlot::Secondary,
                                                                    selected,
                                                                    true,
                                                                    mixed_drag_generation,
                                                                    &self.terminals.sessions,
                                                                    self.settings_menu_locale,
                                                                    cx,
                                                                )
                                                            }))
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .id("mixed-secondary-toolbar")
                                                    .h(px(TAB_STRIP_HEIGHT))
                                                    .pr_2()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .bg(rgb(TOOLBAR))
                                                    .border_b_1()
                                                    .border_color(rgb(BORDER))
                                                    .child(
                                                        div()
                                                            .id("mixed-secondary-tabs-scroll")
                                                            .flex_1()
                                                            .min_w_0()
                                                            .flex()
                                                            .items_center()
                                                            .overflow_x_scroll()
                                                            .pl(px(TAB_STRIP_LEADING_PADDING))
                                                            .children(mixed_secondary_tabs.clone().into_iter().enumerate().map(|(index, tab)| {
                                                                let selected = mixed_secondary_active.as_ref() == Some(&tab.identity);
                                                                let pinned = pinned_ids.contains(&tab.identity);
                                                                Self::render_mixed_tab_chip(
                                                                    index, tab, PaneSlot::Secondary, selected, pinned,
                                                                    mixed_drag_generation, &self.terminals.sessions, self.settings_menu_locale, cx,
                                                                )
                                                            }))
                                                            .child(
                                                                div()
                                                                    .id("mixed-secondary-tabs-empty-area")
                                                                    .flex_1()
                                                                    .min_w(px(40.0))
                                                                    .h(px(TAB_STRIP_HEIGHT - 1.0))
                                                                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                                                                        if event.click_count() == 2 {
                                                                            this.new_editor_in_pane(PaneSlot::Secondary, cx);
                                                                        }
                                                                    }))
                                                                    .on_drop(cx.listener(|this, drag: &MixedTabDrag, window, cx| {
                                                                        this.drop_mixed_tab_on_strip_tail(drag, PaneSlot::Secondary, window, cx);
                                                                    })),
                                                            )

                                                    )
                                                    .child({
                                                        let owner = cx.entity().downgrade();
                                                        let refresh_owner = owner.clone();
                                                        let running = self.terminals.sessions.iter()
                                                            .filter(|session| session.running)
                                                            .map(|session| (
                                                                session.clone(),
                                                                format!("{} · {}", session.label(), session.workspace_id),
                                                            ))
                                                            .collect::<Vec<_>>();
                                                        let target = PaneAttachGuard::capture(
                                                            self.workspace_generation,
                                                            PaneSlot::Secondary,
                                                            &self.mixed_panes,
                                                        );
                                                        let profiles = self.agent_profiles.clone();
                                                        let installed = self.installed_agent_clis.clone();
                                                        let installed_loading = self.installed_agent_clis_loading;
                                                        icon_button("mixed-secondary-new-tab", UiIcon::Add, tr(self.settings_menu_locale, "New Tab in second pane"), terminal_creating
                                                            && self.terminal_create_pane.as_ref().is_some_and(|pending| pending.target.pane() == PaneSlot::Secondary))
                                                            .w(px(28.0))
                                                            .h(px(28.0))
                                                            .dropdown_menu(Self::kit_new_tab_menu(
                                                                owner,
                                                                target,
                                                                running,
                                                                profiles,
                                                                installed,
                                                                installed_loading,
                                                                terminal_create_busy,
                                                                terminal_create_timed_out,
                                                                terminal_create_unknown,
                                                            ))
                                                            .on_open_change(move |open, _, cx| {
                                                                if *open {
                                                                    let _ = refresh_owner.update(cx, |view, cx| {
                                                                        view.refresh_terminals(cx);
                                                                        view.refresh_agent_profiles(cx);
                                                                        view.refresh_installed_agent_clis(cx);
                                                                    });
                                                                }
                                                            })
                                                    })
                                                    .child(
                                                        icon_button("native-terminal-history-up-secondary", UiIcon::ChevronUp,
                                                            tr(self.settings_menu_locale, "Scroll second-pane terminal history up (Shift+PageUp)"), false)
                                                            .when(!terminal_is_vt || mixed_secondary_host.is_none(), |button| button.hidden())
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.scroll_native_terminal_page(PaneSlot::Secondary, true, cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("native-terminal-history-down-secondary", UiIcon::ChevronDown,
                                                            tr(self.settings_menu_locale, "Scroll second-pane terminal history down (Shift+PageDown)"), false)
                                                            .when(!terminal_is_vt || mixed_secondary_host.is_none(), |button| button.hidden())
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.scroll_native_terminal_page(PaneSlot::Secondary, false, cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("mixed-secondary-pane-actions", UiIcon::More, tr(self.settings_menu_locale, "Second pane actions"), false)
                                                            .dropdown_menu(Self::kit_pane_actions_menu(
                                                                cx.entity().downgrade(),
                                                                mixed_secondary_active.clone(),
                                                                true,
                                                                mixed_has_secondary,
                                                            )),
                                                    ),
                                            )
                                            .into_any_element()
                                    } else {
                                        div().h(px(0.0)).into_any_element()
                                    }),
                            )
                            .child(
                                div()
                                    .h(px(EDITOR_TOOLBAR_HEIGHT))
                                    .px_4()
                                    .bg(rgb(BG))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .when(graph_in_primary || !show_editor_toolbar || show_center_git_diff, |panel| panel.hidden())
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .child(if has_native_editor {
                                                format!("{}: {}{}", tr(self.settings_menu_locale, "Editing"), editor_title, if editor_dirty { " ●" } else { "" })
                                            } else {
                                                format!("{} · {}", filename, tr(self.settings_menu_locale, "read-only preview"))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                icon_button("native-editor-new", UiIcon::NewFile, tr(self.settings_menu_locale, "Create new untitled editor"), false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.new_editor(cx);
                                                    }))
                                                    .when(has_native_editor, |button| button.hidden()),
                                            )
                                            .child(
                                                icon_button("native-editor-edit", UiIcon::Edit, tr(self.settings_menu_locale, "Edit currently selected file"), false)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.open_editor(cx);
                                                    }))
                                                    .when(has_native_editor, |button| button.hidden()),
                                            )
                                            .when(editor.is_some() && !showing_explorer_preview, |toolbar| {
                                                toolbar
                                                    .child(
                                                        icon_button("native-editor-save", UiIcon::Save, tr(self.settings_menu_locale, "Save current editor file (Ctrl+S)"), false)
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.save_editor(cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("native-editor-save-as", UiIcon::SaveAs, tr(self.settings_menu_locale, "Save current editor as a different file"), false)
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.pick_save_as(cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("native-editor-close", UiIcon::Close, tr(self.settings_menu_locale, "Close current editor tab"), false)
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.close_editor(false, cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("native-editor-discard", if discard_pending { UiIcon::Check } else { UiIcon::Delete }, tr(self.settings_menu_locale, "Discard unsaved changes (confirmation required)"), discard_pending)
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
                                            .when(!center_is_editor && !show_center_git_diff, |pane| {
                                                pane.bg(rgb(TERMINAL_DEFAULT_BG))
                                            })
                            .child(
                                if graph_in_primary {
                                    self.render_center_git_graph(window, cx)
                                } else if let Some(original) = active_git_original.clone() {
                                    div().id("git-original-readonly-view")
                                        .flex_1().min_h_0().flex().flex_col()
                                        .bg(rgb(BG))
                                        .child(div().px_4().py_2().bg(rgb(PANEL))
                                            .flex().items_center().gap_2()
                                            .child(div().flex_1().min_w_0().text_xs().text_color(rgb(TEXT))
                                                .child(format!("Read-only Git original · {}", original.description())))
                                            .child(icon_button("git-original-copy-scratch", UiIcon::NewFile,
                                                "Copy this Git revision into a new editable scratch tab", false)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.copy_git_original_to_scratch(cx);
                                                }))))
                                        .child(div().id("git-original-readonly-scroll")
                                            .flex_1().min_h_0().overflow_y_scroll()
                                            .p_4().text_sm().font_family("JetBrains Mono")
                                            .text_color(rgb(TEXT))
                                            .child(original.snapshot.content))
                                        .into_any_element()
                                } else if show_center_git_diff {
                                    self.render_center_git_diff(cx)
                                } else if let Some(editor) = editor.clone().filter(|_| !showing_explorer_preview) {
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
                                        .font_family(terminal_font_family.clone())
                                        .child(content)
                                        .into_any_element()
                                },
                            )
                            .child(if has_terminal_preview {
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .when(graph_in_primary || native_center_git_diff_ui::should_hide_underlying_terminal(center_is_editor, show_center_git_diff), |panel| panel.hidden())
                                    .border_t_1()
                                    .border_color(rgb(BORDER))
                                    // Match the VT viewport, including the 12px text inset,
                                    // split gutters and any remaining resize slack.
                                    .bg(rgb(TERMINAL_DEFAULT_BG))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .min_h_0()
                                            .flex()
                                            .bg(rgb(TERMINAL_DEFAULT_BG))
                                            .when(terminal_split_horizontal, |layout| layout.flex_row())
                                            .when(!terminal_split_horizontal, |layout| layout.flex_col())
                                            .child(if terminal_is_vt {
                                        div()
                                            .id("terminal-colored-grid")
                                            .relative()
                                            .min_w_0()
                                            .bg(rgb(TERMINAL_DEFAULT_BG))
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
                                                if this.send_terminal_key_for_pane(PaneSlot::Primary, event, cx) {
                                                    cx.stop_propagation();
                                                }
                                            }))
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_x_scroll()
                                            .overflow_y_scroll()
                                            .px_3()
                                            .py_3()
                                            .font_family(terminal_font_family.clone())
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
                                                        .line_height(px(terminal_font_size * terminal_line_height))
                                                        .bg(rgb(if selected { ACTIVE } else { TERMINAL_DEFAULT_BG }))
                                                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, window, cx| {
                                                            // Text rows consume the pointer event; explicitly
                                                            // restore the Terminal IME sink, not the old Explorer.
                                                            this.focus_primary_mixed_terminal(cx);
                                                            let focus = this.terminal_input.read(cx).focus_handle(cx);
                                                            window.focus(&focus, cx);
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
                                            .min_w_0()
                                            .min_h_0()
                                            .bg(rgb(TERMINAL_DEFAULT_BG))
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
                                                if this.send_terminal_key_for_pane(PaneSlot::Primary, event, cx) {
                                                    cx.stop_propagation();
                                                }
                                            }))
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_y_scroll()
                                            .px_3()
                                            .py_3()
                                            .font_family(terminal_font_family.clone())
                                            .text_size(px(terminal_font_size))
                                            .child(terminal_input)
                                            .child(if terminal_output.is_empty() {
                                                "No terminal content yet. Auto updates are enabled by default.".into()
                                            } else {
                                                terminal_output
                                            })
                                            })
                                            .child(if let Some((title, output, styled_rows)) = terminal_secondary_output {
                                                div()
                                                    .id("terminal-secondary-preview")
                                                    .flex_1()
                                                    .min_h_0()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .bg(rgb(TERMINAL_DEFAULT_BG))
                                                    .when(terminal_split_horizontal, |pane| pane.border_l_1())
                                                    .when(!terminal_split_horizontal, |pane| pane.border_t_1())
                                                    .border_color(rgb(BORDER))
                                                    .child(div()
                                                        .id("terminal-secondary-focus")
                                                        .h(px(TERMINAL_INNER_SPLIT_HEADER_HEIGHT))
                                                        // PTY rows reserve this exact 28px: prevent
                                                        // Flex shrinking the focus header in short panes.
                                                        .flex_shrink_0()
                                                        .min_w_0()
                                                        .flex()
                                                        .items_center()
                                                        .px_2()
                                                        .bg(rgb(PANEL))
                                                        .text_sm()
                                                        .cursor_pointer()
                                                        .child(div().min_w_0().truncate().child(format!(
                                                            "{}: {title} · {}",
                                                            tr(self.settings_menu_locale, "Second pane"),
                                                            tr(self.settings_menu_locale, "Click to focus"),
                                                        )))
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
                                                        .min_w_0()
                                                        .bg(rgb(TERMINAL_DEFAULT_BG))
                                                        .overflow_x_scroll()
                                                        .overflow_y_scroll()
                                                        .px_3()
                                                        .py_3()
                                                        .font_family(terminal_font_family.clone())
                                                        .text_size(px(terminal_font_size))
                                                        .whitespace_nowrap()
                                                        .child(if let Some(rows) = styled_rows {
                                                            div()
                                                                .id("terminal-secondary-colored-grid")
                                                                .flex()
                                                                .flex_col()
                                                                .whitespace_nowrap()
                                                                .children(rows.into_iter().enumerate().map(|(row_index, row)| {
                                                                    div()
                                                                        .id(("terminal-secondary-colored-row", row_index))
                                                                        .whitespace_nowrap()
                                                                        .line_height(px(terminal_font_size * terminal_line_height))
                                                                        .bg(rgb(TERMINAL_DEFAULT_BG))
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
                                                            div().child(output).into_any_element()
                                                        }))
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
                                    .when(graph_in_primary || native_center_git_diff_ui::should_hide_underlying_terminal(center_is_editor, show_center_git_diff), |panel| panel.hidden())
                                    .p_4()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .child(tr(self.settings_menu_locale, "No Host terminal selected. Choose an existing session in Projects on the left."))
                                            .child(
                                                div()
                                                    .text_color(rgb(TEXT))
                                                    .child(terminal_status),
                                            ),
                                    )
                                    .into_any_element()
                            }),
                                    )
                                    .child(if let Some((_, preview)) = mixed_secondary_preview {
                                        div()
                                            .id("mixed-secondary-pane")
                                            .flex_1()
                                            .min_w_0()
                                            .min_h_0()
                                            .flex()
                                            .flex_col()
                                            .when(mixed_split_horizontal, |pane| pane.border_l_1())
                                            .when(!mixed_split_horizontal, |pane| pane.border_t_1())
                                            .border_color(rgb(BORDER))
                                            // Stacked panes keep their own header; horizontal
                                            // panes share the aligned header above this split.
                                            .child(if mixed_split_horizontal {
                                                div().h(px(0.0)).into_any_element()
                                            } else {
                                                div()
                                                    .id("mixed-secondary-stacked-header")
                                                    .flex()
                                                    .flex_col()
                                                    .min_w_0()
                                            .child(
                                                div()
                                                    .id("mixed-secondary-pinned-tab-strip")

                                                    .when(secondary_two_rows, |row| {
                                                        row.h(px(TAB_STRIP_HEIGHT))
                                                            .min_w_0()
                                                            .flex()
                                                            .items_center()
                                                            .overflow_x_scroll()
                                                            .pl(px(TAB_STRIP_LEADING_PADDING))
                                                            .bg(rgb(TOOLBAR))
                                                            .border_b_1()
                                                            .border_color(rgb(BORDER))
                                                            .children(secondary_pinned_tabs.into_iter().enumerate().map(|(index, tab)| {
                                                                let selected = mixed_secondary_active.as_ref() == Some(&tab.identity);
                                                                Self::render_mixed_tab_chip(
                                                                    index + 512,
                                                                    tab,
                                                                    PaneSlot::Secondary,
                                                                    selected,
                                                                    true,
                                                                    mixed_drag_generation,
                                                                    &self.terminals.sessions,
                                                                    self.settings_menu_locale,
                                                                    cx,
                                                                )
                                                            }))
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .id("mixed-secondary-toolbar")

                                                    .h(px(TAB_STRIP_HEIGHT))
                                                    .pr_2()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1()
                                                    .bg(rgb(TOOLBAR))
                                                    .border_b_1()
                                                    .border_color(rgb(BORDER))
                                                    .child(
                                                        div()
                                                            .id("mixed-secondary-tabs-scroll")
                                                            .flex_1()
                                                            .min_w_0()
                                                            .flex()
                                                            .items_center()
                                                            .overflow_x_scroll()
                                                            .pl(px(TAB_STRIP_LEADING_PADDING))
                                                            .children(mixed_secondary_tabs.into_iter().enumerate().map(|(index, tab)| {
                                                                let selected = mixed_secondary_active.as_ref() == Some(&tab.identity);
                                                                let pinned = pinned_ids.contains(&tab.identity);
                                                                Self::render_mixed_tab_chip(
                                                                    index, tab, PaneSlot::Secondary, selected, pinned,
                                                                    mixed_drag_generation, &self.terminals.sessions, self.settings_menu_locale, cx,
                                                                )
                                                            }))
                                                            .child(
                                                                div()
                                                                    .id("mixed-secondary-tabs-empty-area")
                                                                    .flex_1()
                                                                    .min_w(px(40.0))
                                                                    .h(px(TAB_STRIP_HEIGHT - 1.0))
                                                                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                                                                        if event.click_count() == 2 {
                                                                            this.new_editor_in_pane(PaneSlot::Secondary, cx);
                                                                        }
                                                                    }))
                                                                    .on_drop(cx.listener(|this, drag: &MixedTabDrag, window, cx| {
                                                                        this.drop_mixed_tab_on_strip_tail(drag, PaneSlot::Secondary, window, cx);
                                                                    })),
                                                            )

                                                    )
                                                    .child({
                                                        let owner = cx.entity().downgrade();
                                                        let refresh_owner = owner.clone();
                                                        let running = self.terminals.sessions.iter()
                                                            .filter(|session| session.running)
                                                            .map(|session| (
                                                                session.clone(),
                                                                format!("{} · {}", session.label(), session.workspace_id),
                                                            ))
                                                            .collect::<Vec<_>>();
                                                        let target = PaneAttachGuard::capture(
                                                            self.workspace_generation,
                                                            PaneSlot::Secondary,
                                                            &self.mixed_panes,
                                                        );
                                                        let profiles = self.agent_profiles.clone();
                                                        let installed = self.installed_agent_clis.clone();
                                                        let installed_loading = self.installed_agent_clis_loading;
                                                        icon_button("mixed-secondary-new-tab", UiIcon::Add, tr(self.settings_menu_locale, "New Tab in second pane"), terminal_creating
                                                            && self.terminal_create_pane.as_ref().is_some_and(|pending| pending.target.pane() == PaneSlot::Secondary))
                                                            .w(px(28.0))
                                                            .h(px(28.0))
                                                            .dropdown_menu(Self::kit_new_tab_menu(
                                                                owner,
                                                                target,
                                                                running,
                                                                profiles,
                                                                installed,
                                                                installed_loading,
                                                                terminal_create_busy,
                                                                terminal_create_timed_out,
                                                                terminal_create_unknown,
                                                            ))
                                                            .on_open_change(move |open, _, cx| {
                                                                if *open {
                                                                    let _ = refresh_owner.update(cx, |view, cx| {
                                                                        view.refresh_terminals(cx);
                                                                        view.refresh_agent_profiles(cx);
                                                                        view.refresh_installed_agent_clis(cx);
                                                                    });
                                                                }
                                                            })
                                                    })
                                                    .child(
                                                        icon_button("native-terminal-history-up-secondary", UiIcon::ChevronUp,
                                                            tr(self.settings_menu_locale, "Scroll second-pane terminal history up (Shift+PageUp)"), false)
                                                            .when(!terminal_is_vt || mixed_secondary_host.is_none(), |button| button.hidden())
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.scroll_native_terminal_page(PaneSlot::Secondary, true, cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("native-terminal-history-down-secondary", UiIcon::ChevronDown,
                                                            tr(self.settings_menu_locale, "Scroll second-pane terminal history down (Shift+PageDown)"), false)
                                                            .when(!terminal_is_vt || mixed_secondary_host.is_none(), |button| button.hidden())
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.scroll_native_terminal_page(PaneSlot::Secondary, false, cx);
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button("mixed-secondary-pane-actions", UiIcon::More, tr(self.settings_menu_locale, "Second pane actions"), false)
                                                            .dropdown_menu(Self::kit_pane_actions_menu(
                                                                cx.entity().downgrade(),
                                                                mixed_secondary_active.clone(),
                                                                true,
                                                                mixed_has_secondary,
                                                            )),
                                                    ),
                                            )
                                                    .into_any_element()
                                            })
                                            .child(if graph_in_secondary {
                                                self.render_center_git_graph(window, cx)
                                            } else if let Some(editor) = secondary_native_editor {
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
                                                    .min_w_0()
                                                    .bg(rgb(TERMINAL_DEFAULT_BG))
                                                    .when(mixed_split_horizontal, |panel| panel.border_t_1())
                                                    .border_color(rgb(BORDER))
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
                                                    .font_family(terminal_font_family.clone())
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
                                                                let selected = terminal_secondary_selected_rows.get(index).copied().unwrap_or(false);
                                                                div()
                                                                    .id(("mixed-secondary-colored-row", index))
                                                                    .whitespace_nowrap()
                                                                    .line_height(px(terminal_font_size * terminal_line_height))
                                                                    .bg(rgb(if selected { ACTIVE } else { TERMINAL_DEFAULT_BG }))
                                                                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, window, cx| {
                                                                        if let Some(WorkbenchTabIdentity::Host(target)) =
                                                                            this.mixed_panes.active_in(PaneSlot::Secondary).cloned()
                                                                        {
                                                                            if this.terminals.context_target_is_live(&target) {
                                                                                if let Some(tab_index) = this.terminals.index_for_context(&target) {
                                                                                    this.mixed_panes.focus_pane(PaneSlot::Secondary);
                                                                                    this.terminals.activate(tab_index);
                                                                                    this.center_view = CenterView::Terminal;
                                                                                    window.focus(&this.secondary_terminal_input.read(cx).focus_handle(cx), cx);
                                                                                    this.begin_terminal_line_selection_for_pane(PaneSlot::Secondary, index, cx);
                                                                                }
                                                                            }
                                                                        }
                                                                    }))
                                                                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                                                                        this.extend_terminal_line_selection(index, cx);
                                                                    }))
                                                                    .on_mouse_up(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                                                                        this.finish_terminal_line_selection(cx);
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
                            .id("projects-sidebar-resize-handle")
                            .when(self.projects_sidebar_collapsed, |handle| handle.hidden())
                            .w(px(SIDEBAR_RESIZE_HANDLE_WIDTH))
                            .flex_shrink_0()
                            .h_full()
                            .bg(rgb(BORDER))
                            .cursor(gpui::CursorStyle::ResizeLeftRight)
                            .hover(|style| style.bg(rgb(BORDER_ACTIVE)))
                            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                this.sidebar_resize_drag = Some(SidebarResizeDrag {
                                    side: SidebarResizeSide::Projects,
                                    start_x: f32::from(event.position.x),
                                    start_width: this.projects_sidebar_width,
                                });
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("projects-sidebar-expanded")
                            .w(px(projects_width))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .when(self.projects_sidebar_collapsed, |panel| panel.hidden())
                            .bg(rgb(PANEL))
                            .border_r_1()
                            .border_color(rgb(BORDER))
                            .px_2()
                            .gap_1()
                            // Classic sidebar: brand header, search, then actions.
                            .child(
                                div()
                                    .id("projects-brand-header")
                                    .h(px(SECTION_HEADER_HEIGHT))
                                    .px_1()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(gpui::img(classic_brand_logo()).w(px(16.0)).h(px(16.0)))
                                    .child(div().text_sm().font_weight(gpui::FontWeight::SEMIBOLD).child("Alera"))
                                    .child(div().flex_1())
                                    .child(icon_button(
                                        "projects-sidebar-collapse", UiIcon::PanelLeft, "Collapse Sidebar", false,
                                    ).on_click(cx.listener(|this, _, _, cx| {
                                        this.projects_sidebar_collapsed = true;
                                        this.save_sidebar_layout(cx);
                                        cx.notify();
                                    }))),
                            )
                            .child(
                                div()
                                    .id("projects-sidebar-search")
                                    .child(sidebar_search_input),
                            )
                            .child(
                                div()
                                    .id("projects-sidebar-actions")
                                    .h(px(36.0))
                                    .px_1()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .id("projects-toggle")
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .cursor_pointer()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child(icon(if show_recent_workspaces { UiIcon::ChevronDown } else { UiIcon::ChevronRight }, 12.0, MUTED))
                                            .child("Recent")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.show_recent_workspaces = !this.show_recent_workspaces;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        icon_button("projects-open-folder", UiIcon::NewFolder, "Open a project folder…", picking_workspace)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.pick_workspace(cx);
                                            })),
                                    ),
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
                                    .child(if let Some(error) = runtime_catalog_error {
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child("Classic unavailable"),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .id("project-catalog-details")
                                                            .cursor_pointer()
                                                            .text_xs()
                                                            .text_color(rgb(MUTED))
                                                            .child("Details")
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.model.set_status(format!(
                                                                    "Classic catalog query failed: {error}"
                                                                ));
                                                                cx.notify();
                                                            })),
                                                    )
                                                    .child(
                                                        icon_button(
                                                            "project-catalog-inline-retry",
                                                            UiIcon::Refresh,
                                                            "Retry Classic catalog synchronization",
                                                            project_tree_loading,
                                                        )
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.refresh_project_tree(cx);
                                                        })),
                                                    ),
                                            )
                                            .into_any_element()
                                    } else {
                                        div().h(px(0.0)).into_any_element()
                                    })
                                    .child(if let Some(input) = sidebar_rename_input {
                                        div().flex().flex_col().gap_1().p_2()
                                            .child(div().text_xs().text_color(rgb(MUTED)).child("Rename Classic project / Workspace"))
                                            .child(input)
                                            .child(div().flex().gap_3()
                                                .child(div().id("sidebar-rename-confirm").cursor_pointer().text_sm()
                                                    .child("Save display name")
                                                    .on_click(cx.listener(|this, _, _, cx| this.submit_sidebar_rename(cx))))
                                                .child(div().id("sidebar-rename-cancel").cursor_pointer().text_sm()
                                                    .child("Cancel")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.sidebar_rename_target = None;
                                                        this.sidebar_rename_input = None;
                                                        cx.notify();
                                                    }))))
                                            .into_any_element()
                                    } else {
                                        div().h(px(0.0)).into_any_element()
                                    })
                                    .child(if let Some(input) = sidebar_create_input {
                                        div().flex().flex_col().gap_1().p_2()
                                            .child(div().text_xs().text_color(rgb(MUTED)).child("New Classic tag / workspace section"))
                                            .child(input)
                                            .child(div().flex().gap_3()
                                                .child(div().id("sidebar-create-confirm").cursor_pointer().text_sm()
                                                    .child("Create")
                                                    .on_click(cx.listener(|this, _, _, cx| this.submit_sidebar_create(cx))))
                                                .child(div().id("sidebar-create-cancel").cursor_pointer().text_sm()
                                                    .child("Cancel")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.sidebar_create_target = None;
                                                        this.sidebar_create_input = None;
                                                        cx.notify();
                                                    }))))
                                            .into_any_element()
                                    } else {
                                        div().h(px(0.0)).into_any_element()
                                    })
                                    .child(if sidebar_search_empty {
                                        div()
                                            .id("project-search-empty")
                                            .px_2()
                                            .py_3()
                                            .text_sm()
                                            .text_color(rgb(MUTED))
                                            .child("No matching projects or worktrees")
                                            .into_any_element()
                                    } else {
                                        div().h(px(0.0)).into_any_element()
                                    })
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
                                                let menu_project_id = runtime_catalog.as_ref()
                                                    .and_then(|catalog| catalog.projects.iter().find(|project| project.root == group.root))
                                                    .map(|project| project.id.clone());
                                                let project_remove_armed = menu_project_id.as_deref()
                                                    == sidebar_project_remove_id.as_deref();
                                                let project_context_root = group.root.clone();
                                                let worktree_project_root = group.root.clone();
                                                let title = runtime_catalog.as_ref()
                                                    .and_then(|catalog| catalog.projects.iter().find(|project| project.root == group.root))
                                                    .map(|project| project.name.clone())
                                                    .unwrap_or_else(|| group.label.clone());
                                                let show_all_worktrees = project_title_matches_search(
                                                    &group, runtime_catalog.as_ref(), &sidebar_search_query,
                                                );
                                                let rows = order_worktree_rows(
                                                    &sidebar_workspaces_for_group(&group, runtime_catalog.as_ref()),
                                                    runtime_catalog.as_ref(),
                                                );
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
                                                                    }))
                                                                    .context_menu(Self::sidebar_project_context_menu(
                                                                        cx.entity().downgrade(), project_context_root, pinned,
                                                                        menu_project_id,
                                                                        project_remove_armed,
                                                                    )),
                                                            ),
                                                    )
                                                    .children(rows.into_iter()
                                                        .filter(|row| !sidebar_search_active || show_all_worktrees
                                                            || worktree_matches_search(&row.entry, runtime_catalog.as_ref(), &sidebar_search_query))
                                                        .take(if collapsed { 0 } else { 96 })
                                                        .enumerate().map(|(child_index, row)| {
                                                        let entry = row.entry;
                                                        let section_header = row.section_header;
                                                        let depth = row.depth;
                                                        let target = entry.path.clone();
                                                        let active = workspace_root == target;
                                                        let menu_root = worktree_project_root.clone();
                                                        let menu_path = target.clone();
                                                        let menu_catalog = runtime_catalog.clone();
                                                        let menu_confirmation = sidebar_pending_action.clone();
                                                        let menu_selected_parent = selected_sidebar_parent.clone();
                                                        let metadata = runtime_catalog.as_ref().and_then(|catalog|
                                                            catalog.workspaces.iter().find(|workspace| workspace.path == target)
                                                        );
                                                        let label = metadata.map(|workspace| {
                                                            let mut text = workspace.name.clone();
                                                            if workspace.is_pinned { text.push_str(" ★"); }
                                                            if workspace.is_archived { text.push_str(" (archived)"); }
                                                            if !workspace.tag_names.is_empty() { text.push_str(&format!(" [{}]", workspace.tag_names.join(", "))); }
                                                            text
                                                        }).unwrap_or_else(|| entry.branch.unwrap_or_else(|| {
                                                            if entry.detached { "Detached HEAD".into() } else { "Unnamed".into() }
                                                        }));
                                                        div()
                                                            .id(("project-tree-worktree-container", index * 128 + child_index))
                                                            .flex().flex_col()
                                                            .child(if let Some(section) = section_header {
                                                                div().pl_3().pt_1().text_xs()
                                                                    .text_color(rgb(MUTED))
                                                                    .child(section).into_any_element()
                                                            } else { div().h(px(0.0)).into_any_element() })
                                                            .child(div()
                                                            .id(("project-tree-worktree", index * 128 + child_index))
                                                            .pl(px(16.0 + depth.min(8) as f32 * 14.0))
                                                            .py_1()
                                                            .bg(rgb(if active { ACTIVE } else { PANEL }))
                                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                                            .cursor_pointer()
                                                            .text_xs()
                                                            .text_color(rgb(if active { TEXT } else { MUTED }))
                                                            .child(format!(
                                                                "{} {} {}",
                                                                if active { "●" } else { "○" },
                                                                if depth > 0 { "↳" } else if entry.is_primary { "main:" } else { "worktree:" },
                                                                label
                                                            ))
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.switch_workspace(target.clone(), cx);
                                                            }))
                                                            .context_menu(Self::sidebar_workspace_context_menu(
                                                                cx.entity().downgrade(), menu_root, menu_path,
                                                                metadata.cloned(), menu_catalog, menu_confirmation,
                                                                menu_selected_parent, active,
                                                            )))
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
                                    .child(div().text_xs().text_color(rgb(MUTED))
                                        .child(format!("Project: {}", worktree_create_root.as_deref()
                                            .unwrap_or(workspace_root.as_path()).display())))
                                    .child(input)
                                    .child(
                                        div().id("worktree-create-branch-mode")
                                            .cursor_pointer().text_xs().text_color(rgb(MUTED))
                                            .child(if worktree_create_from_existing {
                                                "Use existing local branch · change"
                                            } else {
                                                "Create new local branch · change"
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.worktree_create_from_existing = !this.worktree_create_from_existing;
                                                cx.notify();
                                            }))
                                    )
                                    .child(
                                        div()
                                            .id("worktree-create-confirm")
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(rgb(TEXT))
                                            .child(if worktree_busy { "Creating…" } else if worktree_create_from_existing {
                                                "Checkout existing branch into Worktree"
                                            } else { "Create branch + Worktree" })
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
                                    .when(agent_state_summary.total() > 0, |header| {
                                        header.child(native_agent_quota_ui::agent_state_summary_chips(
                                            agent_state_summary, agent_ui_locale, false,
                                        ))
                                    })
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
                                    let brand = profile.agent_type.trim().to_ascii_lowercase();
                                    let kind = if profile.managed { "Managed" } else { "Command" };
                                    let label = format!("{} · {} ({kind})", profile.name, profile.agent_type);
                                    div()
                                        .id(("agent-profile-row", index))
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(rgb(if selected { TEXT } else { MUTED }))
                                        .hover(|style| style.bg(rgb(ACTIVE)))
                                        .py_1()
                                        .flex()
                                        .min_w_0()
                                        .items_center()
                                        .gap_2()
                                        // Keep the pre-existing selected/unselected state,
                                        // separate from the Agent's brand identity.
                                        .child(div().w(px(10.0)).flex_shrink_0().child(
                                            if selected { "●" } else { "○" },
                                        ))
                                        .child(native_agent_quota_ui::quota_provider_icon(&brand, 16.0))
                                        .child(div().min_w_0().truncate().child(label))
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
                                            this.agent_launch_pane = None;
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
                                    .children(terminal_sessions.into_iter().enumerate().map(|(index, (handle, label, workspace_id, agent_type, visual_state))| {
                                        let brand = agent_type.as_deref().unwrap_or("terminal").trim().to_ascii_lowercase();
                                        let row_status_tooltip = visual_state
                                            .map(|state| state.tooltip(&brand, agent_ui_locale));
                                        div()
                                            .id(("terminal-session", index))
                                            // The whole Session row is easier to hover than a
                                            // 6px status dot; both show the same state.
                                            .when(row_status_tooltip.is_some(), |row| {
                                                let tip = row_status_tooltip.clone().unwrap_or_default();
                                                row.tooltip(move |window, cx| {
                                                    gpui_kit::component::tooltip::Tooltip::new(tip.clone())
                                                        .build(window, cx)
                                                })
                                            })
                                            .py_1()
                                            .px_1()
                                            .cursor_pointer()
                                            .text_sm()
                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(native_agent_quota_ui::quota_provider_icon(&brand, 14.0))
                                                    // Like Classic's sidebar, animate only Working,
                                                    // and reserve a stable slot for every Hook state.
                                                    .child(
                                                        div()
                                                            .id(("host-agent-status-dot", index))
                                                            .w(px(12.0))
                                                            .h(px(12.0))
                                                            .flex_shrink_0()
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .when(visual_state.is_some(), |dot| {
                                                                let state = visual_state.unwrap();
                                                                let tooltip = state.tooltip(&brand, agent_ui_locale);
                                                                dot.child(native_agent_quota_ui::agent_run_indicator(
                                                                    state,
                                                                    if state == AgentVisualState::Working { 11.0 } else { TAB_ATTENTION_DOT_SIZE },
                                                                ))
                                                                .tooltip(move |window, cx| {
                                                                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                                                                        .build(window, cx)
                                                                })
                                                            })
                                                    )
                                                    .child(div().min_w_0().truncate().child(label)),
                                            )
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
                    )
                    .child(
                        div()
                            .id("projects-sidebar-rail")
                            .w(px(PROJECTS_COLLAPSED_WIDTH))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .when(!self.projects_sidebar_collapsed, |rail| rail.hidden())
                            .items_center()
                            .bg(rgb(PANEL))
                            .border_r_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .h(px(SECTION_HEADER_HEIGHT))
                                    .flex()
                                    .items_center()
                                    .child(icon_button(
                                        "projects-sidebar-expand", UiIcon::PanelLeft, "Expand Sidebar", false,
                                    ).on_click(cx.listener(|this, _, _, cx| {
                                        this.projects_sidebar_collapsed = false;
                                        this.save_sidebar_layout(cx);
                                        cx.notify();
                                    }))),
                            )
                            .child(icon_button(
                                "projects-sidebar-rail-open", UiIcon::NewFolder, "Open project folder", false,
                            ).on_click(cx.listener(|this, _, _, cx| {
                                this.pick_workspace(cx);
                            }))),
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
                    .child(
                        div()
                            .id("native-runtime-status")
                            .max_w(px(320.0))
                            .min_w_0()
                            .overflow_hidden()
                            .child(status),
                    )
                    .when(agent_state_summary.total() > 0, |footer| {
                        footer.child(
                            native_agent_quota_ui::agent_state_summary_chips(
                                agent_state_summary, agent_ui_locale, true,
                            )
                        )
                    })
                    .child(self.render_native_quota_status(cx))
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
    // Only this process registers its ephemeral class factory. Persistent
    // registry/Start Menu registration remains an explicit Native-only action.
    #[cfg(windows)]
    let _native_toast_com =
        match alera_gpui_desktop::native_toast_activation::NativeToastComRegistration::register() {
            Ok(registration) => Some(registration),
            Err(error) => {
                eprintln!("Alera Native: Windows toast COM activation unavailable: {error}");
                None
            }
        };
    let session_store = PreviewSessionStore::platform_default();
    // Windows COM LocalServer32 may launch with -Embedding instead of a
    // workspace argument. It must not be opened as a relative project path.
    let launch_arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let toast_activated_launch = launch_arguments.iter().any(|argument| {
        let arg = argument.to_string_lossy();
        arg.eq_ignore_ascii_case("-ToastActivated")
    });
    if toast_activated_launch {
        eprintln!("Alera Native: started for Windows Action Center toast activation");
    }
    let root = launch_arguments
        .into_iter()
        .find(|argument| {
            let arg = argument.to_string_lossy();
            !arg.eq_ignore_ascii_case("-Embedding")
                && !arg.eq_ignore_ascii_case("/Embedding")
                && !arg.eq_ignore_ascii_case("-ToastActivated")
        })
        .map(std::path::PathBuf::from)
        .or_else(|| {
            session_store.as_ref().and_then(|store| {
                store.last_workspace().unwrap_or_else(|error| {
                    eprintln!("Alera GPUI: cannot load last workspace: {error}");
                    None
                })
            })
        })
        .or_else(|| {
            if toast_activated_launch {
                std::env::var_os("USERPROFILE").map(std::path::PathBuf::from)
            } else {
                None
            }
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
    let initial_review_url = alera_core::git::repository_remote_url(
        &model.root().to_string_lossy(),
    ).ok().flatten().and_then(|remote| repository_pull_requests_url(&remote));
    let initial_visibility = ExplorerVisibilityOptions {
        show_ignored: native_settings.explorer_show_ignored,
        show_hidden: native_settings.explorer_show_hidden,
    };

    let initial_native_scrollback_lines = native_settings.terminal_scrollback_lines as usize;

    gpui_kit::application()
        // Match Classic's Lucide action set in addition to Codicon Git glyphs.
        .with_assets(gpui_kit::assets::AllAssets)
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
                KeyBinding::new("ctrl-c", GitCopyFiles, Some("AleraGpuiGit")),
                KeyBinding::new("cmd-c", GitCopyFiles, Some("AleraGpuiGit")),
                KeyBinding::new("ctrl-x", GitCutFiles, Some("AleraGpuiGit")),
                KeyBinding::new("cmd-x", GitCutFiles, Some("AleraGpuiGit")),
                KeyBinding::new("ctrl-v", GitPasteFiles, Some("AleraGpuiGit")),
                KeyBinding::new("cmd-v", GitPasteFiles, Some("AleraGpuiGit")),
                KeyBinding::new("ctrl-a", ExplorerSelectAll, Some("AleraGpuiExplorer")),
                KeyBinding::new("cmd-a", ExplorerSelectAll, Some("AleraGpuiExplorer")),
                KeyBinding::new("f2", ExplorerRename, Some("AleraGpuiExplorer")),
                KeyBinding::new("delete", ExplorerDelete, Some("AleraGpuiExplorer")),
                KeyBinding::new("f5", ExplorerRefresh, Some("AleraGpuiExplorer")),
                KeyBinding::new("up", ExplorerUp, Some("AleraGpuiExplorer")),
                KeyBinding::new("down", ExplorerDown, Some("AleraGpuiExplorer")),
                KeyBinding::new("home", ExplorerFirst, Some("AleraGpuiExplorer")),
                KeyBinding::new("end", ExplorerLast, Some("AleraGpuiExplorer")),
                KeyBinding::new("enter", ExplorerOpen, Some("AleraGpuiExplorer")),
                KeyBinding::new("space", ExplorerPreview, Some("AleraGpuiExplorer")),
                KeyBinding::new("ctrl-n", ExplorerNewFile, Some("AleraGpuiExplorer")),
                KeyBinding::new("cmd-n", ExplorerNewFile, Some("AleraGpuiExplorer")),
                KeyBinding::new("ctrl-shift-n", ExplorerNewFolder, Some("AleraGpuiExplorer")),
                KeyBinding::new("cmd-shift-n", ExplorerNewFolder, Some("AleraGpuiExplorer")),
                KeyBinding::new("ctrl-shift-f", ToggleContentSearch, None),
                KeyBinding::new("cmd-shift-f", ToggleContentSearch, None),
                KeyBinding::new("ctrl-enter", RunContentSearch, Some("AleraGpuiSearch")),
                KeyBinding::new("cmd-enter", RunContentSearch, Some("AleraGpuiSearch")),
                KeyBinding::new("shift-f12", FindTextReferences, Some("AleraGpuiEditor")),
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
                        let git_graph_search_input = cx.new(|cx| {
                            QuickOpenInput::with_placeholder(
                                "Search Graph · author:, sha:, message:, branch:, tag:",
                                cx,
                            )
                        });
                        cx.observe(&git_graph_search_input, |_: &mut Workbench, _, cx| {
                            cx.notify();
                        })
                        .detach();
                        let git_branch_picker_input = cx.new(|cx| {
                            QuickOpenInput::with_placeholder("Filter local or remote branches…", cx)
                        });
                        cx.observe(&git_branch_picker_input, |_: &mut Workbench, _, cx| cx.notify())
                            .detach();
                        let sidebar_search_input = cx.new(|cx| {
                            QuickOpenInput::with_placeholder("Search projects / worktrees…", cx)
                        });
                        cx.observe(&sidebar_search_input, |_, _, cx| cx.notify())
                            .detach();
                        let settings_menu_search =
                            cx.new(|cx| QuickOpenInput::with_placeholder("Search settings", cx));
                        cx.observe(&settings_menu_search, |_, _, cx| cx.notify())
                            .detach();
                        let settings_shortcut_search = cx.new(|cx| {
                            QuickOpenInput::with_placeholder("Search shortcut or key…", cx)
                        });
                        cx.observe(&settings_shortcut_search, |_, _, cx| cx.notify())
                            .detach();
                        let settings_exclusion_input = cx.new(|cx| {
                            QuickOpenInput::with_placeholder(
                                "Add directory name (e.g. generated)",
                                cx,
                            )
                        });
                        cx.observe(&settings_exclusion_input, |_, _, cx| cx.notify())
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
                            explorer_scroll: ScrollHandle::new(),
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
                            right_sidebar_view: if native_settings.sidebar_layout.context_is_pull_requests {
                                RightSidebarView::PullRequests
                            } else if native_settings.sidebar_layout.context_is_references {
                                RightSidebarView::References
                            } else if native_settings.sidebar_layout.context_is_search {
                                RightSidebarView::Search
                            } else if native_settings.sidebar_layout.context_is_git {
                                RightSidebarView::Git
                            } else {
                                RightSidebarView::Explorer
                            },
                            projects_sidebar_collapsed: native_settings.sidebar_layout.projects_collapsed,
                            projects_sidebar_width: native_settings.sidebar_layout.projects_width,
                            right_sidebar_visible: native_settings.sidebar_layout.context_visible,
                            right_sidebar_width: native_settings.sidebar_layout.context_width,
                            sidebar_resize_drag: None,
                            sidebar_save_busy: false,
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
                            runtime_catalog: None,
                            runtime_catalog_error: None,
                            project_settings_project_id: None,
                            project_settings_selected_project_id: None,
                            project_settings_rows: Vec::new(),
                            project_settings_loading: false,
                            project_settings_error: None,
                            project_settings_request: 0,
                            project_settings_config: None,
                            project_settings_source: None,
                            project_settings_repo_error: false,
                            project_settings_use_repo_confirmed: false,
                            project_settings_editing: false,
                            project_settings_draft_provider: None,
                            project_settings_prompt_input: cx.new(|cx| QuickOpenInput::profile_multiline(cx)),
                            project_settings_copy_inputs: Vec::new(),
                            project_settings_setup_inputs: Vec::new(),
                            project_settings_save_busy: false,
                            project_settings_save_status: String::new(),
                            project_settings_mutation_request: 0,
                            sidebar_pending_action: None,
                            sidebar_rename_target: None,
                            sidebar_rename_input: None,
                            sidebar_selected_parent: None,
                            sidebar_project_remove_pending: None,
                            sidebar_create_target: None,
                            sidebar_create_input: None,
                            project_tree_loading: false,
                            project_tree_request: 0,
                            collapsed_project_roots: HashSet::new(),
                            show_recent_workspaces: false,
                            sidebar_search_input,
                            worktrees: Vec::new(),
                            worktree_error: None,
                            worktree_loading: false,
                            worktree_busy: false,
                            worktree_create_input: None,
                            worktree_create_root: None,
                            worktree_create_from_existing: false,
                            worktree_delete_pending: None,
                            picking_workspace: false,
                            quick_open: None,
                            quick_open_selected_index: 0,
                            quick_open_selection_query: String::new(),
                            quick_open_results_scroll: ScrollHandle::new(),
                            explorer_filter_input,
                            file_index: None,
                            file_index_loading: false,
                            file_index_request: 0,
                            content_query: Some(cx.new(|cx| {
                                QuickOpenInput::with_placeholder("Search in files…", cx)
                            })),
                            content_search_scope: WorktreeContentScope::Current,
                            content_worktrees_searched: 0,
                            content_worktrees_skipped: 0,
                            content_loading: false,
                            content_matches: Vec::new(),
                            content_truncated: false,
                            content_error: None,
                            content_searched_query: String::new(),
                            content_request: 0,
                            references_symbol: None,
                            references_matches: Vec::new(),
                            references_loading: false,
                            references_error: None,
                            references_truncated: false,
                            references_request: 0,
                            review_browser_url: initial_review_url,
                            review_browser_error: None,
                            git_snapshot: None,
                            git_original_tabs: GitOriginalTabs::default(),
                            git_original_active: false,
                            git_original_request: 0,
                            git_selected_changes: HashSet::new(),
                            git_selected_folders: HashSet::new(),
                            git_keyboard_focus: cx.focus_handle(),
                            git_selection_anchor: None,
                            git_discard_armed: None,
                            git_trash_preview: None,
                            git_trash_loading: false,
                            git_trash_request: 0,
                            git_sidebar_options: GitSidebarOptions::default(),
                            git_sidebar_patch_expanded: false,
                            git_advanced_actions_open: false,
                            git_history: None,
                            git_graph_render_cache: None,
                            git_center_graph_scroll: UniformListScrollHandle::new(),
                            git_center_graph_filter: String::new(),
                            git_history_open: false,
                            git_history_sidebar_open: false,
                            git_history_tab_open: false,
                            git_graph_center_open: false,
                            git_graph_ref_tab: GitGraphRefTab::Local,
                            git_history_options: GitHistoryOptions {
                                ..Default::default()
                            },
                            git_favorites_busy: false,
                            git_favorites_request: 0,
                            git_favorites_last_refresh: Instant::now()
                                .checked_sub(Duration::from_secs(20))
                                .unwrap_or_else(Instant::now),
                            git_history_loading: false,
                            git_history_watch_pending: false,
                            git_history_error: None,
                            git_history_request: 0,
                            git_history_cancel: Arc::new(AtomicBool::new(false)),
                            git_merge_armed: None,
                            git_graph_remote_armed: None,
                            git_branch_delete_armed: None,
                            git_commit_ref_target: None,
                            git_commit_ref_armed: None,
                            git_commit_apply_preview: None,
                            git_commit_apply_armed: false,
                            git_commit_apply_loading: false,
                            git_commit_apply_request: 0,
                            git_soft_reset_preview: None,
                            git_soft_reset_armed: false,
                            git_soft_reset_mode: NativeResetMode::Soft,
                            git_soft_reset_loading: false,
                            git_soft_reset_request: 0,
                            git_rebase_preview: None,
                            git_rebase_armed: false,
                            git_rebase_loading: false,
                            git_rebase_request: 0,
                            git_worktree_commit_target: None,
                            git_worktree_commit_armed: None,
                            git_worktree_commit_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("New worktree branch", cx)
                            }),
                            git_rename_source: None,
                            git_rename_armed: None,
                            git_sensitive_armed: None,
                            git_conflict_preview: None,
                            git_conflict_choices: HashMap::new(),
                            git_conflict_selected: None,
                            git_conflict_loading: false,
                            git_conflict_request: 0,
                            git_conflict_confirm_armed: false,
                            git_filter_input: cx
                                .new(|cx| QuickOpenInput::with_placeholder("Filter files…", cx)),
                            git_graph_search_input,
                            git_branch_picker_input,
                            git_branch_picker_mode: None,
                            git_amend_preview: None,
                            git_amend_loading: false,
                            git_amend_request: 0,
                            git_commit_input: cx
                                .new(|cx| QuickOpenInput::with_placeholder("Commit message", cx)),
                            git_commit_draft_loading: false,
                            git_commit_draft_request: 0,
                            git_ai_prompt_preview: None,
                            git_ai_prompt_loading: false,
                            git_ai_prompt_request: 0,
                            git_ai_generate_loading: false,
                            git_ai_generate_request: 0,
                            git_ai_generate_cancel: Arc::new(AtomicBool::new(false)),
                            git_branch_input: cx
                                .new(|cx| QuickOpenInput::with_placeholder("Existing branch", cx)),
                            git_rename_input: cx
                                .new(|cx| QuickOpenInput::with_placeholder("New branch name", cx)),
                            git_commit_ref_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("New branch or tag name", cx)
                            }),
                            git_action_busy: false,
                            git_action_request: 0,
                            git_action_status: String::new(),
                            git_loading: false,
                            git_refresh_gate: GitRefreshGate::default(),
                            git_watch_service: None,
                            git_error: None,
                            git_diff: None,
                            git_branch_comparison: None,
                            git_branch_compare_loading: false,
                            git_graph_compare_center: false,
                            git_graph_compare_selected: None,
                            git_graph_compare_rows: Arc::new(Vec::new()),
                            git_graph_compare_hunk: None,
                            git_graph_compare_scroll: UniformListScrollHandle::new(),
                            git_diff_loading: false,
                            git_diff_request: 0,
                            git_file_diff_paging: None,
                            git_file_compare_ticket: 0,
                            git_file_compare_result: None,
                            git_file_compare_split: true,
                            git_file_compare_rows: Arc::new(Vec::new()),
                            git_file_compare_additions: 0,
                            git_file_compare_deletions: 0,
                            git_file_compare_clipped: false,
                            center_git_diff_source: None,
                            center_git_diff_scroll: UniformListScrollHandle::new(),
                            center_git_diff_hunk: None,
                            center_git_diff_rows: Arc::new(Vec::new()),
                            workspace_generation: 0,
                            editor_epoch: 0,
                            mixed_tabs: WorkbenchTabs::for_workspace(0),
                            host_pin_snapshots: HashMap::new(),
                            host_pins_loading: false,
                            host_pins_busy: false,
                            host_pins_request: 0,
                            host_pins_error: None,
                            mixed_panes: WorkbenchPaneLayout::default(),
                            mixed_layout_restore: None,
                            mixed_layout_read_only: false,
                            mixed_tab_context: None,
                            runtime_bridge,
                            runtime_status,
                            runtime_loading: false,
                            quota_snapshots: Vec::new(),
                            quota_loading: false,
                            quota_error: None,
                            quota_last_refresh: Instant::now()
                                .checked_sub(Duration::from_secs(
                                    alera_gpui_desktop::agent_quota::REFRESH_INTERVAL_SECS,
                                ))
                                .unwrap_or_else(Instant::now),
                            quota_request: 0,
                            quota_enabled_order: None,
                            // Test-only: show a *live Host* overview on launch
                            // when the Windows remote mouse cannot reach the
                            // small status-bar target. No effect in release.
                            quota_popover_open: cfg!(all(
                                feature = "quota-visual-fixture",
                                debug_assertions
                            )) && std::env::var(
                                "ALERA_GPUI_QUOTA_LIVE_PREVIEW_OPEN",
                            )
                            .as_deref()
                                == Ok("1"),
                            quota_popover_account: None,
                            quota_usage_open: false,
                            quota_usage_days: 7,
                            quota_usage_loading: false,
                            quota_usage_error: None,
                            quota_usage_summary: None,
                            quota_usage_request: 0,
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
                            native_scrollback_busy: false,
                            native_scrollback_status: String::new(),
                            history_sync_snapshot: None,
                            history_sync_loading: false,
                            history_sync_busy: false,
                            history_sync_request: 0,
                            history_sync_status: String::new(),
                            history_sync_edit: None,
                            history_sync_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("Group (empty = independent)", cx)
                            }),
                            agy_accounts_snapshot: None,
                            agy_account_edit: None,
                            agy_account_remove_confirm: None,
                            agy_account_id_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder(
                                    "Local account ID (not an email)",
                                    cx,
                                )
                            }),
                            agy_account_name_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("Google account label", cx)
                            }),
                            settings_section: None,
                            settings_modal_focus: cx.focus_handle(),
                            settings_kit_dialog_open: false,
                            settings_menu_locale: UiLocale::from_preference("system"),
                            settings_menu_language: "system".into(),
                            settings_extra_values: serde_json::Map::new(),
                            settings_shared_busy: false,
                            settings_native_profile_migrated: false,
                            settings_shared_request: 0,
                            settings_shared_last_refresh: Instant::now()
                                .checked_sub(Duration::from_secs(10))
                                .unwrap_or_else(Instant::now),
                            settings_shared_error: None,
                            settings_menu_search,
                            settings_shortcut_search,
                            settings_exclusion_input,
                            last_auto_git_fetch: Instant::now(),
                            agent_profiles: Vec::new(),
                            installed_agent_clis: Vec::new(),
                            installed_agent_clis_loading: false,
                            installed_agent_clis_request: 0,
                            agent_profiles_loading: false,
                            agent_profiles_request: 0,
                            agent_selected_profile: None,
                            agent_profile_name_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("New Profile name / Rename selected Profile…", cx)
                            }),
                            agent_profile_description_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("Description (empty clears)", cx)
                            }),
                            agent_profile_quota_group_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("Quota Group (empty clears)", cx)
                            }),
                            agent_profile_routing_loaded: None,
                            agent_profile_content_loaded: None,
                            agent_profile_managed_loaded: None,
                            agent_profile_model_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("Exact Model ID (empty = default)…", cx)
                            }),
                            agent_profile_persona_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("Exact Persona / Agent name (empty = default)…", cx)
                            }),
                            agent_profile_discovered_models: Vec::new(),
                            agent_profile_discovered_personas: Vec::new(),
                            agent_profile_discovery_busy: false,
                            agent_profile_discovery_request: 0,
                            agent_profile_discovery_status: String::new(),
                            agent_profile_command_input: cx.new(|cx| {
                                QuickOpenInput::profile_command(cx)
                            }),
                            agent_profile_custom_prompt_input: cx.new(|cx| {
                                QuickOpenInput::profile_multiline(cx)
                            }),
                            agent_profile_new_adapter: "codex".into(),
                            agent_profile_mutation_busy: false,
                            agent_profile_mutation_request: 0,
                            agent_profile_mutation_status: "Create a standard command profile, or rename the selected Host profile".into(),
                            agent_profile_removal_preview: None,
                            agent_prompt_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder("Task for selected Agent…", cx)
                            }),
                            agent_override_selected_id: Some("codex".into()),
                            agent_override_input: cx.new(|cx| {
                                QuickOpenInput::with_placeholder(
                                    "Absolute Agent executable path…",
                                    cx,
                                )
                            }),
                            agent_override_busy: false,
                            agent_override_request: 0,
                            agent_override_status: String::new(),
                            agent_launch_busy: false,
                            agent_launch_request: 0,
                            agent_pending_tab: None,
                            agent_launch_pane: None,
                            agent_status: "Select an Agent Profile and enter a task".into(),
                            terminals: {
                                let mut dock = TerminalDock::for_workspace(0);
                                dock.set_scrollback_lines(initial_native_scrollback_lines);
                                dock
                            },
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
                            terminal_create_journal: None,
                            terminal_create_inventory_verified: false,
                            terminal_create_pane: None,
                            terminal_layout_restore: None,
                            terminal_list_loading: false,
                            agent_presence_event_generation: 0,
                            agent_notification_delivery: AgentNotificationDelivery::new(),
                            agent_notification_clock: Instant::now(),
                            agent_hook_switches: None,
                            agent_hook_write_loading: false,
                            agent_hook_ui_status: String::new(),
                            agent_hook_switches_loading: false,
                            agent_hook_switches_request: 0,
                            agent_hook_switches_last_refresh: Instant::now()
                                .checked_sub(Duration::from_secs(30))
                                .unwrap_or_else(Instant::now),
                            #[cfg(windows)]
                            agent_windows_toast: WindowsAgentToastPresenter::new(),
                            agent_toast_activation: None,
                            agent_toast_activation_retries: 0,
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
                        workbench.restore_host_create_journal();
                        workbench.refresh_shared_desktop_settings(cx);
                        workbench.refresh_agent_quotas(false, None, cx);
                        workbench
                    });
                    view.update(cx, |view, cx| {
                        // Each GPUI input surface shares one Windows HWND; a
                        // logical focus switch does not emit WM_KILLFOCUS.
                        for input in [
                            view.terminal_input.clone(),
                            view.secondary_terminal_input.clone(),
                        ] {
                            let focus = input.read(cx).focus_handle(cx);
                            cx.on_blur(&focus, window, |view, _, cx| {
                                let result = win_ime::cancel_focused_composition();
                                if std::env::var_os("ALERA_GPUI_TSF_TRACE").is_some() {
                                    eprintln!("Alera GPUI TSF: blur cancel={result:?}");
                                }
                                view.clear_terminal_text_inputs(cx);
                            })
                            .detach();
                        }
                        // Explorer shares the same Windows HWND as both Terminal
                        // text sinks, so switching focus does not generate
                        // WM_KILLFOCUS. Cancel its OS composition on GPUI blur
                        // and remove only unfinished search preedit text.
                        let explorer_focus = view.explorer_filter_input.read(cx).focus_handle(cx);
                        cx.on_blur(&explorer_focus, window, |view, _, cx| {
                            let result = win_ime::cancel_focused_composition();
                            view.explorer_filter_input.update(cx, |input, cx| {
                                input.discard_marked_preedit(cx);
                            });
                            if std::env::var_os("ALERA_GPUI_TSF_TRACE").is_some() {
                                eprintln!("Alera GPUI TSF: explorer blur cancel={result:?}");
                            }
                        })
                        .detach();
                        view.start_terminal_polling(cx);
                        // Restore saved Host tab previews immediately after
                        // Native window startup, rather than waiting for the
                        // first 1.5s housekeeping session-list tick.
                        // Uses authenticated CLI --no-start; no PTY is minted.
                        view.refresh_terminals(cx);
                        view.refresh_project_tree(cx);
                        view.refresh_worktrees(cx);
                        view.start_git_watch(cx);
                        view.refresh_git(cx);
                        view.refresh_shared_git_favorites(cx);
                        view.refresh_agent_profiles(cx);
                        view.refresh_installed_agent_clis(cx);
                        view.connect_mcp_ui_bridge(cx);
                    });
                    view
                },
            )
            .expect("create native GPUI desktop window");
            cx.activate(true);
        });
}
