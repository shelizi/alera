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

pub const MAX_EDITOR_TABS: usize = 24;
pub const EDITOR_TAB_LIMIT: &str = "Editor tab limit reached";

/// Editable tabs for one workspace. Preview tabs stay outside this type.
#[derive(Debug)]
pub struct EditorTabs {
    drafts: Option<EditorDraftStore>,
    tabs: Vec<EditorSession>,
    active: Option<usize>,
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

/// Outcome of a bulk clean-tab close. `blocked` counts tabs that stayed open
/// because they hold unsaved changes or could not be closed; their buffers
/// and recovery drafts are never touched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanCloseReport {
    pub closed: usize,
    pub blocked: usize,
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
            active: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
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
        let Some(store) = &self.drafts else {
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
        self.active = Some(self.tabs.len() - 1);
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
    use super::EditorTabs;
    use crate::editor::EditorDraftStore;
    use crate::editor_session::OpenEffect;
    use std::fs;

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
}
