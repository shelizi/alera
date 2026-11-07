//! Bounded UTF-8 text editor document, independent of GPUI and Runtime Host.
//!
//! The document is in-memory until explicit save. Saving rechecks workspace
//! containment and the original disk bytes to avoid silent external-change
//! overwrites. IME uses UTF-16 offsets at the GPUI boundary and byte offsets
//! internally, with grapheme-aware cursor movements.
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_EDITOR_BYTES: usize = 256 * 1024;
const MAX_DRAFT_BYTES: u64 = MAX_EDITOR_BYTES as u64 * 2 + 32 * 1024;
const DRAFT_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct EditorDraftStore {
    directory: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedDraft {
    version: u32,
    root: PathBuf,
    path: PathBuf,
    disk_original: String,
    unsaved: String,
}

impl EditorDraftStore {
    pub fn new(state_root: impl AsRef<Path>) -> Self {
        Self {
            directory: state_root.as_ref().join("editor-drafts"),
        }
    }

    fn draft_path(&self, doc: &EditorDocument) -> io::Result<PathBuf> {
        let path = doc.path.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Scratch drafts are not enabled yet",
            )
        })?;
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in doc
            .root
            .to_string_lossy()
            .bytes()
            .chain([0])
            .chain(path.to_string_lossy().bytes())
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Ok(self.directory.join(format!("draft-{hash:016x}.json")))
    }

    /// Store committed unsaved edits only in isolated user application data.
    /// IME preedit is transient and never flushed before composition commit.
    pub fn persist(&self, doc: &EditorDocument) -> io::Result<()> {
        if doc.marked.is_some() {
            return Ok(());
        }
        if !doc.is_dirty() {
            return self.remove(doc);
        }
        let (Some(path), Some(original)) = (doc.path.as_ref(), doc.original.as_ref()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "No file-backed draft",
            ));
        };
        let draft = SavedDraft {
            version: DRAFT_VERSION,
            root: doc.root.clone(),
            path: path.clone(),
            disk_original: original.clone(),
            unsaved: doc.content.clone(),
        };
        let bytes = serde_json::to_vec(&draft)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if bytes.len() as u64 > MAX_DRAFT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Draft too large",
            ));
        }
        fs::create_dir_all(&self.directory)?;
        let destination = self.draft_path(doc)?;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp = destination.with_extension(format!("tmp-{}-{unique}", std::process::id()));
        let result = (|| -> io::Result<()> {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&tmp, &destination)
        })();
        if result.is_err() {
            let _ = fs::remove_file(tmp);
        }
        result
    }

    pub fn restore(&self, doc: &mut EditorDocument) -> io::Result<bool> {
        let path = self.draft_path(doc)?;
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if file.metadata()?.len() > MAX_DRAFT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Draft too large",
            ));
        }
        let mut bytes = Vec::new();
        file.take(MAX_DRAFT_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_DRAFT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Draft too large",
            ));
        }
        let saved: SavedDraft = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if saved.version != DRAFT_VERSION
            || saved.root != doc.root
            || Some(&saved.path) != doc.path.as_ref()
            || saved.disk_original.len() > MAX_EDITOR_BYTES
            || saved.unsaved.len() > MAX_EDITOR_BYTES
            || saved.disk_original.contains('\0')
            || saved.unsaved.contains('\0')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid draft identity/text",
            ));
        }
        // Keep the historical disk snapshot. If the disk changed while
        // Alera was closed, Save will refuse to overwrite its new contents.
        doc.original = Some(saved.disk_original);
        doc.content = saved.unsaved;
        let cursor = doc.content.len();
        doc.selection = cursor..cursor;
        doc.marked = None;
        Ok(true)
    }

    pub fn remove(&self, doc: &EditorDocument) -> io::Result<()> {
        let path = self.draft_path(doc)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug)]
pub struct EditorDocument {
    root: PathBuf,
    path: Option<PathBuf>,
    content: String,
    original: Option<String>,
    selection: Range<usize>,
    marked: Option<Range<usize>>,
}

impl EditorDocument {
    pub fn open(root: impl AsRef<Path>, file: impl AsRef<Path>) -> io::Result<Self> {
        let root = checked_root(root.as_ref())?;
        let path = checked_file(&root, file.as_ref())?;
        let content = load_text(&path)?;
        let cursor = content.len();
        Ok(Self {
            root,
            path: Some(path),
            original: Some(content.clone()),
            content,
            selection: cursor..cursor,
            marked: None,
        })
    }

