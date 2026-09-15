use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use git2::{
    Delta, Diff, DiffFindOptions, DiffFormat, DiffLineType, DiffOptions, ErrorCode, Oid,
    Repository, Status, StatusEntry, StatusOptions,
};

use super::{
    open_repo, GitChangeArea, GitChangeEntry, GitChangeGroup, GitChangeStatus, GitChangeTreeRow,
    GitChangeTreeRowKind, GitCommitChangeEntry, GitCommitCompareResult, GitCommitCompareStatus,
    GitCommitCompareSummary, GitDiffFile, GitDiffLine, GitDiffLineKind, GitDiffPage, GitDiffResult,
    GitDiffSideBySideRow, GitDiffSideBySideRowKind, GitDiffWhitespaceMode, GitError, GitErrorKind,
    GitStatusResult, GitSubmoduleStatus,
};

#[path = "git_diff_combined.rs"]
mod git_diff_combined;
#[path = "git_diff_render.rs"]
mod git_diff_render;
#[path = "git_diff_untracked.rs"]
mod git_diff_untracked;
#[path = "git_reading_diff_patch.rs"]
pub(in crate::api) mod git_reading_diff_patch;

use git_diff_combined::{
    append_combined_diff_file, append_combined_diff_for_path_with_whitespace,
    git_diff_all_for_file_with_whitespace,
};
use git_diff_render::render_diff_for_path;
use git_diff_untracked::{build_untracked_patch, read_untracked_text_up_to, untracked_diff_file};

#[path = "git_submodule_impl.rs"]
mod git_submodule_impl;

use super::git_diff_paths::GitPathContext;

type CommitDiffLineStatsByPath = HashMap<String, (Option<u32>, Option<u32>)>;

pub(super) fn git_status(path: String) -> Result<GitStatusResult, GitError> {
    let repo = open_repo(&path)?;
    let paths = GitPathContext::new(&repo, &path)?;
    let mut options = status_options();
    git_status_with_options(&repo, &paths, &mut options)
}

pub(super) fn git_status_for_path(
    path: String,
    file_path: String,
) -> Result<GitStatusResult, GitError> {
    let repo = open_repo(&path)?;
    let paths = GitPathContext::new(&repo, &path)?;
    let mut options = status_options();
    options
        .disable_pathspec_match(true)
        .pathspec(paths.to_repo_path(&file_path));
    git_status_with_options(&repo, &paths, &mut options)
}

pub(super) fn git_submodule_status(
    path: String,
    submodule_path: String,
    area: GitChangeArea,
) -> Result<GitStatusResult, GitError> {
    git_submodule_impl::git_submodule_status(path, submodule_path, area)
}

pub(super) fn git_submodule_worktree_status(
    path: String,
    submodule_path: String,
) -> Result<GitStatusResult, GitError> {
    git_submodule_impl::git_submodule_worktree_status(path, submodule_path)
}

pub(super) fn discard_submodule_gitlink(
    repo: &Repository,
    workspace_path: &str,
    submodule_path: &str,
    skip_dirty: bool,
) -> Result<bool, GitError> {
    let paths = GitPathContext::new(repo, workspace_path)?;
    git_submodule_impl::discard_gitlink_change(repo, &paths, submodule_path, skip_dirty)
}

