use super::git_diff_impl::{full_file_projection, full_file_side_by_side_projection};
use super::*;

#[test]
fn diff_accepts_backslash_separators_from_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&lib_dir).expect("create lib dir");
    std::fs::write(lib_dir.join("foo.dart"), "void main() {}\n").expect("write file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app file"]);
    std::fs::write(lib_dir.join("foo.dart"), "void main() {\n  print(1);\n}\n")
        .expect("modify file");

    let diff = git_diff(
        path_str(&app_dir),
        "lib\\foo.dart".to_string(),
        GitChangeArea::Unstaged,
    )
    .unwrap();

    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "lib/foo.dart");
    assert!(diff_text(&diff.files[0]).contains("+  print(1);"));
}

#[test]
fn git_diff_decodes_big5_tracked_text_from_full_file_encoding() {
    let repo = init_repo();
    let (before, _, before_had_errors) =
        encoding_rs::BIG5.encode("\u{6a19}\u{984c}\n\u{820a}\u{5167}\u{5bb9}\n\u{7d50}\u{5c3e}\n");
    assert!(!before_had_errors);
    std::fs::write(repo.path().join("legacy.txt"), before.as_ref()).expect("write Big5 baseline");
    run_git(repo.path(), &["add", "legacy.txt"]);
    run_git(repo.path(), &["commit", "-m", "add Big5 fixture"]);

    let (after, _, after_had_errors) =
        encoding_rs::BIG5.encode("\u{6a19}\u{984c}\n\u{65b0}\u{5167}\u{5bb9}\n\u{7d50}\u{5c3e}\n");
    assert!(!after_had_errors);
    std::fs::write(repo.path().join("legacy.txt"), after.as_ref()).expect("write Big5 change");

    let diff = git_diff(
        path_str(repo.path()),
        "legacy.txt".to_string(),
        GitChangeArea::Unstaged,
    )
    .expect("Big5 diff");
    let text = diff_text(&diff.files[0]);

    assert!(text.contains("-\u{820a}\u{5167}\u{5bb9}"), "{text}");
    assert!(text.contains("+\u{65b0}\u{5167}\u{5bb9}"), "{text}");
    assert!(!text.contains('\u{fffd}'), "{text}");

    run_git(repo.path(), &["add", "legacy.txt"]);
    let staged = git_diff(
        path_str(repo.path()),
        "legacy.txt".to_string(),
        GitChangeArea::Staged,
    )
    .expect("staged Big5 diff");
    let staged_text = diff_text(&staged.files[0]);
    assert!(
        staged_text.contains("-\u{820a}\u{5167}\u{5bb9}"),
        "{staged_text}"
    );
    assert!(
        staged_text.contains("+\u{65b0}\u{5167}\u{5bb9}"),
        "{staged_text}"
    );
    assert!(!staged_text.contains('\u{fffd}'), "{staged_text}");

    run_git(repo.path(), &["commit", "-m", "modify Big5 fixture"]);
    let history =
        git_history(path_str(repo.path()), Some(2), None, None, None).expect("Big5 commit history");
    let committed = git_commit_diff(
        path_str(repo.path()),
        history.items[0].id.clone(),
        Some(history.items[1].id.clone()),
        Some("legacy.txt".to_string()),
        None,
    )
    .expect("committed Big5 diff");
    let committed_text = diff_text(&committed.files[0]);
    assert!(
        committed_text.contains("-\u{820a}\u{5167}\u{5bb9}"),
        "{committed_text}"
    );
    assert!(
        committed_text.contains("+\u{65b0}\u{5167}\u{5bb9}"),
        "{committed_text}"
    );
    assert!(!committed_text.contains('\u{fffd}'), "{committed_text}");
}

