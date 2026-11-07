//! Single native editor for one workspace file.
//!
//! The GPUI window paints and forwards input. Open, save, discard, cancel,
//! and workspace-switch decisions live here so tests can run them without a
//! window. Only one document is installed at a time: a dirty editor refuses
//! another file instead of showing one path and saving another.

use crate::editor::{EditorDocument, EditorDraftStore};
use std::io;
use std::path::Path;

pub const RECOVERY_PLAINTEXT_NOTICE: &str =
    "Recovery drafts are local plaintext on this computer, not secure storage.";
pub const DIRTY_OPEN_REFUSED: &str =
    "Save or explicitly discard current edits before opening another file";
pub const DIRTY_SWITCH_REFUSED: &str =
    "Save or explicitly discard native editor changes before switching workspace";
pub const CANCEL_DIRTY_CLOSE: &str = "Unsaved editor changes: Save or use Discard twice";
pub const DISCARD_ARMED: &str = "Click Confirm discard to lose unsaved editor content";
pub const ALREADY_EDITING: &str = "The selected file is already in the native editor";
pub const EDITOR_OPENED: &str = "Native text editor opened. Click a line to focus and type";
pub const EDITOR_CLOSED: &str = "Native editor closed; read-only preview remains available";

#[derive(Debug, PartialEq, Eq)]
pub enum OpenEffect {
    AlreadyOpen,
    RefusedDirty,
    Opened { recovered: bool },
}

impl OpenEffect {
    pub fn status(&self) -> String {
        match self {
            OpenEffect::AlreadyOpen => ALREADY_EDITING.to_string(),
            OpenEffect::RefusedDirty => DIRTY_OPEN_REFUSED.to_string(),
            OpenEffect::Opened { recovered: true } => format!(
                "Recovered unsaved editor changes; disk file was not changed. {RECOVERY_PLAINTEXT_NOTICE}"
            ),
            OpenEffect::Opened { recovered: false } => EDITOR_OPENED.to_string(),
        }
    }
}

#[derive(Debug)]
pub enum CloseEffect {
    RefusedUnsaved,
    Armed,
    DraftClearFailed(io::Error),
    Closed,
}

impl CloseEffect {
    pub fn status(&self) -> String {
        match self {
            CloseEffect::RefusedUnsaved => CANCEL_DIRTY_CLOSE.to_string(),
            CloseEffect::Armed => DISCARD_ARMED.to_string(),
            CloseEffect::DraftClearFailed(error) => {
                format!("Cannot clear local draft: {error}")
            }
            CloseEffect::Closed => EDITOR_CLOSED.to_string(),
        }
    }

    pub fn closed(&self) -> bool {
        matches!(self, CloseEffect::Closed)
    }
}

#[derive(Debug)]
enum SaveKind {
    Saved,
    Blocked(io::Error),
    SavedDraftRemains(io::Error),
}

#[derive(Debug)]
pub struct SaveCommit {
    kind: SaveKind,
}

impl SaveCommit {
    fn saved() -> Self {
        Self {
            kind: SaveKind::Saved,
        }
    }

    fn blocked_by(error: io::Error) -> Self {
        Self {
            kind: SaveKind::Blocked(error),
        }
    }

    fn saved_draft_remains(error: io::Error) -> Self {
        Self {
            kind: SaveKind::SavedDraftRemains(error),
        }
    }

    pub fn wrote_file(&self) -> bool {
        matches!(self.kind, SaveKind::Saved | SaveKind::SavedDraftRemains(_))
    }

    pub fn blocked(&self) -> bool {
        matches!(self.kind, SaveKind::Blocked(_))
    }

    pub fn status(&self) -> String {
        match &self.kind {
            SaveKind::Saved => "File saved; reopen preview to see latest bytes".to_string(),
            SaveKind::SavedDraftRemains(error) => {
                format!("File saved; draft cleanup failed: {error}. {RECOVERY_PLAINTEXT_NOTICE}")
            }
            SaveKind::Blocked(error) => {
                format!("Save blocked: {error}. {RECOVERY_PLAINTEXT_NOTICE}")
            }
        }
    }
}

/// The only editable document. Preview tabs stay outside this type.
#[derive(Debug)]
pub struct EditorSession {
    document: Option<EditorDocument>,
    drafts: Option<EditorDraftStore>,
    discard_armed: bool,
}

