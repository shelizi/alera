use super::rope::RopeBridge;
use flutter_rust_bridge::frb;
use ropey::Rope as RustRope;
use std::cmp::{max, min};
use std::sync::Mutex;
use tree_sitter::{
    InputEdit, Language, Parser, Point, Query, QueryCursor, StreamingIterator, Tree,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorDocumentEdit {
    /// Scalar-value (Rope char) offset in the pre-edit document.
    pub start: usize,
    /// Scalar-value (Rope char) offset in the pre-edit document.
    pub end: usize,
    pub replacement: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorDocumentRevision {
    pub document_id: String,
    pub revision: u64,
    pub applied: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSyntaxSpan {
    pub line: usize,
    /// Unicode scalar column, matching CodeForge's Rope offsets.
    pub start_column: usize,
    /// Unicode scalar column, matching CodeForge's Rope offsets.
    pub end_column: usize,
    /// Highlight.js-compatible scope consumed by the existing Dart theme.
    pub scope: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxSpanResponse {
    pub document_id: String,
    pub revision: u64,
    pub requested_start_line: usize,
    pub requested_end_line: usize,
    pub actual_start_line: usize,
    pub actual_end_line: usize,
    pub supported: bool,
    pub stale: bool,
    pub spans: Vec<NativeSyntaxSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeEditorDocumentInfo {
    pub document_id: String,
    pub revision: u64,
    pub language_id: String,
    pub len_chars: usize,
    pub len_lines: usize,
    pub parser_supported: bool,
    pub closed: bool,
}

struct NativeLanguage {
    language: Language,
    highlight_query: String,
}

struct NativeEditorDocumentState {
    document_id: String,
    revision: u64,
    language_id: String,
    rope: RustRope,
    parser: Option<Parser>,
    tree: Option<Tree>,
    query: Option<Query>,
    closed: bool,
}

#[frb(opaque)]
pub struct NativeEditorDocument {
    state: Mutex<NativeEditorDocumentState>,
}

impl NativeEditorDocument {
    /// Opens a retained native document. The initial full text crosses FFI once;
    /// subsequent updates are edit deltas and parser input is read from Rope chunks.
    pub fn open(
        document_id: String,
        revision: u64,
        text: String,
        language_id: String,
    ) -> Result<Self, String> {
        Self::open_with_rope(
            document_id,
            revision,
            RustRope::from_str(&text),
            language_id,
        )
    }

    /// Opens retained syntax state by structurally cloning the existing native
    /// CodeForge Rope. This avoids Rope -> Dart String -> second native Rope
    /// during editor startup.
    pub fn open_from_rope(
        document_id: String,
        revision: u64,
        rope: &RopeBridge,
        language_id: String,
    ) -> Result<Self, String> {
        let rope = rope.rope.read().map_err(|_| "rope lock poisoned")?.clone();
        Self::open_with_rope(document_id, revision, rope, language_id)
    }

    fn open_with_rope(
        document_id: String,
        revision: u64,
        rope: RustRope,
        language_id: String,
    ) -> Result<Self, String> {
        let normalized_language_id = normalize_language_id(&language_id);
        let native_language = native_language(&normalized_language_id)?;

        let (parser, tree, query) = if let Some(native_language) = native_language {
            let mut parser = Parser::new();
            parser
                .set_language(&native_language.language)
                .map_err(|error| format!("failed to configure parser: {error}"))?;
            let tree = parse_rope(&mut parser, &rope, None)
                .ok_or_else(|| "tree-sitter parser returned no tree".to_string())?;
            let query = Query::new(&native_language.language, &native_language.highlight_query)
                .map_err(|error| format!("failed to compile highlight query: {error}"))?;
            (Some(parser), Some(tree), Some(query))
        } else {
            (None, None, None)
        };

        Ok(Self {
            state: Mutex::new(NativeEditorDocumentState {
                document_id,
                revision,
                language_id: normalized_language_id,
                rope,
                parser,
                tree,
                query,
                closed: false,
            }),
        })
    }

    /// Applies one or more edits against the retained Rope and incrementally reparses
    /// from the previously edited tree. A revision mismatch is rejected without
    /// modifying native state so stale Dart work cannot overwrite newer text.
    pub fn apply_edits(
        &self,
        expected_revision: u64,
        new_revision: u64,
        edits: Vec<EditorDocumentEdit>,
    ) -> Result<EditorDocumentRevision, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "native document lock poisoned")?;
        if state.closed {
            return Err("native editor document is closed".to_string());
        }
        if state.revision != expected_revision || new_revision <= expected_revision {
            return Ok(EditorDocumentRevision {
                document_id: state.document_id.clone(),
                revision: state.revision,
                applied: false,
            });
        }

        for edit in edits {
            apply_edit(&mut state, edit)?;
        }
        state.revision = new_revision;

        Ok(EditorDocumentRevision {
            document_id: state.document_id.clone(),
            revision: state.revision,
            applied: true,
        })
    }

    /// Returns syntax captures only for the requested viewport plus fixed overscan.
    /// No document-sized span collection is built or transferred across FFI.
    pub fn query_syntax_spans(
        &self,
        expected_revision: u64,
        start_line: usize,
        end_line: usize,
        overscan: usize,
    ) -> Result<SyntaxSpanResponse, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "native document lock poisoned")?;
        let requested_start = min(start_line, state.rope.len_lines().saturating_sub(1));
        let requested_end = min(
            max(end_line, requested_start),
            state.rope.len_lines().saturating_sub(1),
        );

        if state.closed {
            return Ok(SyntaxSpanResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                requested_start_line: requested_start,
                requested_end_line: requested_end,
                actual_start_line: requested_start,
                actual_end_line: requested_end,
                supported: false,
                stale: true,
                spans: Vec::new(),
            });
        }

        if state.revision != expected_revision {
            return Ok(SyntaxSpanResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                requested_start_line: requested_start,
                requested_end_line: requested_end,
                actual_start_line: requested_start,
                actual_end_line: requested_end,
                supported: state.tree.is_some() && state.query.is_some(),
                stale: true,
                spans: Vec::new(),
            });
        }

        let total_lines = state.rope.len_lines();
        let actual_start = requested_start.saturating_sub(overscan);
        let actual_end = min(
            requested_end.saturating_add(overscan),
            total_lines.saturating_sub(1),
        );

        let (Some(tree), Some(query)) = (&state.tree, &state.query) else {
            return Ok(SyntaxSpanResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                requested_start_line: requested_start,
                requested_end_line: requested_end,
                actual_start_line: actual_start,
                actual_end_line: actual_end,
                supported: false,
                stale: false,
                spans: Vec::new(),
            });
        };

        let start_char = state.rope.line_to_char(actual_start);
        let end_char = if actual_end + 1 < total_lines {
            state.rope.line_to_char(actual_end + 1)
        } else {
            state.rope.len_chars()
        };
        let start_byte = state.rope.char_to_byte(start_char);
        let end_byte = state.rope.char_to_byte(end_char);

        let capture_names = query.capture_names();
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(start_byte..end_byte);
        let rope = &state.rope;
        let mut captures =
            cursor.captures(query, tree.root_node(), |node: tree_sitter::Node<'_>| {
                let range = node.byte_range();
                let text = if range.start <= range.end && range.end <= rope.len_bytes() {
                    rope.byte_slice(range).to_string()
                } else {
                    String::new()
                };
                std::iter::once(text)
            });

        let mut spans = Vec::new();
        while let Some((query_match, capture_index)) = captures.next() {
            let capture = query_match.captures[*capture_index];
            let Some(scope) = capture_names
                .get(capture.index as usize)
                .and_then(|name| normalize_scope(name))
            else {
                continue;
            };
            append_capture_spans(
                &mut spans,
                rope,
                capture.node.start_byte(),
                capture.node.end_byte(),
                actual_start,
                actual_end,
                scope,
            );
        }

        spans.sort_by(|left, right| {
            left.line
                .cmp(&right.line)
                .then(left.start_column.cmp(&right.start_column))
                .then(right.end_column.cmp(&left.end_column))
                .then(left.scope.cmp(&right.scope))
        });
        spans.dedup();

        Ok(SyntaxSpanResponse {
            document_id: state.document_id.clone(),
            revision: state.revision,
            requested_start_line: requested_start,
            requested_end_line: requested_end,
            actual_start_line: actual_start,
            actual_end_line: actual_end,
            supported: true,
            stale: false,
            spans,
        })
    }

    #[frb(sync)]
    pub fn info(&self) -> Result<NativeEditorDocumentInfo, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "native document lock poisoned")?;
        Ok(NativeEditorDocumentInfo {
            document_id: state.document_id.clone(),
            revision: state.revision,
            language_id: state.language_id.clone(),
            len_chars: state.rope.len_chars(),
            len_lines: state.rope.len_lines(),
            parser_supported: state.tree.is_some() && state.query.is_some(),
            closed: state.closed,
        })
    }

    /// Explicitly releases retained Rope/parser/tree state before the opaque Dart
    /// handle itself becomes unreachable.
    pub fn close(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "native document lock poisoned")?;
        state.rope = RustRope::new();
        state.parser = None;
        state.tree = None;
        state.query = None;
        state.closed = true;
        Ok(())
    }
}

