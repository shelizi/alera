//! UI-independent syntax highlighting boundary for the native GPUI editor.
//! The renderer consumes byte-range foreground spans, not parser-specific
//! syntax nodes. A Tree-sitter backend can implement SyntaxBackend later.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use two_face::re_exports::syntect::{
    easy::HighlightLines,
    highlighting::Theme,
    parsing::{SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

/// A half-open UTF-8 byte range relative to one displayed line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxSpan {
    pub range: Range<usize>,
    pub rgb: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyntaxSnapshot {
    language: Option<String>,
    lines: Vec<Vec<SyntaxSpan>>,
}

impl SyntaxSnapshot {
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    pub fn line(&self, index: usize) -> &[SyntaxSpan] {
        self.lines.get(index).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// The language override is intentionally independent of file names so
/// future manual language selection and structural backends share one API.
pub struct SyntaxRequest<'a> {
    pub text: &'a str,
    pub path: Option<&'a Path>,
    pub language: Option<&'a str>,
}

pub trait SyntaxBackend: Send {
    fn highlight(&mut self, request: SyntaxRequest<'_>) -> SyntaxSnapshot;
}

#[derive(Default)]
pub struct SyntectBackend;

fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

fn theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| two_face::theme::extra()[two_face::theme::EmbeddedThemeName::Nord].clone())
}

fn find_syntax<'a>(
    set: &'a SyntaxSet,
    path: Option<&Path>,
    language: Option<&str>,
) -> Option<&'a SyntaxReference> {
    if let Some(language) = language {
        if language.eq_ignore_ascii_case("plaintext") || language.eq_ignore_ascii_case("text") {
            return None;
        }
        return set
            .find_syntax_by_name(language)
            .or_else(|| set.find_syntax_by_token(language))
            .or_else(|| set.find_syntax_by_extension(language));
    }

    let path = path?;
    let name = path.file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();
    if lower == "cargo.lock" {
        return set.find_syntax_by_extension("toml");
    }
    if lower.starts_with("dockerfile") {
        return set.find_syntax_by_name("Dockerfile");
    }
    if lower == ".env" || lower.starts_with(".env.") {
        return set.find_syntax_by_name("DotENV");
    }
    if lower == "makefile" || lower == "gnumakefile" {
        return set.find_syntax_by_name("Makefile");
    }
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        if let Some(syntax) = set.find_syntax_by_extension(&extension.to_ascii_lowercase()) {
            return Some(syntax);
        }
    }
    set.find_syntax_by_token(name)
}

impl SyntaxBackend for SyntectBackend {
    fn highlight(&mut self, request: SyntaxRequest<'_>) -> SyntaxSnapshot {
        let set = syntaxes();
        let syntax = find_syntax(set, request.path, request.language);
        let mut snapshot = SyntaxSnapshot {
            language: syntax.map(|syntax| syntax.name.clone()),
            lines: vec![Vec::new(); request.text.bytes().filter(|byte| *byte == b'\n').count() + 1],
        };
        let Some(syntax) = syntax else {
            return snapshot;
        };

        let mut highlighter = HighlightLines::new(syntax, theme());
        for (index, line) in LinesWithEndings::from(request.text).enumerate() {
            let Ok(fragments) = highlighter.highlight_line(line, set) else {
                continue;
            };
            let visible_end = line.strip_suffix('\n').unwrap_or(line).len();
            let mut offset = 0;
            let spans = &mut snapshot.lines[index];
            for (style, fragment) in fragments {
                let start = offset;
                offset += fragment.len();
                let end = offset.min(visible_end);
                if start >= end {
                    continue;
                }
                let color = style.foreground;
                let rgb = u32::from(color.r) << 16 | u32::from(color.g) << 8 | u32::from(color.b);
                if let Some(previous) = spans.last_mut() {
                    if previous.rgb == rgb && previous.range.end == start {
                        previous.range.end = end;
                        continue;
                    }
                }
                spans.push(SyntaxSpan {
                    range: start..end,
                    rgb,
                });
            }
        }
        snapshot
    }
}

/// One cache per native editor surface; no parser state leaks between panes or
/// tab instances. With EditorDocument's 256 KiB limit, content comparison is
/// bounded and avoids reparsing on cursor moves, selection or scrolling.
pub struct SyntaxHighlightCache {
    backend: Box<dyn SyntaxBackend>,
    tab: Option<u64>,
    path: Option<PathBuf>,
    language: Option<String>,
    source: String,
    snapshot: SyntaxSnapshot,
}

