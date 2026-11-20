//! Safe, bounded two-column projection of a single-file unified Git patch.
//! It never reads a worktree or changes the index: all input is a read-only
//! comparison already obtained by the caller.
use std::mem;

const MAX_DIFF_BYTES: usize = 128 * 1024 + 160;
const MAX_ROWS: usize = 8192;
const MAX_CELL_CHARS: usize = 2048;

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
