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
