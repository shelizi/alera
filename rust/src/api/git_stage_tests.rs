use super::*;

#[test]
fn git_stage_unstage_commit_and_state_follow_index() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "hello\nupdated\n").expect("modify readme");

    git_stage(path_str(repo.path()), Some("README.md".to_string())).unwrap();
    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "README.md" && entry.area == GitChangeArea::Staged));

    git_unstage(path_str(repo.path()), Some("README.md".to_string())).unwrap();
    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "README.md" && entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "README.md" && entry.area == GitChangeArea::Staged));

    git_stage(path_str(repo.path()), Some("README.md".to_string())).unwrap();
    let oid = git_commit(path_str(repo.path()), "update readme".to_string()).unwrap();
    assert!(!oid.is_empty());
    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.is_empty());

    let state = git_repository_state(path_str(repo.path())).unwrap();
    assert_eq!(state.branch, "main");
    assert!(!state.has_conflicts);
}

#[test]
fn git_unstage_treats_selected_pathspec_characters_as_literals() {
    let repo = init_repo();
    std::fs::write(repo.path().join("file[1].txt"), "literal bracket\n")
        .expect("write bracket file");
    std::fs::write(repo.path().join("file1.txt"), "glob match\n").expect("write match file");
    run_git(repo.path(), &["add", "file[1].txt", "file1.txt"]);
    run_git(repo.path(), &["commit", "-m", "add bracket files"]);

    std::fs::write(repo.path().join("file[1].txt"), "literal bracket changed\n")
        .expect("modify bracket file");
    std::fs::write(repo.path().join("file1.txt"), "glob match changed\n")
        .expect("modify match file");
    run_git(repo.path(), &["add", "file[1].txt", "file1.txt"]);

    git_unstage(path_str(repo.path()), Some("file[1].txt".to_string())).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "file[1].txt" && entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "file[1].txt" && entry.area == GitChangeArea::Staged));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "file1.txt" && entry.area == GitChangeArea::Staged));
}

#[test]
fn git_stage_all_treats_workspace_pathspec_characters_as_literals() {
    let repo = init_repo();
    let bracket_dir = repo.path().join("packages").join("app[1]");
    let glob_match_dir = repo.path().join("packages").join("app1");
    std::fs::create_dir_all(&bracket_dir).expect("create bracket dir");
    std::fs::create_dir_all(&glob_match_dir).expect("create glob match dir");
    std::fs::write(bracket_dir.join("foo.dart"), "bracket\n").expect("write bracket file");
    std::fs::write(glob_match_dir.join("foo.dart"), "glob\n").expect("write glob file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app dirs"]);

    std::fs::write(bracket_dir.join("foo.dart"), "bracket changed\n").expect("modify bracket file");
    std::fs::write(glob_match_dir.join("foo.dart"), "glob changed\n").expect("modify glob file");

    git_stage(path_str(&bracket_dir), None).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app[1]/foo.dart" && entry.area == GitChangeArea::Staged
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app1/foo.dart" && entry.area == GitChangeArea::Unstaged
    }));
    assert!(!status.entries.iter().any(|entry| {
        entry.path == "packages/app1/foo.dart" && entry.area == GitChangeArea::Staged
    }));
}

#[test]
fn git_stage_area_limits_to_selected_area_and_folder() {
    let repo = init_repo();
    std::fs::create_dir_all(repo.path().join("lib")).expect("create lib");
    std::fs::create_dir_all(repo.path().join("test")).expect("create test");
    std::fs::write(repo.path().join("lib/tracked.dart"), "lib\n").expect("write lib tracked");
    std::fs::write(repo.path().join("test/tracked.dart"), "test\n").expect("write test tracked");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add tracked files"]);

    std::fs::write(repo.path().join("lib/tracked.dart"), "lib changed\n")
        .expect("modify lib tracked");
    std::fs::write(repo.path().join("lib/new.dart"), "new\n").expect("write lib new");
    std::fs::write(repo.path().join("test/tracked.dart"), "test changed\n")
        .expect("modify test tracked");

    git_stage_area(
        path_str(repo.path()),
        GitChangeArea::Unstaged,
        Some("lib".to_string()),
    )
    .unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| { entry.path == "lib/tracked.dart" && entry.area == GitChangeArea::Staged }));
    assert!(status
        .entries
        .iter()
        .any(|entry| { entry.path == "lib/new.dart" && entry.area == GitChangeArea::Untracked }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "test/tracked.dart" && entry.area == GitChangeArea::Unstaged
    }));
}

