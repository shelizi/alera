# Native editor syntax highlighting

## Scope

The GPUI NativeEditor retains ownership of its EditorDocument, tabs, undo/redo,
IME composition and two-pane state. Syntax highlighting is an optional
read-only presentation layer. It must not mutate document bytes, selection,
marked text, or caret geometry.

## Data flow

1. NativeEditor identifies the active tab by its stable tab instance ID.
2. SyntaxHighlightCache compares tab identity, path, language override and
   document contents. Cursor movement, selection changes and scrolling do not
   trigger parsing.
3. SyntaxBackend receives SyntaxRequest with source, optional file path and
   optional explicit language.
4. SyntectBackend resolves a bat/two-face syntax, parses in document order
   so multiline scopes remain valid, and returns SyntaxSnapshot with
   per-line, UTF-8 byte-relative foreground spans.
5. compose_line_styles merges those foreground spans with selection and IME
   markers without overlapping ranges. NativeEditor maps the result to GPUI
   HighlightStyle in its virtualized row renderer.

Unknown extensions and untitled buffers without an explicit language fall
back to plain text. Syntax parser errors do not prevent editing or saving.
The current native EditorDocument limit is 256 KiB, bounding a full parse
after each content change. A larger document cap requires incremental parsing
or asynchronous work and profiling before deployment.

## Adding another highlighting backend

Implement the UI-independent SyntaxBackend trait in
src/syntax_highlighting.rs. Return the same SyntaxSnapshot shape and keep all
ranges within the original UTF-8 line, excluding the newline byte. Create a
SyntaxHighlightCache with Box<dyn SyntaxBackend> and leave the editor renderer
unchanged.

For Tree-sitter, translate captures into the same SyntaxSpan foreground data,
maintain parser/tree state inside the new backend or cache implementation,
and invalidate it for each relevant content edit. Grammar capture priorities,
language-specific injections, and theme-to-scope mapping remain backend
concerns. This change does not enable Tree-sitter grammars or an LSP.

## Validation

Run:

    cargo test --manifest-path desktop_gpui/Cargo.toml --lib syntax_highlighting
    cargo check --manifest-path desktop_gpui/Cargo.toml --bin alera-gpui-desktop

Test cases cover language discovery, multiline comment scopes, UTF-8 offsets,
unknown syntax fallback, cache invalidation, and selection/IME overlays.
Windows GUI acceptance should additionally exercise Traditional Chinese IME,
dual panes, file switch, undo/redo and a large file near the document limit.
