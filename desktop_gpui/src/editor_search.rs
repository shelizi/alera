//! Literal in-document search, independent of GPUI and disk I/O.

use crate::editor::{byte_to_utf16, EditorDocument, MAX_EDITOR_BYTES};
use std::{io, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

/// Non-overlapping literal matches, expressed as original UTF-8 byte ranges.
///
/// Insensitive matching uses locale-independent Unicode scalar lowercase,
/// including expansions (for example İ -> i + combining dot). It does not
/// perform normalization or full Unicode case folding (ß does not equal ss).
/// Results always cover complete extended graphemes, including CRLF pairs.
/// No document offsets are cached: edits, undo and tab changes cannot leave
/// stale ranges that a later replacement would apply to different text.
#[derive(Clone, Debug)]
pub struct EditorSearch {
    query: String,
    case_sensitive: bool,
}

impl EditorSearch {
    pub fn new(query: impl Into<String>, case_sensitive: bool) -> Self {
        Self {
            query: query.into(),
            case_sensitive,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub fn matches(&self, doc: &EditorDocument) -> Vec<Range<usize>> {
        if self.query.is_empty() {
            return Vec::new();
        }
        let needle = self.search_text(&self.query);
        let mut haystack = String::with_capacity(doc.content().len());
        let mut boundaries = Vec::new();
        for (offset, grapheme) in doc.content().grapheme_indices(true) {
            boundaries.push((haystack.len(), offset));
            if self.case_sensitive {
                haystack.push_str(grapheme);
            } else {
                haystack.extend(grapheme.chars().flat_map(char::to_lowercase));
            }
        }
        boundaries.push((haystack.len(), doc.content().len()));
        let mut matches = Vec::new();
        let mut cursor = 0;
        while let Some(relative) = haystack[cursor..].find(&needle) {
            let start = cursor + relative;
            let end = start + needle.len();
            let start_boundary = boundaries.binary_search_by_key(&start, |entry| entry.0);
            let end_boundary = boundaries.binary_search_by_key(&end, |entry| entry.0);
            if let (Ok(first), Ok(last)) = (start_boundary, end_boundary) {
                matches.push(boundaries[first].1..boundaries[last].1);
                cursor = end;
            } else {
                // An interior-grapheme candidate must not hide an overlapping
                // candidate that starts at a complete grapheme boundary.
                cursor = start + haystack[start..].chars().next().unwrap().len_utf8();
            }
        }
        matches
    }

    fn search_text(&self, text: &str) -> String {
        if self.case_sensitive {
            text.to_owned()
        } else {
            text.chars().flat_map(char::to_lowercase).collect()
        }
    }

    /// Select the first match at/after the selection end, wrapping at EOF.
    pub fn find_next(&self, doc: &mut EditorDocument) -> io::Result<Option<Range<usize>>> {
        ensure_not_composing(doc)?;
        let matches = self.matches(doc);
        let found = matches
            .iter()
            .find(|range| range.start >= doc.selection().end)
            .or_else(|| matches.first())
            .cloned();
        if let Some(range) = &found {
            doc.set_selection(range.clone())?;
        }
        Ok(found)
    }

    /// Select the last match before the selection start, wrapping at BOF.
    pub fn find_previous(&self, doc: &mut EditorDocument) -> io::Result<Option<Range<usize>>> {
        ensure_not_composing(doc)?;
        let matches = self.matches(doc);
        let found = matches
            .iter()
            .rev()
            .find(|range| range.end <= doc.selection().start)
            .or_else(|| matches.last())
            .cloned();
        if let Some(range) = &found {
            doc.set_selection(range.clone())?;
        }
        Ok(found)
    }

    /// Replace only an exact, currently selected match; otherwise return false.
    /// Recompute first so arbitrary edits never turn a stale match into an edit.
    pub fn replace_current(&self, doc: &mut EditorDocument, text: &str) -> io::Result<bool> {
        ensure_not_composing(doc)?;
        let selection = doc.selection();
        if !self.matches(doc).contains(&selection) {
            return Ok(false);
        }
        let range = byte_to_utf16(doc.content(), selection.start)
            ..byte_to_utf16(doc.content(), selection.end);
        doc.replace(Some(range), text)?;
        Ok(true)
    }

    /// Replace the original matches once, atomically, as one undo transaction.
    /// Returns the match count, including matches whose replacement is identical.
    pub fn replace_all(&self, doc: &mut EditorDocument, text: &str) -> io::Result<usize> {
        ensure_not_composing(doc)?;
        let matches = self.matches(doc);
        if matches.is_empty() {
            return Ok(0);
        }
        let removed: usize = matches.iter().map(|range| range.len()).sum();
        let size = text
            .len()
            .checked_mul(matches.len())
            .and_then(|added| (doc.content().len() - removed).checked_add(added))
            .filter(|size| *size <= MAX_EDITOR_BYTES)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Replacement exceeds editor size limit",
                )
            })?;
        let mut result = String::with_capacity(size);
        let mut cursor = 0;
        for range in &matches {
            result.push_str(&doc.content()[cursor..range.start]);
            result.push_str(text);
            cursor = range.end;
        }
        result.push_str(&doc.content()[cursor..]);
        let end = byte_to_utf16(doc.content(), doc.content().len());
        doc.replace(Some(0..end), &result)?;
        Ok(matches.len())
    }
}