    /// Scratch content cannot accidentally overwrite a source file.
    pub fn scratch(root: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            root: checked_root(root.as_ref())?,
            path: None,
            content: String::new(),
            original: None,
            selection: 0..0,
            marked: None,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn content(&self) -> &str {
        &self.content
    }
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }
    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }
    pub fn is_dirty(&self) -> bool {
        self.original.as_ref() != Some(&self.content)
    }
    pub fn cursor(&self) -> usize {
        self.selection.end
    }

    pub fn utf16_selection(&self) -> Range<usize> {
        byte_to_utf16(&self.content, self.selection.start)
            ..byte_to_utf16(&self.content, self.selection.end)
    }
    pub fn utf16_marked(&self) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| {
            byte_to_utf16(&self.content, range.start)..byte_to_utf16(&self.content, range.end)
        })
    }
    pub fn text_for_utf16_range(&self, utf16: Range<usize>) -> (String, Range<usize>) {
        let bytes = self.from_utf16_range(utf16);
        (
            self.content[bytes.clone()].to_string(),
            byte_to_utf16(&self.content, bytes.start)..byte_to_utf16(&self.content, bytes.end),
        )
    }

    pub fn set_selection(&mut self, range: Range<usize>) -> io::Result<()> {
        if range.start > range.end
            || range.end > self.content.len()
            || !self.content.is_char_boundary(range.start)
            || !self.content.is_char_boundary(range.end)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Selection is not on UTF-8 character boundaries",
            ));
        }
        self.selection = range;
        Ok(())
    }

    pub fn move_left(&mut self) {
        let caret = self.selection.start;
        let previous = self.content[..caret]
            .grapheme_indices(true)
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.selection = previous..previous;
    }
    pub fn move_right(&mut self) {
        let caret = self.selection.end;
        let next = self.content[caret..]
            .graphemes(true)
            .next()
            .map(|grapheme| caret + grapheme.len())
            .unwrap_or(caret);
        self.selection = next..next;
    }
    pub fn backspace(&mut self) -> io::Result<()> {
        if self.selection.is_empty() {
            let end = self.selection.end;
            self.move_left();
            self.selection.end = end;
        }
        self.replace(None, "")
    }
    pub fn delete_forward(&mut self) -> io::Result<()> {
        if self.selection.is_empty() {
            let start = self.selection.start;
            self.move_right();
            self.selection.start = start;
        }
        self.replace(None, "")
    }

    /// Replace selected text (or a UTF-16 range supplied by the platform).
    pub fn replace(&mut self, utf16: Option<Range<usize>>, text: &str) -> io::Result<()> {
        let range = utf16
            .map(|range| self.from_utf16_range(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        self.apply_replace(range, text)?;
        self.marked = None;
        Ok(())
    }

    /// Replace native IME preedit, preserving composition underlines/cursor.
    /// A composing string is not automatically saved to the source file.
    pub fn replace_and_mark(
        &mut self,
        utf16: Option<Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
    ) -> io::Result<()> {
        let range = utf16
            .map(|range| self.from_utf16_range(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        let start = range.start;
        self.apply_replace(range, text)?;
        self.marked = (!text.is_empty()).then_some(start..start + text.len());
        if let Some(selected) = selected_utf16 {
            let range_start = utf16_to_byte(text, selected.start);
            let range_end = utf16_to_byte(text, selected.end);
            self.set_selection(start + range_start..start + range_end)?;
        }
        Ok(())
    }

    pub fn unmark(&mut self) {
        self.marked = None;
    }

    fn from_utf16_range(&self, range: Range<usize>) -> Range<usize> {
        let start = utf16_to_byte(&self.content, range.start);
        let end = utf16_to_byte(&self.content, range.end);
        start.min(end)..end.max(start)
    }

    fn apply_replace(&mut self, range: Range<usize>, text: &str) -> io::Result<()> {
        let next_size = self.content.len() - (range.end - range.start) + text.len();
        if next_size > MAX_EDITOR_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Editor document exceeds the 256 KiB preview limit",
            ));
        }
        if text.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NUL is not allowed",
            ));
        }
        self.content.replace_range(range.clone(), text);
        let cursor = range.start + text.len();
        self.selection = cursor..cursor;
        Ok(())
    }

    /// Only a saved document can be written. Refuse if the disk changed since
    /// opening, or the original path now resolves outside the workspace.
    pub fn save(&mut self) -> io::Result<()> {
        if self.marked.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Finish IME composition before saving",
            ));
        }
        let Some(path) = self.path.clone() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Scratch document requires Save As",
            ));
        };
        let current = checked_file(&self.root, &path)?;
        if current != path {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Document path changed since opening",
            ));
        }
        let disk = load_text(&path)?;
        if self.original.as_ref() != Some(&disk) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "File changed on disk; reload or resolve conflict",
            ));
        }
        if self.content == disk {
            return Ok(());
        }
        let permissions = fs::metadata(&path)?.permissions();
        if permissions.readonly() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "File is read-only",
            ));
        }
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing parent"))?;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp = parent.join(format!(".alera-gpui-{}-{unique}.tmp", std::process::id()));
        let result = (|| -> io::Result<()> {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            file.set_permissions(permissions)?;
            file.write_all(self.content.as_bytes())?;
            file.sync_all()?;
            // The destination is validated a second time immediately before
            // replacement. This does not make arbitrary concurrent hostile
            // path mutation safe; production file saves need stronger OS APIs.
            if checked_file(&self.root, &path)? != path || load_text(&path)? != disk {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "File changed while saving",
                ));
            }
            fs::rename(&tmp, &path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
            return result;
        }
        self.original = Some(self.content.clone());
        Ok(())
    }
}

