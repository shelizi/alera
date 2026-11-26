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
fn mutation_path_resolver_preserves_scopes_and_lexical_validation() {
    let dir = init_repo();
    let repo = Repository::open(dir.path()).unwrap();
    let workspace = dir.path().join("packages/app[1]");
    std::fs::create_dir_all(&workspace).unwrap();
    let workspace_alias = workspace.join("../app[1]");
    for scope in [dir.path().to_path_buf(), workspace_alias] {
        let scope = path_str(&scope);
        let mut resolver = MutationPathResolver::new(&repo, &scope);
        assert!(resolver.roots.is_none());
        // Missing/deleted paths, literal brackets and spaces must remain
        // lexical Git paths, not filesystem-canonicalized file names.
        for path in [
            "",
            ".",
            "src/./old name.txt",
            "file[1].txt",
            "missing/deleted.txt",
        ] {
            let expected = repo_relative_path(&repo, &scope, path).unwrap();
            assert_eq!(resolver.resolve(path).unwrap(), expected);
        }
        assert!(resolver.roots.is_some());
        for path in ["../outside.txt", "src/../../outside.txt", "/absolute.txt"] {
            let expected = repo_relative_path(&repo, &scope, path).unwrap_err();
            let actual = resolver.resolve(path).unwrap_err();
            assert_eq!(
                (actual.kind, actual.context),
                (expected.kind, expected.context)
            );
        }
        #[cfg(windows)]
        for path in [r"C:relative.txt", r"C:\absolute.txt", r"\rooted.txt"] {
            assert!(resolver.resolve(path).is_err(), "{path}");
        }
    }

    let outside = tempfile::tempdir().unwrap();
    let outside = path_str(outside.path());
    let mut resolver = MutationPathResolver::new(&repo, &outside);
    for path in ["file.txt", "other.txt"] {
        let expected = repo_relative_path(&repo, &outside, path).unwrap_err();
        let actual = resolver.resolve(path).unwrap_err();
        assert_eq!(
            (actual.kind, actual.context),
            (expected.kind, expected.context)
        );
    }
}

#[test]
fn mutation_path_resolver_stays_lazy_for_empty_index_actions() {
    let dir = init_repo();
    let repo = Repository::open(dir.path()).unwrap();
    let missing = path_str(&dir.path().join("missing-workspace"));
    // These helpers previously did not resolve any paths for an empty
    // selection. Do not introduce a new canonicalization failure on no-ops.
    stage_status_entries(&repo, &missing, &[]).unwrap();
    unstage_status_entries(&repo, &missing, &[]).unwrap();
    let mut resolver = MutationPathResolver::new(&repo, &missing);
    assert!(resolver.roots.is_none());
    assert!(resolver.resolve("file.txt").is_err());
    assert!(resolver.roots.is_none());
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
    // Isolate the cost of path resolution with the same changed file list.
    let repo = open_repo(&path).unwrap();
    let entries = git_diff_impl::git_status_entries_only(&path, None).unwrap();
    let paths = entries
        .iter()
        .filter(|entry| entry.area == GitChangeArea::Unstaged)
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 160);
    let expected_paths = paths
        .iter()
        .map(|p| repo_relative_path(&repo, &path, p).unwrap())
        .collect::<Vec<_>>();
    let measure_paths = |cached: bool| {
        let start = Instant::now();
        let mut resolver = MutationPathResolver::new(&repo, &path);
        let result = paths
            .iter()
            .map(|p| {
                if cached {
                    resolver.resolve(p).unwrap()
                } else {
                    repo_relative_path(&repo, &path, p).unwrap()
                }
            })
            .collect::<Vec<_>>();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(result, expected_paths);
        elapsed
    };
    let mut old_paths = Vec::new();
    let mut new_paths = Vec::new();
    measure_paths(false);
    measure_paths(true);
    for run in 0..7 {
        if run % 2 == 0 {
            old_paths.push(measure_paths(false));
            new_paths.push(measure_paths(true));
        } else {
            new_paths.push(measure_paths(true));
            old_paths.push(measure_paths(false));
        }
    }
    old_paths.sort_by(f64::total_cmp);
    new_paths.sort_by(f64::total_cmp);
    println!("Path resolution (160 paths): repeated roots median {:.3} ms, request-local roots median {:.3} ms", old_paths[3], new_paths[3]);
    println!("Path resolution repeated ms {old_paths:?}; request-local ms {new_paths:?}");
    drop(repo);

    let measure = |stage: bool, variant: usize| {
        reset_index();
        let start = Instant::now();
        if variant == 2 {
            // Phase 11 production entry point.
            if stage {
                git_stage(path.clone(), None).unwrap();
            } else {
                git_unstage(path.clone(), None).unwrap();
            }
        } else {
            // Phase 9 and Phase 10 both canonicalized the two roots for
            // every resolved path. Preserve this exact baseline using
            // the same mutation core and the original path helper.
            let repo = open_repo(&path).unwrap();
            let entries = if variant == 0 {
                git_status(path.clone()).unwrap().entries
            } else {
                git_diff_impl::git_status_entries_only(&path, None).unwrap()
            };
            let resolve = |p: &str| repo_relative_path(&repo, &path, p);
            if stage {
                stage_status_entries_with_resolver(&repo, &entries, resolve).unwrap();
            } else {
                unstage_status_entries_with_resolver(&repo, &entries, resolve).unwrap();
            }
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        verify_index(if stage { all_staged_oid } else { baseline_oid });
        elapsed
    };
    for (label, stage) in [("Stage all", true), ("Unstage all", false)] {
        // Warm every path; exclude setup and index/content verification.
        for variant in 0..3 {
            measure(stage, variant);
        }
        let mut samples = [Vec::new(), Vec::new(), Vec::new()];
        // Collect the older full-status baseline separately. Alternate the
        // two adjacent implementations directly rather than interposing a
        // substantially more expensive full-status operation.
        for _ in 0..7 {
            samples[0].push(measure(stage, 0));
        }
        for run in 0..7 {
            let order = if run % 2 == 0 { [1, 2] } else { [2, 1] };
            for variant in order {
                samples[variant].push(measure(stage, variant));
            }
        }
        for sample in &mut samples {
            sample.sort_by(f64::total_cmp);
        }
        println!("{label}: 512 tracked files, 160 modified (80 also staged), 240 entries, seven alternating runs; full status median {:.3} ms, repeated roots median {:.3} ms, request-local roots median {:.3} ms", samples[0][3], samples[1][3], samples[2][3]);
        println!(
            "{label} full status ms {:?}; repeated roots ms {:?}; request-local roots ms {:?}",
            samples[0], samples[1], samples[2]
        );
    }
}

