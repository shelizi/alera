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
