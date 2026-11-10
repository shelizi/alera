//! Read-only, bounded Git inspection. No Git index/refs/worktree writes.
//! Background-only caller: Git status and patch calculation may touch many files.

use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_CHANGES: usize = 120;
const MAX_PATCH_BYTES: usize = 32 * 1024;
const MAX_PAGED_DIFF_LINES: usize = 100_000;

/// One bounded, read-only slice of a file's current Git patch. Cursors count
/// Git patch records rather than byte offsets, so multi-byte text is never
/// split across normal page boundaries. Every page is fetched independently.
#[derive(Debug, Clone)]
pub struct GitPatchPage {
    pub text: String,
    pub start_line: usize,
    pub next_line: Option<usize>,
    pub clipped_long_line: bool,
}

#[derive(Debug, Clone)]
pub struct GitChange {
    pub relative: PathBuf,
    pub label: &'static str,
}

#[derive(Debug, Clone)]
pub struct GitSnapshot {
    pub branch: String,
    pub changed_files: Vec<GitChange>,
    pub truncated: bool,
    pub repository_root: PathBuf,
}

impl GitSnapshot {
    pub fn load(workspace: &Path) -> Result<Self, String> {
        let repo = git2::Repository::discover(workspace).map_err(|e| e.to_string())?;
        let workdir = repo
            .workdir()
            .ok_or("A bare repository has no editable working tree")?;
        let root = dunce::canonicalize(workdir).map_err(|e| e.to_string())?;
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        let statuses = repo.statuses(Some(&mut opts)).map_err(|e| e.to_string())?;
        let mut changed_files = Vec::new();
        let mut truncated = false;
        for item in statuses.iter() {
            let Ok(path) = item.path() else { continue };
            let relative = PathBuf::from(path);
            if !safe_relative(&relative) {
                continue;
            }
            changed_files.push(GitChange {
                relative,
                label: classify(item.status()),
            });
            if changed_files.len() >= MAX_CHANGES {
                truncated = true;
                break;
            }
        }
        changed_files.sort_by(|a, b| a.relative.cmp(&b.relative));
        let branch = repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().ok().map(str::to_string))
            .unwrap_or_else(|| "(detached or unborn HEAD)".to_string());
        Ok(Self {
            branch,
            changed_files,
            truncated,
            repository_root: root,
        })
    }
}

fn safe_relative(path: &Path) -> bool {
    path.components().all(|c| matches!(c, Component::Normal(_))) && !path.as_os_str().is_empty()
}

fn classify(status: git2::Status) -> &'static str {
    use git2::Status;
    if status.contains(Status::CONFLICTED) {
        "CONFLICT"
    } else if status.intersects(Status::INDEX_DELETED | Status::WT_DELETED) {
        "D"
    } else if status.intersects(Status::INDEX_NEW | Status::WT_NEW) {
        "?"
    } else if status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
        "R"
    } else if status.intersects(Status::INDEX_MODIFIED | Status::WT_MODIFIED) {
        "M"
    } else if status.intersects(Status::INDEX_TYPECHANGE | Status::WT_TYPECHANGE) {
        "T"
    } else {
        "·"
    }
}

pub fn read_file_diff(workspace: &Path, relative: &Path) -> Result<String, String> {
    if !safe_relative(relative) {
        return Err("Unsafe Git path".into());
    }
    let repo = git2::Repository::discover(workspace).map_err(|e| e.to_string())?;
    let repo_root = repo.workdir().ok_or("No Git working tree")?;
    let mut options = git2::DiffOptions::new();
    options.pathspec(relative).context_lines(3);
    let diff = repo
        .diff_index_to_workdir(None, Some(&mut options))
        .map_err(|e| e.to_string())?;
    let mut output = String::new();
    let mut truncated = false;
    append_diff(&diff, &mut output, &mut truncated)?;
    if output.is_empty() {
        let tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
        let staged = repo
            .diff_tree_to_index(tree.as_ref(), None, Some(&mut options))
            .map_err(|e| e.to_string())?;
        append_diff(&staged, &mut output, &mut truncated)?;
    }
    if output.is_empty() {
        // Untracked files don't participate in Git's index-to-workdir diff.
        let file = repo_root.join(relative);
        let canonical = dunce::canonicalize(&file).map_err(|e| e.to_string())?;
        let root = dunce::canonicalize(repo_root).map_err(|e| e.to_string())?;
        if !canonical.starts_with(root) || !canonical.is_file() {
            return Err("Git preview target is not a regular workspace file".into());
        }
        let metadata = fs::metadata(&canonical).map_err(|e| e.to_string())?;
        if metadata.len() > MAX_PATCH_BYTES as u64 {
            return Ok("Untracked file · too large for native diff preview".into());
        }
        let text = fs::read_to_string(&canonical)
            .map_err(|_| "Binary or non-UTF-8 file; preview unavailable".to_string())?;
        output = format!("Untracked (read-only): {}\n{}", relative.display(), text);
    }
    if truncated {
        output.push_str("\n… diff preview truncated");
    }
    Ok(output)
}

