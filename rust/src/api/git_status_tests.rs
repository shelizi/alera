use super::*;

#[test]
fn operates_from_subdirectory() {
    let repo = init_repo();
    let subdir = repo.path().join("nested").join("dir");
    std::fs::create_dir_all(&subdir).expect("create subdir");

    let subdir_path = path_str(&subdir);
    let repository_root = git_repository_root(subdir_path.clone())
        .unwrap()
        .expect("repository root");
    assert_eq!(
        canonical(&repository_root),
        canonical(&path_str(repo.path()))
    );
    assert!(is_git_repository(subdir_path.clone()).unwrap());
    assert_eq!(current_branch(subdir_path.clone()).unwrap(), "main");
    assert!(list_branches(subdir_path)
        .unwrap()
        .contains(&"main".to_string()));
}

#[test]
fn status_preserves_punctuation_and_control_character_paths() {
    let repo = init_repo();
    let comma_path = repo.path().join("comma,name.txt");
    #[cfg(unix)]
    let newline_path = repo.path().join("line\nbreak.txt");
    std::fs::write(&comma_path, "comma\n").expect("write comma path");
    #[cfg(unix)]
    std::fs::write(&newline_path, "newline\n").expect("write newline path");

    let status = git_status(path_str(repo.path())).unwrap();

    assert!(status.entries.iter().any(|entry| {
        entry.path == "comma,name.txt"
            && entry.area == GitChangeArea::Untracked
            && entry.status == GitChangeStatus::Untracked
    }));
    #[cfg(unix)]
    assert!(status.entries.iter().any(|entry| {
        entry.path == "line\nbreak.txt"
            && entry.area == GitChangeArea::Untracked
            && entry.status == GitChangeStatus::Untracked
    }));
}

#[test]
fn concurrent_status_refreshes_do_not_share_mutable_probe_state() {
    let repo = init_repo();
    std::fs::write(repo.path().join("changed.txt"), "changed\n").expect("write changed");
    let repo_path = path_str(repo.path());

    let handles = (0..8)
        .map(|_| {
            let path = repo_path.clone();
            std::thread::spawn(move || git_status(path).unwrap())
        })
        .collect::<Vec<_>>();

    for handle in handles {
        let status = handle.join().expect("status thread joins");
        assert!(status.entries.iter().any(|entry| {
            entry.path == "changed.txt" && entry.status == GitChangeStatus::Untracked
        }));
    }
}

#[test]
fn git_status_splits_untracked_unstaged_and_staged_changes() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\n").expect("write staged");
    run_git(repo.path(), &["add", "README.md"]);
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\nunstaged\n")
        .expect("write unstaged");
    std::fs::write(repo.path().join("new.txt"), "new\nfile\n").expect("write untracked");

    let status = git_status(path_str(repo.path())).unwrap();

    assert!(status.entries.iter().any(|entry| {
        entry.path == "README.md"
            && entry.area == GitChangeArea::Staged
            && entry.status == GitChangeStatus::Modified
            && entry.added.unwrap_or(0) >= 1
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "README.md"
            && entry.area == GitChangeArea::Unstaged
            && entry.status == GitChangeStatus::Modified
            && entry.added == Some(1)
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "new.txt"
            && entry.area == GitChangeArea::Untracked
            && entry.status == GitChangeStatus::Untracked
            && entry.added.is_none()
            && entry.removed == Some(0)
    }));
    // Group projections reference the flat entries by index, so changes still
    // cross the bridge only once while native code owns area grouping/sorting.
    assert_eq!(status.groups.len(), 3);
    assert_eq!(
        status
            .entries
            .iter()
            .map(|entry| entry.area)
            .collect::<Vec<_>>(),
        vec![
            GitChangeArea::Untracked,
            GitChangeArea::Unstaged,
            GitChangeArea::Staged,
        ]
    );
}

