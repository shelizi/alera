use std::fs;

use git2::{Repository, RepositoryState, ResetType};
use tempfile::TempDir;

use super::{
    checkout_remote_branch, cherry_pick_commit, create_archive, create_branch_at_commit,
    create_tag, delete_remote_branch, delete_tag, drop_commit, git_in_dir, merge_ref, push_tag,
    rebase_onto, rename_branch, GitArchiveFormat, GitErrorKind,
};

#[test]
fn creates_branch_at_commit_and_checks_it_out() {
    let directory = init_repo();
    let root = head_oid(&directory);
    let branch = "release/candidate";

    create_branch_at_commit(path_str(&directory), &root.to_string(), branch, true)
        .expect("create branch at commit");

    let repo = Repository::open(directory.path()).expect("open repository");
    assert_eq!(head_branch(&repo), branch);
    assert_eq!(branch_oid(&repo, branch), root);
}

#[test]
fn cherry_pick_commits_and_aborts_conflicts() {
    let success = init_repo();
    run_git(&success, &["checkout", "-b", "source"]);
    let source_oid = commit_file(&success, "source.txt", "source\n", "source change");
    run_git(&success, &["checkout", "main"]);
    let base_oid = head_oid(&success);
    let picked_oid = cherry_pick_commit(path_str(&success), &source_oid.to_string(), None)
        .expect("cherry-pick non-conflicting commit");
    let picked_oid = git2::Oid::from_str(&picked_oid).expect("picked oid");
    assert_eq!(parent_oid(&success, picked_oid), base_oid);
    assert_eq!(read_file(&success, "source.txt"), "source\n");

    let conflict = init_repo();
    run_git(&conflict, &["checkout", "-b", "source"]);
    let source_oid = commit_file(&conflict, "README.md", "source\n", "source change");
    run_git(&conflict, &["checkout", "main"]);
    let main_oid = commit_file(&conflict, "README.md", "main\n", "main change");

    let error = cherry_pick_commit(path_str(&conflict), &source_oid.to_string(), None)
        .expect_err("conflicting cherry-pick");
    assert_eq!(error.kind, GitErrorKind::Conflict);
    let repo = Repository::open(conflict.path()).expect("reopen repository");
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert_eq!(repo.head().expect("read HEAD").target(), Some(main_oid));
    assert_eq!(read_file(&conflict, "README.md"), "main\n");
}

#[test]
fn drop_replays_descendants_and_rejects_root_commit() {
    let directory = init_repo();
    let root = head_oid(&directory);
    let target_oid = commit_file(&directory, "README.md", "target\n", "target change");
    let descendant_oid = commit_file(
        &directory,
        "descendant.txt",
        "descendant\n",
        "descendant change",
    );

    drop_commit(path_str(&directory), &target_oid.to_string()).expect("drop target commit");
    let repo = Repository::open(directory.path()).expect("reopen repository");
    let new_head = head_oid(&directory);
    assert_ne!(new_head, target_oid);
    assert_eq!(parent_oid(&directory, new_head), root);
    assert_eq!(commit_message(&repo, new_head), "descendant change\n");
    assert_eq!(read_file(&directory, "README.md"), "initial\n");
    assert_eq!(read_file(&directory, "descendant.txt"), "descendant\n");
    assert_ne!(new_head, descendant_oid);

    let error = drop_commit(path_str(&directory), &root.to_string())
        .expect_err("root commit cannot be dropped");
    assert_eq!(error.kind, GitErrorKind::Internal);
    assert!(error.context.contains("root"));

    let head_drop = init_repo();
    let parent = head_oid(&head_drop);
    let tip = commit_file(&head_drop, "head.txt", "head\n", "head change");
    drop_commit(path_str(&head_drop), &tip.to_string()).expect("drop HEAD commit");
    assert_eq!(head_oid(&head_drop), parent);
    assert!(!head_drop.path().join("head.txt").exists());
}

#[test]
fn merge_supports_fast_forward_and_merge_commit() {
    let fast_forward = init_repo();
    run_git(&fast_forward, &["checkout", "-b", "feature"]);
    let feature_oid = commit_file(&fast_forward, "feature.txt", "feature\n", "feature");
    run_git(&fast_forward, &["checkout", "main"]);
    assert_eq!(
        merge_ref(path_str(&fast_forward), "feature").expect("fast-forward merge"),
        None
    );
    assert_eq!(head_oid(&fast_forward), feature_oid);

    let normal = init_repo();
    let root = head_oid(&normal);
    run_git(&normal, &["checkout", "-b", "feature"]);
    let feature_oid = commit_file(&normal, "feature.txt", "feature\n", "feature");
    run_git(&normal, &["checkout", "main"]);
    let main_oid = commit_file(&normal, "main.txt", "main\n", "main");
    let merge_oid = merge_ref(path_str(&normal), "feature")
        .expect("normal merge")
        .expect("merge commit oid");
    let merge_oid = git2::Oid::from_str(&merge_oid).expect("merge oid");
    let repo = Repository::open(normal.path()).expect("reopen repository");
    let merge = repo.find_commit(merge_oid).expect("find merge commit");
    assert_eq!(merge.parent_count(), 2);
    assert_eq!(merge.parent_id(0).expect("first parent"), main_oid);
    assert_eq!(merge.parent_id(1).expect("second parent"), feature_oid);
    assert_eq!(parent_oid(&normal, merge_oid), main_oid);
    assert_ne!(root, merge_oid);
}

