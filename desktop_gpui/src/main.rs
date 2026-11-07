//! Experimental Alera desktop shell powered by native GPUI (no Flutter/WebView).
mod editor_ui;
mod quick_open_ui;

use alera_gpui_desktop::editor::EditorDraftStore;
use alera_gpui_desktop::editor_tabs::EditorTabs;
use alera_gpui_desktop::explorer_ops::ExplorerFiles;
use alera_gpui_desktop::file_search::WorkspaceFileIndex;
use alera_gpui_desktop::git_inspect::{read_file_diff, GitSnapshot};
use alera_gpui_desktop::model::WorkbenchModel;
use alera_gpui_desktop::project_search::{ContentMatch, ProjectSearch};
use alera_gpui_desktop::runtime_bridge::RuntimeCliBridge;
use alera_gpui_desktop::session::PreviewSessionStore;
use alera_gpui_desktop::terminal::{
    TerminalDock, TerminalPollGate, TerminalViewMode, HOST_SESSION_LIST_SCOPE,
};
use alera_gpui_desktop::workspace_tree::WorkspaceTree;
use alera_gpui_desktop::worktree_manager::{WorktreeEntry, WorktreeManager};
use editor_ui::{bind_editor_keys, NativeEditor};
use gpui::{
    actions, div, prelude::*, px, rgb, size, App, Application, Bounds, Context, Entity, Focusable,
    KeyBinding, PathPromptOptions, Window, WindowBounds, WindowOptions,
};
use quick_open_ui::{bind_quick_open_keys, QuickOpenInput};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const BG: u32 = 0x15171b;
const PANEL: u32 = 0x1c1f24;
const TOOLBAR: u32 = 0x252931;
const BORDER: u32 = 0x343942;
const TEXT: u32 = 0xe1e4e8;
const MUTED: u32 = 0xa6adb7;
const ACTIVE: u32 = 0x303741;

actions!(
    alera_gpui_workbench,
    [ToggleFileSearch, SubmitTerminalCommand]
);

/// Deliberately small desktop shell: terminal first, basic editor on demand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum CenterView {
    #[default]
    Terminal,
    Editor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExplorerAction {
    NewFile,
    NewFolder,
    Rename,
    Move,
    Delete,
}

impl ExplorerAction {
    fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New file",
            Self::NewFolder => "New folder",
            Self::Rename => "Rename",
            Self::Move => "Move into folder",
            Self::Delete => "Delete",
        }
    }
}

struct Workbench {
    model: WorkbenchModel,
    center_view: CenterView,
    tree: Option<WorkspaceTree>,
    tree_mode: bool,
    explorer_selection: Option<PathBuf>,
    explorer_action: Option<ExplorerAction>,
    explorer_action_input: Option<Entity<QuickOpenInput>>,
    explorer_delete_confirmed: bool,
    editor: Option<Entity<NativeEditor>>,
    session_store: Option<PreviewSessionStore>,
    recent_workspaces: Vec<PathBuf>,
    saved_projects: Vec<PathBuf>,
    show_recent_workspaces: bool,
    worktrees: Vec<WorktreeEntry>,
    worktree_error: Option<String>,
    worktree_loading: bool,
    worktree_busy: bool,
    worktree_create_input: Option<Entity<QuickOpenInput>>,
    worktree_delete_pending: Option<PathBuf>,
    picking_workspace: bool,
    picking_save_as: bool,
    quick_open: Option<Entity<QuickOpenInput>>,
    file_index: Option<Arc<WorkspaceFileIndex>>,
    file_index_loading: bool,
    content_query: Option<Entity<QuickOpenInput>>,
    content_loading: bool,
    content_matches: Vec<ContentMatch>,
    content_truncated: bool,
    content_error: Option<String>,
    content_searched_query: String,
    content_request: u64,
    git_snapshot: Option<GitSnapshot>,
    git_loading: bool,
    git_error: Option<String>,
    git_diff: Option<String>,
    git_diff_loading: bool,
    git_diff_request: u64,
    workspace_generation: u64,
    runtime_bridge: Option<RuntimeCliBridge>,
    runtime_status: String,
    runtime_loading: bool,
    terminals: TerminalDock,
    terminal_input: Entity<QuickOpenInput>,
    terminal_send_loading: bool,
    terminal_list_loading: bool,
    terminal_read_loading: Option<String>,
    terminal_poll: TerminalPollGate,
    terminal_tick: u64,
}

