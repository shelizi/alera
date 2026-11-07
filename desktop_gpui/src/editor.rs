//! Bounded UTF-8 text editor document, independent of GPUI and Runtime Host.
//!
//! The document is in-memory until explicit save. Saving rechecks workspace
//! containment and the original disk bytes to avoid silent external-change
//! overwrites. IME uses UTF-16 offsets at the GPUI boundary and byte offsets
//! internally, with grapheme-aware cursor movements.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_EDITOR_BYTES: usize = 256 * 1024;

/// Byte-oriented line offsets used by the native virtualized viewport.
///
/// Count is always at least one. A trailing newline has its own empty line.
/// Rebuild after an edit/undo/IME update; the 256 KiB document cap keeps this
/// index bounded without storing full text or decoding every rendered row.
#[derive(Clone, Debug)]
pub struct EditorLineIndex {
    starts: Vec<usize>,
}

impl EditorLineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = Vec::with_capacity(text.bytes().filter(|byte| *byte == b'\n').count() + 1);
        starts.push(0);
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        Self { starts }
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// Returns the UTF-8 byte range for a line, excluding its newline.
    pub fn line_range(&self, text: &str, line: usize) -> Option<Range<usize>> {
        let start = *self.starts.get(line)?;
        let end = match self.starts.get(line + 1) {
            Some(next) => next - 1,
            None => text.len(),
        };
        (start <= end
            && end <= text.len()
            && text.is_char_boundary(start)
            && text.is_char_boundary(end))
        .then_some(start..end)
    }

    pub fn line_for_offset(&self, text: &str, offset: usize) -> Option<usize> {
        (offset <= text.len() && text.is_char_boundary(offset)).then(|| {
            self.starts
                .partition_point(|start| *start <= offset)
                .saturating_sub(1)
        })
    }
}
const MAX_HISTORY_ENTRIES: usize = 64;
const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;
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
        doc.anchor = None;
        doc.desired_column = None;
        doc.marked = None;
        doc.undo_stack.clear();
        doc.redo_stack.clear();
        doc.composition_start = None;
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
    /// Anchor is set only while extending a selection. The caret may be at
    /// either end (Windows TSF needs this orientation in UTF-16 units).
    anchor: Option<usize>,
    /// Sticky visual grapheme column for consecutive Up/Down movements.
    desired_column: Option<usize>,
    marked: Option<Range<usize>>,
    undo_stack: VecDeque<EditorSnapshot>,
    redo_stack: VecDeque<EditorSnapshot>,
    /// Captured only at the beginning of an IME preedit, so all updates
    /// and the final committed text form one undoable transaction.
    composition_start: Option<EditorSnapshot>,
}