fn apply_edit(
    state: &mut NativeEditorDocumentState,
    edit: EditorDocumentEdit,
) -> Result<(), String> {
    let len_chars = state.rope.len_chars();
    if edit.start > edit.end || edit.end > len_chars {
        return Err(format!(
            "invalid editor edit range {}..{} for document length {}",
            edit.start, edit.end, len_chars
        ));
    }

    let start_byte = state.rope.char_to_byte(edit.start);
    let old_end_byte = state.rope.char_to_byte(edit.end);
    let start_position = point_for_char(&state.rope, edit.start);
    let old_end_position = point_for_char(&state.rope, edit.end);
    let replacement_len_chars = edit.replacement.chars().count();
    let replacement_len_bytes = edit.replacement.len();
    let new_end_position = advance_point(start_position, &edit.replacement);

    if let Some(tree) = state.tree.as_mut() {
        tree.edit(&InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte: start_byte + replacement_len_bytes,
            start_position,
            old_end_position,
            new_end_position,
        });
    }

    if edit.start < edit.end {
        state.rope.remove(edit.start..edit.end);
    }
    if !edit.replacement.is_empty() {
        state.rope.insert(edit.start, &edit.replacement);
    }

    debug_assert_eq!(
        state.rope.len_chars(),
        len_chars - (edit.end - edit.start) + replacement_len_chars
    );

    if let Some(parser) = state.parser.as_mut() {
        let previous_tree = state.tree.as_ref();
        state.tree = parse_rope(parser, &state.rope, previous_tree);
        if state.tree.is_none() {
            return Err("tree-sitter incremental parse returned no tree".to_string());
        }
    }

    Ok(())
}