#[test]
fn git_unstage_all_treats_workspace_pathspec_characters_as_literals() {
    let repo = init_repo();
    let bracket_dir = repo.path().join("packages").join("app[1]");
    let glob_match_dir = repo.path().join("packages").join("app1");
    std::fs::create_dir_all(&bracket_dir).expect("create bracket dir");
    std::fs::create_dir_all(&glob_match_dir).expect("create glob match dir");
    std::fs::write(bracket_dir.join("foo.dart"), "bracket\n").expect("write bracket file");
    std::fs::write(glob_match_dir.join("foo.dart"), "glob\n").expect("write glob file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app dirs"]);

    std::fs::write(bracket_dir.join("foo.dart"), "bracket changed\n").expect("modify bracket file");
    std::fs::write(glob_match_dir.join("foo.dart"), "glob changed\n").expect("modify glob file");
    run_git(
        repo.path(),
        &["add", "packages/app[1]/foo.dart", "packages/app1/foo.dart"],
    );

    git_unstage(path_str(&bracket_dir), None).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app[1]/foo.dart" && entry.area == GitChangeArea::Unstaged
    }));
    assert!(!status.entries.iter().any(|entry| {
        entry.path == "packages/app[1]/foo.dart" && entry.area == GitChangeArea::Staged
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app1/foo.dart" && entry.area == GitChangeArea::Staged
    }));
}

#[test]
fn git_stage_selected_path_uses_repo_relative_index_path_from_subdirectory() {
    let repo = init_repo();
    let root_lib_dir = repo.path().join("lib");
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&root_lib_dir).expect("create root lib dir");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(root_lib_dir.join("foo.dart"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add colliding paths"]);

    std::fs::write(root_lib_dir.join("foo.dart"), "root changed\n").expect("modify root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");

    git_stage(path_str(&app_dir), Some("lib/foo.dart".to_string())).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Staged));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "lib/foo.dart" && entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "lib/foo.dart" && entry.area == GitChangeArea::Staged));
}

#[test]
fn git_stage_selected_path_handles_rename_out_from_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    let other_dir = repo.path().join("packages").join("other");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::create_dir_all(&other_dir).expect("create other dir");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app file"]);

    std::fs::rename(app_lib_dir.join("foo.dart"), other_dir.join("foo.dart"))
        .expect("rename file out of workspace");

    git_stage(path_str(&app_dir), Some("lib/foo.dart".to_string())).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Staged
            && entry.status == GitChangeStatus::Deleted
    }));
    assert!(!status.entries.iter().any(|entry| {
        entry.path == "packages/other/foo.dart" && entry.area == GitChangeArea::Staged
    }));
}

#[test]
fn git_stage_all_handles_rename_out_from_subdirectory_workspace() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    let other_dir = repo.path().join("packages").join("other");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::create_dir_all(&other_dir).expect("create other dir");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app file"]);

    std::fs::rename(app_lib_dir.join("foo.dart"), other_dir.join("foo.dart"))
        .expect("rename file out of workspace");

    git_stage(path_str(&app_dir), None).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status.entries.iter().any(|entry| {
        entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Staged
            && entry.status == GitChangeStatus::Deleted
    }));
    assert!(!status.entries.iter().any(|entry| {
        entry.path == "packages/other/foo.dart" && entry.area == GitChangeArea::Staged
    }));
}

#[test]
fn git_unstage_selected_path_uses_repo_relative_index_path_from_subdirectory() {
    let repo = init_repo();
    let root_lib_dir = repo.path().join("lib");
    let app_dir = repo.path().join("packages").join("app");
    let app_lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&root_lib_dir).expect("create root lib dir");
    std::fs::create_dir_all(&app_lib_dir).expect("create app lib dir");
    std::fs::write(root_lib_dir.join("foo.dart"), "root\n").expect("write root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add colliding paths"]);

    std::fs::write(root_lib_dir.join("foo.dart"), "root changed\n").expect("modify root file");
    std::fs::write(app_lib_dir.join("foo.dart"), "app changed\n").expect("modify app file");
    run_git(
        repo.path(),
        &["add", "lib/foo.dart", "packages/app/lib/foo.dart"],
    );

    git_unstage(path_str(&app_dir), Some("lib/foo.dart".to_string())).unwrap();

    let status = git_status(path_str(repo.path())).unwrap();
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "packages/app/lib/foo.dart"
            && entry.area == GitChangeArea::Staged));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "lib/foo.dart" && entry.area == GitChangeArea::Staged));
}

