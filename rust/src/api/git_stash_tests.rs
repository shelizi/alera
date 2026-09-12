use super::*;

#[test]
fn git_stash_lists_and_pops_tracked_changes() {
    let repo = init_repo();
    let tracked = repo.path().join("README.md");
    std::fs::write(&tracked, "stashed\n").expect("modify readme");
    std::fs::write(repo.path().join("untracked.txt"), "left alone\n").expect("write untracked");

    git_stash(path_str(repo.path())).unwrap();
    assert_eq!(std::fs::read_to_string(&tracked).unwrap(), "hello");
    assert!(repo.path().join("untracked.txt").exists());

    let stashes = git_list_stashes(path_str(repo.path())).unwrap();
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].reference, "stash@{0}");

    git_stash_pop(path_str(repo.path()), stashes[0].index).unwrap();
    assert_eq!(std::fs::read_to_string(&tracked).unwrap(), "stashed\n");
    assert!(git_list_stashes(path_str(repo.path())).unwrap().is_empty());
}

#[test]
fn git_stash_rejects_tracked_changes_outside_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(repo.path().join("root.txt"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add stash files"]);

    std::fs::write(repo.path().join("root.txt"), "root changed\n").expect("modify root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");

    let error = git_stash(path_str(&app_dir)).unwrap_err();
    assert_eq!(error.kind, GitErrorKind::WorkspaceScope);

    assert_eq!(
        std::fs::read_to_string(repo.path().join("root.txt")).unwrap(),
        "root changed\n"
    );
    assert_eq!(
        std::fs::read_to_string(app_lib_dir.join("foo.dart")).unwrap(),
        "app changed\n"
    );
    assert!(git_list_stashes(path_str(repo.path())).unwrap().is_empty());
}

#[test]
fn git_stash_allows_subdirectory_workspace_when_tracked_changes_are_scoped() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(repo.path().join("root.txt"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add stash files"]);

    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");

    git_stash(path_str(&app_dir)).unwrap();

    assert_eq!(
        std::fs::read_to_string(repo.path().join("root.txt")).unwrap(),
        "root\n"
    );
    assert_eq!(
        std::fs::read_to_string(app_lib_dir.join("foo.dart")).unwrap(),
        "app\n"
    );
    assert_eq!(git_list_stashes(path_str(repo.path())).unwrap().len(), 1);
    assert!(git_status(path_str(repo.path()))
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn git_stash_pop_rejects_stashes_with_changes_outside_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(repo.path().join("root.txt"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add stash pop files"]);

    std::fs::write(repo.path().join("root.txt"), "root changed\n").expect("modify root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");
    run_git(repo.path(), &["stash", "push"]);

    let error = git_stash_pop(path_str(&app_dir), 0).unwrap_err();
    assert_eq!(error.kind, GitErrorKind::WorkspaceScope);

    assert_eq!(
        std::fs::read_to_string(repo.path().join("root.txt")).unwrap(),
        "root\n"
    );
    assert_eq!(
        std::fs::read_to_string(app_lib_dir.join("foo.dart")).unwrap(),
        "app\n"
    );
    assert_eq!(git_list_stashes(path_str(repo.path())).unwrap().len(), 1);
}

#[test]
fn git_stash_pop_allows_subdirectory_workspace_when_stash_is_scoped() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(repo.path().join("root.txt"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add scoped stash pop files"]);

    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");
    run_git(repo.path(), &["stash", "push"]);

    git_stash_pop(path_str(&app_dir), 0).unwrap();

    assert_eq!(
        std::fs::read_to_string(repo.path().join("root.txt")).unwrap(),
        "root\n"
    );
    assert_eq!(
        std::fs::read_to_string(app_lib_dir.join("foo.dart")).unwrap(),
        "app changed\n"
    );
    assert!(git_list_stashes(path_str(repo.path())).unwrap().is_empty());
}
