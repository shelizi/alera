use std::fs;
use std::path::{Path, PathBuf};

use git2::{
    IndexAddOption, Repository, RepositoryInitOptions, RepositoryState, Signature, StatusOptions,
};
use tempfile::TempDir;

use super::{
    checkout_commit, current_branch, reset_to_commit, revert_commit, GitErrorKind, GitResetMode,
};

#[test]
fn revert_commit_creates_expected_commit() {
    let (_directory, repo) = init_repo();
    let target_oid = commit_file(&repo, "HEAD", "README.md", "second\n", "second change");
    let target = repo.find_commit(target_oid).expect("find target commit");
    let parent_oid = target.parent_id(0).expect("target parent");
    let parent_tree_id = repo
        .find_commit(parent_oid)
        .expect("find target parent")
        .tree_id();
    drop(target);

    let new_oid = revert_commit(path_str(workdir(&repo)), &target_oid.to_string(), None)
        .expect("revert commit");
    let verification_repo = Repository::open(workdir(&repo)).expect("reopen repository");
    let new_commit = verification_repo
        .revparse_single(&new_oid)
        .expect("find new commit")
        .peel_to_commit()
        .expect("peel new commit");

    assert_eq!(new_oid, new_commit.id().to_string());
    assert_eq!(
        new_commit.parent_id(0).expect("new commit parent"),
        target_oid
    );
    assert_eq!(new_commit.tree_id(), parent_tree_id);
    assert_eq!(
        new_commit.message().expect("new commit message"),
        format!("Revert \"second change\"\n\nThis reverts commit {target_oid}.")
    );
    assert_eq!(worktree_file(&verification_repo, "README.md"), "initial\n");
    assert_eq!(
        current_branch(path_str(workdir(&verification_repo))).expect("read current branch"),
        "main"
    );
    assert!(status_snapshot(&verification_repo).is_empty());
}

#[test]
fn rejects_merge_revert_without_mainline() {
    let (_directory, repo) = init_repo();
    let merge_oid = create_merge_commit(&repo);

    let error = revert_commit(path_str(workdir(&repo)), &merge_oid.to_string(), None)
        .expect_err("merge revert without mainline must fail");

    assert_eq!(error.kind, GitErrorKind::Internal);
    assert!(error.context.contains("mainline"));
    assert_eq!(repo.head().expect("read HEAD").target(), Some(merge_oid));
    assert!(status_snapshot(&repo).is_empty());
}

#[test]
fn rejects_merge_revert_with_out_of_range_mainline() {
    let (_directory, repo) = init_repo();
    let merge_oid = create_merge_commit(&repo);

    let error = revert_commit(path_str(workdir(&repo)), &merge_oid.to_string(), Some(3))
        .expect_err("out-of-range mainline must fail");

    assert_eq!(error.kind, GitErrorKind::Internal);
    assert!(error.context.contains("mainline"));
    assert_eq!(repo.head().expect("read HEAD").target(), Some(merge_oid));
    assert!(status_snapshot(&repo).is_empty());
}

#[test]
fn reverts_merge_with_selected_mainline() {
    let (_directory, repo) = init_repo();
    let merge_oid = create_merge_commit(&repo);

    let new_oid = revert_commit(path_str(workdir(&repo)), &merge_oid.to_string(), Some(1))
        .expect("revert merge commit");
    let verification_repo = Repository::open(workdir(&repo)).expect("reopen repository");
    let new_commit = verification_repo
        .revparse_single(&new_oid)
        .expect("find merge revert commit")
        .peel_to_commit()
        .expect("peel merge revert commit");

    assert_eq!(
        new_commit.parent_id(0).expect("merge revert parent"),
        merge_oid
    );
    assert_eq!(
        new_commit.message().expect("merge revert message"),
        format!("Revert \"merge change\"\n\nThis reverts commit {merge_oid}.")
    );
    assert_eq!(worktree_file(&verification_repo, "README.md"), "main\n");
    assert!(!workdir(&verification_repo).join("side.txt").exists());
    assert!(status_snapshot(&verification_repo).is_empty());
}

#[test]
fn rejects_non_merge_revert_with_mainline() {
    let (_directory, repo) = init_repo();
    let target_oid = commit_file(&repo, "HEAD", "README.md", "second\n", "second change");

    let error = revert_commit(path_str(workdir(&repo)), &target_oid.to_string(), Some(1))
        .expect_err("non-merge mainline must fail");

    assert_eq!(error.kind, GitErrorKind::Internal);
    assert!(error.context.contains("only valid for merge commits"));
    assert_eq!(repo.head().expect("read HEAD").target(), Some(target_oid));
    assert!(status_snapshot(&repo).is_empty());
}