#[derive(Clone, Debug)]
struct EditorSnapshot {
    text: String,
    selection: Range<usize>,
    anchor: Option<usize>,
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
            anchor: None,
            desired_column: None,
            marked: None,
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            composition_start: None,
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
            anchor: None,
            desired_column: None,
            marked: None,
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            composition_start: None,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn content(&self) -> &str {
        &self.content
    }
    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// Bytes a copy command would place on the clipboard.
    /// The range stays on the selection the document already normalized,
    /// which callers keep on grapheme boundaries.
    pub fn copied_text(&self) -> &str {
        self.content
            .get(self.selection.start..self.selection.end)
            .unwrap_or("")
    }
    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }
    pub fn is_dirty(&self) -> bool {
        self.original.as_ref() != Some(&self.content)
    }
    pub fn cursor(&self) -> usize {
        match self.anchor {
            Some(anchor) if anchor == self.selection.end => self.selection.start,
            _ => self.selection.end,
        }
    }

    pub fn selection_reversed(&self) -> bool {
        !self.selection.is_empty() && self.anchor == Some(self.selection.end)
    }

    pub fn history_depth(&self) -> (usize, usize) {
        (self.undo_stack.len(), self.redo_stack.len())
    }

    pub fn history_bytes(&self) -> usize {
        self.undo_stack
            .iter()
            .chain(self.redo_stack.iter())
            .map(|entry| entry.text.len())
            .sum()
    }

    fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            text: self.content.clone(),
            selection: self.selection.clone(),
            anchor: self.anchor,
        }
    }

    fn restore_snapshot(&mut self, snapshot: EditorSnapshot) {
        self.content = snapshot.text;
        self.selection = snapshot.selection;
        self.anchor = snapshot.anchor;
        self.desired_column = None;
        self.marked = None;
        self.composition_start = None;
    }

    fn trim_history(&mut self) {
        while self.undo_stack.len() + self.redo_stack.len() > MAX_HISTORY_ENTRIES
            || self.history_bytes() > MAX_HISTORY_BYTES
        {
            if self.undo_stack.pop_front().is_none() {
                self.redo_stack.pop_front();
            }
        }
    }

    fn record_edit(&mut self, before: EditorSnapshot) {
        if before.text != self.content {
            self.undo_stack.push_back(before);
            self.redo_stack.clear();
            self.trim_history();
        }
    }

    pub fn undo(&mut self) -> bool {
        if self.marked.is_some() {
            if let Some(original) = self.composition_start.take() {
                self.restore_snapshot(original);
                return true;
            }
        }
        let Some(before) = self.undo_stack.pop_back() else {
            return false;
        };
        self.redo_stack.push_back(self.snapshot());
        self.restore_snapshot(before);
        self.trim_history();
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.marked.is_some() {
            return false;
        }
        let Some(after) = self.redo_stack.pop_back() else {
            return false;
        };
        self.undo_stack.push_back(self.snapshot());
        self.restore_snapshot(after);
        self.trim_history();
        true
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
        self.anchor = None;
        self.desired_column = None;
        Ok(())
    }

    pub fn select_offset(&mut self, offset: usize) -> io::Result<()> {
        if offset > self.content.len() || !self.content.is_char_boundary(offset) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot select invalid character offset",
            ));
        }
        if self.marked.is_none() {
            self.move_to(offset, true, true);
        }
        Ok(())
    }

    /// Pointer caret positions must be complete Unicode grapheme boundaries.
    pub fn set_pointer_caret(&mut self, offset: usize) -> io::Result<()> {
        self.validate_pointer_offset(offset)?;
        self.move_to(offset, false, true);
        Ok(())
    }

    pub fn extend_pointer_selection(&mut self, offset: usize) -> io::Result<()> {
        self.validate_pointer_offset(offset)?;
        self.move_to(offset, true, true);
        Ok(())
    }

    fn validate_pointer_offset(&self, offset: usize) -> io::Result<()> {
        if self.marked.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Finish native IME composition before mouse selection",
            ));
        }
        if offset > self.content.len()
            || (offset != self.content.len()
                && !self
                    .content
                    .grapheme_indices(true)
                    .any(|(boundary, _)| boundary == offset))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Mouse caret must be on a Unicode grapheme boundary",
            ));
        }
        Ok(())
    }

    fn move_to(&mut self, offset: usize, extend: bool, clear_column: bool) {
        if extend {
            let caret = self.cursor();
            let anchor = *self.anchor.get_or_insert(caret);
            self.selection = anchor.min(offset)..anchor.max(offset);
        } else {
            self.selection = offset..offset;
            self.anchor = None;
        }
        if clear_column {
            self.desired_column = None;
        }
    }

    fn previous_grapheme(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(i, _)| (i < offset).then_some(i))
            .unwrap_or(0)
    }

    fn next_grapheme(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(i, _)| (i > offset).then_some(i))
            .unwrap_or(self.content.len())
    }

    pub fn move_left(&mut self) {
        if self.marked.is_some() {
            return;
        }
        let position = if self.selection.is_empty() {
            self.previous_grapheme(self.cursor())
        } else {
            self.selection.start
        };
        self.move_to(position, false, true);
    }
    pub fn move_right(&mut self) {
        if self.marked.is_some() {
            return;
        }
        let position = if self.selection.is_empty() {
            self.next_grapheme(self.cursor())
        } else {
            self.selection.end
        };
        self.move_to(position, false, true);
    }

    pub fn select_left(&mut self) {
        if self.marked.is_none() {
            let position = self.previous_grapheme(self.cursor());
            self.move_to(position, true, true);
        }
    }

    pub fn select_right(&mut self) {
        if self.marked.is_none() {
            let position = self.next_grapheme(self.cursor());
            self.move_to(position, true, true);
        }
    }

    fn line_bounds(&self, offset: usize) -> (usize, usize) {
        let start = self.content[..offset].rfind('\n').map_or(0, |i| i + 1);
        let end = self.content[offset..]
            .find('\n')
            .map_or(self.content.len(), |i| offset + i);
        (start, end)
    }

    pub fn line_column(&self) -> (usize, usize) {
        let offset = self.cursor();
        let line = self.content[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        let start = self.line_bounds(offset).0;
        let column = self.content[start..offset].graphemes(true).count();
        (line, column)
    }

    fn offset_at_column(&self, line_start: usize, line_end: usize, col: usize) -> usize {
        line_start
            + self.content[line_start..line_end]
                .graphemes(true)
                .take(col)
                .map(str::len)
                .sum::<usize>()
    }

    pub fn move_up(&mut self, extend: bool) {
        if self.marked.is_some() {
            return;
        }
        let caret = self.cursor();
        let (start, _) = self.line_bounds(caret);
        if start == 0 {
            return;
        }
        let column = self.desired_column.unwrap_or_else(|| self.line_column().1);
        let previous_end = start - 1;
        let previous_start = self.line_bounds(previous_end).0;
        let target = self.offset_at_column(previous_start, previous_end, column);
        self.move_to(target, extend, false);
        self.desired_column = Some(column);
    }

    pub fn move_down(&mut self, extend: bool) {
        if self.marked.is_some() {
            return;
        }
        let caret = self.cursor();
        let (_, end) = self.line_bounds(caret);
        if end == self.content.len() {
            return;
        }
        let column = self.desired_column.unwrap_or_else(|| self.line_column().1);
        let next_start = end + 1;
        let next_end = self.line_bounds(next_start).1;
        let target = self.offset_at_column(next_start, next_end, column);
        self.move_to(target, extend, false);
        self.desired_column = Some(column);
    }

    pub fn move_home(&mut self, extend: bool) {
        if self.marked.is_none() {
            let start = self.line_bounds(self.cursor()).0;
            self.move_to(start, extend, true);
        }
    }

    pub fn move_end(&mut self, extend: bool) {
        if self.marked.is_none() {
            let end = self.line_bounds(self.cursor()).1;
            self.move_to(end, extend, true);
        }
    }

    /// Go to a document boundary without converting the entire document to
    /// UTF-16. Ctrl/Cmd+Home/End remains usable with thousands of rows.
    pub fn move_document_start(&mut self, extend: bool) {
        if self.marked.is_none() {
            self.move_to(0, extend, true);
        }
    }

    pub fn move_document_end(&mut self, extend: bool) {
        if self.marked.is_none() {
            self.move_to(self.content.len(), extend, true);
        }
    }

    pub fn backspace(&mut self) -> io::Result<()> {
        if self.marked.is_some() {
            return Ok(());
        }
        if self.selection.is_empty() {
            let end = self.selection.end;
            self.move_left();
            self.selection.end = end;
        }
        self.replace(None, "")
    }
    pub fn delete_forward(&mut self) -> io::Result<()> {
        if self.marked.is_some() {
            return Ok(());
        }
        if self.selection.is_empty() {
            let start = self.selection.start;
            self.move_right();
            self.selection.start = start;
        }
        self.replace(None, "")
    }

    /// Replace selected text (or a UTF-16 range supplied by the platform).
    pub fn replace(&mut self, utf16: Option<Range<usize>>, text: &str) -> io::Result<()> {
        let before = self
            .composition_start
            .clone()
            .unwrap_or_else(|| self.snapshot());
        let range = utf16
            .map(|range| self.from_utf16_range(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        self.apply_replace(range, text)?;
        self.marked = None;
        self.composition_start = None;
        self.record_edit(before);
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
        if selected_utf16
            .as_ref()
            .is_some_and(|range| range.start > range.end)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "IME selection range is reversed",
            ));
        }
        let before = self
            .composition_start
            .clone()
            .unwrap_or_else(|| self.snapshot());
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
        if self.marked.is_some() {
            self.composition_start = Some(before);
        } else {
            self.composition_start = None;
            self.record_edit(before);
        }
        Ok(())
    }

    pub fn unmark(&mut self) {
        self.marked = None;
        if let Some(before) = self.composition_start.take() {
            self.record_edit(before);
        }
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
        self.anchor = None;
        self.desired_column = None;
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

/// Snap a shaped-glyph index to the closest complete grapheme boundary.
/// The shaping engine can return indices inside ZWJ emoji or combining text.
/// Consult only the two neighboring grapheme edges' pixel positions.
pub fn nearest_grapheme_edge(
    text: &str,
    glyph_index: usize,
    x: f32,
    mut x_for_index: impl FnMut(usize) -> f32,
) -> usize {
    let index = glyph_index.min(text.len());
    let mut previous = 0;
    for (boundary, _) in text.grapheme_indices(true) {
        if boundary == index {
            return boundary;
        }
        if boundary > index {
            let before = (x_for_index(previous) - x).abs();
            let after = (x_for_index(boundary) - x).abs();
            return if before <= after { previous } else { boundary };
        }
        previous = boundary;
    }
    if index == text.len() {
        return index;
    }
    let before = (x_for_index(previous) - x).abs();
    let after = (x_for_index(text.len()) - x).abs();
    if before <= after {
        previous
    } else {
        text.len()
    }
}

/// Next vertical GPUI ScrollHandle offset while dragging near the viewport
/// edge. Negative offsets mean scrolling down. This helper has no timer or
/// global input state: the UI owns a cancellable 40ms tick only during drag.
pub fn edge_scroll_offset(
    pointer_y: f32,
    view_top: f32,
    view_bottom: f32,
    current_offset: f32,
    max_scroll: f32,
) -> Option<f32> {
    const EDGE_MARGIN: f32 = 24.0;
    const STEP: f32 = 19.0;
    if ![pointer_y, view_top, view_bottom, current_offset, max_scroll]
        .into_iter()
        .all(f32::is_finite)
        || view_bottom - view_top <= EDGE_MARGIN * 2.0
        || max_scroll <= 0.0
    {
        return None;
    }
    let offset = current_offset.clamp(-max_scroll, 0.0);
    let next = if pointer_y <= view_top + EDGE_MARGIN {
        (offset + STEP).min(0.0)
    } else if pointer_y >= view_bottom - EDGE_MARGIN {
        (offset - STEP).max(-max_scroll)
    } else {
        return None;
    };
    (next - offset).abs().gt(&0.5).then_some(next)
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

    #[test]
    fn undo_redo_restore_exact_unicode_content_selection_and_dirty_status() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("note.txt");
        fs::write(&file, "基礎").unwrap();
        let mut doc = EditorDocument::open(root.path(), &file).unwrap();
        doc.replace(None, "🧑‍💻").unwrap();
        assert!(doc.is_dirty());
        assert!(doc.undo());
        assert_eq!(doc.content(), "基礎");
        assert!(!doc.is_dirty());
        assert_eq!(doc.cursor(), "基礎".len());
        assert!(doc.redo());
        assert_eq!(doc.content(), "基礎🧑‍💻");
        assert!(doc.is_dirty());
        assert!(!doc.redo());
        assert!(doc.undo());
        doc.replace(None, "新").unwrap();
        assert!(!doc.redo(), "new edit must invalidate redo");
        assert_eq!(doc.content(), "基礎新");
    }

    #[test]
    fn ime_preedit_and_commit_are_one_undo_step_and_cancellation_is_noop() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "hello").unwrap();
        let before = doc.history_depth();
        doc.replace_and_mark(None, "ㄓ", Some(1..1)).unwrap();
        doc.replace_and_mark(None, "中", Some(1..1)).unwrap();
        doc.replace(None, "中文").unwrap();
        assert_eq!(doc.history_depth().0, before.0 + 1);
        assert_eq!(doc.content(), "hello中文");
        assert!(doc.undo());
        assert_eq!(doc.content(), "hello");
        assert!(doc.redo());
        assert_eq!(doc.content(), "hello中文");
        let prior = doc.history_depth();
        doc.replace_and_mark(None, "ㄅ", Some(1..1)).unwrap();
        doc.replace(None, "").unwrap();
        assert_eq!(doc.content(), "hello中文");
        assert_eq!(
            doc.history_depth(),
            prior,
            "canceled composition cannot add history"
        );
    }

    #[test]
    fn reversed_shift_selection_across_graphemes_has_native_utf16_range() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "a🧑‍💻中文").unwrap();
        doc.set_selection(doc.content().len()..doc.content().len())
            .unwrap();
        doc.select_left();
        doc.select_left();
        assert_eq!(&doc.content()[doc.selection()], "中文");
        assert!(doc.selection_reversed());
        assert_eq!(doc.utf16_selection(), 6..8);
        doc.select_right();
        assert_eq!(&doc.content()[doc.selection()], "文");
        doc.move_left();
        assert_eq!(doc.selection().start, "a🧑‍💻中".len());
        assert!(doc.selection().is_empty());
    }

    #[test]
    fn vertical_navigation_retains_grapheme_column_through_short_lines() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "a🧑‍💻c\n中\n12345").unwrap();
        doc.set_selection("a🧑‍💻c".len().."a🧑‍💻c".len()).unwrap();
        assert_eq!(doc.line_column(), (0, 3));
        doc.move_down(false);
        assert_eq!(doc.line_column(), (1, 1));
        doc.move_down(false);
        assert_eq!(doc.line_column(), (2, 3));
        doc.move_up(false);
        assert_eq!(doc.line_column(), (1, 1));
        doc.move_up(false);
        assert_eq!(doc.line_column(), (0, 3));
        doc.move_home(false);
        assert_eq!(doc.line_column(), (0, 0));
        doc.move_end(false);
        assert_eq!(doc.line_column(), (0, 3));
    }

    #[test]
    fn shift_up_down_extends_multiline_selection_and_replaces_selected_text() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "one\n二\nthree").unwrap();
        doc.set_selection(0..0).unwrap();
        doc.move_down(true);
        assert_eq!(&doc.content()[doc.selection()], "one\n");
        doc.move_down(true);
        assert_eq!(&doc.content()[doc.selection()], "one\n二\n");
        doc.replace(None, "new\n").unwrap();
        assert_eq!(doc.content(), "new\nthree");
        assert!(doc.undo());
        assert_eq!(doc.content(), "one\n二\nthree");
    }

    #[test]
    fn bounded_history_evicts_old_snapshots_without_exceeding_budget() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        for _ in 0..100 {
            doc.replace(None, "x").unwrap();
        }
        let (undos, redos) = doc.history_depth();
        assert!(undos <= 64 && undos > 0);
        assert_eq!(redos, 0);
        assert!(doc.history_bytes() <= 4 * 1024 * 1024);
        for _ in 0..undos {
            assert!(doc.undo());
        }
        assert!(!doc.undo());
        assert_eq!(doc.history_depth().0, 0);
    }

    #[test]
    fn failed_paste_does_not_erase_selection_or_undo_history() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "🥐").unwrap();
        let baseline = doc.history_depth();
        doc.set_selection(0..doc.content().len()).unwrap();
        assert!(doc
            .replace(None, &"x".repeat(MAX_EDITOR_BYTES + 1))
            .is_err());
        assert_eq!(doc.content(), "🥐");
        assert_eq!(doc.selection(), 0.."🥐".len());
        assert_eq!(doc.history_depth(), baseline);
    }

    #[test]
    fn replacing_reversed_selection_resets_anchor_before_next_shift_select() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "abcde").unwrap();
        doc.select_left();
        doc.select_left();
        assert!(doc.selection_reversed());
        assert_eq!(&doc.content()[doc.selection()], "de");
        doc.replace(None, "🧪").unwrap();
        assert_eq!(doc.content(), "abc🧪");
        assert_eq!(doc.cursor(), doc.content().len());
        doc.select_left();
        assert_eq!(&doc.content()[doc.selection()], "🧪");
        assert!(doc.selection_reversed());
        assert!(doc.undo());
        assert_eq!(doc.content(), "abcde");
        assert_eq!(&doc.content()[doc.selection()], "de");
    }

    #[test]
    fn ime_unmark_commits_one_undo_step_and_recovery_draft() {
        let root = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let file = root.path().join("ime.txt");
        fs::write(&file, "").unwrap();
        let store = EditorDraftStore::new(state.path());
        let mut doc = EditorDocument::open(root.path(), &file).unwrap();
        doc.replace_and_mark(None, "ㄅ", Some(1..1)).unwrap();
        doc.replace_and_mark(None, "不", Some(1..1)).unwrap();
        store.persist(&doc).unwrap();
        let mut fresh = EditorDocument::open(root.path(), &file).unwrap();
        assert!(!store.restore(&mut fresh).unwrap());
        doc.unmark();
        assert_eq!(doc.history_depth(), (1, 0));
        store.persist(&doc).unwrap();
        assert!(store.restore(&mut fresh).unwrap());
        assert_eq!(fresh.content(), "不");
        assert!(doc.undo());
        assert_eq!(doc.content(), "");
        assert!(doc.redo());
        assert_eq!(doc.content(), "不");
    }

    #[test]
    fn select_offset_can_extend_multiline_selection_without_splitting_unicode() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "甲\n🧑‍💻乙").unwrap();
        doc.set_selection(0..0).unwrap();
        doc.select_offset("甲\n🧑‍💻".len()).unwrap();
        assert_eq!(&doc.content()[doc.selection()], "甲\n🧑‍💻");
        assert!(!doc.selection_reversed());
        assert_eq!(doc.cursor(), "甲\n🧑‍💻".len());
        let previous = doc.selection();
        assert!(
            doc.select_offset(5).is_err(),
            "mid-emoji UTF-8 offset must fail"
        );
        assert_eq!(doc.selection(), previous);
        doc.select_offset(0).unwrap();
        assert!(doc.selection().is_empty());
    }

    #[test]
    fn pointer_caret_snaps_to_full_unicode_graphemes_not_individual_codepoints() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "ae\u{301}🧑‍💻中").unwrap();
        assert!(
            doc.set_pointer_caret(2).is_err(),
            "combining mark is not a caret stop"
        );
        assert!(doc.set_pointer_caret(6).is_err(), "inside an emoji cluster");
        doc.set_pointer_caret(4).unwrap();
        assert_eq!(doc.cursor(), 4);
        assert!(doc.extend_pointer_selection(2).is_err());
        doc.extend_pointer_selection(doc.content().len()).unwrap();
        assert_eq!(&doc.content()[doc.selection()], "🧑‍💻中");
        assert_eq!(doc.cursor(), doc.content().len());
        doc.extend_pointer_selection(1).unwrap();
        assert!(doc.selection_reversed());
    }

    #[test]
    fn shaped_hit_test_snaps_interior_emoji_and_combining_indices_to_nearest_edge() {
        let family = "🧑‍💻";
        assert_eq!(
            nearest_grapheme_edge(family, 4, 4.0, |offset| {
                if offset == 0 {
                    0.0
                } else {
                    24.0
                }
            }),
            0
        );
        assert_eq!(
            nearest_grapheme_edge(family, 4, 19.0, |offset| {
                if offset == 0 {
                    0.0
                } else {
                    24.0
                }
            }),
            family.len()
        );
        assert_eq!(
            nearest_grapheme_edge("e\u{301}x", 1, 16.0, |offset| {
                match offset {
                    0 => 0.0,
                    3 => 14.0,
                    _ => 22.0,
                }
            }),
            3
        );
        assert_eq!(
            nearest_grapheme_edge("abc", 1, 8.0, |offset| offset as f32 * 8.0),
            1
        );
        assert_eq!(nearest_grapheme_edge("", 99, 400.0, |_| 0.0), 0);
    }

    #[test]
    fn pointer_drag_can_reverse_across_lines_without_changing_document_or_undo_stack() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "abc\n中文字\nend").unwrap();
        let before = doc.history_depth();
        doc.set_pointer_caret("abc\n中文".len()).unwrap();
        doc.extend_pointer_selection(1).unwrap();
        assert!(doc.selection_reversed());
        assert_eq!(&doc.content()[doc.selection()], "bc\n中文");
        assert_eq!(doc.cursor(), 1);
        doc.extend_pointer_selection(doc.content().len()).unwrap();
        assert_eq!(&doc.content()[doc.selection()], "字\nend");
        assert!(!doc.selection_reversed());
        assert_eq!(doc.history_depth(), before);
    }

    #[test]
    fn pointer_edge_scroll_is_clamped_and_stops_at_both_ends() {
        assert_eq!(
            edge_scroll_offset(101.0, 100.0, 400.0, -50.0, 200.0),
            Some(-31.0)
        );
        assert_eq!(
            edge_scroll_offset(399.0, 100.0, 400.0, -50.0, 200.0),
            Some(-69.0)
        );
        assert_eq!(
            edge_scroll_offset(115.0, 100.0, 400.0, -2.0, 200.0),
            Some(0.0)
        );
        assert_eq!(
            edge_scroll_offset(390.0, 100.0, 400.0, -195.0, 200.0),
            Some(-200.0)
        );
        assert_eq!(edge_scroll_offset(110.0, 100.0, 400.0, 0.0, 200.0), None);
        assert_eq!(edge_scroll_offset(390.0, 100.0, 400.0, -200.0, 200.0), None);
        assert_eq!(edge_scroll_offset(250.0, 100.0, 400.0, -50.0, 200.0), None);
    }

    #[test]
    fn pointer_edge_scroll_ignores_invalid_viewports_and_non_scrollable_content() {
        assert_eq!(edge_scroll_offset(5.0, 0.0, 40.0, -20.0, 100.0), None);
        assert_eq!(edge_scroll_offset(200.0, 100.0, 400.0, 0.0, 0.0), None);
        assert_eq!(
            edge_scroll_offset(f32::NAN, 100.0, 400.0, -40.0, 200.0),
            None
        );
        assert_eq!(edge_scroll_offset(200.0, 500.0, 100.0, -40.0, 200.0), None);
        assert_eq!(
            edge_scroll_offset(200.0, 100.0, 400.0, f32::NAN, 200.0),
            None
        );
        assert_eq!(
            edge_scroll_offset(200.0, 100.0, 400.0, -20.0, f32::INFINITY),
            None
        );
    }

    #[test]
    fn editor_line_index_preserves_trailing_empty_and_unicode_byte_offsets() {
        let text = "甲🧑‍💻\n\n中文\n";
        let lines = EditorLineIndex::new(text);
        assert_eq!(lines.line_count(), 4);
        assert_eq!(&text[lines.line_range(text, 0).unwrap()], "甲🧑‍💻");
        assert_eq!(&text[lines.line_range(text, 1).unwrap()], "");
        assert_eq!(&text[lines.line_range(text, 2).unwrap()], "中文");
        assert_eq!(lines.line_range(text, 3), Some(text.len()..text.len()));
        assert_eq!(lines.line_for_offset(text, 0), Some(0));
        assert_eq!(lines.line_for_offset(text, "甲🧑‍💻".len()), Some(0));
        assert_eq!(lines.line_for_offset(text, "甲🧑‍💻\n".len()), Some(1));
        assert_eq!(lines.line_for_offset(text, text.len()), Some(3));
        assert_eq!(lines.line_for_offset(text, 1), None, "mid-CJK is invalid");
        assert_eq!(lines.line_range(text, 4), None);
        assert_eq!(lines.line_for_offset(text, text.len() + 1), None);
    }

    #[test]
    fn virtual_line_index_resolves_far_past_600_without_copying_or_truncating_text() {
        let rows: Vec<_> = (0..5000).map(|i| format!("line {i:05} 中文🧑‍💻")).collect();
        let text = rows.join("\n");
        assert!(text.len() < MAX_EDITOR_BYTES);
        let lines = EditorLineIndex::new(&text);
        assert_eq!(lines.line_count(), 5000);
        for index in [0, 1, 599, 600, 601, 2345, 4998, 4999] {
            let range = lines.line_range(&text, index).unwrap();
            assert_eq!(&text[range.clone()], rows[index]);
            assert_eq!(lines.line_for_offset(&text, range.start), Some(index));
            assert_eq!(lines.line_for_offset(&text, range.end), Some(index));
        }
        assert_eq!(lines.line_range(&text, 5000), None);
    }

    #[test]
    fn line_index_rebuilt_after_multiline_edits_and_undo_has_no_stale_offsets() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "第一行\n\n末尾").unwrap();
        let original = EditorLineIndex::new(doc.content());
        assert_eq!(original.line_count(), 3);
        doc.set_selection(0..0).unwrap();
        doc.replace(None, "新增\n").unwrap();
        let current = EditorLineIndex::new(doc.content());
        assert_eq!(current.line_count(), 4);
        assert_eq!(
            &doc.content()[current.line_range(doc.content(), 2).unwrap()],
            ""
        );
        assert!(doc.undo());
        let restored = EditorLineIndex::new(doc.content());
        assert_eq!(restored.line_count(), 3);
        assert_eq!(
            &doc.content()[restored.line_range(doc.content(), 2).unwrap()],
            "末尾"
        );
    }

    #[test]
    fn document_boundary_navigation_selects_across_long_unicode_file() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, &(0..900).map(|_| "中🧑‍💻\n").collect::<String>())
            .unwrap();
        let total = doc.content().len();
        assert_eq!(EditorLineIndex::new(doc.content()).line_count(), 901);
        doc.move_document_start(false);
        assert_eq!(doc.cursor(), 0);
        doc.move_document_end(true);
        assert_eq!(doc.selection(), 0..total);
        assert!(!doc.selection_reversed());
        doc.move_document_end(false);
        assert_eq!(doc.cursor(), total);
        doc.move_document_start(true);
        assert_eq!(doc.selection(), 0..total);
        assert!(doc.selection_reversed());
        assert_eq!(doc.cursor(), 0);
        doc.replace(None, "更新").unwrap();
        assert_eq!(doc.content(), "更新");
        assert!(doc.undo());
        assert_eq!(EditorLineIndex::new(doc.content()).line_count(), 901);
    }
}
