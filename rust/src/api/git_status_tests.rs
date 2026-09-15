use super::*;

#[test]
fn operates_from_subdirectory() {
    let repo = init_repo();
    let subdir = repo.path().join("nested").join("dir");
    std::fs::create_dir_all(&subdir).expect("create subdir");

    let subdir_path = path_str(&subdir);
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