#[test]
fn rejects_dirty_revert_overlap_without_changing_repository_state() {
    let (_directory, repo) = init_repo();
    let target_oid = commit_file(&repo, "HEAD", "README.md", "second\n", "second change");
    fs::write(workdir(&repo).join("README.md"), "local\n").expect("write local change");
    let before = status_snapshot(&repo);

    let error = revert_commit(path_str(workdir(&repo)), &target_oid.to_string(), None)
        .expect_err("overlapping dirty change must fail");
    let verification_repo = Repository::open(workdir(&repo)).expect("reopen repository");

    assert_eq!(error.kind, GitErrorKind::Conflict);
    assert_eq!(
        verification_repo.head().expect("read HEAD").target(),
        Some(target_oid)
    );
    assert_eq!(status_snapshot(&verification_repo), before);
    assert_eq!(worktree_file(&verification_repo, "README.md"), "local\n");
    assert_eq!(verification_repo.state(), RepositoryState::Clean);
}

#[test]
fn rejects_dirty_revert_overlap_for_root_commit() {
    let (_directory, repo) = init_repo();
    let root_oid = repo
        .head()
        .expect("read root HEAD")
        .target()
        .expect("root oid");
    fs::write(workdir(&repo).join("tracked.txt"), "local\n").expect("write local change");

    let error = revert_commit(path_str(workdir(&repo)), &root_oid.to_string(), None)
        .expect_err("root revert must reject overlapping dirty changes");
    let verification_repo = Repository::open(workdir(&repo)).expect("reopen repository");

    assert_eq!(error.kind, GitErrorKind::Conflict);
    assert_eq!(
        verification_repo.head().expect("read HEAD").target(),
        Some(root_oid)
    );
    assert_eq!(worktree_file(&verification_repo, "tracked.txt"), "local\n");
    assert_eq!(verification_repo.state(), RepositoryState::Clean);
}

#[test]
fn restores_clean_repository_after_revert_conflict() {
    let (_directory, repo) = init_repo();
    let target_oid = commit_file(&repo, "HEAD", "README.md", "second\n", "second change");
    let head_oid = commit_file(&repo, "HEAD", "README.md", "current\n", "current change");
    let before = status_snapshot(&repo);

    let error = revert_commit(path_str(workdir(&repo)), &target_oid.to_string(), None)
        .expect_err("conflicting revert must fail");
    let verification_repo = Repository::open(workdir(&repo)).expect("reopen repository");

    assert_eq!(error.kind, GitErrorKind::Conflict);
    assert_eq!(
        verification_repo.head().expect("read HEAD").target(),
        Some(head_oid)
    );
    assert_eq!(status_snapshot(&verification_repo), before);
    assert_eq!(worktree_file(&verification_repo, "README.md"), "current\n");
    assert!(!verification_repo
        .index()
        .expect("open index")
        .has_conflicts());
    assert_eq!(verification_repo.state(), RepositoryState::Clean);
}

#[test]
fn reset_modes_update_head_index_and_worktree_as_expected() {
    let (_soft_directory, soft_repo, soft_base, soft_target) = reset_fixture();
    reset_to_commit(
        path_str(workdir(&soft_repo)),
        &soft_base.to_string(),
        GitResetMode::Soft,
    )
    .expect("soft reset");
    assert_reset_result(&soft_repo, soft_base, "second\n", "second\n");
    assert_ne!(soft_base, soft_target);

    let (_mixed_directory, mixed_repo, mixed_base, _mixed_target) = reset_fixture();
    reset_to_commit(
        path_str(workdir(&mixed_repo)),
        &mixed_base.to_string(),
        GitResetMode::Mixed,
    )
    .expect("mixed reset");
    assert_reset_result(&mixed_repo, mixed_base, "initial\n", "second\n");

    let (_hard_directory, hard_repo, hard_base, _hard_target) = reset_fixture();
    reset_to_commit(
        path_str(workdir(&hard_repo)),
        &hard_base.to_string(),
        GitResetMode::Hard,
    )
    .expect("hard reset");
    assert_reset_result(&hard_repo, hard_base, "initial\n", "initial\n");
}