fn parse_rope(parser: &mut Parser, rope: &RustRope, old_tree: Option<&Tree>) -> Option<Tree> {
    parser.parse_with_options(
        &mut |byte_offset, _position| {
            if byte_offset >= rope.len_bytes() {
                return &[][..];
            }
            let (chunk, chunk_byte_idx, _, _) = rope.chunk_at_byte(byte_offset);
            &chunk.as_bytes()[byte_offset - chunk_byte_idx..]
        },
        old_tree,
        None,
    )
}

fn point_for_char(rope: &RustRope, char_offset: usize) -> Point {
    let safe_char = min(char_offset, rope.len_chars());
    let row = rope.char_to_line(safe_char);
    let row_start_char = rope.line_to_char(row);
    let byte = rope.char_to_byte(safe_char);
    let row_start_byte = rope.char_to_byte(row_start_char);
    Point::new(row, byte - row_start_byte)
}

fn advance_point(start: Point, inserted: &str) -> Point {
    let bytes = inserted.as_bytes();
    let newline_count = bytes.iter().filter(|byte| **byte == b'\n').count();
    if newline_count == 0 {
        Point::new(start.row, start.column + bytes.len())
    } else {
        let trailing_column = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map(|index| bytes.len() - index - 1)
            .unwrap_or(bytes.len());
        Point::new(start.row + newline_count, trailing_column)
    }
}

