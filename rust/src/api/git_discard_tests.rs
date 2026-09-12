use super::*;

#[test]
fn git_discard_treats_selected_pathspec_characters_as_literals() {
    let repo = init_repo();
    std::fs::write(repo.path().join("file[1].txt"), "literal bracket\n")
        .expect("write bracket file");
    std::fs::write(repo.path().join("file1.txt"), "glob match\n").expect("write match file");
    run_git(repo.path(), &["add", "file[1].txt", "file1.txt"]);
    run_git(repo.path(), &["commit", "-m", "add bracket files"]);

    std::fs::write(repo.path().join("file[1].txt"), "literal bracket changed\n")
        .expect("modify bracket file");
    std::fs::write(repo.path().join("file1.txt"), "glob match changed\n")
        .expect("modify match file");

    git_discard(path_str(repo.path()), Some("file[1].txt".to_string())).unwrap();

    assert_eq!(
        std::fs::read_to_string(repo.path().join("file[1].txt")).unwrap(),
        "literal bracket\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("file1.txt")).unwrap(),
        "glob match changed\n"
    );
    let status = git_status(path_str(repo.path())).unwrap();
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "file[1].txt"));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "file1.txt" && entry.area == GitChangeArea::Unstaged));
}

#[test]
fn git_discard_restores_tracked_and_deletes_untracked() {
    let repo = init_repo();
    let tracked = repo.path().join("README.md");
    let untracked = repo.path().join("scratch.txt");
    std::fs::write(&tracked, "changed\n").expect("modify readme");
    std::fs::write(&untracked, "scratch\n").expect("write scratch");

    git_discard(path_str(repo.path()), None).unwrap();

    assert_eq!(std::fs::read_to_string(&tracked).unwrap(), "hello");
    assert!(!untracked.exists());
    assert!(git_status(path_str(repo.path()))
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn git_discard_all_removes_unstaged_rename_destination() {
    let repo = init_repo();
    let old_path = repo.path().join("old.txt");
    let new_path = repo.path().join("new.txt");
    std::fs::write(&old_path, "old\n").expect("write old file");
    run_git(repo.path(), &["add", "old.txt"]);
    run_git(repo.path(), &["commit", "-m", "add old file"]);

    std::fs::rename(&old_path, &new_path).expect("rename tracked file");

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "new.txt"
            && entry.old_path.as_deref() == Some("old.txt")
            && entry.area == GitChangeArea::Unstaged
            && entry.status == GitChangeStatus::Renamed
    }));

    git_discard(path_str(repo.path()), None).unwrap();

    assert_eq!(std::fs::read_to_string(&old_path).unwrap(), "old\n");
    assert!(!new_path.exists());
    assert!(git_status(path_str(repo.path()))
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn git_discard_all_keeps_rename_out_destination_outside_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    let other_dir = repo.path().join("packages").join("other");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::create_dir_all(&other_dir).expect("create other dir");
    let old_path = app_lib_dir.join("foo.dart");
    let new_path = other_dir.join("foo.dart");
    std::fs::write(&old_path, "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app file"]);

    std::fs::rename(&old_path, &new_path).expect("rename file out of workspace");

    let status = git_status(path_str(&app_dir)).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "lib/foo.dart"
            && entry.old_path.as_deref() == Some("lib/foo.dart")
            && entry.area == GitChangeArea::Unstaged
            && entry.status == GitChangeStatus::Renamed
    }));

    git_discard(path_str(&app_dir), None).unwrap();

    assert_eq!(std::fs::read_to_string(&old_path).unwrap(), "app\n");
    assert_eq!(std::fs::read_to_string(&new_path).unwrap(), "app\n");
    assert!(git_status(path_str(&app_dir)).unwrap().entries.is_empty());
}

#[test]
fn git_discard_area_limits_to_untracked_folder_entries() {
    let repo = init_repo();
    std::fs::create_dir_all(repo.path().join("lib")).expect("create lib");
    std::fs::create_dir_all(repo.path().join("test")).expect("create test");
    std::fs::write(repo.path().join("lib/tracked.dart"), "lib\n").expect("write lib tracked");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add tracked file"]);

    std::fs::write(repo.path().join("lib/tracked.dart"), "lib changed\n")
        .expect("modify lib tracked");
    std::fs::write(repo.path().join("lib/new.dart"), "new\n").expect("write lib new");
    std::fs::write(repo.path().join("test/new.dart"), "test new\n").expect("write test new");

    git_discard_area(
        path_str(repo.path()),
        GitChangeArea::Untracked,
        Some("lib".to_string()),
    )
    .unwrap();

    assert!(!repo.path().join("lib/new.dart").exists());
    assert!(repo.path().join("test/new.dart").exists());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("lib/tracked.dart")).unwrap(),
        "lib changed\n"
    );
}
