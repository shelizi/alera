use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use grep_matcher::Matcher;
use ignore::WalkBuilder;

use super::compile::CompiledSearch;
use super::globs::matches_globs;
use super::paths::{
    content_token, is_protected_relative_path, relative_string, safe_regular_file_metadata,
    should_walk_entry,
};
use super::{
    WorkspaceSearchError, WorkspaceSearchErrorKind, WorkspaceSearchFileResult,
    WorkspaceSearchMatch, WorkspaceSearchResult, MAX_LINE_CONTENT_LENGTH, MAX_MATCHES_PER_FILE,
    MAX_TEXT_FILE_BYTES,
};

pub(super) fn run_search(
    compiled: &CompiledSearch,
    full_lines: bool,
    cancellation: Option<&AtomicBool>,
) -> Result<WorkspaceSearchResult, WorkspaceSearchError> {
    let mut files = Vec::<WorkspaceSearchFileResult>::new();
    let mut total_matches = 0_u32;
    let mut truncated = false;
    let mut walker = WalkBuilder::new(&compiled.root);
    let root = compiled.root.clone();
    // Do not walk other linked worktrees, even when ignored files are included.
    let nested_worktrees = alera_core::workspace_files::nested_git_worktree_paths(&root);
    walker
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .filter_entry(move |entry| {
            should_walk_entry(&root, entry.path()) && !nested_worktrees.contains(entry.path())
        });
    if compiled.include_ignored {
        walker
            .ignore(false)
            .git_global(false)
            .git_ignore(false)
            .git_exclude(false);
    }

    for entry in walker.build() {
        ensure_not_cancelled(cancellation)?;
        if total_matches >= compiled.max_results {
            truncated = true;
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        if path == compiled.root {
            continue;
        }
        let file_type = match entry.file_type() {
            Some(file_type) => file_type,
            None => continue,
        };
        if !file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        let relative_path = match relative_string(&compiled.root, path) {
            Some(path) => path,
            None => continue,
        };
        if is_protected_relative_path(&relative_path) {
            continue;
        }
        if !matches_globs(&relative_path, &compiled.include, true)
            || matches_globs(&relative_path, &compiled.exclude, false)
        {
            continue;
        }
        let metadata = match safe_regular_file_metadata(&compiled.root, path, &relative_path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.len() > MAX_TEXT_FILE_BYTES {
            continue;
        }
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        ensure_not_cancelled(cancellation)?;
        if bytes.contains(&0) {
            continue;
        }
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let matches = matches_in_file(
            &relative_path,
            &text,
            compiled,
            full_lines,
            &mut total_matches,
            &mut truncated,
            cancellation,
        )?;
        if !matches.is_empty() {
            files.push(WorkspaceSearchFileResult {
                relative_path,
                content_token: content_token(&metadata),
                matches,
            });
        }
    }

    Ok(WorkspaceSearchResult {
        files,
        total_matches,
        truncated,
    })
}

fn matches_in_file(
    relative_path: &str,
    text: &str,
    compiled: &CompiledSearch,
    full_lines: bool,
    total_matches: &mut u32,
    truncated: &mut bool,
    cancellation: Option<&AtomicBool>,
) -> Result<Vec<WorkspaceSearchMatch>, WorkspaceSearchError> {
    let mut out = Vec::new();
    let mut all_lines = text.lines().enumerate();
    let mut seek_offset = 0;
    let mut seek_line_index = 0;
    loop {
        ensure_not_cancelled(cancellation)?;
        let next = if compiled.seek_literal_lines {
            if seek_offset >= text.len() {
                break;
            }
            // Preserve the original truncation contract, including a
            // remaining nonmatching line after the global result limit.
            if *total_matches >= compiled.max_results {
                *truncated = true;
                break;
            }
            let next = seek_literal_line(text, compiled, &mut seek_offset, &mut seek_line_index)?;
            ensure_not_cancelled(cancellation)?;
            next
        } else {
            all_lines.next()
        };
        let Some((line_index, line)) = next else {
            break;
        };
        if *total_matches >= compiled.max_results {
            *truncated = true;
            break;
        }
        let line_bytes = line.as_bytes();
        let mut file_count = out.len() as u32;
        // Advance character columns from the previous nonoverlapping match.
        // Avoid repeatedly counting the entire prefix of long/minified lines.
        let mut previous_end = 0;
        let mut previous_end_chars = 0;
        let mut line_chars = None;
        compiled
            .matcher
            .find_iter(line_bytes, |m| {
                let start = m.start();
                let end = m.end();
                // The byte-oriented iterator can advance through the middle
                // of UTF-8 after an empty regex match. Such offsets cannot
                // be represented by editor character columns.
                if !line.is_char_boundary(start) || !line.is_char_boundary(end) {
                    return true;
                }
                if *total_matches >= compiled.max_results || file_count >= MAX_MATCHES_PER_FILE {
                    *truncated = true;
                    return false;
                }
                let start_chars = previous_end_chars + line[previous_end..start].chars().count();
                let matched_chars = line[start..end].chars().count();
                previous_end = end;
                previous_end_chars = start_chars + matched_chars;
                let column = start_chars as u32 + 1;
                let match_length = matched_chars as u32;
                let line_number = line_index as u32 + 1;
                let clamped = if full_lines {
                    LinePreview {
                        line_content: line.to_string(),
                        display_column: None,
                        display_match_length: None,
                    }
                } else {
                    // Most source lines fit without counting or allocating
                    // a temporary Vec<char>. Long lines count once per line.
                    let count = *line_chars.get_or_insert_with(|| {
                        if line.len() <= MAX_LINE_CONTENT_LENGTH {
                            line.len()
                        } else {
                            line.chars().count()
                        }
                    });
                    clamp_line_context(line, start, start_chars, matched_chars, count)
                };
                let id = format!("{relative_path}:{line_number}:{column}:{}", out.len());
                out.push(WorkspaceSearchMatch {
                    id,
                    line: line_number,
                    column,
                    match_length,
                    line_content: clamped.line_content,
                    display_column: clamped.display_column,
                    display_match_length: clamped.display_match_length,
                    replacement_preview: None,
                });
                *total_matches += 1;
                file_count += 1;
                true
            })
            .map_err(|error| {
                WorkspaceSearchError::new(
                    WorkspaceSearchErrorKind::InvalidPattern,
                    error.to_string(),
                )
            })?;
        // Once this file is capped and truncation is already known, no
        // additional line can change the returned matches or truncation bit.
        if out.len() as u32 >= MAX_MATCHES_PER_FILE && *truncated {
            break;
        }
    }
    Ok(out)
}

/// Find a matching physical line using the same grep matcher over the
/// remaining file. Only used for nonempty plain literals without CR/LF
/// or word-boundary mode. Re-run the matcher on that exact str::lines()
/// slice to retain the existing match, column and replacement semantics.
fn seek_literal_line<'a>(
    text: &'a str,
    compiled: &CompiledSearch,
    offset: &mut usize,
    line_index: &mut usize,
) -> Result<Option<(usize, &'a str)>, WorkspaceSearchError> {
    let found = compiled
        .matcher
        .find_at(text.as_bytes(), *offset)
        .map_err(|error| {
            WorkspaceSearchError::new(WorkspaceSearchErrorKind::InvalidPattern, error.to_string())
        })?;
    let Some(found) = found else {
        *offset = text.len();
        return Ok(None);
    };
    let skipped = &text[*offset..found.start()];
    *line_index += skipped.bytes().filter(|&byte| byte == b'\n').count();
    let start = skipped
        .rfind('\n')
        .map_or(*offset, |index| *offset + index + 1);
    let remainder = &text[start..];
    let length = remainder
        .find('\n')
        .map_or(remainder.len(), |index| index + 1);
    let line = remainder[..length].lines().next().expect("matching line");
    let current = *line_index;
    *offset = start + length;
    *line_index += 1;
    Ok(Some((current, line)))
}

pub(super) fn ensure_not_cancelled(
    cancellation: Option<&AtomicBool>,
) -> Result<(), WorkspaceSearchError> {
    if cancellation.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err(WorkspaceSearchError::new(
            WorkspaceSearchErrorKind::Cancelled,
            "search cancelled",
        ));
    }
    Ok(())
}