#[test]
fn git_diff_decodes_big5_untracked_text_instead_of_marking_it_binary() {
    let repo = init_repo();
    let (bytes, _, had_errors) = encoding_rs::BIG5.encode("\u{7e41}\u{9ad4}\u{4e2d}\u{6587}\n");
    assert!(!had_errors);
    std::fs::write(repo.path().join("legacy-new.txt"), bytes.as_ref())
        .expect("write untracked Big5 fixture");

    let diff = git_diff(
        path_str(repo.path()),
        "legacy-new.txt".to_string(),
        GitChangeArea::Untracked,
    )
    .expect("untracked Big5 diff");
    let file = &diff.files[0];

    assert!(!file.is_binary);
    let text = diff_text(file);
    assert!(text.contains("+\u{7e41}\u{9ad4}\u{4e2d}\u{6587}"), "{text}");
    assert!(!text.contains('\u{fffd}'), "{text}");
}

#[test]
fn git_diff_projects_side_by_side_rows_without_duplicating_line_text() {
    let repo = init_repo();
    std::fs::write(
        repo.path().join("paired.txt"),
        "before\nold one\nold two\nafter\n",
    )
    .expect("write baseline");
    run_git(repo.path(), &["add", "paired.txt"]);
    run_git(repo.path(), &["commit", "-m", "add paired fixture"]);
    std::fs::write(
        repo.path().join("paired.txt"),
        "before\nnew one\nnew two\nnew three\nafter\n",
    )
    .expect("write modified fixture");

    let diff = git_diff(
        path_str(repo.path()),
        "paired.txt".to_string(),
        GitChangeArea::Unstaged,
    )
    .unwrap();
    let file = &diff.files[0];

    assert!(!file.side_by_side_rows.is_empty());
    assert!(file
        .side_by_side_rows
        .iter()
        .filter(|row| row.kind == GitDiffSideBySideRowKind::Passthrough)
        .all(|row| row.line_index.is_some()
            && row.left_line_index.is_none()
            && row.right_line_index.is_none()));

    let pairs = file
        .side_by_side_rows
        .iter()
        .filter(|row| row.kind == GitDiffSideBySideRowKind::Pair)
        .map(|row| {
            let left = row.left_line_index.map(|index| {
                (
                    row.left_line_number,
                    file.lines[index as usize].kind,
                    file.lines[index as usize].text.as_str(),
                )
            });
            let right = row.right_line_index.map(|index| {
                (
                    row.right_line_number,
                    file.lines[index as usize].kind,
                    file.lines[index as usize].text.as_str(),
                )
            });
            (left, right)
        })
        .collect::<Vec<_>>();

    assert!(pairs.iter().any(|(left, right)| {
        matches!(left, Some((Some(1), GitDiffLineKind::Context, text)) if *text == " before")
            && matches!(right, Some((Some(1), GitDiffLineKind::Context, text)) if *text == " before")
    }));
    assert!(pairs.iter().any(|(left, right)| {
        matches!(left, Some((Some(2), GitDiffLineKind::Deletion, text)) if *text == "-old one")
            && matches!(right, Some((Some(2), GitDiffLineKind::Addition, text)) if *text == "+new one")
    }));
    assert!(pairs.iter().any(|(left, right)| {
        matches!(left, Some((Some(3), GitDiffLineKind::Deletion, text)) if *text == "-old two")
            && matches!(right, Some((Some(3), GitDiffLineKind::Addition, text)) if *text == "+new two")
    }));
    assert!(pairs.iter().any(|(left, right)| {
        left.is_none()
            && matches!(right, Some((Some(4), GitDiffLineKind::Addition, text)) if *text == "+new three")
    }));
}