impl Workbench {
    fn begin_explorer_action(
        &mut self,
        action: ExplorerAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            action,
            ExplorerAction::Rename | ExplorerAction::Move | ExplorerAction::Delete
        ) && self.explorer_selection.is_none()
        {
            self.model
                .set_status("Select a file or folder in Explorer first");
            cx.notify();
            return;
        }
        self.explorer_action = Some(action);
        self.explorer_delete_confirmed = false;
        self.explorer_action_input = if action == ExplorerAction::Delete {
            None
        } else {
            let placeholder = match action {
                ExplorerAction::NewFile => "New file name",
                ExplorerAction::NewFolder => "New folder name",
                ExplorerAction::Rename => "New name",
                ExplorerAction::Move => "Destination folder inside workspace",
                ExplorerAction::Delete => unreachable!(),
            };
            let input = cx.new(|cx| QuickOpenInput::with_placeholder(placeholder, cx));
            window.focus(&input.read(cx).focus_handle(cx));
            Some(input)
        };
        cx.notify();
    }

    fn cancel_explorer_action(&mut self, cx: &mut Context<Self>) {
        self.explorer_action = None;
        self.explorer_action_input = None;
        self.explorer_delete_confirmed = false;
        cx.notify();
    }

    fn apply_explorer_action(&mut self, cx: &mut Context<Self>) {
        let Some(action) = self.explorer_action else {
            return;
        };
        let selected = self.explorer_selection.clone();
        let root = self.model.root().to_path_buf();
        let input = self
            .explorer_action_input
            .as_ref()
            .map(|input| input.read(cx).query().trim().to_owned())
            .unwrap_or_default();

        if matches!(
            action,
            ExplorerAction::Rename | ExplorerAction::Move | ExplorerAction::Delete
        ) && self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).switch_allowed().is_err())
        {
            self.model.set_status(
                "Save or discard editor changes before renaming, moving or deleting Explorer paths",
            );
            cx.notify();
            return;
        }
        if action == ExplorerAction::Delete && !self.explorer_delete_confirmed {
            self.explorer_delete_confirmed = true;
            self.model.set_status(
                "Confirm permanent deletion. Nonempty folders and their contents will be removed.",
            );
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
            ExplorerAction::Delete => files.delete(&source, true).map(|_| None),
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

    // Clear the parent editor after closing its last inline tab.
    fn install_editor(&mut self, tabs: EditorTabs, cx: &mut Context<Self>) {
        let editor = cx.new(|cx| NativeEditor::new(tabs, cx));
        cx.observe(&editor, |workbench, _, cx| {
            if workbench
                .editor
                .as_ref()
                .is_some_and(|current| current.read(cx).tabs_empty())
            {
                workbench.editor = None;
                workbench.model.set_status("Native editor closed");
                cx.notify();
            }
        })
        .detach();
        self.editor = Some(editor);
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

    fn toggle_file_search_action(
        &mut self,
        _: &ToggleFileSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_file_search(window, cx);
    }

    fn submit_terminal_action(
        &mut self,
        _: &SubmitTerminalCommand,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_terminal_input(true, cx);
    }
    fn toggle_file_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quick_open.is_some() {
            self.quick_open = None;
            cx.notify();
            return;
        }
        let input = cx.new(QuickOpenInput::new);
        cx.observe(&input, |_, _, cx| cx.notify()).detach();
        window.focus(&input.read(cx).focus_handle(cx));
        self.quick_open = Some(input);
        if self
            .file_index
            .as_ref()
            .is_none_or(|index| index.root() != self.model.root())
            && !self.file_index_loading
        {
            let root = self.model.root().to_path_buf();
            let generation = self.workspace_generation;
            self.file_index_loading = true;
            cx.spawn(async move |view, cx| {
                let result = cx
                    .background_spawn(async move { WorkspaceFileIndex::scan(&root) })
                    .await;
                view.update(cx, |view, cx| {
                    if view.workspace_generation != generation {
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
        cx.notify();
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
            window.focus(&input.read(cx).focus_handle(cx));
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

    fn editor_dirty(&self, cx: &mut Context<Self>) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).has_unsaved_changes())
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
        let root = self.model.root().to_path_buf();
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
            cx.notify();
            return;
        }
        let result = self
            .editor
            .as_ref()
            .unwrap()
            .update(cx, |editor, cx| editor.open_in_place(&root, &path, cx));
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
        cx.notify();
    }

    fn save_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let path = editor.read(cx).document().path().map(PathBuf::from);
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

    fn new_editor(&mut self, cx: &mut Context<Self>) {
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
        let result = self
            .editor
            .as_ref()
            .unwrap()
            .update(cx, |editor, cx| editor.open_scratch(&root, cx));
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
        if self.editor.is_none() || self.picking_save_as {
            return;
        }
        self.picking_save_as = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Save As".into()),
        });
        cx.spawn(async move |view, cx| {
            let result = selection.await;
            view.update(cx, |view, cx| {
                view.picking_save_as = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            view.save_editor_as(path, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => view.model.set_status(format!("Save As failed: {error}")),
                    Err(error) => view.model.set_status(format!("Save As canceled: {error}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn save_editor_as(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let saved = editor.update(cx, |editor, cx| editor.save_as(&path, false, cx));
        if saved && self.model.selected_path() == Some(path.as_path()) {
            self.model.reload_active();
        }
        let status = editor.read(cx).status_text().to_owned();
        self.model.set_status(status);
        cx.notify();
    }

    fn close_editor(&mut self, discard: bool, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let path = editor.read(cx).document().path().map(PathBuf::from);
        let effect = editor.update(cx, |editor, _| editor.close_editor(discard));
        if effect.closed() {
            if self.model.selected_path() == path.as_deref() {
                self.model.reload_active();
            }
            if editor.read(cx).tabs_empty() {
                self.editor = None;
            }
        }
        self.model.set_status(effect.status());
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
        if self.worktree_busy || self.editor_dirty(cx) {
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
        if self.worktree_busy {
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
        if self.worktree_busy {
            self.model
                .set_status("Finish the current Worktree operation before switching projects");
            cx.notify();
            return;
        }
        if let Some(editor) = &self.editor {
            if let Err(status) = editor.read(cx).switch_allowed() {
                self.model.set_status(status);
                cx.notify();
                return;
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
        self.model = next;
        self.center_view = CenterView::Terminal;
        self.tree = WorkspaceTree::new(&canonical).ok();
        self.explorer_selection = None;
        self.cancel_explorer_action(cx);
        self.editor = None;
        self.restore_scratch_tabs(cx);
        self.quick_open = None;
        self.file_index = None;
        self.file_index_loading = false;
        self.content_query = None;
        self.content_loading = false;
        self.content_matches.clear();
        self.content_error = None;
        self.content_truncated = false;
        self.content_searched_query.clear();
        self.content_request = self.content_request.wrapping_add(1);
        self.git_snapshot = None;
        self.git_loading = false;
        self.git_diff = None;
        self.git_diff_loading = false;
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        self.workspace_generation = self.workspace_generation.wrapping_add(1);
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
        self.terminal_send_loading = false;
        self.terminal_input.update(cx, |input, cx| input.clear(cx));
        self.terminal_poll = TerminalPollGate::default();
        self.terminal_list_loading = false;
        self.terminal_read_loading = None;
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

    fn refresh_terminals(&mut self, cx: &mut Context<Self>) {
        if self.terminal_list_loading {
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.update_sessions(Vec::new());
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
                        let was_disconnected = view.terminals.sessions.is_empty();
                        if view.terminals.apply_session_list(generation, sessions)
                            && was_disconnected
                            && !view.terminals.sessions.is_empty()
                        {
                            view.terminal_poll.reset();
                        }
                    }
                    Err(error) => {
                        view.terminals.apply_session_list(generation, Vec::new());
                        view.terminals.status = format!("Session refresh failed: {error}");
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
            self.center_view = CenterView::Terminal;
            self.terminal_send_loading = false;
            self.terminal_input.update(cx, |input, cx| input.clear(cx));
            self.refresh_terminal_output(cx);
        } else {
            cx.notify();
        }
    }

    fn send_terminal_input(&mut self, enter: bool, cx: &mut Context<Self>) {
        if self.terminal_send_loading {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.status = "Host CLI is not available for terminal input".into();
            cx.notify();
            return;
        };
        let Some(tab) = self.terminals.active() else {
            self.terminals.status = "Select a Host terminal session first".into();
            cx.notify();
            return;
        };
        if !tab.running {
            self.terminals.status = "Stopped Host terminals cannot accept input".into();
            cx.notify();
            return;
        }
        if self.terminal_input.read(cx).composing() {
            self.terminals.status = "Finish IME composition before sending terminal input".into();
            cx.notify();
            return;
        }
        let text = self.terminal_input.read(cx).query().to_owned();
        if text.trim().is_empty() {
            self.terminals.status = "Type terminal input before sending".into();
            cx.notify();
            return;
        }
        let handle = tab.handle.clone();
        let tab_id = tab.tab_id.clone();
        let generation = self.workspace_generation;
        self.terminal_send_loading = true;
        self.terminals.status = "Sending to existing Host terminal…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let sent_handle = handle.clone();
            let sent_text = text.clone();
            let result = cx
                .background_spawn(async move { bridge.write_terminal(&handle, &text, enter) })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.terminal_send_loading = false;
                if !view
                    .terminals
                    .active()
                    .is_some_and(|active| active.handle == sent_handle && active.tab_id == tab_id)
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(()) => {
                        view.terminal_input.update(cx, |input, cx| {
                            if input.query() == sent_text {
                                input.clear(cx);
                            }
                        });
                        view.terminals.status = if enter {
                            "Command submitted to Runtime Host".into()
                        } else {
                            "Input sent to Runtime Host without Enter".into()
                        };
                        view.refresh_terminal_output(cx);
                    }
                    Err(error) => {
                        view.terminals.status = format!("Host terminal rejected input: {error}");
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
        let generation = self.workspace_generation;
        self.terminal_read_loading = Some(handle.clone());
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.read_terminal(&handle, cursor) })
                .await;
            view.update(cx, |view, cx| {
                if view.terminals.workspace_generation() != generation {
                    return;
                }
                view.terminal_read_loading = None;
                match result {
                    Ok(chunk) => {
                        if view.terminals.active_ticket().as_ref() == Some(&ticket) {
                            // Successful reads with no new output still
                            // reset previous failure backoff.
                            view.terminal_poll.reset();
                        }
                        view.terminals
                            .apply_workspace_read(generation, &ticket, chunk);
                    }
                    Err(error) => {
                        if view.terminals.active_ticket().as_ref() == Some(&ticket) {
                            view.terminal_poll.failed();
                            view.terminals.status = format!("Terminal read failed: {error}");
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn start_terminal_polling(&mut self, cx: &mut Context<Self>) {
        // A single lifecycle-bound timer: no timer per tab, no stale callback
        // after the Workbench entity/window has been destroyed.
        cx.spawn(async move |view, cx| loop {
            gpui::Timer::after(Duration::from_millis(1500)).await;
            if view
                .update(cx, |view, cx| {
                    view.terminal_tick = view.terminal_tick.wrapping_add(1);
                    if view.terminal_tick % 10 == 1 {
                        view.refresh_terminals(cx);
                    }
                    if view.terminal_poll.tick()
                        && view.terminal_read_loading.is_none()
                        && view.terminals.active_ticket().is_some()
                    {
                        view.refresh_terminal_output(cx);
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let tree_mode = self.tree_mode && self.tree.is_some();
        let tree_truncated = self.tree.as_ref().is_some_and(WorkspaceTree::truncated);
        let explorer_selection = self.explorer_selection.clone();
        let explorer_action = self.explorer_action;
        let explorer_action_input = self.explorer_action_input.clone();
        let explorer_delete_confirmed = self.explorer_delete_confirmed;
        let tabs: Vec<_> = self
            .model
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| (index, tab.title.clone()))
            .collect();
        let active_tab = self.model.active_tab();
        let center_is_editor = self.center_view == CenterView::Editor;
        let editor = self.editor.clone();
        let editor_dirty = self.editor_dirty(cx);
        let discard_pending = self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).discard_armed());
        let editor_title = self
            .editor
            .as_ref()
            .and_then(|editor| {
                editor
                    .read(cx)
                    .document()
                    .path()
                    .map(|path| path.display().to_string())
            })
            .unwrap_or_else(|| filename.clone());
        let status = self.model.status().to_owned();
        let branch = self.model.branch().to_owned();
        let can_up = self.model.can_go_up();
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
        let terminal_list_loading = self.terminal_list_loading;
        let terminal_read_loading = self.terminal_read_loading.is_some();
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
        let terminal_tabs: Vec<_> = self
            .terminals
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| (i, tab.title.clone(), tab.running))
            .collect();
        let terminal_active = self.terminals.active_tab;
        let terminal_output = self
            .terminals
            .active()
            .map(|tab| match self.terminals.view_mode {
                TerminalViewMode::Screen => tab.screen.contents(),
                TerminalViewMode::Transcript => tab.output.clone(),
            })
            .unwrap_or_default();
        let terminal_status = self.terminals.status.clone();
        let has_terminal_preview = self.terminals.active().is_some();
        let terminal_input = self.terminal_input.clone();
        let terminal_send_loading = self.terminal_send_loading;
        let terminal_auto = self.terminal_poll.enabled();
        let terminal_is_vt = self.terminals.view_mode == TerminalViewMode::Screen;
        let terminal_screen_rows = if terminal_is_vt {
            self.terminals
                .active()
                .map(|tab| tab.screen.styled_rows())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let terminal_status = if terminal_is_vt {
            if let Some(tab) = self.terminals.active() {
                let (rows, cols) = tab.screen.dimensions();
                let (cursor_row, cursor_col) = tab.screen.cursor();
                format!(
                    "{terminal_status} · {cols}×{rows} · Cursor {}:{}{}",
                    cursor_row + 1,
                    cursor_col + 1,
                    if tab.screen.alternate_screen() {
                        " · Alt"
                    } else {
                        ""
                    }
                )
            } else {
                terminal_status
            }
        } else {
            terminal_status
        };
        let quick_open = self.quick_open.clone();
        let indexed_total = self.file_index.as_ref().map(|index| index.total());
        let index_loading = self.file_index_loading;
        let quick_results = self
            .quick_open
            .as_ref()
            .zip(self.file_index.as_ref())
            .map(|(input, index)| index.search(input.read(cx).query(), 18))
            .unwrap_or_default();
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
        let git_changes = self
            .git_snapshot
            .as_ref()
            .map(|snapshot| snapshot.changed_files.clone())
            .unwrap_or_default();
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
            .on_action(cx.listener(Self::toggle_file_search_action))
            .on_action(cx.listener(Self::submit_terminal_action))
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .h(px(46.0))
                    .px_4()
                    .bg(rgb(TOOLBAR))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(gpui::FontWeight::BOLD).child("ALERA"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("GPUI · Native Desktop Preview"),
                    )
                    .child(div().text_sm().child(branch.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row_reverse()
                    .child(
                        div()
                            .w(px(315.0))
                            .flex()
                            .flex_col()
                            .bg(rgb(PANEL))
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .px_3()
                                    .py_3()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().text_sm().child(format!("Explorer · {folder_name}")))
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("explorer-tree-toggle")
                                                    .cursor_pointer()
                                                    .text_color(rgb(if tree_mode { TEXT } else { MUTED }))
                                                    .child(if tree_mode { "Tree ✓" } else { "Tree" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.tree_mode = !this.tree_mode;
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("up")
                                                    .cursor_pointer()
                                                    .text_color(rgb(MUTED))
                                                    .child(if can_up { "↑ Up" } else { "Root" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.model.go_up();
                                                        this.persist_session();
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("explorer-refresh")
                                                    .cursor_pointer()
                                                    .text_color(rgb(MUTED))
                                                    .child("↻")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.model.refresh_directory();
                                                        if let Some(tree) = &mut this.tree {
                                                            if let Err(error) = tree.refresh() {
                                                                this.model.set_status(format!("Tree refresh failed: {error}"));
                                                            }
                                                        }
                                                        cx.notify();
                                                    })),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("explorer-file-actions")
                                    .px_2()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .gap_2()
                                            .children([
                                                ("+ File", ExplorerAction::NewFile),
                                                ("+ Folder", ExplorerAction::NewFolder),
                                                ("Rename", ExplorerAction::Rename),
                                                ("Move", ExplorerAction::Move),
                                                ("Delete", ExplorerAction::Delete),
                                            ].into_iter().enumerate().map(|(i, (label, action))| {
                                                div()
                                                    .id(("explorer-file-action", i))
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(rgb(if action == ExplorerAction::Delete { MUTED } else { TEXT }))
                                                    .child(label)
                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                        this.begin_explorer_action(action, window, cx);
                                                    }))
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(explorer_selection.as_ref().map(|path| {
                                                format!("Selected: {}", path.strip_prefix(&workspace_root).unwrap_or(path).display())
                                            }).unwrap_or_else(|| "Select a file/folder to rename, move or delete".into())),
                                    )
                                    .child(if let Some(action) = explorer_action {
                                        div()
                                            .id("explorer-action-editor")
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().text_xs().text_color(rgb(TEXT)).child(
                                                if action == ExplorerAction::Delete {
                                                    "Delete selection permanently? Folder contents will be deleted.".into()
                                                } else {
                                                    action.label().to_string()
                                                }
                                            ))
                                            .child(if let Some(input) = explorer_action_input {
                                                div().child(input).into_any_element()
                                            } else {
                                                div().h(px(0.0)).into_any_element()
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap_3()
                                                    .child(
                                                        div()
                                                            .id("explorer-apply")
                                                            .cursor_pointer()
                                                            .text_sm()
                                                            .text_color(rgb(TEXT))
                                                            .child(if explorer_delete_confirmed {
                                                                "CONFIRM DELETE"
                                                            } else if action == ExplorerAction::Delete {
                                                                "Delete…"
                                                            } else {
                                                                "Apply"
                                                            })
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.apply_explorer_action(cx);
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("explorer-action-cancel")
                                                            .cursor_pointer()
                                                            .text_sm()
                                                            .text_color(rgb(MUTED))
                                                            .child("Cancel")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.cancel_explorer_action(cx);
                                                            })),
                                                    ),
                                            )
                                            .into_any_element()
                                    } else {
                                        div().h(px(0.0)).into_any_element()
                                    }),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        div()
                                            .id("quick-open-toggle")
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(rgb(TEXT))
                                            .child(if quick_open.is_some() {
                                                "⌕ Quick Open  ×"
                                            } else {
                                                "⌕ Quick Open  Ctrl+P"
                                            })
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.toggle_file_search(window, cx);
                                            })),
                                    )
                                    .child(if let Some(input) = quick_open {
                                        div()
                                            .mt_2()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(input)
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child(if index_loading {
                                                        "Indexing workspace files…".into()
                                                    } else {
                                                        format!(
                                                            "{} files indexed · {} matches",
                                                            indexed_total.unwrap_or(0),
                                                            quick_results.len()
                                                        )
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .id("quick-open-results")
                                                    .max_h(px(260.0))
                                                    .overflow_y_scroll()
                                                    .children(quick_results.into_iter().enumerate().map(
                                                        |(index, path)| {
                                                            let label = path
                                                                .strip_prefix(&workspace_root)
                                                                .unwrap_or(&path)
                                                                .display()
                                                                .to_string();
                                                            div()
                                                                .id(("quick-open-result", index))
                                                                .cursor_pointer()
                                                                .text_sm()
                                                                .py_1()
                                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                                .child(label)
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    if this.model.open_path(&path) {
                                                                        this.open_editor(cx);
                                                                    }
                                                                    this.quick_open = None;
                                                                    this.persist_session();
                                                                    cx.notify();
                                                                }))
                                                        }
                                                    )),
                                            )
                                    } else {
                                        div().h(px(0.0))
                                    }),
                            )
                            .child(
                                div()
                                    .id("content-search-panel")
                                    .px_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .id("content-search-toggle")
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(TEXT))
                                                    .child(if content_query.is_some() {
                                                        "⌕ Search Contents  ×"
                                                    } else {
                                                        "⌕ Search Contents"
                                                    })
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.toggle_content_search(window, cx);
                                                    })),
                                            )
                                            .child(if content_query.is_some() {
                                                div()
                                                    .id("content-search-run")
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(TEXT))
                                                    .child(if content_loading { "Search again" } else { "Search" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.search_project_contents(cx);
                                                    }))
                                                    .into_any_element()
                                            } else {
                                                div().h(px(0.0)).into_any_element()
                                            }),
                                    )
                                    .child(if let Some(input) = content_query {
                                        div()
                                            .mt_2()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(input)
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
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                if this.model.open_path(&path) {
                                                                    this.open_editor(cx);
                                                                    this.persist_session();
                                                                }
                                                                cx.notify();
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
                                    .overflow_y_scroll()
                                    .children(if tree_mode {
                                        tree_rows.into_iter().enumerate().map(|(i, item)| {
                                            let relative = item.relative_path.clone();
                                            let row_path = workspace_root.join(&relative);
                                            let selected = explorer_selection.as_ref() == Some(&row_path);
                                            let is_directory = item.is_directory;
                                            let prefix = if !is_directory { "·" } else if item.expanded { "▾" } else { "▸" };
                                            div()
                                                .id(("explorer-tree-row", i))
                                                .pl(px(12.0 + item.depth as f32 * 14.0))
                                                .pr_2()
                                                .py_1()
                                                .cursor_pointer()
                                                .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(format!("{prefix} {}", item.name))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.explorer_selection = Some(this.model.root().join(&relative));
                                                    this.explorer_delete_confirmed = false;
                                                    if is_directory {
                                                        if let Some(tree) = &mut this.tree {
                                                            if let Err(error) = tree.toggle(&relative) {
                                                                this.model.set_status(format!("Cannot expand folder: {error}"));
                                                            }
                                                        }
                                                    } else {
                                                        let path = this.model.root().join(&relative);
                                                        if this.model.open_path(&path) {
                                                            this.open_editor(cx);
                                                        }
                                                        this.persist_session();
                                                    }
                                                    cx.notify();
                                                }))
                                        }).collect::<Vec<_>>()
                                    } else {
                                        entries.into_iter().enumerate().map(|(i, item)| {
                                            let is_directory = item.is_directory;
                                            let file_path = item.path.clone();
                                            let selected = explorer_selection.as_ref() == Some(&file_path);
                                            div()
                                                .id(i)
                                                .px_3()
                                                .py_1()
                                                .cursor_pointer()
                                                .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                                .hover(|style| style.bg(rgb(ACTIVE)))
                                                .child(format!(
                                                    "{} {}",
                                                    if item.is_directory { "▸" } else { "·" },
                                                    item.name
                                                ))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.explorer_selection = Some(file_path.clone());
                                                    this.explorer_delete_confirmed = false;
                                                    this.model.open_entry(i);
                                                    if !is_directory && this.model.selected_path() == Some(file_path.as_path()) {
                                                        this.open_editor(cx);
                                                    }
                                                    this.persist_session();
                                                    cx.notify();
                                                }))
                                        }).collect::<Vec<_>>()
                                    })
                                    .when(tree_mode && tree_truncated, |view| {
                                        view.child(div().text_color(rgb(MUTED)).child("Tree limited to 4,000 visible items"))
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child("Git changes · read-only"),
                                    )
                                    .child(
                                        div()
                                            .id("git-refresh")
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(rgb(TEXT))
                                            .child("↻")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.refresh_git(cx);
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(git_summary),
                            )
                            .child(
                                div()
                                    .id("git-change-list")
                                    .max_h(px(135.0))
                                    .overflow_y_scroll()
                                    .children(git_changes.into_iter().enumerate().map(|(i, change)| {
                                        let label = format!("{} {}", change.label, change.relative.display());
                                        let relative = change.relative;
                                        div()
                                            .id(("git-change-file", i))
                                            .cursor_pointer()
                                            .py_1()
                                            .text_xs()
                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                            .child(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.preview_git_change(relative.clone(), cx);
                                            }))
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .id("center-mode-switch")
                                    .h(px(38.0))
                                    .bg(rgb(TOOLBAR))
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .px_3()
                                    .child(
                                        div()
                                            .id("center-terminal-tab")
                                            .cursor_pointer()
                                            .text_color(rgb(if center_is_editor { MUTED } else { TEXT }))
                                            .child(if center_is_editor { "Terminal" } else { "● Terminal" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.center_view = CenterView::Terminal;
                                                this.refresh_terminals(cx);
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("center-editor-tab")
                                            .cursor_pointer()
                                            .text_color(rgb(if center_is_editor { TEXT } else { MUTED }))
                                            .child(if center_is_editor { "● Editor" } else { "Editor" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.center_view = CenterView::Editor;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(38.0))
                                    .when(!center_is_editor, |panel| panel.hidden())
                                    .flex()
                                    .items_center()
                                    .bg(rgb(PANEL))
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        div()
                                            .id("file-tabs")
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .overflow_x_scroll()
                                            .children(tabs.into_iter().map(|(index, title)| {
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .w(px(165.0))
                                                    .h(px(37.0))
                                                    .bg(rgb(if active_tab == Some(index) {
                                                        ACTIVE
                                                    } else {
                                                        PANEL
                                                    }))
                                                    .border_r_1()
                                                    .border_color(rgb(BORDER))
                                                    .child(
                                                        div()
                                                            .id(("file-tab", index))
                                                            .flex_1()
                                                            .min_w_0()
                                                            .px_2()
                                                            .cursor_pointer()
                                                            .text_sm()
                                                            .text_color(rgb(
                                                                if active_tab == Some(index) {
                                                                    TEXT
                                                                } else {
                                                                    MUTED
                                                                },
                                                            ))
                                                            .child(title)
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.model.activate_tab(index);
                                                                    this.persist_session();
                                                                    cx.notify();
                                                                },
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .id(("close-tab", index))
                                                            .px_2()
                                                            .cursor_pointer()
                                                            .text_color(rgb(MUTED))
                                                            .child("×")
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.model.close_tab(index);
                                                                    this.persist_session();
                                                                    cx.notify();
                                                                },
                                                            )),
                                                    )
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("reload-preview")
                                            .px_3()
                                            .cursor_pointer()
                                            .text_color(rgb(MUTED))
                                            .child("↻")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.model.reload_active();
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(29.0))
                                    .when(!center_is_editor, |panel| panel.hidden())
                                    .px_4()
                                    .bg(rgb(BG))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(if editor.is_some() {
                                        format!("Editing: {}{}", editor_title, if editor_dirty { " ●" } else { "" })
                                    } else {
                                        format!("{} · read-only preview", filename)
                                    })
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .id("native-editor-new")
                                                    .cursor_pointer()
                                                    .text_color(rgb(TEXT))
                                                    .child("New")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.new_editor(cx);
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("native-editor-edit")
                                                    .cursor_pointer()
                                                    .text_color(rgb(TEXT))
                                                    .child("Edit")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.open_editor(cx);
                                                    })),
                                            )
                                            .when(editor.is_some(), |toolbar| {
                                                toolbar
                                                    .child(
                                                        div()
                                                            .id("native-editor-save")
                                                            .cursor_pointer()
                                                            .text_color(rgb(TEXT))
                                                            .child("Save")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.save_editor(cx);
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("native-editor-save-as")
                                                            .cursor_pointer()
                                                            .text_color(rgb(TEXT))
                                                            .child("Save As")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.pick_save_as(cx);
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("native-editor-close")
                                                            .cursor_pointer()
                                                            .child("Close")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.close_editor(false, cx);
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("native-editor-discard")
                                                            .cursor_pointer()
                                                            .text_color(rgb(MUTED))
                                                            .child(if discard_pending {
                                                                "Confirm discard"
                                                            } else {
                                                                "Discard"
                                                            })
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.close_editor(true, cx);
                                                            })),
                                                    )
                                            }),
                                    ),
                            )
                            .child(
                                if let Some(editor) = editor {
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .when(!center_is_editor, |panel| panel.hidden())
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
                                    .when(center_is_editor, |panel| panel.hidden())
                                    .flex()
                                    .flex_col()
                                    .border_t_1()
                                    .border_color(rgb(BORDER))
                                    .bg(rgb(BG))
                                    .child(
                                        div()
                                            .h(px(36.0))
                                            .flex()
                                            .items_center()
                                            .bg(rgb(PANEL))
                                            .child(
                                                div()
                                                    .id("terminal-preview-tabs")
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .overflow_x_scroll()
                                                    .children(terminal_tabs.into_iter().map(
                                                        |(index, title, running)| {
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .h(px(35.0))
                                                                .bg(rgb(if terminal_active == Some(index) {
                                                                    ACTIVE
                                                                } else {
                                                                    PANEL
                                                                }))
                                                                .border_r_1()
                                                                .border_color(rgb(BORDER))
                                                                .child(
                                                                    div()
                                                                        .id(("term-tab", index))
                                                                        .px_2()
                                                                        .cursor_pointer()
                                                                        .text_sm()
                                                                        .child(format!(
                                                                            "{} {title}",
                                                                            if running { "●" } else { "○" }
                                                                        ))
                                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                                            if this.terminals.activate(index) {
                                                                                this.refresh_terminal_output(cx);
                                                                            }
                                                                            cx.notify();
                                                                        })),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .id(("term-close", index))
                                                                        .px_2()
                                                                        .cursor_pointer()
                                                                        .text_color(rgb(MUTED))
                                                                        .child("×")
                                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                                            this.terminals.close(index);
                                                                            cx.notify();
                                                                        })),
                                                                )
                                                        },
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .id("terminal-vt-mode")
                                                    .px_2()
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(TEXT))
                                                    .child(if terminal_is_vt { "VT screen" } else { "Text log" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.terminals.toggle_view_mode();
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("terminal-preview-autopoll")
                                                    .px_2()
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(if terminal_auto { TEXT } else { MUTED }))
                                                    .child(if terminal_auto { "Auto: On" } else { "Auto: Off" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.terminal_poll.toggle();
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("terminal-preview-refresh")
                                                    .px_3()
                                                    .cursor_pointer()
                                                    .text_color(rgb(MUTED))
                                                    .child(if terminal_read_loading { "Reading…" } else { "Read more ↻" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.refresh_terminal_output(cx);
                                                    })),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .h(px(22.0))
                                            .px_3()
                                            .text_sm()
                                            .text_color(rgb(MUTED))
                                            .child(terminal_status),
                                    )
                                    .child(if terminal_is_vt {
                                        div()
                                            .id("terminal-colored-grid")
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_x_scroll()
                                            .overflow_y_scroll()
                                            .px_3()
                                            .py_2()
                                            .font_family("JetBrains Mono")
                                            .text_sm()
                                            .whitespace_nowrap()
                                            .children(terminal_screen_rows.into_iter().enumerate().map(
                                                |(row_index, row)| {
                                                    div()
                                                        .id(("terminal-colored-row", row_index))
                                                        .whitespace_nowrap()
                                                        .line_height(px(17.0))
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
                                                },
                                            ))
                                    } else {
                                        div()
                                            .id("terminal-output-scroll")
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_y_scroll()
                                            .px_3()
                                            .py_2()
                                            .font_family("JetBrains Mono")
                                            .text_sm()
                                            .child(if terminal_output.is_empty() {
                                                "No terminal content yet. Auto updates are enabled by default.".into()
                                            } else {
                                                terminal_output
                                            })
                                    })
                                    .child(
                                        div()
                                            .id("terminal-command-bar")
                                            .h(px(45.0))
                                            .px_3()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .bg(rgb(PANEL))
                                            .border_t_1()
                                            .border_color(rgb(BORDER))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(terminal_input),
                                            )
                                            .child(
                                                div()
                                                    .id("terminal-send-raw")
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(MUTED))
                                                    .child("Send")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.send_terminal_input(false, cx);
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("terminal-send-enter")
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(TEXT))
                                                    .child(if terminal_send_loading { "Sending…" } else { "Run ↵" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.send_terminal_input(true, cx);
                                                    })),
                                            ),
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
                                    .child("No Host terminal selected. Choose an existing session in Projects on the left.")
                                    .into_any_element()
                            }),
                    )
                    .child(
                        div()
                            .w(px(248.0))
                            .flex()
                            .flex_col()
                            .bg(rgb(PANEL))
                            .border_r_1()
                            .border_color(rgb(BORDER))
                            .p_3()
                            .gap_3()
                            .child(
                                div()
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
                                        div()
                                            .id("projects-open-folder")
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(rgb(TEXT))
                                            .child(if picking_workspace { "Opening…" } else { "+ Open" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.pick_workspace(cx);
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(format!("Active: {}", workspace_root.display())),
                            )
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
                                        div()
                                            .id("project-pin-current")
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(rgb(TEXT))
                                            .child("☆ Save current")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.pin_current_project(cx);
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("saved-projects-scroll")
                                    .max_h(px(142.0))
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
                                    .max_h(px(235.0))
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
                                    .max_h(px(198.0))
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
                                    .flex_1()
                                    .min_h_0()
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
                    ),
            )
            .child(
                div()
                    .h(px(27.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(TOOLBAR))
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(status)
                    .child(format!("{} · GPUI", branch)),
            )
    }
}