fn git_status_with_options(
    repo: &Repository,
    paths: &GitPathContext,
    options: &mut StatusOptions,
) -> Result<GitStatusResult, GitError> {
    let statuses = repo.statuses(Some(options)).map_err(GitError::from_git2)?;
    let mut entries = Vec::new();

    for entry in statuses.iter() {
        if let Some(change) = status_entry_to_change(repo, paths, &entry, GitChangeArea::Staged)? {
            entries.push(change);
        }
        if let Some(change) = status_entry_to_change(repo, paths, &entry, GitChangeArea::Untracked)?
        {
            entries.push(change);
            continue;
        }
        if let Some(change) = status_entry_to_change(repo, paths, &entry, GitChangeArea::Unstaged)?
        {
            entries.push(change);
        }
    }

    entries.sort_by(|a, b| {
        area_sort_key(a.area)
            .cmp(&area_sort_key(b.area))
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(status_result_from_entries(entries))
}

pub(super) fn git_diff(
    path: String,
    file_path: String,
    area: GitChangeArea,
) -> Result<GitDiffResult, GitError> {
    git_diff_with_whitespace(path, file_path, area, GitDiffWhitespaceMode::Normal)
}

pub(super) fn git_diff_with_whitespace(
    path: String,
    file_path: String,
    area: GitChangeArea,
    whitespace_mode: GitDiffWhitespaceMode,
) -> Result<GitDiffResult, GitError> {
    let repo = open_repo(&path)?;
    let paths = GitPathContext::new(&repo, &path)?;
    let files =
        diff_file_for_area_with_whitespace(&repo, &paths, &file_path, area, whitespace_mode)?
            .into_iter()
            .collect::<Vec<_>>();
    let truncated = files.iter().any(|file| file.truncated);
    Ok(GitDiffResult { files, truncated })
}

pub(super) fn git_diff_all(
    path: String,
    file_path: Option<String>,
) -> Result<GitDiffResult, GitError> {
    git_diff_all_with_whitespace(path, file_path, GitDiffWhitespaceMode::Normal)
}

pub(super) fn git_diff_all_with_whitespace(
    path: String,
    file_path: Option<String>,
    whitespace_mode: GitDiffWhitespaceMode,
) -> Result<GitDiffResult, GitError> {
    let repo = open_repo(&path)?;
    let paths = GitPathContext::new(&repo, &path)?;
    if let Some(file_path) = file_path {
        return git_diff_all_for_file_with_whitespace(&repo, &paths, &file_path, whitespace_mode);
    }
    let status = git_status(path.clone())?;
    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    let mut truncated = false;

    'changes: for entry in status.entries {
        if let Some(submodule) = &entry.submodule {
            if submodule.commit_changed {
                if let Some(file) = diff_file_for_area_with_whitespace(
                    &repo,
                    &paths,
                    &entry.path,
                    entry.area,
                    whitespace_mode,
                )? {
                    if append_combined_diff_file(&mut files, &mut total_bytes, &mut truncated, file)
                    {
                        break;
                    }
                }
            }
            if entry.area == GitChangeArea::Unstaged
                && submodule.inspectable
                && (submodule.tracked_changes || submodule.untracked_changes)
            {
                let inner = git_submodule_worktree_status(path.clone(), entry.path.clone())?;
                for inner_entry in inner.entries {
                    let inner_path = format!("{}/{}", entry.path, inner_entry.path);
                    if let Some(file) = diff_file_for_area_with_whitespace(
                        &repo,
                        &paths,
                        &inner_path,
                        inner_entry.area,
                        whitespace_mode,
                    )? {
                        if append_combined_diff_file(
                            &mut files,
                            &mut total_bytes,
                            &mut truncated,
                            file,
                        ) {
                            break 'changes;
                        }
                    }
                }
            }
            continue;
        }
        if let Some(file) = diff_file_for_area_with_whitespace(
            &repo,
            &paths,
            &entry.path,
            entry.area,
            whitespace_mode,
        )? {
            if append_combined_diff_file(&mut files, &mut total_bytes, &mut truncated, file) {
                break;
            }
        }
    }

    Ok(GitDiffResult { files, truncated })
}

pub(super) fn git_diff_all_page(
    path: String,
    file_paths: Vec<String>,
) -> Result<GitDiffPage, GitError> {
    git_diff_all_page_with_whitespace(path, file_paths, GitDiffWhitespaceMode::Normal)
}

pub(super) fn git_diff_all_page_with_whitespace(
    path: String,
    file_paths: Vec<String>,
    whitespace_mode: GitDiffWhitespaceMode,
) -> Result<GitDiffPage, GitError> {
    let repo = open_repo(&path)?;
    let paths = GitPathContext::new(&repo, &path)?;
    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    let mut truncated = false;

    for file_path in file_paths {
        if append_combined_diff_for_path_with_whitespace(
            &repo,
            &paths,
            &file_path,
            whitespace_mode,
            &mut files,
            &mut total_bytes,
            &mut truncated,
        )? {
            break;
        }
    }

    Ok(GitDiffPage { files, truncated })
}

pub(super) fn git_commit_compare(
    path: String,
    commit_id: String,
) -> Result<GitCommitCompareResult, GitError> {
    let repo = open_repo(&path)?;
    let commit = match repo.revparse_single(&format!("{commit_id}^{{commit}}")) {
        Ok(object) => match object.peel_to_commit() {
            Ok(commit) => commit,
            Err(_) => return Ok(invalid_commit_compare(commit_id)),
        },
        Err(_) => return Ok(invalid_commit_compare(commit_id)),
    };
    let commit_oid = commit.id();
    let parent_oid = commit.parent_id(0).ok();
    let mut summary = GitCommitCompareSummary {
        commit_oid: commit_oid.to_string(),
        parent_oid: parent_oid.map(|oid| oid.to_string()),
        compare_ref: short_oid(commit_oid),
        base_ref: parent_oid
            .map(short_oid)
            .unwrap_or_else(|| "empty tree".to_string()),
        changed_files: 0,
        status: GitCommitCompareStatus::Ready,
        error_message: None,
    };
    match commit_change_entries(&repo, &path, parent_oid, commit_oid) {
        Ok(entries) => {
            summary.changed_files = entries.len() as u32;
            Ok(GitCommitCompareResult { summary, entries })
        }
        Err(error) => Ok(GitCommitCompareResult {
            summary: GitCommitCompareSummary {
                status: GitCommitCompareStatus::Error,
                error_message: Some(error.context),
                ..summary
            },
            entries: Vec::new(),
        }),
    }
}

pub(super) fn git_compare_range(
    path: String,
    base_ref: String,
    head_ref: String,
) -> Result<GitCommitCompareResult, GitError> {
    let repo = open_repo(&path)?;
    let base_name = base_ref.trim();
    let head_name = head_ref.trim();
    if base_name.is_empty() || head_name.is_empty() {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            "base and head refs cannot be empty",
        ));
    }
    let base_oid = resolve_compare_ref_oid(&repo, base_name, "base")?;
    let head_oid = resolve_compare_ref_oid(&repo, head_name, "head")?;
    let merge_base_oid = repo
        .merge_base(base_oid, head_oid)
        .map_err(GitError::from_git2)?;
    let summary = GitCommitCompareSummary {
        commit_oid: head_oid.to_string(),
        parent_oid: Some(merge_base_oid.to_string()),
        compare_ref: head_name.to_string(),
        base_ref: base_name.to_string(),
        changed_files: 0,
        status: GitCommitCompareStatus::Ready,
        error_message: None,
    };
    match commit_change_entries(&repo, &path, Some(merge_base_oid), head_oid) {
        Ok(entries) => Ok(GitCommitCompareResult {
            summary: GitCommitCompareSummary {
                changed_files: entries.len() as u32,
                ..summary
            },
            entries,
        }),
        Err(error) => Ok(GitCommitCompareResult {
            summary: GitCommitCompareSummary {
                status: GitCommitCompareStatus::Error,
                error_message: Some(error.context),
                ..summary
            },
            entries: Vec::new(),
        }),
    }
}