/// Fetch a fresh segment without assembling the complete diff in memory.
/// Page navigation is intentionally limited to the selected changed file;
/// commit history and branch comparisons retain their own preview semantics.
///
/// A caller must replace the displayed page rather than concatenating pages:
/// worktree contents can change between independent requests.
pub fn read_file_diff_page(
    workspace: &Path,
    relative: &Path,
    start_line: usize,
) -> Result<GitPatchPage, String> {
    if !safe_relative(relative) {
        return Err("Unsafe Git path".into());
    }
    if start_line >= MAX_PAGED_DIFF_LINES {
        return Err("Git diff paging safety limit reached".into());
    }
    let repo = git2::Repository::discover(workspace).map_err(|e| e.to_string())?;
    let mut options = git2::DiffOptions::new();
    options.pathspec(relative).context_lines(3);
    let unstaged = repo
        .diff_index_to_workdir(None, Some(&mut options))
        .map_err(|e| e.to_string())?;
    if unstaged.deltas().len() != 0 {
        return render_git_diff_page(&unstaged, start_line);
    }

    let tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
    let staged = repo
        .diff_tree_to_index(tree.as_ref(), None, Some(&mut options))
        .map_err(|e| e.to_string())?;
    if staged.deltas().len() != 0 {
        return render_git_diff_page(&staged, start_line);
    }

    // An untracked file is not represented in the Git diff engine; preserve
    // the existing safe absolute-path/symlink/size checks for its preview.
    if start_line != 0 {
        return Err("Git patch changed; reopen its first page".into());
    }
    let mut text = read_file_diff(workspace, relative)?;
    let mut clipped = false;
    if text.len() > MAX_PATCH_BYTES {
        // The normal untracked file-size check counts file bytes, whereas
        // its display adds a read-only header. The *complete preview* must
        // still respect the same page budget and explicitly warn on loss.
        text.truncate(text.floor_char_boundary(MAX_PATCH_BYTES));
        text.push_str("\n… diff preview truncated");
        clipped = true;
    }
    Ok(GitPatchPage {
        text,
        start_line,
        next_line: None,
        clipped_long_line: clipped,
    })
}

fn render_git_diff_page(diff: &git2::Diff<'_>, start_line: usize) -> Result<GitPatchPage, String> {
    let mut text = String::new();
    let mut seen = 0usize;
    let mut next_line = None;
    let mut clipped_long_line = false;
    let mut deliberately_stopped = false;
    let mut safety_limit_reached = false;
    let print = diff.print(git2::DiffFormat::Patch, |_, _, line| {
        seen += 1;
        if seen > MAX_PAGED_DIFF_LINES {
            safety_limit_reached = true;
            deliberately_stopped = true;
            return false;
        }
        if seen <= start_line {
            return true;
        }
        let prefix = matches!(line.origin(), '+' | '-' | ' ');
        let raw = line.content();
        let requested = raw.len().saturating_add(usize::from(prefix));
        let remaining = MAX_PATCH_BYTES.saturating_sub(text.len());
        if clipped_long_line || (requested > remaining && !text.is_empty()) {
            next_line = Some(seen - 1);
            deliberately_stopped = true;
            return false;
        }
        if prefix {
            text.push(line.origin());
        }
        let remaining = MAX_PATCH_BYTES.saturating_sub(text.len());
        let slice = &raw[..raw.len().min(remaining)];
        let lossy = String::from_utf8_lossy(slice);
        if lossy.len() <= remaining {
            text.push_str(&lossy);
        } else {
            text.push_str(&lossy[..lossy.floor_char_boundary(remaining)]);
            clipped_long_line = true;
        }
        if raw.len() > remaining {
            // Exception for a single oversize line: preserve a bounded
            // excerpt and explicitly disclose that the rest was skipped.
            clipped_long_line = true;
        }
        true
    });
    if safety_limit_reached {
        return Err("Git diff paging safety limit reached".into());
    }
    if let Err(error) = print {
        if !deliberately_stopped {
            return Err(error.to_string());
        }
    }
    if seen < start_line || (text.is_empty() && start_line != 0) {
        return Err("Git patch changed; reopen its first page".into());
    }
    if clipped_long_line {
        text.push_str("\n… Git diff line exceeds 32 KiB and was clipped");
    }
    Ok(GitPatchPage {
        text,
        start_line,
        next_line,
        clipped_long_line,
    })
}

