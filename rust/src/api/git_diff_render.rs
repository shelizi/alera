use std::path::Path;

use git2::{Diff, DiffDelta, DiffFormat, DiffLineType, Oid, Repository};

use crate::api::workspace_files::{decode_workspace_text_bytes, WorkspaceTextEncoding};

use super::{GitChangeArea, GitDiffLine, GitDiffLineKind, GitError};

pub(super) const MAX_DIFF_PATCH_BYTES: usize = 512 * 1024;

pub(super) const MAX_DIFF_PREVIEW_LINES: usize = 5000;

const MAX_DIFF_ENCODING_SOURCE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Default)]
struct SideEncodingCache {
    attempted: bool,
    encoding: Option<WorkspaceTextEncoding>,
}

impl SideEncodingCache {
    fn resolve(
        &mut self,
        repo: &Repository,
        delta: &DiffDelta<'_>,
        area: Option<GitChangeArea>,
        old_side: bool,
    ) -> Option<WorkspaceTextEncoding> {
        if !self.attempted {
            self.attempted = true;
            self.encoding = detect_delta_side_encoding(repo, delta, area, old_side);
        }
        self.encoding
    }
}

pub(super) struct RenderedDiff {
    pub(super) lines: Vec<GitDiffLine>,
    pub(super) added: u32,
    pub(super) removed: u32,
    pub(super) is_binary: bool,
    pub(super) truncated: bool,
    pub(super) line_preview_truncated: bool,
}

pub(super) fn render_diff_for_path(
    repo: &Repository,
    diff: &mut Diff<'_>,
    repo_path: &str,
    area: Option<GitChangeArea>,
) -> Result<RenderedDiff, GitError> {
    let mut lines = Vec::new();
    let mut preview_bytes = 0usize;
    let mut added = 0u32;
    let mut removed = 0u32;
    let mut is_binary = false;
    let mut truncated = false;
    let mut line_preview_truncated = false;
    let mut old_encoding = SideEncodingCache::default();
    let mut new_encoding = SideEncodingCache::default();

    let print_result = diff.print(DiffFormat::Patch, |delta, _hunk, line| {
        if !delta_matches_repo_path(&delta, repo_path) {
            return true;
        }
        match line.origin_value() {
            DiffLineType::Addition => added = added.saturating_add(line.num_lines()),
            DiffLineType::Deletion => removed = removed.saturating_add(line.num_lines()),
            DiffLineType::Binary => is_binary = true,
            _ => {}
        }
        if line_preview_truncated || truncated {
            return true;
        }
        if preview_bytes >= MAX_DIFF_PATCH_BYTES {
            truncated = true;
            return true;
        }
        let (mut text, kind) = match line.origin_value() {
            DiffLineType::Context
            | DiffLineType::Addition
            | DiffLineType::Deletion
            | DiffLineType::ContextEOFNL
            | DiffLineType::AddEOFNL
            | DiffLineType::DeleteEOFNL => {
                let content = decode_source_line(
                    repo,
                    &delta,
                    area,
                    line.origin_value(),
                    line.content(),
                    &mut old_encoding,
                    &mut new_encoding,
                );
                let mut text = String::new();
                text.push(line.origin());
                text.push_str(content.trim_end_matches('\n'));
                (text, kind_for_line(line.origin_value()))
            }
            DiffLineType::FileHeader | DiffLineType::HunkHeader | DiffLineType::Binary => {
                let content = String::from_utf8_lossy(line.content());
                (
                    content.trim_end_matches('\n').to_string(),
                    kind_for_line(line.origin_value()),
                )
            }
        };
        let line_bytes = text.len().saturating_add(1);
        if preview_bytes.saturating_add(line_bytes) > MAX_DIFF_PATCH_BYTES {
            let remaining = MAX_DIFF_PATCH_BYTES.saturating_sub(preview_bytes);
            truncate_to_char_boundary(&mut text, remaining);
            truncated = true;
        }
        if !text.is_empty() {
            if lines.len() >= MAX_DIFF_PREVIEW_LINES {
                line_preview_truncated = true;
                return true;
            }
            preview_bytes = preview_bytes.saturating_add(text.len().saturating_add(1));
            lines.push(GitDiffLine { text, kind });
        }
        if truncated {
            return true;
        }
        true
    });
    match print_result {
        Ok(()) => {}
        Err(_) if truncated || line_preview_truncated => {}
        Err(error) => return Err(GitError::from_git2(error)),
    }

    Ok(RenderedDiff {
        lines,
        added,
        removed,
        is_binary,
        truncated,
        line_preview_truncated,
    })
}

