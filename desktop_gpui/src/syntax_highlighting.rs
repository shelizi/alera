//! UI-independent syntax highlighting boundary for the native GPUI editor.
//! The renderer consumes byte-range foreground spans, not parser-specific
//! syntax nodes. A Tree-sitter backend can implement SyntaxBackend later.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use crate::editor_syntax_theme::{
    palette_for_name, EditorSyntaxPalette, CLASSIC_EDITOR_SYNTAX_PALETTES,
};
use two_face::re_exports::syntect::{
    easy::HighlightLines,
    highlighting::{
        Color, FontStyle as SyntectFontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem,
        ThemeSettings,
    },
    parsing::{SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};

/// A half-open UTF-8 byte range relative to one displayed line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxSpan {
    pub range: Range<usize>,
    pub rgb: u32,
    /// Token background only when it differs from the editor canvas.
    pub bg: Option<u32>,
    pub bold: bool,
    pub italic: bool,
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
    /// Classic/Host theme, independent of the opened file's grammar.
    pub theme_name: &'a str,
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

fn syntect_color(rgb: u32) -> Color {
    Color {
        r: (rgb >> 16) as u8,
        g: (rgb >> 8) as u8,
        b: rgb as u8,
        a: 255,
    }
}

/// Map syntect scope classes onto Classic's actual re_highlight token colors.
/// Highlight.js and Syntect have different grammars; we share the same color
/// meanings, not their internal token/parse trees.
fn build_theme(palette: &EditorSyntaxPalette) -> Theme {
    const RULES: &[(&str, &[&str])] = &[
        ("comment", &["comment", "quote"]),
        ("keyword", &["keyword", "meta-keyword"]),
        ("keyword.operator", &["operator", "keyword"]),
        ("keyword.control", &["keyword", "meta-keyword"]),
        ("storage", &["keyword", "meta-keyword"]),
        ("string", &["string", "meta-string"]),
        ("string.regexp", &["regexp", "string"]),
        ("constant.numeric", &["number", "literal"]),
        ("constant.language", &["literal", "built_in"]),
        ("constant.character.escape", &["literal", "string"]),
        ("constant.other.symbol", &["symbol", "literal"]),
        (
            "entity.name.function",
            &["title.function_", "function", "title"],
        ),
        (
            "entity.name.type, entity.name.class",
            &["title.class_", "type", "class-title"],
        ),
        ("support.function", &["function", "title"]),
        ("support.type", &["type", "built_in"]),
        ("support.constant", &["built_in", "literal"]),
        ("entity.name.tag", &["tag", "name"]),
        // Syntect's CSS-specific scopes correspond to Classic/Highlight.js
        // selector-class, selector-id and selector-pseudo, not HTML attrs.
        // Classic falls back to root when a palette omits a selector color.
        ("entity.name.tag.css", &["selector-tag", "root"]),
        (
            "entity.other.attribute-name.class.css",
            &["selector-class", "root"],
        ),
        (
            "entity.other.attribute-name.id.css",
            &["selector-id", "root"],
        ),
        (
            "entity.other.pseudo-class.css",
            &["selector-pseudo", "root"],
        ),
        (
            "entity.other.pseudo-element.css",
            &["selector-pseudo", "root"],
        ),
        (
            "support.type.property-name.css",
            &["attribute", "attr", "root"],
        ),
        (
            "constant.other.color.rgb-value.css",
            &["number", "literal", "root"],
        ),
        (
            "support.constant.color.w3c-standard-color-name.css",
            &["literal", "built_in", "root"],
        ),
        ("entity.other.attribute-name", &["attribute", "attr"]),
        ("variable", &["variable", "params"]),
        ("variable.parameter", &["params", "variable"]),
        ("meta.parameter", &["params", "variable"]),
        ("punctuation", &["punctuation"]),
        ("markup.heading", &["section", "title"]),
        ("markup.inserted", &["addition"]),
        ("markup.deleted", &["deletion"]),
        ("markup.bold", &["strong"]),
        ("markup.italic", &["emphasis"]),
        ("markup.list", &["bullet"]),
        ("markup.inline.raw", &["code"]),
        ("meta", &["meta"]),
    ];
    let scopes = RULES
        .iter()
        .filter_map(|(selector, categories)| {
            let color = palette.first_color(categories);
            let background = palette.first_background(categories);
            let modifier = palette.first_modifier(categories);
            if color.is_none() && background.is_none() && modifier.is_none() {
                return None;
            }
            let font_style = modifier.map(|bits| {
                let mut style = SyntectFontStyle::empty();
                if bits & 1 != 0 {
                    style |= SyntectFontStyle::BOLD;
                }
                if bits & 2 != 0 {
                    style |= SyntectFontStyle::ITALIC;
                }
                style
            });
            Some(ThemeItem {
                scope: ScopeSelectors::from_str(selector)
                    .expect("static Classic to Syntect scope selector"),
                style: StyleModifier {
                    foreground: color.map(syntect_color),
                    background: background.map(syntect_color),
                    font_style,
                    ..StyleModifier::default()
                },
            })
        })
        .collect();
    Theme {
        name: Some(palette.name.to_owned()),
        settings: ThemeSettings {
            foreground: Some(syntect_color(palette.foreground)),
            background: Some(syntect_color(palette.background)),
            ..ThemeSettings::default()
        },
        scopes,
        ..Theme::default()
    }
}

fn theme(name: &str) -> &'static Theme {
    static THEMES: OnceLock<Vec<Theme>> = OnceLock::new();
    let themes = THEMES.get_or_init(|| {
        CLASSIC_EDITOR_SYNTAX_PALETTES
            .iter()
            .map(build_theme)
            .collect()
    });
    let canonical = palette_for_name(name).name;
    let index = CLASSIC_EDITOR_SYNTAX_PALETTES
        .iter()
        .position(|entry| entry.name == canonical)
        .expect("Classic default palette always exists");
    &themes[index]
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

        let palette = palette_for_name(request.theme_name);
        let mut highlighter = HighlightLines::new(syntax, theme(request.theme_name));
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
                let bg_color = style.background;
                let bg_rgb = u32::from(bg_color.r) << 16
                    | u32::from(bg_color.g) << 8
                    | u32::from(bg_color.b);
                let bg = (bg_rgb != palette.background).then_some(bg_rgb);
                let bold = style.font_style.contains(SyntectFontStyle::BOLD);
                let italic = style.font_style.contains(SyntectFontStyle::ITALIC);
                if let Some(previous) = spans.last_mut() {
                    if previous.rgb == rgb
                        && previous.bg == bg
                        && previous.bold == bold
                        && previous.italic == italic
                        && previous.range.end == start
                    {
                        previous.range.end = end;
                        continue;
                    }
                }
                spans.push(SyntaxSpan {
                    range: start..end,
                    rgb,
                    bg,
                    bold,
                    italic,
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
    theme_name: String,
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
            theme_name: "Alera".to_owned(),
            source: String::new(),
            snapshot: SyntaxSnapshot::default(),
        }
    }

    pub fn theme_name(&self) -> &str {
        &self.theme_name
    }

    /// Only invalidate colors. Preserve document bytes, cursor and undo; the
    /// next paint reprojects the same source using the new Classic palette.
    pub fn set_theme_name(&mut self, name: &str) -> bool {
        let canonical = palette_for_name(name).name;
        if self.theme_name == canonical {
            return false;
        }
        self.theme_name = canonical.to_owned();
        self.tab = None;
        true
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
            theme_name: &self.theme_name,
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
    pub bg: Option<u32>,
    pub bold: bool,
    pub italic: bool,
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
        let syntax_style = syntax
            .get(syntax_index)
            .filter(|span| span.range.start <= start && end <= span.range.end);
        let rgb = syntax_style.map(|span| span.rgb);
        let bg = syntax_style.and_then(|span| span.bg);
        let bold = syntax_style.is_some_and(|span| span.bold);
        let italic = syntax_style.is_some_and(|span| span.italic);
        let selected = selection
            .as_ref()
            .is_some_and(|range| range.start <= start && end <= range.end);
        let composing = composition
            .as_ref()
            .is_some_and(|range| range.start <= start && end <= range.end);
        if rgb.is_none() && bg.is_none() && !selected && !composing {
            continue;
        }
        if let Some(previous) = output.last_mut() {
            if previous.range.end == start
                && previous.rgb == rgb
                && previous.bg == bg
                && previous.bold == bold
                && previous.italic == italic
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
            bg,
            bold,
            italic,
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
                theme_name: "Alera",
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
            theme_name: "Alera",
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
            theme_name: "Alera",
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
            theme_name: "Alera",
        });
        let line = text.trim_end_matches('\n');
        for span in snapshot.line(0) {
            assert!(line.is_char_boundary(span.range.start));
            assert!(line.is_char_boundary(span.range.end));
            assert!(span.range.end <= line.len());
        }
    }

    #[test]
    fn classic_host_theme_rehighlights_open_document_without_changing_source_or_utf8_ranges() {
        let source = "fn main() { let greeting = \"你好\"; }";
        let mut cache = SyntaxHighlightCache::default();
        let path = Path::new("main.rs");
        cache.refresh(19, source, Some(path), None);
        let baseline = cache.snapshot().line(0).to_vec();
        assert!(!baseline.is_empty());
        assert_eq!(cache.theme_name(), "Alera");

        assert!(cache.set_theme_name("Tokyo Night Dark"));
        cache.refresh(19, source, Some(path), None);
        let themed = cache.snapshot().line(0).to_vec();
        assert_ne!(
            baseline, themed,
            "same source must repaint when Host theme changes"
        );
        assert_eq!(cache.theme_name(), "Tokyo Night Dark");
        assert!(themed.iter().all(|span| span.range.end <= source.len()
            && source.is_char_boundary(span.range.start)
            && source.is_char_boundary(span.range.end)));
        assert!(themed.iter().any(|span| {
            span.rgb
                == palette_for_name("Tokyo Night Dark")
                    .color("keyword")
                    .unwrap()
        }));
        assert!(!cache.set_theme_name("tokyo night dark"));

        // Unknown saved Host values fail closed to Classic's Alera default.
        assert!(cache.set_theme_name("missing legacy theme"));
        cache.refresh(19, source, Some(path), None);
        assert_eq!(cache.theme_name(), "Alera");
        assert_eq!(cache.snapshot().line(0), baseline.as_slice());
    }

    #[test]
    fn all_ten_classic_palettes_highlight_real_rust_code() {
        let source = "fn main() { let greeting = \"hello\"; }\n";
        for palette in CLASSIC_EDITOR_SYNTAX_PALETTES {
            let highlighted = SyntectBackend.highlight(SyntaxRequest {
                text: source,
                path: Some(Path::new("sample.rs")),
                language: None,
                theme_name: palette.name,
            });
            assert_eq!(highlighted.line_count(), 2, "{}", palette.name);
            let spans = highlighted.line(0);
            assert!(
                spans.len() >= 2,
                "No styled Rust syntax in {}",
                palette.name
            );
            assert!(
                spans.iter().any(|span| span.rgb != palette.foreground),
                "All tokens fell back to plain text in {}",
                palette.name
            );
            assert!(spans.iter().all(|span| {
                source.is_char_boundary(span.range.start)
                    && source.is_char_boundary(span.range.end)
                    && span.range.end <= source.trim_end_matches('\n').len()
            }));
        }
    }

    #[test]
    fn classic_bold_and_italic_are_applied_to_real_syntect_tokens() {
        let code = "fn greet() { let message = \"hi\"; } // comment\n";
        let monokai = SyntectBackend.highlight(SyntaxRequest {
            text: code,
            path: Some(Path::new("sample.rs")),
            language: None,
            theme_name: "Monokai",
        });
        assert!(
            monokai.line(0).iter().any(|span| span.bold),
            "Monokai should bold Classic keyword/title tokens"
        );
        let night_owl = SyntectBackend.highlight(SyntaxRequest {
            text: code,
            path: Some(Path::new("sample.rs")),
            language: None,
            theme_name: "Night Owl",
        });
        assert!(
            night_owl
                .line(0)
                .iter()
                .any(|span| { span.italic && code[span.range.clone()].contains("comment") }),
            "Night Owl should italicize Rust comments"
        );
        let atom = SyntectBackend.highlight(SyntaxRequest {
            text: code,
            path: Some(Path::new("sample.rs")),
            language: None,
            theme_name: "Atom One Dark",
        });
        assert!(atom.line(0).iter().any(|span| span.italic));
    }

    #[test]
    fn classic_markdown_emphasis_and_strong_keep_font_style() {
        let markdown = "# Heading\n\n**bold** and *italic*\n";
        let snapshot = SyntectBackend.highlight(SyntaxRequest {
            text: markdown,
            path: Some(Path::new("README.md")),
            language: None,
            theme_name: "GitHub Dark",
        });
        assert_eq!(snapshot.line_count(), 4);
        assert!(
            snapshot.line(2).iter().any(|span| span.bold),
            "Classic strong should produce a bold Markdown span"
        );
        assert!(
            snapshot.line(2).iter().any(|span| span.italic),
            "Classic emphasis should produce an italic Markdown span"
        );
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
        assert!(cache.set_theme_name("Dracula"));
        cache.refresh(2, "fn main() { }", Some(Path::new("one.ts")), Some("Rust"));
        assert_eq!(counter.load(Ordering::Relaxed), 6);
        assert!(!cache.set_theme_name("Dracula"));
        cache.refresh(2, "fn main() { }", Some(Path::new("one.ts")), Some("Rust"));
        assert_eq!(counter.load(Ordering::Relaxed), 6);
    }

    #[test]
    fn css_and_html_scope_mapping_regression() {
        let css = ".card:hover, #panel { color: #fff; background-color: red; }";
        let night = palette_for_name("Night Owl");
        let css_result = SyntectBackend.highlight(SyntaxRequest {
            text: css,
            path: Some(Path::new("styles.css")),
            language: None,
            theme_name: night.name,
        });
        let at = |needle: &str| {
            let position = css.find(needle).expect("CSS sample token");
            css_result
                .line(0)
                .iter()
                .find(|span| span.range.contains(&position))
                .expect("CSS token must be highlighted")
        };
        assert_eq!(at("card").rgb, night.color("selector-class").unwrap());
        assert!(at("card").italic, "Night Owl CSS class names are italic");
        assert_eq!(at("hover").rgb, night.color("selector-pseudo").unwrap());
        assert!(at("hover").italic);
        assert_eq!(at("panel").rgb, night.color("selector-id").unwrap());
        assert_eq!(at("color:").rgb, night.color("attribute").unwrap());
        assert_eq!(at("fff").rgb, night.color("number").unwrap());

        let nord = SyntectBackend.highlight(SyntaxRequest {
            text: css,
            path: Some(Path::new("styles.css")),
            language: None,
            theme_name: "Nord",
        });
        let panel = css.find("panel").unwrap();
        assert!(
            nord.line(0)
                .iter()
                .any(|span| span.range.contains(&panel) && span.bold),
            "Nord's Classic CSS selector-id is bold"
        );

        // Classic does not define selector-class for Monokai: fall back to
        // its root foreground instead of incorrectly using HTML attr color.
        let monokai = SyntectBackend.highlight(SyntaxRequest {
            text: css,
            path: Some(Path::new("styles.css")),
            language: None,
            theme_name: "Monokai",
        });
        let card = css.find("card").unwrap();
        assert!(monokai.line(0).iter().any(|span| {
            span.range.contains(&card) && span.rgb == palette_for_name("Monokai").foreground
        }));

        let html = "<div class=\"card\" data-id=\"5\">Hi</div>";
        let html_result = SyntectBackend.highlight(SyntaxRequest {
            text: html,
            path: Some(Path::new("index.html")),
            language: None,
            theme_name: "Night Owl",
        });
        let element = html.find("div").unwrap();
        let attribute = html.find("data-id").unwrap();
        assert!(html_result.line(0).iter().any(|span| {
            span.range.contains(&element)
                && span.rgb == night.first_color(&["tag", "name"]).unwrap()
        }));
        assert!(html_result.line(0).iter().any(|span| {
            span.range.contains(&attribute)
                && span.rgb == night.first_color(&["attribute", "attr"]).unwrap()
        }));
    }

    #[test]
    fn classic_diff_insertions_and_deletions_use_original_token_backgrounds() {
        let source = "diff --git a/example b/example\n--- a/example\n+++ b/example\n@@ -1 +1 @@\n-old\n+new\n";
        let snapshot = SyntectBackend.highlight(SyntaxRequest {
            text: source,
            path: Some(Path::new("change.diff")),
            language: None,
            theme_name: "GitHub Dark",
        });
        assert!(
            snapshot.language().is_some(),
            "Diff must use a real syntax grammar"
        );
        let removed = snapshot.line(4);
        let added = snapshot.line(5);
        assert!(
            removed.iter().any(|span| span.bg == Some(0x67060c)),
            "Classic deletion background missing: {removed:?}"
        );
        assert!(
            added.iter().any(|span| span.bg == Some(0x033a16)),
            "Classic addition background missing: {added:?}"
        );
    }

    #[test]
    fn selected_diff_token_keeps_syntax_background_as_lower_priority() {
        let syntax = [SyntaxSpan {
            range: 0..4,
            rgb: 0x12ab34,
            bg: Some(0x033a16),
            bold: false,
            italic: false,
        }];
        let result = compose_line_styles("+中", &syntax, Some(1..4), None);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].bg, Some(0x033a16));
        assert!(!result[0].selected);
        assert_eq!(result[1].bg, Some(0x033a16));
        assert!(result[1].selected);
        assert_eq!(result[1].range, 1..4);
    }

    #[test]
    fn selection_and_composition_keep_syntax_color_and_utf8_offsets() {
        let text = "a中b";
        let syntax = vec![SyntaxSpan {
            range: 0..text.len(),
            rgb: 0x123456,
            bg: None,
            bold: false,
            italic: false,
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
    fn selection_and_ime_preserve_distinct_font_style_ranges() {
        let text = "a中b!";
        let spans = [
            SyntaxSpan {
                range: 0..1,
                rgb: 0x556677,
                bg: None,
                bold: false,
                italic: false,
            },
            SyntaxSpan {
                range: 1..4,
                rgb: 0x556677,
                bg: None,
                bold: true,
                italic: false,
            },
            SyntaxSpan {
                range: 4..5,
                rgb: 0x556677,
                bg: None,
                bold: false,
                italic: true,
            },
        ];
        let styled = compose_line_styles(text, &spans, Some(1..5), Some(4..5));
        // Unstyled, unselected trailing text stays in the normal text run.
        assert_eq!(styled.len(), 3);
        assert_eq!(styled[0].range, 0..1);
        assert!(!styled[0].bold);
        assert!(styled[1].bold);
        assert!(styled[1].selected);
        assert_eq!(styled[1].range, 1..4);
        assert!(styled[2].italic);
        assert!(styled[2].selected);
        assert!(styled[2].composing);
        assert_eq!(styled[2].range, 4..5);
        assert_eq!(styled[2].rgb, Some(0x556677));
    }

    #[test]
    fn invalid_backend_ranges_are_discarded_without_panic() {
        let text = "中";
        let styled = compose_line_styles(
            text,
            &[SyntaxSpan {
                range: 1..2,
                rgb: 0xff0000,
                bg: None,
                bold: false,
                italic: false,
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
                    bg: None,
                    bold: false,
                    italic: false,
                },
                SyntaxSpan {
                    range: 0..3,
                    rgb: 0x111111,
                    bg: None,
                    bold: false,
                    italic: false,
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
