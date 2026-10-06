use std::fs;
use std::path::{Path, PathBuf};

use git2::{
    BranchType, Repository, RepositoryInitOptions, Signature, StatusOptions, WorktreeAddOptions,
};
use tempfile::TempDir;

use super::{checkout_branch, create_and_checkout_branch, current_branch, GitErrorKind};

#[test]
fn creates_and_checks_out_branch_without_touching_pending_changes() {
    let (_directory, repo) = init_repo();
    fs::write(workdir(&repo).join("README.md"), "staged\n").expect("write staged file");
    let mut index = repo.index().expect("open index");
    index
        .add_path(Path::new("README.md"))
        .expect("stage README");
    index.write().expect("write index");
    fs::write(workdir(&repo).join("tracked.txt"), "unstaged\n").expect("write unstaged file");
    fs::write(workdir(&repo).join("untracked.txt"), "untracked\n").expect("write untracked file");
    let before = status_snapshot(&repo);
    let main_oid = branch_oid(&repo, "main");

    create_and_checkout_branch(path_str(workdir(&repo)), "ship/staged-change")
        .expect("create ship branch");

    assert_eq!(
        current_branch(path_str(workdir(&repo))).expect("read current branch"),
        "ship/staged-change"
    );
    assert_eq!(branch_oid(&repo, "ship/staged-change"), main_oid);
    assert_eq!(status_snapshot(&repo), before);
}

#[test]
fn rejects_an_existing_branch_without_switching_head() {
    let (_directory, repo) = init_repo();
    let head = repo
        .head()
        .expect("read HEAD")
        .peel_to_commit()
        .expect("read HEAD commit");
    repo.branch("ship/existing", &head, false)
        .expect("create existing branch");

    let error = create_and_checkout_branch(path_str(workdir(&repo)), "ship/existing")
        .expect_err("existing branch must fail");

    assert_eq!(error.kind, GitErrorKind::BranchAlreadyExists);
    assert_eq!(
        current_branch(path_str(workdir(&repo))).expect("read current branch"),
        "main"
    );
}

#[test]
fn rejects_detached_head() {
    let (_directory, repo) = init_repo();
    let head_oid = repo
        .head()
        .expect("read HEAD")
        .target()
        .expect("HEAD target");
    repo.set_head_detached(head_oid).expect("detach HEAD");

    let error = create_and_checkout_branch(path_str(workdir(&repo)), "ship/detached")
        .expect_err("detached HEAD must fail");

    assert_eq!(error.kind, GitErrorKind::DetachedHead);
}

#[test]
fn switches_to_existing_branch_without_touching_pending_changes() {
    let (_directory, repo) = init_repo();
    let head = repo
        .head()
        .expect("read HEAD")
        .peel_to_commit()
        .expect("read HEAD commit");
    repo.branch("feature/existing", &head, false)
        .expect("create existing branch");
    drop(head);
    fs::write(workdir(&repo).join("tracked.txt"), "unstaged\n").expect("write unstaged file");
    fs::write(workdir(&repo).join("untracked.txt"), "untracked\n").expect("write untracked file");
    let before = status_snapshot(&repo);

    checkout_branch(path_str(workdir(&repo)), "feature/existing").expect("switch branch");

    assert_eq!(
        current_branch(path_str(workdir(&repo))).expect("read current branch"),
        "feature/existing"
    );
    assert_eq!(status_snapshot(&repo), before);
}

#[test]
fn rejects_branch_checked_out_by_another_worktree() {
    let (directory, repo) = init_repo();
    let head = repo
        .head()
        .expect("read HEAD")
        .peel_to_commit()
        .expect("read HEAD commit");
    let branch = repo
        .branch("feature/other-worktree", &head, false)
        .expect("create branch");
    let reference = branch.into_reference();
    let mut options = WorktreeAddOptions::new();
    options.reference(Some(&reference));
    let worktree_path = directory.path().join("other-worktree");
    repo.worktree("other-worktree", &worktree_path, Some(&options))
        .expect("create worktree");

    let error = checkout_branch(path_str(workdir(&repo)), "feature/other-worktree")
        .expect_err("branch in another worktree must fail");

    assert_eq!(error.kind, GitErrorKind::WorktreeAlreadyExists);
    assert_eq!(
        current_branch(path_str(workdir(&repo))).expect("read current branch"),
        "main"
    );
}

