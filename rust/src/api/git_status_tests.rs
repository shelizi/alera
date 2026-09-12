use super::*;

#[test]
fn operates_from_subdirectory() {
    let repo = init_repo();
    let subdir = repo.path().join("nested").join("dir");
    std::fs::create_dir_all(&subdir).expect("create subdir");

    let subdir_path = path_str(&subdir);
    assert!(is_git_repository(subdir_path.clone()).unwrap());
    assert_eq!(current_branch(subdir_path.clone()).unwrap(), "main");
    assert!(list_branches(subdir_path)
        .unwrap()
        .contains(&"main".to_string()));
}

#[test]
fn status_preserves_punctuation_and_control_character_paths() {
    let repo = init_repo();
    let comma_path = repo.path().join("comma,name.txt");
    #[cfg(unix)]
    let newline_path = repo.path().join("line\nbreak.txt");
    std::fs::write(&comma_path, "comma\n").expect("write comma path");
    #[cfg(unix)]
    std::fs::write(&newline_path, "newline\n").expect("write newline path");

    let status = git_status(path_str(repo.path())).unwrap();

    assert!(status.entries.iter().any(|entry| {
        entry.path == "comma,name.txt"
            && entry.area == GitChangeArea::Untracked
            && entry.status == GitChangeStatus::Untracked
    }));
    #[cfg(unix)]
    assert!(status.entries.iter().any(|entry| {
        entry.path == "line\nbreak.txt"
            && entry.area == GitChangeArea::Untracked
            && entry.status == GitChangeStatus::Untracked
    }));
}

#[test]
fn concurrent_status_refreshes_do_not_share_mutable_probe_state() {
    let repo = init_repo();
    std::fs::write(repo.path().join("changed.txt"), "changed\n").expect("write changed");
    let repo_path = path_str(repo.path());

    let handles = (0..8)
        .map(|_| {
            let path = repo_path.clone();
            std::thread::spawn(move || git_status(path).unwrap())
        })
        .collect::<Vec<_>>();

    for handle in handles {
        let status = handle.join().expect("status thread joins");
        assert!(status.entries.iter().any(|entry| {
            entry.path == "changed.txt" && entry.status == GitChangeStatus::Untracked
        }));
    }
}

#[test]
fn git_status_splits_untracked_unstaged_and_staged_changes() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\n").expect("write staged");
    run_git(repo.path(), &["add", "README.md"]);
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\nunstaged\n")
        .expect("write unstaged");
    std::fs::write(repo.path().join("new.txt"), "new\nfile\n").expect("write untracked");

    let status = git_status(path_str(repo.path())).unwrap();

    assert!(status.entries.iter().any(|entry| {
        entry.path == "README.md"
            && entry.area == GitChangeArea::Staged
            && entry.status == GitChangeStatus::Modified
            && entry.added.unwrap_or(0) >= 1
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "README.md"
            && entry.area == GitChangeArea::Unstaged
            && entry.status == GitChangeStatus::Modified
            && entry.added == Some(1)
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "new.txt"
            && entry.area == GitChangeArea::Untracked
            && entry.status == GitChangeStatus::Untracked
            && entry.added.is_none()
            && entry.removed == Some(0)
    }));
    // Groups and tree rows are derived on the Dart side, so the wire result
    // carries the flat entries only, ordered untracked then unstaged then staged.
    assert!(status.groups.is_empty());
    assert_eq!(
        status
            .entries
            .iter()
            .map(|entry| entry.area)
            .collect::<Vec<_>>(),
        vec![
            GitChangeArea::Untracked,
            GitChangeArea::Unstaged,
            GitChangeArea::Staged,
        ]
    );
}

#[cfg(unix)]
#[test]
fn git_status_lists_unreadable_untracked_files_without_reading_content() {
    use std::os::unix::fs::PermissionsExt;

    let repo = init_repo();
    let unreadable = repo.path().join("unreadable.txt");
    std::fs::write(&unreadable, "unreadable\ncontent\n").expect("write unreadable file");
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000))
        .expect("make unreadable");

    let status = git_status(path_str(repo.path())).unwrap();

    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644))
        .expect("restore permissions");
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path == "unreadable.txt")
        .expect("unreadable untracked entry");
    assert_eq!(entry.area, GitChangeArea::Untracked);
    assert!(entry.added.is_none());
    assert_eq!(entry.removed, Some(0));
}

#[test]
fn git_status_for_path_limits_results_to_selected_file() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\n").expect("modify readme");
    run_git(repo.path(), &["add", "README.md"]);
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\nunstaged\n")
        .expect("modify readme again");
    std::fs::write(repo.path().join("unrelated.txt"), "unrelated\n").expect("write unrelated");

    let status = git_status_for_path(path_str(repo.path()), "README.md".to_string()).unwrap();

    assert_eq!(status.entries.len(), 2);
    assert!(status.entries.iter().all(|entry| entry.path == "README.md"));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.area == GitChangeArea::Staged));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "unrelated.txt"));
}

#[test]
fn git_status_leaves_groups_empty_for_client_side_tree_projection() {
    let repo = init_repo();
    std::fs::create_dir_all(repo.path().join("lib/src")).expect("create lib src");
    std::fs::write(repo.path().join("lib/src/a.dart"), "a\n").expect("write a");
    std::fs::write(repo.path().join("lib/b.dart"), "b\n").expect("write b");

    let status = git_status(path_str(repo.path())).unwrap();

    // The source control panel rebuilds groups and tree rows on the Dart side
    // from these flat entries, so nested paths cross the bridge exactly once.
    assert!(status.groups.is_empty());
    assert_eq!(
        status
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec!["lib/b.dart", "lib/src/a.dart"]
    );
    assert!(status
        .entries
        .iter()
        .all(|entry| entry.area == GitChangeArea::Untracked));
}

#[test]
fn git_status_uses_workspace_relative_paths_for_subdirectories() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&lib_dir).expect("create app lib dir");
    std::fs::write(lib_dir.join("foo.dart"), "void main() {}\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app"]);

    std::fs::write(
        lib_dir.join("foo.dart"),
        "void main() {\n  print('changed');\n}\n",
    )
    .expect("modify app file");
    std::fs::write(repo.path().join("README.md"), "root changed\n").expect("modify root file");

    let status = git_status(path_str(&app_dir)).unwrap();

    assert_eq!(status.entries.len(), 1);
    assert_eq!(status.entries[0].path, "lib/foo.dart");
    assert_eq!(status.entries[0].area, GitChangeArea::Unstaged);

    let diff = git_diff(
        path_str(&app_dir),
        "lib/foo.dart".to_string(),
        GitChangeArea::Unstaged,
    )
    .unwrap();
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "lib/foo.dart");
    assert!(diff_text(&diff.files[0]).contains("changed"));
}