#[test]
fn git_status_projects_area_groups_as_sorted_entry_indices() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\n").expect("write staged");
    run_git(repo.path(), &["add", "README.md"]);
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\nunstaged\n")
        .expect("write unstaged");
    std::fs::write(repo.path().join("z-last.txt"), "z\n").expect("write z");
    std::fs::write(repo.path().join("a-first.txt"), "a\n").expect("write a");

    let status = git_status(path_str(repo.path())).unwrap();

    assert_eq!(status.groups.len(), 3);
    assert_eq!(status.groups[0].area, GitChangeArea::Staged);
    assert_eq!(status.groups[1].area, GitChangeArea::Unstaged);
    assert_eq!(status.groups[2].area, GitChangeArea::Untracked);

    let paths_for = |group: &GitChangeGroup| {
        group
            .entry_indices
            .iter()
            .map(|index| status.entries[*index as usize].path.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(paths_for(&status.groups[0]), vec!["README.md"]);
    assert_eq!(paths_for(&status.groups[1]), vec!["README.md"]);
    assert_eq!(
        paths_for(&status.groups[2]),
        vec!["a-first.txt", "z-last.txt"]
    );
}

#[cfg(unix)]
#[test]
fn git_status_lists_unreadable_untracked_files_without_reading_content() {
    use std::os::unix::fs::PermissionsExt;

    let repo = init_repo();
    let unreadable = repo.path().join("unreadable.txt");
    std::fs::write(&unreadable, "unreadable\ncontent\n").expect("write unreadable file");
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000))
        .expect("make unreadable");

    let status = git_status(path_str(repo.path())).unwrap();

    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644))
        .expect("restore permissions");
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path == "unreadable.txt")
        .expect("unreadable untracked entry");
    assert_eq!(entry.area, GitChangeArea::Untracked);
    assert!(entry.added.is_none());
    assert_eq!(entry.removed, Some(0));
}

#[test]
fn git_status_for_path_limits_results_to_selected_file() {
    let repo = init_repo();
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\n").expect("modify readme");
    run_git(repo.path(), &["add", "README.md"]);
    std::fs::write(repo.path().join("README.md"), "hello\nstaged\nunstaged\n")
        .expect("modify readme again");
    std::fs::write(repo.path().join("unrelated.txt"), "unrelated\n").expect("write unrelated");

    let status = git_status_for_path(path_str(repo.path()), "README.md".to_string()).unwrap();

    assert_eq!(status.entries.len(), 2);
    assert!(status.entries.iter().all(|entry| entry.path == "README.md"));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.area == GitChangeArea::Staged));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.area == GitChangeArea::Unstaged));
    assert!(!status
        .entries
        .iter()
        .any(|entry| entry.path == "unrelated.txt"));
}

#[test]
fn git_status_keeps_group_projection_entries_index_only() {
    let repo = init_repo();
    std::fs::create_dir_all(repo.path().join("lib/src")).expect("create lib src");
    std::fs::write(repo.path().join("lib/src/a.dart"), "a\n").expect("write a");
    std::fs::write(repo.path().join("lib/b.dart"), "b\n").expect("write b");

    let status = git_status(path_str(repo.path())).unwrap();

    // Native grouping and tree rows reference the flat entries by index and do
    // not clone GitChangeEntry payloads.
    assert_eq!(status.groups.len(), 1);
    assert_eq!(status.groups[0].area, GitChangeArea::Untracked);
    assert_eq!(status.groups[0].entry_indices, vec![0, 1]);
    assert!(status.groups[0]
        .tree_rows
        .iter()
        .filter(|row| row.kind == GitChangeTreeRowKind::File)
        .all(|row| row.entry_index.is_some()));
    assert_eq!(
        status
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec!["lib/b.dart", "lib/src/a.dart"]
    );
    assert!(status
        .entries
        .iter()
        .all(|entry| entry.area == GitChangeArea::Untracked));
}

