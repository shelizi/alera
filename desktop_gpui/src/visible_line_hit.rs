//! Pointer hits against the editor rows painted for the current viewport.
//!
//! The GPUI window records plain geometry and forwards the pointer. This
//! module decides which row, if any, receives the hit. A later viewport
//! replaces the previous rows, so a coordinate from a line that scrolled
//! away cannot move the caret.
use crate::editor::{nearest_grapheme_edge, EditorDocument};
use std::io;

/// One painted row, in pixels, clipped to the viewport that produced it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisibleLineGeometry {
    pub start: usize,
    pub end: usize,
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub clip_top: f32,
    pub clip_bottom: f32,
}

/// Rows the current frame may hit. [`Self::replace`] drops every previous row.
#[derive(Clone, Debug, Default)]
pub struct VisibleLineHitSet {
    lines: Vec<VisibleLineGeometry>,
}

impl VisibleLineHitSet {
    /// Install the rows painted for the viewport that is on screen now.
    /// Rows that miss that clip, or that have an unusable span, are omitted.
    pub fn replace(&mut self, lines: impl IntoIterator<Item = VisibleLineGeometry>) {
        self.lines.clear();
        self.lines.extend(lines.into_iter().filter(row_is_hittable));
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Document byte under `(x, y)`, using only the rows from the latest
    /// [`Self::replace`]. `y` outside every current row returns `None`.
    /// The closures are consulted only for the chosen row.
    pub fn pointer_offset(
        &self,
        content: &str,
        x: f32,
        y: f32,
        mut x_for_index: impl FnMut(&VisibleLineGeometry, usize) -> f32,
        mut glyph_index_for_x: impl FnMut(&VisibleLineGeometry, f32) -> usize,
    ) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let row = self.lines.iter().find(|line| y_hits(line, y))?;
        let text = content.get(row.start..row.end)?;
        let local_x = x - row.left;
        if !local_x.is_finite() {
            return None;
        }
        let guessed = glyph_index_for_x(row, local_x).min(text.len());
        let edge = nearest_grapheme_edge(text, guessed, local_x, |index| x_for_index(row, index));
        Some(row.start + edge)
    }
}

/// Apply a hit from [`VisibleLineHitSet::pointer_offset`].
/// `None` leaves the caret, selection, and document bytes unchanged.
pub fn apply_pointer_hit(
    document: &mut EditorDocument,
    offset: Option<usize>,
    extend: bool,
) -> io::Result<bool> {
    let Some(offset) = offset else {
        return Ok(false);
    };
    if extend {
        document.extend_pointer_selection(offset)?;
    } else {
        document.set_pointer_caret(offset)?;
    }
    Ok(true)
}

fn row_is_hittable(line: &VisibleLineGeometry) -> bool {
    line.start <= line.end
        && [
            line.top,
            line.bottom,
            line.left,
            line.clip_top,
            line.clip_bottom,
        ]
        .into_iter()
        .all(f32::is_finite)
        && clipped_span(line).is_some()
}

fn clipped_span(line: &VisibleLineGeometry) -> Option<(f32, f32)> {
    let top = line.top.max(line.clip_top);
    let bottom = line.bottom.min(line.clip_bottom);
    (bottom > top).then_some((top, bottom))
}

fn y_hits(line: &VisibleLineGeometry, y: f32) -> bool {
    clipped_span(line).is_some_and(|(top, bottom)| y >= top && y < bottom)
}

#[cfg(test)]
mod tests {
    use super::{apply_pointer_hit, VisibleLineGeometry, VisibleLineHitSet};
    use crate::editor::{EditorDocument, EditorLineIndex};

    fn frame(start: usize, end: usize, top: f32, clip_bottom: f32) -> VisibleLineGeometry {
        VisibleLineGeometry {
            start,
            end,
            top,
            bottom: top + 19.0,
            left: 4.0,
            clip_top: 0.0,
            clip_bottom,
        }
    }

    fn offset_of(hits: &VisibleLineHitSet, text: &str, x: f32, y: f32) -> Option<usize> {
        hits.pointer_offset(
            text,
            x,
            y,
            |row, index| {
                assert!(row.start == 3 || row.start == 9 || row.start == 0);
                index as f32 * 8.0
            },
            |_row, local_x| (local_x / 8.0) as usize,
        )
    }

    #[test]
    fn pointer_hit_uses_only_the_current_visible_lines() {
        let text = "aa\n😀b\nzz\n";
        let index = EditorLineIndex::new(text);
        let line0 = index.line_range(text, 0).unwrap();
        let line1 = index.line_range(text, 1).unwrap();
        let line2 = index.line_range(text, 2).unwrap();
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("hit.txt");
        std::fs::write(&path, text).unwrap();
        let mut document = EditorDocument::open(root.path(), &path).unwrap();
        let original = document.content().to_owned();
        let before_caret = document.cursor();
        let before_selection = document.selection();

        let mut hits = VisibleLineHitSet::default();
        hits.replace([
            frame(line1.start, line1.end, 19.0, 80.0),
            frame(line0.start, line0.end, 400.0, 40.0),
        ]);
        assert_eq!(hits.len(), 1, "a row outside the clip is not hittable");
        // Local x=16 lands inside the emoji's 4-byte grapheme.
        let on_emoji = offset_of(&hits, document.content(), 4.0 + 16.0, 20.0).unwrap();
        assert_eq!(on_emoji, line1.start);
        assert!(apply_pointer_hit(&mut document, Some(on_emoji), false).unwrap());
        assert_eq!(document.cursor(), line1.start);
        assert_eq!(document.content(), original);

        hits.replace([frame(line2.start, line2.end, 200.0, 400.0)]);
        let missed = offset_of(&hits, document.content(), 4.0 + 16.0, 20.0);
        assert_eq!(missed, None);
        assert!(!apply_pointer_hit(&mut document, missed, false).unwrap());
        assert_eq!(document.cursor(), line1.start);
        assert_eq!(document.selection(), line1.start..line1.start);
        assert_eq!(document.content(), original);
        assert_ne!(document.cursor(), before_caret);
        assert_ne!(document.selection(), before_selection);

        let on_last = offset_of(&hits, document.content(), 4.0, 200.0).unwrap();
        assert_eq!(on_last, line2.start);
        assert!(apply_pointer_hit(&mut document, Some(on_last), true).unwrap());
        assert_eq!(document.selection(), line1.start..line2.start);
        assert_eq!(document.cursor(), line2.start);
        assert_eq!(document.content(), original);
        assert_eq!(offset_of(&hits, document.content(), 4.0, f32::NAN), None);
    }
}
