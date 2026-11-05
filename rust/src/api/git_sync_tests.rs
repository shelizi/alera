use super::*;

#[test]
fn git_push_sets_origin_upstream_when_missing() {
    let repo = init_repo();
    let bare = tempfile::tempdir().expect("bare remote");
    run_git(bare.path(), &["init", "--bare"]);
    run_git(
        repo.path(),
        &["remote", "add", "origin", &path_str(bare.path())],
    );
    std::fs::write(repo.path().join("README.md"), "pushed\n").expect("modify readme");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "pushed"]);

    git_push(path_str(repo.path())).unwrap();

    let state = git_repository_state(path_str(repo.path())).unwrap();
    assert_eq!(state.upstream.as_deref(), Some("origin/main"));
    assert_eq!(state.ahead, 0);
}

#[test]
fn git_pull_respects_configured_rebase_strategy() {
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

    std::fs::write(source.path().join("remote.txt"), "remote\n").expect("write remote file");
    run_git(source.path(), &["add", "remote.txt"]);
    run_git(source.path(), &["commit", "-m", "remote change"]);
    run_git(source.path(), &["push"]);

    std::fs::write(clone_path.join("local.txt"), "local\n").expect("write local file");
    run_git(&clone_path, &["add", "local.txt"]);
    run_git(&clone_path, &["commit", "-m", "local change"]);
    run_git(&clone_path, &["config", "pull.rebase", "true"]);

    git_pull(path_str(&clone_path)).unwrap();

    let output = git_command(&clone_path, &["rev-list", "--parents", "-n", "1", "HEAD"])
        .output()
        .expect("rev-list runs");
    assert!(output.status.success());
    let parents = String::from_utf8_lossy(&output.stdout);
    assert_eq!(parents.split_whitespace().count(), 2);
    let subject = git_command(&clone_path, &["log", "-1", "--format=%s"])
        .output()
        .expect("log runs");
    assert!(subject.status.success());
    assert_eq!(
        String::from_utf8_lossy(&subject.stdout).trim(),
        "local change"
    );
}

#[test]
fn selected_branch_push_does_not_checkout_or_push_current_branch() {
    let source = init_repo();
    let bare = tempfile::tempdir().expect("bare remote");
    run_git(bare.path(), &["init", "--bare"]);
    run_git(
        source.path(),
        &["remote", "add", "origin", &path_str(bare.path())],
    );
    run_git(source.path(), &["push", "-u", "origin", "main"]);
    run_git(source.path(), &["checkout", "-b", "feature"]);
    commit_file(
        source.path(),
        "feature.txt",
        "version 1\n",
        "feature commit",
    );
    let feature_oid = branch_oid(source.path(), "feature");
    run_git(source.path(), &["checkout", "main"]);

    git_push_branch(path_str(source.path()), "feature".to_owned()).unwrap();

    assert_eq!(current_branch(path_str(source.path())).unwrap(), "main");
    let remote_feature = git_command(bare.path(), &["rev-parse", "refs/heads/feature"])
        .output()
        .expect("remote feature lookup");
    assert!(remote_feature.status.success());
    assert_eq!(
        String::from_utf8_lossy(&remote_feature.stdout).trim(),
        feature_oid.to_string()
    );
    assert_eq!(
        git_command(
            source.path(),
            &["rev-parse", "--abbrev-ref", "feature@{upstream}"]
        )
        .output()
        .unwrap()
        .status
        .success(),
        true
    );
}

#[test]
fn selected_branch_pull_fast_forwards_non_current_without_checkout() {
    let source = init_repo();
    let bare = tempfile::tempdir().expect("bare remote");
    run_git(bare.path(), &["init", "--bare"]);
    run_git(
        source.path(),
        &["remote", "add", "origin", &path_str(bare.path())],
    );
    run_git(source.path(), &["push", "-u", "origin", "main"]);
    run_git(source.path(), &["checkout", "-b", "feature"]);
    commit_file(source.path(), "feature.txt", "old\n", "feature start");
    run_git(source.path(), &["push", "-u", "origin", "feature"]);
    run_git(source.path(), &["checkout", "main"]);

    let clone_parent = tempfile::tempdir().expect("clone parent");
    run_git(
        clone_parent.path(),
        &["clone", &path_str(bare.path()), "clone"],
    );
    let clone = clone_parent.path().join("clone");
    configure_git_identity(&clone);
    run_git(&clone, &["checkout", "-b", "feature", "origin/feature"]);
    commit_file(&clone, "feature.txt", "new\n", "feature advance");
    run_git(&clone, &["push", "origin", "feature"]);

    git_pull_branch(path_str(source.path()), "feature".to_owned()).unwrap();
    assert_eq!(current_branch(path_str(source.path())).unwrap(), "main");
    assert_eq!(
        branch_oid(source.path(), "feature"),
        branch_oid(&clone, "feature")
    );
}
