use super::*;

#[test]
fn git_stage_unstage_commit_and_state_follow_index() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "hello\nupdated\n").expect("modify readme");

    git_stage(path_str(repo.path()), Some("README.md".to_string())).unwrap();
    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "README.md" && entry.area == GitChangeArea::Staged));

    git_unstage(path_str(repo.path()), Some("README.md".to_string())).unwrap();
    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "README.md" && entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "README.md" && entry.area == GitChangeArea::Staged));

    git_stage(path_str(repo.path()), Some("README.md".to_string())).unwrap();
    let oid = git_commit(path_str(repo.path()), "update readme".to_string()).unwrap();
    assert!(!oid.is_empty());
    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.is_empty());

    let state = git_repository_state(path_str(repo.path())).unwrap();
    assert_eq!(state.branch, "main");
    assert!(!state.has_conflicts);
}

#[test]
fn git_unstage_treats_selected_pathspec_characters_as_literals() {
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
    run_git(repo.path(), &["add", "file[1].txt", "file1.txt"]);

    git_unstage(path_str(repo.path()), Some("file[1].txt".to_string())).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "file[1].txt" && entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "file[1].txt" && entry.area == GitChangeArea::Staged));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "file1.txt" && entry.area == GitChangeArea::Staged));
}

#[test]
fn git_stage_all_treats_workspace_pathspec_characters_as_literals() {
    let repo = init_repo();
    let bracket_dir = repo.path().join("packages").join("app[1]");
    let glob_match_dir = repo.path().join("packages").join("app1");
    std::fs::create_dir_all(&bracket_dir).expect("create bracket dir");
    std::fs::create_dir_all(&glob_match_dir).expect("create glob match dir");
    std::fs::write(bracket_dir.join("foo.dart"), "bracket\n").expect("write bracket file");
    std::fs::write(glob_match_dir.join("foo.dart"), "glob\n").expect("write glob file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app dirs"]);

    std::fs::write(bracket_dir.join("foo.dart"), "bracket changed\n").expect("modify bracket file");
    std::fs::write(glob_match_dir.join("foo.dart"), "glob changed\n").expect("modify glob file");

    git_stage(path_str(&bracket_dir), None).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app[1]/foo.dart" && entry.area == GitChangeArea::Staged
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app1/foo.dart" && entry.area == GitChangeArea::Unstaged
    }));
    assert!(!status.entries.iter().any(|entry| {
        entry.path == "packages/app1/foo.dart" && entry.area == GitChangeArea::Staged
    }));
}

#[test]
fn git_stage_area_limits_to_selected_area_and_folder() {
    let repo = init_repo();
    std::fs::create_dir_all(repo.path().join("lib")).expect("create lib");
    std::fs::create_dir_all(repo.path().join("test")).expect("create test");
    std::fs::write(repo.path().join("lib/tracked.dart"), "lib\n").expect("write lib tracked");
    std::fs::write(repo.path().join("test/tracked.dart"), "test\n").expect("write test tracked");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add tracked files"]);

    std::fs::write(repo.path().join("lib/tracked.dart"), "lib changed\n")
        .expect("modify lib tracked");
    std::fs::write(repo.path().join("lib/new.dart"), "new\n").expect("write lib new");
    std::fs::write(repo.path().join("test/tracked.dart"), "test changed\n")
        .expect("modify test tracked");

    git_stage_area(
        path_str(repo.path()),
        GitChangeArea::Unstaged,
        Some("lib".to_string()),
    )
    .unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| { entry.path == "lib/tracked.dart" && entry.area == GitChangeArea::Staged }));
    assert!(status
        .entries
        .iter()
        .any(|entry| { entry.path == "lib/new.dart" && entry.area == GitChangeArea::Untracked }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "test/tracked.dart" && entry.area == GitChangeArea::Unstaged
    }));
}

