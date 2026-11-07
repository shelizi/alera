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
    pub armed: bool,
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
        for tab in &self.tabs {
            tab.persist_draft()?;
        }
        Ok(())
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
}