fn checked_root(root: &Path) -> io::Result<PathBuf> {
    let root = dunce::canonicalize(root)?;
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Not a workspace directory",
        ));
    }
    Ok(root)
}

fn checked_file(root: &Path, candidate: &Path) -> io::Result<PathBuf> {
    let path = dunce::canonicalize(candidate)?;
    if !path.starts_with(root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "File is outside the workspace",
        ));
    }
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Not a regular file",
        ));
    }
    Ok(path)
}

fn load_text(path: &Path) -> io::Result<String> {
    if fs::metadata(path)?.len() > MAX_EDITOR_BYTES as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "File exceeds 256 KiB",
        ));
    }
    let content = fs::read_to_string(path)?;
    if content.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Binary file"));
    }
    Ok(content)
}

pub fn utf16_to_byte(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units + ch.len_utf16() > offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

pub fn byte_to_utf16(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .unwrap_or("")
        .chars()
        .map(char::len_utf16)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_utf8_conversion_does_not_split_cjk_or_surrogates() {
        let text = "a中文🧑‍💻z";
        let chinese = 1;
        assert_eq!(utf16_to_byte(text, chinese), 1);
        assert_eq!(utf16_to_byte(text, 2), 4);
        assert_eq!(byte_to_utf16(text, 7), 3);
        assert_eq!(utf16_to_byte(text, 4), 7);
        assert_eq!(
            utf16_to_byte(text, 5),
            11,
            "surrogate end advances to next code point"
        );
        assert_eq!(utf16_to_byte(text, 500), text.len());
    }

    #[test]
    fn grapheme_navigation_and_backspace_preserve_combined_characters() {
        let temp = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(temp.path()).unwrap();
        doc.replace(None, "e\u{301}家🧑‍💻").unwrap();
        doc.move_left();
        assert_eq!(doc.cursor(), "e\u{301}家".len());
        doc.backspace().unwrap();
        assert_eq!(doc.content(), "e\u{301}🧑‍💻");
        doc.set_selection(3..3).unwrap();
        doc.backspace().unwrap();
        assert_eq!(doc.content(), "🧑‍💻");
    }

    #[test]
    fn marked_chinese_ime_text_replaces_preedit_without_duplicating() {
        let temp = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(temp.path()).unwrap();
        doc.replace_and_mark(None, "ㄓ", Some(1..1)).unwrap();
        assert!(doc.marked().is_some());
        doc.replace_and_mark(None, "中文", Some(2..2)).unwrap();
        assert_eq!(doc.content(), "中文");
        assert_eq!(doc.utf16_selection(), 2..2);
        doc.replace(None, "中文").unwrap();
        assert_eq!(doc.content(), "中文");
        assert_eq!(doc.marked(), None);
    }

    #[test]
    fn invalid_selection_and_oversized_paste_do_not_mutate_text() {
        let temp = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(temp.path()).unwrap();
        doc.replace(None, "你好").unwrap();
        assert!(doc.set_selection(1..2).is_err());
        assert!(doc.replace(None, &"x".repeat(MAX_EDITOR_BYTES)).is_err());
        assert_eq!(doc.content(), "你好");
    }

    #[test]
    fn existing_file_saves_and_detects_external_conflicts() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file.rs");
        fs::write(&file, "before\r\n").unwrap();
        let mut doc = EditorDocument::open(temp.path(), &file).unwrap();
        doc.replace(None, "after").unwrap();
        doc.save().unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "before\r\nafter");
        assert!(!doc.is_dirty());
        doc.replace(None, " in memory").unwrap();
        fs::write(&file, "updated externally").unwrap();
        assert_eq!(doc.save().unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&file).unwrap(), "updated externally");
        assert!(doc.is_dirty());
    }

    #[test]
    fn scratch_requires_save_as_and_cannot_escape_workspace_root() {
        let inside = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("secret.txt");
        fs::write(&file, "untouched").unwrap();
        assert_eq!(
            EditorDocument::open(inside.path(), &file)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        let mut scratch = EditorDocument::scratch(inside.path()).unwrap();
        scratch.replace(None, "draft").unwrap();
        assert_eq!(
            scratch.save().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "untouched");
    }

    #[test]
    fn active_ime_composition_must_finish_before_disk_save() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("chinese.txt");
        fs::write(&file, "").unwrap();
        let mut doc = EditorDocument::open(temp.path(), &file).unwrap();
        doc.replace_and_mark(None, "ㄎ", Some(1..1)).unwrap();
        assert_eq!(doc.save().unwrap_err().kind(), io::ErrorKind::WouldBlock);
        doc.replace(None, "可").unwrap();
        doc.save().unwrap();
        assert_eq!(fs::read_to_string(file).unwrap(), "可");
    }

    #[test]
    fn draft_recovers_unsaved_text_after_restart_without_changing_source() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let path = workspace.path().join("hello.rs");
        fs::write(&path, "original").unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut editor = EditorDocument::open(workspace.path(), &path).unwrap();
        editor.replace(None, "\n草稿").unwrap();
        store.persist(&editor).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        let mut restarted = EditorDocument::open(workspace.path(), &path).unwrap();
        assert!(store.restore(&mut restarted).unwrap());
        assert_eq!(restarted.content(), "original\n草稿");
        assert!(restarted.is_dirty());
        restarted.save().unwrap();
        store.persist(&restarted).unwrap();
        assert!(!store
            .restore(&mut EditorDocument::open(workspace.path(), &path).unwrap())
            .unwrap());
        assert_eq!(fs::read_to_string(path).unwrap(), "original\n草稿");
    }

    #[test]
    fn restored_draft_remembers_old_disk_content_for_conflict_safety() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let path = workspace.path().join("conflict.rs");
        fs::write(&path, "base").unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut first = EditorDocument::open(workspace.path(), &path).unwrap();
        first.replace(None, "\nlocal").unwrap();
        store.persist(&first).unwrap();
        fs::write(&path, "external").unwrap();
        let mut restarted = EditorDocument::open(workspace.path(), &path).unwrap();
        assert!(store.restore(&mut restarted).unwrap());
        assert_eq!(restarted.content(), "base\nlocal");
        assert_eq!(
            restarted.save().unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "external");
    }

    #[test]
    fn no_draft_is_persisted_until_ime_composition_commits() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let path = workspace.path().join("note.txt");
        fs::write(&path, "pre").unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut editor = EditorDocument::open(workspace.path(), &path).unwrap();
        editor.replace_and_mark(None, "ㄅ", Some(1..1)).unwrap();
        store.persist(&editor).unwrap();
        let mut fresh = EditorDocument::open(workspace.path(), &path).unwrap();
        assert!(!store.restore(&mut fresh).unwrap());
        editor.replace(None, "不").unwrap();
        store.persist(&editor).unwrap();
        assert!(store.restore(&mut fresh).unwrap());
        assert_eq!(fresh.content(), "pre不");
    }

    #[test]
    fn corrupt_and_wrong_document_drafts_are_not_applied() {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let path = workspace.path().join("main.rs");
        fs::write(&path, "safe").unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut doc = EditorDocument::open(workspace.path(), &path).unwrap();
        fs::create_dir_all(&store.directory).unwrap();
        let draft_path = store.draft_path(&doc).unwrap();
        fs::write(&draft_path, "not-json").unwrap();
        assert_eq!(
            store.restore(&mut doc).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(doc.content(), "safe");
        let tampered = SavedDraft {
            version: DRAFT_VERSION,
            root: workspace.path().join("other-root"),
            path: path.clone(),
            disk_original: "safe".into(),
            unsaved: "injected".into(),
        };
        fs::write(&draft_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert_eq!(
            store.restore(&mut doc).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(doc.content(), "safe");
    }
}