fn resolve_compare_ref_oid(repo: &Repository, name: &str, role: &str) -> Result<Oid, GitError> {
    if name.starts_with('-') {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            format!("invalid {role} ref '{name}'"),
        ));
    }
    let object = repo
        .revparse_single(name)
        .map_err(|error| match error.code() {
            ErrorCode::NotFound | ErrorCode::Ambiguous | ErrorCode::InvalidSpec => GitError::new(
                GitErrorKind::BranchNotFound,
                format!("{role} ref '{name}' was not found"),
            ),
            _ => GitError::from_git2(error),
        })?;
    object
        .peel_to_commit()
        .map(|commit| commit.id())
        .map_err(|_| {
            GitError::new(
                GitErrorKind::BranchNotFound,
                format!("{role} ref '{name}' does not name a commit"),
            )
        })
}

pub(super) fn git_commit_diff(
    path: String,
    commit_oid: String,
    parent_oid: Option<String>,
    file_path: Option<String>,
    old_path: Option<String>,
) -> Result<GitDiffResult, GitError> {
    git_commit_diff_with_whitespace(
        path,
        commit_oid,
        parent_oid,
        file_path,
        old_path,
        GitDiffWhitespaceMode::Normal,
    )
}

pub(super) fn git_commit_diff_with_whitespace(
    path: String,
    commit_oid: String,
    parent_oid: Option<String>,
    file_path: Option<String>,
    old_path: Option<String>,
    whitespace_mode: GitDiffWhitespaceMode,
) -> Result<GitDiffResult, GitError> {
    let repo = open_repo(&path)?;
    let paths = GitPathContext::new(&repo, &path)?;
    let commit_oid = Oid::from_str(&commit_oid).map_err(GitError::from_git2)?;
    let parent_oid = parent_oid
        .as_deref()
        .map(Oid::from_str)
        .transpose()
        .map_err(GitError::from_git2)?;
    if let Some(file_path) = file_path {
        let repo_path = paths.to_repo_path(&file_path);
        let old_repo_path = old_path
            .as_deref()
            .map(|old_path| paths.to_repo_path(old_path));
        let mut diff = diff_for_commit_range_with_whitespace(
            &repo,
            parent_oid,
            commit_oid,
            &[repo_path.as_str(), old_repo_path.as_deref().unwrap_or("")],
            whitespace_mode,
        )?;
        let file = commit_diff_file_for_path(
            &repo,
            &paths,
            &mut diff,
            &repo_path,
            old_repo_path.as_deref(),
        )?;
        return Ok(GitDiffResult {
            files: file.into_iter().collect(),
            truncated: false,
        });
    }

    let mut diff =
        diff_for_commit_range_with_whitespace(&repo, parent_oid, commit_oid, &[], whitespace_mode)?;
    let mut files = Vec::new();
    let mut total_bytes = 0usize;
    let mut truncated = false;
    let selections = commit_diff_selections(&diff)?;
    for selection in selections {
        let Some(file) = commit_diff_file_for_path(
            &repo,
            &paths,
            &mut diff,
            &selection.path,
            selection.old_path.as_deref(),
        )?
        else {
            continue;
        };
        if append_combined_diff_file(&mut files, &mut total_bytes, &mut truncated, file) {
            break;
        }
    }
    Ok(GitDiffResult { files, truncated })
}

fn status_options() -> StatusOptions {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    options
}
fn status_entry_to_change(
    repo: &Repository,
    paths: &GitPathContext,
    entry: &StatusEntry<'_>,
    area: GitChangeArea,
) -> Result<Option<GitChangeEntry>, GitError> {
    let status = entry.status();
    match area {
        GitChangeArea::Staged => {
            if !has_staged_status(status) {
                return Ok(None);
            }
            let Some(delta) = entry.head_to_index() else {
                return Ok(None);
            };
            let repo_path = delta_path(&delta, false)?;
            let old_path = old_path_for_delta(&delta)?;
            let change_status = change_status_for_delta(delta.status(), area);
            let Some(path) =
                visible_workspace_path(paths, &repo_path, old_path.as_deref(), change_status)
            else {
                return Ok(None);
            };
            let mut change = GitChangeEntry {
                path: path.clone(),
                old_path: old_path
                    .as_deref()
                    .and_then(|old_path| paths.to_workspace_path(old_path)),
                area,
                status: change_status,
                added: None,
                removed: None,
                is_binary: delta.old_file().is_binary() || delta.new_file().is_binary(),
                is_large: false,
                submodule: git_submodule_impl::status_for_path(repo, &repo_path, area)?,
            };
            if change.submodule.is_none() {
                if let Some(stats) =
                    diff_line_stats_for_paths(repo, &repo_path, old_path.as_deref(), area)?
                {
                    change.added = Some(stats.0);
                    change.removed = Some(stats.1);
                }
            }
            Ok(Some(change))
        }
        GitChangeArea::Untracked => {
            if !status.contains(Status::WT_NEW) {
                return Ok(None);
            }
            let path = entry
                .index_to_workdir()
                .as_ref()
                .map(|delta| delta_path(delta, false))
                .transpose()?
                .or_else(|| entry.path().ok().map(ToString::to_string));
            let Some(repo_path) = path else {
                return Ok(None);
            };
            let Some(path) = paths.to_workspace_path(&repo_path) else {
                return Ok(None);
            };
            Ok(Some(GitChangeEntry {
                path,
                old_path: None,
                area,
                status: GitChangeStatus::Untracked,
                added: None,
                removed: Some(0),
                is_binary: false,
                is_large: false,
                submodule: None,
            }))
        }
        GitChangeArea::Unstaged => {
            if !has_unstaged_status(status) {
                return Ok(None);
            }
            if status.contains(Status::CONFLICTED) {
                let Some(repo_path) = status_entry_path(entry)? else {
                    return Ok(None);
                };
                let Some(path) = paths.to_workspace_path(&repo_path) else {
                    return Ok(None);
                };
                return Ok(Some(GitChangeEntry {
                    path,
                    old_path: None,
                    area,
                    status: GitChangeStatus::Modified,
                    added: None,
                    removed: None,
                    is_binary: false,
                    is_large: false,
                    submodule: None,
                }));
            }
            let Some(delta) = entry.index_to_workdir() else {
                return Ok(None);
            };
            let repo_path = delta_path(&delta, false)?;
            let old_path = old_path_for_delta(&delta)?;
            let change_status = change_status_for_delta(delta.status(), area);
            let Some(path) =
                visible_workspace_path(paths, &repo_path, old_path.as_deref(), change_status)
            else {
                return Ok(None);
            };
            let mut change = GitChangeEntry {
                path: path.clone(),
                old_path: old_path
                    .as_deref()
                    .and_then(|old_path| paths.to_workspace_path(old_path)),
                area,
                status: change_status,
                added: None,
                removed: None,
                is_binary: delta.old_file().is_binary() || delta.new_file().is_binary(),
                is_large: false,
                submodule: git_submodule_impl::status_for_path(repo, &repo_path, area)?,
            };
            if change.submodule.is_none() {
                if let Some(stats) =
                    diff_line_stats_for_paths(repo, &repo_path, old_path.as_deref(), area)?
                {
                    change.added = Some(stats.0);
                    change.removed = Some(stats.1);
                }
            }
            Ok(Some(change))
        }
    }
}

