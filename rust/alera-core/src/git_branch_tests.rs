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

fn metadata_worktree_fixture(repo: &Repository, count: usize) -> TempDir {
    let checkouts = TempDir::new().expect("temporary worktrees");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    for index in 0..count {
        let name = format!("metadata-{index:02}");
        let reference = repo.branch(&name, &head, false).unwrap().into_reference();
        let mut options = WorktreeAddOptions::new();
        options.reference(Some(&reference));
        repo.worktree(&name, &checkouts.path().join(&name), Some(&options))
            .unwrap();
    }
    checkouts
}

#[test]
fn readonly_head_metadata_matches_legacy_for_live_unborn_and_unusual_refs() {
    let (_directory, repo) = init_repo();
    let oid = repo.head().unwrap().target().unwrap();
    let assert_parity = |repo: &Repository, expected: &str| {
        assert_eq!(
            super::read_head_branch_name(repo),
            super::head_branch_name(repo)
        );
        assert_eq!(super::read_head_branch_name(repo), expected);
    };
    assert_parity(&repo, "main");
    let commit = repo.find_commit(oid).unwrap();
    repo.branch("feature/metadata", &commit, false).unwrap();
    repo.set_head("refs/heads/feature/metadata").unwrap();
    assert_parity(&repo, "feature/metadata");
    repo.set_head_detached(oid).unwrap();
    assert_parity(&repo, "HEAD");

    repo.reference("refs/tags/metadata", oid, false, "fixture")
        .unwrap();
    repo.reference_symbolic("HEAD", "refs/tags/metadata", true, "fixture")
        .unwrap();
    assert_eq!(
        super::read_head_branch_name(&repo),
        super::head_branch_name(&repo)
    );
    assert!(!repo.head().unwrap().is_branch());

    repo.set_head("refs/heads/unborn-metadata").unwrap();
    assert_parity(&repo, "unborn-metadata");
    // Broken HEAD keeps the exact former fallback behavior.
    fs::write(repo.path().join("HEAD"), "not-a-valid-ref\n").unwrap();
    let fresh = Repository::open(repo.workdir().unwrap()).unwrap();
    assert_eq!(
        super::read_head_branch_name(&fresh),
        super::head_branch_name(&fresh)
    );
}

#[test]
fn readonly_worktree_listing_keeps_rows_order_and_fresh_branch_metadata() {
    let (_directory, repo) = init_repo();
    let checkouts = metadata_worktree_fixture(&repo, 5);
    let linked = checkouts.path().join("metadata-00");
    let linked_repo = Repository::open(&linked).unwrap();
    let detached_path = checkouts.path().join("metadata-01");
    let detached = Repository::open(&detached_path).unwrap();
    detached
        .set_head_detached(detached.head().unwrap().target().unwrap())
        .unwrap();
    drop(detached);
    let missing = checkouts.path().join("metadata-04");
    fs::remove_dir_all(&missing).unwrap();

    for path in [repo.workdir().unwrap(), linked.as_path()] {
        let path = path_str(path);
        let old = super::list_worktrees_with_policy(path, false).unwrap();
        let new = super::list_worktrees(path).unwrap();
        assert_eq!(old, new);
        assert!(new.iter().any(|row| row.branch == "metadata-00"));
        assert!(new
            .iter()
            .any(|row| row.path.contains("metadata-01") && row.branch == "HEAD"));
        // A removed checkout, if listed by libgit2, must retain the old HEAD fallback.
        assert!(new
            .iter()
            .all(|row| !row.path.contains("metadata-04") || row.branch == "HEAD"));
    }
    // When discovery starts in a linked checkout the old API includes it both
    // as the input workdir and in the linked rows; preserve that API contract.
    let old = super::list_worktrees_with_policy(path_str(&linked), false).unwrap();
    assert!(old.iter().filter(|row| row.branch == "metadata-00").count() >= 2);
    linked_repo
        .set_head_detached(linked_repo.head().unwrap().target().unwrap())
        .unwrap();
    let fresh = super::list_worktrees(path_str(&linked)).unwrap();
    assert_eq!(
        fresh,
        super::list_worktrees_with_policy(path_str(&linked), false).unwrap()
    );
    assert!(fresh.iter().all(|row| row.branch != "metadata-00"));
    assert!(fresh
        .iter()
        .any(|row| row.path.contains("metadata-00") && row.branch == "HEAD"));
}

#[test]
#[ignore = "real linked worktree metadata performance benchmark"]
fn benchmark_shared_readonly_head_metadata() {
    use std::time::{Duration, Instant};
    let (_directory, repo) = init_repo();
    let checkouts = metadata_worktree_fixture(&repo, 32);
    let root = repo.workdir().unwrap();
    let linked = checkouts.path().join("metadata-00");
    fn median(samples: &mut [Duration]) -> f64 {
        samples.sort();
        samples[samples.len() / 2].as_secs_f64() * 1000.0
    }
    for (scope, path) in [("main", root), ("linked", linked.as_path())] {
        let path = path_str(path);
        let expected = super::list_worktrees_with_policy(path, false).unwrap();
        assert_eq!(super::list_worktrees(path).unwrap(), expected);
        let mut old = Vec::new();
        let mut new = Vec::new();
        for sample in 0..7 {
            for optimized in if sample % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let start = Instant::now();
                let actual = super::list_worktrees_with_policy(path, optimized).unwrap();
                let elapsed = start.elapsed();
                assert_eq!(actual, expected);
                if optimized {
                    new.push(elapsed);
                } else {
                    old.push(elapsed);
                }
            }
        }
        let old = median(&mut old);
        let new = median(&mut new);
        println!("Worktree list scope={scope} linked=32 rows={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", expected.len(), old/new);
    }
    // Freshly open/discover each repository, as current_branch does in
    // production. This measures the shared branch display path, not a warm
    // already-initialized object database.
    let path = path_str(&linked);
    let mut old = Vec::new();
    let mut new = Vec::new();
    for optimized in [false, true] {
        let opened = super::open_repo(path).unwrap();
        assert_eq!(
            super::head_branch_name_with_policy(&opened, optimized),
            "metadata-00"
        );
    }
    for sample in 0..7 {
        for optimized in if sample % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let start = Instant::now();
            for _ in 0..100 {
                let opened = super::open_repo(path).unwrap();
                let name = super::head_branch_name_with_policy(&opened, optimized);
                assert_eq!(name, "metadata-00");
            }
            let elapsed = start.elapsed();
            if optimized {
                new.push(elapsed);
            } else {
                old.push(elapsed);
            }
        }
    }
    let old = median(&mut old) / 100.0;
    let new = median(&mut new) / 100.0;
    println!("Shared current_branch fresh linked discovery old_ms={old:.3} new_ms={new:.3} speedup={:.2}x (100 reads/sample)", old/new);
}
