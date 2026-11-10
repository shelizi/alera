//! Local, bounded scratch recovery. No workspace file is created here.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_SCRATCH_DRAFTS: usize = 24;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn new_identity() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{timestamp:032x}{:08x}{:016x}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn valid_identity(id: &str) -> bool {
    id.len() == 56 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Serialize, Deserialize)]
struct SavedScratch {
    version: u32,
    root: PathBuf,
    id: String,
    unsaved: String,
}

impl EditorDraftStore {
    fn scratch_directory(&self, root: &Path) -> PathBuf {
        self.identity_path(root, Path::new("scratch"))
            .with_extension("scratch")
    }

    // Refuse redirected recovery directories rather than writing through them.
    fn check_scratch_directory(&self, root: &Path) -> io::Result<PathBuf> {
        let directory = self.scratch_directory(root);
        for path in [&self.directory, &directory] {
            match fs::symlink_metadata(path) {
                Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                    return Err(invalid("Invalid scratch recovery directory"));
                }
                Ok(_) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
        }
        Ok(directory)
    }

    pub(super) fn scratch_path(&self, doc: &EditorDocument) -> io::Result<PathBuf> {
        let id = doc
            .scratch_id
            .as_deref()
            .filter(|id| valid_identity(id))
            .ok_or_else(|| invalid("Invalid scratch identity"))?;
        Ok(self
            .check_scratch_directory(&doc.root)?
            .join(format!("{id}.json")))
    }