fn has_staged_status(status: Status) -> bool {
    status.intersects(
        Status::INDEX_NEW
            | Status::INDEX_MODIFIED
            | Status::INDEX_DELETED
            | Status::INDEX_RENAMED
            | Status::INDEX_TYPECHANGE,
    )
}

fn has_unstaged_status(status: Status) -> bool {
    status.intersects(
        Status::WT_MODIFIED
            | Status::WT_DELETED
            | Status::WT_RENAMED
            | Status::WT_TYPECHANGE
            | Status::CONFLICTED,
    )
}

fn area_sort_key(area: GitChangeArea) -> u8 {
    match area {
        GitChangeArea::Untracked => 0,
        GitChangeArea::Unstaged => 1,
        GitChangeArea::Staged => 2,
    }
}

pub(super) fn side_by_side_projection(lines: &[GitDiffLine]) -> Vec<GitDiffSideBySideRow> {
    let mut rows = Vec::with_capacity(lines.len());
    let mut current_old_line = None;
    let mut current_new_line = None;
    let mut pending_deletions = Vec::<(u32, Option<u32>)>::new();
    let mut pending_additions = Vec::<(u32, Option<u32>)>::new();

    for (line_index, line) in lines.iter().enumerate() {
        let line_index = line_index as u32;
        match line.kind {
            GitDiffLineKind::Hunk => {
                flush_side_by_side_changes(
                    &mut rows,
                    &mut pending_deletions,
                    &mut pending_additions,
                );
                if let Some((old_line, new_line)) = parse_hunk_line_starts(&line.text) {
                    current_old_line = old_line;
                    current_new_line = new_line;
                } else {
                    current_old_line = None;
                    current_new_line = None;
                }
                rows.push(passthrough_side_by_side_row(line_index));
            }
            GitDiffLineKind::Header => {
                flush_side_by_side_changes(
                    &mut rows,
                    &mut pending_deletions,
                    &mut pending_additions,
                );
                rows.push(passthrough_side_by_side_row(line_index));
            }
            GitDiffLineKind::Deletion => {
                if !pending_additions.is_empty() {
                    flush_side_by_side_changes(
                        &mut rows,
                        &mut pending_deletions,
                        &mut pending_additions,
                    );
                }
                let line_number = current_old_line;
                if let Some(value) = current_old_line {
                    current_old_line = Some(value.saturating_add(1));
                }
                pending_deletions.push((line_index, line_number));
            }
            GitDiffLineKind::Addition => {
                let line_number =
                    current_new_line.or_else(|| current_old_line.is_none().then_some(1));
                if let Some(value) = current_new_line {
                    current_new_line = Some(value.saturating_add(1));
                } else if current_old_line.is_none() {
                    current_new_line = Some(2);
                }
                pending_additions.push((line_index, line_number));
            }
            GitDiffLineKind::Context => {
                flush_side_by_side_changes(
                    &mut rows,
                    &mut pending_deletions,
                    &mut pending_additions,
                );
                let left_line_number = current_old_line;
                if let Some(value) = current_old_line {
                    current_old_line = Some(value.saturating_add(1));
                }
                let right_line_number = current_new_line;
                if let Some(value) = current_new_line {
                    current_new_line = Some(value.saturating_add(1));
                }
                rows.push(GitDiffSideBySideRow {
                    kind: GitDiffSideBySideRowKind::Pair,
                    line_index: None,
                    left_line_index: Some(line_index),
                    left_line_number,
                    right_line_index: Some(line_index),
                    right_line_number,
                });
            }
        }
    }

    flush_side_by_side_changes(&mut rows, &mut pending_deletions, &mut pending_additions);
    rows
}

fn passthrough_side_by_side_row(line_index: u32) -> GitDiffSideBySideRow {
    GitDiffSideBySideRow {
        kind: GitDiffSideBySideRowKind::Passthrough,
        line_index: Some(line_index),
        left_line_index: None,
        left_line_number: None,
        right_line_index: None,
        right_line_number: None,
    }
}

