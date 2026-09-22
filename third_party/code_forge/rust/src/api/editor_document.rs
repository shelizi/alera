use super::editor_languages::resolve_native_language;
use super::rope::RopeBridge;
use flutter_rust_bridge::frb;
use ropey::Rope as RustRope;
use std::cmp::{max, min};
use std::ops::ControlFlow;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use tree_sitter::{
    InputEdit, ParseOptions, Parser, Point, Query, QueryCursor, StreamingIterator, Tree,
};

const NATIVE_PARSE_CANCELLED_ERROR: &str = "native editor parse cancelled";

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
pub struct NativeFoldingRange {
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldingRangeResponse {
    pub document_id: String,
    pub revision: u64,
    pub supported: bool,
    pub stale: bool,
    pub ranges: Vec<NativeFoldingRange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BracketMatchResponse {
    pub document_id: String,
    pub revision: u64,
    pub supported: bool,
    pub stale: bool,
    /// Unicode-scalar offset of the structural matching delimiter, or -1.
    pub match_offset: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralSelectionResponse {
    pub document_id: String,
    pub revision: u64,
    pub supported: bool,
    pub stale: bool,
    /// Unicode-scalar start offset of the next enclosing named syntax node, or -1.
    pub start_offset: i64,
    /// Unicode-scalar exclusive end offset of the next enclosing named syntax node, or -1.
    pub end_offset: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeDocumentSymbol {
    pub name: String,
    pub kind: String,
    /// Unicode-scalar range of the full declaration node.
    pub start_offset: usize,
    pub end_offset: usize,
    /// Unicode-scalar range of the declaration name used for navigation.
    pub selection_start_offset: usize,
    pub selection_end_offset: usize,
    pub start_line: usize,
    pub end_line: usize,
    /// Symbol nesting depth, not raw AST depth.
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSymbolsResponse {
    pub document_id: String,
    pub revision: u64,
    pub supported: bool,
    pub stale: bool,
    pub truncated: bool,
    pub symbols: Vec<NativeDocumentSymbol>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeParseProgress {
    pub current_byte_offset: usize,
    pub total_bytes: usize,
}

#[derive(Default)]
#[frb(ignore)]
struct NativeParseProgressState {
    current_byte_offset: AtomicUsize,
    total_bytes: AtomicUsize,
}

#[derive(Clone, Default)]
#[frb(ignore)]
struct NativeParseProgressTracker {
    state: Arc<NativeParseProgressState>,
}

impl NativeParseProgressTracker {
    fn begin(&self, total_bytes: usize) {
        self.state.total_bytes.store(total_bytes, Ordering::Release);
        self.state.current_byte_offset.store(0, Ordering::Release);
    }

    fn update(&self, current_byte_offset: usize) {
        let total = self.state.total_bytes.load(Ordering::Acquire);
        self.state
            .current_byte_offset
            .fetch_max(min(current_byte_offset, total), Ordering::AcqRel);
    }

    fn complete(&self) {
        let total = self.state.total_bytes.load(Ordering::Acquire);
        self.state
            .current_byte_offset
            .store(total, Ordering::Release);
    }

    fn snapshot(&self) -> NativeParseProgress {
        NativeParseProgress {
            current_byte_offset: self.state.current_byte_offset.load(Ordering::Acquire),
            total_bytes: self.state.total_bytes.load(Ordering::Acquire),
        }
    }
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
    parse_progress: NativeParseProgressTracker,
}

#[derive(Clone)]
#[frb(opaque)]
pub struct NativeParseCancellation {
    cancelled: Arc<AtomicBool>,
    parse_progress: NativeParseProgressTracker,
}

impl NativeParseCancellation {
    #[frb(sync)]
    pub fn create() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            parse_progress: NativeParseProgressTracker::default(),
        }
    }

    #[frb(sync)]
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[frb(sync)]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    #[frb(sync)]
    pub fn progress(&self) -> NativeParseProgress {
        self.parse_progress.snapshot()
    }
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
            None,
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
        Self::open_with_rope(document_id, revision, rope, language_id, None)
    }

    /// Opens retained syntax state with cooperative Tree-sitter cancellation.
    /// A cancelled parse never publishes a partially initialized document.
    pub fn open_from_rope_cancellable(
        document_id: String,
        revision: u64,
        rope: &RopeBridge,
        language_id: String,
        cancellation: &NativeParseCancellation,
    ) -> Result<Self, String> {
        ensure_parse_not_cancelled(Some(cancellation))?;
        let rope = rope.rope.read().map_err(|_| "rope lock poisoned")?.clone();
        Self::open_with_rope(document_id, revision, rope, language_id, Some(cancellation))
    }

    fn open_with_rope(
        document_id: String,
        revision: u64,
        rope: RustRope,
        language_id: String,
        cancellation: Option<&NativeParseCancellation>,
    ) -> Result<Self, String> {
        ensure_parse_not_cancelled(cancellation)?;
        let parse_progress = cancellation
            .map(|value| value.parse_progress.clone())
            .unwrap_or_default();
        let resolved_language = resolve_native_language(&language_id);
        let normalized_language_id = resolved_language.canonical_id;
        let native_language = resolved_language.descriptor;

        let (parser, tree, query) = if let Some(native_language) = native_language {
            let mut parser = Parser::new();
            parser
                .set_language(&native_language.language)
                .map_err(|error| format!("failed to configure parser: {error}"))?;
            let tree = parse_rope(&mut parser, &rope, None, cancellation, &parse_progress)?;
            ensure_parse_not_cancelled(cancellation)?;
            let query = Query::new(&native_language.language, &native_language.highlight_query)
                .map_err(|error| format!("failed to compile highlight query: {error}"))?;
            ensure_parse_not_cancelled(cancellation)?;
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
            parse_progress,
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
            apply_edit(&mut state, edit, &self.parse_progress)?;
        }
        state.revision = new_revision;

        Ok(EditorDocumentRevision {
            document_id: state.document_id.clone(),
            revision: state.revision,
            applied: true,
        })
    }

    #[frb(sync)]
    pub fn parse_progress(&self) -> NativeParseProgress {
        self.parse_progress.snapshot()
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

    /// Finds a structural matching delimiter from the retained Tree-sitter tree.
    /// A supported response with `match_offset == -1` is authoritative (for example,
    /// a brace inside a string) and must not fall back to raw-text bracket scanning.
    #[frb(sync)]
    pub fn query_matching_bracket(
        &self,
        expected_revision: u64,
        target_offset: usize,
    ) -> Result<BracketMatchResponse, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "native document lock poisoned")?;

        if state.closed {
            return Ok(BracketMatchResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                supported: false,
                stale: true,
                match_offset: -1,
            });
        }
        if state.revision != expected_revision {
            return Ok(BracketMatchResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                supported: state.tree.is_some(),
                stale: true,
                match_offset: -1,
            });
        }
        let Some(tree) = &state.tree else {
            return Ok(BracketMatchResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                supported: false,
                stale: false,
                match_offset: -1,
            });
        };

