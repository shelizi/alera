use super::*;
use crate::api::git_explorer_status::{git_explorer_status_snapshot, GitExplorerStatus};

#[test]
fn detects_repository() {
    let repo = init_repo();
    assert!(is_git_repository(path_str(repo.path())).unwrap());

    let plain = tempfile::tempdir().expect("tempdir");
    assert!(!is_git_repository(path_str(plain.path())).unwrap());
}

#[test]
fn explorer_status_snapshot_aggregates_changed_ancestors_once() {
    let repo = init_repo();
    std::fs::create_dir_all(repo.path().join("src/nested")).expect("create nested");
    std::fs::write(repo.path().join("src/nested/new.txt"), "new").expect("write new");
    std::fs::write(repo.path().join("README.md"), "changed").expect("modify readme");

    let snapshot = git_explorer_status_snapshot(path_str(repo.path())).expect("snapshot");
    let statuses = snapshot
        .entries
        .into_iter()
        .map(|entry| (entry.path, entry.status))
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(statuses["README.md"], GitExplorerStatus::Modified);
    assert_eq!(statuses["src/nested/new.txt"], GitExplorerStatus::Untracked);
    assert_eq!(statuses["src/nested"], GitExplorerStatus::Untracked);
    assert_eq!(statuses["src"], GitExplorerStatus::Untracked);
}

fn assert_explorer_matches_full_status(path: &Path) {
    let old = crate::api::git_explorer_status::legacy_explorer_status_snapshot(path_str(path))
        .expect("full status projection");
    let new = git_explorer_status_snapshot(path_str(path)).expect("lightweight status projection");
    assert_eq!(new, old);
}

