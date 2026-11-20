//! Immutable, bounded three-way Git conflict projections.
//! Reads only Git index stages (or an already computed in-memory merge plan);
//! no checkout, Git action, worktree write or index mutation is performed.
use git2::{IndexEntry, Repository};
use similar::{ChangeTag, TextDiff};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

const MAX_STAGE_BYTES: usize = 64 * 1024;
const MAX_STAGE_LINES: usize = 4096;
const MAX_DISPLAY_LINE_CHARS: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictLine {
    pub number: usize,
    pub text: String,
    /// UTF-8 byte offsets suitable for GPUI StyledText highlights.
    pub highlights: Vec<Range<usize>>,
    pub changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictVersion {
    pub lines: Vec<ConflictLine>,
    pub changed_lines: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitConflictComparison {
    pub relative: PathBuf,
    pub base: Option<ConflictVersion>,
    pub ours: Option<ConflictVersion>,
    pub theirs: Option<ConflictVersion>,
    /// Shared visual row coordinates for all three versions. A None cell is
    /// deliberate padding, not an invented line or Git index stage.
    pub aligned_rows: Vec<ConflictAlignedRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictAlignedRow {
    /// Indices into base / ours / theirs ConflictVersion.lines respectively.
    pub indices: [Option<usize>; 3],
    pub changed: bool,
}

struct SideLineMap {
    per_reference: Vec<Option<usize>>,
    inserted_before: Vec<Vec<usize>>,
}

/// Match one side to a stable reference line by line. Insertions get their
/// own visual rows before the next reference line; deletions occupy a blank
/// cell opposite the reference instead of shifting all later line numbers.
fn map_side_lines(reference: &[ConflictLine], side: Option<&ConflictVersion>) -> SideLineMap {
    let mut map = SideLineMap {
        per_reference: vec![None; reference.len()],
        inserted_before: vec![Vec::new(); reference.len() + 1],
    };
    let Some(side) = side else {
        return map;
    };
    let base_text = reference
        .iter()
        .map(|line| format!("{}\n", line.text))
        .collect::<String>();
    let side_text = side
        .lines
        .iter()
        .map(|line| format!("{}\n", line.text))
        .collect::<String>();
    for op in TextDiff::from_lines(&base_text, &side_text).ops() {
        let old = op.old_range();
        let new = op.new_range();
        match op.tag() {
            similar::DiffTag::Equal => {
                for (base, side) in old.zip(new) {
                    map.per_reference[base] = Some(side);
                }
            }
            similar::DiffTag::Delete => {}
            similar::DiffTag::Insert => {
                map.inserted_before[old.start].extend(new);
            }
            similar::DiffTag::Replace => {
                let paired = old.len().min(new.len());
                for offset in 0..paired {
                    map.per_reference[old.start + offset] = Some(new.start + offset);
                }
                map.inserted_before[old.end].extend((new.start + paired)..new.end);
            }
        }
    }
    map
}

fn build_aligned_rows(
    base: Option<&ConflictVersion>,
    ours: Option<&ConflictVersion>,
    theirs: Option<&ConflictVersion>,
) -> Vec<ConflictAlignedRow> {
    // Add/add has no stage 1. Use Ours as a reference without manufacturing
    // base content; this also handles either side being absent or empty.
    if base.is_none() {
        let Some(reference) = ours.or(theirs) else {
            return Vec::new();
        };
        let reference_is_ours = ours.is_some();
        let other = if reference_is_ours { theirs } else { ours };
        let other_map = map_side_lines(&reference.lines, other);
        let mut rows = Vec::new();
        for gap in 0..=reference.lines.len() {
            for inserted in &other_map.inserted_before[gap] {
                rows.push(ConflictAlignedRow {
                    indices: [
                        None,
                        (!reference_is_ours).then_some(*inserted),
                        reference_is_ours.then_some(*inserted),
                    ],
                    changed: true,
                });
            }
            if gap < reference.lines.len() {
                let other_index = other_map.per_reference[gap];
                rows.push(ConflictAlignedRow {
                    indices: [
                        None,
                        if reference_is_ours {
                            Some(gap)
                        } else {
                            other_index
                        },
                        if reference_is_ours {
                            other_index
                        } else {
                            Some(gap)
                        },
                    ],
                    changed: true,
                });
            }
        }
        return rows;
    }
    let base = base.expect("base checked above");
    let ours_map = map_side_lines(&base.lines, ours);
    let theirs_map = map_side_lines(&base.lines, theirs);
    let mut rows = Vec::new();
    for gap in 0..=base.lines.len() {
        let our_inserts = &ours_map.inserted_before[gap];
        let their_inserts = &theirs_map.inserted_before[gap];
        for index in 0..our_inserts.len().max(their_inserts.len()) {
            rows.push(ConflictAlignedRow {
                indices: [
                    None,
                    our_inserts.get(index).copied(),
                    their_inserts.get(index).copied(),
                ],
                changed: true,
            });
        }
        if gap < base.lines.len() {
            let our_index = ours_map.per_reference[gap];
            let their_index = theirs_map.per_reference[gap];
            let changed = base.lines[gap].changed
                || our_index.is_none_or(|index| ours.is_some_and(|v| v.lines[index].changed))
                || their_index.is_none_or(|index| theirs.is_some_and(|v| v.lines[index].changed));
            rows.push(ConflictAlignedRow {
                indices: [Some(gap), our_index, their_index],
                changed,
            });
        }
    }
    rows
}

fn line_texts(text: &str) -> Vec<&str> {
    text.lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect()
}

fn merged_spans(mut spans: Vec<Range<usize>>) -> Vec<Range<usize>> {
    spans.retain(|range| range.start < range.end);
    spans.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for span in spans {
        if let Some(last) = merged.last_mut() {
            if span.start <= last.end {
                last.end = last.end.max(span.end);
                continue;
            }
        }
        merged.push(span);
    }
    merged
}

fn inline_spans(old: Option<&str>, new: &str) -> Vec<Range<usize>> {
    let Some(old) = old else {
        return (!new.is_empty())
            .then_some(0..new.len())
            .into_iter()
            .collect();
    };
    if old == new {
        return Vec::new();
    }
    let mut offset = 0;
    let mut spans = Vec::new();
    for change in TextDiff::from_chars(old, new).iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => {}
            ChangeTag::Equal => offset += change.value().len(),
            ChangeTag::Insert => {
                let start = offset;
                offset += change.value().len();
                spans.push(start..offset);
            }
        }
    }
    // A deletion-only edit still needs a visible changed-line marker. Avoid
    // pretending that unchanged characters were inserted into the file.
    merged_spans(spans)
}

fn build_version(text: &str, reference: Option<&str>) -> ConflictVersion {
    let original = line_texts(text);
    let mut lines = original
        .iter()
        .enumerate()
        .map(|(i, line)| ConflictLine {
            number: i + 1,
            text: (*line).to_owned(),
            highlights: Vec::new(),
            changed: false,
        })
        .collect::<Vec<_>>();

    if let Some(reference) = reference {
        let base = line_texts(reference);
        let diff = TextDiff::from_lines(reference, text);
        for op in diff.ops() {
            if op.tag() == similar::DiffTag::Equal {
                continue;
            }
            for index in op.new_range() {
                let old_index = op.old_range().start + index - op.new_range().start;
                let old = (old_index < op.old_range().end)
                    .then(|| base.get(old_index).copied())
                    .flatten();
                if let Some(line) = lines.get_mut(index) {
                    line.changed = true;
                    line.highlights = merged_spans(
                        line.highlights
                            .iter()
                            .cloned()
                            .chain(inline_spans(old, &line.text))
                            .collect(),
                    );
                }
            }
        }
    } else {
        for line in &mut lines {
            line.changed = true;
            line.highlights = inline_spans(None, &line.text);
        }
    }
    let changed_lines = lines.iter().filter(|line| line.changed).count();
    ConflictVersion {
        lines,
        changed_lines,
    }
}

fn valid_stage_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_STAGE_BYTES || text.contains('\0') {
        return Err("Binary or oversized Git conflict stage; use a dedicated merge tool".into());
    }
    if text.lines().count() > MAX_STAGE_LINES
        || text
            .lines()
            .any(|line| line.chars().count() > MAX_DISPLAY_LINE_CHARS)
    {
        return Err(
            "Conflict text exceeds safe preview limits (4,096 lines / 2,048 chars per line)".into(),
        );
    }
    Ok(())
}