fn ensure_not_composing(doc: &EditorDocument) -> io::Result<()> {
    if doc.marked().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Finish IME composition before find or replace",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{EditorDocument, MAX_EDITOR_BYTES};
    use std::{fs, io};

    #[test]
    fn navigation_wraps_from_selection_and_empty_queries_do_nothing() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "Cat cat Cat").unwrap();
        doc.set_selection(0..0).unwrap();
        let search = EditorSearch::new("Cat", true);
        let history = doc.history_depth();
        assert_eq!(search.find_next(&mut doc).unwrap(), Some(0..3));
        assert_eq!(search.find_next(&mut doc).unwrap(), Some(8..11));
        assert_eq!(search.find_next(&mut doc).unwrap(), Some(0..3));
        assert_eq!(search.find_previous(&mut doc).unwrap(), Some(8..11));
        assert_eq!(doc.history_depth(), history);
        assert_eq!(
            EditorSearch::new("cat", false).matches(&doc),
            vec![0..3, 4..7, 8..11]
        );
        let empty = EditorSearch::new("", false);
        assert_eq!(empty.find_next(&mut doc).unwrap(), None);
        assert_eq!(empty.find_previous(&mut doc).unwrap(), None);
        assert_eq!(empty.replace_all(&mut doc, "x").unwrap(), 0);
        assert_eq!(doc.selection(), 8..11);
    }

    #[test]
    fn unicode_matches_map_to_original_bytes_and_never_split_graphemes() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "中É e\u{301} 🧑‍💻 İ i\u{307}\nÉ").unwrap();
        assert_eq!(
            EditorSearch::new("é", false).matches(&doc),
            vec![3..5, 29..31]
        );
        assert!(EditorSearch::new("e", false).matches(&doc).is_empty());
        assert!(EditorSearch::new("💻", true).matches(&doc).is_empty());
        assert!(EditorSearch::new("i", false).matches(&doc).is_empty());
        let dotted = EditorSearch::new("i\u{307}", false);
        let ranges = dotted.matches(&doc);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&doc.content()[ranges[0].clone()], "İ");
        assert_eq!(&doc.content()[ranges[1].clone()], "i\u{307}");
        assert_eq!(dotted.replace_all(&mut doc, "中").unwrap(), 2);
        assert_eq!(doc.content(), "中É e\u{301} 🧑‍💻 中 中\nÉ");
    }

    #[test]
    fn matches_refresh_after_edits_and_multiline_replace_all_handles_overlap() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "aaa\r\naaa").unwrap();
        let search = EditorSearch::new("aa", true);
        assert_eq!(search.matches(&doc), vec![0..2, 5..7]);
        assert!(EditorSearch::new("\n", true).matches(&doc).is_empty());
        let multiline = EditorSearch::new("a\r\na", true);
        assert_eq!(multiline.matches(&doc), vec![2..6]);
        assert_eq!(multiline.replace_all(&mut doc, "").unwrap(), 1);
        assert_eq!(doc.content(), "aaaa");
        assert_eq!(search.matches(&doc), vec![0..2, 2..4]);
        assert!(doc.undo());
        assert_eq!(search.matches(&doc), vec![0..2, 5..7]);
        let history = doc.history_depth();
        assert_eq!(search.replace_all(&mut doc, "aa").unwrap(), 2);
        assert_eq!(
            doc.history_depth(),
            history,
            "identical replacements preserve redo"
        );
        doc.replace(None, "x").unwrap();
        assert_eq!(search.matches(&doc), vec![0..2, 5..7]);
        doc.set_selection(0..2).unwrap();
        doc.replace(None, "x").unwrap();
        assert_eq!(search.matches(&doc), vec![4..6]);
        assert!(!search.replace_current(&mut doc, "stale").unwrap());
    }

    #[test]
    fn replacement_is_one_undo_step_and_preserves_disk_conflict_checks() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        fs::write(&path, "猫 cat cat").unwrap();
        let mut doc = EditorDocument::open(root.path(), &path).unwrap();
        let search = EditorSearch::new("cat", true);
        doc.set_selection(0..0).unwrap();
        assert!(!search.replace_current(&mut doc, "犬").unwrap());
        search.find_next(&mut doc).unwrap();
        assert!(search.replace_current(&mut doc, "犬").unwrap());
        assert_eq!(doc.content(), "猫 犬 cat");
        assert!(doc.undo());
        assert!(!doc.is_dirty());
        let selection = doc.selection();
        assert_eq!(search.replace_all(&mut doc, "catcat").unwrap(), 2);
        assert_eq!(doc.content(), "猫 catcat catcat");
        assert_eq!(doc.history_depth(), (1, 0));
        assert!(doc.undo());
        assert_eq!(doc.selection(), selection);
        assert!(!doc.is_dirty());
        assert!(doc.redo());
        fs::write(&path, "external").unwrap();
        assert_eq!(doc.save().unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    }

    #[test]
    fn rejected_replacements_preserve_selection_history_and_ime() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "aa aa").unwrap();
        let search = EditorSearch::new("aa", true);
        doc.set_selection(0..2).unwrap();
        let history = doc.history_depth();
        for replacement in ["\0".to_string(), "x".repeat(MAX_EDITOR_BYTES / 2)] {
            assert!(search.replace_all(&mut doc, &replacement).is_err());
            assert!(search
                .replace_current(&mut doc, &"x".repeat(MAX_EDITOR_BYTES))
                .is_err());
            assert_eq!(doc.content(), "aa aa");
            assert_eq!(doc.selection(), 0..2);
            assert_eq!(doc.history_depth(), history);
        }
        doc.replace_and_mark(None, "aa", Some(2..2)).unwrap();
        let marked = doc.marked();
        let selection = doc.selection();
        assert_eq!(
            search.find_next(&mut doc).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert!(search.find_previous(&mut doc).is_err());
        assert!(search.replace_current(&mut doc, "x").is_err());
        assert!(search.replace_all(&mut doc, "x").is_err());
        assert_eq!(doc.marked(), marked);
        assert_eq!(doc.selection(), selection);
        assert_eq!(doc.history_depth(), history);
    }
}