#[test]
fn reset_allows_detached_head() {
    let (_directory, repo, _target_base_oid, target_oid) = reset_fixture();
    let base_oid = repo
        .head()
        .expect("read HEAD")
        .peel_to_commit()
        .expect("read HEAD commit")
        .parent_id(0)
        .expect("base commit");
    repo.set_head_detached(target_oid).expect("detach HEAD");

    reset_to_commit(
        path_str(workdir(&repo)),
        &base_oid.to_string(),
        GitResetMode::Soft,
    )
    .expect("reset detached HEAD");

    assert!(repo.head_detached().expect("read detached HEAD"));
    assert_eq!(
        repo.head().expect("read detached target").target(),
        Some(base_oid)
    );
}

#[test]
fn checkout_commit_detaches_head_and_updates_worktree() {
    let (_directory, repo) = init_repo();
    let base_oid = repo
        .head()
        .expect("read base HEAD")
        .target()
        .expect("base oid");
    let tip_oid = commit_file(&repo, "HEAD", "README.md", "second\n", "second change");

    checkout_commit(path_str(workdir(&repo)), &base_oid.to_string()).expect("checkout base commit");
    let verification_repo = Repository::open(workdir(&repo)).expect("reopen repository");

    assert!(verification_repo
        .head_detached()
        .expect("read detached HEAD"));
    assert_eq!(
        verification_repo.head().expect("read HEAD").target(),
        Some(base_oid)
    );
    assert_ne!(base_oid, tip_oid);
    assert_eq!(worktree_file(&verification_repo, "README.md"), "initial\n");
    assert!(status_snapshot(&verification_repo).is_empty());
    assert_eq!(verification_repo.state(), RepositoryState::Clean);
}

#[test]
fn checkout_commit_is_noop_when_already_detached_at_target() {
    let (_directory, repo) = init_repo();
    let base_oid = repo
        .head()
        .expect("read base HEAD")
        .target()
        .expect("base oid");
    repo.set_head_detached(base_oid).expect("detach HEAD");

    checkout_commit(path_str(workdir(&repo)), &base_oid.to_string())
        .expect("checkout detached target");

    assert_eq!(repo.head().expect("read HEAD").target(), Some(base_oid));
}

#[test]
fn checkout_commit_rejects_non_clean_state() {
    let (_directory, repo) = init_repo();
    let target_oid = repo
        .head()
        .expect("read HEAD")
        .target()
        .expect("initial oid");
    fs::write(repo.path().join("MERGE_HEAD"), format!("{target_oid}\n"))
        .expect("write merge state");

    let error = checkout_commit(path_str(workdir(&repo)), &target_oid.to_string())
        .expect_err("checkout must reject merge state");

    assert_eq!(error.kind, GitErrorKind::Conflict);
    assert_eq!(repo.state(), RepositoryState::Merge);
    assert!(!repo.head_detached().expect("read detached HEAD"));
}

#[test]
fn checkout_commit_rejects_overlapping_dirty_changes() {
    let (_directory, repo) = init_repo();
    let base_oid = repo
        .head()
        .expect("read base HEAD")
        .target()
        .expect("base oid");
    let tip_oid = commit_file(&repo, "HEAD", "README.md", "second\n", "second change");
    fs::write(workdir(&repo).join("README.md"), "local\n").expect("write local change");

    let error = checkout_commit(path_str(workdir(&repo)), &base_oid.to_string())
        .expect_err("overlapping dirty change must abort checkout");
    let verification_repo = Repository::open(workdir(&repo)).expect("reopen repository");

    assert_eq!(error.kind, GitErrorKind::Conflict);
    assert_eq!(
        verification_repo.head().expect("read HEAD").target(),
        Some(tip_oid)
    );
    assert!(!verification_repo
        .head_detached()
        .expect("read detached HEAD"));
    assert_eq!(worktree_file(&verification_repo, "README.md"), "local\n");
}

#[test]
fn checkout_commit_preserves_non_overlapping_changes() {
    let (_directory, repo) = init_repo();
    let base_oid = repo
        .head()
        .expect("read base HEAD")
        .target()
        .expect("base oid");
    commit_file(&repo, "HEAD", "README.md", "second\n", "second change");
    fs::write(workdir(&repo).join("tracked.txt"), "local\n").expect("write local change");

    checkout_commit(path_str(workdir(&repo)), &base_oid.to_string()).expect("checkout base commit");
    let verification_repo = Repository::open(workdir(&repo)).expect("reopen repository");

    assert!(verification_repo
        .head_detached()
        .expect("read detached HEAD"));
    assert_eq!(worktree_file(&verification_repo, "README.md"), "initial\n");
    assert_eq!(worktree_file(&verification_repo, "tracked.txt"), "local\n");
    let snapshot = status_snapshot(&verification_repo);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].0, PathBuf::from("tracked.txt"));
}

