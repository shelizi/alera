//! Several editable documents in one workspace.
//!
//! Each tab owns its buffer, undo stack, and save path. Opening or focusing
//! another file does not replace a dirty buffer, and Save writes only the
//! active tab.

use crate::editor::{EditorDocument, EditorDraftStore};
use crate::editor_session::{
    CloseEffect, EditorSession, OpenEffect, SaveCommit, DIRTY_SWITCH_REFUSED,
};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_EDITOR_TABS: usize = 24;
pub const EDITOR_TAB_LIMIT: &str = "Editor tab limit reached";

/// Process-wide editor tab instance allocator. Every `EditorTabs` owner —
/// one per pane — draws identities from this counter so an instance ID stays
/// unique across all live owners at once. IDs are handed out once, start at
/// one and are never reused or zeroed.
static NEXT_TAB_INSTANCE: AtomicU64 = AtomicU64::new(1);

fn allocate_tab_instance() -> u64 {
    NEXT_TAB_INSTANCE.fetch_add(1, Ordering::Relaxed)
}

/// Editable tabs for one workspace. Preview tabs stay outside this type.
#[derive(Debug)]
pub struct EditorTabs {
    drafts: Option<EditorDraftStore>,
    tabs: Vec<EditorSession>,
    tab_instances: Vec<u64>,
    active: Option<usize>,
    /// Changes whenever a tab is opened, closed, selected or reordered.
    /// Invalidates a Save As picker callback after close/reopen of the same path.
    revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorTabLabel {
    pub title: String,
    pub dirty: bool,
    pub armed: bool,
}

/// Stable tab identity for mouse-drag reorder. A source tab cannot be
/// mistaken for a different tab that later reuses its numerical index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorTabDragIdentity {
    root: PathBuf,
    path: Option<PathBuf>,
    scratch_id: Option<String>,
}

/// Snapshot of the editor tab that launched a native Save As dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSaveGuard {
    identity: EditorTabDragIdentity,
    revision: u64,
}

/// Outcome of a bulk clean-tab close. `blocked` counts tabs that stayed open
/// because they hold unsaved changes or could not be closed; their buffers
/// and recovery drafts are never touched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanCloseReport {
    pub closed: usize,
    pub blocked: usize,
}

/// Save All keeps untitled buffers and incomplete IME compositions untouched.
/// Errors are reported without preventing later dirty files from saving.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveAllReport {
    pub saved: usize,
    pub needs_save_as: usize,
    pub composing: usize,
    pub failures: Vec<String>,
}

impl SaveAllReport {
    pub fn merge(&mut self, other: Self) {
        self.saved += other.saved;
        self.needs_save_as += other.needs_save_as;
        self.composing += other.composing;
        self.failures.extend(other.failures);
    }

    pub fn status(&self) -> String {
        if self.saved == 0
            && self.needs_save_as == 0
            && self.composing == 0
            && self.failures.is_empty()
        {
            return "No unsaved editor files".to_owned();
        }
        let mut parts = vec![format!("Saved {} editor file(s)", self.saved)];
        if self.needs_save_as > 0 {
            parts.push(format!(
                "{} untitled file(s) need Save As",
                self.needs_save_as
            ));
        }
        if self.composing > 0 {
            parts.push(format!(
                "{} active IME composition(s) skipped",
                self.composing
            ));
        }
        if !self.failures.is_empty() {
            parts.push(format!(
                "{} save error(s): {}",
                self.failures.len(),
                self.failures.join("; ")
            ));
        }
        parts.join("; ")
    }
}

/// A live editor tab held outside every `EditorTabs` owner while it moves
/// between per-pane editors. The exact [`EditorSession`] — buffer, undo
/// history, selection, armed-discard flag and draft-store link — travels
/// unchanged; no file is reloaded and nothing is saved or discarded.
/// Reinstall it with [`EditorTabs::insert_from_pane`]; a rejected insert
/// hands the value back so the caller can restore the previous owner.
#[derive(Debug)]
pub struct DetachedEditorTab {
    session: EditorSession,
    instance: u64,
}

impl EditorTabs {
    pub fn drag_identity_at(&self, index: usize) -> Option<EditorTabDragIdentity> {
        let doc = self.tabs.get(index)?.document()?;
        Some(EditorTabDragIdentity {
            root: doc.root().to_path_buf(),
            path: doc.path().map(Path::to_path_buf),
            scratch_id: doc.scratch_id().map(ToString::to_string),
        })
    }

    pub fn move_tab_identity(&mut self, identity: &EditorTabDragIdentity, to: usize) -> bool {
        let Some(from) = self.tabs.iter().position(|tab| {
            tab.document().is_some_and(|doc| {
                doc.root() == identity.root
                    && doc.path() == identity.path.as_deref()
                    && doc.scratch_id() == identity.scratch_id.as_deref()
            })
        }) else {
            return false;
        };
        self.move_tab(from, to)
    }

    pub fn new(drafts: Option<EditorDraftStore>) -> Self {
        Self {
            drafts,
            tabs: Vec::new(),
            tab_instances: Vec::new(),
            active: None,
            revision: 0,
        }
    }

    pub fn active_save_guard(&self) -> Option<EditorSaveGuard> {
        self.tab_guard_at(self.active?)
    }

    /// A stable contextual tab target. Unlike a bare index or path, it becomes
    /// stale if the user closes/reopens, changes or reorders the tab list.
    pub fn tab_guard_at(&self, index: usize) -> Option<EditorSaveGuard> {
        Some(EditorSaveGuard {
            identity: self.drag_identity_at(index)?,
            revision: self.revision,
        })
    }

    pub fn index_for_guard(&self, guard: &EditorSaveGuard) -> Option<usize> {
        if self.revision != guard.revision {
            return None;
        }
        (0..self.tabs.len())
            .find(|&index| self.drag_identity_at(index).as_ref() == Some(&guard.identity))
    }