impl EditorSession {
    pub fn new(drafts: Option<EditorDraftStore>) -> Self {
        Self {
            document: None,
            drafts,
            discard_armed: false,
        }
    }

    pub fn document(&self) -> Option<&EditorDocument> {
        self.document.as_ref()
    }

    pub fn document_mut(&mut self) -> Option<&mut EditorDocument> {
        self.document.as_mut()
    }

    pub fn discard_armed(&self) -> bool {
        self.discard_armed
    }

    pub fn has_draft_store(&self) -> bool {
        self.drafts.is_some()
    }

    pub fn target_path(&self) -> Option<&Path> {
        self.document.as_ref().and_then(EditorDocument::path)
    }

    pub fn workspace_root(&self) -> Option<&Path> {
        self.document.as_ref().map(EditorDocument::root)
    }

    /// Install `path` as the only editor, or leave the current one untouched.
    pub fn open_file(&mut self, root: &Path, path: &Path) -> io::Result<OpenEffect> {
        if let Some(document) = &self.document {
            if document.path() == Some(path) {
                return Ok(OpenEffect::AlreadyOpen);
            }
            if document.is_dirty() {
                return Ok(OpenEffect::RefusedDirty);
            }
        }
        let mut document = EditorDocument::open(root, path)?;
        let mut recovered = false;
        if let Some(store) = &self.drafts {
            recovered = store.restore(&mut document)?;
        }
        self.document = Some(document);
        self.discard_armed = false;
        Ok(OpenEffect::Opened { recovered })
    }

    pub fn save(&mut self) -> SaveCommit {
        let Some(document) = self.document.as_mut() else {
            return SaveCommit::blocked_by(io::Error::new(
                io::ErrorKind::InvalidInput,
                "No editor is open",
            ));
        };
        if let Err(error) = document.save() {
            return SaveCommit::blocked_by(error);
        }
        if let Some(store) = &self.drafts {
            if let Err(error) = store.remove(document) {
                return SaveCommit::saved_draft_remains(error);
            }
        }
        self.discard_armed = false;
        SaveCommit::saved()
    }

    /// Save the open buffer to `destination`.
    /// The current file is left unchanged when the destination is different.
    /// `overwrite` must already be the user's confirmed replacement.
    pub fn save_as(&mut self, destination: &Path, overwrite: bool) -> SaveCommit {
        let Some(document) = self.document.as_mut() else {
            return SaveCommit::blocked_by(io::Error::new(
                io::ErrorKind::InvalidInput,
                "No editor is open",
            ));
        };
        let previous = document.path().map(Path::to_path_buf);
        let root = document.root().to_path_buf();
        if let Err(error) = document.save_as(destination, overwrite) {
            return SaveCommit::blocked_by(error);
        }
        if let Some(store) = &self.drafts {
            if let Some(old) = previous.as_ref() {
                if document.path() != Some(old.as_path()) {
                    if let Err(error) = store.remove_identity(&root, old) {
                        return SaveCommit::saved_draft_remains(error);
                    }
                }
            }
            if let Err(error) = store.remove(document) {
                return SaveCommit::saved_draft_remains(error);
            }
        }
        self.discard_armed = false;
        SaveCommit::saved()
    }

    /// Flush committed text only. Marked IME preedit is omitted by the store.
    pub fn persist_draft(&self) -> io::Result<()> {
        let (Some(document), Some(store)) = (&self.document, &self.drafts) else {
            return Ok(());
        };
        store.persist(document)
    }

    pub fn cancel(&mut self) {
        self.discard_armed = false;
    }

    pub fn close(&mut self, discard: bool) -> CloseEffect {
        let dirty = self.document.as_ref().is_some_and(EditorDocument::is_dirty);
        if dirty {
            if !discard {
                self.cancel();
                return CloseEffect::RefusedUnsaved;
            }
            if !self.discard_armed {
                self.discard_armed = true;
                return CloseEffect::Armed;
            }
            if let Some(document) = &self.document {
                if let Some(store) = &self.drafts {
                    if let Err(error) = store.remove(document) {
                        return CloseEffect::DraftClearFailed(error);
                    }
                }
            }
        }
        self.document = None;
        self.discard_armed = false;
        CloseEffect::Closed
    }

    pub fn can_switch_workspace(&self) -> Result<(), &'static str> {
        if self.document.as_ref().is_some_and(EditorDocument::is_dirty) {
            Err(DIRTY_SWITCH_REFUSED)
        } else {
            Ok(())
        }
    }
}
