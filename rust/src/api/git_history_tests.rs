use super::*;

#[test]
fn git_history_includes_upstream_only_commits() {
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

    commit_file(
        source.path(),
        "remote-only.txt",
        "remote\n",
        "remote only change",
    );
    run_git(source.path(), &["push"]);
    run_git(&clone_path, &["fetch", "origin"]);

    let history = git_history(path_str(&clone_path), Some(50), None, None, None).unwrap();

    assert!(history.has_incoming_changes);
    assert_eq!(history.remote_ref.unwrap().name, "origin/main");
    assert!(history
        .items
        .iter()
        .any(|item| item.subject == "remote only change"));
}

#[test]
fn git_history_refs_peel_annotated_tags_to_commits() {
    let repo = init_repo();
    run_git(repo.path(), &["tag", "-a", "v1.0.0", "-m", "release v1"]);

    let history = git_history(path_str(repo.path()), Some(50), None, None, None).unwrap();

    let initial = history
        .items
        .iter()
        .find(|item| item.subject == "initial")
        .expect("initial commit in history");
    assert!(initial.references.iter().any(|reference| {
        reference.name == "v1.0.0" && reference.category == Some(GitHistoryRefCategory::Tags)
    }));
}

#[test]
fn git_history_filters_commits_for_scoped_workspace_path() {
    let repo = init_repo();
    std::fs::create_dir_all(repo.path().join("packages/app/lib")).expect("create app dir");
    std::fs::create_dir_all(repo.path().join("packages/other/lib")).expect("create other dir");
    commit_file(
        repo.path(),
        "packages/app/lib/main.dart",
        "app\n",
        "app change",
    );
    commit_file(repo.path(), "README.md", "hello\nroot\n", "root change");
    commit_file(
        repo.path(),
        "packages/other/lib/main.dart",
        "other\n",
        "other change",
    );
    commit_file(
        repo.path(),
        "packages/app/lib/main.dart",
        "app\napp 2\n",
        "app change 2",
    );

    let history = git_history(
        path_str(&repo.path().join("packages/app")),
        Some(50),
        None,
        None,
        None,
    )
    .unwrap();
    let subjects = history
        .items
        .iter()
        .map(|item| item.subject.as_str())
        .collect::<Vec<_>>();

    assert!(subjects.contains(&"app change"));
    assert!(subjects.contains(&"app change 2"));
    assert!(!subjects.contains(&"other change"));
    assert!(!subjects.contains(&"root change"));
    assert!(!subjects.contains(&"initial"));
    let older = history
        .items
        .iter()
        .find(|item| item.subject == "app change")
        .expect("older scoped commit");
    let newer = history
        .items
        .iter()
        .find(|item| item.subject == "app change 2")
        .expect("newer scoped commit");
    assert_eq!(newer.parent_ids, vec![older.id.clone()]);
}

#[test]
fn git_history_suppresses_divergence_markers_outside_scoped_workspace_path() {
    let source = init_repo();
    std::fs::create_dir_all(source.path().join("packages/app/lib")).expect("create app dir");
    std::fs::create_dir_all(source.path().join("packages/other/lib")).expect("create other dir");
    commit_file(
        source.path(),
        "packages/app/lib/main.dart",
        "app\n",
        "app change",
    );
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

    commit_file(
        source.path(),
        "packages/other/remote.dart",
        "remote\n",
        "remote other change",
    );
    run_git(source.path(), &["push"]);
    run_git(&clone_path, &["fetch", "origin"]);
    std::fs::create_dir_all(clone_path.join("packages/other")).expect("create clone other dir");
    commit_file(
        &clone_path,
        "packages/other/local.dart",
        "local\n",
        "local other change",
    );

    let history = git_history(
        path_str(&clone_path.join("packages/app")),
        Some(50),
        None,
        None,
        None,
    )
    .unwrap();
    let subjects = history
        .items
        .iter()
        .map(|item| item.subject.as_str())
        .collect::<Vec<_>>();

    assert!(!history.has_incoming_changes);
    assert!(!history.has_outgoing_changes);
    assert!(subjects.contains(&"app change"));
    assert!(!subjects.contains(&"remote other change"));
    assert!(!subjects.contains(&"local other change"));
}

#[test]
fn git_history_include_all_refs_walks_other_branch_tips() {
    let repo = init_repo();
    run_git(repo.path(), &["checkout", "-b", "feature"]);
    commit_file(repo.path(), "feature.txt", "feature\n", "feature change");
    run_git(repo.path(), &["checkout", "main"]);
    commit_file(repo.path(), "main.txt", "main\n", "main change");

    let head_only = git_history(path_str(repo.path()), Some(50), None, None, None).unwrap();
    assert!(!head_only
        .items
        .iter()
        .any(|item| item.subject == "feature change"));

    let all = git_history(path_str(repo.path()), Some(50), None, Some(true), None).unwrap();
    let subjects = all
        .items
        .iter()
        .map(|item| item.subject.as_str())
        .collect::<Vec<_>>();

    assert!(subjects.contains(&"feature change"));
    assert!(subjects.contains(&"main change"));
    assert!(subjects.contains(&"initial"));
}

#[test]
fn git_history_selected_ref_walks_branch_without_checkout() {
    let repo = init_repo();
    run_git(repo.path(), &["checkout", "-b", "feature"]);
    commit_file(repo.path(), "feature.txt", "feature\n", "feature change");
    run_git(repo.path(), &["checkout", "main"]);
    commit_file(repo.path(), "main.txt", "main\n", "main change");

    let selected = git_history(
        path_str(repo.path()),
        Some(50),
        Some("feature".to_string()),
        Some(false),
        None,
    )
    .unwrap();
    let subjects = selected
        .items
        .iter()
        .map(|item| item.subject.as_str())
        .collect::<Vec<_>>();

    assert!(subjects.contains(&"feature change"));
    assert!(!subjects.contains(&"main change"));
    assert_eq!(
        head_branch_name(&git2::Repository::open(repo.path()).unwrap()),
        "main"
    );
}

#[test]
fn git_history_offset_pages_visible_items() {
    let repo = init_repo();
    commit_file(repo.path(), "a.txt", "1\n", "commit 1");
    commit_file(repo.path(), "a.txt", "2\n", "commit 2");
    commit_file(repo.path(), "a.txt", "3\n", "commit 3");

    let full = git_history(path_str(repo.path()), Some(50), None, None, None).unwrap();
    let first = git_history(path_str(repo.path()), Some(2), None, None, None).unwrap();
    let rest = git_history(path_str(repo.path()), Some(2), None, None, Some(2)).unwrap();
    let past_end = git_history(
        path_str(repo.path()),
        Some(2),
        None,
        None,
        Some(full.items.len() as u32),
    )
    .unwrap();

    let full_ids = full
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();

    assert!(first.has_more);
    assert_eq!(
        first
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        full_ids[..2].to_vec(),
    );
    assert!(!rest.has_more);
    assert_eq!(
        rest.items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        full_ids[2..].to_vec(),
    );
    assert!(past_end.items.is_empty());
    assert!(!past_end.has_more);
}