#[test]
fn rejects_conflicting_dirty_checkout_without_switching_head() {
    let (_directory, repo) = init_repo();
    create_branch_commit(&repo, "feature/changed-readme", "branch version\n");
    assert_ne!(
        branch_oid(&repo, "main"),
        branch_oid(&repo, "feature/changed-readme")
    );
    assert_eq!(
        current_branch(path_str(workdir(&repo))).expect("read current branch"),
        "main"
    );
    assert_eq!(
        branch_diff_paths(&repo, "main", "feature/changed-readme"),
        vec![PathBuf::from("README.md")]
    );
    fs::write(workdir(&repo).join("README.md"), "local version\n").expect("write local change");
    assert!(
        status_snapshot(&repo)
            .iter()
            .any(|(path, _)| path == Path::new("README.md")),
        "README.md must be dirty before switching"
    );

    let error = checkout_branch(path_str(workdir(&repo)), "feature/changed-readme")
        .expect_err("conflicting dirty checkout must fail");

    assert_eq!(error.kind, GitErrorKind::Conflict);
    assert_eq!(
        current_branch(path_str(workdir(&repo))).expect("read current branch"),
        "main"
    );
    assert_eq!(
        fs::read_to_string(workdir(&repo).join("README.md")).expect("read README"),
        "local version\n"
    );
}

#[test]
fn switching_branch_removes_previous_branch_files_and_preserves_untracked_files() {
    let (_directory, repo) = init_repo();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    let parent_tree = parent.tree().unwrap();
    let mut builder = repo.treebuilder(Some(&parent_tree)).unwrap();
    builder
        .insert("feature-only.txt", repo.blob(b"feature").unwrap(), 0o100644)
        .unwrap();
    builder.remove("tracked.txt").unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let signature = Signature::now("Tests", "tests@example.com").unwrap();
    repo.commit(
        Some("refs/heads/feature"),
        &signature,
        &signature,
        "feature files",
        &tree,
        &[&parent],
    )
    .unwrap();
    fs::write(workdir(&repo).join("untracked.txt"), "keep me").unwrap();
    checkout_branch(path_str(workdir(&repo)), "feature").unwrap();
    assert!(workdir(&repo).join("feature-only.txt").exists());
    assert!(!workdir(&repo).join("tracked.txt").exists());
    assert_eq!(
        fs::read_to_string(workdir(&repo).join("untracked.txt")).unwrap(),
        "keep me"
    );
    checkout_branch(path_str(workdir(&repo)), "main").unwrap();
    assert!(!workdir(&repo).join("feature-only.txt").exists());
    assert!(workdir(&repo).join("tracked.txt").exists());
    assert_eq!(status_snapshot(&repo).len(), 1);
}

fn create_branch_commit(repo: &Repository, branch: &str, readme: &str) {
    let parent = repo
        .head()
        .expect("read HEAD")
        .peel_to_commit()
        .expect("read HEAD commit");
    repo.branch(branch, &parent, false).expect("create branch");
    let parent_tree = parent.tree().expect("read parent tree");
    let readme_blob = repo.blob(readme.as_bytes()).expect("write README blob");
    let mut tree_builder = repo
        .treebuilder(Some(&parent_tree))
        .expect("create branch tree builder");
    tree_builder
        .insert("README.md", readme_blob, 0o100644)
        .expect("replace branch README");
    let tree_oid = tree_builder.write().expect("write branch tree");
    let tree = repo.find_tree(tree_oid).expect("find branch tree");
    let signature = Signature::now("Alera Tests", "tests@alera.build").expect("signature");
    repo.commit(
        Some(&format!("refs/heads/{branch}")),
        &signature,
        &signature,
        "branch change",
        &tree,
        &[&parent],
    )
    .expect("commit branch change");
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
        .add_all(
            ["README.md", "tracked.txt"],
            git2::IndexAddOption::DEFAULT,
            None,
        )
        .expect("stage initial files");
    index.write().expect("write index");
    let tree_oid = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_oid).expect("find tree");
    let signature = Signature::now("Alera Tests", "tests@alera.build").expect("signature");
    repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .expect("initial commit");
    drop(tree);
    (directory, repo)
}

fn branch_oid(repo: &Repository, branch: &str) -> git2::Oid {
    repo.find_branch(branch, BranchType::Local)
        .expect("find branch")
        .get()
        .target()
        .expect("branch target")
}

fn branch_diff_paths(repo: &Repository, from: &str, to: &str) -> Vec<PathBuf> {
    let from_tree = repo
        .find_branch(from, BranchType::Local)
        .expect("find from branch")
        .get()
        .peel_to_commit()
        .expect("peel from commit")
        .tree()
        .expect("read from tree");
    let to_tree = repo
        .find_branch(to, BranchType::Local)
        .expect("find to branch")
        .get()
        .peel_to_commit()
        .expect("peel to commit")
        .tree()
        .expect("read to tree");
    let diff = repo
        .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
        .expect("diff branches");
    let mut paths = diff
        .deltas()
        .filter_map(|delta| delta.new_file().path().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
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

fn workdir(repo: &Repository) -> &Path {
    repo.workdir().expect("repository worktree")
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}
