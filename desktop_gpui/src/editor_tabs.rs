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
use std::path::Path;

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
}

impl EditorTabs {
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
        let effect = self.tabs[index].close(discard);
        if effect.closed() {
            self.tabs.remove(index);
            self.active = if self.tabs.is_empty() {
                None
            } else {
                Some(index.min(self.tabs.len() - 1))
            };
        }
        effect
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
}