#[test]
fn merge_conflict_is_left_for_resolution_and_blocks_non_clean_operations() {
    let directory = init_repo();
    let root = head_oid(&directory);
    run_git(&directory, &["checkout", "-b", "feature"]);
    let feature_oid = commit_file(&directory, "README.md", "feature\n", "feature");
    run_git(&directory, &["checkout", "main"]);
    let main_oid = commit_file(&directory, "README.md", "main\n", "main");

    let error = merge_ref(path_str(&directory), "feature").expect_err("merge conflict");
    assert_eq!(error.kind, GitErrorKind::Conflict);
    let repo = Repository::open(directory.path()).expect("reopen conflicted repository");
    assert_eq!(repo.state(), RepositoryState::Merge);
    assert!(repo.index().expect("open conflict index").has_conflicts());
    let non_clean =
        rebase_onto(path_str(&directory), "main").expect_err("in-progress merge blocks rebase");
    assert_eq!(non_clean.kind, GitErrorKind::Conflict);
    let non_clean = drop_commit(path_str(&directory), &root.to_string())
        .expect_err("in-progress merge blocks drop");
    assert_eq!(non_clean.kind, GitErrorKind::Conflict);

    cleanup_merge(&directory, main_oid);
    let repo = Repository::open(directory.path()).expect("reopen cleaned repository");
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert_eq!(head_oid(&directory), main_oid);
    assert_ne!(feature_oid, main_oid);
}

#[test]
fn rebase_onto_replays_commits_and_aborts_conflicts() {
    let success = init_repo();
    let root = head_oid(&success);
    run_git(&success, &["checkout", "-b", "feature"]);
    let feature_oid = commit_file(&success, "feature.txt", "feature\n", "feature");
    run_git(&success, &["checkout", "main"]);
    let main_oid = commit_file(&success, "main.txt", "main\n", "main");
    run_git(&success, &["checkout", "feature"]);

    rebase_onto(path_str(&success), "main").expect("rebase feature onto main");
    let rebased_head = head_oid(&success);
    assert_ne!(rebased_head, feature_oid);
    assert_eq!(parent_oid(&success, rebased_head), main_oid);
    assert_eq!(read_file(&success, "feature.txt"), "feature\n");
    assert_ne!(root, rebased_head);

    let conflict = init_repo();
    run_git(&conflict, &["checkout", "-b", "feature"]);
    let feature_oid = commit_file(&conflict, "README.md", "feature\n", "feature");
    run_git(&conflict, &["checkout", "main"]);
    let main_oid = commit_file(&conflict, "README.md", "main\n", "main");
    run_git(&conflict, &["checkout", "feature"]);

    let error = rebase_onto(path_str(&conflict), "main").expect_err("rebase conflict");
    assert_eq!(error.kind, GitErrorKind::Conflict);
    let repo = Repository::open(conflict.path()).expect("reopen repository");
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert_eq!(head_oid(&conflict), feature_oid);
    assert_ne!(head_oid(&conflict), main_oid);
    assert_eq!(read_file(&conflict, "README.md"), "feature\n");
}

#[test]
fn checks_out_remote_tracking_branch_and_handles_tags_archive_and_rename() {
    let directory = init_repo();
    let bare = tempfile::tempdir().expect("bare remote directory");
    run_git(&bare, &["init", "--bare"]);
    run_git(&directory, &["remote", "add", "origin", path_str(&bare)]);
    run_git(&directory, &["checkout", "-b", "feature"]);
    let feature_oid = commit_file(&directory, "feature.txt", "feature\n", "feature");
    run_git(&directory, &["checkout", "main"]);
    run_git(&directory, &["branch", "-D", "feature"]);
    run_git(
        &directory,
        &[
            "update-ref",
            "refs/remotes/origin/feature",
            &feature_oid.to_string(),
        ],
    );

    let local_name = checkout_remote_branch(path_str(&directory), "origin/feature")
        .expect("checkout remote branch");
    assert_eq!(local_name, "feature");
    let repo = Repository::open(directory.path()).expect("reopen repository");
    assert_eq!(head_branch(&repo), "feature");
    assert_eq!(branch_oid(&repo, "feature"), feature_oid);
    assert_eq!(
        repo.find_branch("feature", git2::BranchType::Local)
            .expect("find local branch")
            .upstream()
            .expect("read upstream")
            .name()
            .expect("upstream name"),
        Some("origin/feature")
    );

    create_tag(path_str(&directory), &feature_oid.to_string(), "v1", None)
        .expect("create lightweight tag");
    let archive_path = directory.path().join("feature.zip");
    create_archive(
        path_str(&directory),
        "v1",
        &archive_path.to_string_lossy(),
        GitArchiveFormat::Zip,
    )
    .expect("create archive");
    assert!(archive_path.is_file());
    assert!(fs::metadata(&archive_path).expect("archive metadata").len() > 0);
    rename_branch(path_str(&directory), "feature", "renamed").expect("rename current branch");
    delete_tag(path_str(&directory), "v1").expect("delete tag");
    let missing = delete_tag(path_str(&directory), "v1").expect_err("missing tag");
    assert_eq!(missing.kind, GitErrorKind::BranchNotFound);
}