#[test]
fn git_status_projects_tree_rows_as_entry_indices() {
    let repo = init_repo();
    std::fs::create_dir_all(repo.path().join("lib/src/nested")).expect("create nested dirs");
    for (path, contents) in [
        ("z.txt", "z\n"),
        ("a.txt", "a\n"),
        ("lib/src/nested/c.dart", "c\n"),
        ("lib/src/b.dart", "b\n"),
        ("lib/a.dart", "lib a\n"),
    ] {
        std::fs::write(repo.path().join(path), contents).expect("write tree fixture");
    }

    let status = git_status(path_str(repo.path())).unwrap();
    let group = status
        .groups
        .iter()
        .find(|group| group.area == GitChangeArea::Untracked)
        .expect("untracked group");

    let row_labels = group
        .tree_rows
        .iter()
        .map(|row| {
            let entry_path = row
                .entry_index
                .map(|index| status.entries[index as usize].path.as_str())
                .unwrap_or("-");
            format!(
                "{:?}:{}:{}:{}:{}:{}",
                row.kind, row.depth, row.path, row.file_count, row.name, entry_path
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        row_labels,
        vec![
            "Directory:0:lib:3:lib:-",
            "Directory:1:lib/src:2:src:-",
            "Directory:2:lib/src/nested:1:nested:-",
            "File:3:lib/src/nested/c.dart:1:c.dart:lib/src/nested/c.dart",
            "File:2:lib/src/b.dart:1:b.dart:lib/src/b.dart",
            "File:1:lib/a.dart:1:a.dart:lib/a.dart",
            "File:0:a.txt:1:a.txt:a.txt",
            "File:0:z.txt:1:z.txt:z.txt",
        ]
    );
}

#[test]
fn git_status_uses_workspace_relative_paths_for_subdirectories() {
    let repo = init_repo();
    let app_dir = repo.path().join("packages").join("app");
    let lib_dir = app_dir.join("lib");
    std::fs::create_dir_all(&lib_dir).expect("create app lib dir");
    std::fs::write(lib_dir.join("foo.dart"), "void main() {}\n").expect("write app file");
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add app"]);

    std::fs::write(
        lib_dir.join("foo.dart"),
        "void main() {\n  print('changed');\n}\n",
    )
    .expect("modify app file");
    std::fs::write(repo.path().join("README.md"), "root changed\n").expect("modify root file");

    let status = git_status(path_str(&app_dir)).unwrap();

    assert_eq!(status.entries.len(), 1);
    assert_eq!(status.entries[0].path, "lib/foo.dart");
    assert_eq!(status.entries[0].area, GitChangeArea::Unstaged);

    let diff = git_diff(
        path_str(&app_dir),
        "lib/foo.dart".to_string(),
        GitChangeArea::Unstaged,
    )
    .unwrap();
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "lib/foo.dart");
    assert!(diff_text(&diff.files[0]).contains("changed"));
}

#[test]
fn git_status_line_counts_match_rendered_diffs_without_preview_limits() {
    let repo = init_repo();
    for (path, bytes) in [
        ("ascii.txt", b"old\n\nlast".as_slice()),
        ("legacy.txt", b"old\n".as_slice()),
        ("large.txt", b"old\n".as_slice()),
        ("binary.bin", b"\x00\x01\x02\x03".as_slice()),
        ("rename.txt", b"unchanged rename\n".as_slice()),
        ("deleted.txt", b"deleted without newline".as_slice()),
    ] {
        std::fs::write(repo.path().join(path), bytes).expect("write baseline");
    }
    run_git(repo.path(), &["add", "."]);
    run_git(repo.path(), &["commit", "-m", "add line count fixtures"]);
    run_git(repo.path(), &["mv", "rename.txt", "renamed.txt"]);
    std::fs::write(repo.path().join("empty.txt"), b"").expect("add empty file");
    std::fs::write(repo.path().join("ascii.txt"), b"stage\n\nlast").expect("stage ASCII");
    run_git(repo.path(), &["add", "empty.txt", "ascii.txt"]);
    std::fs::write(repo.path().join("ascii.txt"), b"stage\n\nnew last").expect("modify ASCII");
    let (legacy, _, errors) = encoding_rs::BIG5.encode("\u{65b0}\u{5167}\u{5bb9}\n");
    assert!(!errors);
    std::fs::write(repo.path().join("legacy.txt"), legacy.as_ref()).expect("write Big5");
    let large = (0..7000)
        .map(|i| format!("\u{65b0}{i}\n"))
        .collect::<String>();
    std::fs::write(repo.path().join("large.txt"), large).expect("write large Unicode");
    std::fs::write(repo.path().join("binary.bin"), b"\x00\x04\x05\x06").expect("modify binary");
    std::fs::remove_file(repo.path().join("deleted.txt")).expect("delete fixture");
    let root = path_str(repo.path());
    let status = git_status(root.clone()).expect("status");
    assert_eq!(status, legacy_git_status_for_test(root.clone()).unwrap());
    assert_eq!(status.entries.len(), 8);
    for entry in &status.entries {
        let result = git_diff(root.clone(), entry.path.clone(), entry.area).expect("render diff");
        let file = result.files.first().expect("visible file");
        if file.is_binary {
            assert_eq!((entry.added, entry.removed), (None, None), "{}", entry.path);
        } else {
            assert_eq!(
                (entry.added, entry.removed),
                (file.added, file.removed),
                "{}",
                entry.path
            );
        }
        if entry.path == "large.txt" {
            assert_eq!(entry.added, Some(7000));
            assert_eq!(entry.removed, Some(1));
            assert!(file.line_preview_truncated);
        }
        if entry.path == "renamed.txt" || entry.path == "empty.txt" {
            assert_eq!((entry.added, entry.removed), (Some(0), Some(0)));
        }
    }
}

#[test]
fn batched_status_matches_legacy_across_chunks_and_workspace_scopes() {
    let repo = init_repo();
    let root = repo.path();
    std::fs::create_dir_all(root.join("src/nested")).unwrap();
    std::fs::create_dir_all(root.join("other")).unwrap();
    for i in 0..72 {
        std::fs::write(
            root.join(format!("src/nested/file_{i:03}.txt")),
            format!("baseline {i}\nlast"),
        )
        .unwrap();
    }
    for name in [
        "src/rename.txt",
        "src/delete.txt",
        "src/file[1].txt",
        "src/file1.txt",
        "other/outside.txt",
    ] {
        std::fs::write(root.join(name), "baseline\n").unwrap();
    }
    std::fs::write(root.join(".gitignore"), "*.ignored\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "fixture"]);
    for i in 0..72 {
        std::fs::write(
            root.join(format!("src/nested/file_{i:03}.txt")),
            format!("baseline {i}\nnew staged line\nlast"),
        )
        .unwrap();
    }
    std::fs::write(root.join("src/file[1].txt"), "literal staged\n").unwrap();
    std::fs::write(root.join("src/staged.txt"), "new staged\n").unwrap();
    run_git(root, &["mv", "src/rename.txt", "src/renamed.txt"]);
    run_git(root, &["add", "src"]);
    for i in 0..72 {
        std::fs::write(
            root.join(format!("src/nested/file_{i:03}.txt")),
            format!("baseline {i}\nnew staged line\nnew worktree line\nlast"),
        )
        .unwrap();
    }
    std::fs::write(root.join("src/file[1].txt"), "literal unstaged\n").unwrap();
    std::fs::write(root.join("src/file1.txt"), "different literal\n").unwrap();
    std::fs::write(root.join("other/outside.txt"), "outside edit\n").unwrap();
    std::fs::remove_file(root.join("src/delete.txt")).unwrap();
    std::fs::write(root.join("src/untracked.txt"), "untracked").unwrap();
    std::fs::write(root.join("src/hidden.ignored"), "ignored").unwrap();

    for scope in [
        root.to_path_buf(),
        root.join("src"),
        root.join("src/nested"),
    ] {
        let path = path_str(&scope);
        let expected = legacy_git_status_for_test(path.clone()).unwrap();
        let actual = git_status(path).unwrap();
        assert_eq!(actual, expected, "scope {scope:?}");
        let normal_count = actual
            .entries
            .iter()
            .filter(|entry| {
                entry.path.contains("file_") && entry.status == GitChangeStatus::Modified
            })
            .count();
        assert_eq!(normal_count, 144);
    }

    // The plan is request-local, so changes after a previous request cannot
    // return a retained line-count snapshot.
    std::fs::write(root.join("src/nested/file_000.txt"), "replaced\n").unwrap();
    let path = path_str(root);
    assert_eq!(
        git_status(path.clone()).unwrap(),
        legacy_git_status_for_test(path).unwrap()
    );
}

#[test]
#[ignore = "real repository timing benchmark; run explicitly in isolation"]
fn benchmark_batched_status_line_stats() {
    use std::time::Instant;
    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let mut index = repo.index().unwrap();
    let name = |i: usize| format!("src/file-{i:04}.txt");
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    for i in 0..512 {
        let file = name(i);
        std::fs::write(dir.path().join(&file), format!("original {i}\n").repeat(80)).unwrap();
        index.add_path(Path::new(&file)).unwrap();
    }
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = git2::Signature::now("Status Benchmark", "benchmark@example.invalid").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "baseline", &tree, &[])
        .unwrap();
    drop(tree);
    for i in 0..160 {
        let file = name(i);
        std::fs::write(
            dir.path().join(&file),
            format!("original {i}\n").repeat(70) + "staged line\n",
        )
        .unwrap();
        if i < 80 {
            index.add_path(Path::new(&file)).unwrap();
        }
        std::fs::write(
            dir.path().join(&file),
            format!("original {i}\n").repeat(70) + "staged line\nworktree line\n",
        )
        .unwrap();
    }
    index.write().unwrap();
    drop(index);
    drop(repo);

    let path = path_str(dir.path());
    let old = || legacy_git_status_for_test(path.clone()).unwrap();
    let new = || git_status(path.clone()).unwrap();
    let expected = old();
    assert_eq!(expected.entries.len(), 240);
    assert_eq!(new(), expected);
    let timed = |f: &dyn Fn() -> GitStatusResult| {
        let start = Instant::now();
        let result = f();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(result, expected);
        ms
    };
    let mut before = Vec::new();
    let mut after = Vec::new();
    for i in 0..7 {
        if i % 2 == 0 {
            before.push(timed(&old));
            after.push(timed(&new));
        } else {
            after.push(timed(&new));
            before.push(timed(&old));
        }
    }
    before.sort_by(f64::total_cmp);
    after.sort_by(f64::total_cmp);
    println!("Source Control status: 512 tracked files, 160 modified (80 also staged), 240 entries; seven alternating runs; old median {:.3} ms, new median {:.3} ms", before[3], after[3]);
    println!("old ms {before:?}; new ms {after:?}");
}
