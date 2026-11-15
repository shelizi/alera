//! Read-only textual reference helpers for the Native editor.
//!
//! Classic uses language-server semantic references. Until GPUI has a
//! language-provider bridge these results must be labeled "text matches":
//! identifier boundaries are enforced, but strings/comments are not parsed.
//! The actual file scan and checked result navigation reuse WorktreeContentSearch.

use std::ops::Range;

const MAX_IDENTIFIER_BYTES: usize = 128;
// ProjectSearch::search_file includes up to 80 Unicode scalar values before
// each hit (see its char_indices().rev().nth(79) calculation).
const SEARCH_CONTEXT_BEFORE: usize = 80;

fn word_part(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$'
}

fn valid_identifier(text: &str) -> bool {
    if text.is_empty() || text.len() > MAX_IDENTIFIER_BYTES {
        return false;
    }
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_alphabetic() || ch == '_' || ch == '$')
        && chars.all(word_part)
}

/// Prefer a selected symbol; otherwise inspect the word under the caret.
/// EditorDocument provides grapheme-safe byte offsets. Never reinterpret
/// UTF-16 offsets as Rust String byte indexes.
pub fn identifier_at_cursor(text: &str, selected: Range<usize>, caret: usize) -> Option<String> {
    if let Some(selected_text) = text.get(selected.clone()) {
        if !selected.is_empty() {
            return valid_identifier(selected_text).then(|| selected_text.to_owned());
        }
    } else {
        return None;
    }
    if !text.is_char_boundary(caret) {
        return None;
    }
    let mut start = caret;
    while let Some((offset, ch)) = text[..start].char_indices().last() {
        if !word_part(ch) {
            break;
        }
        start = offset;
    }
    let mut end = caret;
    while let Some(ch) = text[end..].chars().next() {
        if !word_part(ch) {
            break;
        }
        end += ch.len_utf8();
    }
    let word = text.get(start..end)?;
    valid_identifier(word).then(|| word.to_owned())
}

/// Filter the precise occurrence returned by ProjectSearch, not just any
/// nearby token in its snippet. Columns are 1-based Unicode scalar indexes.
pub fn is_whole_identifier_hit(snippet: &str, column: usize, identifier: &str) -> bool {
    if column == 0 || !valid_identifier(identifier) {
        return false;
    }
    let at = (column - 1).min(SEARCH_CONTEXT_BEFORE);
    let mut chars = snippet.chars();
    let prior = if at > 0 { chars.nth(at - 1) } else { None };
    // nth consumed the prefix; remaining iterator begins at the hit.
    let mut iter = snippet.chars().skip(at);
    let matched = identifier
        .chars()
        .all(|expected| iter.next() == Some(expected));
    if !matched {
        return false;
    }
    let after = iter.next();
    !prior.is_some_and(word_part) && !after.is_some_and(word_part)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_word_and_explicit_selection_handle_unicode_and_utf8() {
        let source = "const name = getValue(); // 變數_label";
        let at = source.find("getValue").unwrap() + 3;
        assert_eq!(
            identifier_at_cursor(source, at..at, at).as_deref(),
            Some("getValue")
        );
        let start = source.find("變數").unwrap();
        assert_eq!(
            identifier_at_cursor(source, start..start, start).as_deref(),
            Some("變數_label")
        );
        assert_eq!(
            identifier_at_cursor(source, 6..10, 8).as_deref(),
            Some("name")
        );
    }

    #[test]
    fn no_whitespace_multiline_or_numeric_literal_as_reference() {
        let source = "alpha beta\n55";
        assert_eq!(
            identifier_at_cursor(source, 5..5, 5).as_deref(),
            Some("alpha")
        );
        assert_eq!(identifier_at_cursor(source, 5..7, 6), None);
        assert_eq!(identifier_at_cursor(source, 11..13, 13), None);
        assert_eq!(identifier_at_cursor(source, 12..12, 12), None);
        assert_eq!(identifier_at_cursor(source, 100..100, 100), None);
    }

    #[test]
    fn whole_identifier_hit_uses_reported_location_not_neighboring_text() {
        assert!(is_whole_identifier_hit("foo + foo", 1, "foo"));
        assert!(is_whole_identifier_hit("foo + foo", 7, "foo"));
        assert!(!is_whole_identifier_hit("foobar + foo", 1, "foo"));
        assert!(!is_whole_identifier_hit("foo_bar + foo", 1, "foo"));
        assert!(!is_whole_identifier_hit("abcfoo xyz foo", 4, "foo"));
        assert!(!is_whole_identifier_hit("foo1 = foo", 1, "foo"));
        assert!(!is_whole_identifier_hit("x", 0, "x"));
        assert!(is_whole_identifier_hit("變數 test", 1, "變數"));
    }

    #[test]
    fn truncated_snippet_prefix_with_80_scalars_still_checks_boundaries() {
        let prefix = "a".repeat(80);
        let line = format!("{prefix} target() / target");
        assert!(is_whole_identifier_hit(&line[1..], 82, "target"));
        // The source line's first target starts at col 82, but the example
        // reports an occurrence inside a larger identifier instead.
        let joined = format!("{}footarget + target", "a".repeat(80));
        assert!(!is_whole_identifier_hit(&joined[3..], 84, "target"));
    }
}