fn append_capture_spans(
    spans: &mut Vec<NativeSyntaxSpan>,
    rope: &RustRope,
    start_byte: usize,
    end_byte: usize,
    min_line: usize,
    max_line: usize,
    scope: &str,
) {
    if start_byte >= end_byte || start_byte >= rope.len_bytes() {
        return;
    }
    let safe_end_byte = min(end_byte, rope.len_bytes());
    let start_char = rope.byte_to_char(start_byte);
    let end_char = rope.byte_to_char(safe_end_byte);
    let first_line = max(rope.char_to_line(start_char), min_line);
    let last_line = min(rope.char_to_line(end_char), max_line);

    for line in first_line..=last_line {
        let line_start = rope.line_to_char(line);
        let line_content_end = line_content_end_char(rope, line);
        let span_start = max(start_char, line_start);
        let span_end = min(end_char, line_content_end);
        if span_start >= span_end {
            continue;
        }
        spans.push(NativeSyntaxSpan {
            line,
            start_column: span_start - line_start,
            end_column: span_end - line_start,
            scope: scope.to_string(),
        });
    }
}

fn line_content_end_char(rope: &RustRope, line: usize) -> usize {
    let line_start = rope.line_to_char(line);
    let slice = rope.line(line);
    let mut content_len = slice.len_chars();
    if content_len > 0 && slice.char(content_len - 1) == '\n' {
        content_len -= 1;
        if content_len > 0 && slice.char(content_len - 1) == '\r' {
            content_len -= 1;
        }
    }
    line_start + content_len
}

fn normalize_language_id(language_id: &str) -> String {
    match language_id.trim().to_lowercase().as_str() {
        "js" | "jsx" => "javascript".to_string(),
        "ts" => "typescript".to_string(),
        "py" => "python".to_string(),
        "rs" => "rust".to_string(),
        "jsonc" => "json".to_string(),
        other => other.to_string(),
    }
}

fn native_language(language_id: &str) -> Result<Option<NativeLanguage>, String> {
    let language = match language_id {
        "dart" => NativeLanguage {
            language: tree_sitter_dart::LANGUAGE.into(),
            highlight_query: tree_sitter_dart::HIGHLIGHTS_QUERY.to_string(),
        },
        "rust" => NativeLanguage {
            language: tree_sitter_rust::LANGUAGE.into(),
            highlight_query: tree_sitter_rust::HIGHLIGHTS_QUERY.to_string(),
        },
        "javascript" => NativeLanguage {
            language: tree_sitter_javascript::LANGUAGE.into(),
            highlight_query: format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
            ),
        },
        "typescript" => NativeLanguage {
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            highlight_query: format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            ),
        },
        "tsx" => NativeLanguage {
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            highlight_query: format!(
                "{}\n{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            ),
        },
        "python" => NativeLanguage {
            language: tree_sitter_python::LANGUAGE.into(),
            highlight_query: tree_sitter_python::HIGHLIGHTS_QUERY.to_string(),
        },
        "json" => NativeLanguage {
            language: tree_sitter_json::LANGUAGE.into(),
            highlight_query: tree_sitter_json::HIGHLIGHTS_QUERY.to_string(),
        },
        _ => return Ok(None),
    };
    Ok(Some(language))
}