#[test]
fn checkout_commit_rejects_bad_commit_id() {
    let (_directory, repo) = init_repo();

    let error = checkout_commit(path_str(workdir(&repo)), "not-a-commit")
        .expect_err("checkout must reject an invalid commit id");

    assert_eq!(error.kind, GitErrorKind::Internal);
    assert!(error.context.contains("not-a-commit"));
    assert!(!repo.head_detached().expect("read detached HEAD"));
}

#[test]
fn rejects_non_clean_repository_state_for_both_operations() {
    let (_directory, repo) = init_repo();
    let target_oid = repo
        .head()
        .expect("read HEAD")
        .target()
        .expect("initial oid");
    fs::write(repo.path().join("MERGE_HEAD"), format!("{target_oid}\n"))
        .expect("write merge state");
    assert_eq!(repo.state(), RepositoryState::Merge);

    let revert_error = revert_commit(path_str(workdir(&repo)), &target_oid.to_string(), None)
        .expect_err("revert must reject merge state");
    assert_eq!(revert_error.kind, GitErrorKind::Conflict);

    let reset_error = reset_to_commit(
        path_str(workdir(&repo)),
        &target_oid.to_string(),
        GitResetMode::Mixed,
    )
    .expect_err("reset must reject merge state");
    assert_eq!(reset_error.kind, GitErrorKind::Conflict);
    assert_eq!(repo.state(), RepositoryState::Merge);
    assert_eq!(repo.head().expect("read HEAD").target(), Some(target_oid));
}

#[test]
fn rejects_bad_commit_ids_with_internal_errors() {
    let (_directory, repo) = init_repo();

    let revert_error = revert_commit(path_str(workdir(&repo)), "not-a-commit", None)
        .expect_err("revert must reject an invalid commit id");
    assert_eq!(revert_error.kind, GitErrorKind::Internal);
    assert!(revert_error.context.contains("not-a-commit"));

    let reset_error = reset_to_commit(
        path_str(workdir(&repo)),
        "not-a-commit",
        GitResetMode::Mixed,
    )
    .expect_err("reset must reject an invalid commit id");
    assert_eq!(reset_error.kind, GitErrorKind::Internal);
    assert!(reset_error.context.contains("not-a-commit"));
}

fn assert_reset_result(repo: &Repository, expected_head: git2::Oid, index: &str, worktree: &str) {
    let verification_repo = Repository::open(workdir(repo)).expect("reopen repository");
    assert_eq!(
        verification_repo.head().expect("read reset HEAD").target(),
        Some(expected_head)
    );
    assert_eq!(
        repo_index_file(&verification_repo, "README.md"),
        Some(index.to_string())
    );
    assert_eq!(worktree_file(&verification_repo, "README.md"), worktree);
}

fn reset_fixture() -> (TempDir, Repository, git2::Oid, git2::Oid) {
    let (directory, repo) = init_repo();
    let base_oid = repo
        .head()
        .expect("read base HEAD")
        .target()
        .expect("base oid");
    let target_oid = commit_file(&repo, "HEAD", "README.md", "second\n", "second change");
    (directory, repo, base_oid, target_oid)
}