#[test]
#[ignore = "real repository mutation timing benchmark; run explicitly in isolation"]
fn benchmark_mutation_preflight_and_index_actions() {
    use std::time::Instant;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let repo = Repository::init(root).unwrap();
    let name = |i: usize| format!("src/file-{i:04}.txt");
    std::fs::create_dir(root.join("src")).unwrap();
    let mut index = repo.index().unwrap();
    for i in 0..512 {
        let file = name(i);
        std::fs::write(root.join(&file), format!("baseline {i}\n").repeat(80)).unwrap();
        index.add_path(Path::new(&file)).unwrap();
    }
    index.write().unwrap();
    let baseline_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(baseline_oid).unwrap();
    let sig = git2::Signature::now("Mutation Benchmark", "test@example.invalid").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "baseline", &tree, &[])
        .unwrap();
    drop(tree);

    for i in 0..80 {
        let file = name(i);
        std::fs::write(
            root.join(&file),
            format!("baseline {i}\n").repeat(70) + "staged\n",
        )
        .unwrap();
        index.add_path(Path::new(&file)).unwrap();
    }
    index.write().unwrap();
    let partial_oid = index.write_tree().unwrap();
    for i in 0..160 {
        let file = name(i);
        std::fs::write(
            root.join(&file),
            format!("baseline {i}\n").repeat(70) + "worktree\n",
        )
        .unwrap();
        index.add_path(Path::new(&file)).unwrap();
    }
    index.write().unwrap();
    let all_staged_oid = index.write_tree().unwrap();
    drop(index);
    drop(repo);
    let path = path_str(root);
    let reset_index = || {
        let repo = Repository::open(root).unwrap();
        let tree = repo.find_tree(partial_oid).unwrap();
        let mut index = repo.index().unwrap();
        index.read_tree(&tree).unwrap();
        index.write().unwrap();
    };
    let verify_index = |expected_oid| {
        let repo = Repository::open(root).unwrap();
        let mut index = repo.index().unwrap();
        assert_eq!(index.write_tree().unwrap(), expected_oid);
        for i in 0..160 {
            assert_eq!(
                std::fs::read_to_string(root.join(name(i))).unwrap(),
                format!("baseline {i}\n").repeat(70) + "worktree\n"
            );
        }
    };
    reset_index();
    assert_eq!(git_status(path.clone()).unwrap().entries.len(), 240);
    let measure = |stage: bool, lightweight: bool| {
        reset_index();
        let start = Instant::now();
        if lightweight {
            if stage {
                git_stage(path.clone(), None).unwrap();
            } else {
                git_unstage(path.clone(), None).unwrap();
            }
        } else {
            // The exact prior production call sequence, including the
            // independent status repository discovery and full UI stats.
            let repo = open_repo(&path).unwrap();
            let status = git_status(path.clone()).unwrap();
            if stage {
                stage_status_entries(&repo, &path, &status.entries).unwrap();
            } else {
                unstage_status_entries(&repo, &path, &status.entries).unwrap();
            }
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        verify_index(if stage { all_staged_oid } else { baseline_oid });
        elapsed
    };
    for (label, stage) in [("Stage all", true), ("Unstage all", false)] {
        // Warm both paths; exclude setup and index/content verification.
        measure(stage, false);
        measure(stage, true);
        let mut old = Vec::new();
        let mut new = Vec::new();
        for run in 0..7 {
            if run % 2 == 0 {
                old.push(measure(stage, false));
                new.push(measure(stage, true));
            } else {
                new.push(measure(stage, true));
                old.push(measure(stage, false));
            }
        }
        old.sort_by(f64::total_cmp);
        new.sort_by(f64::total_cmp);
        println!("{label}: 512 tracked files, 160 modified (80 also staged), 240 entries, seven alternating runs; old median {:.3} ms, new median {:.3} ms", old[3], new[3]);
        println!("{label} old ms {old:?}; new ms {new:?}");
    }
}
