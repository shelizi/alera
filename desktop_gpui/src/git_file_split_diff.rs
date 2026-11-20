//! Safe, bounded two-column projection of a single-file unified Git patch.
//! It never reads a worktree or changes the index: all input is a read-only
//! comparison already obtained by the caller.
use std::mem;

const MAX_DIFF_BYTES: usize = 128 * 1024 + 160;
const MAX_ROWS: usize = 8192;
const MAX_CELL_CHARS: usize = 2048;
/// The same virtual-row ceiling applies to the full-file projection: if the
/// file will not fit, the caller must retain the bounded Diff-only preview.
pub const MAX_FULL_FILE_LINES: usize = MAX_ROWS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitDiffCellKind {
    Context,
    Removed,
    Added,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitDiffCell {
    pub line: usize,
    pub text: String,
    pub kind: GitDiffCellKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitSplitDiffRow {
    pub old: Option<GitDiffCell>,
    pub new: Option<GitDiffCell>,
    pub label: Option<String>,
}

impl GitSplitDiffRow {
    fn label(text: impl Into<String>) -> Self {
        Self {
            old: None,
            new: None,
            label: Some(text.into()),
        }
    }
    fn paired(old: Option<GitDiffCell>, new: Option<GitDiffCell>) -> Self {
        Self {
            old,
            new,
            label: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GitSplitDiffView {
    pub rows: Vec<GitSplitDiffRow>,
    pub additions: usize,
    pub deletions: usize,
    pub clipped: bool,
}

fn hunk_starts(line: &str) -> Option<(usize, usize)> {
    let text = line.strip_prefix("@@ ")?;
    let (ranges, _) = text.split_once(" @@")?;
    let (old, new) = ranges.split_once(' ')?;
    let number = |spec: &str, sign: char| {
        spec.strip_prefix(sign)?
            .split(',')
            .next()?
            .parse::<usize>()
            .ok()
    };
    Some((number(old, '-')?, number(new, '+')?))
}

fn clip_cell(input: &str, clipped: &mut bool) -> String {
    let mut chars = input.chars();
    let text: String = chars.by_ref().take(MAX_CELL_CHARS).collect();
    if chars.next().is_some() {
        *clipped = true;
        format!("{text} … [display line clipped]")
    } else {
        text
    }
}

fn flush_pair(
    out: &mut GitSplitDiffView,
    removed: &mut Vec<GitDiffCell>,
    added: &mut Vec<GitDiffCell>,
) {
    let old = mem::take(removed);
    let new = mem::take(added);
    for index in 0..old.len().max(new.len()) {
        if out.rows.len() >= MAX_ROWS {
            out.clipped = true;
            return;
        }
        out.rows.push(GitSplitDiffRow::paired(
            old.get(index).cloned(),
            new.get(index).cloned(),
        ));
    }
}

/// Align consecutive '-'/ '+' change blocks within each hunk. Context
/// occupies both columns; removed-only and added-only rows keep an empty peer.
/// A changed line is counted once on its actual source side.
pub fn align_git_file_patch(patch: &str) -> GitSplitDiffView {
    let mut out = GitSplitDiffView::default();
    if patch.len() > MAX_DIFF_BYTES {
        out.rows.push(GitSplitDiffRow::label(
            "Comparison exceeded preview safety limit",
        ));
        out.clipped = true;
        return out;
    }
    let (mut old_number, mut new_number) = (None::<usize>, None::<usize>);
    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut in_hunk = false;
    for line in patch.lines().map(|line| line.trim_end_matches('\r')) {
        if out.rows.len() >= MAX_ROWS {
            out.clipped = true;
            break;
        }
        if let Some((old, new)) = hunk_starts(line) {
            flush_pair(&mut out, &mut removed, &mut added);
            out.rows.push(GitSplitDiffRow::label(line));
            old_number = Some(old);
            new_number = Some(new);
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            // Git metadata (paths, hashes and ---/+++ markers) is not a file
            // line and must never acquire a fake old/new line number.
            if line.starts_with("Binary files ")
                || line.starts_with("No differences ")
                || line.starts_with("… ")
                || line.starts_with("Diff unavailable")
            {
                out.rows.push(GitSplitDiffRow::label(line));
            }
            continue;
        }
        if let Some(text) = line.strip_prefix('-') {
            if let Some(number) = old_number {
                removed.push(GitDiffCell {
                    line: number,
                    text: clip_cell(text, &mut out.clipped),
                    kind: GitDiffCellKind::Removed,
                });
                old_number = number.checked_add(1);
                out.deletions += 1;
            }
        } else if let Some(text) = line.strip_prefix('+') {
            if let Some(number) = new_number {
                added.push(GitDiffCell {
                    line: number,
                    text: clip_cell(text, &mut out.clipped),
                    kind: GitDiffCellKind::Added,
                });
                new_number = number.checked_add(1);
                out.additions += 1;
            }
        } else if let Some(text) = line.strip_prefix(' ') {
            flush_pair(&mut out, &mut removed, &mut added);
            if let (Some(old), Some(new)) = (old_number, new_number) {
                let content = clip_cell(text, &mut out.clipped);
                out.rows.push(GitSplitDiffRow::paired(
                    Some(GitDiffCell {
                        line: old,
                        text: content.clone(),
                        kind: GitDiffCellKind::Context,
                    }),
                    Some(GitDiffCell {
                        line: new,
                        text: content,
                        kind: GitDiffCellKind::Context,
                    }),
                ));
                old_number = old.checked_add(1);
                new_number = new.checked_add(1);
            }
        } else if line.starts_with("\\ No newline at end of file") {
            flush_pair(&mut out, &mut removed, &mut added);
            out.rows.push(GitSplitDiffRow::label(line));
        } else if line.starts_with("… ") {
            flush_pair(&mut out, &mut removed, &mut added);
            out.rows.push(GitSplitDiffRow::label(line));
            out.clipped = true;
            in_hunk = false;
        } else if line.starts_with("diff --git ") || line.starts_with("@@@ ") {
            // Never reinterpret combined-parent patches as two-parent rows.
            flush_pair(&mut out, &mut removed, &mut added);
            if line.starts_with("@@@ ") {
                out.rows.push(GitSplitDiffRow::label(
                    "Combined-parent diff is not supported by this two-column viewer",
                ));
            }
            old_number = None;
            new_number = None;
            in_hunk = false;
        }
    }
    flush_pair(&mut out, &mut removed, &mut added);
    if out.clipped {
        out.rows.push(GitSplitDiffRow::label(
            "Preview clipped · switch to Unified for the bounded original patch",
        ));
    } else if out.rows.is_empty() {
        out.rows
            .push(GitSplitDiffRow::label("No text changes to display"));
    }
    out
}

/// Parse a unified hunk's next *physical* old/new line. A zero-count range
/// represents an insertion/deletion boundary *after* the numbered line; its
/// next physical line is therefore start+1, including the -0,0 start case.
fn full_file_hunk_cursors(header: &str) -> Option<(usize, usize)> {
    let ranges = header.strip_prefix("@@ ")?.split_once(" @@")?.0;
    let (old, new) = ranges.split_once(' ')?;
    let parse = |range: &str, sign: char| {
        let range = range.strip_prefix(sign)?;
        let (start, count) = range.split_once(',').unwrap_or((range, "1"));
        let start: usize = start.parse().ok()?;
        let count: usize = count.parse().ok()?;
        if count == 0 {
            start.checked_add(1)
        } else {
            Some(start)
        }
    };
    Some((parse(old, '-')?, parse(new, '+')?))
}

fn append_unchanged_gap(
    out: &mut GitSplitDiffView,
    old_lines: &[&str],
    new_lines: &[&str],
    old_cursor: &mut usize,
    new_cursor: &mut usize,
    old_until: usize,
    new_until: usize,
) -> Result<(), String> {
    if old_until < *old_cursor || new_until < *new_cursor {
        return Err("Git hunk cursor moved backwards".into());
    }
    let old_gap = old_until - *old_cursor;
    if old_gap != new_until - *new_cursor {
        return Err("Git full-file comparison has inconsistent hunk line positions".into());
    }
    for _ in 0..old_gap {
        if out.rows.len() >= MAX_FULL_FILE_LINES {
            return Err("Full-file comparison exceeds 8,192 rows; use Differences only".into());
        }
        let old_text = old_lines
            .get(*old_cursor - 1)
            .ok_or("Committed file changed while building the preview")?;
        let new_text = new_lines
            .get(*new_cursor - 1)
            .ok_or("Working file changed while building the preview")?;
        if old_text != new_text {
            return Err("Unchanged lines differ; reopen the Git comparison".into());
        }
        let text = clip_cell(old_text, &mut out.clipped);
        out.rows.push(GitSplitDiffRow::paired(
            Some(GitDiffCell {
                line: *old_cursor,
                text: text.clone(),
                kind: GitDiffCellKind::Context,
            }),
            Some(GitDiffCell {
                line: *new_cursor,
                text,
                kind: GitDiffCellKind::Context,
            }),
        ));
        *old_cursor += 1;
        *new_cursor += 1;
    }
    Ok(())
}

/// Expand a *complete* single-file Git patch back into an aligned full-file
/// projection. Identical regions outside hunks come from checked source
/// snapshots rather than being guessed from 3 context lines.
///
/// Fail closed on truncated/mismatched patches so "Full file" never silently
/// presents an incomplete or incorrect source document.
pub fn expand_git_patch_to_full_file(
    patch: &str,
    old_text: &str,
    new_text: &str,
) -> Result<GitSplitDiffView, String> {
    let old_lines = old_text.lines().collect::<Vec<_>>();
    let new_lines = new_text.lines().collect::<Vec<_>>();
    if old_lines.len() > MAX_FULL_FILE_LINES || new_lines.len() > MAX_FULL_FILE_LINES {
        return Err("Full-file preview exceeds 8,192 lines; use Differences only".into());
    }
    let changes = align_git_file_patch(patch);
    if changes.clipped {
        return Err("Git patch was clipped; full-file alignment is unavailable".into());
    }
    let mut out = GitSplitDiffView::default();
    let (mut old_cursor, mut new_cursor) = (1usize, 1usize);
    let mut had_hunk = false;
    for row in changes.rows {
        if let Some(header) = row.label {
            if let Some((old_start, new_start)) = full_file_hunk_cursors(&header) {
                had_hunk = true;
                append_unchanged_gap(
                    &mut out,
                    &old_lines,
                    &new_lines,
                    &mut old_cursor,
                    &mut new_cursor,
                    old_start,
                    new_start,
                )?;
            }
            // Hunk metadata and \No newline sentinels aren't file contents.
            continue;
        }
        if out.rows.len() >= MAX_FULL_FILE_LINES {
            return Err("Full-file comparison exceeds 8,192 rows; use Differences only".into());
        }
        for cell in [&row.old, &row.new].into_iter().flatten() {
            let (lines, cursor) = if cell.kind == GitDiffCellKind::Removed {
                (&old_lines, &mut old_cursor)
            } else if cell.kind == GitDiffCellKind::Added {
                (&new_lines, &mut new_cursor)
            } else {
                // Context pairs are validated separately below.
                continue;
            };
            if cell.line != *cursor || lines.get(*cursor - 1).is_none_or(|line| *line != cell.text)
            {
                return Err("Diff contents differ from full-file source; reopen comparison".into());
            }
            *cursor += 1;
        }
        // For context rows, both sides advance, and are verified against
        // their respective original snapshots including Unicode and CRLF.
        if row
            .old
            .as_ref()
            .is_some_and(|c| c.kind == GitDiffCellKind::Context)
        {
            for (cell, lines, cursor) in [
                (row.old.as_ref(), &old_lines, &mut old_cursor),
                (row.new.as_ref(), &new_lines, &mut new_cursor),
            ] {
                let cell = cell.ok_or("Missing peer context line in Git diff")?;
                if cell.kind != GitDiffCellKind::Context
                    || cell.line != *cursor
                    || lines.get(*cursor - 1).is_none_or(|text| *text != cell.text)
                {
                    return Err("Git context changed while building full-file preview".into());
                }
                *cursor += 1;
            }
        }
        out.additions += usize::from(
            row.new
                .as_ref()
                .is_some_and(|c| c.kind == GitDiffCellKind::Added),
        );
        out.deletions += usize::from(
            row.old
                .as_ref()
                .is_some_and(|c| c.kind == GitDiffCellKind::Removed),
        );
        out.rows.push(row);
    }
    if !had_hunk && old_lines != new_lines {
        return Err("File contents differ but Git returned no complete text hunk".into());
    }
    append_unchanged_gap(
        &mut out,
        &old_lines,
        &new_lines,
        &mut old_cursor,
        &mut new_cursor,
        old_lines.len() + 1,
        new_lines.len() + 1,
    )?;
    if out.clipped {
        return Err("Full-file preview contains clipped lines; use Differences only".into());
    }
    if out.rows.is_empty() {
        out.rows
            .push(GitSplitDiffRow::label("Both files are empty"));
    }
    Ok(out)
}

/// A full-file unified viewport uses the same verified source rows, without
/// recreating or re-diffing files. Replacements become old then new lines.
pub fn flatten_full_file_rows(full: &GitSplitDiffView) -> Vec<GitDiffCell> {
    let mut out = Vec::with_capacity(full.rows.len());
    for row in &full.rows {
        match (&row.old, &row.new) {
            (Some(old), Some(new))
                if old.kind == GitDiffCellKind::Context && new.kind == GitDiffCellKind::Context =>
            {
                out.push(new.clone());
            }
            (old, new) => {
                if let Some(old) = old {
                    out.push(old.clone());
                }
                if let Some(new) = new {
                    out.push(new.clone());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_inspect::{read_file_diff_page_for_area, GitChangeArea, GitSnapshot};
    use std::{fs, path::Path};

    #[test]
    fn replacement_and_context_have_aligned_numbers_and_colors() {
        let patch = "diff --git a/a b/a\nindex 111..222\n--- a/a\n+++ b/a\n@@ -8,3 +8,4 @@\n context\n-old A\n-old B\n+new A\n+new B\n+extra\n tail\n";
        let view = align_git_file_patch(patch);
        assert_eq!((view.additions, view.deletions), (3, 2));
        assert_eq!(view.rows[0].label.as_deref(), Some("@@ -8,3 +8,4 @@"));
        assert_eq!(
            (
                view.rows[1].old.as_ref().unwrap().line,
                view.rows[1].new.as_ref().unwrap().line
            ),
            (8, 8)
        );
        assert_eq!(
            (
                view.rows[2].old.as_ref().unwrap().line,
                view.rows[2].new.as_ref().unwrap().line
            ),
            (9, 9)
        );
        assert_eq!(
            view.rows[2].old.as_ref().unwrap().kind,
            GitDiffCellKind::Removed
        );
        assert_eq!(
            view.rows[2].new.as_ref().unwrap().kind,
            GitDiffCellKind::Added
        );
        assert!(view.rows[4].old.is_none());
        assert_eq!(view.rows[4].new.as_ref().unwrap().line, 11);
        assert_eq!(
            (
                view.rows[5].old.as_ref().unwrap().line,
                view.rows[5].new.as_ref().unwrap().line
            ),
            (11, 12)
        );
    }

    #[test]
    fn multi_hunk_deletion_empty_line_and_crlf_are_safe() {
        let patch = "@@ -1,2 +1,1 @@\r\n-first\r\n \r\n@@ -90,2 +89,3 @@ later\r\n last\r\n+\r\n+second\r\n\\ No newline at end of file\r\n";
        let v = align_git_file_patch(patch);
        assert_eq!((v.additions, v.deletions), (2, 1));
        assert!(v
            .rows
            .iter()
            .any(|r| r.old.as_ref().is_some_and(|cell| cell.line == 90)));
        assert!(v
            .rows
            .iter()
            .any(|r| r.new.as_ref().is_some_and(|cell| cell.text.is_empty())));
        assert!(v.rows.iter().any(|r| r
            .label
            .as_ref()
            .is_some_and(|text| text.contains("No newline"))));
    }

    #[test]
    fn no_diff_binary_and_malformed_hunks_have_safe_fallbacks() {
        let empty =
            align_git_file_patch("No differences between HEAD and the current working file.");
        assert_eq!(empty.rows.len(), 1);
        assert!(empty.rows[0].label.is_some());
        let binary = align_git_file_patch("Binary files a/x and b/x differ\n");
        assert!(binary.rows[0].label.as_ref().unwrap().contains("Binary"));
        let malformed = align_git_file_patch("@@ -not-a-number +3 @@\n+data\n");
        assert_eq!(malformed.additions, 0);
        assert_eq!(
            malformed.rows[0].label.as_deref(),
            Some("No text changes to display")
        );
    }

    #[test]
    fn oversized_patch_and_display_cells_clip_without_panicking() {
        assert!(align_git_file_patch(&"x".repeat(MAX_DIFF_BYTES + 1)).clipped);
        let patch = format!(
            "@@ -1 +1 @@\n-{}\n+value\n",
            "長".repeat(MAX_CELL_CHARS + 9)
        );
        let v = align_git_file_patch(&patch);
        assert!(v.clipped);
        assert!(v.rows.iter().any(|row| row
            .old
            .as_ref()
            .is_some_and(|cell| cell.text.contains("[display line clipped]"))));
    }

    #[test]
    fn full_file_includes_unmodified_prefix_suffix_and_middle() {
        let old = "start\nsame1\nwas\nsame2\nsame3\nsame4\nend\n";
        let new = "start\nsame1\nnow\nsame2\nsame3\nsame4\nend\n";
        let patch =
            "diff --git a/a b/a\n@@ -2,5 +2,5 @@\n same1\n-was\n+now\n same2\n same3\n same4\n";
        let full = expand_git_patch_to_full_file(patch, old, new).unwrap();
        assert_eq!(full.rows.len(), 7);
        assert_eq!((full.additions, full.deletions), (1, 1));
        assert_eq!(full.rows[0].old.as_ref().unwrap().text, "start");
        assert_eq!(full.rows[6].new.as_ref().unwrap().text, "end");
        assert_eq!(full.rows[2].old.as_ref().unwrap().text, "was");
        assert_eq!(full.rows[2].new.as_ref().unwrap().text, "now");
        assert!(full.rows.iter().all(|r| r.label.is_none()));
        let flat = flatten_full_file_rows(&full);
        assert_eq!(flat.len(), 8);
        assert_eq!(flat[2].kind, GitDiffCellKind::Removed);
        assert_eq!(flat[3].kind, GitDiffCellKind::Added);
        assert_eq!(flat[7].text, "end");
    }

    #[test]
    fn full_file_handles_far_apart_hunks_and_zero_count_boundaries() {
        let mut old = (1..=24).map(|i| format!("line {i}")).collect::<Vec<_>>();
        let mut new = old.clone();
        old[3] = "old fourth".into();
        new[3] = "new fourth".into();
        old[20] = "old twenty-one".into();
        new[20] = "new twenty-one".into();
        let old_text = format!("{}\n", old.join("\n"));
        let new_text = format!("{}\n", new.join("\n"));
        let patch = "@@ -3,3 +3,3 @@\n line 3\n-old fourth\n+new fourth\n line 5\n@@ -20,3 +20,3 @@\n line 20\n-old twenty-one\n+new twenty-one\n line 22\n";
        let full = expand_git_patch_to_full_file(patch, &old_text, &new_text).unwrap();
        assert_eq!(full.rows.len(), 24);
        assert_eq!((full.additions, full.deletions), (2, 2));
        assert_eq!(full.rows[11].old.as_ref().unwrap().text, "line 12");

        let old = "first\nthird\n";
        let new = "first\nsecond\nthird\n";
        let patch = "@@ -1,0 +2 @@\n+second\n";
        let full = expand_git_patch_to_full_file(patch, old, new).unwrap();
        assert_eq!(full.rows.len(), 3);
        assert!(full.rows[1].old.is_none());
        assert_eq!(full.rows[1].new.as_ref().unwrap().line, 2);
        let deleted = expand_git_patch_to_full_file("@@ -2 +1,0 @@\n-second\n", new, old).unwrap();
        assert_eq!(deleted.rows.len(), 3);
        assert!(deleted.rows[1].new.is_none());
    }

    #[test]
    fn unchanged_full_file_and_deleted_file_keep_every_line() {
        let text = "第一行\r\n第二行\r\n";
        let view = expand_git_patch_to_full_file(
            "No differences between HEAD and the current working file.",
            text,
            text,
        )
        .unwrap();
        assert_eq!(view.rows.len(), 2);
        assert_eq!(view.rows[1].old.as_ref().unwrap().text, "第二行");
        assert_eq!((view.additions, view.deletions), (0, 0));
        let deleted = expand_git_patch_to_full_file(
            "@@ -1,2 +0,0 @@\n-第一行\n-第二行\n",
            "第一行\n第二行\n",
            "",
        )
        .unwrap();
        assert_eq!(deleted.rows.len(), 2);
        assert!(deleted.rows.iter().all(|row| row.new.is_none()));
    }

    #[test]
    fn full_file_refuses_partial_or_inconsistent_views() {
        assert!(expand_git_patch_to_full_file(
            "@@ -2 +2 @@\n-old\n+new\n",
            "first\nold\n",
            "first\nwrong\n"
        )
        .is_err());
        assert!(expand_git_patch_to_full_file(
            "@@ -1 +1 @@\n-old\n+new\n… Comparison truncated at 128 KiB. No files were modified.",
            "old\n",
            "new\n",
        )
        .is_err());
        assert!(expand_git_patch_to_full_file("", "alpha\n", "beta\n").is_err());
        assert!(expand_git_patch_to_full_file(
            "",
            &"hello\n".repeat(MAX_FULL_FILE_LINES + 1),
            &"hello\n".repeat(MAX_FULL_FILE_LINES + 1),
        )
        .is_err());
    }

    #[test]
    fn real_staged_unstaged_and_conflicted_git_areas_remain_isolated() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        let relative = Path::new("dual.txt");
        let path = temp.path().join(relative);
        fs::write(&path, "base\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        index.write().unwrap();
        let sig = git2::Signature::now("Diff QA", "qa@example.invalid").unwrap();
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
            .unwrap();
        drop(tree);
        fs::write(&path, "staged\n").unwrap();
        index.add_path(relative).unwrap();
        index.write().unwrap();
        drop(index);
        fs::write(&path, "working\n").unwrap();
        let before_index = fs::read(repo.path().join("index")).unwrap();
        let before_head = repo.head().unwrap().target();
        let rows = GitSnapshot::load(temp.path()).unwrap().visible_rows("");
        assert_eq!(
            rows.iter().filter(|row| row.relative == relative).count(),
            2
        );
        assert!(rows.iter().any(|row| row.area == GitChangeArea::Staged));
        assert!(rows.iter().any(|row| row.area == GitChangeArea::Changes));
        let staged =
            read_file_diff_page_for_area(temp.path(), relative, GitChangeArea::Staged, 0).unwrap();
        let working =
            read_file_diff_page_for_area(temp.path(), relative, GitChangeArea::Changes, 0).unwrap();
        let staged_split = align_git_file_patch(&staged.text);
        let working_split = align_git_file_patch(&working.text);
        assert!(staged_split
            .rows
            .iter()
            .any(|row| row.new.as_ref().is_some_and(|cell| cell.text == "staged")));
        assert!(!staged_split
            .rows
            .iter()
            .any(|row| row.new.as_ref().is_some_and(|cell| cell.text == "working")));
        assert!(working_split
            .rows
            .iter()
            .any(|row| row.new.as_ref().is_some_and(|cell| cell.text == "working")));
        assert!(!working_split
            .rows
            .iter()
            .any(|row| row.new.as_ref().is_some_and(|cell| cell.text == "staged")));
        assert_eq!(fs::read(repo.path().join("index")).unwrap(), before_index);
        assert_eq!(repo.head().unwrap().target(), before_head);

        // Divergent, true two-parent merge conflicts in a disposable repo:
        // normal Staged/Changes viewers must refuse the unresolved file.
        fs::write(&path, "ours\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        index.write().unwrap();
        let base = repo.find_commit(base_oid).unwrap();
        let ours_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "ours", &tree, &[&base])
            .unwrap();
        drop(tree);
        drop(index);
        let base_tree = base.tree().unwrap();
        let mut builder = repo.treebuilder(Some(&base_tree)).unwrap();
        let theirs_blob = repo.blob(b"theirs\n").unwrap();
        builder.insert("dual.txt", theirs_blob, 0o100644).unwrap();
        let theirs_tree = repo.find_tree(builder.write().unwrap()).unwrap();
        let theirs_oid = repo
            .commit(None, &sig, &sig, "theirs", &theirs_tree, &[&base])
            .unwrap();
        assert_eq!(repo.head().unwrap().target(), Some(ours_oid));
        let annotated = repo.find_annotated_commit(theirs_oid).unwrap();
        repo.merge(&[&annotated], None, None).unwrap();
        assert!(repo.index().unwrap().has_conflicts());
        let conflicted = GitSnapshot::load(temp.path()).unwrap().visible_rows("");
        assert!(conflicted
            .iter()
            .any(|row| row.area == GitChangeArea::Conflicts));
        for area in [
            GitChangeArea::Conflicts,
            GitChangeArea::Staged,
            GitChangeArea::Changes,
        ] {
            assert!(read_file_diff_page_for_area(temp.path(), relative, area, 0).is_err());
        }
    }
}