fn flush_side_by_side_changes(
    rows: &mut Vec<GitDiffSideBySideRow>,
    pending_deletions: &mut Vec<(u32, Option<u32>)>,
    pending_additions: &mut Vec<(u32, Option<u32>)>,
) {
    if pending_deletions.is_empty() && pending_additions.is_empty() {
        return;
    }
    let count = pending_deletions.len().max(pending_additions.len());
    for index in 0..count {
        let left = pending_deletions.get(index).copied();
        let right = pending_additions.get(index).copied();
        rows.push(GitDiffSideBySideRow {
            kind: GitDiffSideBySideRowKind::Pair,
            line_index: None,
            left_line_index: left.map(|(line_index, _)| line_index),
            left_line_number: left.and_then(|(_, line_number)| line_number),
            right_line_index: right.map(|(line_index, _)| line_index),
            right_line_number: right.and_then(|(_, line_number)| line_number),
        });
    }
    pending_deletions.clear();
    pending_additions.clear();
}

fn parse_hunk_line_starts(text: &str) -> Option<(Option<u32>, Option<u32>)> {
    let mut parts = text.strip_prefix("@@")?.trim_start().split_whitespace();
    let old_range = parts.next()?;
    let new_range = parts.next()?;
    Some((
        parse_hunk_side_start(old_range, '-')?,
        parse_hunk_side_start(new_range, '+')?,
    ))
}

fn parse_hunk_side_start(token: &str, prefix: char) -> Option<Option<u32>> {
    let range = token.strip_prefix(prefix)?;
    let mut parts = range.splitn(2, ',');
    let start = parts.next()?.parse::<u32>().ok()?;
    let count = match parts.next() {
        Some(value) => value.parse::<u32>().ok()?,
        None => 1,
    };
    Some((count != 0).then_some(start))
}

fn diff_file_for_area(
    repo: &Repository,
    paths: &GitPathContext,
    workspace_file_path: &str,
    area: GitChangeArea,
) -> Result<Option<GitDiffFile>, GitError> {
    diff_file_for_area_with_whitespace(
        repo,
        paths,
        workspace_file_path,
        area,
        GitDiffWhitespaceMode::Normal,
    )
}

fn diff_file_for_area_with_whitespace(
    repo: &Repository,
    paths: &GitPathContext,
    workspace_file_path: &str,
    area: GitChangeArea,
    whitespace_mode: GitDiffWhitespaceMode,
) -> Result<Option<GitDiffFile>, GitError> {
    match git_submodule_impl::diff_file_for_submodule(repo, paths, workspace_file_path, area)? {
        git_submodule_impl::SubmoduleDiff::NotSubmodule => {}
        git_submodule_impl::SubmoduleDiff::NoDiff => return Ok(None),
        git_submodule_impl::SubmoduleDiff::File(file) => return Ok(Some(file)),
    }
    let file_path = paths.to_repo_path(workspace_file_path);
    match area {
        GitChangeArea::Untracked => {
            if !is_untracked_file(repo, &file_path)? {
                return Ok(None);
            }
            match untracked_diff_file(repo, paths, &file_path) {
                Ok(file) => Ok(Some(file)),
                Err(_error) => {
                    let path = paths
                        .to_workspace_path(&file_path)
                        .unwrap_or_else(|| workspace_file_path.to_string());
                    Ok(Some(untracked_placeholder_diff_file(path)))
                }
            }
        }
        GitChangeArea::Staged | GitChangeArea::Unstaged => {
            let Some(selection) =
                selected_delta_for_area_with_whitespace(repo, &file_path, area, whitespace_mode)?
            else {
                return Ok(None);
            };
            let mut pathspecs = vec![selection.path.as_str()];
            if let Some(old_path) = selection.old_path.as_deref() {
                pathspecs.push(old_path);
            }
            let mut diff = diff_for_area_with_whitespace(repo, &pathspecs, area, whitespace_mode)?;
            let rendered = render_diff_for_path(&mut diff, &selection.path)?;
            if rendered.lines.is_empty() {
                return Ok(None);
            }
            let Some(path) = visible_workspace_path(
                paths,
                &selection.path,
                selection.old_path.as_deref(),
                selection.status,
            ) else {
                return Ok(None);
            };
            let side_by_side_rows = side_by_side_projection(&rendered.lines);
            Ok(Some(GitDiffFile {
                path,
                old_path: selection
                    .old_path
                    .as_deref()
                    .and_then(|old_path| paths.to_workspace_path(old_path)),
                area,
                status: selection.status,
                lines: rendered.lines,
                side_by_side_rows,
                added: Some(rendered.added),
                removed: Some(rendered.removed),
                is_binary: rendered.is_binary,
                is_large: false,
                is_gitlink: false,
                truncated: rendered.truncated,
                line_preview_truncated: rendered.line_preview_truncated,
            }))
        }
    }
}

fn untracked_placeholder_diff_file(path: String) -> GitDiffFile {
    GitDiffFile {
        path,
        old_path: None,
        area: GitChangeArea::Untracked,
        status: GitChangeStatus::Untracked,
        lines: Vec::new(),
        side_by_side_rows: Vec::new(),
        added: None,
        removed: Some(0),
        is_binary: false,
        is_large: false,
        is_gitlink: false,
        truncated: false,
        line_preview_truncated: false,
    }
}

fn invalid_commit_compare(commit_id: String) -> GitCommitCompareResult {
    GitCommitCompareResult {
        summary: GitCommitCompareSummary {
            commit_oid: String::new(),
            parent_oid: None,
            compare_ref: commit_id.clone(),
            base_ref: "parent".to_string(),
            changed_files: 0,
            status: GitCommitCompareStatus::InvalidCommit,
            error_message: Some(format!(
                "Commit {commit_id} could not be resolved in this repository."
            )),
        },
        entries: Vec::new(),
    }
}

