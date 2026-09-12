use super::*;

#[test]
fn git_commit_rejects_empty_index() {
    let repo = init_repo();
    let error = git_commit(path_str(repo.path()), "empty".to_string()).unwrap_err();
    assert_eq!(error.kind, GitErrorKind::NothingToCommit);
}

#[test]
fn git_commit_rejects_missing_identity() {
    let repo = init_repo();
    run_git(repo.path(), &["config", "user.name", ""]);
    run_git(repo.path(), &["config", "user.email", ""]);
    std::fs::write(repo.path().join("README.md"), "identity missing\n").expect("modify readme");
    run_git(repo.path(), &["add", "README.md"]);

    let error = git_commit(path_str(repo.path()), "update readme".to_string()).unwrap_err();

    assert_eq!(error.kind, GitErrorKind::MissingIdentity);
    assert!(error.context.contains("user.name"));
    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "README.md" && entry.area == GitChangeArea::Staged));
}

#[test]
fn git_commit_rejects_staged_changes_outside_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(repo.path().join("root.txt"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add workspace files"]);

    std::fs::write(repo.path().join("root.txt"), "root changed\n").expect("modify root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");
    run_git(
        repo.path(),
        &["add", "root.txt", "packages/app/lib/foo.dart"],
    );

    let error = git_commit(path_str(&app_dir), "workspace commit".to_string()).unwrap_err();
    assert_eq!(error.kind, GitErrorKind::WorkspaceScope);

    let state = git_repository_state(path_str(repo.path())).unwrap();
    assert_eq!(state.ahead, 0);
    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "root.txt" && entry.area == GitChangeArea::Staged));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app/lib/foo.dart" && entry.area == GitChangeArea::Staged
    }));
}

#[test]
fn git_commit_allows_subdirectory_workspace_when_staged_changes_are_scoped() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app"]);

    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");
    run_git(repo.path(), &["add", "packages/app/lib/foo.dart"]);

    let oid = git_commit(path_str(&app_dir), "workspace commit".to_string()).unwrap();
    assert!(!oid.is_empty());
    assert!(git_status(path_str(&app_dir)).unwrap().entries.is_empty());
}

#[test]
fn git_commit_amend_uses_staged_tree_and_edited_message() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "amended\n").expect("modify readme");
    run_git(repo.path(), &["add", "README.md"]);

    let oid = git_commit_amend(path_str(repo.path()), "amended message".to_string()).unwrap();

    assert!(!oid.is_empty());
    assert_eq!(head_commit_message(repo.path()), "amended message");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("README.md")).unwrap(),
        "amended\n"
    );
    assert!(git_status(path_str(repo.path()))
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn git_commit_amend_preserves_author_and_uses_configured_committer() {
    let repo = init_repo();
    run_git(repo.path(), &["config", "user.name", "Amender"]);
    run_git(
        repo.path(),
        &["config", "user.email", "amender@example.com"],
    );
    std::fs::write(repo.path().join("README.md"), "amended\n").expect("modify readme");
    run_git(repo.path(), &["add", "README.md"]);

    git_commit_amend(path_str(repo.path()), "amended message".to_string()).unwrap();

    assert_eq!(head_author_name(repo.path()), "Test");
    assert_eq!(head_committer_name(repo.path()), "Amender");
}

#[test]
fn git_commit_amend_rejects_empty_message() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "amended\n").expect("modify readme");
    run_git(repo.path(), &["add", "README.md"]);

    let error = git_commit_amend(path_str(repo.path()), "   ".to_string()).unwrap_err();

    assert_eq!(error.kind, GitErrorKind::NothingToCommit);
}

#[test]
fn git_commit_amend_rejects_missing_identity() {
    let repo = init_repo();
    run_git(repo.path(), &["config", "user.name", ""]);
    run_git(repo.path(), &["config", "user.email", ""]);
    std::fs::write(repo.path().join("README.md"), "amended\n").expect("modify readme");
    run_git(repo.path(), &["add", "README.md"]);

    let error = git_commit_amend(path_str(repo.path()), "amended message".to_string()).unwrap_err();

    assert_eq!(error.kind, GitErrorKind::MissingIdentity);
    assert!(error.context.contains("user.name"));
}

#[test]
fn git_commit_amend_rejects_no_staged_changes() {
    let repo = init_repo();

    let error = git_commit_amend(path_str(repo.path()), "amended message".to_string()).unwrap_err();

    assert_eq!(error.kind, GitErrorKind::NothingToCommit);
}

#[test]
fn git_commit_amend_rejects_staged_changes_outside_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(repo.path().join("root.txt"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add workspace files"]);

    std::fs::write(repo.path().join("root.txt"), "root changed\n").expect("modify root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");
    run_git(
        repo.path(),
        &["add", "root.txt", "packages/app/lib/foo.dart"],
    );

    let error = git_commit_amend(path_str(&app_dir), "amend workspace".to_string()).unwrap_err();

    assert_eq!(error.kind, GitErrorKind::WorkspaceScope);
}

#[test]
fn git_commit_amend_rejects_unresolved_merge_conflicts() {
    let repo = init_repo();
    run_git(repo.path(), &["checkout", "-b", "feature"]);
    std::fs::write(repo.path().join("README.md"), "feature\n").expect("write feature change");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "feature change"]);
    run_git(repo.path(), &["checkout", "main"]);
    std::fs::write(repo.path().join("README.md"), "main\n").expect("write main change");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "main change"]);
    run_git_expect_failure(repo.path(), &["merge", "feature"]);

    let error = git_commit_amend(path_str(repo.path()), "amended message".to_string()).unwrap_err();

    assert_eq!(error.kind, GitErrorKind::Conflict);
}

#[test]
fn git_commit_rejects_unresolved_merge_conflicts() {
    let repo = init_repo();
    run_git(repo.path(), &["checkout", "-b", "feature"]);
    std::fs::write(repo.path().join("README.md"), "feature\n").expect("write feature change");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "feature change"]);
    run_git(repo.path(), &["checkout", "main"]);
    std::fs::write(repo.path().join("README.md"), "main\n").expect("write main change");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "main change"]);
    run_git_expect_failure(repo.path(), &["merge", "feature"]);

    let error = git_commit(path_str(repo.path()), "merge commit".to_string()).unwrap_err();
    assert_eq!(error.kind, GitErrorKind::Conflict);
}

#[test]
fn git_commit_allows_resolved_merge_state() {
    let repo = init_repo();
    run_git(repo.path(), &["checkout", "-b", "feature"]);
    std::fs::write(repo.path().join("README.md"), "feature\n").expect("write feature change");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "feature change"]);
    run_git(repo.path(), &["checkout", "main"]);
    std::fs::write(repo.path().join("README.md"), "main\n").expect("write main change");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "main change"]);
    run_git_expect_failure(repo.path(), &["merge", "feature"]);

    std::fs::write(repo.path().join("README.md"), "resolved\n").expect("resolve readme");
    run_git(repo.path(), &["add", "README.md"]);

    let oid = git_commit(path_str(repo.path()), "merge commit".to_string()).unwrap();

    assert!(!oid.is_empty());
    let repository = Repository::discover(repo.path()).expect("open repo");
    assert_eq!(repository.state(), RepositoryState::Clean);
    assert!(git_status(path_str(repo.path()))
        .unwrap()
        .entries
        .is_empty());
}