fn decode_source_line(
    repo: &Repository,
    delta: &DiffDelta<'_>,
    area: Option<GitChangeArea>,
    line_type: DiffLineType,
    bytes: &[u8],
    old_encoding: &mut SideEncodingCache,
    new_encoding: &mut SideEncodingCache,
) -> String {
    if bytes.is_ascii() {
        return String::from_utf8_lossy(bytes).into_owned();
    }

    let primary_old_side = matches!(
        line_type,
        DiffLineType::Deletion | DiffLineType::DeleteEOFNL
    );
    if let Some(decoded) = decode_source_line_from_side(
        repo,
        delta,
        area,
        primary_old_side,
        bytes,
        old_encoding,
        new_encoding,
    ) {
        return decoded;
    }
    if matches!(
        line_type,
        DiffLineType::Context | DiffLineType::ContextEOFNL
    ) {
        if let Some(decoded) = decode_source_line_from_side(
            repo,
            delta,
            area,
            !primary_old_side,
            bytes,
            old_encoding,
            new_encoding,
        ) {
            return decoded;
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn decode_source_line_from_side(
    repo: &Repository,
    delta: &DiffDelta<'_>,
    area: Option<GitChangeArea>,
    old_side: bool,
    bytes: &[u8],
    old_encoding: &mut SideEncodingCache,
    new_encoding: &mut SideEncodingCache,
) -> Option<String> {
    let encoding = if old_side {
        old_encoding.resolve(repo, delta, area, true)
    } else {
        new_encoding.resolve(repo, delta, area, false)
    }?;
    decode_workspace_text_bytes(bytes.to_vec(), Some(encoding))
        .ok()
        .map(|decoded| decoded.content)
}

fn detect_delta_side_encoding(
    repo: &Repository,
    delta: &DiffDelta<'_>,
    area: Option<GitChangeArea>,
    old_side: bool,
) -> Option<WorkspaceTextEncoding> {
    let bytes = delta_side_bytes(repo, delta, area, old_side)?;
    decode_workspace_text_bytes(bytes, None)
        .ok()
        .map(|decoded| decoded.encoding)
}

fn delta_side_bytes(
    repo: &Repository,
    delta: &DiffDelta<'_>,
    area: Option<GitChangeArea>,
    old_side: bool,
) -> Option<Vec<u8>> {
    let file = if old_side {
        delta.old_file()
    } else {
        delta.new_file()
    };
    if let Some(bytes) = blob_bytes(repo, file.id()) {
        return Some(bytes);
    }
    let path = file.path()?;
    match (area, old_side) {
        (Some(GitChangeArea::Staged), true) => head_blob_bytes(repo, path),
        (Some(GitChangeArea::Staged), false) => index_blob_bytes(repo, path),
        (Some(GitChangeArea::Unstaged), true) => index_blob_bytes(repo, path),
        (Some(GitChangeArea::Unstaged), false) => workdir_file_bytes(repo, path),
        _ => None,
    }
}

fn blob_bytes(repo: &Repository, oid: Oid) -> Option<Vec<u8>> {
    let blob = repo.find_blob(oid).ok()?;
    if blob.size() as u64 > MAX_DIFF_ENCODING_SOURCE_BYTES {
        return None;
    }
    Some(blob.content().to_vec())
}

fn head_blob_bytes(repo: &Repository, path: &Path) -> Option<Vec<u8>> {
    let tree = repo.head().ok()?.peel_to_tree().ok()?;
    let entry = tree.get_path(path).ok()?;
    blob_bytes(repo, entry.id())
}

fn index_blob_bytes(repo: &Repository, path: &Path) -> Option<Vec<u8>> {
    let index = repo.index().ok()?;
    let entry = index.get_path(path, 0)?;
    blob_bytes(repo, entry.id)
}

fn workdir_file_bytes(repo: &Repository, path: &Path) -> Option<Vec<u8>> {
    let full_path = repo.workdir()?.join(path);
    let metadata = std::fs::metadata(&full_path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_DIFF_ENCODING_SOURCE_BYTES {
        return None;
    }
    std::fs::read(full_path).ok()
}

fn delta_matches_repo_path(delta: &git2::DiffDelta<'_>, repo_path: &str) -> bool {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .is_some_and(|path| path.to_string_lossy() == repo_path)
}

pub(super) fn truncate_to_char_boundary(value: &mut String, max_bytes: usize) -> bool {
    if value.len() <= max_bytes {
        return false;
    }
    let mut boundary = max_bytes.min(value.len());
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    true
}

pub(super) fn diff_lines_byte_len(lines: &[GitDiffLine]) -> usize {
    lines
        .iter()
        .map(|line| line.text.len().saturating_add(1))
        .sum()
}

pub(super) fn truncate_diff_lines_to_bytes(lines: &mut Vec<GitDiffLine>, max_bytes: usize) -> bool {
    let mut used = 0usize;
    let mut keep = 0usize;
    for line in lines.iter_mut() {
        let line_bytes = line.text.len().saturating_add(1);
        if used.saturating_add(line_bytes) <= max_bytes {
            used = used.saturating_add(line_bytes);
            keep += 1;
            continue;
        }
        let remaining = max_bytes.saturating_sub(used);
        if remaining > 0 {
            truncate_to_char_boundary(&mut line.text, remaining);
            if !line.text.is_empty() {
                keep += 1;
            }
        }
        lines.truncate(keep);
        return true;
    }
    false
}

pub(super) fn diff_lines_from_patch(patch: &str) -> (Vec<GitDiffLine>, bool) {
    let mut lines = Vec::new();
    let mut truncated = false;
    for text in patch.lines() {
        if lines.len() >= MAX_DIFF_PREVIEW_LINES {
            truncated = true;
            break;
        }
        lines.push(GitDiffLine {
            text: text.to_string(),
            kind: kind_for_text(text),
        });
    }
    (lines, truncated)
}

fn kind_for_line(line_type: DiffLineType) -> GitDiffLineKind {
    match line_type {
        DiffLineType::Addition | DiffLineType::AddEOFNL => GitDiffLineKind::Addition,
        DiffLineType::Deletion | DiffLineType::DeleteEOFNL => GitDiffLineKind::Deletion,
        DiffLineType::HunkHeader => GitDiffLineKind::Hunk,
        DiffLineType::FileHeader | DiffLineType::Binary => GitDiffLineKind::Header,
        _ => GitDiffLineKind::Context,
    }
}

fn kind_for_text(text: &str) -> GitDiffLineKind {
    if text.starts_with("@@") {
        GitDiffLineKind::Hunk
    } else if text.starts_with("diff --git")
        || text.starts_with("index ")
        || text.starts_with("--- ")
        || text.starts_with("+++ ")
        || text.starts_with("new file mode ")
        || text.starts_with("deleted file mode ")
        || text.starts_with("rename from ")
        || text.starts_with("rename to ")
        || text.starts_with("similarity index ")
    {
        GitDiffLineKind::Header
    } else if text.starts_with('+') {
        GitDiffLineKind::Addition
    } else if text.starts_with('-') {
        GitDiffLineKind::Deletion
    } else {
        GitDiffLineKind::Context
    }
}
