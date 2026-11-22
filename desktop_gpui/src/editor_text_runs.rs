//! GUI-independent Editor syntax run partitioning.
//! The NativeEditor projects these exact segments into GPUI TextRuns for
//! caret, click and IME geometry using the same syntax style as StyledText.

use std::ops::Range;

/// Split source bytes into ordered highlighted/default runs. Preserve UTF-8
/// boundaries and ignore a selection-only newline marker beyond source len.
/// No GUI dependency is allowed in this core library module.
pub fn editor_shaped_segments<T: Copy>(
    text: &str,
    highlights: &[(Range<usize>, T)],
) -> Vec<(Range<usize>, Option<T>)> {
    let mut runs = Vec::with_capacity(highlights.len() * 2 + 1);
    let mut offset = 0usize;
    for (range, highlight) in highlights {
        if range.start < offset
            || range.start >= range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            continue;
        }
        if offset < range.start {
            runs.push((offset..range.start, None));
        }
        runs.push((range.clone(), Some(*highlight)));
        offset = range.end;
    }
    if offset < text.len() || runs.is_empty() {
        runs.push((offset..text.len(), None));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_caret_segments_match_bold_italic_and_skip_newline_marker() {
        let highlights = [(0..2, 1u8), (3..5, 2u8), (5..8, 3u8)];
        assert_eq!(
            editor_shaped_segments("ab cd", &highlights),
            vec![(0..2, Some(1)), (2..3, None), (3..5, Some(2))]
        );
    }

    #[test]
    fn utf8_segments_keep_multibyte_offsets_and_style() {
        let highlights = [(1..4, 3u8)];
        assert_eq!(
            editor_shaped_segments("a中b", &highlights),
            vec![(0..1, None), (1..4, Some(3)), (4..5, None)]
        );
    }

    #[test]
    fn invalid_utf8_span_and_overlap_are_ignored() {
        let highlights = [(0..1, 1u8), (1..2, 2u8), (0..5, 3u8)];
        assert_eq!(
            editor_shaped_segments("a中b", &highlights),
            vec![(0..1, Some(1)), (1..5, None)]
        );
    }

    #[test]
    fn empty_source_keeps_one_default_zero_length_run() {
        assert_eq!(editor_shaped_segments::<u8>("", &[]), vec![(0..0, None)]);
    }
}
