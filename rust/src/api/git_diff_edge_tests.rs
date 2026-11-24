use super::*;

#[test]
fn git_diff_all_page_advances_through_the_file_snapshot() {
    let repo = init_repo();
    std::fs::write(repo.path().join("first.txt"), "first\n").expect("write first");
    std::fs::write(repo.path().join("second.txt"), "second\n").expect("write second");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add page files"]);
    std::fs::write(repo.path().join("first.txt"), "first changed\n").expect("modify first");
    std::fs::write(repo.path().join("second.txt"), "second changed\n").expect("modify second");

    let first = git_diff_all_page(path_str(repo.path()), vec!["first.txt".to_string()]).unwrap();
    assert_eq!(first.files.len(), 1);
    assert_eq!(first.files[0].path, "first.txt");
    assert!(!first.truncated);

    let second = git_diff_all_page(path_str(repo.path()), vec!["second.txt".to_string()]).unwrap();
    assert_eq!(second.files.len(), 1);
    assert_eq!(second.files[0].path, "second.txt");
    assert!(!second.truncated);

    let empty = git_diff_all_page(path_str(repo.path()), Vec::new()).unwrap();
    assert!(empty.files.is_empty());
    assert!(!empty.truncated);
}

#[test]
fn git_diff_returns_no_untracked_file_after_staging() {
    let repo = init_repo();
    std::fs::write(repo.path().join("stale.txt"), "stale\n").expect("write untracked");

    let before = git_diff(
        path_str(repo.path()),
        "stale.txt".to_string(),
        GitChangeArea::Untracked,
    )
    .unwrap();
    assert_eq!(before.files.len(), 1);

    run_git(repo.path(), &["add", "stale.txt"]);

    let after = git_diff(
        path_str(repo.path()),
        "stale.txt".to_string(),
        GitChangeArea::Untracked,
    )
    .unwrap();
    assert!(after.files.is_empty());
}

#[test]
fn git_diff_returns_no_untracked_file_after_commit() {
    let repo = init_repo();
    std::fs::write(repo.path().join("committed.txt"), "committed\n").expect("write untracked");

    let before = git_diff(
        path_str(repo.path()),
        "committed.txt".to_string(),
        GitChangeArea::Untracked,
    )
    .unwrap();
    assert_eq!(before.files.len(), 1);

    run_git(repo.path(), &["add", "committed.txt"]);
    run_git(repo.path(), &["commit", "-m", "commit stale file"]);

    let after = git_diff(
        path_str(repo.path()),
        "committed.txt".to_string(),
        GitChangeArea::Untracked,
    )
    .unwrap();
    assert!(after.files.is_empty());
}

#[test]
fn git_diff_keeps_untracked_replacement_after_staged_delete() {
    let repo = init_repo();
    run_git(repo.path(), &["rm", "README.md"]);
    std::fs::write(repo.path().join("README.md"), "replacement\n").expect("write replacement");

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "README.md"
            && entry.area == GitChangeArea::Staged
            && entry.status == GitChangeStatus::Deleted
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "README.md"
            && entry.area == GitChangeArea::Untracked
            && entry.status == GitChangeStatus::Untracked
    }));

    let untracked = git_diff(
        path_str(repo.path()),
        "README.md".to_string(),
        GitChangeArea::Untracked,
    )
    .unwrap();
    assert_eq!(untracked.files.len(), 1);
    assert!(diff_text(&untracked.files[0]).contains("+replacement"));

    let all = git_diff_all(path_str(repo.path()), None).unwrap();
    assert!(all.files.iter().any(|file| {
        file.path == "README.md"
            && file.area == GitChangeArea::Staged
            && file.status == GitChangeStatus::Deleted
    }));
    assert!(all.files.iter().any(|file| {
        file.path == "README.md"
            && file.area == GitChangeArea::Untracked
            && diff_text(file).contains("+replacement")
    }));
}