#[test]
fn git_diff_projects_single_column_full_file_alignment_plan() {
    let lines = vec![
        GitDiffLine {
            text: "@@ -3,2 +3,2 @@".to_string(),
            kind: GitDiffLineKind::Hunk,
        },
        GitDiffLine {
            text: "-old three".to_string(),
            kind: GitDiffLineKind::Deletion,
        },
        GitDiffLine {
            text: "+new three".to_string(),
            kind: GitDiffLineKind::Addition,
        },
        GitDiffLine {
            text: " line four".to_string(),
            kind: GitDiffLineKind::Context,
        },
    ];

    let projection = full_file_projection(&lines);

    assert_eq!(projection.len(), 5);
    assert_eq!(projection[0].kind, GitDiffFullFileRowKind::ContextRange);
    assert_eq!(projection[0].start_index, Some(0));
    assert_eq!(projection[0].end_index, Some(2));

    assert_eq!(projection[1].kind, GitDiffFullFileRowKind::Line);
    assert_eq!(projection[1].full_line_index, None);
    assert_eq!(projection[1].diff_line_index, Some(1));
    assert_eq!(projection[1].line_number, Some(3));

    assert_eq!(projection[2].kind, GitDiffFullFileRowKind::Line);
    assert_eq!(projection[2].full_line_index, Some(2));
    assert_eq!(projection[2].diff_line_index, Some(2));
    assert_eq!(projection[2].line_number, Some(3));

    assert_eq!(projection[3].kind, GitDiffFullFileRowKind::Line);
    assert_eq!(projection[3].full_line_index, Some(3));
    assert_eq!(projection[3].diff_line_index, Some(3));
    assert_eq!(projection[3].line_number, Some(4));

    assert_eq!(projection[4].kind, GitDiffFullFileRowKind::ContextRange);
    assert_eq!(projection[4].start_index, Some(4));
    assert_eq!(projection[4].end_index, None);
}

#[test]
fn git_diff_projects_full_file_side_by_side_alignment_plan() {
    let lines = vec![
        GitDiffLine {
            text: "@@ -3,2 +3,2 @@".to_string(),
            kind: GitDiffLineKind::Hunk,
        },
        GitDiffLine {
            text: "-old three".to_string(),
            kind: GitDiffLineKind::Deletion,
        },
        GitDiffLine {
            text: "+new three".to_string(),
            kind: GitDiffLineKind::Addition,
        },
        GitDiffLine {
            text: " line four".to_string(),
            kind: GitDiffLineKind::Context,
        },
    ];

    let projection = full_file_side_by_side_projection(&lines);

    assert_eq!(projection.len(), 4);
    assert_eq!(
        projection[0].kind,
        GitDiffFullFileSideBySideRowKind::ContextRange
    );
    assert_eq!(projection[0].old_start_index, Some(0));
    assert_eq!(projection[0].old_end_index, Some(2));
    assert_eq!(projection[0].new_start_index, Some(0));
    assert_eq!(projection[0].new_end_index, Some(2));

    assert_eq!(projection[1].kind, GitDiffFullFileSideBySideRowKind::Pair);
    assert_eq!(projection[1].old_start_index, Some(2));
    assert_eq!(projection[1].new_start_index, Some(2));
    assert_eq!(projection[1].left_diff_line_index, Some(1));
    assert_eq!(projection[1].right_diff_line_index, Some(2));

    assert_eq!(projection[2].kind, GitDiffFullFileSideBySideRowKind::Pair);
    assert_eq!(projection[2].old_start_index, Some(3));
    assert_eq!(projection[2].new_start_index, Some(3));
    assert_eq!(projection[2].left_diff_line_index, Some(3));
    assert_eq!(projection[2].right_diff_line_index, Some(3));

    assert_eq!(
        projection[3].kind,
        GitDiffFullFileSideBySideRowKind::ContextRange
    );
    assert_eq!(projection[3].old_start_index, Some(4));
    assert_eq!(projection[3].old_end_index, None);
    assert_eq!(projection[3].new_start_index, Some(4));
    assert_eq!(projection[3].new_end_index, None);
}