struct LinePreview {
    line_content: String,
    display_column: Option<u32>,
    display_match_length: Option<u32>,
}

fn clamp_line_context(
    line: &str,
    start_byte: usize,
    start: usize,
    match_len: usize,
    line_chars: usize,
) -> LinePreview {
    if line_chars <= MAX_LINE_CONTENT_LENGTH {
        return LinePreview {
            line_content: line.to_string(),
            display_column: None,
            display_match_length: None,
        };
    }
    let clamped_match_len = match_len.min(MAX_LINE_CONTENT_LENGTH);
    let remaining = MAX_LINE_CONTENT_LENGTH.saturating_sub(clamped_match_len);
    let left_budget = remaining / 2;
    let mut window_start = start.saturating_sub(left_budget);
    let window_end = (window_start + MAX_LINE_CONTENT_LENGTH).min(line_chars);
    window_start = window_end.saturating_sub(MAX_LINE_CONTENT_LENGTH);
    // The context window starts at or before the match. Walk backwards
    // at most 500 characters, then forwards at most 500, instead of
    // allocating a vector proportional to the entire source line.
    let start_byte = if window_start == start {
        start_byte
    } else {
        line[..start_byte]
            .char_indices()
            .rev()
            .nth(start - window_start - 1)
            .expect("context start")
            .0
    };
    let end_byte = line[start_byte..]
        .char_indices()
        .nth(window_end - window_start)
        .map_or(line.len(), |(index, _)| start_byte + index);
    let mut snippet = line[start_byte..end_byte].to_string();
    let mut display_column = start.saturating_sub(window_start) as u32 + 1;
    if window_start > 0 {
        snippet.insert(0, '…');
        display_column += 1;
    }
    if window_end < line_chars {
        snippet.push('…');
    }
    LinePreview {
        line_content: snippet,
        display_column: Some(display_column),
        display_match_length: Some(clamped_match_len as u32),
    }
}

#[cfg(test)]
#[path = "engine_perf_tests.rs"]
mod perf_tests;
