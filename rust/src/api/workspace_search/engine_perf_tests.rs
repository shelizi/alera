use super::*;
use crate::api::workspace_search::{compile::compile_search, WorkspaceSearchOptions};
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

// Keep the previous matcher/preview implementation as an independent oracle.
// Only the UTF-8 boundary guard is shared: the previous byte iterator could
// panic on empty regex matches inside a multibyte character.
fn reference(
    text: &str,
    compiled: &CompiledSearch,
    full_lines: bool,
    mut total: u32,
    mut truncated: bool,
) -> (Vec<WorkspaceSearchMatch>, u32, bool) {
    let mut out = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if total >= compiled.max_results {
            truncated = true;
            break;
        }
        compiled
            .matcher
            .find_iter(line.as_bytes(), |m| {
                if !line.is_char_boundary(m.start()) || !line.is_char_boundary(m.end()) {
                    return true;
                }
                if total >= compiled.max_results || out.len() as u32 >= MAX_MATCHES_PER_FILE {
                    truncated = true;
                    return false;
                }
                let start = line[..m.start()].chars().count();
                let length = line[m.start()..m.end()].chars().count();
                let line_number = line_index as u32 + 1;
                let column = start as u32 + 1;
                let (line_content, display_column, display_match_length) = if full_lines {
                    (line.to_string(), None, None)
                } else {
                    let chars = line.chars().collect::<Vec<_>>();
                    if chars.len() <= MAX_LINE_CONTENT_LENGTH {
                        (line.to_string(), None, None)
                    } else {
                        let clamped = length.min(MAX_LINE_CONTENT_LENGTH);
                        let left = (MAX_LINE_CONTENT_LENGTH - clamped) / 2;
                        let window_start = start.saturating_sub(left);
                        let window_end = (window_start + MAX_LINE_CONTENT_LENGTH).min(chars.len());
                        let window_start = window_end.saturating_sub(MAX_LINE_CONTENT_LENGTH);
                        let mut snippet =
                            chars[window_start..window_end].iter().collect::<String>();
                        let mut display = (start - window_start) as u32 + 1;
                        if window_start > 0 {
                            snippet.insert(0, '\u{2026}');
                            display += 1;
                        }
                        if window_end < chars.len() {
                            snippet.push('\u{2026}');
                        }
                        (snippet, Some(display), Some(clamped as u32))
                    }
                };
                out.push(WorkspaceSearchMatch {
                    id: format!("sample.txt:{line_number}:{column}:{}", out.len()),
                    line: line_number,
                    column,
                    match_length: length as u32,
                    line_content,
                    display_column,
                    display_match_length,
                    replacement_preview: None,
                });
                total += 1;
                true
            })
            .unwrap();
    }
    (out, total, truncated)
}

fn candidate(
    text: &str,
    compiled: &CompiledSearch,
    full_lines: bool,
    mut total: u32,
    mut truncated: bool,
) -> (Vec<WorkspaceSearchMatch>, u32, bool) {
    let matches = matches_in_file(
        "sample.txt",
        text,
        compiled,
        full_lines,
        &mut total,
        &mut truncated,
        None,
    )
    .unwrap();
    (matches, total, truncated)
}

fn options(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    use_regex: bool,
    limit: u32,
) -> WorkspaceSearchOptions {
    WorkspaceSearchOptions {
        workspace_path: root.to_string_lossy().to_string(),
        query: query.to_string(),
        case_sensitive,
        whole_word,
        use_regex,
        include_pattern: None,
        exclude_pattern: None,
        include_ignored: false,
        max_results: Some(limit),
    }
}

fn assert_same(
    text: &str,
    compiled: &CompiledSearch,
    full_lines: bool,
    total: u32,
    truncated: bool,
) {
    let old = reference(text, compiled, full_lines, total, truncated);
    let new = candidate(text, compiled, full_lines, total, truncated);
    assert_eq!((old.1, old.2), (new.1, new.2));
    assert_eq!(old.0.len(), new.0.len());
    for (old, new) in old.0.iter().zip(new.0.iter()) {
        assert_eq!(format!("{old:?}"), format!("{new:?}"));
    }
}