    /// Ignore late OS-dialog responses if the user changed, closed, opened
    /// or reordered tabs while choosing the destination.
    pub fn save_as_if_current(
        &mut self,
        guard: &EditorSaveGuard,
        destination: &Path,
        overwrite: bool,
    ) -> Option<SaveCommit> {
        if self.revision != guard.revision || self.active_save_guard().as_ref() != Some(guard) {
            return None;
        }
        Some(self.save_as(destination, overwrite))
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    /// Opaque per-mount tab identity, unique across every `EditorTabs` owner
    /// in the process. A closed/reopened path never inherits its former
    /// identity, even when both documents have the same filename.
    pub fn tab_instance_at(&self, index: usize) -> Option<u64> {
        self.tab_instances.get(index).copied()
    }

    pub fn index_for_instance(&self, instance: u64) -> Option<usize> {
        self.tab_instances.iter().position(|id| *id == instance)
    }

    /// Read-only snapshot for the Workbench's secondary pane. This never
    /// switches the active editor tab or writes to its file/draft.
    pub fn content_for_instance(&self, instance: u64) -> Option<&str> {
        let index = self.index_for_instance(instance)?;
        self.tabs
            .get(index)?
            .document()
            .map(EditorDocument::content)
    }

    /// File-backed tabs alone can be restored by path on another launch.
    /// Unsaved scratch buffers remain exclusively in EditorDraftStore.
    pub fn file_backed_instances(&self) -> Vec<(u64, PathBuf)> {
        self.tabs
            .iter()
            .zip(self.tab_instances.iter().copied())
            .filter_map(|(session, instance)| {
                session
                    .document()?
                    .path()
                    .map(|path| (instance, path.to_path_buf()))
            })
            .collect()
    }

    pub fn has_draft_store(&self) -> bool {
        self.drafts.is_some()
    }

    pub fn document(&self) -> Option<&EditorDocument> {
        self.active_session().and_then(EditorSession::document)
    }

    pub fn document_mut(&mut self) -> Option<&mut EditorDocument> {
        self.active_session_mut()
            .and_then(EditorSession::document_mut)
    }

    pub fn discard_armed(&self) -> bool {
        self.active_session()
            .is_some_and(EditorSession::discard_armed)
    }

    pub fn tab_labels(&self) -> Vec<EditorTabLabel> {
        self.tabs
            .iter()
            .map(|tab| EditorTabLabel {
                title: tab
                    .target_path()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".to_string()),
                dirty: tab.document().is_some_and(EditorDocument::is_dirty),
                armed: tab.discard_armed(),
            })
            .collect()
    }

    /// Open `path` as its own tab. An editor already holding that file is
    /// focused and its buffer is left untouched.
    pub fn open_file(&mut self, root: &Path, path: &Path) -> io::Result<OpenEffect> {
        if let Some(index) = self.matching_tab(Some(path)) {
            return Ok(self.focus(index));
        }
        self.push_session(|session| session.open_file(root, path))
    }

    pub fn open_scratch(&mut self, root: &Path) -> io::Result<OpenEffect> {
        self.push_session(|session| session.open_scratch(root))
    }

    /// Startup hook after selecting a workspace and constructing its draft store.
    /// Idempotent; invalid records or insufficient capacity leave all tabs intact.
    pub fn restore_scratch_drafts(&mut self, root: &Path) -> io::Result<usize> {
        let Some(store) = self.drafts.clone() else {
            return Ok(0);
        };
        let documents: Vec<_> = store
            .restore_scratch(root)?
            .into_iter()
            .filter(|doc| {
                !self
                    .tabs
                    .iter()
                    .filter_map(EditorSession::document)
                    .any(|open| open.root() == doc.root() && open.scratch_id() == doc.scratch_id())
            })
            .collect();
        if self.tabs.len() + documents.len() > MAX_EDITOR_TABS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                EDITOR_TAB_LIMIT,
            ));
        }
        let count = documents.len();
        for document in documents {
            self.tabs
                .push(EditorSession::recovered_scratch(document, store.clone()));
            let instance = allocate_tab_instance();
            self.tab_instances.push(instance);
        }
        if count > 0 {
            self.revision = self.revision.wrapping_add(1);
        }
        if self.active.is_none() && count > 0 {
            self.active = Some(0);
        }
        Ok(count)
    }

    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || self.active == Some(index) {
            return false;
        }
        self.active = Some(index);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Focus the next tab, wrapping from last to first.
    pub fn activate_next(&mut self) -> bool {
        self.activate_wrapped(1)
    }

    /// Focus the previous tab, wrapping from first to last.
    pub fn activate_previous(&mut self) -> bool {
        self.activate_wrapped(-1)
    }

    /// Move the tab at `from` so it lands at `to` in the tab bar. The active
    /// document keeps focus no matter which slot it occupies afterwards.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        let instance = self.tab_instances.remove(from);
        self.tab_instances.insert(to, instance);
        self.revision = self.revision.wrapping_add(1);
        self.active = self.active.map(|current| {
            if current == from {
                to
            } else if to > from && current > from && current <= to {
                current - 1
            } else if to < from && current >= to && current < from {
                current + 1
            } else {
                current
            }
        });
        true
    }

    pub fn save(&mut self) -> SaveCommit {
        match self.active_session_mut() {
            Some(session) => session.save(),
            None => SaveCommit::blocked_by(io::Error::new(
                io::ErrorKind::InvalidInput,
                "No editor is open",
            )),
        }
    }

    /// Save all dirty file-backed tabs without changing the focused tab.
    pub fn save_all(&mut self) -> SaveAllReport {
        let mut report = SaveAllReport::default();
        for tab in &mut self.tabs {
            let Some(document) = tab.document() else {
                continue;
            };
            if !document.is_dirty() {
                continue;
            }
            if document.marked().is_some() {
                report.composing += 1;
                continue;
            }
            let Some(path) = document.path().map(Path::to_path_buf) else {
                report.needs_save_as += 1;
                continue;
            };
            let commit = tab.save();
            if commit.wrote_file() {
                report.saved += 1;
            }
            if commit.blocked() || commit.status().contains("draft cleanup failed") {
                report
                    .failures
                    .push(format!("{}: {}", path.display(), commit.status()));
            }
        }
        report
    }

    pub fn first_dirty_untitled_index(&self) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            tab.document().is_some_and(|document| {
                document.is_dirty() && document.path().is_none() && document.marked().is_none()
            })
        })
    }

    pub fn save_as(&mut self, destination: &Path, overwrite: bool) -> SaveCommit {
        match self.active_session_mut() {
            Some(session) => session.save_as(destination, overwrite),
            None => SaveCommit::blocked_by(io::Error::new(
                io::ErrorKind::InvalidInput,
                "No editor is open",
            )),
        }
    }

    pub fn persist_draft(&self) -> io::Result<()> {
        let mut first_error = None;
        for tab in &self.tabs {
            if let Err(error) = tab.persist_draft() {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub fn close_active(&mut self, discard: bool) -> CloseEffect {
        let Some(index) = self.active else {
            return CloseEffect::Closed;
        };
        self.close_tab(index, discard)
    }

    /// Close one tab. `discard` follows that tab's own arming, not the active tab's.
    pub fn close_tab(&mut self, index: usize, discard: bool) -> CloseEffect {
        if index >= self.tabs.len() {
            return CloseEffect::RefusedUnsaved;
        }
        let effect = self.tabs[index].close(discard);
        if effect.closed() {
            let active = self.active;
            self.tabs.remove(index);
            self.tab_instances.remove(index);
            self.revision = self.revision.wrapping_add(1);
            self.active = match active {
                _ if self.tabs.is_empty() => None,
                Some(current) if current > index => Some(current - 1),
                Some(current) if current == index => Some(index.min(self.tabs.len() - 1)),
                other => other,
            };
        }
        effect
    }

    /// Close every clean tab except `keep`. Dirty file and scratch tabs are
    /// never discarded: they stay open and are counted in `blocked`.
    pub fn close_clean_others(&mut self, keep: usize) -> CleanCloseReport {
        let mut report = CleanCloseReport::default();
        if keep >= self.tabs.len() {
            return report;
        }
        let mut index = self.tabs.len();
        while index > 0 {
            index -= 1;
            if index != keep {
                self.close_clean_at(index, &mut report);
            }
        }
        report
    }

    /// Close every clean tab to the right of `keep`. Dirty tabs stay open and
    /// are counted in `blocked`; nothing is force-discarded.
    pub fn close_clean_to_right(&mut self, keep: usize) -> CleanCloseReport {
        let mut report = CleanCloseReport::default();
        if keep >= self.tabs.len() {
            return report;
        }
        let mut index = self.tabs.len();
        while index > keep + 1 {
            index -= 1;
            self.close_clean_at(index, &mut report);
        }
        report
    }

    /// Detach the live tab registered under `instance` for a move to another
    /// per-pane `EditorTabs` owner. The exact [`EditorSession`] — buffer,
    /// undo history, selection, armed-discard flag and draft-store link — is
    /// carried unchanged: no file is reloaded and nothing is saved or
    /// discarded. `active` and `revision` follow the removal exactly as in
    /// [`Self::close_tab`]. Returns `None` for an unknown or stale instance.
    pub fn take_for_pane(&mut self, instance: u64) -> Option<DetachedEditorTab> {
        let index = self.index_for_instance(instance)?;
        // The platform IME composition is still owned by the current native
        // input handler. A pane move must not detach that live buffer while
        // Windows TSF may still deliver marked-text callbacks to this view.
        // Leave tab order, selection, revision and undo history unchanged.
        if self.tabs[index]
            .document()
            .is_some_and(|document| document.marked().is_some())
        {
            return None;
        }
        let session = self.tabs.remove(index);
        self.tab_instances.remove(index);
        self.revision = self.revision.wrapping_add(1);
        self.active = match self.active {
            _ if self.tabs.is_empty() => None,
            Some(current) if current > index => Some(current - 1),
            Some(current) if current == index => Some(index.min(self.tabs.len() - 1)),
            other => other,
        };
        Some(DetachedEditorTab { session, instance })
    }

    /// Install a tab detached by [`Self::take_for_pane`] under its original
    /// instance ID and make it active. When the tab limit is reached or
    /// another tab already holds the same file/scratch identity, the
    /// detached tab is returned unchanged so the caller can roll back to its
    /// previous owner. No save or discard runs on either side of the move.
    pub fn insert_from_pane(
        &mut self,
        detached: DetachedEditorTab,
    ) -> Result<(), DetachedEditorTab> {
        let DetachedEditorTab { session, instance } = detached;
        let duplicate = session
            .document()
            .is_some_and(|document| self.has_document_identity(document));
        if self.tabs.len() >= MAX_EDITOR_TABS || duplicate {
            return Err(DetachedEditorTab { session, instance });
        }
        self.tabs.push(session);
        self.tab_instances.push(instance);
        self.active = Some(self.tabs.len() - 1);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Whether a live tab already holds `document`'s identity: the same
    /// workspace root plus either the same canonical file path or the same
    /// scratch recovery id. File and scratch tabs share this one rule.
    fn has_document_identity(&self, document: &EditorDocument) -> bool {
        self.tabs
            .iter()
            .filter_map(EditorSession::document)
            .any(|open| {
                open.root() == document.root()
                    && open.path() == document.path()
                    && open.scratch_id() == document.scratch_id()
            })
    }

    pub fn can_switch_workspace(&self) -> Result<(), &'static str> {
        if self
            .tabs
            .iter()
            .any(|tab| tab.document().is_some_and(EditorDocument::is_dirty))
        {
            Err(DIRTY_SWITCH_REFUSED)
        } else {
            Ok(())
        }
    }

    /// Close one tab only when it holds no unsaved changes. A dirty tab is
    /// left completely untouched (no arming, no draft removal) and reported
    /// as blocked.
    fn close_clean_at(&mut self, index: usize, report: &mut CleanCloseReport) {
        if self.tabs[index]
            .document()
            .is_some_and(EditorDocument::is_dirty)
        {
            report.blocked += 1;
            return;
        }
        if self.close_tab(index, false).closed() {
            report.closed += 1;
        } else {
            report.blocked += 1;
        }
    }

    fn focus(&mut self, index: usize) -> OpenEffect {
        if self.active == Some(index) {
            OpenEffect::AlreadyOpen
        } else {
            self.active = Some(index);
            self.revision = self.revision.wrapping_add(1);
            OpenEffect::Focused
        }
    }

    fn matching_tab(&self, path: Option<&Path>) -> Option<usize> {
        let path = path?;
        self.tabs
            .iter()
            .position(|tab| tab.target_path() == Some(path))
    }

    fn push_session(
        &mut self,
        open: impl FnOnce(&mut EditorSession) -> io::Result<OpenEffect>,
    ) -> io::Result<OpenEffect> {
        if self.tabs.len() >= MAX_EDITOR_TABS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                EDITOR_TAB_LIMIT,
            ));
        }
        let mut session = EditorSession::new(self.drafts.clone());
        let effect = open(&mut session)?;
        if let Some(index) = self.matching_tab(session.target_path()) {
            return Ok(self.focus(index));
        }
        self.tabs.push(session);
        let instance = allocate_tab_instance();
        self.tab_instances.push(instance);
        self.active = Some(self.tabs.len() - 1);
        self.revision = self.revision.wrapping_add(1);
        Ok(effect)
    }

    fn activate_wrapped(&mut self, delta: isize) -> bool {
        let len = self.tabs.len();
        if len < 2 {
            return false;
        }
        let next = match self.active {
            Some(index) => (index as isize + delta).rem_euclid(len as isize) as usize,
            None if delta > 0 => 0,
            None => len - 1,
        };
        self.activate(next)
    }

    fn active_session(&self) -> Option<&EditorSession> {
        self.active.and_then(|index| self.tabs.get(index))
    }

    fn active_session_mut(&mut self) -> Option<&mut EditorSession> {
        let index = self.active?;
        self.tabs.get_mut(index)
    }
}