fn main() {
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

    Application::new().run(move |cx: &mut App| {
        bind_editor_keys(cx);
        bind_quick_open_keys(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-p", ToggleFileSearch, None),
            KeyBinding::new("cmd-p", ToggleFileSearch, None),
            KeyBinding::new(
                "enter",
                SubmitTerminalCommand,
                Some("AleraGpuiTerminalInput"),
            ),
        ]);
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                let view = cx.new(|cx| {
                    let terminal_input = cx.new(QuickOpenInput::terminal);
                    let mut workbench = Workbench {
                        tree: WorkspaceTree::new(model.root()).ok(),
                        tree_mode: true,
                        explorer_selection: None,
                        explorer_action: None,
                        explorer_action_input: None,
                        explorer_delete_confirmed: false,
                        model,
                        center_view: CenterView::Terminal,
                        editor: None,
                        session_store,
                        recent_workspaces,
                        saved_projects,
                        show_recent_workspaces: true,
                        worktrees: Vec::new(),
                        worktree_error: None,
                        worktree_loading: false,
                        worktree_busy: false,
                        worktree_create_input: None,
                        worktree_delete_pending: None,
                        picking_workspace: false,
                        picking_save_as: false,
                        quick_open: None,
                        file_index: None,
                        file_index_loading: false,
                        content_query: None,
                        content_loading: false,
                        content_matches: Vec::new(),
                        content_truncated: false,
                        content_error: None,
                        content_searched_query: String::new(),
                        content_request: 0,
                        git_snapshot: None,
                        git_loading: false,
                        git_error: None,
                        git_diff: None,
                        git_diff_loading: false,
                        git_diff_request: 0,
                        workspace_generation: 0,
                        runtime_bridge,
                        runtime_status,
                        runtime_loading: false,
                        terminals: TerminalDock::for_workspace(0),
                        terminal_input,
                        terminal_send_loading: false,
                        terminal_list_loading: false,
                        terminal_read_loading: None,
                        terminal_poll: TerminalPollGate::default(),
                        terminal_tick: 0,
                    };
                    workbench.restore_scratch_tabs(cx);
                    workbench
                });
                view.update(cx, |view, cx| {
                    view.start_terminal_polling(cx);
                    view.refresh_worktrees(cx);
                    view.refresh_git(cx);
                });
                view
            },
        )
        .expect("create native GPUI desktop window");
        cx.activate(true);
    });
}