#[test]
fn explorer_lightweight_status_matches_full_status_across_changes_and_scopes() {
    let repo = init_repo();
    let root = repo.path();
    std::fs::create_dir_all(root.join("src/nested")).unwrap();
    std::fs::create_dir_all(root.join("other")).unwrap();
    for name in [
        "src/modified.txt",
        "src/renamed.txt",
        "src/deleted.txt",
        "src/nested/data.bin",
        "other/outside.txt",
    ] {
        std::fs::write(root.join(name), b"original\n").unwrap();
    }
    std::fs::write(root.join(".gitignore"), "*.ignored\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "fixture"]);
    assert_explorer_matches_full_status(root);

    std::fs::write(root.join("src/modified.txt"), "staged\n").unwrap();
    run_git(root, &["add", "src/modified.txt"]);
    std::fs::write(root.join("src/modified.txt"), "unstaged\n").unwrap();
    run_git(root, &["mv", "src/renamed.txt", "src/nested/renamed.txt"]);
    std::fs::remove_file(root.join("src/deleted.txt")).unwrap();
    std::fs::write(root.join("src/nested/data.bin"), [0u8, 1, 2, 0, 3]).unwrap();
    std::fs::write(root.join("src/nested/new.txt"), "untracked").unwrap();
    std::fs::write(root.join("src/new-staged.txt"), "added").unwrap();
    run_git(root, &["add", "src/new-staged.txt"]);
    std::fs::write(root.join("src/nested/hidden.ignored"), "ignored").unwrap();
    std::fs::write(root.join("other/outside.txt"), "outside changed").unwrap();

    for scope in [
        root.to_path_buf(),
        root.join("src"),
        root.join("src/nested"),
    ] {
        assert_explorer_matches_full_status(&scope);
    }
    let result = git_explorer_status_snapshot(path_str(root)).unwrap();
    let statuses = result
        .entries
        .into_iter()
        .map(|entry| (entry.path, entry.status))
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(statuses["src"], GitExplorerStatus::Modified);
    assert_eq!(statuses["src/new-staged.txt"], GitExplorerStatus::Added);
    assert_eq!(statuses["src/nested/new.txt"], GitExplorerStatus::Untracked);
    assert!(!statuses.contains_key("src/nested/hidden.ignored"));

    // Fresh queries, not a cache: restore one file and add another.
    std::fs::remove_file(root.join("src/nested/new.txt")).unwrap();
    std::fs::write(root.join("src/nested/next.txt"), "next").unwrap();
    assert_explorer_matches_full_status(root);
    let fresh = git_explorer_status_snapshot(path_str(root)).unwrap();
    assert!(!fresh
        .entries
        .iter()
        .any(|entry| entry.path == "src/nested/new.txt"));
    assert!(fresh
        .entries
        .iter()
        .any(|entry| entry.path == "src/nested/next.txt"));
}

#[test]
fn explorer_lightweight_status_isolated_linked_worktree_and_plain_folder() {
    let main = init_repo();
    let sibling = tempfile::tempdir().unwrap();
    let linked = sibling.path().join("linked");
    run_git(
        main.path(),
        &["worktree", "add", "-b", "linked-test", &path_str(&linked)],
    );
    std::fs::write(main.path().join("main-only.txt"), "main").unwrap();
    std::fs::write(linked.join("README.md"), "linked edit").unwrap();
    std::fs::write(linked.join("linked-only.txt"), "linked").unwrap();
    assert_explorer_matches_full_status(main.path());
    assert_explorer_matches_full_status(&linked);
    let actual = git_explorer_status_snapshot(path_str(&linked)).unwrap();
    assert!(!actual
        .entries
        .iter()
        .any(|entry| entry.path == "main-only.txt"));
    assert!(actual
        .entries
        .iter()
        .any(|entry| entry.path == "linked-only.txt"));
    let plain = tempfile::tempdir().unwrap();
    assert!(git_explorer_status_snapshot(path_str(plain.path()))
        .unwrap()
        .entries
        .is_empty());
}

#[test]
#[ignore = "real repository timing benchmark; run explicitly in isolation"]
fn benchmark_explorer_lightweight_status() {
    use std::time::Instant;
    let repo = init_repo();
    let root = repo.path();
    for i in 0..8 {
        std::fs::create_dir_all(root.join(format!("src/module_{i:02}"))).unwrap();
    }
    for i in 0..320 {
        let file = root.join(format!("src/module_{:02}/file_{i:03}.txt", i % 8));
        std::fs::write(&file, "original data line\n".repeat(80)).unwrap();
    }
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "benchmark fixture"]);
    for i in 0..160 {
        let file = root.join(format!("src/module_{:02}/file_{i:03}.txt", i % 8));
        std::fs::write(&file, "original data line\n".repeat(70) + "changed line\n").unwrap();
        if i < 40 {
            run_git(
                root,
                &["add", &format!("src/module_{:02}/file_{i:03}.txt", i % 8)],
            );
        }
    }
    for i in 0..40 {
        std::fs::write(
            root.join(format!("src/module_{:02}/new_{i:03}.txt", i % 8)),
            "new",
        )
        .unwrap();
    }
    let path = path_str(root);
    let old =
        || crate::api::git_explorer_status::legacy_explorer_status_snapshot(path.clone()).unwrap();
    let new = || git_explorer_status_snapshot(path.clone()).unwrap();
    let expected = old();
    assert_eq!(new(), expected);
    let mut old_ms = Vec::new();
    let mut new_ms = Vec::new();
    let timed = |f: &dyn Fn() -> crate::api::git_explorer_status::GitExplorerStatusSnapshot| {
        let start = Instant::now();
        let result = f();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(result, expected);
        ms
    };
    for i in 0..7 {
        if i % 2 == 0 {
            old_ms.push(timed(&old));
            new_ms.push(timed(&new));
        } else {
            new_ms.push(timed(&new));
            old_ms.push(timed(&old));
        }
    }
    old_ms.sort_by(f64::total_cmp);
    new_ms.sort_by(f64::total_cmp);
    println!("Explorer projection: {} rows, 320 tracked files, 160 changed, 40 untracked; seven alternating runs; old median {:.3} ms, new median {:.3} ms", expected.entries.len(), old_ms[3], new_ms[3]);
    println!("old ms {old_ms:?}; new ms {new_ms:?}");
}