#[cfg(unix)]
#[test]
fn git_diff_all_keeps_readable_files_when_untracked_file_is_unreadable() {
    use std::os::unix::fs::PermissionsExt;

    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "hello\nreadable\n").expect("modify tracked");
    let unreadable = repo.path().join("unreadable.txt");
    std::fs::write(&unreadable, "unreadable\ncontent\n").expect("write unreadable file");
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000))
        .expect("make unreadable");

    let all = git_diff_all(path_str(repo.path()), None);

    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644))
        .expect("restore permissions");
    let all = all.unwrap();
    assert!(all
        .files
        .iter()
        .any(|file| file.path == "README.md" && file.area == GitChangeArea::Unstaged));
    let unreadable = all
        .files
        .iter()
        .find(|file| file.path == "unreadable.txt")
        .expect("unreadable untracked placeholder");
    assert_eq!(unreadable.area, GitChangeArea::Untracked);
    assert_eq!(unreadable.status, GitChangeStatus::Untracked);
    assert!(unreadable.lines.is_empty());
    assert_eq!(unreadable.added, None);
    assert_eq!(unreadable.removed, Some(0));
}

#[cfg(unix)]
#[test]
fn git_diff_untracked_unreadable_file_returns_placeholder() {
    use std::os::unix::fs::PermissionsExt;

    let repo = init_repo();
    let unreadable = repo.path().join("unreadable.txt");
    std::fs::write(&unreadable, "unreadable\ncontent\n").expect("write unreadable file");
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000))
        .expect("make unreadable");

    let status = git_status(path_str(repo.path())).unwrap();
    let diff = git_diff(
        path_str(repo.path()),
        "unreadable.txt".to_string(),
        GitChangeArea::Untracked,
    );

    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644))
        .expect("restore permissions");
    assert!(status.entries.iter().any(|entry| {
        entry.path == "unreadable.txt"
            && entry.area == GitChangeArea::Untracked
            && entry.status == GitChangeStatus::Untracked
    }));
    let diff = diff.unwrap();
    assert_eq!(diff.files.len(), 1);
    let file = &diff.files[0];
    assert_eq!(file.path, "unreadable.txt");
    assert_eq!(file.area, GitChangeArea::Untracked);
    assert_eq!(file.status, GitChangeStatus::Untracked);
    assert!(file.lines.is_empty());
    assert_eq!(file.added, None);
    assert_eq!(file.removed, Some(0));
}

#[cfg(unix)]
#[test]
fn git_diff_all_for_file_ignores_unrelated_changes() {
    use std::os::unix::fs::PermissionsExt;

    let repo = init_repo();
    std::fs::write(repo.path().join("target.txt"), "base\n").expect("write target");
    std::fs::write(repo.path().join("unrelated.txt"), "unrelated\n").expect("write unrelated");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add target and unrelated"]);
    std::fs::write(repo.path().join("target.txt"), "base\nstaged\n").expect("stage target");
    run_git(repo.path(), &["add", "target.txt"]);
    std::fs::write(repo.path().join("target.txt"), "base\nstaged\nunstaged\n")
        .expect("modify target");
    std::fs::write(repo.path().join("unrelated.txt"), "unrelated changed\n")
        .expect("modify unrelated");
    let unreadable = repo.path().join("unreadable.txt");
    std::fs::write(&unreadable, "unreadable\n").expect("write unreadable");
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000))
        .expect("make unreadable");

    let result = git_diff_all(path_str(repo.path()), Some("target.txt".to_string()));

    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644))
        .expect("restore permissions");
    let result = result.unwrap();
    assert!(result.files.iter().all(|file| file.path == "target.txt"));
    assert!(result
        .files
        .iter()
        .any(|file| file.area == GitChangeArea::Staged));
    assert!(result
        .files
        .iter()
        .any(|file| file.area == GitChangeArea::Unstaged));
    assert!(!result.files.iter().any(|file| file.path == "unrelated.txt"));
    assert!(!result
        .files
        .iter()
        .any(|file| file.path == "unreadable.txt"));
}

#[test]
fn git_status_lists_conflicted_files_as_unstaged_changes() {
    let repo = init_repo();
    run_git(repo.path(), &["checkout", "-b", "feature"]);
    std::fs::write(repo.path().join("README.md"), "feature\n").expect("write feature");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "feature change"]);
    run_git(repo.path(), &["checkout", "main"]);
    std::fs::write(repo.path().join("README.md"), "main\n").expect("write main");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "main change"]);

    let merge = git_command(repo.path(), &["merge", "feature"])
        .status()
        .expect("git merge runs");
    assert!(!merge.success());
    let unmerged = git_command(repo.path(), &["diff", "--name-only", "--diff-filter=U"])
        .output()
        .expect("git diff unmerged runs");
    assert!(unmerged.status.success());
    assert_eq!(
        String::from_utf8_lossy(&unmerged.stdout).trim(),
        "README.md"
    );

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "README.md"
            && entry.area == GitChangeArea::Unstaged
            && entry.status == GitChangeStatus::Modified
    }));
}