fn assert_head_index_entry_eq(actual: Option<IndexEntry>, expected: Option<IndexEntry>) {
    match (actual, expected) {
        (Some(actual), Some(expected)) => {
            assert_eq!(actual.path, expected.path);
            assert_eq!(actual.id, expected.id);
            assert_eq!(actual.mode, expected.mode);
            assert_eq!(actual.flags, expected.flags);
            assert_eq!(actual.flags_extended, expected.flags_extended);
            assert_eq!(
                (actual.ctime.seconds(), actual.ctime.nanoseconds()),
                (expected.ctime.seconds(), expected.ctime.nanoseconds())
            );
            assert_eq!(
                (actual.mtime.seconds(), actual.mtime.nanoseconds()),
                (expected.mtime.seconds(), expected.mtime.nanoseconds())
            );
            assert_eq!(
                (
                    actual.dev,
                    actual.ino,
                    actual.uid,
                    actual.gid,
                    actual.file_size
                ),
                (
                    expected.dev,
                    expected.ino,
                    expected.uid,
                    expected.gid,
                    expected.file_size
                )
            );
        }
        (None, None) => {}
        (actual, expected) => panic!("HEAD lookup mismatch: {actual:?} != {expected:?}"),
    }
}

#[test]
fn direct_head_lookup_matches_every_detached_index_field() {
    let dir = init_repo();
    let repo = Repository::open(dir.path()).unwrap();
    let blob = repo.blob(b"contents").unwrap();
    let link = repo.blob(b"target.txt").unwrap();
    let commit = repo.head().unwrap().target().unwrap();
    let mut nested = repo.treebuilder(None).unwrap();
    nested.insert("file[1].txt", blob, 0o100644).unwrap();
    nested.insert("run.sh", blob, 0o100755).unwrap();
    nested.insert("symbolic", link, 0o120000).unwrap();
    let nested_id = nested.write().unwrap();
    let mut root = repo.treebuilder(None).unwrap();
    root.insert("nested", nested_id, 0o040000).unwrap();
    root.insert("target.txt", blob, 0o100644).unwrap();
    root.insert("Case.txt", blob, 0o100644).unwrap();
    root.insert("gitlink", commit, 0o160000).unwrap();
    let tree_id = root.write().unwrap();
    let mut full = HeadIndexEntries::new(repo.find_tree(tree_id).unwrap(), false).unwrap();
    let mut direct = HeadIndexEntries::new(repo.find_tree(tree_id).unwrap(), true).unwrap();
    for path in [
        "target.txt",
        "Case.txt",
        "case.txt",
        "nested/file[1].txt",
        "nested/run.sh",
        "nested/symbolic",
        "gitlink",
        "nested",
        "nested/missing.txt",
        "missing.txt",
        "target.txt/child",
    ] {
        assert_head_index_entry_eq(direct.get(path).unwrap(), full.get(path).unwrap());
        assert!(
            direct.full_index.is_none(),
            "unexpected fallback for {path}"
        );
    }
    assert_eq!(direct.get("nested/run.sh").unwrap().unwrap().mode, 0o100755);
    assert_eq!(
        direct.get("nested/symbolic").unwrap().unwrap().mode,
        0o120000
    );
    assert_eq!(direct.get("gitlink").unwrap().unwrap().mode, 0o160000);
    assert!(full.full_index.is_some());
}