impl Default for SyntaxHighlightCache {
    fn default() -> Self {
        Self::new(Box::new(SyntectBackend))
    }
}

impl SyntaxHighlightCache {
    pub fn new(backend: Box<dyn SyntaxBackend>) -> Self {
        Self {
            backend,
            tab: None,
            path: None,
            language: None,
            source: String::new(),
            snapshot: SyntaxSnapshot::default(),
        }
    }

    pub fn refresh(&mut self, tab: u64, source: &str, path: Option<&Path>, language: Option<&str>) {
        if self.tab == Some(tab)
            && self.source == source
            && self.path.as_deref() == path
            && self.language.as_deref() == language
        {
            return;
        }
        self.snapshot = self.backend.highlight(SyntaxRequest {
            text: source,
            path,
            language,
        });
        self.tab = Some(tab);
        self.source.clear();
        self.source.push_str(source);
        self.path = path.map(Path::to_path_buf);
        self.language = language.map(str::to_owned);
    }

    pub fn snapshot(&self) -> &SyntaxSnapshot {
        &self.snapshot
    }
}

/// Foreground and selection/composition are combined before GPUI StyledText
/// receives them. This prevents overlapping highlights from silently losing
/// the selection background or the syntax foreground.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplaySpan {
    pub range: Range<usize>,
    pub rgb: Option<u32>,
    pub selected: bool,
    pub composing: bool,
}