#[test]
fn git_diff_keeps_staged_rename_out_visible_from_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib = app_dir.join("lib");
    let other_dir = repo.path().join("packages").join("other");
    std::fs::create_dir_all(&app_lib).expect("create app lib");
    std::fs::create_dir_all(&other_dir).expect("create other dir");
    std::fs::write(app_lib.join("foo.dart"), "void main() {}\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app file"]);
    run_git(
        repo.path(),
        &["mv", "packages/app/lib/foo.dart", "packages/other/foo.dart"],
    );

    let status = git_status(path_str(&app_dir)).unwrap();
    let rename = status
        .entries
        .iter()
        .find(|entry| entry.path == "lib/foo.dart" && entry.area == GitChangeArea::Staged)
        .expect("rename out status entry");
    assert_eq!(rename.status, GitChangeStatus::Renamed);
    assert_eq!(rename.old_path.as_deref(), Some("lib/foo.dart"));

    let all = git_diff_all(path_str(&app_dir), None).unwrap();
    assert!(all.files.iter().any(|file| {
        file.path == "lib/foo.dart"
            && file.area == GitChangeArea::Staged
            && file.status == GitChangeStatus::Renamed
    }));

    let diff = git_diff(
        path_str(&app_dir),
        "lib/foo.dart".to_string(),
        GitChangeArea::Staged,
    )
    .unwrap();
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "lib/foo.dart");
    assert_eq!(diff.files[0].status, GitChangeStatus::Renamed);
    assert!(diff_text(&diff.files[0]).contains("rename from packages/app/lib/foo.dart"));
    assert!(diff_text(&diff.files[0]).contains("rename to packages/other/foo.dart"));
}

#[test]
fn git_diff_does_not_select_copy_delta_by_source_path() {
    let repo = init_repo();
    let source = repo
        .path()
        .join("packages")
        .join("app")
        .join("lib")
        .join("foo.dart");
    let copy = repo.path().join("packages").join("aaa").join("foo.dart");
    std::fs::create_dir_all(source.parent().unwrap()).expect("create source dir");
    std::fs::create_dir_all(copy.parent().unwrap()).expect("create copy dir");
    std::fs::write(&source, "base\n").expect("write source");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add source"]);

    std::fs::copy(&source, &copy).expect("copy source");
    std::fs::write(&source, "base\nsource change\n").expect("modify source");
    run_git(
        repo.path(),
        &["add", "packages/app/lib/foo.dart", "packages/aaa/foo.dart"],
    );

    let copy_diff = git_diff(
        path_str(repo.path()),
        "packages/aaa/foo.dart".to_string(),
        GitChangeArea::Staged,
    )
    .unwrap();
    assert_eq!(copy_diff.files.len(), 1);
    assert_eq!(copy_diff.files[0].path, "packages/aaa/foo.dart");
    assert_eq!(copy_diff.files[0].status, GitChangeStatus::Copied);
    assert_eq!(copy_diff.files[0].added, Some(0));
    assert_eq!(copy_diff.files[0].removed, Some(0));
    assert!(!diff_text(&copy_diff.files[0]).contains("+source change"));

    let diff = git_diff(
        path_str(repo.path()),
        "packages/app/lib/foo.dart".to_string(),
        GitChangeArea::Staged,
    )
    .unwrap();
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "packages/app/lib/foo.dart");
    assert_eq!(diff.files[0].status, GitChangeStatus::Modified);
    assert!(diff_text(&diff.files[0]).contains("+source change"));
    assert!(!diff_text(&diff.files[0]).contains("packages/aaa/foo.dart"));

    let all = git_diff_all(
        path_str(repo.path()),
        Some("packages/app/lib/foo.dart".to_string()),
    )
    .unwrap();
    assert_eq!(all.files.len(), 1);
    assert_eq!(all.files[0].path, "packages/app/lib/foo.dart");
    assert_eq!(all.files[0].status, GitChangeStatus::Modified);
    assert!(diff_text(&all.files[0]).contains("+source change"));
    assert!(!diff_text(&all.files[0]).contains("packages/aaa/foo.dart"));
}