    fn scratch_entries(&self, root: &Path) -> io::Result<Vec<PathBuf>> {
        let directory = self.check_scratch_directory(root)?;
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                if !entry.file_type()?.is_file() {
                    return Err(invalid("Invalid scratch draft file"));
                }
                paths.push(path);
                if paths.len() > MAX_SCRATCH_DRAFTS {
                    return Err(invalid("Scratch recovery limit reached"));
                }
            }
        }
        paths.sort();
        Ok(paths)
    }

    pub(super) fn persist_scratch(&self, doc: &EditorDocument) -> io::Result<()> {
        let destination = self.scratch_path(doc)?;
        let entries = self.scratch_entries(&doc.root)?;
        if !entries.contains(&destination) && entries.len() >= MAX_SCRATCH_DRAFTS {
            return Err(invalid("Scratch recovery limit reached"));
        }
        let draft = SavedScratch {
            version: DRAFT_VERSION,
            root: doc.root.clone(),
            id: doc
                .scratch_id
                .clone()
                .ok_or_else(|| invalid("Missing scratch identity"))?,
            unsaved: doc.content.clone(),
        };
        let bytes = serde_json::to_vec(&draft).map_err(|error| invalid(&error.to_string()))?;
        if bytes.len() as u64 > MAX_DRAFT_BYTES {
            return Err(invalid("Draft too large"));
        }
        self.write_draft_bytes(&destination, &bytes)
    }

    fn read_scratch(&self, root: &Path, path: &Path) -> io::Result<EditorDocument> {
        if !fs::symlink_metadata(path)?.file_type().is_file() {
            return Err(invalid("Invalid scratch draft file"));
        }
        let file = fs::File::open(path)?;
        if file.metadata()?.len() > MAX_DRAFT_BYTES {
            return Err(invalid("Draft too large"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_DRAFT_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_DRAFT_BYTES {
            return Err(invalid("Draft too large"));
        }
        let saved: SavedScratch =
            serde_json::from_slice(&bytes).map_err(|error| invalid(&error.to_string()))?;
        if saved.version != DRAFT_VERSION
            || saved.root != root
            || !valid_identity(&saved.id)
            || path.file_name() != Some(std::ffi::OsStr::new(&format!("{}.json", saved.id)))
            || saved.unsaved.len() > MAX_EDITOR_BYTES
            || saved.unsaved.contains('\0')
        {
            return Err(invalid("Invalid scratch draft identity/text"));
        }
        let mut document = EditorDocument::scratch(root)?;
        document.scratch_id = Some(saved.id);
        document.content = saved.unsaved;
        let cursor = document.content.len();
        document.selection = cursor..cursor;
        Ok(document)
    }

    /// Discover unsaved scratch buffers for exactly this canonical workspace.
    /// All records are validated before returning; failures leave disk untouched.
    /// Text is local plaintext, subject to the editor byte and 24 draft limits.
    pub fn restore_scratch(&self, root: &Path) -> io::Result<Vec<EditorDocument>> {
        let root = checked_root(root)?;
        self.scratch_entries(&root)?
            .iter()
            .map(|path| self.read_scratch(&root, path))
            .collect()
    }

    pub(super) fn restore_scratch_document(&self, doc: &mut EditorDocument) -> io::Result<bool> {
        let path = self.scratch_path(doc)?;
        match self.read_scratch(&doc.root, &path) {
            Ok(restored) => {
                *doc = restored;
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(super) fn remove_scratch(&self, doc: &EditorDocument) -> io::Result<()> {
        if doc.scratch_id.is_none() {
            return Ok(());
        }
        match fs::remove_file(self.scratch_path(doc)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_session::{CloseEffect, EditorSession};

    #[test]
    fn invalid_scratch_records_never_change_workspace_or_existing_tabs() {
        let workspace = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut doc = EditorDocument::scratch(workspace.path()).unwrap();
        doc.replace(None, "safe").unwrap();
        store.persist(&doc).unwrap();
        let path = store.scratch_path(&doc).unwrap();
        let original = fs::read(&path).unwrap();
        let mut tabs = crate::editor_tabs::EditorTabs::new(Some(store.clone()));
        tabs.open_scratch(workspace.path()).unwrap();
        tabs.document_mut().unwrap().replace(None, "live").unwrap();
        for corruption in [
            "not-json".to_string(),
            " ".repeat(MAX_DRAFT_BYTES as usize + 1),
        ] {
            fs::write(&path, corruption).unwrap();
            assert!(tabs.restore_scratch_drafts(workspace.path()).is_err());
            assert_eq!(tabs.tab_labels().len(), 1);
            assert_eq!(tabs.document().unwrap().content(), "live");
        }
        for (key, value) in [
            (
                "root",
                serde_json::to_value(checked_root(other.path()).unwrap()).unwrap(),
            ),
            ("id", serde_json::json!("../../escaped")),
            ("unsaved", serde_json::json!("bad\0text")),
        ] {
            let mut record: serde_json::Value = serde_json::from_slice(&original).unwrap();
            record[key] = value;
            fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
            assert!(store.restore_scratch(workspace.path()).is_err());
        }
        fs::write(&path, &original).unwrap();
        assert_eq!(
            store.restore_scratch(workspace.path()).unwrap()[0].content(),
            "safe"
        );
        assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 0);
        assert_eq!(fs::read_dir(other.path()).unwrap().count(), 0);
    }

    #[test]
    fn failed_persist_preserves_previous_draft_and_ime_is_never_persisted() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut doc = EditorDocument::scratch(workspace.path()).unwrap();
        doc.replace(None, "committed").unwrap();
        store.persist(&doc).unwrap();
        doc.replace_and_mark(None, "ㄅ", Some(1..1)).unwrap();
        store.persist(&doc).unwrap();
        assert_eq!(
            store.restore_scratch(workspace.path()).unwrap()[0].content(),
            "committed"
        );
        doc.unmark();
        let end = doc.content().len();
        doc.replace(Some(0..end), &"\u{1}".repeat(MAX_EDITOR_BYTES))
            .unwrap();
        assert!(store.persist(&doc).is_err());
        assert_eq!(
            store.restore_scratch(workspace.path()).unwrap()[0].content(),
            "committed"
        );
        assert_eq!(doc.content().len(), MAX_EDITOR_BYTES);
    }

    #[test]
    fn cleanup_failure_keeps_tab_open_and_save_as_reports_written_file() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut session = EditorSession::new(Some(store.clone()));
        session.open_scratch(workspace.path()).unwrap();
        session
            .document_mut()
            .unwrap()
            .replace(None, "recover")
            .unwrap();
        session.persist_draft().unwrap();
        let path = store.scratch_path(session.document().unwrap()).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(matches!(session.close(true), CloseEffect::Armed));
        assert!(matches!(
            session.close(true),
            CloseEffect::DraftClearFailed(_)
        ));
        assert_eq!(session.document().unwrap().content(), "recover");
        let saved = workspace.path().join("saved.txt");
        let outcome = session.save_as(&saved, false);
        assert!(outcome.wrote_file() && outcome.status().contains("cleanup failed"));
        assert_eq!(fs::read_to_string(saved).unwrap(), "recover");
        assert!(matches!(
            session.close(false),
            CloseEffect::DraftClearFailed(_)
        ));
        fs::remove_dir(path).unwrap();
        assert!(session.close(false).closed());
    }

    #[test]
    fn scratch_count_is_bounded_and_existing_drafts_remain_updatable() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut documents = Vec::new();
        for _ in 0..MAX_SCRATCH_DRAFTS {
            let doc = EditorDocument::scratch(workspace.path()).unwrap();
            store.persist(&doc).unwrap();
            documents.push(doc);
        }
        let extra = EditorDocument::scratch(workspace.path()).unwrap();
        assert!(store.persist(&extra).is_err());
        documents[0].replace(None, "updated").unwrap();
        store.persist(&documents[0]).unwrap();
        assert_eq!(
            store.restore_scratch(workspace.path()).unwrap().len(),
            MAX_SCRATCH_DRAFTS
        );
        let mut tabs = crate::editor_tabs::EditorTabs::new(Some(store.clone()));
        tabs.open_scratch(workspace.path()).unwrap();
        assert!(tabs.restore_scratch_drafts(workspace.path()).is_err());
        assert_eq!(tabs.tab_labels().len(), 1);
        store.remove(&documents[0]).unwrap();
        store.persist(&extra).unwrap();
    }
}