fn commit_change_entries(
    repo: &Repository,
    workspace_path: &str,
    parent_oid: Option<Oid>,
    commit_oid: Oid,
) -> Result<Vec<GitCommitChangeEntry>, GitError> {
    let paths = GitPathContext::new(repo, workspace_path)?;
    let mut diff = diff_for_commit_range(repo, parent_oid, commit_oid, &[])?;
    let stats = commit_diff_stats_by_path(&mut diff)?;
    let selections = commit_diff_selections(&diff)?;
    let mut entries = Vec::new();
    for selection in selections {
        let Some(path) = visible_workspace_path(
            &paths,
            &selection.path,
            selection.old_path.as_deref(),
            selection.status,
        ) else {
            continue;
        };
        let old_path = selection
            .old_path
            .as_deref()
            .and_then(|old_path| paths.to_workspace_path(old_path));
        let (added, removed) = stats.get(&selection.path).copied().unwrap_or((None, None));
        entries.push(GitCommitChangeEntry {
            path,
            old_path,
            status: selection.status,
            added,
            removed,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn commit_diff_file_for_path(
    _repo: &Repository,
    paths: &GitPathContext,
    diff: &mut Diff<'_>,
    repo_path: &str,
    old_repo_path: Option<&str>,
) -> Result<Option<GitDiffFile>, GitError> {
    let selections = commit_diff_selections(diff)?;
    let selection = selections
        .iter()
        .find(|selection| {
            selected_delta_matches_path(
                delta_for_status(selection.status),
                &selection.path,
                selection.old_path.as_deref(),
                repo_path,
            )
        })
        .or_else(|| {
            old_repo_path.and_then(|old_repo_path| {
                selections.iter().find(|selection| {
                    selection.status == GitChangeStatus::Renamed
                        && selection.old_path.as_deref() == Some(old_repo_path)
                })
            })
        });
    let Some(selection) = selection else {
        return Ok(None);
    };
    let rendered = render_diff_for_path(diff, &selection.path)?;
    if rendered.lines.is_empty() {
        return Ok(None);
    }
    let Some(path) = visible_workspace_path(
        paths,
        &selection.path,
        selection.old_path.as_deref(),
        selection.status,
    ) else {
        return Ok(None);
    };
    let side_by_side_rows = side_by_side_projection(&rendered.lines);
    Ok(Some(GitDiffFile {
        path,
        old_path: selection
            .old_path
            .as_deref()
            .and_then(|old_path| paths.to_workspace_path(old_path)),
        area: GitChangeArea::Staged,
        status: selection.status,
        lines: rendered.lines,
        side_by_side_rows,
        added: Some(rendered.added),
        removed: Some(rendered.removed),
        is_binary: rendered.is_binary,
        is_large: false,
        is_gitlink: false,
        truncated: rendered.truncated,
        line_preview_truncated: rendered.line_preview_truncated,
    }))
}

fn diff_for_commit_range<'repo>(
    repo: &'repo Repository,
    parent_oid: Option<Oid>,
    commit_oid: Oid,
    pathspecs: &[&str],
) -> Result<Diff<'repo>, GitError> {
    diff_for_commit_range_with_whitespace(
        repo,
        parent_oid,
        commit_oid,
        pathspecs,
        GitDiffWhitespaceMode::Normal,
    )
}

fn diff_for_commit_range_with_whitespace<'repo>(
    repo: &'repo Repository,
    parent_oid: Option<Oid>,
    commit_oid: Oid,
    pathspecs: &[&str],
    whitespace_mode: GitDiffWhitespaceMode,
) -> Result<Diff<'repo>, GitError> {
    let commit = repo.find_commit(commit_oid).map_err(GitError::from_git2)?;
    let commit_tree = commit.tree().map_err(GitError::from_git2)?;
    let parent_tree = parent_oid
        .map(|oid| {
            repo.find_commit(oid)
                .and_then(|commit| commit.tree())
                .map_err(GitError::from_git2)
        })
        .transpose()?;
    let mut options = DiffOptions::new();
    apply_whitespace_mode(&mut options, whitespace_mode);
    let mut has_pathspec = false;
    for pathspec in pathspecs.iter().filter(|pathspec| !pathspec.is_empty()) {
        options.pathspec(pathspec);
        has_pathspec = true;
    }
    if has_pathspec {
        options.disable_pathspec_match(true);
    }
    let mut diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&commit_tree), Some(&mut options))
        .map_err(GitError::from_git2)?;
    let mut find_options = DiffFindOptions::new();
    find_options.renames(true).copies(true);
    diff.find_similar(Some(&mut find_options))
        .map_err(GitError::from_git2)?;
    Ok(diff)
}

fn commit_diff_selections(diff: &Diff<'_>) -> Result<Vec<DiffSelection>, GitError> {
    let mut selections = Vec::new();
    for delta in diff.deltas() {
        let path = delta_path(&delta, false)?;
        let old_path = old_path_for_delta(&delta)?;
        selections.push(DiffSelection {
            path,
            old_path,
            status: change_status_for_delta(delta.status(), GitChangeArea::Staged),
        });
    }
    Ok(selections)
}

fn commit_diff_stats_by_path(diff: &mut Diff<'_>) -> Result<CommitDiffLineStatsByPath, GitError> {
    let mut stats = HashMap::<String, (u32, u32)>::new();
    diff.print(DiffFormat::Patch, |delta, _hunk, line| {
        let Ok(path) = delta_path(&delta, false) else {
            return true;
        };
        let entry = stats.entry(path).or_insert((0, 0));
        match line.origin_value() {
            DiffLineType::Addition => entry.0 = entry.0.saturating_add(line.num_lines()),
            DiffLineType::Deletion => entry.1 = entry.1.saturating_add(line.num_lines()),
            _ => {}
        }
        true
    })
    .map_err(GitError::from_git2)?;
    Ok(stats
        .into_iter()
        .map(|(path, (added, removed))| {
            (
                path,
                (
                    if added > 0 { Some(added) } else { None },
                    if removed > 0 { Some(removed) } else { None },
                ),
            )
        })
        .collect())
}