#[test]
fn git_diff_loads_single_area_and_combined_results() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\n").expect("write staged");
    run_git(repo.path(), &["add", "README.md"]);
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\nunstaged\n")
        .expect("write unstaged");
    std::fs::write(repo.path().join("new.txt"), "new\n").expect("write untracked");

    let staged = git_diff(
        path_str(repo.path()),
        "README.md".to_string(),
        GitChangeArea::Staged,
    )
    .unwrap();
    assert_eq!(staged.files.len(), 1);
    assert!(diff_text(&staged.files[0]).contains("+staged"));
    assert!(staged.files[0].added.unwrap_or(0) >= 1);

    let all = git_diff_all(path_str(repo.path()), None).unwrap();
    assert!(all
        .files
        .iter()
        .any(|file| file.path == "README.md" && file.area == GitChangeArea::Staged));
    assert!(all
        .files
        .iter()
        .any(|file| file.path == "README.md" && file.area == GitChangeArea::Unstaged));
    assert!(all
        .files
        .iter()
        .any(|file| file.path == "new.txt" && file.area == GitChangeArea::Untracked));
    let untracked = all
        .files
        .iter()
        .find(|file| file.path == "new.txt")
        .expect("untracked diff");
    assert_eq!(untracked.added, Some(1));
}

#[test]
fn git_diff_whitespace_modes_filter_worktree_and_staged_changes() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "alpha beta\ntrail\n").expect("write baseline");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "whitespace baseline"]);
    std::fs::write(repo.path().join("README.md"), "alpha    beta\ntrail   \n")
        .expect("write whitespace changes");

    let normal = git_diff_with_whitespace(
        path_str(repo.path()),
        "README.md".to_string(),
        GitChangeArea::Unstaged,
        GitDiffWhitespaceMode::Normal,
    )
    .unwrap();
    assert_eq!(normal.files.len(), 1);

    let ignore_eol = git_diff_with_whitespace(
        path_str(repo.path()),
        "README.md".to_string(),
        GitChangeArea::Unstaged,
        GitDiffWhitespaceMode::IgnoreEol,
    )
    .unwrap();
    assert_eq!(ignore_eol.files.len(), 1);

    let ignore_changes = git_diff_with_whitespace(
        path_str(repo.path()),
        "README.md".to_string(),
        GitChangeArea::Unstaged,
        GitDiffWhitespaceMode::IgnoreChanges,
    )
    .unwrap();
    assert!(ignore_changes.files.is_empty());

    let ignore_all = git_diff_with_whitespace(
        path_str(repo.path()),
        "README.md".to_string(),
        GitChangeArea::Unstaged,
        GitDiffWhitespaceMode::IgnoreAll,
    )
    .unwrap();
    assert!(ignore_all.files.is_empty());

    let combined = git_diff_all_with_whitespace(
        path_str(repo.path()),
        None,
        GitDiffWhitespaceMode::IgnoreChanges,
    )
    .unwrap();
    assert!(combined.files.is_empty());

    let page = git_diff_all_page_with_whitespace(
        path_str(repo.path()),
        vec!["README.md".to_string()],
        GitDiffWhitespaceMode::IgnoreChanges,
    )
    .unwrap();
    assert!(page.files.is_empty());

    run_git(repo.path(), &["add", "README.md"]);
    let staged = git_diff_with_whitespace(
        path_str(repo.path()),
        "README.md".to_string(),
        GitChangeArea::Staged,
        GitDiffWhitespaceMode::IgnoreChanges,
    )
    .unwrap();
    assert!(staged.files.is_empty());
}