#[test]
fn git_unstage_all_treats_workspace_pathspec_characters_as_literals() {
    let repo = init_repo();
    let bracket_dir = repo.path().join("packages").join("app[1]");
    let glob_match_dir = repo.path().join("packages").join("app1");
    std::fs::create_dir_all(&bracket_dir).expect("create bracket dir");
    std::fs::create_dir_all(&glob_match_dir).expect("create glob match dir");
    std::fs::write(bracket_dir.join("foo.dart"), "bracket\n").expect("write bracket file");
    std::fs::write(glob_match_dir.join("foo.dart"), "glob\n").expect("write glob file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app dirs"]);

    std::fs::write(bracket_dir.join("foo.dart"), "bracket changed\n").expect("modify bracket file");
    std::fs::write(glob_match_dir.join("foo.dart"), "glob changed\n").expect("modify glob file");
    run_git(
        repo.path(),
        &["add", "packages/app[1]/foo.dart", "packages/app1/foo.dart"],
    );

    git_unstage(path_str(&bracket_dir), None).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app[1]/foo.dart" && entry.area == GitChangeArea::Unstaged
    }));
    assert!(!status.entries.iter().any(|entry| {
        entry.path == "packages/app[1]/foo.dart" && entry.area == GitChangeArea::Staged
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app1/foo.dart" && entry.area == GitChangeArea::Staged
    }));
}

#[test]
fn git_stage_selected_path_uses_repo_relative_index_path_from_subdirectory() {
    let repo = init_repo();
    let root_lib_dir = repo.path().join("lib");
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&root_lib_dir).expect("create root lib dir");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(root_lib_dir.join("foo.dart"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add colliding paths"]);

    std::fs::write(root_lib_dir.join("foo.dart"), "root changed\n").expect("modify root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");

    git_stage(path_str(&app_dir), Some("lib/foo.dart".to_string())).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Staged));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "lib/foo.dart" && entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "lib/foo.dart" && entry.area == GitChangeArea::Staged));
}

#[test]
fn git_stage_selected_path_handles_rename_out_from_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    let other_dir = repo.path().join("packages").join("other");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::create_dir_all(&other_dir).expect("create other dir");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app file"]);

    std::fs::rename(app_lib_dir.join("foo.dart"), other_dir.join("foo.dart"))
        .expect("rename file out of workspace");

    git_stage(path_str(&app_dir), Some("lib/foo.dart".to_string())).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Staged
            && entry.status == GitChangeStatus::Deleted
    }));
    assert!(!status.entries.iter().any(|entry| {
        entry.path == "packages/other/foo.dart" && entry.area == GitChangeArea::Staged
    }));
}

#[test]
fn git_stage_all_handles_rename_out_from_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    let other_dir = repo.path().join("packages").join("other");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::create_dir_all(&other_dir).expect("create other dir");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app file"]);

    std::fs::rename(app_lib_dir.join("foo.dart"), other_dir.join("foo.dart"))
        .expect("rename file out of workspace");

    git_stage(path_str(&app_dir), None).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Staged
            && entry.status == GitChangeStatus::Deleted
    }));
    assert!(!status.entries.iter().any(|entry| {
        entry.path == "packages/other/foo.dart" && entry.area == GitChangeArea::Staged
    }));
}

#[test]
fn git_unstage_selected_path_uses_repo_relative_index_path_from_subdirectory() {
    let repo = init_repo();
    let root_lib_dir = repo.path().join("lib");
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&root_lib_dir).expect("create root lib dir");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(root_lib_dir.join("foo.dart"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add colliding paths"]);

    std::fs::write(root_lib_dir.join("foo.dart"), "root changed\n").expect("modify root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");
    run_git(
        repo.path(),
        &["add", "lib/foo.dart", "packages/app/lib/foo.dart"],
    );

    git_unstage(path_str(&app_dir), Some("lib/foo.dart".to_string())).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Staged));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "lib/foo.dart" && entry.area == GitChangeArea::Staged));
}