fn delta_for_status(status: GitChangeStatus) -> Delta {
    match status {
        GitChangeStatus::Added | GitChangeStatus::Untracked => Delta::Added,
        GitChangeStatus::Deleted => Delta::Deleted,
        GitChangeStatus::Renamed => Delta::Renamed,
        GitChangeStatus::Copied => Delta::Copied,
        GitChangeStatus::Modified => Delta::Modified,
    }
}

fn short_oid(oid: Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

struct DiffSelection {
    path: String,
    old_path: Option<String>,
    status: GitChangeStatus,
}

fn selected_delta_for_area_with_whitespace(
    repo: &Repository,
    file_path: &str,
    area: GitChangeArea,
    whitespace_mode: GitDiffWhitespaceMode,
) -> Result<Option<DiffSelection>, GitError> {
    let diff = diff_for_area_with_whitespace(repo, &[], area, whitespace_mode)?;
    for delta in diff.deltas() {
        let new_path = delta_path(&delta, false)?;
        let old_path = old_path_for_delta(&delta)?;
        let delta_status = delta.status();
        if selected_delta_matches_path(delta_status, &new_path, old_path.as_deref(), file_path) {
            return Ok(Some(DiffSelection {
                path: new_path,
                old_path,
                status: change_status_for_delta(delta_status, area),
            }));
        }
    }
    Ok(None)
}

fn selected_delta_matches_path(
    delta_status: Delta,
    new_path: &str,
    old_path: Option<&str>,
    file_path: &str,
) -> bool {
    new_path == file_path || (delta_status == Delta::Renamed && old_path == Some(file_path))
}

fn visible_workspace_path(
    paths: &GitPathContext,
    repo_path: &str,
    old_path: Option<&str>,
    status: GitChangeStatus,
) -> Option<String> {
    paths.to_workspace_path(repo_path).or_else(|| {
        if matches!(status, GitChangeStatus::Renamed | GitChangeStatus::Deleted) {
            old_path.and_then(|old_path| paths.to_workspace_path(old_path))
        } else {
            None
        }
    })
}

fn diff_for_area<'repo>(
    repo: &'repo Repository,
    pathspecs: &[&str],
    area: GitChangeArea,
) -> Result<Diff<'repo>, GitError> {
    diff_for_area_with_whitespace(repo, pathspecs, area, GitDiffWhitespaceMode::Normal)
}

fn diff_for_area_with_whitespace<'repo>(
    repo: &'repo Repository,
    pathspecs: &[&str],
    area: GitChangeArea,
    whitespace_mode: GitDiffWhitespaceMode,
) -> Result<Diff<'repo>, GitError> {
    let mut options = DiffOptions::new();
    apply_whitespace_mode(&mut options, whitespace_mode);
    if !pathspecs.is_empty() {
        options.disable_pathspec_match(true);
    }
    for pathspec in pathspecs {
        options.pathspec(pathspec);
    }
    let mut diff = match area {
        GitChangeArea::Staged => {
            let index = repo.index().map_err(GitError::from_git2)?;
            let head_tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
            repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut options))
                .map_err(GitError::from_git2)
        }
        GitChangeArea::Unstaged => repo
            .diff_index_to_workdir(None, Some(&mut options))
            .map_err(GitError::from_git2),
        GitChangeArea::Untracked => unreachable!("untracked diffs are built from disk"),
    }?;
    let mut find_options = DiffFindOptions::new();
    find_options.renames(true).copies(true);
    diff.find_similar(Some(&mut find_options))
        .map_err(GitError::from_git2)?;
    Ok(diff)
}

fn apply_whitespace_mode(options: &mut DiffOptions, mode: GitDiffWhitespaceMode) {
    match mode {
        GitDiffWhitespaceMode::Normal => {}
        GitDiffWhitespaceMode::IgnoreEol => {
            options.ignore_whitespace_eol(true);
        }
        GitDiffWhitespaceMode::IgnoreChanges => {
            options.ignore_whitespace_change(true);
        }
        GitDiffWhitespaceMode::IgnoreAll => {
            options.ignore_whitespace(true);
        }
    }
}

fn is_untracked_file(repo: &Repository, file_path: &str) -> Result<bool, GitError> {
    match repo.status_file(Path::new(file_path)) {
        Ok(status) => {
            Ok(status.contains(Status::WT_NEW) && !staged_status_blocks_untracked(status))
        }
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(error) => Err(GitError::from_git2(error)),
    }
}

fn staged_status_blocks_untracked(status: Status) -> bool {
    status.intersects(Status::INDEX_NEW | Status::INDEX_MODIFIED)
}

fn diff_line_stats_for_paths(
    repo: &Repository,
    path: &str,
    old_path: Option<&str>,
    area: GitChangeArea,
) -> Result<Option<(u32, u32)>, GitError> {
    let mut pathspecs = vec![path];
    if let Some(old_path) = old_path {
        pathspecs.push(old_path);
    }
    let mut diff = diff_for_area(repo, &pathspecs, area)?;
    let rendered = render_diff_for_path(&mut diff, path)?;
    if rendered.is_binary || rendered.lines.is_empty() {
        return Ok(None);
    }
    Ok(Some((rendered.added, rendered.removed)))
}

fn status_result_from_entries(entries: Vec<GitChangeEntry>) -> GitStatusResult {
    let groups = status_group_projections(&entries);
    GitStatusResult { entries, groups }
}