pub fn compose_line_styles(
    text: &str,
    syntax: &[SyntaxSpan],
    selection: Option<Range<usize>>,
    composition: Option<Range<usize>>,
) -> Vec<DisplaySpan> {
    let len = text.len();
    let valid = |r: &Range<usize>| {
        r.start < r.end
            && r.end <= len
            && text.is_char_boundary(r.start)
            && text.is_char_boundary(r.end)
    };
    let selection = selection.filter(&valid);
    let composition = composition.filter(&valid);
    let mut syntax: Vec<_> = syntax.iter().filter(|s| valid(&s.range)).collect();
    syntax.sort_unstable_by_key(|span| (span.range.start, span.range.end));
    let mut cuts = vec![0, len];
    for span in &syntax {
        cuts.extend([span.range.start, span.range.end]);
    }
    for range in selection.iter().chain(composition.iter()) {
        cuts.extend([range.start, range.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();

    let mut output: Vec<DisplaySpan> = Vec::new();
    let mut syntax_index = 0;
    for edge in cuts.windows(2) {
        let (start, end) = (edge[0], edge[1]);
        if start == end {
            continue;
        }
        while syntax
            .get(syntax_index)
            .is_some_and(|span| span.range.end <= start)
        {
            syntax_index += 1;
        }
        let rgb = syntax
            .get(syntax_index)
            .filter(|span| span.range.start <= start && end <= span.range.end)
            .map(|span| span.rgb);
        let selected = selection
            .as_ref()
            .is_some_and(|range| range.start <= start && end <= range.end);
        let composing = composition
            .as_ref()
            .is_some_and(|range| range.start <= start && end <= range.end);
        if rgb.is_none() && !selected && !composing {
            continue;
        }
        if let Some(previous) = output.last_mut() {
            if previous.range.end == start
                && previous.rgb == rgb
                && previous.selected == selected
                && previous.composing == composing
            {
                previous.range.end = end;
                continue;
            }
        }
        output.push(DisplaySpan {
            range: start..end,
            rgb,
            selected,
            composing,
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn recognizes_common_languages_and_special_filenames() {
        let mut backend = SyntectBackend;
        for name in [
            "main.rs",
            "app.ts",
            "view.tsx",
            "main.dart",
            "index.php",
            "config.json",
            "schema.yaml",
            "Cargo.toml",
            "Dockerfile",
            "Dockerfile.dev",
            "Cargo.lock",
            "app.py",
            "app.go",
        ] {
            let snapshot = backend.highlight(SyntaxRequest {
                text: "let value = 123;\n",
                path: Some(Path::new(name)),
                language: None,
            });
            assert!(snapshot.language().is_some(), "no grammar for {name}");
        }
    }

    #[test]
    fn unknown_extension_remains_plain_text() {
        let snapshot = SyntectBackend.highlight(SyntaxRequest {
            text: "hello\nworld\n",
            path: Some(Path::new("example.unknown-alera-extension")),
            language: None,
        });
        assert_eq!(snapshot.language(), None);
        assert_eq!(snapshot.line_count(), 3);
        assert!(snapshot.line(0).is_empty());
    }

    #[test]
    fn retains_multiline_comment_state_and_newline_alignment() {
        let snapshot = SyntectBackend.highlight(SyntaxRequest {
            text: "/* begin\nstill a comment */\nlet value = 1;\n",
            path: Some(Path::new("demo.rs")),
            language: None,
        });
        assert_eq!(snapshot.line_count(), 4);
        assert!(!snapshot.line(0).is_empty());
        assert!(!snapshot.line(1).is_empty());
        assert_ne!(snapshot.line(1)[0].rgb, snapshot.line(2)[0].rgb);
        assert!(snapshot.line(3).is_empty());
    }

    #[test]
    fn all_highlight_ranges_follow_utf8_boundaries() {
        let text = "const 名稱 = '中文';\n";
        let snapshot = SyntectBackend.highlight(SyntaxRequest {
            text,
            path: Some(Path::new("demo.ts")),
            language: None,
        });
        let line = text.trim_end_matches('\n');
        for span in snapshot.line(0) {
            assert!(line.is_char_boundary(span.range.start));
            assert!(line.is_char_boundary(span.range.end));
            assert!(span.range.end <= line.len());
        }
    }

    struct CountingBackend(Arc<AtomicUsize>);
    impl SyntaxBackend for CountingBackend {
        fn highlight(&mut self, request: SyntaxRequest<'_>) -> SyntaxSnapshot {
            self.0.fetch_add(1, Ordering::Relaxed);
            SyntaxSnapshot {
                language: request.path.map(|p| p.to_string_lossy().into_owned()),
                lines: vec![vec![]],
            }
        }
    }

    #[test]
    fn cache_only_reparses_when_document_identity_text_or_language_changes() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut cache = SyntaxHighlightCache::new(Box::new(CountingBackend(counter.clone())));
        cache.refresh(1, "fn main() {}", Some(Path::new("one.rs")), None);
        cache.refresh(1, "fn main() {}", Some(Path::new("one.rs")), None);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        cache.refresh(1, "fn main() { }", Some(Path::new("one.rs")), None);
        cache.refresh(2, "fn main() { }", Some(Path::new("one.rs")), None);
        cache.refresh(2, "fn main() { }", Some(Path::new("one.ts")), None);
        cache.refresh(2, "fn main() { }", Some(Path::new("one.ts")), Some("Rust"));
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn selection_and_composition_keep_syntax_color_and_utf8_offsets() {
        let text = "a中b";
        let syntax = vec![SyntaxSpan {
            range: 0..text.len(),
            rgb: 0x123456,
        }];
        let segments = compose_line_styles(text, &syntax, Some(1..4), None);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[1].range, 1..4);
        assert!(segments[1].selected);
        assert_eq!(segments[1].rgb, Some(0x123456));
        let composing = compose_line_styles(text, &syntax, None, Some(1..4));
        assert!(composing[1].composing);
        assert_eq!(composing[1].rgb, Some(0x123456));
    }

    #[test]
    fn invalid_backend_ranges_are_discarded_without_panic() {
        let text = "中";
        let styled = compose_line_styles(
            text,
            &[SyntaxSpan {
                range: 1..2,
                rgb: 0xff0000,
            }],
            Some(0..3),
            None,
        );
        assert_eq!(styled.len(), 1);
        assert_eq!(styled[0].range, 0..3);
        assert!(styled[0].selected);
        assert_eq!(styled[0].rgb, None);
    }

    #[test]
    fn out_of_order_backend_spans_are_sorted_before_composition() {
        let styled = compose_line_styles(
            "abcdef",
            &[
                SyntaxSpan {
                    range: 3..6,
                    rgb: 0x222222,
                },
                SyntaxSpan {
                    range: 0..3,
                    rgb: 0x111111,
                },
            ],
            None,
            None,
        );
        assert_eq!(styled.len(), 2);
        assert_eq!(styled[0].range, 0..3);
        assert_eq!(styled[0].rgb, Some(0x111111));
        assert_eq!(styled[1].range, 3..6);
        assert_eq!(styled[1].rgb, Some(0x222222));
    }
}
