use super::*;

#[test]
fn creates_lists_and_removes_worktree() {
    let repo = init_repo();
    let worktree_base = tempfile::tempdir().expect("tempdir");
    let worktree_path = path_str(&worktree_base.path().join("feature"));

    create_worktree(
        path_str(repo.path()),
        "feature".to_string(),
        worktree_path.clone(),
        "main".to_string(),
        false,
    )
    .unwrap();

    assert!(branch_exists(path_str(repo.path()), "feature".to_string()).unwrap());
    let worktrees = list_worktrees(path_str(repo.path())).unwrap();
    assert!(worktrees.iter().any(|entry| entry.branch == "feature"));
    assert!(worktrees.iter().any(|entry| entry.branch == "main"));

    remove_worktree(path_str(repo.path()), worktree_path.clone(), true).unwrap();
    let worktrees = list_worktrees(path_str(repo.path())).unwrap();
    assert!(!worktrees.iter().any(|entry| entry.branch == "feature"));

    delete_branch(path_str(repo.path()), "feature".to_string(), true).unwrap();
    assert!(!branch_exists(path_str(repo.path()), "feature".to_string()).unwrap());
}

#[test]
fn creates_worktree_from_existing_branch() {
    let repo = init_repo();
    run_git(repo.path(), &["branch", "feature/existing"]);
    let worktree_base = tempfile::tempdir().expect("tempdir");
    let worktree_path = path_str(&worktree_base.path().join("existing"));

    create_worktree(
        path_str(repo.path()),
        "feature/existing".to_string(),
        worktree_path.clone(),
        "main".to_string(),
        true,
    )
    .unwrap();

    assert_eq!(current_branch(worktree_path).unwrap(), "feature/existing");
}

#[test]
fn rejects_missing_existing_branch_for_reused_worktree() {
    let repo = init_repo();
    let worktree_base = tempfile::tempdir().expect("tempdir");
    let worktree_path = path_str(&worktree_base.path().join("missing"));

    let error = create_worktree(
        path_str(repo.path()),
        "feature/missing".to_string(),
        worktree_path,
        "main".to_string(),
        true,
    )
    .unwrap_err();

    assert!(matches!(error.kind, GitErrorKind::BranchNotFound));
}

#[test]
fn removes_worktree_metadata_when_checkout_directory_is_missing() {
    let repo = init_repo();
    let worktree_base = tempfile::tempdir().expect("tempdir");
    let worktree_path = path_str(&worktree_base.path().join("feature-missing-dir"));

    create_worktree(
        path_str(repo.path()),
        "feature/missing-dir".to_string(),
        worktree_path.clone(),
        "main".to_string(),
        false,
    )
    .unwrap();
    std::fs::remove_dir_all(&worktree_path).expect("remove checkout dir");

    remove_worktree(path_str(repo.path()), worktree_path, true).unwrap();

    let worktrees = list_worktrees(path_str(repo.path())).unwrap();
    assert!(!worktrees
        .iter()
        .any(|entry| entry.branch == "feature/missing-dir"));
}

#[test]
fn rejects_duplicate_branch() {
    let repo = init_repo();
    run_git(repo.path(), &["branch", "feature"]);
    let worktree_base = tempfile::tempdir().expect("tempdir");
    let worktree_path = path_str(&worktree_base.path().join("dupe"));
    let error = create_worktree(
        path_str(repo.path()),
        "feature".to_string(),
        worktree_path,
        "main".to_string(),
        false,
    )
    .unwrap_err();
    assert!(matches!(error.kind, GitErrorKind::BranchAlreadyExists));
}

#[test]
fn creates_worktree_from_remote_tracking_branch() {
    let repo = init_repo();
    run_git(
        repo.path(),
        &["remote", "add", "origin", "https://example.com/repo.git"],
    );
    run_git(
        repo.path(),
        &["update-ref", "refs/remotes/origin/feature", "HEAD"],
    );

    let worktree_base = tempfile::tempdir().expect("tempdir");
    let worktree_path = path_str(&worktree_base.path().join("from-remote"));
    create_worktree(
        path_str(repo.path()),
        "local-feature".to_string(),
        worktree_path,
        "origin/feature".to_string(),
        false,
    )
    .unwrap();

    assert!(branch_exists(path_str(repo.path()), "local-feature".to_string()).unwrap());
    let repo_handle = Repository::open(repo.path()).unwrap();
    let config = repo_handle.config().unwrap();
    assert_eq!(
        config.get_string("branch.local-feature.remote").unwrap(),
        "origin"
    );
    assert_eq!(
        config.get_string("branch.local-feature.merge").unwrap(),
        "refs/heads/feature"
    );
}

#[test]
fn rolls_back_branch_when_worktree_fails() {
    let repo = init_repo();
    let worktree_base = tempfile::tempdir().expect("tempdir");
    let blocked = worktree_base.path().join("blocked");
    std::fs::create_dir_all(&blocked).expect("create blocker dir");
    std::fs::write(blocked.join("busy.txt"), "x").expect("write blocker");
    let worktree_path = path_str(&blocked);

    let error = create_worktree(
        path_str(repo.path()),
        "feature".to_string(),
        worktree_path.clone(),
        "main".to_string(),
        false,
    )
    .unwrap_err();
    assert!(!matches!(error.kind, GitErrorKind::BranchAlreadyExists));
    assert!(!branch_exists(path_str(repo.path()), "feature".to_string()).unwrap());

    std::fs::remove_dir_all(&worktree_path).expect("remove blocker dir");
    create_worktree(
        path_str(repo.path()),
        "feature".to_string(),
        worktree_path,
        "main".to_string(),
        false,
    )
    .unwrap();
    assert!(branch_exists(path_str(repo.path()), "feature".to_string()).unwrap());
}

#[test]
fn creates_same_basename_worktrees_under_different_parents() {
    let repo = init_repo();
    let parent_a = tempfile::tempdir().expect("tempdir");
    let parent_b = tempfile::tempdir().expect("tempdir");
    let path_a = path_str(&parent_a.path().join("shared"));
    let path_b = path_str(&parent_b.path().join("shared"));

    create_worktree(
        path_str(repo.path()),
        "feature-a".to_string(),
        path_a.clone(),
        "main".to_string(),
        false,
    )
    .unwrap();
    create_worktree(
        path_str(repo.path()),
        "feature-b".to_string(),
        path_b.clone(),
        "main".to_string(),
        false,
    )
    .unwrap();

    assert!(Path::new(&path_a).join(".git").exists());
    assert!(Path::new(&path_b).join(".git").exists());

    let worktrees = list_worktrees(path_str(repo.path())).unwrap();
    let target_a = canonical(&path_a);
    let target_b = canonical(&path_b);
    assert!(worktrees
        .iter()
        .any(|entry| canonical(&entry.path) == target_a && entry.branch == "feature-a"));
    assert!(worktrees
        .iter()
        .any(|entry| canonical(&entry.path) == target_b && entry.branch == "feature-b"));
}