#[test]
fn pushes_tag_through_origin() {
    let directory = init_repo();
    let bare = tempfile::tempdir().expect("bare remote directory");
    run_git(&bare, &["init", "--bare"]);
    run_git(&directory, &["remote", "add", "origin", path_str(&bare)]);
    let root = head_oid(&directory);
    create_tag(
        path_str(&directory),
        &root.to_string(),
        "v1",
        Some("release"),
    )
    .expect("create annotated tag");
    push_tag(path_str(&directory), "v1", None).expect("push tag");
    assert!(run_git(&bare, &["show-ref", "--verify", "refs/tags/v1"]).contains("refs/tags/v1"));

    let remote_branch = "to-delete";
    let refspec = format!("HEAD:refs/heads/{remote_branch}");
    run_git(&directory, &["push", "origin", refspec.as_str()]);
    delete_remote_branch(path_str(&directory), "origin", remote_branch)
        .expect("delete remote branch");
    assert!(git_in_dir(
        bare.path(),
        &[
            "show-ref",
            "--verify",
            &format!("refs/heads/{remote_branch}")
        ],
    )
    .is_err());
}

fn init_repo() -> TempDir {
    let directory = tempfile::tempdir().expect("temporary repository");
    run_git(&directory, &["init", "-b", "main"]);
    run_git(&directory, &["config", "user.name", "Alera Tests"]);
    run_git(&directory, &["config", "user.email", "tests@alera.build"]);
    commit_file(&directory, "README.md", "initial\n", "initial");
    directory
}

fn commit_file(directory: &TempDir, file: &str, contents: &str, message: &str) -> git2::Oid {
    fs::write(directory.path().join(file), contents).expect("write test file");
    run_git(directory, &["add", "--", file]);
    run_git(directory, &["commit", "-m", message]);
    git2::Oid::from_str(run_git(directory, &["rev-parse", "HEAD"]).trim())
        .expect("parse commit oid")
}

fn cleanup_merge(directory: &TempDir, head: git2::Oid) {
    let repo = Repository::open(directory.path()).expect("open repository for cleanup");
    repo.cleanup_state().expect("cleanup merge state");
    let commit = repo.find_commit(head).expect("find original head");
    repo.reset(commit.as_object(), ResetType::Hard, None)
        .expect("restore original head");
}

fn head_oid(directory: &TempDir) -> git2::Oid {
    let repo = Repository::open(directory.path()).expect("open repository");
    let oid = repo
        .head()
        .expect("read HEAD")
        .target()
        .expect("HEAD target");
    oid
}

fn branch_oid(repo: &Repository, name: &str) -> git2::Oid {
    repo.find_branch(name, git2::BranchType::Local)
        .expect("find branch")
        .get()
        .target()
        .expect("branch target")
}

fn parent_oid(directory: &TempDir, oid: git2::Oid) -> git2::Oid {
    Repository::open(directory.path())
        .expect("open repository")
        .find_commit(oid)
        .expect("find commit")
        .parent_id(0)
        .expect("parent oid")
}

fn commit_message(repo: &Repository, oid: git2::Oid) -> String {
    format!(
        "{}\n",
        repo.find_commit(oid)
            .expect("find commit")
            .summary()
            .expect("read summary")
            .expect("commit summary")
    )
}

fn head_branch(repo: &Repository) -> String {
    repo.head()
        .expect("read HEAD")
        .shorthand()
        .expect("read branch")
        .to_string()
}

fn read_file(directory: &TempDir, file: &str) -> String {
    fs::read_to_string(directory.path().join(file))
        .expect("read test file")
        .replace("\r\n", "\n")
}

fn path_str(directory: &TempDir) -> &str {
    directory.path().to_str().expect("utf-8 test path")
}

fn run_git(directory: &TempDir, args: &[&str]) -> String {
    git_in_dir(directory.path(), args)
        .unwrap_or_else(|error| panic!("git {:?} failed: {}", args, error.message))
}