impl GitConflictComparison {
    /// Build an independent snapshot from either a simulated merge's index
    /// stages or the current unresolved index. No reference to Git is retained.
    pub fn from_versions(
        relative: PathBuf,
        base: Option<&str>,
        ours: Option<&str>,
        theirs: Option<&str>,
    ) -> Result<Self, String> {
        if !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
            || relative.as_os_str().is_empty()
        {
            return Err("Git conflict preview requires a relative file path".into());
        }
        if ours.is_none() && theirs.is_none() {
            return Err("Git conflict has no readable side".into());
        }
        for text in [base, ours, theirs].into_iter().flatten() {
            valid_stage_text(text)?;
        }
        // Base highlights show text removed or changed on either branch.
        let mut base_version = base.map(|text| build_version(text, ours));
        if let (Some(version), Some(text), Some(incoming)) = (&mut base_version, base, theirs) {
            let incoming_diff = build_version(text, Some(incoming));
            for (line, other) in version.lines.iter_mut().zip(incoming_diff.lines) {
                line.changed |= other.changed;
                line.highlights = merged_spans(
                    line.highlights
                        .iter()
                        .cloned()
                        .chain(other.highlights)
                        .collect(),
                );
            }
            version.changed_lines = version.lines.iter().filter(|line| line.changed).count();
        } else if let (Some(version), None) = (&mut base_version, theirs) {
            for line in &mut version.lines {
                line.changed = true;
                line.highlights = inline_spans(None, &line.text);
            }
            version.changed_lines = version.lines.len();
        }
        let ours_version = ours.map(|text| build_version(text, base));
        let theirs_version = theirs.map(|text| build_version(text, base));
        let aligned_rows = build_aligned_rows(
            base_version.as_ref(),
            ours_version.as_ref(),
            theirs_version.as_ref(),
        );
        Ok(Self {
            relative,
            base: base_version,
            ours: ours_version,
            theirs: theirs_version,
            aligned_rows,
        })
    }
}