fn status_group_projections(entries: &[GitChangeEntry]) -> Vec<GitChangeGroup> {
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();

    for (index, entry) in entries.iter().enumerate() {
        let index = index as u32;
        match entry.area {
            GitChangeArea::Staged => staged.push(index),
            GitChangeArea::Unstaged => unstaged.push(index),
            GitChangeArea::Untracked => untracked.push(index),
        }
    }

    let mut groups = Vec::with_capacity(3);
    for (area, mut entry_indices) in [
        (GitChangeArea::Staged, staged),
        (GitChangeArea::Unstaged, unstaged),
        (GitChangeArea::Untracked, untracked),
    ] {
        if entry_indices.is_empty() {
            continue;
        }
        entry_indices.sort_unstable_by(|left, right| {
            entries[*left as usize]
                .path
                .cmp(&entries[*right as usize].path)
        });
        let tree_rows = status_tree_rows(entries, &entry_indices);
        groups.push(GitChangeGroup {
            area,
            entry_indices,
            tree_rows,
        });
    }
    groups
}

#[derive(Debug)]
struct StatusTreeNode {
    name: String,
    path: String,
    depth: u32,
    file_count: u32,
    subdirectories: BTreeMap<String, StatusTreeNode>,
    files: Vec<StatusTreeFile>,
}

#[derive(Debug)]
struct StatusTreeFile {
    entry_index: u32,
    name: String,
    path: String,
    depth: u32,
}

fn status_tree_rows(entries: &[GitChangeEntry], entry_indices: &[u32]) -> Vec<GitChangeTreeRow> {
    if entry_indices.is_empty() {
        return Vec::new();
    }

    let mut root = StatusTreeNode {
        name: String::new(),
        path: String::new(),
        depth: 0,
        file_count: 0,
        subdirectories: BTreeMap::new(),
        files: Vec::new(),
    };
    for &entry_index in entry_indices {
        let entry = &entries[entry_index as usize];
        let path = entry.path.as_str();
        if path.is_empty() {
            continue;
        }

        let Some((dir_path, file_name)) = path.rsplit_once('/') else {
            root.files.push(StatusTreeFile {
                entry_index,
                name: path.to_string(),
                path: path.to_string(),
                depth: 0,
            });
            continue;
        };
        if file_name.is_empty() {
            continue;
        }

        let mut parent = &mut root;
        let mut accumulated_path = String::new();
        for (depth, component) in dir_path.split('/').enumerate() {
            if depth > 0 {
                accumulated_path.push('/');
            }
            accumulated_path.push_str(component);
            let child_path = accumulated_path.clone();
            parent = parent
                .subdirectories
                .entry(component.to_string())
                .or_insert_with(|| StatusTreeNode {
                    name: component.to_string(),
                    path: child_path,
                    depth: depth as u32,
                    file_count: 0,
                    subdirectories: BTreeMap::new(),
                    files: Vec::new(),
                });
        }
        parent.files.push(StatusTreeFile {
            entry_index,
            name: file_name.to_string(),
            path: path.to_string(),
            depth: parent.depth + 1,
        });
    }

    for child in root.subdirectories.values_mut() {
        finalize_status_tree(child);
    }

    let mut rows = Vec::new();
    for child in root.subdirectories.values() {
        append_status_tree_rows(child, &mut rows);
    }
    for file in &root.files {
        rows.push(status_tree_file_row(file));
    }
    rows
}

fn finalize_status_tree(node: &mut StatusTreeNode) -> u32 {
    let mut file_count = node.files.len() as u32;
    for child in node.subdirectories.values_mut() {
        file_count += finalize_status_tree(child);
    }
    node.file_count = file_count;
    file_count
}

fn append_status_tree_rows(node: &StatusTreeNode, rows: &mut Vec<GitChangeTreeRow>) {
    rows.push(GitChangeTreeRow {
        kind: GitChangeTreeRowKind::Directory,
        name: node.name.clone(),
        path: node.path.clone(),
        depth: node.depth,
        file_count: node.file_count,
        entry_index: None,
    });
    for child in node.subdirectories.values() {
        append_status_tree_rows(child, rows);
    }
    for file in &node.files {
        rows.push(status_tree_file_row(file));
    }
}

fn status_tree_file_row(file: &StatusTreeFile) -> GitChangeTreeRow {
    GitChangeTreeRow {
        kind: GitChangeTreeRowKind::File,
        name: file.name.clone(),
        path: file.path.clone(),
        depth: file.depth,
        file_count: 1,
        entry_index: Some(file.entry_index),
    }
}

fn delta_path(delta: &git2::DiffDelta<'_>, old: bool) -> Result<String, GitError> {
    let path = if old {
        delta.old_file().path()
    } else {
        delta.new_file().path().or_else(|| delta.old_file().path())
    };
    path.map(|path| path.to_string_lossy().to_string())
        .ok_or_else(|| GitError::new(GitErrorKind::Internal, "diff entry has no path"))
}

fn status_entry_path(entry: &StatusEntry<'_>) -> Result<Option<String>, GitError> {
    if let Some(delta) = entry.index_to_workdir().or_else(|| entry.head_to_index()) {
        return delta_path(&delta, false).map(Some);
    }
    Ok(entry.path().ok().map(ToString::to_string))
}

fn old_path_for_delta(delta: &git2::DiffDelta<'_>) -> Result<Option<String>, GitError> {
    if matches!(delta.status(), Delta::Renamed | Delta::Copied) {
        return delta_path(delta, true).map(Some);
    }
    Ok(None)
}

fn change_status_for_delta(delta: Delta, area: GitChangeArea) -> GitChangeStatus {
    match delta {
        Delta::Added => GitChangeStatus::Added,
        Delta::Deleted => GitChangeStatus::Deleted,
        Delta::Renamed => GitChangeStatus::Renamed,
        Delta::Copied => GitChangeStatus::Copied,
        Delta::Untracked => GitChangeStatus::Untracked,
        Delta::Modified | Delta::Typechange | Delta::Conflicted | Delta::Unreadable => {
            if area == GitChangeArea::Untracked {
                GitChangeStatus::Untracked
            } else {
                GitChangeStatus::Modified
            }
        }
        Delta::Unmodified | Delta::Ignored => GitChangeStatus::Modified,
    }
}