#[test]
fn literal_seek_and_bounded_previews_preserve_all_match_fields() {
    let dir = tempfile::tempdir().unwrap();
    let texts = [
        String::new(),
        "\n\r\n\n".to_string(),
        "nothing\r\n\nNEEDLE \u{1f642}needle\n\u{4e2d}\u{6587}\u{67e5}\u{8a62}\r\nneedle\r"
            .to_string(),
        "\u{212a} k K \u{017f} S s \u{1f642} e\u{301} \t a.b\r\nend".to_string(),
        format!(
            "{}needle {}needle\r\n",
            "\u{1f642}\u{4e2d}".repeat(600),
            "x".repeat(550)
        ),
        format!("{}\n{}needle", "x".repeat(501), "\u{4e2d}".repeat(1000)),
        "needle\r\n".repeat(105),
        "needle ".repeat(105),
        "needle\n".repeat(100),
        format!("{}\nno match\n", "needle ".repeat(100)),
        "needle".to_string(),
        "needle\n".to_string(),
        "needle\n\n".to_string(),
        "needle\nno match".to_string(),
        format!("{}needle\n", "unrelated line\n".repeat(10_000)),
        "a".repeat(650),
        format!("{}\r\n", "\u{1f642}needle ".repeat(140)),
    ];
    let queries = [
        ("needle", true, false, false),
        ("needle", false, false, false),
        ("NEEDLE", false, false, false),
        ("\u{67e5}\u{8a62}", false, false, false),
        ("k", false, false, false),
        ("s", false, false, false),
        ("\u{1f642}", true, false, false),
        (" ", true, false, false),
        ("\t", true, false, false),
        ("\r", true, false, false),
        ("a.b", true, false, false),
        ("needle", false, true, false),
        ("^$", true, false, true),
        ("$", true, false, true),
        ("a*", true, false, true),
        ("a+", true, false, true),
        ("[a-z]+", true, false, true),
    ];
    for (query, case_sensitive, whole_word, use_regex) in queries {
        for limit in [1, 2, 100, 101, 2000] {
            let compiled = compile_search(&options(
                dir.path(),
                query,
                case_sensitive,
                whole_word,
                use_regex,
                limit,
            ))
            .unwrap();
            for text in &texts {
                for full_lines in [false, true] {
                    assert_same(text, &compiled, full_lines, 0, false);
                    assert_same(text, &compiled, full_lines, limit - 1, true);
                }
            }
        }
    }
}

#[test]
fn cancelled_literal_seek_never_returns_partial_results() {
    let dir = tempfile::tempdir().unwrap();
    let compiled =
        compile_search(&options(dir.path(), "needle", false, false, false, 2000)).unwrap();
    let cancelled = AtomicBool::new(true);
    let mut total = 0;
    let mut truncated = false;
    let error = matches_in_file(
        "sample.txt",
        "no match\nneedle\n",
        &compiled,
        false,
        &mut total,
        &mut truncated,
        Some(&cancelled),
    )
    .unwrap_err();
    assert_eq!(error.kind, WorkspaceSearchErrorKind::Cancelled);
    assert_eq!(total, 0);
}

#[test]
fn empty_regex_matches_use_only_valid_unicode_character_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let compiled = compile_search(&options(dir.path(), "a*", true, false, true, 2000)).unwrap();
    let (matches, total, truncated) = candidate("\u{212a}\u{1f642}", &compiled, false, 0, false);
    assert_eq!(total, 3);
    assert!(!truncated);
    assert_eq!(
        matches
            .iter()
            .map(|m| (m.column, m.match_length))
            .collect::<Vec<_>>(),
        vec![(1, 0), (2, 0), (3, 0)]
    );
}

#[test]
#[ignore = "repeatable matcher/preview phase benchmark; run release and serialized"]
fn compare_workspace_search_engine_phases() {
    let dir = tempfile::tempdir().unwrap();
    let ordinary_line = "let buffer = read_workspace_file(path, options);\n";
    let sparse = format!(
        "{}NEEDLE \u{4e2d}\u{6587}\n{}needle\r\n{}",
        ordinary_line.repeat(15_000),
        ordinary_line.repeat(15_000),
        ordinary_line.repeat(15_000)
    );
    let absent = ordinary_line.repeat(45_000);
    let dense = format!("{}\n", "needle ".repeat(50_000));
    let long_unicode = format!(
        "{}{}",
        "\u{1f642}\u{4e2d}".repeat(100_000),
        " needle ".repeat(105)
    );
    let compiled =
        compile_search(&options(dir.path(), "needle", false, false, false, 2000)).unwrap();
    for (name, text, iterations) in [
        ("absent", absent.as_str(), 10),
        ("sparse", sparse.as_str(), 10),
        ("dense_long_ascii", dense.as_str(), 3),
        ("long_unicode", long_unicode.as_str(), 3),
    ] {
        assert_same(text, &compiled, false, 0, false);
        let mut old_times = Vec::new();
        let mut new_times = Vec::new();
        let run = |old: bool| {
            let started = Instant::now();
            for _ in 0..iterations {
                let result = if old {
                    reference(black_box(text), &compiled, false, 0, false)
                } else {
                    candidate(black_box(text), &compiled, false, 0, false)
                };
                black_box(result);
            }
            started.elapsed().as_secs_f64() * 1000.0 / f64::from(iterations)
        };
        for round in 0..7 {
            if round % 2 == 0 {
                old_times.push(run(true));
                new_times.push(run(false));
            } else {
                new_times.push(run(false));
                old_times.push(run(true));
            }
        }
        old_times.sort_by(f64::total_cmp);
        new_times.sort_by(f64::total_cmp);
        println!(
            "SEARCH_PHASE name={name} bytes={} old_ms={:.6} new_ms={:.6}",
            text.len(),
            old_times[3],
            new_times[3],
        );
    }
}