#[test]
fn direct_head_lookup_falls_back_for_noncanonical_raw_modes() {
    let dir = init_repo();
    let repo = Repository::open(dir.path()).unwrap();
    let blob = repo.blob(b"legacy contents").unwrap();
    // A historical noncanonical mode is possible in an existing tree even
    // though the modern TreeBuilder only accepts canonical modes.
    let mut raw = b"100664 legacy.txt\0".to_vec();
    raw.extend_from_slice(blob.as_bytes());
    let tree_id = repo.odb().unwrap().write(ObjectType::Tree, &raw).unwrap();
    let mut full = HeadIndexEntries::new(repo.find_tree(tree_id).unwrap(), false).unwrap();
    let mut direct = HeadIndexEntries::new(repo.find_tree(tree_id).unwrap(), true).unwrap();
    assert_head_index_entry_eq(
        direct.get("legacy.txt").unwrap(),
        full.get("legacy.txt").unwrap(),
    );
    assert!(direct.full_index.is_some());
}

#[test]
fn unstage_small_selection_preserves_rename_deletion_and_untracked_contents() {
    let dir = init_repo();
    let root = dir.path();
    std::fs::create_dir(root.join("src")).unwrap();
    for file in [
        "src/old.txt",
        "src/delete.txt",
        "src/file[1].txt",
        "src/other.txt",
    ] {
        std::fs::write(root.join(file), format!("baseline {file}\n")).unwrap();
    }
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "baseline"]);
    run_git(root, &["mv", "src/old.txt", "src/new.txt"]);
    std::fs::remove_file(root.join("src/delete.txt")).unwrap();
    std::fs::write(root.join("src/file[1].txt"), "changed\n").unwrap();
    std::fs::write(root.join("src/added.txt"), "added\n").unwrap();
    std::fs::write(root.join("src/other.txt"), "outside selection\n").unwrap();
    run_git(root, &["add", "src"]);
    let repo = Repository::open(root).unwrap();
    let entries = git_diff_impl::git_status_entries_only(&path_str(root), None).unwrap();
    let selected: Vec<_> = entries
        .into_iter()
        .filter(|entry| entry.path != "src/other.txt")
        .collect();
    let staged_tree = repo.index().unwrap().write_tree().unwrap();
    let run = |limit| {
        let repo = Repository::open(root).unwrap();
        let tree = repo.find_tree(staged_tree).unwrap();
        let mut index = repo.index().unwrap();
        index.clear().unwrap();
        index.read_tree(&tree).unwrap();
        index.write().unwrap();
        drop(index);
        let workspace = path_str(root);
        let mut paths = MutationPathResolver::new(&repo, &workspace);
        // Hold the workspace string for the whole operation.
        unstage_status_entries_with_lookup(&repo, &selected, |p| paths.resolve(p), limit).unwrap();
        let mut index = repo.index().unwrap();
        let result_tree = index.write_tree().unwrap();
        let entries = index.iter().collect::<Vec<_>>();
        (result_tree, entries)
    };
    let (expected_tree, expected_entries) = run(0);
    let (actual_tree, actual_entries) = run(MAX_DIRECT_HEAD_PATHS);
    assert_eq!(actual_tree, expected_tree);
    assert_eq!(actual_entries.len(), expected_entries.len());
    for (actual, expected) in actual_entries.into_iter().zip(expected_entries) {
        assert_head_index_entry_eq(Some(actual), Some(expected));
    }
    let index = Repository::open(root).unwrap().index().unwrap();
    assert!(index.get_path(Path::new("src/old.txt"), 0).is_some());
    assert!(index.get_path(Path::new("src/new.txt"), 0).is_none());
    assert!(index.get_path(Path::new("src/delete.txt"), 0).is_some());
    assert!(index.get_path(Path::new("src/added.txt"), 0).is_none());
    assert!(!root.join("src/old.txt").exists());
    assert!(!root.join("src/delete.txt").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("src/new.txt")).unwrap(),
        "baseline src/old.txt\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/added.txt")).unwrap(),
        "added\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/file[1].txt")).unwrap(),
        "changed\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/other.txt")).unwrap(),
        "outside selection\n"
    );
}