fn append_diff(
    diff: &git2::Diff<'_>,
    out: &mut String,
    truncated: &mut bool,
) -> Result<(), String> {
    diff.print(git2::DiffFormat::Patch, |_, _, line| {
        if *truncated {
            return true;
        }
        if matches!(line.origin(), '+' | '-' | ' ') {
            if out.len() == MAX_PATCH_BYTES {
                *truncated = true;
                return true;
            }
            out.push(line.origin());
        }
        let available = MAX_PATCH_BYTES.saturating_sub(out.len());
        let bytes = line.content();
        let raw = String::from_utf8_lossy(&bytes[..bytes.len().min(available)]);
        if raw.len() <= available {
            out.push_str(&raw);
        } else {
            // A sliced multibyte UTF-8 codepoint can expand to U+FFFD.
            // Preserve a valid UTF-8 boundary without exceeding the limit.
            out.push_str(&raw[..raw.floor_char_boundary(available)]);
            *truncated = true;
        }
        if bytes.len() > available {
            *truncated = true;
        }
        true
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_git_status_and_unified_diff_support_unicode_paths() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("existing.rs"), "before\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("existing.rs")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_oid).unwrap();
        let signature = git2::Signature::now("Alera Test", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
            .unwrap();
        drop(tree);
        fs::write(temp.path().join("existing.rs"), "after\n").unwrap();
        fs::write(temp.path().join("中文檔案.txt"), "測試文字").unwrap();
        let snapshot = GitSnapshot::load(temp.path()).unwrap();
        assert_eq!(snapshot.changed_files.len(), 2);
        assert!(snapshot
            .changed_files
            .iter()
            .any(|x| x.relative == Path::new("中文檔案.txt")));
        let patch = read_file_diff(temp.path(), Path::new("existing.rs")).unwrap();
        assert!(
            patch.contains("+after"),
            "expected Git unified addition, got: {patch}"
        );
        assert!(read_file_diff(temp.path(), Path::new("中文檔案.txt"))
            .unwrap()
            .contains("測試文字"));
        assert!(read_file_diff(temp.path(), Path::new("../escape")).is_err());
    }

    #[test]
    fn staged_patch_preview_does_not_change_head_index_or_worktree_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("example.rs");
        let file = temp.path().join(relative);
        fs::write(&file, "fn main() { println!(\"before\"); }\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let base_tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        let base_tree = repo.find_tree(base_tree_id).unwrap();
        let signature = git2::Signature::now("Alera QA", "test@local.invalid").unwrap();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "baseline",
            &base_tree,
            &[],
        )
        .unwrap();
        drop(base_tree);
        fs::write(&file, "fn main() { println!(\"staged\"); }\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        index.write().unwrap();
        let staged_id = index.get_path(relative, 0).unwrap().id;
        drop(index);

        let head_before = repo.head().unwrap().target().unwrap();
        let bytes_before = fs::read(&file).unwrap();
        let patch = read_file_diff(temp.path(), relative).unwrap();
        assert!(
            patch.contains("+fn main() { println!(\"staged\"); }"),
            "{patch}"
        );
        assert_eq!(repo.head().unwrap().target(), Some(head_before));
        assert_eq!(
            repo.index().unwrap().get_path(relative, 0).unwrap().id,
            staged_id
        );
        assert_eq!(fs::read(file).unwrap(), bytes_before);
    }

    #[test]
    fn untracked_large_patch_is_bounded_and_cannot_escape_workspace() {
        let temp = tempfile::tempdir().unwrap();
        git2::Repository::init(temp.path()).unwrap();
        let path = temp.path().join("large-untracked.txt");
        let text = "x".repeat(MAX_PATCH_BYTES + 2048);
        fs::write(&path, &text).unwrap();
        let message = read_file_diff(temp.path(), Path::new("large-untracked.txt")).unwrap();
        assert!(message.contains("too large"), "{message}");
        assert!(message.len() < MAX_PATCH_BYTES);
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        assert!(read_file_diff(temp.path(), Path::new("../large-untracked.txt")).is_err());
        assert!(read_file_diff(temp.path(), &path).is_err());
    }

    #[test]
    fn untracked_file_at_exact_size_limit_does_not_overflow_paged_preview() {
        let temp = tempfile::tempdir().unwrap();
        git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("exactly-limit.txt");
        let path = temp.path().join(relative);
        let contents = "x".repeat(MAX_PATCH_BYTES);
        fs::write(&path, &contents).unwrap();
        let page = read_file_diff_page(temp.path(), relative, 0).unwrap();
        assert!(page.text.len() <= MAX_PATCH_BYTES + 64);
        assert!(
            page.text.contains("preview truncated"),
            "Untracked preview header must not silently exceed page limit"
        );
        assert!(page.next_line.is_none());
        assert_eq!(fs::read_to_string(path).unwrap(), contents);
    }

    #[test]
    fn oversized_tracked_ascii_patch_explicitly_reports_truncation_without_writes() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("large-ascii.txt");
        let path = temp.path().join(relative);
        fs::write(&path, "baseline\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_oid = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_oid).unwrap();
        let signature = git2::Signature::now("Alera QA", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "baseline", &tree, &[])
            .unwrap();
        drop(tree);
        let content = "plain ASCII content in a tracked change\n".repeat(3_000);
        fs::write(&path, &content).unwrap();
        let head_before = repo.head().unwrap().target();
        let index_before = repo.index().unwrap().get_path(relative, 0).unwrap().id;

        let preview = read_file_diff(temp.path(), relative).unwrap();
        assert!(preview.starts_with("diff --git "), "{preview:?}");
        assert!(
            preview.ends_with("\n… diff preview truncated"),
            "A partial tracked patch must carry a visible truncation warning"
        );
        assert!(preview.len() <= MAX_PATCH_BYTES + 64);
        assert!(preview.is_char_boundary(preview.len()));
        assert_eq!(fs::read_to_string(path).unwrap(), content);
        assert_eq!(repo.head().unwrap().target(), head_before);
        assert_eq!(
            repo.index().unwrap().get_path(relative, 0).unwrap().id,
            index_before
        );
    }

    #[test]
    fn truncated_tracked_unicode_patch_stays_valid_utf8_and_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("unicode.txt");
        let file = temp.path().join(relative);
        fs::write(&file, "original\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Alera QA", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        let content = "中文修改🙂\n".repeat(6_000);
        fs::write(&file, &content).unwrap();
        let patch = read_file_diff(temp.path(), relative).unwrap();
        assert!(patch.ends_with("\n… diff preview truncated"));
        assert!(patch.len() <= MAX_PATCH_BYTES + 64);
        assert!(patch.is_char_boundary(patch.len()));
        assert_eq!(fs::read_to_string(file).unwrap(), content);
    }

    #[test]
    fn large_tracked_patch_can_be_read_across_bounded_pages_without_git_writes() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("pages.txt");
        let file = temp.path().join(relative);
        fs::write(&file, "original\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Alera Test", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        let modified = (0..2_500)
            .map(|index| format!("line-{index:04} 中文🙂 checked and safe\n"))
            .collect::<String>();
        fs::write(&file, &modified).unwrap();
        let head_before = repo.head().unwrap().target();
        let index_before = repo.index().unwrap().get_path(relative, 0).unwrap().id;

        let mut cursor = 0;
        let mut pages = 0;
        let mut seen_text = String::new();
        loop {
            let page = read_file_diff_page(temp.path(), relative, cursor).unwrap();
            assert_eq!(page.start_line, cursor);
            assert!(!page.clipped_long_line);
            assert!(!page.text.is_empty());
            assert!(page.text.len() <= MAX_PATCH_BYTES);
            seen_text.push_str(&page.text);
            pages += 1;
            assert!(pages < 16, "Pagination must always advance");
            match page.next_line {
                Some(next) => {
                    assert!(next > cursor, "The next cursor must make progress");
                    cursor = next;
                }
                None => break,
            }
        }
        assert!(pages >= 3, "Expected more than two bounded pages");
        assert!(seen_text.contains("line-0000 中文🙂"));
        assert!(seen_text.contains("line-2499 中文🙂"));
        let first_again = read_file_diff_page(temp.path(), relative, 0).unwrap();
        assert_eq!(first_again.start_line, 0);
        assert_eq!(first_again.text, seen_text[..first_again.text.len()]);
        assert_eq!(fs::read_to_string(file).unwrap(), modified);
        assert_eq!(repo.head().unwrap().target(), head_before);
        assert_eq!(
            repo.index().unwrap().get_path(relative, 0).unwrap().id,
            index_before
        );
        assert!(read_file_diff_page(temp.path(), Path::new("../escape"), 0).is_err());
        assert!(read_file_diff_page(temp.path(), relative, MAX_PAGED_DIFF_LINES).is_err());
        assert!(read_file_diff_page(temp.path(), relative, cursor + 99_000).is_err());
    }

    #[test]
    fn staged_only_patch_pages_without_changing_head_index_or_disk() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("staged.rs");
        let path = temp.path().join(relative);
        fs::write(&path, "before\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Alera Test", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        let staged_text = (0..2_000)
            .map(|i| format!("let staged_line_{i:04} = true;\n"))
            .collect::<String>();
        fs::write(&path, &staged_text).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        index.write().unwrap();
        drop(index);
        let head_before = repo.head().unwrap().target();
        let staged_oid = repo.index().unwrap().get_path(relative, 0).unwrap().id;
        let mut cursor = 0;
        let mut seen = String::new();
        let mut pages = 0;
        loop {
            let page = read_file_diff_page(temp.path(), relative, cursor).unwrap();
            seen.push_str(&page.text);
            pages += 1;
            assert!(pages < 12);
            if let Some(next) = page.next_line {
                assert!(next > cursor);
                cursor = next;
            } else {
                break;
            }
        }
        assert!(pages > 1);
        assert!(seen.contains("+let staged_line_0000 = true;"));
        assert!(seen.contains("+let staged_line_1999 = true;"));
        assert_eq!(repo.head().unwrap().target(), head_before);
        assert_eq!(
            repo.index().unwrap().get_path(relative, 0).unwrap().id,
            staged_oid
        );
        assert_eq!(fs::read_to_string(path).unwrap(), staged_text);
    }

    #[test]
    fn paged_preview_clips_oversized_single_line_with_explicit_warning() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("long.txt");
        let file = temp.path().join(relative);
        fs::write(&file, "old\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Alera Test", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        let changed = format!("{}\nEND\n", "很長🙂".repeat(MAX_PATCH_BYTES));
        fs::write(&file, &changed).unwrap();
        let mut cursor = 0;
        let mut clipped = false;
        let mut found_end = false;
        for _ in 0..8 {
            let page = read_file_diff_page(temp.path(), relative, cursor).unwrap();
            clipped |= page.clipped_long_line && page.text.contains("was clipped");
            found_end |= page.text.contains("+END");
            if let Some(next) = page.next_line {
                assert!(next > cursor);
                cursor = next;
            } else {
                break;
            }
        }
        assert!(
            clipped,
            "A single oversized line must not be shown as complete"
        );
        assert!(found_end, "Later complete lines must remain accessible");
        assert_eq!(fs::read_to_string(file).unwrap(), changed);
    }
}