#[test]
fn git_commit_diff_applies_whitespace_mode() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "alpha beta\n").expect("write baseline");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "commit whitespace baseline"]);
    let parent_oid = git2::Repository::open(repo.path())
        .unwrap()
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id();

    std::fs::write(repo.path().join("README.md"), "alpha    beta\n")
        .expect("write whitespace change");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "commit whitespace change"]);
    let commit_oid = git2::Repository::open(repo.path())
        .unwrap()
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id();

    let normal = git_commit_diff_with_whitespace(
        path_str(repo.path()),
        commit_oid.to_string(),
        Some(parent_oid.to_string()),
        Some("README.md".to_string()),
        None,
        GitDiffWhitespaceMode::Normal,
    )
    .unwrap();
    assert_eq!(normal.files.len(), 1);

    let ignored = git_commit_diff_with_whitespace(
        path_str(repo.path()),
        commit_oid.to_string(),
        Some(parent_oid.to_string()),
        Some("README.md".to_string()),
        None,
        GitDiffWhitespaceMode::IgnoreChanges,
    )
    .unwrap();
    assert!(ignored.files.is_empty());
}

#[cfg(unix)]
#[test]
fn git_untracked_symlink_diff_does_not_read_target_contents() {
    use std::os::unix::fs::symlink;

    let repo = init_repo();
    let outside = tempfile::tempdir().expect("external tempdir");
    let target = outside.path().join("secret.txt");
    std::fs::write(&target, "SECRET MATERIAL\n").expect("write external target");
    symlink(&target, repo.path().join("secrets")).expect("create symlink");

    let diff = git_diff(
        path_str(repo.path()),
        "secrets".to_string(),
        GitChangeArea::Untracked,
    )
    .unwrap();

    assert_eq!(diff.files.len(), 1);
    assert!(diff_text(&diff.files[0]).contains("new file mode 120000"));
    assert!(diff_text(&diff.files[0]).contains(&path_str(&target)));
    assert!(!diff_text(&diff.files[0]).contains("SECRET MATERIAL"));
}

#[test]
fn git_diff_preserves_staged_rename_pairs() {
    let repo = init_repo();
    std::fs::write(repo.path().join("old.txt"), "same\ncontent\n").expect("write old file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add old"]);
    run_git(repo.path(), &["mv", "old.txt", "new.txt"]);
    run_git(repo.path(), &["add", "-A"]);

    let status = git_status(path_str(repo.path())).unwrap();
    let rename = status
        .entries
        .iter()
        .find(|entry| entry.path == "new.txt")
        .expect("rename status entry");

    assert_eq!(rename.area, GitChangeArea::Staged);
    assert_eq!(rename.status, GitChangeStatus::Renamed);
    assert_eq!(rename.old_path.as_deref(), Some("old.txt"));

    let diff = git_diff(
        path_str(repo.path()),
        "new.txt".to_string(),
        GitChangeArea::Staged,
    )
    .unwrap();

    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].status, GitChangeStatus::Renamed);
    assert_eq!(diff.files[0].old_path.as_deref(), Some("old.txt"));
    assert!(diff_text(&diff.files[0]).contains("rename from old.txt"));
    assert!(diff_text(&diff.files[0]).contains("rename to new.txt"));
}

#[test]
fn git_diff_truncates_large_unicode_without_panicking() {
    let repo = init_repo();
    let total_added_lines = 120_000u32;
    let expected_added_lines = total_added_lines + 1;
    let repeated = (0..total_added_lines)
        .map(|index| format!("á{index}\n"))
        .collect::<String>();
    std::fs::write(repo.path().join("README.md"), format!("hello\n{repeated}"))
        .expect("write large unicode diff");

    let single = git_diff(
        path_str(repo.path()),
        "README.md".to_string(),
        GitChangeArea::Unstaged,
    )
    .unwrap();

    assert_eq!(single.files.len(), 1);
    assert!(single.files[0].line_preview_truncated);
    assert_eq!(single.files[0].added, Some(expected_added_lines));
    let single_text = diff_text(&single.files[0]);
    assert!(single_text.is_char_boundary(single_text.len()));

    let combined = git_diff_all(path_str(repo.path()), None).unwrap();
    assert!(combined.files[0].line_preview_truncated);
    assert_eq!(combined.files[0].added, Some(expected_added_lines));
    let combined_text = diff_text(&combined.files[0]);
    assert!(combined_text.is_char_boundary(combined_text.len()));
}