#[test]
fn git_commit_diff_prefers_copied_destination_over_shared_source_path() {
    let repo = init_repo();
    let source = repo
        .path()
        .join("packages")
        .join("app")
        .join("lib")
        .join("foo.dart");
    let copy_a = repo.path().join("packages").join("aaa").join("foo.dart");
    let copy_b = repo.path().join("packages").join("bbb").join("foo.dart");
    std::fs::create_dir_all(source.parent().unwrap()).expect("create source dir");
    std::fs::create_dir_all(copy_a.parent().unwrap()).expect("create copy a dir");
    std::fs::create_dir_all(copy_b.parent().unwrap()).expect("create copy b dir");
    std::fs::write(&source, "base\n").expect("write source");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add source"]);

    std::fs::copy(&source, &copy_a).expect("copy source a");
    std::fs::copy(&source, &copy_b).expect("copy source b");
    std::fs::write(&source, "base\nsource change\n").expect("modify source");
    run_git(
        repo.path(),
        &[
            "add",
            "packages/app/lib/foo.dart",
            "packages/aaa/foo.dart",
            "packages/bbb/foo.dart",
        ],
    );
    run_git(repo.path(), &["commit", "-m", "copy source twice"]);

    let git_repo = git2::Repository::open(repo.path()).expect("open repo");
    let commit = git_repo
        .head()
        .expect("head")
        .peel_to_commit()
        .expect("head commit");
    let diff = git_commit_diff(
        path_str(repo.path()),
        commit.id().to_string(),
        Some(commit.parent_id(0).expect("parent").to_string()),
        Some("packages/bbb/foo.dart".to_string()),
        Some("packages/app/lib/foo.dart".to_string()),
    )
    .unwrap();

    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "packages/bbb/foo.dart");
    assert_eq!(
        diff.files[0].old_path.as_deref(),
        Some("packages/app/lib/foo.dart")
    );
    assert_eq!(diff.files[0].status, GitChangeStatus::Copied);
    assert!(!diff_text(&diff.files[0]).contains("packages/aaa/foo.dart"));
}

#[test]
fn git_commit_compare_ignores_copy_sources_outside_scoped_workspace() {
    let repo = init_repo();
    let source = repo
        .path()
        .join("packages")
        .join("app")
        .join("lib")
        .join("foo.dart");
    let copy = repo.path().join("packages").join("other").join("foo.dart");
    std::fs::create_dir_all(source.parent().unwrap()).expect("create source dir");
    std::fs::create_dir_all(copy.parent().unwrap()).expect("create copy dir");
    std::fs::write(&source, "base\n").expect("write source");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add source"]);

    std::fs::copy(&source, &copy).expect("copy source");
    std::fs::write(&source, "base\nsource change\n").expect("modify source");
    run_git(
        repo.path(),
        &[
            "add",
            "packages/app/lib/foo.dart",
            "packages/other/foo.dart",
        ],
    );
    run_git(repo.path(), &["commit", "-m", "copy source out"]);

    let git_repo = git2::Repository::open(repo.path()).expect("open repo");
    let commit = git_repo
        .head()
        .expect("head")
        .peel_to_commit()
        .expect("head commit");
    let compare = git_commit_compare(
        path_str(&repo.path().join("packages/app")),
        commit.id().to_string(),
    )
    .unwrap();

    assert_eq!(compare.entries.len(), 1);
    assert_eq!(compare.entries[0].path, "lib/foo.dart");
    assert_eq!(compare.entries[0].status, GitChangeStatus::Modified);
}

