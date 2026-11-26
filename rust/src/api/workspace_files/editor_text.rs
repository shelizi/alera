use super::{WorkspaceEditorTextFile, WorkspaceTextEncoding, WorkspaceTextFile};

pub(super) fn editor_text_file_from_raw(
    file: WorkspaceTextFile,
    tab_size: i32,
    encoding: WorkspaceTextEncoding,
) -> WorkspaceEditorTextFile {
    let display_content = expand_workspace_editor_tabs(&file.content, tab_size);
    WorkspaceEditorTextFile {
        raw_content: file.content,
        display_content,
        content_token: file.content_token,
        modified_millis: file.modified_millis,
        size: file.size,
        encoding,
    }
}

pub(super) fn encode_workspace_editor_text_for_save(
    current_display_content: &str,
    original_raw_content: Option<&str>,
    original_display_content: Option<&str>,
) -> String {
    let (Some(original_raw_content), Some(original_display_content)) =
        (original_raw_content, original_display_content)
    else {
        return current_display_content.to_string();
    };
    if current_display_content == original_display_content {
        return original_raw_content.to_string();
    }
    // With identical raw/display text there are no hidden tabs to restore.
    // Avoid building three line-index vectors for the common spaces-only file.
    if original_raw_content == original_display_content {
        return current_display_content.to_string();
    }

    let original_raw_segments = split_line_segments(original_raw_content);
    let original_display_segments = split_line_segments(original_display_content);
    let current_segments = split_line_segments(current_display_content);
    if original_raw_segments.len() != original_display_segments.len() {
        return current_display_content.to_string();
    }

    let mut prefix = 0usize;
    while prefix < original_display_segments.len()
        && prefix < current_segments.len()
        && original_display_segments[prefix] == current_segments[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix < original_display_segments.len().saturating_sub(prefix)
        && suffix < current_segments.len().saturating_sub(prefix)
    {
        let original_index = original_display_segments.len() - suffix - 1;
        let current_index = current_segments.len() - suffix - 1;
        if original_display_segments[original_index] != current_segments[current_index] {
            break;
        }
        suffix += 1;
    }

    let mut encoded = String::with_capacity(current_display_content.len());
    let suffix_start = current_segments.len().saturating_sub(suffix);
    for (index, current_segment) in current_segments.iter().enumerate() {
        if index < prefix {
            encoded.push_str(original_raw_segments[index]);
        } else if index >= suffix_start {
            let original_index = original_raw_segments.len() - (current_segments.len() - index);
            encoded.push_str(original_raw_segments[original_index]);
        } else {
            encoded.push_str(current_segment);
        }
    }
    encoded
}

fn expand_workspace_editor_tabs(text: &str, tab_size: i32) -> String {
    // Most files are space-indented. Copy them as a single UTF-8 buffer
    // rather than decoding and appending every character.
    if !text.as_bytes().contains(&b'\t') {
        return text.to_owned();
    }
    let effective_tab_size = normalize_workspace_editor_tab_size(tab_size);
    let mut expanded = String::with_capacity(text.len());
    let mut column = 0usize;
    let mut start = 0;
    for (index, _) in text.match_indices('\t') {
        let segment = &text[start..index];
        expanded.push_str(segment);
        // Only the characters after the last line break affect a tab stop.
        // Preserve scalar-value columns (including non-BMP and combining
        // characters), and the original CR, LF and CRLF bytes.
        if let Some(last_break) = segment.rfind(['\r', '\n']) {
            column = segment[last_break + 1..].chars().count();
        } else {
            column += segment.chars().count();
        }
        let spaces = effective_tab_size - (column % effective_tab_size);
        expanded.extend(std::iter::repeat_n(' ', spaces));
        column += spaces;
        start = index + 1;
    }
    expanded.push_str(&text[start..]);
    expanded
}

fn normalize_workspace_editor_tab_size(tab_size: i32) -> usize {
    tab_size.clamp(1, 8) as usize
}

fn split_line_segments(text: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                index += 1;
            }
            segments.push(&text[start..=index]);
            start = index + 1;
        } else if bytes[index] == b'\n' {
            segments.push(&text[start..=index]);
            start = index + 1;
        }
        index += 1;
    }
    if start < text.len() {
        segments.push(&text[start..]);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_expand(text: &str, tab_size: i32) -> String {
        let size = normalize_workspace_editor_tab_size(tab_size);
        let mut expanded = String::with_capacity(text.len());
        let mut column = 0;
        for character in text.chars() {
            if character == '\t' {
                let spaces = size - column % size;
                expanded.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            } else {
                expanded.push(character);
                if character == '\r' || character == '\n' {
                    column = 0;
                } else {
                    column += 1;
                }
            }
        }
        expanded
    }

    #[test]
    fn segment_expansion_matches_original_for_unicode_and_all_line_breaks() {
        let parts = ["", "a", "中文", "🙂", "e\u{301}", "\t", "\r", "\n", "\r\n"];
        for a in parts {
            for b in parts {
                for c in parts {
                    let text = format!("{a}{b}{c}\t{a}");
                    for tab in -1..=9 {
                        assert_eq!(
                            expand_workspace_editor_tabs(&text, tab),
                            reference_expand(&text, tab)
                        );
                    }
                }
            }
        }
        let no_tabs = "中文 🙂\r\n  code\rfinal";
        assert_eq!(expand_workspace_editor_tabs(no_tabs, 4), no_tabs);
        assert_eq!(
            encode_workspace_editor_text_for_save("edited\r\n", Some(no_tabs), Some(no_tabs)),
            "edited\r\n"
        );
    }

    #[test]
    #[ignore = "repeatable editor expansion microbenchmark"]
    fn compare_editor_tab_expansion() {
        use std::{hint::black_box, time::Instant};
        for (name, text) in [
            ("spaces", "    let value = 中文🙂;\r\n".repeat(40000)),
            (
                "sparse_tabs",
                format!("{}\tend\n", "    let value = 中文🙂;\r\n".repeat(40000)),
            ),
            ("dense_tabs", "\tlet\tvalue = 中文🙂;\r\n".repeat(40000)),
        ] {
            assert_eq!(
                expand_workspace_editor_tabs(&text, 4),
                reference_expand(&text, 4)
            );
            let mut old = Vec::new();
            let mut new = Vec::new();
            let run = |reference: bool| {
                let start = Instant::now();
                for _ in 0..10 {
                    black_box(if reference {
                        reference_expand(black_box(&text), 4)
                    } else {
                        expand_workspace_editor_tabs(black_box(&text), 4)
                    });
                }
                start.elapsed().as_secs_f64() * 100.0
            };
            for i in 0..7 {
                if i % 2 == 0 {
                    old.push(run(true));
                    new.push(run(false));
                } else {
                    new.push(run(false));
                    old.push(run(true));
                }
            }
            old.sort_by(f64::total_cmp);
            new.sort_by(f64::total_cmp);
            println!(
                "EDITOR_TABS mode={name} bytes={} old_ms={:.6} new_ms={:.6}",
                text.len(),
                old[3],
                new[3]
            );
        }
    }

    #[test]
    fn expands_workspace_editor_tabs_to_columns() {
        assert_eq!(expand_workspace_editor_tabs("\talpha", 4), "    alpha");
        assert_eq!(expand_workspace_editor_tabs("a\tbeta", 4), "a   beta");
        assert_eq!(
            expand_workspace_editor_tabs("abcd\tbeta", 4),
            "abcd    beta"
        );
        assert_eq!(expand_workspace_editor_tabs("a\r\n\tb", 2), "a\r\n  b");
        assert_eq!(expand_workspace_editor_tabs("\tx", 0), " x");
        assert_eq!(expand_workspace_editor_tabs("\tx", 99), "        x");
    }

    #[test]
    fn encodes_workspace_editor_text_without_touching_unchanged_raw_tabs() {
        let raw = "\talpha\n\tbeta\n";
        let display = expand_workspace_editor_tabs(raw, 4);

        assert_eq!(
            encode_workspace_editor_text_for_save(&display, Some(raw), Some(&display)),
            raw
        );
    }

    #[test]
    fn encodes_workspace_editor_text_preserving_unchanged_edges() {
        let raw = "\talpha\n\tbeta\n\tgamma\n";
        let display = expand_workspace_editor_tabs(raw, 4);
        let edited = "    alpha\n    beta changed\n    gamma\n";

        assert_eq!(
            encode_workspace_editor_text_for_save(edited, Some(raw), Some(&display)),
            "\talpha\n    beta changed\n\tgamma\n"
        );
    }

    #[test]
    fn encodes_workspace_editor_text_falls_back_without_original_snapshots() {
        assert_eq!(
            encode_workspace_editor_text_for_save("    alpha\n", None, None),
            "    alpha\n"
        );
    }
}