        let match_offset = find_structural_matching_bracket(tree, &state.rope, target_offset)
            .map(|offset| offset as i64)
            .unwrap_or(-1);
        Ok(BracketMatchResponse {
            document_id: state.document_id.clone(),
            revision: state.revision,
            supported: true,
            stale: false,
            match_offset,
        })
    }

    /// Expands a scalar selection to the smallest strictly enclosing named syntax node.
    /// A supported response with negative offsets is authoritative: the current selection
    /// already covers the outermost named syntax node and cannot expand further.
    #[frb(sync)]
    pub fn query_structural_selection(
        &self,
        expected_revision: u64,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<StructuralSelectionResponse, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "native document lock poisoned")?;

        let unsupported = |stale: bool, supported: bool| StructuralSelectionResponse {
            document_id: state.document_id.clone(),
            revision: state.revision,
            supported,
            stale,
            start_offset: -1,
            end_offset: -1,
        };

        if state.closed {
            return Ok(unsupported(true, false));
        }
        if state.revision != expected_revision {
            return Ok(unsupported(true, state.tree.is_some()));
        }
        let Some(tree) = &state.tree else {
            return Ok(unsupported(false, false));
        };

        let start = min(start_offset, end_offset).min(state.rope.len_chars());
        let end = max(start_offset, end_offset).min(state.rope.len_chars());
        let selection = find_structural_selection_range(tree, &state.rope, start, end);
        let (start_offset, end_offset) = selection
            .map(|(start, end)| (start as i64, end as i64))
            .unwrap_or((-1, -1));

        Ok(StructuralSelectionResponse {
            document_id: state.document_id.clone(),
            revision: state.revision,
            supported: true,
            stale: false,
            start_offset,
            end_offset,
        })
    }

    /// Returns a bounded document outline from the already-retained Tree-sitter tree.
    /// Only declaration metadata and short symbol names cross FFI; the document text
    /// itself remains owned by the retained native Rope.
    pub fn query_document_symbols(
        &self,
        expected_revision: u64,
        max_symbols: usize,
    ) -> Result<DocumentSymbolsResponse, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "native document lock poisoned")?;

        if state.closed {
            return Ok(DocumentSymbolsResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                supported: false,
                stale: true,
                truncated: false,
                symbols: Vec::new(),
            });
        }

        if state.revision != expected_revision {
            return Ok(DocumentSymbolsResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                supported: state.tree.is_some(),
                stale: true,
                truncated: false,
                symbols: Vec::new(),
            });
        }

        let Some(tree) = &state.tree else {
            return Ok(DocumentSymbolsResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                supported: false,
                stale: false,
                truncated: false,
                symbols: Vec::new(),
            });
        };

        let limit = max_symbols.clamp(1, 5_000);
        let mut symbols = Vec::new();
        let truncated = collect_document_symbols(
            tree.root_node(),
            &state.rope,
            &state.language_id,
            0,
            limit,
            &mut symbols,
        );

        Ok(DocumentSymbolsResponse {
            document_id: state.document_id.clone(),
            revision: state.revision,
            supported: true,
            stale: false,
            truncated,
            symbols,
        })
    }

    /// Returns foldable structural ranges from the already-retained Tree-sitter tree.
    /// This intentionally does not rescan or materialize the Rope text.
    pub fn query_folding_ranges(
        &self,
        expected_revision: u64,
    ) -> Result<FoldingRangeResponse, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "native document lock poisoned")?;

        if state.closed {
            return Ok(FoldingRangeResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                supported: false,
                stale: true,
                ranges: Vec::new(),
            });
        }

        if state.revision != expected_revision {
            return Ok(FoldingRangeResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                supported: state.tree.is_some(),
                stale: true,
                ranges: Vec::new(),
            });
        }

        let Some(tree) = &state.tree else {
            return Ok(FoldingRangeResponse {
                document_id: state.document_id.clone(),
                revision: state.revision,
                supported: false,
                stale: false,
                ranges: Vec::new(),
            });
        };

        let mut ranges = Vec::new();
        collect_folding_ranges(tree.root_node(), &mut ranges);
        ranges.sort_by(|left, right| {
            left.start_line
                .cmp(&right.start_line)
                .then(right.end_line.cmp(&left.end_line))
        });
        ranges.dedup();

        let mut unique_starts = Vec::with_capacity(ranges.len());
        let mut last_start = None;
        for range in ranges {
            if last_start == Some(range.start_line) {
                continue;
            }
            last_start = Some(range.start_line);
            unique_starts.push(range);
        }

        Ok(FoldingRangeResponse {
            document_id: state.document_id.clone(),
            revision: state.revision,
            supported: true,
            stale: false,
            ranges: unique_starts,
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

fn find_structural_selection_range(
    tree: &Tree,
    rope: &RustRope,
    start_offset: usize,
    end_offset: usize,
) -> Option<(usize, usize)> {
    let len_chars = rope.len_chars();
    let start = min(start_offset, end_offset).min(len_chars);
    let end = max(start_offset, end_offset).min(len_chars);

    let (start_byte, end_byte) = if start < end {
        (rope.char_to_byte(start), rope.char_to_byte(end))
    } else if start < len_chars {
        (rope.char_to_byte(start), rope.char_to_byte(start + 1))
    } else if start > 0 {
        (rope.char_to_byte(start - 1), rope.char_to_byte(start))
    } else {
        (0, 0)
    };

    let root = tree.root_node();
    let mut node = root.named_descendant_for_byte_range(start_byte, end_byte)?;
    loop {
        let node_start = rope.byte_to_char(node.start_byte());
        let node_end = rope.byte_to_char(node.end_byte());
        if node_start <= start && node_end >= end && (node_start < start || node_end > end) {
            return Some((node_start, node_end));
        }
        node = node.parent()?;
        while !node.is_named() {
            node = node.parent()?;
        }
    }
}

fn collect_document_symbols(
    node: tree_sitter::Node<'_>,
    rope: &RustRope,
    language_id: &str,
    depth: usize,
    limit: usize,
    symbols: &mut Vec<NativeDocumentSymbol>,
) -> bool {
    if symbols.len() >= limit {
        return true;
    }

    let symbol_kind = symbol_kind_for_node(language_id, node.kind());
    let mut child_depth = depth;
    if let Some(kind) = symbol_kind {
        if let Some(name_node) = symbol_name_node(node) {
            let start_offset = rope.byte_to_char(node.start_byte());
            let end_offset = rope.byte_to_char(node.end_byte());
            let selection_start_offset = rope.byte_to_char(name_node.start_byte());
            let selection_end_offset = rope.byte_to_char(name_node.end_byte());
            if selection_start_offset < selection_end_offset {
                let name =
                    bounded_rope_text(rope, selection_start_offset, selection_end_offset, 256);
                if !name.is_empty() {
                    symbols.push(NativeDocumentSymbol {
                        name,
                        kind: kind.to_string(),
                        start_offset,
                        end_offset,
                        selection_start_offset,
                        selection_end_offset,
                        start_line: node.start_position().row,
                        end_line: node.end_position().row,
                        depth,
                    });
                    child_depth = depth + 1;
                    if symbols.len() >= limit {
                        return node.named_child_count() > 0;
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if collect_document_symbols(child, rope, language_id, child_depth, limit, symbols) {
            return true;
        }
    }
    false
}

fn symbol_kind_for_node(_language_id: &str, kind: &str) -> Option<&'static str> {
    match kind {
        "function_item" | "function_definition" | "function_declaration" | "function_signature" => {
            Some("function")
        }
        "method_definition"
        | "method_signature"
        | "getter_signature"
        | "setter_signature"
        | "constructor_signature"
        | "constructor_declaration" => Some("method"),
        "class_definition" | "class_declaration" => Some("class"),
        "struct_item" => Some("struct"),
        "enum_item" | "enum_declaration" => Some("enum"),
        "trait_item" => Some("trait"),
        "interface_declaration" => Some("interface"),
        "mixin_declaration" => Some("mixin"),
        "extension_declaration" => Some("extension"),
        "impl_item" => Some("implementation"),
        "mod_item" | "module" => Some("module"),
        "type_item" | "type_alias_declaration" | "type_alias" => Some("type"),
        "const_item" | "static_item" => Some("constant"),
        "macro_definition" => Some("macro"),
        _ => None,
    }
}

fn symbol_name_node(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    node.child_by_field_name("name")
        .or_else(|| {
            if node.kind() == "impl_item" {
                node.child_by_field_name("type")
            } else {
                None
            }
        })
        .or_else(|| first_identifier_child(node))
}

fn first_identifier_child(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier" | "type_identifier" | "field_identifier" | "property_identifier"
        ) {
            return Some(child);
        }
    }
    None
}

fn bounded_rope_text(rope: &RustRope, start: usize, end: usize, max_chars: usize) -> String {
    let safe_start = start.min(rope.len_chars());
    let safe_end = end.min(rope.len_chars()).max(safe_start);
    let bounded_end = safe_start + (safe_end - safe_start).min(max_chars);
    rope.slice(safe_start..bounded_end)
        .to_string()
        .trim()
        .to_string()
}

fn find_structural_matching_bracket(
    tree: &Tree,
    rope: &RustRope,
    target_offset: usize,
) -> Option<usize> {
    if target_offset >= rope.len_chars() {
        return None;
    }
    let target = rope.char(target_offset);
    let counterpart = match target {
        '{' => '}',
        '}' => '{',
        '[' => ']',
        ']' => '[',
        '(' => ')',
        ')' => '(',
        _ => return None,
    };

    let start_byte = rope.char_to_byte(target_offset);
    let end_byte = rope.char_to_byte(target_offset + 1);
    let node = tree
        .root_node()
        .descendant_for_byte_range(start_byte, end_byte)?;
    if node.start_byte() != start_byte
        || node.end_byte() != end_byte
        || node.kind().chars().next() != Some(target)
        || node.kind().chars().count() != 1
    {
        return None;
    }

    let parent = node.parent()?;
    for index in 0..parent.child_count() {
        let Some(sibling) = parent.child(index as u32) else {
            continue;
        };
        if sibling.id() == node.id() || sibling.kind().chars().count() != 1 {
            continue;
        }
        if sibling.kind().chars().next() == Some(counterpart) {
            return Some(rope.byte_to_char(sibling.start_byte()));
        }
    }
    None
}

fn collect_folding_ranges(node: tree_sitter::Node<'_>, ranges: &mut Vec<NativeFoldingRange>) {
    if node.is_named() && is_foldable_node_kind(node.kind()) {
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;
        if start_line < end_line {
            ranges.push(NativeFoldingRange {
                start_line,
                end_line,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_folding_ranges(child, ranges);
    }
}

fn is_foldable_node_kind(kind: &str) -> bool {
    matches!(
        kind,
        "block"
            | "statement_block"
            | "class_body"
            | "switch_body"
            | "declaration_list"
            | "field_declaration_list"
            | "enum_variant_list"
            | "match_block"
            | "object"
            | "array"
            | "object_pattern"
            | "array_pattern"
            | "object_type"
            | "interface_body"
            | "type_parameters"
            | "arguments"
            | "argument_list"
            | "parameters"
            | "formal_parameters"
            | "list"
            | "list_pattern"
            | "dictionary"
            | "set"
            | "tuple"
            | "parenthesized_expression"
    )
}

fn apply_edit(
    state: &mut NativeEditorDocumentState,
    edit: EditorDocumentEdit,
    parse_progress: &NativeParseProgressTracker,
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
        state.tree = Some(parse_rope(
            parser,
            &state.rope,
            previous_tree,
            None,
            parse_progress,
        )?);
    }

    Ok(())
}

fn ensure_parse_not_cancelled(
    cancellation: Option<&NativeParseCancellation>,
) -> Result<(), String> {
    if cancellation.is_some_and(NativeParseCancellation::is_cancelled) {
        Err(NATIVE_PARSE_CANCELLED_ERROR.to_string())
    } else {
        Ok(())
    }
}

fn parse_rope(
    parser: &mut Parser,
    rope: &RustRope,
    old_tree: Option<&Tree>,
    cancellation: Option<&NativeParseCancellation>,
    progress_tracker: &NativeParseProgressTracker,
) -> Result<Tree, String> {
    ensure_parse_not_cancelled(cancellation)?;
    progress_tracker.begin(rope.len_bytes());
    let mut input = |byte_offset, _position| {
        if byte_offset >= rope.len_bytes() {
            return &[][..];
        }
        let (chunk, chunk_byte_idx, _, _) = rope.chunk_at_byte(byte_offset);
        &chunk.as_bytes()[byte_offset - chunk_byte_idx..]
    };

    let mut progress = |state: &tree_sitter::ParseState| {
        progress_tracker.update(state.current_byte_offset());
        if cancellation.is_some_and(NativeParseCancellation::is_cancelled) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let options = ParseOptions::new().progress_callback(&mut progress);
    let tree = parser.parse_with_options(&mut input, old_tree, Some(options));

    ensure_parse_not_cancelled(cancellation)?;
    let tree = tree.ok_or_else(|| "tree-sitter parser returned no tree".to_string())?;
    progress_tracker.complete();
    Ok(tree)
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
    use tree_sitter::Language;

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
    fn cancellable_open_stops_before_retaining_parser_state() {
        let text = (0..20_000)
            .map(|index| format!("fn item_{index}() {{ let value = {index}; }}\n"))
            .collect::<String>();
        let rope = RopeBridge::create(text);
        let cancellation = NativeParseCancellation::create();
        cancellation.cancel();

        let result = NativeEditorDocument::open_from_rope_cancellable(
            "doc-cancelled-open".to_string(),
            1,
            &rope,
            "rust".to_string(),
            &cancellation,
        );

        match result {
            Ok(_) => panic!("cancelled parse must not publish a retained document"),
            Err(error) => assert_eq!(error, NATIVE_PARSE_CANCELLED_ERROR),
        }
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn cancellable_parse_interrupts_work_after_start() {
        let text = (0..100_000)
            .map(|index| format!("fn item_{index}() {{ let value = {index}; }}\n"))
            .collect::<String>();
        let rope = RustRope::from_str(&text);
        let cancellation = NativeParseCancellation::create();
        let canceller = cancellation.clone();
        let cancel_thread = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2));
            canceller.cancel();
        });

        let language: Language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let result = parse_rope(
            &mut parser,
            &rope,
            None,
            Some(&cancellation),
            &cancellation.parse_progress,
        );
        cancel_thread.join().unwrap();

        assert_eq!(result.unwrap_err(), NATIVE_PARSE_CANCELLED_ERROR);
        assert!(cancellation.is_cancelled());
        let progress = cancellation.progress();
        assert!(progress.total_bytes > 0);
        assert!(progress.current_byte_offset <= progress.total_bytes);
    }

    #[test]
    fn reports_completed_byte_progress_for_initial_and_incremental_parse() {
        let text = "fn main() {\n    let value = 1;\n}\n";
        let document = NativeEditorDocument::open(
            "doc-progress".to_string(),
            1,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let initial = document.parse_progress();
        assert_eq!(initial.current_byte_offset, text.len());
        assert_eq!(initial.total_bytes, text.len());

        let start = char_offset(text, "1");
        document
            .apply_edits(
                1,
                2,
                vec![EditorDocumentEdit {
                    start,
                    end: start + 1,
                    replacement: "123".to_string(),
                }],
            )
            .unwrap();

        let updated = document.parse_progress();
        assert_eq!(updated.current_byte_offset, updated.total_bytes);
        assert_eq!(updated.total_bytes, text.len() + 2);
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
    fn deferred_rope_snapshot_applies_pending_edit_exactly_once() {
        let original = "fn main() { let value = 1; }\n";
        let live_rope = RopeBridge::create(original.to_string());
        let parse_baseline = live_rope.deep_clone();
        let edit_start = char_offset(original, "1");
        let replacement = "12345";

        live_rope.replace_range_and_update_selection(
            edit_start,
            edit_start + 1,
            replacement.to_string(),
            false,
            edit_start,
            edit_start,
        );

        let document = NativeEditorDocument::open_from_rope(
            "doc-deferred-baseline".to_string(),
            10,
            &parse_baseline,
            "rust".to_string(),
        )
        .unwrap();
        assert_eq!(document.info().unwrap().len_chars, original.chars().count());

        let result = document
            .apply_edits(
                10,
                11,
                vec![EditorDocumentEdit {
                    start: edit_start,
                    end: edit_start + 1,
                    replacement: replacement.to_string(),
                }],
            )
            .unwrap();
        assert!(result.applied);
        assert_eq!(result.revision, 11);
        assert_eq!(
            document.info().unwrap().len_chars,
            live_rope.len_chars(),
            "the queued delta must advance the immutable baseline exactly once"
        );
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
    fn retained_tree_structural_selection_expands_through_named_nodes() {
        let text = "fn main() {\n    let value = (1 + 2);\n}\n";
        let document = NativeEditorDocument::open(
            "doc-structural-selection".to_string(),
            3,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let caret = char_offset(text, "1 + 2");
        let first = document
            .query_structural_selection(3, caret, caret)
            .unwrap();
        assert!(first.supported);
        assert!(!first.stale);
        assert!(first.start_offset >= 0);
        assert!(first.end_offset > first.start_offset);
        assert!(first.start_offset as usize <= caret);
        assert!(first.end_offset as usize > caret);

        let second = document
            .query_structural_selection(3, first.start_offset as usize, first.end_offset as usize)
            .unwrap();
        assert!(second.supported);
        assert!(!second.stale);
        assert!(
            second.start_offset < first.start_offset || second.end_offset > first.end_offset,
            "second expansion must strictly contain the first"
        );
    }

    #[test]
    fn retained_tree_structural_selection_uses_unicode_scalar_offsets() {
        let text = "fn main() { let value = \"🙂text\"; }\n";
        let document = NativeEditorDocument::open(
            "doc-structural-selection-unicode".to_string(),
            5,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();
        let caret = char_offset(text, "text");

        let response = document
            .query_structural_selection(5, caret, caret)
            .unwrap();
        assert!(response.supported);
        assert!(!response.stale);
        assert!(response.start_offset >= 0);
        assert!(response.start_offset as usize <= caret);
        assert!(response.end_offset as usize > caret);
    }

    #[test]
    fn retained_tree_structural_selection_rejects_stale_revision_and_plaintext() {
        let rust_document = NativeEditorDocument::open(
            "doc-structural-selection-stale".to_string(),
            5,
            "fn main() {}\n".to_string(),
            "rust".to_string(),
        )
        .unwrap();
        let stale = rust_document.query_structural_selection(4, 3, 3).unwrap();
        assert!(stale.supported);
        assert!(stale.stale);
        assert_eq!((stale.start_offset, stale.end_offset), (-1, -1));

        let plain_document = NativeEditorDocument::open(
            "doc-structural-selection-plain".to_string(),
            1,
            "plain text\n".to_string(),
            "plaintext".to_string(),
        )
        .unwrap();
        let unsupported = plain_document.query_structural_selection(1, 2, 2).unwrap();
        assert!(!unsupported.supported);
        assert!(!unsupported.stale);
        assert_eq!((unsupported.start_offset, unsupported.end_offset), (-1, -1));
    }

    #[test]
    fn retained_tree_bracket_matching_uses_structural_delimiters() {
        let text = "fn main() {\n    let values = [1, 2, 3];\n}\n";
        let document = NativeEditorDocument::open(
            "doc-brackets".to_string(),
            4,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let open_brace = char_offset(text, "{");
        let close_brace = text[..text.rfind('}').unwrap()].chars().count();
        let brace_response = document.query_matching_bracket(4, open_brace).unwrap();
        assert!(brace_response.supported);
        assert!(!brace_response.stale);
        assert_eq!(brace_response.match_offset, close_brace as i64);

        let open_array = char_offset(text, "[");
        let close_array = text[..text.find(']').unwrap()].chars().count();
        let array_response = document.query_matching_bracket(4, open_array).unwrap();
        assert_eq!(array_response.match_offset, close_array as i64);
    }

    #[test]
    fn retained_tree_bracket_matching_does_not_match_string_delimiters() {
        let text = "fn main() {\n    let fake = \"{ not structural }\";\n}\n";
        let document = NativeEditorDocument::open(
            "doc-bracket-string".to_string(),
            2,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let string_brace = char_offset(text, "{ not structural");
        let response = document.query_matching_bracket(2, string_brace).unwrap();
        assert!(response.supported);
        assert!(!response.stale);
        assert_eq!(response.match_offset, -1);
    }

    #[test]
    fn retained_tree_bracket_matching_rejects_stale_revision_and_plaintext() {
        let rust_document = NativeEditorDocument::open(
            "doc-bracket-stale".to_string(),
            5,
            "fn main() {}\n".to_string(),
            "rust".to_string(),
        )
        .unwrap();
        let stale = rust_document.query_matching_bracket(4, 10).unwrap();
        assert!(stale.supported);
        assert!(stale.stale);
        assert_eq!(stale.match_offset, -1);

        let plain_document = NativeEditorDocument::open(
            "doc-bracket-plain".to_string(),
            1,
            "{plain}\n".to_string(),
            "plaintext".to_string(),
        )
        .unwrap();
        let unsupported = plain_document.query_matching_bracket(1, 0).unwrap();
        assert!(!unsupported.supported);
        assert!(!unsupported.stale);
        assert_eq!(unsupported.match_offset, -1);
    }

    #[test]
    fn retained_tree_folding_ignores_braces_inside_strings() {
        let text = "fn main() {\n    let fake = \"{\\nnot a block\\n}\";\n    if true {\n        println!(\"ok\");\n    }\n}\n";
        let document = NativeEditorDocument::open(
            "doc-folds".to_string(),
            3,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let response = document.query_folding_ranges(3).unwrap();
        assert!(response.supported);
        assert!(!response.stale);
        assert!(response.ranges.contains(&NativeFoldingRange {
            start_line: 0,
            end_line: 5,
        }));
        assert!(response.ranges.contains(&NativeFoldingRange {
            start_line: 2,
            end_line: 4,
        }));
        assert!(!response.ranges.iter().any(|range| range.start_line == 1));
    }

    #[test]
    fn retained_tree_folding_obeys_revision_and_incremental_edits() {
        let text = "fn main() {\n    if true {\n        println!(\"ok\");\n    }\n}\n";
        let document = NativeEditorDocument::open(
            "doc-fold-edit".to_string(),
            7,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let stale = document.query_folding_ranges(6).unwrap();
        assert!(stale.supported);
        assert!(stale.stale);
        assert!(stale.ranges.is_empty());

        let start = char_offset(text, "    if true {");
        let end = char_offset(text, "}\n") + 2;
        let result = document
            .apply_edits(
                7,
                8,
                vec![EditorDocumentEdit {
                    start,
                    end,
                    replacement: String::new(),
                }],
            )
            .unwrap();
        assert!(result.applied);

        let response = document.query_folding_ranges(8).unwrap();
        assert!(!response.stale);
        assert_eq!(response.ranges.len(), 1);
        assert_eq!(response.ranges[0].start_line, 0);
    }

    #[test]
    fn retained_tree_folding_falls_back_cleanly_for_plaintext() {
        let document = NativeEditorDocument::open(
            "doc-fold-plain".to_string(),
            1,
            "hello {\nworld\n}\n".to_string(),
            "plaintext".to_string(),
        )
        .unwrap();

        let response = document.query_folding_ranges(1).unwrap();
        assert!(!response.supported);
        assert!(!response.stale);
        assert!(response.ranges.is_empty());
    }

    #[test]
    fn retained_tree_symbols_preserve_rust_hierarchy_and_scalar_ranges() {
        let text = "struct User { name: String }\nimpl User { fn display(&self) -> &str { &self.name } }\nfn top_level() {}\n";
        let document = NativeEditorDocument::open(
            "doc-symbols-rust".to_string(),
            9,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let response = document.query_document_symbols(9, 100).unwrap();
        assert!(response.supported);
        assert!(!response.stale);
        assert!(!response.truncated);

        let user = response
            .symbols
            .iter()
            .find(|symbol| symbol.name == "User" && symbol.kind == "struct")
            .expect("struct symbol");
        assert_eq!(user.depth, 0);
        assert_eq!(user.selection_start_offset, char_offset(text, "User"));

        let display = response
            .symbols
            .iter()
            .find(|symbol| symbol.name == "display")
            .expect("method symbol");
        assert_eq!(display.kind, "function");
        assert_eq!(display.depth, 1);
        assert_eq!(display.selection_start_offset, char_offset(text, "display"));

        let top_level = response
            .symbols
            .iter()
            .find(|symbol| symbol.name == "top_level")
            .expect("top-level function symbol");
        assert_eq!(top_level.depth, 0);
    }

    #[test]
    fn retained_tree_symbols_cover_typescript_classes_methods_and_interfaces() {
        let text = "interface Shape { area(): number; }\nclass Circle { area(): number { return 1; } }\nfunction helper() {}\n";
        let document = NativeEditorDocument::open(
            "doc-symbols-ts".to_string(),
            4,
            text.to_string(),
            "typescript".to_string(),
        )
        .unwrap();

        let response = document.query_document_symbols(4, 100).unwrap();
        assert!(response.supported);
        assert!(!response.stale);

        assert!(response
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Shape" && symbol.kind == "interface"));
        let circle = response
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Circle")
            .expect("class symbol");
        assert_eq!(circle.kind, "class");
        assert_eq!(circle.depth, 0);
        let area = response
            .symbols
            .iter()
            .find(|symbol| symbol.name == "area" && symbol.depth == 1)
            .expect("method symbol");
        assert_eq!(area.kind, "method");
        assert!(response
            .symbols
            .iter()
            .any(|symbol| symbol.name == "helper" && symbol.kind == "function"));
    }

    #[test]
    fn retained_tree_symbols_cover_dart_classes_methods_and_functions() {
        let text = "class Greeter { String greet() { return 'hi'; } }\nString helper() => 'ok';\n";
        let document = NativeEditorDocument::open(
            "doc-symbols-dart".to_string(),
            6,
            text.to_string(),
            "dart".to_string(),
        )
        .unwrap();

        let response = document.query_document_symbols(6, 100).unwrap();
        assert!(response.supported);
        assert!(!response.stale);
        let greeter = response
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Greeter")
            .expect("class symbol");
        assert_eq!(greeter.kind, "class");
        assert_eq!(greeter.depth, 0);
        let greet = response
            .symbols
            .iter()
            .find(|symbol| symbol.name == "greet")
            .expect("method symbol");
        assert_eq!(greet.depth, 1);
        assert!(response
            .symbols
            .iter()
            .any(|symbol| symbol.name == "helper" && symbol.depth == 0));
    }

    #[test]
    fn retained_tree_symbols_use_unicode_scalar_offsets_and_obey_revision() {
        let text = "fn café() {}\nfn second() {}\n";
        let document = NativeEditorDocument::open(
            "doc-symbols-unicode".to_string(),
            3,
            text.to_string(),
            "rust".to_string(),
        )
        .unwrap();

        let response = document.query_document_symbols(3, 100).unwrap();
        let cafe = response
            .symbols
            .iter()
            .find(|symbol| symbol.name == "café")
            .expect("unicode symbol");
        assert_eq!(cafe.selection_start_offset, char_offset(text, "café"));
        assert_eq!(cafe.selection_end_offset - cafe.selection_start_offset, 4);

        let stale = document.query_document_symbols(2, 100).unwrap();
        assert!(stale.supported);
        assert!(stale.stale);
        assert!(stale.symbols.is_empty());

        let limited = document.query_document_symbols(3, 1).unwrap();
        assert!(limited.truncated);
        assert_eq!(limited.symbols.len(), 1);
    }

    #[test]
    fn retained_tree_symbols_fall_back_cleanly_for_plaintext() {
        let document = NativeEditorDocument::open(
            "doc-symbols-plain".to_string(),
            1,
            "class Fake {}\n".to_string(),
            "plaintext".to_string(),
        )
        .unwrap();

        let response = document.query_document_symbols(1, 100).unwrap();
        assert!(!response.supported);
        assert!(!response.stale);
        assert!(!response.truncated);
        assert!(response.symbols.is_empty());
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