#[test]
fn git_diff_batch_preserves_requested_order_and_refreshes_between_requests() {
    let repo = init_repo();
    for i in 0..24 {
        std::fs::write(
            repo.path().join(format!("file-{i:02}.txt")),
            format!("base {i}\n"),
        )
        .expect("write batch baseline");
    }
    std::fs::write(repo.path().join("rename-from.txt"), "rename fixture\n")
        .expect("write rename fixture");
    std::fs::write(repo.path().join("copy-source.txt"), "copy fixture\n")
        .expect("write copy fixture");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add batch fixtures"]);
    run_git(repo.path(), &["mv", "rename-from.txt", "rename-to.txt"]);
    std::fs::copy(
        repo.path().join("copy-source.txt"),
        repo.path().join("copy-destination.txt"),
    )
    .expect("copy fixture");
    std::fs::write(
        repo.path().join("copy-source.txt"),
        "copy fixture\nsource change\n",
    )
    .expect("modify copy source");
    run_git(
        repo.path(),
        &["add", "copy-source.txt", "copy-destination.txt"],
    );
    for i in 0..24 {
        let name = format!("file-{i:02}.txt");
        std::fs::write(repo.path().join(&name), format!("base {i}\nstaged {i}\n"))
            .expect("write staged");
        if i % 2 == 0 {
            run_git(repo.path(), &["add", &name]);
        }
        std::fs::write(
            repo.path().join(&name),
            format!("base {i}\nstaged {i}\nworktree {i}\n"),
        )
        .expect("write worktree");
    }
    std::fs::write(repo.path().join("untracked.txt"), "new file\n").expect("write untracked");
    let root = path_str(repo.path());
    let mut requested = vec![
        "rename-from.txt".to_string(),
        "copy-destination.txt".to_string(),
        "copy-source.txt".to_string(),
        "untracked.txt".to_string(),
        "missing.txt".to_string(),
    ];
    requested.extend((0..24).rev().map(|i| format!("file-{i:02}.txt")));
    let signature = |file: &GitDiffFile| {
        (
            file.path.clone(),
            file.old_path.clone(),
            file.area,
            file.status,
            file.added,
            file.removed,
            file.is_binary,
            diff_text(file),
        )
    };
    let expected = requested
        .iter()
        .flat_map(|path| {
            git_diff_all(root.clone(), Some(path.clone()))
                .expect("single-path diff")
                .files
        })
        .map(|file| signature(&file))
        .collect::<Vec<_>>();
    let page = git_diff_all_page(root.clone(), requested.clone()).expect("batch diff");
    assert!(!page.truncated);
    assert_eq!(
        page.files.iter().map(signature).collect::<Vec<_>>(),
        expected
    );
    // Different request must see newly staged and committed changes.
    run_git(repo.path(), &["add", "untracked.txt"]);
    let updated = git_diff_all_page(root.clone(), requested.clone()).expect("fresh batch");
    assert!(updated
        .files
        .iter()
        .any(|file| file.path == "untracked.txt" && file.area == GitChangeArea::Staged));
    assert!(!updated
        .files
        .iter()
        .any(|file| file.path == "untracked.txt" && file.area == GitChangeArea::Untracked));
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "commit batch"]);
    let clean = git_diff_all_page(root, requested).expect("clean batch");
    assert!(clean.files.is_empty());
}

#[test]
fn git_diff_batch_keeps_whitespace_mode_and_literal_paths() {
    let repo = init_repo();
    std::fs::write(repo.path().join("file[1].txt"), "alpha beta\n").expect("baseline");
    std::fs::write(repo.path().join("file1.txt"), "before\n").expect("baseline");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add literal path fixtures"]);
    std::fs::write(repo.path().join("file[1].txt"), "alpha  beta\n").expect("whitespace change");
    std::fs::write(repo.path().join("file1.txt"), "after\n").expect("content change");
    let root = path_str(repo.path());
    let files = vec!["file[1].txt".to_string(), "file1.txt".to_string()];
    let page = git_diff_all_page_with_whitespace(
        root.clone(),
        files.clone(),
        GitDiffWhitespaceMode::IgnoreAll,
    )
    .expect("whitespace batch");
    let expected = files
        .iter()
        .flat_map(|path| {
            git_diff_all_with_whitespace(
                root.clone(),
                Some(path.clone()),
                GitDiffWhitespaceMode::IgnoreAll,
            )
            .expect("single-path whitespace diff")
            .files
        })
        .map(|file| (file.path.clone(), diff_text(&file)))
        .collect::<Vec<_>>();
    assert_eq!(
        page.files
            .iter()
            .map(|file| (file.path.clone(), diff_text(file)))
            .collect::<Vec<_>>(),
        expected
    );
    let normal = git_diff_all_page(root, files).expect("normal batch");
    assert_eq!(normal.files.len(), 2);
}
