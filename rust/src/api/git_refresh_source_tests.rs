use super::*;

#[test]
fn refresh_source_branch_fetches_remote_tracking_source() {
    let source = init_repo();
    let bare = tempfile::tempdir().expect("bare remote");
    run_git(bare.path(), &["init", "--bare"]);
    run_git(
        source.path(),
        &["remote", "add", "origin", &path_str(bare.path())],
    );
    run_git(source.path(), &["push", "-u", "origin", "main"]);
    run_git(bare.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let clone_parent = tempfile::tempdir().expect("clone parent");
    run_git(
        clone_parent.path(),
        &["clone", &path_str(bare.path()), "checkout"],
    );
    let clone_path = clone_parent.path().join("checkout");
    configure_git_identity(&clone_path);

    commit_file(source.path(), "remote.txt", "remote\n", "remote change");
    run_git(source.path(), &["push"]);

    refresh_source_branch(path_str(&clone_path), "origin/main".to_string()).unwrap();

    let worktree_base = tempfile::tempdir().expect("worktree base");
    let worktree_path = worktree_base.path().join("from-remote");
    create_worktree(
        path_str(&clone_path),
        "feature/from-remote".to_string(),
        path_str(&worktree_path),
        "origin/main".to_string(),
        false,
    )
    .unwrap();
    assert!(worktree_path.join("remote.txt").exists());
}

#[test]
fn refresh_source_branch_pulls_current_local_source() {
    let source = init_repo();
    let bare = tempfile::tempdir().expect("bare remote");
    run_git(bare.path(), &["init", "--bare"]);
    run_git(
        source.path(),
        &["remote", "add", "origin", &path_str(bare.path())],
    );
    run_git(source.path(), &["push", "-u", "origin", "main"]);
    run_git(bare.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let clone_parent = tempfile::tempdir().expect("clone parent");
    run_git(
        clone_parent.path(),
        &["clone", &path_str(bare.path()), "checkout"],
    );
    let clone_path = clone_parent.path().join("checkout");
    configure_git_identity(&clone_path);

    commit_file(source.path(), "pulled.txt", "pulled\n", "remote change");
    run_git(source.path(), &["push"]);

    refresh_source_branch(path_str(&clone_path), "main".to_string()).unwrap();

    assert!(clone_path.join("pulled.txt").exists());
}

#[test]
fn refresh_source_branch_rejects_diverged_current_local_source() {
    let source = init_repo();
    let bare = tempfile::tempdir().expect("bare remote");
    run_git(bare.path(), &["init", "--bare"]);
    run_git(
        source.path(),
        &["remote", "add", "origin", &path_str(bare.path())],
    );
    run_git(source.path(), &["push", "-u", "origin", "main"]);
    run_git(bare.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let clone_parent = tempfile::tempdir().expect("clone parent");
    run_git(
        clone_parent.path(),
        &["clone", &path_str(bare.path()), "checkout"],
    );
    let clone_path = clone_parent.path().join("checkout");
    configure_git_identity(&clone_path);
    run_git(&clone_path, &["config", "pull.rebase", "false"]);
    run_git(&clone_path, &["config", "pull.ff", "false"]);

    commit_file(
        &clone_path,
        "local-main.txt",
        "local\n",
        "local main change",
    );
    let local_oid = branch_oid(&clone_path, "main");

    commit_file(
        source.path(),
        "remote-main.txt",
        "remote\n",
        "remote main change",
    );
    run_git(source.path(), &["push"]);

    let error = refresh_source_branch(path_str(&clone_path), "main".to_string()).unwrap_err();

    assert!(matches!(error.kind, GitErrorKind::GitCli));
    assert_eq!(branch_oid(&clone_path, "main"), local_oid);
    assert_eq!(current_branch(path_str(&clone_path)).unwrap(), "main");
}

#[test]
fn refresh_source_branch_fast_forwards_non_current_local_source() {
    let source = init_repo();
    let bare = tempfile::tempdir().expect("bare remote");
    run_git(bare.path(), &["init", "--bare"]);
    run_git(
        source.path(),
        &["remote", "add", "origin", &path_str(bare.path())],
    );
    run_git(source.path(), &["push", "-u", "origin", "main"]);
    run_git(bare.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    run_git(source.path(), &["checkout", "-b", "feature"]);
    commit_file(source.path(), "feature.txt", "v1\n", "feature v1");
    run_git(source.path(), &["push", "-u", "origin", "feature"]);
    run_git(source.path(), &["checkout", "main"]);

    let clone_parent = tempfile::tempdir().expect("clone parent");
    run_git(
        clone_parent.path(),
        &["clone", &path_str(bare.path()), "checkout"],
    );
    let clone_path = clone_parent.path().join("checkout");
    configure_git_identity(&clone_path);
    run_git(
        &clone_path,
        &["checkout", "-b", "feature", "origin/feature"],
    );
    run_git(&clone_path, &["checkout", "main"]);

    run_git(source.path(), &["checkout", "feature"]);
    commit_file(source.path(), "feature.txt", "v2\n", "feature v2");
    run_git(source.path(), &["push"]);

    refresh_source_branch(path_str(&clone_path), "feature".to_string()).unwrap();

    assert_eq!(current_branch(path_str(&clone_path)).unwrap(), "main");
    let worktree_base = tempfile::tempdir().expect("worktree base");
    let worktree_path = worktree_base.path().join("from-local");
    create_worktree(
        path_str(&clone_path),
        "from-local-feature".to_string(),
        path_str(&worktree_path),
        "feature".to_string(),
        false,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(worktree_path.join("feature.txt")).unwrap(),
        "v2\n"
    );
}

#[test]
fn refresh_source_branch_rejects_diverged_non_current_local_source() {
    let source = init_repo();
    let bare = tempfile::tempdir().expect("bare remote");
    run_git(bare.path(), &["init", "--bare"]);
    run_git(
        source.path(),
        &["remote", "add", "origin", &path_str(bare.path())],
    );
    run_git(source.path(), &["push", "-u", "origin", "main"]);
    run_git(bare.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    run_git(source.path(), &["checkout", "-b", "feature"]);
    commit_file(source.path(), "feature.txt", "v1\n", "feature v1");
    run_git(source.path(), &["push", "-u", "origin", "feature"]);
    run_git(source.path(), &["checkout", "main"]);

    let clone_parent = tempfile::tempdir().expect("clone parent");
    run_git(
        clone_parent.path(),
        &["clone", &path_str(bare.path()), "checkout"],
    );
    let clone_path = clone_parent.path().join("checkout");
    configure_git_identity(&clone_path);
    run_git(
        &clone_path,
        &["checkout", "-b", "feature", "origin/feature"],
    );
    commit_file(
        &clone_path,
        "local-feature.txt",
        "local\n",
        "local feature change",
    );
    let local_oid = branch_oid(&clone_path, "feature");
    run_git(&clone_path, &["checkout", "main"]);

    run_git(source.path(), &["checkout", "feature"]);
    commit_file(
        source.path(),
        "remote-feature.txt",
        "remote\n",
        "remote feature change",
    );
    run_git(source.path(), &["push"]);

    let error = refresh_source_branch(path_str(&clone_path), "feature".to_string()).unwrap_err();

    assert!(matches!(error.kind, GitErrorKind::Conflict));
    assert_eq!(branch_oid(&clone_path, "feature"), local_oid);
    assert_eq!(current_branch(path_str(&clone_path)).unwrap(), "main");
}