#[cfg(test)]
mod tests {
    use super::{EditorTabs, MAX_EDITOR_TABS};
    use crate::editor::EditorDraftStore;
    use crate::editor_session::OpenEffect;
    use std::fs;

    #[test]
    fn explicit_git_snapshot_to_scratch_is_editable_without_modifying_source_file() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        fs::write(root.join("tracked.txt"), "live worktree content").unwrap();
        let mut tabs = EditorTabs::new(None);
        tabs.open_scratch(root).unwrap();
        let snapshot = "old committed content\n";
        let doc = tabs.document_mut().unwrap();
        doc.replace(None, snapshot).unwrap();
        assert!(doc.is_dirty());
        assert!(doc.path().is_none());
        assert_eq!(doc.content(), snapshot);
        doc.undo();
        assert_eq!(doc.content(), "");
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).unwrap(),
            "live worktree content"
        );
    }

    #[test]
    fn tab_instance_follows_reorder_but_is_not_reused_after_close() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let mut tabs = EditorTabs::new(None);
        let first_path = root.join("first.txt");
        let second_path = root.join("second.txt");
        fs::write(&first_path, "first").unwrap();
        fs::write(&second_path, "second").unwrap();
        tabs.open_file(root, &first_path).unwrap();
        let first = tabs.tab_instance_at(0).unwrap();
        tabs.open_file(root, &second_path).unwrap();
        let second = tabs.tab_instance_at(1).unwrap();
        assert_ne!(first, second);
        assert!(tabs.move_tab(0, 1));
        assert_eq!(tabs.index_for_instance(first), Some(1));
        assert!(tabs.close_tab(1, false).closed());
        tabs.open_file(root, &first_path).unwrap();
        assert!(tabs.index_for_instance(first).is_none());
        assert_ne!(tabs.tab_instance_at(1), Some(first));
    }

    #[test]
    fn one_failed_scratch_flush_does_not_block_other_tabs() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut tabs = EditorTabs::new(Some(store.clone()));
        tabs.open_scratch(workspace.path()).unwrap();
        tabs.document_mut()
            .unwrap()
            .replace(None, &"\u{1}".repeat(crate::editor::MAX_EDITOR_BYTES))
            .unwrap();
        tabs.open_scratch(workspace.path()).unwrap();
        tabs.document_mut()
            .unwrap()
            .replace(None, "survives")
            .unwrap();
        assert!(tabs.persist_draft().is_err());
        assert_eq!(
            store.restore_scratch(workspace.path()).unwrap()[0].content(),
            "survives"
        );
        tabs.activate(0);
        assert_eq!(
            tabs.document().unwrap().content().len(),
            crate::editor::MAX_EDITOR_BYTES
        );
    }

    #[test]
    fn scratch_restart_is_independent_idempotent_and_workspace_scoped() {
        let workspace = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut tabs = EditorTabs::new(Some(store.clone()));
        for text in ["first 中文", "second"] {
            tabs.open_scratch(workspace.path()).unwrap();
            tabs.document_mut().unwrap().replace(None, text).unwrap();
        }
        tabs.persist_draft().unwrap();
        tabs.activate(0);
        tabs.document_mut().unwrap().replace(None, "!").unwrap();
        tabs.persist_draft().unwrap();
        drop(tabs);
        let mut tabs = EditorTabs::new(Some(store));
        assert_eq!(tabs.restore_scratch_drafts(other.path()).unwrap(), 0);
        assert_eq!(tabs.restore_scratch_drafts(workspace.path()).unwrap(), 2);
        assert_eq!(tabs.restore_scratch_drafts(workspace.path()).unwrap(), 0);
        let mut contents = Vec::new();
        for index in 0..2 {
            tabs.activate(index);
            let doc = tabs.document().unwrap();
            assert!(doc.path().is_none() && doc.is_dirty());
            contents.push(doc.content().to_owned());
        }
        contents.sort();
        assert_eq!(contents, ["first 中文!", "second"]);
        assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 0);
    }

    #[test]
    fn scratch_cancel_discard_and_save_as_preserve_or_clear_recovery() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut tabs = EditorTabs::new(Some(store.clone()));
        tabs.open_scratch(workspace.path()).unwrap();
        tabs.document_mut().unwrap().replace(None, "keep").unwrap();
        tabs.persist_draft().unwrap();
        assert!(!tabs.close_active(true).closed());
        assert!(!tabs.close_active(false).closed());
        let mut restarted = EditorTabs::new(Some(store.clone()));
        assert_eq!(
            restarted.restore_scratch_drafts(workspace.path()).unwrap(),
            1
        );
        let occupied = workspace.path().join("occupied.txt");
        fs::write(&occupied, "original").unwrap();
        assert!(tabs.save_as(&occupied, false).blocked());
        assert_eq!(store.restore_scratch(workspace.path()).unwrap().len(), 1);
        let saved = workspace.path().join("saved.txt");
        assert!(tabs.save_as(&saved, false).wrote_file());
        assert_eq!(fs::read_to_string(saved).unwrap(), "keep");
        assert!(store.restore_scratch(workspace.path()).unwrap().is_empty());
        tabs.open_scratch(workspace.path()).unwrap();
        tabs.document_mut()
            .unwrap()
            .replace(None, "discard")
            .unwrap();
        tabs.persist_draft().unwrap();
        assert!(!tabs.close_active(true).closed());
        assert!(tabs.close_active(true).closed());
        assert!(store.restore_scratch(workspace.path()).unwrap().is_empty());
    }

    #[test]
    fn save_all_writes_other_dirty_tabs_but_leaves_untitled_and_active_untouched() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let a = root.join("a.txt");
        let b = root.join("b.txt");
        fs::write(&a, "a").unwrap();
        fs::write(&b, "b").unwrap();
        let mut tabs = EditorTabs::new(Some(EditorDraftStore::new(state.path())));
        tabs.open_file(root, &a).unwrap();
        tabs.document_mut().unwrap().replace(None, "!").unwrap();
        tabs.open_file(root, &b).unwrap();
        tabs.document_mut().unwrap().replace(None, "?").unwrap();
        tabs.open_scratch(root).unwrap();
        tabs.document_mut()
            .unwrap()
            .replace(None, "keep me")
            .unwrap();
        let current = tabs.active_index();
        let first = tabs.save_all();
        assert_eq!(first.saved, 2, "{}", first.status());
        assert_eq!(first.needs_save_as, 1);
        assert_eq!(first.failures.len(), 0);
        assert_eq!(tabs.first_dirty_untitled_index(), current);
        assert_eq!(tabs.active_index(), current);
        assert_eq!(tabs.document().unwrap().content(), "keep me");
        assert_eq!(fs::read_to_string(&a).unwrap(), "a!");
        assert_eq!(fs::read_to_string(&b).unwrap(), "b?");
        assert_eq!(tabs.save_all().saved, 0);
        assert_eq!(tabs.save_all().needs_save_as, 1);
    }

    #[test]
    fn tabs_keep_separate_buffers_and_save_only_the_active_file() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let file_a = root.join("a.txt");
        let file_b = root.join("b.txt");
        fs::write(&file_a, "A").unwrap();
        fs::write(&file_b, "B").unwrap();
        let mut tabs = EditorTabs::new(Some(EditorDraftStore::new(state.path())));
        assert!(matches!(
            tabs.open_file(root, &file_a).unwrap(),
            OpenEffect::Opened { recovered: false }
        ));
        tabs.document_mut().unwrap().replace(None, "!").unwrap();
        tabs.persist_draft().unwrap();
        assert!(matches!(
            tabs.open_file(root, &file_b).unwrap(),
            OpenEffect::Opened { recovered: false }
        ));
        assert_eq!(tabs.document().unwrap().content(), "B");
        tabs.document_mut().unwrap().replace(None, "?").unwrap();
        assert!(tabs.activate(0));
        assert_eq!(tabs.document().unwrap().content(), "A!");
        assert_eq!(tabs.document().unwrap().path(), Some(file_a.as_path()));
        let saved = tabs.save();
        assert!(saved.wrote_file(), "{}", saved.status());
        assert_eq!(fs::read(&file_a).unwrap(), b"A!");
        assert_eq!(fs::read(&file_b).unwrap(), b"B");
        assert!(tabs.activate(1));
        assert_eq!(tabs.document().unwrap().content(), "B?");
        assert!(tabs.can_switch_workspace().is_err());
        let refused = tabs.close_active(false);
        assert!(!refused.closed());
        assert_eq!(tabs.tab_labels().len(), 2);
        assert!(!tabs.close_active(true).closed());
        let closed = tabs.close_active(true);
        assert!(closed.closed());
        assert_eq!(tabs.document().unwrap().content(), "A!");
        assert_eq!(fs::read(&file_b).unwrap(), b"B");
        assert!(matches!(
            tabs.open_file(root, &file_a).unwrap(),
            OpenEffect::AlreadyOpen
        ));
        assert_eq!(tabs.document().unwrap().content(), "A!");
    }

    #[test]
    fn closing_one_tab_leaves_the_other_buffer_and_file_unchanged() {
        use crate::editor_session::CloseEffect;

        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let file_a = root.join("a.txt");
        let file_b = root.join("b.txt");
        fs::write(&file_a, "A").unwrap();
        fs::write(&file_b, "B").unwrap();
        let mut tabs = EditorTabs::new(None);
        tabs.open_file(root, &file_a).unwrap();
        tabs.document_mut().unwrap().replace(None, "!").unwrap();
        tabs.open_file(root, &file_b).unwrap();
        tabs.document_mut().unwrap().replace(None, "?").unwrap();

        let refused = tabs.close_tab(0, false);
        assert!(matches!(refused, CloseEffect::RefusedUnsaved));
        assert_eq!(tabs.tab_labels().len(), 2);
        assert_eq!(tabs.document().unwrap().content(), "B?");
        assert_eq!(fs::read(&file_a).unwrap(), b"A");

        assert!(matches!(tabs.close_tab(0, true), CloseEffect::Armed));
        assert!(!tabs.discard_armed());
        assert!(matches!(tabs.close_tab(1, true), CloseEffect::Armed));
        assert!(tabs.discard_armed());
        assert_eq!(tabs.tab_labels().len(), 2);

        assert!(matches!(tabs.close_tab(0, true), CloseEffect::Closed));
        assert_eq!(tabs.tab_labels().len(), 1);
        assert_eq!(tabs.document().unwrap().path(), Some(file_b.as_path()));
        assert_eq!(tabs.document().unwrap().content(), "B?");
        assert_eq!(fs::read(&file_a).unwrap(), b"A");
        assert_eq!(fs::read(&file_b).unwrap(), b"B");
        assert!(matches!(tabs.close_active(true), CloseEffect::Closed));
        assert!(tabs.is_empty());
    }

    #[test]
    fn drag_identity_reorders_only_original_scratch_and_keeps_dirty_content() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let mut tabs = EditorTabs::new(None);
        tabs.open_scratch(root).unwrap();
        tabs.document_mut()
            .unwrap()
            .replace(None, "unsaved 中文")
            .unwrap();
        let scratch = tabs.drag_identity_at(0).unwrap();
        let file = root.join("existing.txt");
        fs::write(&file, "on disk").unwrap();
        tabs.open_file(root, &file).unwrap();
        let file_identity = tabs.drag_identity_at(1).unwrap();
        assert!(tabs.move_tab_identity(&scratch, 1));
        assert_eq!(tabs.active_index(), Some(0));
        assert_eq!(tabs.document().unwrap().path(), Some(file.as_path()));
        assert!(!tabs.move_tab_identity(&file_identity, 0), "already first");
        assert!(tabs.activate(1));
        assert_eq!(tabs.document().unwrap().content(), "unsaved 中文");
        assert!(tabs.document().unwrap().is_dirty());
        assert!(!tabs.move_tab_identity(&scratch, 99));
        tabs.close_tab(1, true);
        assert!(
            tabs.drag_identity_at(1).is_some(),
            "dirty scratch must survive first close until confirmation"
        );
        tabs.close_tab(1, true);
        assert!(
            !tabs.move_tab_identity(&scratch, 0),
            "closed identity must be stale"
        );
    }

    #[test]
    fn tab_reorder_and_wrap_keep_same_active_document() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let paths: Vec<_> = ["a.txt", "b.txt", "c.txt"]
            .iter()
            .map(|name| root.join(name))
            .collect();
        let mut tabs = EditorTabs::new(None);
        assert!(!tabs.activate_next());
        assert!(!tabs.activate_previous());
        for (index, path) in paths.iter().enumerate() {
            fs::write(path, format!("{index}")).unwrap();
            tabs.open_file(root, path).unwrap();
        }
        assert_eq!(tabs.document().unwrap().path(), Some(paths[2].as_path()));
        assert!(tabs.move_tab(0, 2));
        assert_eq!(tabs.active_index(), Some(1));
        assert_eq!(tabs.document().unwrap().path(), Some(paths[2].as_path()));
        assert!(tabs.activate_next());
        assert_eq!(tabs.document().unwrap().path(), Some(paths[0].as_path()));
        assert!(tabs.activate_next());
        assert_eq!(tabs.document().unwrap().path(), Some(paths[1].as_path()));
        assert!(tabs.activate_previous());
        assert_eq!(tabs.document().unwrap().path(), Some(paths[0].as_path()));
        assert!(!tabs.move_tab(5, 0));
    }

    #[test]
    fn bulk_close_skips_unsaved_documents_and_preserves_active() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let paths: Vec<_> = ["a.txt", "b.txt", "c.txt", "d.txt"]
            .iter()
            .map(|name| root.join(name))
            .collect();
        let mut tabs = EditorTabs::new(None);
        for path in &paths {
            fs::write(path, path.file_name().unwrap().to_string_lossy().as_bytes()).unwrap();
            tabs.open_file(root, path).unwrap();
        }
        assert!(tabs.activate(1));
        tabs.document_mut().unwrap().replace(None, "!").unwrap();
        assert!(tabs.activate(2));
        let result = tabs.close_clean_others(2);
        assert_eq!(result.closed, 2);
        assert_eq!(result.blocked, 1);
        assert_eq!(tabs.tab_labels().len(), 2);
        assert_eq!(tabs.active_index(), Some(1));
        assert_eq!(tabs.document().unwrap().path(), Some(paths[2].as_path()));
        assert!(tabs.activate(0));
        assert!(tabs.document().unwrap().is_dirty());
        let result = tabs.close_clean_to_right(0);
        assert_eq!(result.closed, 1);
        assert_eq!(result.blocked, 0);
        assert_eq!(tabs.tab_labels().len(), 1);
        assert!(tabs.document().unwrap().is_dirty());
        assert_eq!(fs::read(&paths[1]).unwrap(), b"b.txt");
    }

    #[test]
    fn inactive_dirty_draft_survives_close_shortcut_and_last_tab_can_reopen() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let file = root.join("clean.txt");
        fs::write(&file, b"saved file").unwrap();
        let mut tabs = EditorTabs::new(None);
        tabs.open_scratch(root).unwrap();
        tabs.document_mut()
            .unwrap()
            .replace(None, "keep me")
            .unwrap();
        tabs.open_file(root, &file).unwrap();
        assert_eq!(tabs.tab_labels().len(), 2);
        assert!(
            !tabs.document().unwrap().is_dirty(),
            "selected tab is clean"
        );
        assert!(
            tabs.can_switch_workspace().is_err(),
            "inactive unsaved scratch must block destructive workspace changes"
        );

        // Ctrl+W only closes the clean file. The dirty draft remains even
        // after the final clean tab closes.
        assert!(tabs.close_active(false).closed());
        assert_eq!(tabs.tab_labels().len(), 1);
        assert_eq!(tabs.document().unwrap().content(), "keep me");
        assert!(!tabs.close_active(false).closed());
        assert_eq!(tabs.document().unwrap().content(), "keep me");

        // Explicit two-step discard still works; the empty tab strip can
        // create a new scratch without needing a different workspace.
        assert!(!tabs.close_active(true).closed());
        assert!(tabs.close_active(true).closed());
        assert!(tabs.is_empty());
        tabs.open_scratch(root).unwrap();
        assert_eq!(tabs.tab_labels().len(), 1);
        assert_eq!(tabs.document().unwrap().content(), "");
        assert_eq!(fs::read(&file).unwrap(), b"saved file");
    }

    #[test]
    fn save_as_picker_cannot_write_another_tab_after_switch_or_reorder() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let destination = root.join("picked.txt");
        let mut tabs = EditorTabs::new(None);
        tabs.open_scratch(root).unwrap();
        tabs.document_mut()
            .unwrap()
            .replace(None, "source draft")
            .unwrap();
        let first_request = tabs.active_save_guard().unwrap();

        tabs.open_scratch(root).unwrap();
        tabs.document_mut()
            .unwrap()
            .replace(None, "other draft")
            .unwrap();
        assert!(tabs
            .save_as_if_current(&first_request, &destination, false)
            .is_none());
        assert!(!destination.exists());
        assert_eq!(tabs.document().unwrap().content(), "other draft");

        assert!(tabs.activate(0));
        assert!(
            tabs.save_as_if_current(&first_request, &destination, false)
                .is_none(),
            "switching back must not reactivate an obsolete dialog"
        );
        let active_request = tabs.active_save_guard().unwrap();
        assert!(tabs.move_tab(0, 1));
        assert!(
            tabs.save_as_if_current(&active_request, &destination, false)
                .is_none(),
            "reordered tabs invalidate the pending request"
        );
        assert!(!destination.exists());

        let fresh_request = tabs.active_save_guard().unwrap();
        assert!(tabs
            .save_as_if_current(&fresh_request, &destination, false)
            .unwrap()
            .wrote_file());
        assert_eq!(fs::read_to_string(&destination).unwrap(), "source draft");
        assert_eq!(tabs.tab_labels().len(), 2);
    }

    #[test]
    fn save_as_picker_rejects_closed_and_reopened_same_path_and_unconfirmed_overwrite() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let named = root.join("named.txt");
        let occupied = root.join("occupied.txt");
        let new_path = root.join("new.txt");
        fs::write(&named, "original").unwrap();
        fs::write(&occupied, "untouched").unwrap();
        let mut tabs = EditorTabs::new(None);
        tabs.open_file(root, &named).unwrap();
        let prior_session = tabs.active_save_guard().unwrap();
        assert!(tabs.close_active(false).closed());
        tabs.open_file(root, &named).unwrap();
        assert!(
            tabs.save_as_if_current(&prior_session, &new_path, false)
                .is_none(),
            "close/reopen the same file is not the same editor session"
        );
        let current_session = tabs.active_save_guard().unwrap();
        let refused = tabs
            .save_as_if_current(&current_session, &occupied, false)
            .unwrap();
        assert!(refused.blocked());
        assert_eq!(fs::read_to_string(&occupied).unwrap(), "untouched");
        assert_eq!(tabs.document().unwrap().path(), Some(named.as_path()));
        assert!(!new_path.exists());
    }

    #[test]
    fn right_click_context_targets_inactive_tab_and_never_drops_dirty_neighbors() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let paths: Vec<_> = ["a.txt", "b.txt", "c.txt", "d.txt"]
            .iter()
            .map(|name| root.join(name))
            .collect();
        let mut tabs = EditorTabs::new(None);
        for path in &paths {
            fs::write(path, path.file_name().unwrap().to_string_lossy().as_bytes()).unwrap();
            tabs.open_file(root, path).unwrap();
        }
        tabs.activate(2);
        tabs.document_mut()
            .unwrap()
            .replace(None, "not saved")
            .unwrap();
        tabs.activate(3);
        let right_clicked = tabs.tab_guard_at(1).unwrap();
        assert_eq!(tabs.index_for_guard(&right_clicked), Some(1));
        assert_eq!(tabs.document().unwrap().path(), Some(paths[3].as_path()));

        // A context action on b does not first switch the current editor:
        // only clean tabs to b's right close, dirty c remains untouched.
        let result = tabs.close_clean_to_right(tabs.index_for_guard(&right_clicked).unwrap());
        assert_eq!(result.closed, 1);
        assert_eq!(result.blocked, 1);
        assert_eq!(tabs.tab_labels().len(), 3);
        assert_eq!(tabs.document().unwrap().path(), Some(paths[2].as_path()));
        assert!(tabs.document().unwrap().is_dirty());
        assert!(tabs.index_for_guard(&right_clicked).is_none());
        assert_eq!(fs::read_to_string(&paths[2]).unwrap(), "c.txt");
    }

    #[test]
    fn pane_transfer_preserves_dirty_buffer_undo_and_disk_bytes() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let file = root.join("move.txt");
        fs::write(&file, "disk\n").unwrap();
        let mut source = EditorTabs::new(None);
        let mut target = EditorTabs::new(None);
        source.open_file(root, &file).unwrap();
        let instance = source.tab_instance_at(0).unwrap();
        {
            let doc = source.document_mut().unwrap();
            doc.replace(None, "alpha").unwrap();
            doc.replace(None, "beta").unwrap();
        }
        let doc = source.document().unwrap();
        assert!(doc.is_dirty());
        let (undo, redo) = doc.history_depth();
        assert!(undo > 0);

        let detached = source.take_for_pane(instance).expect("open tab detaches");
        assert!(source.is_empty());
        assert_eq!(source.active_index(), None);
        assert!(source.index_for_instance(instance).is_none());
        assert_eq!(detached.instance, instance);

        target.insert_from_pane(detached).unwrap();
        assert_eq!(target.active_index(), Some(0));
        assert_eq!(target.index_for_instance(instance), Some(0));
        assert_eq!(target.tab_instance_at(0), Some(instance));
        let doc = target.document().unwrap();
        assert_eq!(doc.path(), Some(file.as_path()));
        assert_eq!(doc.content(), "disk\nalphabeta");
        assert!(doc.is_dirty());
        assert_eq!(doc.history_depth(), (undo, redo));

        target.document_mut().unwrap().undo();
        assert_eq!(target.document().unwrap().content(), "disk\nalpha");
        assert!(target.document().unwrap().is_dirty());
        assert_eq!(fs::read_to_string(&file).unwrap(), "disk\n");
    }

    #[test]
    fn pane_transfer_preserves_scratch_identity_and_draft_recovery() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut source = EditorTabs::new(Some(store.clone()));
        let mut target = EditorTabs::new(None);
        source.open_scratch(workspace.path()).unwrap();
        let instance = source.tab_instance_at(0).unwrap();
        let scratch_id = source.document().unwrap().scratch_id().unwrap().to_string();
        source
            .document_mut()
            .unwrap()
            .replace(None, "scratch 中文")
            .unwrap();

        let detached = source.take_for_pane(instance).unwrap();
        target.insert_from_pane(detached).unwrap();
        let doc = target.document().unwrap();
        assert!(doc.path().is_none());
        assert_eq!(doc.scratch_id(), Some(scratch_id.as_str()));
        assert_eq!(doc.content(), "scratch 中文");
        assert!(doc.is_dirty());

        // The moved session still owns its draft store: persisting through
        // the receiving owner writes the same scratch recovery record.
        target.persist_draft().unwrap();
        let restored = store.restore_scratch(workspace.path()).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].content(), "scratch 中文");
        assert_eq!(restored[0].scratch_id(), Some(scratch_id.as_str()));
    }

    #[test]
    fn tab_instances_are_unique_and_nonzero_across_owners() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let first_file = root.join("first.txt");
        let second_file = root.join("second.txt");
        fs::write(&first_file, "1").unwrap();
        fs::write(&second_file, "2").unwrap();
        let mut first = EditorTabs::new(None);
        let mut second = EditorTabs::new(None);
        first.open_file(root, &first_file).unwrap();
        second.open_file(root, &second_file).unwrap();
        first.open_scratch(root).unwrap();
        second.open_scratch(root).unwrap();
        let collect = |tabs: &EditorTabs| -> Vec<u64> {
            (0..tabs.tab_labels().len())
                .map(|index| tabs.tab_instance_at(index).unwrap())
                .collect()
        };
        let first_ids = collect(&first);
        let second_ids = collect(&second);
        assert_eq!(first_ids.len(), 2);
        assert_eq!(second_ids.len(), 2);
        assert!(first_ids.iter().chain(second_ids.iter()).all(|id| *id != 0));
        for id in &first_ids {
            assert!(
                !second_ids.contains(id),
                "instance {id} collided across owners"
            );
        }
        let mut all = first_ids;
        all.extend(second_ids);
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn insert_rejects_duplicate_file_identity_and_returns_detached_tab() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let file = root.join("shared.txt");
        fs::write(&file, "original").unwrap();
        let mut source = EditorTabs::new(None);
        let mut target = EditorTabs::new(None);
        // Both owners independently opened the same file: two live buffers.
        target.open_file(root, &file).unwrap();
        source.open_file(root, &file).unwrap();
        source.document_mut().unwrap().replace(None, "!").unwrap();
        let instance = source.tab_instance_at(0).unwrap();
        let labels_before = target.tab_labels();
        let guard = target.tab_guard_at(0).unwrap();

        let detached = source.take_for_pane(instance).unwrap();
        let returned = target
            .insert_from_pane(detached)
            .expect_err("duplicate file identity must be rejected");
        assert_eq!(returned.instance, instance);
        assert_eq!(target.tab_labels(), labels_before);
        assert_eq!(
            target.index_for_guard(&guard),
            Some(0),
            "a rejected insert must not bump revision"
        );
        assert_eq!(target.document().unwrap().content(), "original");

        // Rollback: the untouched session goes back to its previous owner.
        source.insert_from_pane(returned).unwrap();
        assert_eq!(source.index_for_instance(instance), Some(0));
        assert_eq!(source.document().unwrap().content(), "original!");
        assert!(source.document().unwrap().is_dirty());
        assert_eq!(fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn insert_rejects_duplicate_scratch_identity_and_returns_detached_tab() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut seed = EditorTabs::new(Some(store.clone()));
        seed.open_scratch(workspace.path()).unwrap();
        seed.document_mut()
            .unwrap()
            .replace(None, "seeded")
            .unwrap();
        seed.persist_draft().unwrap();
        drop(seed);

        // Two owners restore the same persisted scratch draft, so both hold
        // a live session with the same scratch identity.
        let mut first = EditorTabs::new(Some(store.clone()));
        let mut second = EditorTabs::new(Some(store.clone()));
        assert_eq!(first.restore_scratch_drafts(workspace.path()).unwrap(), 1);
        assert_eq!(second.restore_scratch_drafts(workspace.path()).unwrap(), 1);
        let instance = first.tab_instance_at(0).unwrap();
        let scratch_id = first.document().unwrap().scratch_id().unwrap().to_string();

        let detached = first.take_for_pane(instance).unwrap();
        let returned = second
            .insert_from_pane(detached)
            .expect_err("same scratch identity must be rejected");
        assert_eq!(returned.instance, instance);
        assert_eq!(second.tab_labels().len(), 1);

        first.insert_from_pane(returned).unwrap();
        assert_eq!(first.index_for_instance(instance), Some(0));
        assert_eq!(first.document().unwrap().content(), "seeded");
        assert_eq!(
            first.document().unwrap().scratch_id(),
            Some(scratch_id.as_str())
        );
    }

    #[test]
    fn insert_rejects_at_tab_limit_and_returns_detached_tab() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let file = root.join("limited.txt");
        fs::write(&file, "x").unwrap();
        let mut source = EditorTabs::new(None);
        source.open_file(root, &file).unwrap();
        let instance = source.tab_instance_at(0).unwrap();

        let mut target = EditorTabs::new(None);
        for _ in 0..MAX_EDITOR_TABS {
            target.open_scratch(root).unwrap();
        }
        assert_eq!(target.tab_labels().len(), MAX_EDITOR_TABS);
        assert_eq!(target.active_index(), Some(MAX_EDITOR_TABS - 1));

        let detached = source.take_for_pane(instance).unwrap();
        let returned = target
            .insert_from_pane(detached)
            .expect_err("a full target must reject the move");
        assert_eq!(returned.instance, instance);
        assert_eq!(target.tab_labels().len(), MAX_EDITOR_TABS);
        assert_eq!(target.active_index(), Some(MAX_EDITOR_TABS - 1));

        source.insert_from_pane(returned).unwrap();
        assert_eq!(source.index_for_instance(instance), Some(0));
        assert_eq!(source.document().unwrap().path(), Some(file.as_path()));
    }

    #[test]
    fn take_and_insert_bump_revision_and_reject_stale_instances() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let file_a = root.join("a.txt");
        let file_b = root.join("b.txt");
        fs::write(&file_a, "a").unwrap();
        fs::write(&file_b, "b").unwrap();
        let mut source = EditorTabs::new(None);
        source.open_file(root, &file_a).unwrap();
        source.open_file(root, &file_b).unwrap();
        source.activate(0);
        let guard = source.tab_guard_at(0).unwrap();
        let instance_b = source.tab_instance_at(1).unwrap();

        assert!(source.take_for_pane(0).is_none(), "zero is never valid");
        assert!(source.take_for_pane(u64::MAX).is_none());
        assert_eq!(source.tab_labels().len(), 2);

        let detached = source.take_for_pane(instance_b).unwrap();
        assert!(
            source.take_for_pane(instance_b).is_none(),
            "a moved instance is stale in its previous owner"
        );
        assert!(
            source.index_for_guard(&guard).is_none(),
            "take must bump revision so stale guards fail"
        );
        assert_eq!(source.tab_labels().len(), 1);
        assert_eq!(source.active_index(), Some(0));
        assert_eq!(source.document().unwrap().path(), Some(file_a.as_path()));

        let mut target = EditorTabs::new(None);
        target.open_file(root, &file_a).unwrap();
        let target_guard = target.tab_guard_at(0).unwrap();
        target.insert_from_pane(detached).unwrap();
        assert!(
            target.index_for_guard(&target_guard).is_none(),
            "insert must bump revision so stale guards fail"
        );
        assert_eq!(target.tab_labels().len(), 2);
        assert_eq!(target.active_index(), Some(1));
        assert_eq!(target.document().unwrap().path(), Some(file_b.as_path()));
    }

    #[test]
    fn pane_transfer_during_chinese_ime_preedit_is_refused_until_commit() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        let file = root.join("composing.txt");
        fs::write(&file, "base").unwrap();
        let mut source = EditorTabs::new(None);
        let mut target = EditorTabs::new(None);
        source.open_file(root, &file).unwrap();
        let instance = source.tab_instance_at(0).unwrap();
        source
            .document_mut()
            .unwrap()
            .replace_and_mark(None, "\u{3105}\u{310b}", None)
            .unwrap();
        let before = source.tab_guard_at(0).unwrap();

        // Windows IME composition belongs to the source GPUI input handler.
        // Detaching it would orphan pending TSF updates after focus changes.
        assert!(source.take_for_pane(instance).is_none());
        assert_eq!(source.index_for_instance(instance), Some(0));
        assert_eq!(source.index_for_guard(&before), Some(0));
        assert!(source.document().unwrap().marked().is_some());
        assert_eq!(source.document().unwrap().history_depth(), (0, 0));
        assert!(target.is_empty());

        // A committed IME candidate stays one undo step across the move.
        source
            .document_mut()
            .unwrap()
            .replace(None, "\u{4e2d}")
            .unwrap();
        assert!(source.document().unwrap().marked().is_none());
        let detached = source.take_for_pane(instance).unwrap();
        target.insert_from_pane(detached).unwrap();
        assert_eq!(target.document().unwrap().content(), "base\u{4e2d}");
        assert_eq!(target.document().unwrap().history_depth(), (1, 0));
        target.document_mut().unwrap().undo();
        assert_eq!(target.document().unwrap().content(), "base");
        assert_eq!(fs::read_to_string(file).unwrap(), "base");
    }
}