fn read_stage(repo: &Repository, entry: Option<&IndexEntry>) -> Result<Option<String>, String> {
    let Some(entry) = entry else {
        return Ok(None);
    };
    if entry.mode != 0o100644 && entry.mode != 0o100755 {
        return Err("Symlink and submodule conflicts require a dedicated merge tool".into());
    }
    let blob = repo
        .find_blob(entry.id)
        .map_err(|error| error.to_string())?;
    let content = blob.content();
    if content.len() > MAX_STAGE_BYTES || content.contains(&0) {
        return Err("Binary or oversized Git conflict stage; use a dedicated merge tool".into());
    }
    let text = std::str::from_utf8(content)
        .map_err(|_| "Non-UTF-8 Git conflict stage; use a dedicated merge tool".to_string())?;
    valid_stage_text(text)?;
    Ok(Some(text.to_owned()))
}

/// Read only index conflict stages 1/2/3, never the conflict markers in the
/// worktree. This works even while MERGE_HEAD/CHERRY_PICK_HEAD is present.
pub fn preview_index_conflict(
    root: &Path,
    relative: &Path,
) -> Result<GitConflictComparison, String> {
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("Select a relative conflicted file path".into());
    }
    let repo = Repository::open(root).map_err(|error| error.to_string())?;
    let index = repo.index().map_err(|error| error.to_string())?;
    let requested = relative.to_string_lossy().replace('\\', "/");
    for entry in index.conflicts().map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let candidate = entry
            .our
            .as_ref()
            .or(entry.their.as_ref())
            .or(entry.ancestor.as_ref())
            .ok_or("Git index conflict has no stage entries")?;
        if candidate.path != requested.as_bytes() {
            continue;
        }
        for stage in [&entry.ancestor, &entry.our, &entry.their]
            .into_iter()
            .flatten()
        {
            if stage.path != candidate.path {
                return Err("Renamed conflict paths require a dedicated merge tool".into());
            }
        }
        let base = read_stage(&repo, entry.ancestor.as_ref())?;
        let ours = read_stage(&repo, entry.our.as_ref())?;
        let theirs = read_stage(&repo, entry.their.as_ref())?;
        return GitConflictComparison::from_versions(
            relative.to_path_buf(),
            base.as_deref(),
            ours.as_deref(),
            theirs.as_deref(),
        );
    }
    Err("Selected file has no unresolved Git index stages; refresh Source Control".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn aligned_rows_keep_common_lines_at_same_visual_row_after_independent_inserts() {
        let view = GitConflictComparison::from_versions(
            "src/fixture.txt".into(),
            Some("header\ncommon b\ncommon c\ntail\n"),
            Some("header\nOURS INSERT\ncommon b\ncommon c\ntail\n"),
            Some("header\ncommon b\ncommon c\nTHEIRS INSERT\ntail\n"),
        )
        .unwrap();
        let positions = view
            .aligned_rows
            .iter()
            .map(|row| row.indices)
            .collect::<Vec<_>>();
        assert_eq!(
            positions,
            vec![
                [Some(0), Some(0), Some(0)],
                [None, Some(1), None],
                [Some(1), Some(2), Some(1)],
                [Some(2), Some(3), Some(2)],
                [None, None, Some(3)],
                [Some(3), Some(4), Some(4)],
            ]
        );
        assert_eq!(
            view.aligned_rows
                .iter()
                .enumerate()
                .filter_map(|(i, row)| row.changed.then_some(i))
                .collect::<Vec<_>>(),
            vec![1, 4]
        );
    }

    #[test]
    fn aligned_rows_preserve_deletions_and_real_unicode_line_numbers() {
        let view = GitConflictComparison::from_versions(
            "文檔.txt".into(),
            Some("開頭\n刪除這行\n共同內容\n結尾\n"),
            Some("開頭\n共同內容\n結尾\n"),
            Some("開頭\n刪除這行\n共同內容\n傳入新增\n結尾\n"),
        )
        .unwrap();
        let positions = view
            .aligned_rows
            .iter()
            .map(|row| row.indices)
            .collect::<Vec<_>>();
        assert_eq!(
            positions,
            vec![
                [Some(0), Some(0), Some(0)],
                [Some(1), None, Some(1)],
                [Some(2), Some(1), Some(2)],
                [None, None, Some(3)],
                [Some(3), Some(2), Some(4)],
            ]
        );
        assert_eq!(view.ours.as_ref().unwrap().lines[1].number, 2);
        assert_eq!(view.theirs.as_ref().unwrap().lines[4].number, 5);
        assert!(view.aligned_rows[1].changed);
        assert!(!view.aligned_rows[2].changed);
    }

    #[test]
    fn add_add_with_no_base_aligns_existing_sides_and_keeps_stage_one_missing() {
        let view = GitConflictComparison::from_versions(
            "new.txt".into(),
            None,
            Some("first\nOURS ONLY\nlast\n"),
            Some("first\nlast\n"),
        )
        .unwrap();
        assert!(view.base.is_none());
        assert_eq!(view.aligned_rows.len(), 3);
        assert_eq!(view.aligned_rows[1].indices, [None, Some(1), None]);
        assert_eq!(view.aligned_rows[2].indices, [None, Some(2), Some(1)]);
    }

    #[test]
    fn inline_highlights_are_byte_safe_for_unicode_and_only_mark_changed_fragments() {
        let item = GitConflictComparison::from_versions(
            "src/demo.rs".into(),
            Some("prefix 中文 old suffix\ncommon\n"),
            Some("prefix 中文 新 suffix\ncommon\n"),
            Some("prefix 中文 theirs suffix\ncommon\n"),
        )
        .unwrap();
        let ours = &item.ours.unwrap().lines[0];
        assert!(ours.changed);
        assert!(!ours.highlights.is_empty());
        for span in &ours.highlights {
            assert!(ours.text.is_char_boundary(span.start));
            assert!(ours.text.is_char_boundary(span.end));
        }
        let marked = ours
            .highlights
            .iter()
            .map(|s| &ours.text[s.clone()])
            .collect::<String>();
        assert!(marked.contains('新'));
        assert!(!marked.contains("prefix"));
        assert!(!item.theirs.unwrap().lines[1].changed);
        assert_eq!(item.base.unwrap().changed_lines, 1);
    }

    #[test]
    fn add_add_and_delete_modify_support_missing_stages_without_fake_contents() {
        let added = GitConflictComparison::from_versions(
            "new.txt".into(),
            None,
            Some("ours\n"),
            Some("theirs\n"),
        )
        .unwrap();
        assert!(added.base.is_none());
        assert!(added.ours.unwrap().lines[0].changed);
        let deleted = GitConflictComparison::from_versions(
            "removed.txt".into(),
            Some("old\n"),
            None,
            Some("incoming\n"),
        )
        .unwrap();
        assert!(deleted.ours.is_none());
        assert_eq!(deleted.base.unwrap().changed_lines, 1);
        assert!(GitConflictComparison::from_versions(
            "../escape".into(),
            None,
            Some("x"),
            Some("y")
        )
        .is_err());
    }

    #[test]
    fn reads_real_merge_stages_without_changing_head_index_or_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        let path = temp.path().join("conflict.txt");
        fs::write(&path, "common\nbase\n").unwrap();
        let sig = git2::Signature::now("Conflict Test", "test@example.invalid").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("conflict.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        index.write().unwrap();
        let base_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
            .unwrap();
        drop(tree);
        let base = repo.find_commit(base_oid).unwrap();
        fs::write(&path, "common\nours\n").unwrap();
        index.add_path(Path::new("conflict.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        index.write().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "ours", &tree, &[&base])
            .unwrap();
        drop(tree);
        let base_tree = base.tree().unwrap();
        let mut builder = repo.treebuilder(Some(&base_tree)).unwrap();
        builder
            .insert(
                "conflict.txt",
                repo.blob(b"common\ntheirs\n").unwrap(),
                0o100644,
            )
            .unwrap();
        let their_tree = repo.find_tree(builder.write().unwrap()).unwrap();
        let their_oid = repo
            .commit(None, &sig, &sig, "theirs", &their_tree, &[&base])
            .unwrap();
        let annotated = repo.find_annotated_commit(their_oid).unwrap();
        let (analysis, _) = repo.merge_analysis(&[&annotated]).unwrap();
        assert!(analysis.is_normal());
        repo.merge(&[&annotated], None, None).unwrap();
        assert!(repo.index().unwrap().has_conflicts());
        let before_head = repo.head().unwrap().target();
        let before_index = fs::read(repo.path().join("index")).unwrap();
        let before_worktree = fs::read(&path).unwrap();
        let snapshot = preview_index_conflict(temp.path(), Path::new("conflict.txt")).unwrap();
        assert_eq!(snapshot.base.unwrap().lines[1].text, "base");
        assert_eq!(snapshot.ours.unwrap().lines[1].text, "ours");
        assert_eq!(snapshot.theirs.unwrap().lines[1].text, "theirs");
        assert_eq!(repo.head().unwrap().target(), before_head);
        assert_eq!(fs::read(repo.path().join("index")).unwrap(), before_index);
        assert_eq!(fs::read(&path).unwrap(), before_worktree);
        assert!(preview_index_conflict(temp.path(), Path::new("../outside.txt")).is_err());
    }

    #[test]
    fn binary_and_giant_stages_refuse_preview() {
        assert!(GitConflictComparison::from_versions(
            "a.txt".into(),
            None,
            Some("x\0y"),
            Some("z")
        )
        .is_err());
        let huge = "x".repeat(MAX_STAGE_BYTES + 1);
        assert!(
            GitConflictComparison::from_versions("a.txt".into(), None, Some(&huge), Some("z"))
                .is_err()
        );
    }
}