#[test]
fn unstage_added_file_before_first_commit_does_not_remove_worktree_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("first.txt"), "first\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("first.txt")).unwrap();
    index.write().unwrap();
    drop(index);
    git_unstage(path_str(dir.path()), Some("first.txt".into())).unwrap();
    assert!(Repository::open(dir.path())
        .unwrap()
        .index()
        .unwrap()
        .is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("first.txt")).unwrap(),
        "first\n"
    );
}

#[test]
#[ignore = "large HEAD tree lookup and index mutation benchmark; run explicitly"]
fn benchmark_sparse_unstage_head_lookup() {
    use std::time::Instant;
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let baseline_blob = repo.blob(b"baseline").unwrap();
    let staged_blob = repo.blob(b"staged").unwrap();
    // A real HEAD tree/index, not a mock, with 20,000 tracked entries.
    // Avoid checking out all 20,000 files: this benchmark times the HEAD
    // lookup and native index mutation, not worktree status discovery.
    let mut subtree = repo.treebuilder(None).unwrap();
    for i in 0..200 {
        subtree
            .insert(format!("file-{i:04}.txt"), baseline_blob, 0o100644)
            .unwrap();
    }
    let subtree_id = subtree.write().unwrap();
    let mut root = repo.treebuilder(None).unwrap();
    for i in 0..100 {
        root.insert(format!("dir-{i:03}"), subtree_id, 0o040000)
            .unwrap();
    }
    let baseline_id = root.write().unwrap();
    let baseline = repo.find_tree(baseline_id).unwrap();
    let sig = git2::Signature::now("Benchmark", "benchmark@example.invalid").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "baseline", &baseline, &[])
        .unwrap();
    let path = path_str(dir.path());
    let selected_paths = (0..8)
        .map(|i| format!("dir-{:03}/file-{:04}.txt", i * 11, i * 13))
        .collect::<Vec<_>>();
    let mut index = repo.index().unwrap();
    index.read_tree(&baseline).unwrap();
    assert_eq!(index.len(), 20_000);
    let mut selected = Vec::new();
    for p in &selected_paths {
        let mut entry = index.get_path(Path::new(p), 0).unwrap();
        entry.id = staged_blob;
        index.add(&entry).unwrap();
        selected.push(GitChangeEntry {
            path: p.clone(),
            old_path: None,
            area: GitChangeArea::Staged,
            status: GitChangeStatus::Modified,
            added: None,
            removed: None,
            is_binary: false,
            is_large: false,
            submodule: None,
        });
    }
    index.write().unwrap();
    let staged_id = index.write_tree().unwrap();
    drop(index);
    let measure_lookup = |direct: bool| {
        let tree = repo.find_tree(baseline_id).unwrap();
        let start = Instant::now();
        let mut lookup = HeadIndexEntries::new(tree, direct).unwrap();
        let found = selected_paths
            .iter()
            .map(|path| lookup.get(path).unwrap().unwrap())
            .collect::<Vec<_>>();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        for (entry, path) in found.iter().zip(&selected_paths) {
            assert_eq!(entry.id, baseline_blob);
            assert_eq!(entry.path, path.as_bytes());
        }
        assert_eq!(lookup.full_index.is_none(), direct);
        elapsed
    };
    let measure_mutation = |direct: bool| {
        let repo = Repository::open(dir.path()).unwrap();
        let staged_tree = repo.find_tree(staged_id).unwrap();
        let mut index = repo.index().unwrap();
        index.read_tree(&staged_tree).unwrap();
        index.write().unwrap();
        drop(index);
        let mut paths = MutationPathResolver::new(&repo, &path);
        let start = Instant::now();
        unstage_status_entries_with_lookup(
            &repo,
            &selected,
            |p| paths.resolve(p),
            if direct { MAX_DIRECT_HEAD_PATHS } else { 0 },
        )
        .unwrap();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let mut index = repo.index().unwrap();
        assert_eq!(index.len(), 20_000);
        assert_eq!(index.write_tree().unwrap(), baseline_id);
        elapsed
    };
    for (label, measure) in [
        (
            "HEAD lookup (8 of 20000 paths)",
            &measure_lookup as &dyn Fn(bool) -> f64,
        ),
        (
            "Unstage native index mutation (8 of 20000 paths)",
            &measure_mutation as &dyn Fn(bool) -> f64,
        ),
    ] {
        measure(false);
        measure(true);
        let mut old = Vec::new();
        let mut new = Vec::new();
        for run in 0..7 {
            if run % 2 == 0 {
                old.push(measure(false));
                new.push(measure(true));
            } else {
                new.push(measure(true));
                old.push(measure(false));
            }
        }
        old.sort_by(f64::total_cmp);
        new.sort_by(f64::total_cmp);
        println!(
            "{label}: full index median {:.3} ms, direct tree median {:.3} ms",
            old[3], new[3]
        );
        println!("{label}: old {old:?}; new {new:?}");
    }
}