fn normalize_scope(name: &str) -> Option<&'static str> {
    let root = name.split('.').next().unwrap_or(name);
    match root {
        "comment" => Some("comment"),
        "string" => Some("string"),
        "keyword" => Some("keyword"),
        "number" | "float" => Some("number"),
        "function" | "method" => Some("function"),
        "constructor" | "class" => Some("title.class_"),
        "type" => Some("type"),
        "variable" | "parameter" => Some("variable"),
        "property" | "attribute" => Some("attr"),
        "tag" => Some("tag"),
        "constant" | "boolean" => Some("literal"),
        "operator" => Some("operator"),
        "label" => Some("symbol"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn char_offset(text: &str, needle: &str) -> usize {
        let byte_offset = text.find(needle).expect("needle must exist");
        text[..byte_offset].chars().count()
    }

    #[test]
    fn opens_and_queries_only_viewport_plus_overscan() {
        let text = "fn first() {\n    let a = 1;\n}\n\nfn second() {\n    let b = 2;\n}\n";
        let document = NativeEditorDocument::open(
            "doc-1".to_string(),
            7,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let response = document.query_syntax_spans(7, 4, 5, 0).unwrap();
        assert!(response.supported);
        assert!(!response.stale);
        assert_eq!(response.actual_start_line, 4);
        assert_eq!(response.actual_end_line, 5);
        assert!(!response.spans.is_empty());
        assert!(response
            .spans
            .iter()
            .all(|span| (4..=5).contains(&span.line)));
    }

    #[test]
    fn opens_retained_document_from_existing_native_rope() {
        let text = "fn main() {\n    let value = 1;\n}\n";
        let rope = RopeBridge::create(text.to_string());
        let document = NativeEditorDocument::open_from_rope(
            "doc-native-rope".to_string(),
            9,
            &rope,
            "rust".to_string(),
        )
        .unwrap();

        let info = document.info().unwrap();
        assert_eq!(info.revision, 9);
        assert_eq!(info.len_chars, text.chars().count());
        assert_eq!(info.len_lines, text.lines().count() + 1);
        assert!(info.parser_supported);
        let response = document.query_syntax_spans(9, 0, 2, 0).unwrap();
        assert!(response.supported);
        assert!(!response.stale);
        assert!(!response.spans.is_empty());
    }

    #[test]
    fn incrementally_applies_edit_and_advances_revision() {
        let text = "fn main() {\n    let value = 1;\n}\n";
        let document = NativeEditorDocument::open(
            "doc-2".to_string(),
            10,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();
        let start = char_offset(text, "value");

        let revision = document
            .apply_edits(
                10,
                11,
                vec![EditorDocumentEdit {
                    start,
                    end: start,
                    replacement: "mut ".to_string(),
                }],
            )
            .unwrap();
        assert!(revision.applied);
        assert_eq!(revision.revision, 11);
        let info = document.info().unwrap();
        assert_eq!(info.len_chars, text.chars().count() + 4);

        let response = document.query_syntax_spans(11, 0, 2, 0).unwrap();
        assert!(response.supported);
        assert!(!response.stale);
        assert!(response.spans.iter().any(|span| span.scope == "keyword"));
    }

    #[test]
    fn rejects_stale_edits_and_queries() {
        let document = NativeEditorDocument::open(
            "doc-3".to_string(),
            3,
            "fn main() {}\n".to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let revision = document
            .apply_edits(
                2,
                4,
                vec![EditorDocumentEdit {
                    start: 0,
                    end: 0,
                    replacement: "pub ".to_string(),
                }],
            )
            .unwrap();
        assert!(!revision.applied);
        assert_eq!(revision.revision, 3);

        let response = document.query_syntax_spans(2, 0, 0, 0).unwrap();
        assert!(response.stale);
        assert!(response.spans.is_empty());
        assert_eq!(document.info().unwrap().revision, 3);
    }

    #[test]
    fn rejects_non_monotonic_revision_advance() {
        let original = "fn main() {}\n";
        let document = NativeEditorDocument::open(
            "doc-revision".to_string(),
            5,
            original.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        for new_revision in [4, 5] {
            let result = document
                .apply_edits(
                    5,
                    new_revision,
                    vec![EditorDocumentEdit {
                        start: 0,
                        end: 0,
                        replacement: "pub ".to_string(),
                    }],
                )
                .unwrap();
            assert!(!result.applied);
            assert_eq!(result.revision, 5);
        }

        assert_eq!(document.info().unwrap().len_chars, original.chars().count());
    }

    #[test]
    fn unicode_scalar_edit_keeps_parser_and_rope_in_sync() {
        let text = "void main() { final value = \"😀\"; }\n";
        let document = NativeEditorDocument::open(
            "doc-4".to_string(),
            1,
            text.to_string(),
            "dart".to_string(),
        )
        .unwrap();
        let start = char_offset(text, "😀");

        document
            .apply_edits(
                1,
                2,
                vec![EditorDocumentEdit {
                    start,
                    end: start + 1,
                    replacement: "🚀".to_string(),
                }],
            )
            .unwrap();

        let info = document.info().unwrap();
        assert_eq!(info.revision, 2);
        assert_eq!(info.len_chars, text.chars().count());
        let response = document.query_syntax_spans(2, 0, 0, 0).unwrap();
        assert!(response.supported);
        assert!(response.spans.iter().any(|span| span.scope == "string"));
    }

    #[test]
    fn unsupported_language_retains_text_but_falls_back_cleanly() {
        let document = NativeEditorDocument::open(
            "doc-5".to_string(),
            1,
            "hello world\n".to_string(),
            "plaintext".to_string(),
        )
        .unwrap();
        let info = document.info().unwrap();
        assert!(!info.parser_supported);

        let response = document.query_syntax_spans(1, 0, 0, 20).unwrap();
        assert!(!response.supported);
        assert!(!response.stale);
        assert!(response.spans.is_empty());
    }

    #[test]
    #[ignore = "manual performance evidence; run with --ignored --nocapture"]
    fn benchmark_incremental_edit_and_viewport_query_scaling() {
        fn source(lines: usize) -> String {
            let mut text = String::with_capacity(lines * 32);
            for index in 0..lines {
                text.push_str("fn item_");
                text.push_str(&index.to_string());
                text.push_str("() { let value = 1; }\n");
            }
            text
        }

        fn sample(lines: usize) -> (Vec<u128>, Vec<u128>, Vec<u128>) {
            let text = source(lines);
            let target = text.chars().count().saturating_sub(2);
            let mut open_samples = Vec::with_capacity(5);
            let mut edit_samples = Vec::with_capacity(5);
            let mut query_samples = Vec::with_capacity(5);
            for sample_index in 0..5u64 {
                let started = Instant::now();
                let document = NativeEditorDocument::open(
                    format!("bench-{lines}-{sample_index}"),
                    1,
                    text.clone(),
                    "rust".to_string(),
                )
                .unwrap();
                open_samples.push(started.elapsed().as_micros());

                let started = Instant::now();
                let revision = document
                    .apply_edits(
                        1,
                        2,
                        vec![EditorDocumentEdit {
                            start: target,
                            end: target,
                            replacement: " ".to_string(),
                        }],
                    )
                    .unwrap();
                assert!(revision.applied);
                edit_samples.push(started.elapsed().as_micros());

                let last_line = lines.saturating_sub(1);
                let query_start = last_line.saturating_sub(40);
                let started = Instant::now();
                let response = document
                    .query_syntax_spans(2, query_start, last_line, 20)
                    .unwrap();
                assert!(response.supported && !response.stale);
                query_samples.push(started.elapsed().as_micros());
            }
            (open_samples, edit_samples, query_samples)
        }

        for lines in [2_000usize, 20_000, 100_000] {
            let (open_us, edit_us, query_us) = sample(lines);
            println!(
                "NativeEditorDocument benchmark lines={lines} open_us={open_us:?} edit_us={edit_us:?} viewport_query_us={query_us:?}"
            );
        }
    }

    #[test]
    fn close_releases_retained_state() {
        let document = NativeEditorDocument::open(
            "doc-6".to_string(),
            1,
            "fn main() {}\n".to_string(),
            "rust".to_string(),
        )
        .unwrap();
        document.close().unwrap();
        let info = document.info().unwrap();
        assert!(info.closed);
        assert_eq!(info.len_chars, 0);
        assert!(!info.parser_supported);
    }
}