fn create_merge_commit(repo: &Repository) -> git2::Oid {
    let base = repo
        .head()
        .expect("read base HEAD")
        .peel_to_commit()
        .expect("read base commit");
    repo.branch("side", &base, false)
        .expect("create side branch");
    let side_oid = commit_file(repo, "refs/heads/side", "side.txt", "side\n", "side change");
    let main_oid = commit_file(repo, "HEAD", "README.md", "main\n", "main change");
    let main = repo.find_commit(main_oid).expect("find main commit");
    let side = repo.find_commit(side_oid).expect("find side commit");
    let main_tree = main.tree().expect("read main tree");
    let side_blob = repo.blob(b"side\n").expect("write side blob");
    let mut tree_builder = repo
        .treebuilder(Some(&main_tree))
        .expect("create merge tree builder");
    tree_builder
        .insert("side.txt", side_blob, 0o100644)
        .expect("add side file to merge tree");
    let merge_tree_oid = tree_builder.write().expect("write merge tree");
    let merge_tree = repo.find_tree(merge_tree_oid).expect("find merge tree");
    let signature = test_signature();
    let merge_oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "merge change",
            &merge_tree,
            &[&main, &side],
        )
        .expect("create merge commit");
    fs::write(workdir(repo).join("README.md"), "main\n").expect("sync main README");
    fs::write(workdir(repo).join("side.txt"), "side\n").expect("sync side file");
    let mut index = repo.index().expect("open merge index");
    index
        .add_all(["README.md", "side.txt"], IndexAddOption::DEFAULT, None)
        .expect("sync merge index");
    index.write().expect("write merge index");
    drop(merge_tree);
    drop(main_tree);
    drop(side);
    drop(main);
    drop(base);
    merge_oid
}

fn commit_file(
    repo: &Repository,
    reference: &str,
    file_path: &str,
    contents: &str,
    message: &str,
) -> git2::Oid {
    let parent = repo
        .revparse_single(&format!("{reference}^{{commit}}"))
        .expect("find commit parent")
        .peel_to_commit()
        .expect("peel commit parent");
    let parent_tree = parent.tree().expect("read parent tree");
    let blob = repo.blob(contents.as_bytes()).expect("write file blob");
    let mut tree_builder = repo
        .treebuilder(Some(&parent_tree))
        .expect("create tree builder");
    tree_builder
        .insert(file_path, blob, 0o100644)
        .expect("insert file into tree");
    let tree_oid = tree_builder.write().expect("write tree");
    let tree = repo.find_tree(tree_oid).expect("find tree");
    let signature = test_signature();
    let oid = repo
        .commit(
            Some(reference),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent],
        )
        .expect("create commit");
    if reference == "HEAD" {
        fs::write(workdir(repo).join(file_path), contents).expect("sync committed file");
        let mut index = repo.index().expect("open committed index");
        index
            .add_path(Path::new(file_path))
            .expect("sync committed index");
        index.write().expect("write committed index");
    }
    drop(tree);
    drop(parent_tree);
    drop(parent);
    oid
}

fn init_repo() -> (TempDir, Repository) {
    let directory = TempDir::new().expect("temporary repository");
    let path = directory.path();
    let mut options = RepositoryInitOptions::new();
    options.initial_head("main");
    let repo = Repository::init_opts(path, &options).expect("initialize repository");
    fs::write(path.join("README.md"), "initial\n").expect("write README");
    fs::write(path.join("tracked.txt"), "initial\n").expect("write tracked file");
    let mut index = repo.index().expect("open index");
    index
        .add_all(["README.md", "tracked.txt"], IndexAddOption::DEFAULT, None)
        .expect("stage initial files");
    index.write().expect("write index");
    let tree_oid = index.write_tree().expect("write initial tree");
    let tree = repo.find_tree(tree_oid).expect("find initial tree");
    let signature = test_signature();
    repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .expect("initial commit");
    drop(tree);
    (directory, repo)
}

fn repo_index_file(repo: &Repository, file_path: &str) -> Option<String> {
    let entry_id = repo
        .index()
        .expect("open index")
        .get_path(Path::new(file_path), 0)
        .map(|entry| entry.id)?;
    let blob = repo.find_blob(entry_id).expect("find index blob");
    Some(String::from_utf8(blob.content().to_vec()).expect("utf-8 index blob"))
}

fn worktree_file(repo: &Repository, file_path: &str) -> String {
    fs::read_to_string(workdir(repo).join(file_path))
        .expect("read worktree file")
        .replace("\r\n", "\n")
}

fn status_snapshot(repo: &Repository) -> Vec<(PathBuf, u32)> {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .update_index(true);
    let statuses = repo.statuses(Some(&mut options)).expect("read status");
    let mut snapshot = statuses
        .iter()
        .map(|entry| {
            (
                PathBuf::from(entry.path().expect("status path")),
                entry.status().bits(),
            )
        })
        .collect::<Vec<_>>();
    snapshot.sort_by(|left, right| left.0.cmp(&right.0));
    snapshot
}

fn test_signature() -> Signature<'static> {
    Signature::now("Alera Tests", "tests@alera.build").expect("test signature")
}

fn workdir(repo: &Repository) -> &Path {
    repo.workdir().expect("repository worktree")
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}
