use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use super::*;

fn test_scope() -> QuickOpenScope {
    QuickOpenScope {
        root: PathBuf::new(),
        include_internal_symlinks: true,
        exclusions: QuickOpenExclusions::new(Vec::new()),
    }
}

fn start(workspace: &tempfile::TempDir) -> WorkspaceQuickOpenSession {
    start_with_exclusions(workspace, Vec::new())
}

fn start_with_exclusions(
    workspace: &tempfile::TempDir,
    excluded_directories: Vec<String>,
) -> WorkspaceQuickOpenSession {
    start_workspace_quick_open_session(
        workspace.path().to_string_lossy().into_owned(),
        excluded_directories,
    )
    .unwrap()
}

fn paths(session: &WorkspaceQuickOpenSession, query: &str, limit: u32) -> Vec<String> {
    search_workspace_quick_open_session(session.clone(), query.to_string(), limit, false)
        .unwrap()
        .into_iter()
        .map(|item| item.relative_path)
        .collect()
}

fn paths_including_gitignored(
    session: &WorkspaceQuickOpenSession,
    query: &str,
    limit: u32,
) -> Vec<String> {
    search_workspace_quick_open_session(session.clone(), query.to_string(), limit, true)
        .unwrap()
        .into_iter()
        .map(|item| item.relative_path)
        .collect()
}

fn snapshot(session: &WorkspaceQuickOpenSession) -> Arc<QuickOpenIndex> {
    access_session(&mut sessions().lock().unwrap(), &session.id, Instant::now()).unwrap()
}

fn index_fingerprint(session: &WorkspaceQuickOpenSession) -> Vec<(String, bool, bool)> {
    snapshot(session)
        .files
        .iter()
        .map(|file| {
            (
                file.relative_path.clone(),
                file.is_gitignored,
                file.is_directory,
            )
        })
        .collect()
}

#[test]
fn subtree_delta_matches_full_discovery_and_preserves_independent_sessions() {
    let workspace = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(workspace.path()).unwrap();
    for folder in ["src/deep", "lib/nested", "other", "node_modules"] {
        fs::create_dir_all(root.join(folder)).unwrap();
    }
    fs::write(
        root.join(".gitignore"),
        "src/deep/*.tmp\n!src/deep/keep.tmp\nlib/nested/hidden/\n",
    )
    .unwrap();
    fs::write(root.join("src/.ignore"), "private.txt\n").unwrap();
    fs::write(root.join("src/deep/old.rs"), "old").unwrap();
    fs::write(root.join("src/deep/keep.tmp"), "old").unwrap();
    fs::write(root.join("lib/nested/untouched.rs"), "old").unwrap();
    fs::write(root.join("other/untouched.rs"), "old").unwrap();
    fs::write(root.join("node_modules/skip.rs"), "old").unwrap();
    let journal = QuickOpenWatchJournal::new(root.clone());
    journal.activate();
    let initial = start_with_exclusions(&workspace, vec!["node_modules".into()]);
    let initial_paths = index_fingerprint(&initial);
    fs::rename(
        root.join("src/deep/old.rs"),
        root.join("lib/nested/moved.rs"),
    )
    .unwrap();
    fs::remove_file(root.join("src/deep/keep.tmp")).unwrap();
    fs::write(root.join("src/deep/new.tmp"), "ignored").unwrap();
    fs::write(root.join("src/deep/keep.tmp"), "visible").unwrap();
    fs::create_dir(root.join("lib/nested/hidden")).unwrap();
    fs::write(root.join("lib/nested/hidden/x.rs"), "hidden").unwrap();
    fs::create_dir(root.join("src/deep/new-dir")).unwrap();
    fs::write(root.join("src/deep/new-dir/child.rs"), "new").unwrap();
    journal.record(&[
        root.join("src"),
        root.join("lib"),
        root.join("src/deep/old.rs"),
        root.join("lib/nested/moved.rs"),
        root.join("src/deep/keep.tmp"),
        root.join("src/deep/new.tmp"),
        root.join("lib/nested/hidden"),
        root.join("src/deep/new-dir"),
    ]);
    let refresh = |previous| {
        refresh_workspace_quick_open_session(
            root.to_string_lossy().into_owned(),
            vec!["node_modules".into()],
            previous,
        )
        .unwrap()
    };
    let (delta, incremental) = refresh(initial.clone());
    assert!(
        incremental,
        "ordinary localized changes should use subtree discovery"
    );
    let full = start_with_exclusions(&workspace, vec!["node_modules".into()]);
    assert_eq!(index_fingerprint(&delta), index_fingerprint(&full));
    assert_eq!(index_fingerprint(&initial), initial_paths);
    let old_index = snapshot(&initial);
    let next_index = snapshot(&delta);
    let old_other = old_index
        .files
        .iter()
        .find(|f| f.relative_path == "other/untouched.rs")
        .unwrap();
    let next_other = next_index
        .files
        .iter()
        .find(|f| f.relative_path == "other/untouched.rs")
        .unwrap();
    assert!(Arc::ptr_eq(old_other, next_other));
    for query in [
        "",
        "new",
        "moved",
        "tmp",
        "hidden",
        "rs",
        "nested",
        "untouched",
    ] {
        for limit in [1, 5, 100] {
            for ignored in [false, true] {
                for directories in [false, true] {
                    assert_eq!(
                        format!(
                            "{:?}",
                            ranking::search(&next_index, query, limit, ignored, directories)
                        ),
                        format!(
                            "{:?}",
                            ranking::search(&snapshot(&full), query, limit, ignored, directories)
                        ),
                    );
                }
            }
        }
    }
    // A control-file change invalidates all cursors, not only one parent.
    fs::write(root.join(".gitignore"), "other/\n").unwrap();
    journal.record(&[root.join(".gitignore")]);
    let (policy, incremental) = refresh(delta.clone());
    assert!(!incremental);
    assert!(!paths(&policy, "other/untouched.rs", 10).contains(&"other/untouched.rs".into()));
    // Root-level changes, overflow and retired watchers also force full.
    fs::write(root.join("root.rs"), "root").unwrap();
    journal.record(&[root.join("root.rs")]);
    let (root_edit, incremental) = refresh(policy.clone());
    assert!(!incremental);
    journal.invalidate();
    let (overflow, incremental) = refresh(root_edit.clone());
    assert!(!incremental);
    journal.deactivate();
    let (retired, incremental) = refresh(overflow.clone());
    assert!(!incremental);
    for session in [initial, delta, full, policy, root_edit, overflow, retired] {
        stop_workspace_quick_open_session(session);
    }
}

#[test]
fn late_journal_notification_allows_only_one_retry() {
    let workspace = tempfile::tempdir().unwrap();
    let journal = QuickOpenWatchJournal::new(workspace.path().to_path_buf());
    journal.activate();
    let mut attempts = 0;
    let result = retry_changed_journal(&journal, || {
        attempts += 1;
        if attempts == 1 {
            journal.record(&[workspace.path().join("src/deep")]);
            None
        } else {
            Some(42)
        }
    });
    assert_eq!(result, Some(42));
    assert_eq!(attempts, 2);
    attempts = 0;
    assert_eq!(
        retry_changed_journal::<()>(&journal, || {
            attempts += 1;
            None
        }),
        None
    );
    assert_eq!(attempts, 1);
    attempts = 0;
    assert_eq!(
        retry_changed_journal::<()>(&journal, || {
            attempts += 1;
            journal.record(&[workspace.path().join("src/deep")]);
            None
        }),
        None
    );
    assert_eq!(attempts, 2);
    journal.deactivate();
}

#[test]
fn incomplete_quota_snapshot_cannot_seed_incremental_discovery() {
    let workspace = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(workspace.path()).unwrap();
    fs::create_dir(root.join("src")).unwrap();
    for n in 0..12 {
        fs::write(root.join(format!("src/{n}.txt")), "x").unwrap();
    }
    let discovery = discover_paths(
        &root,
        3,
        10000,
        true,
        &QuickOpenExclusions::new(vec![]),
        1,
        &HashSet::new(),
        None,
    )
    .unwrap();
    assert!(!discovery.complete);
    assert_eq!(
        discovery.paths.iter().filter(|p| !p.is_directory).count(),
        3
    );
}

#[test]
#[ignore = "compare actual complete and incremental subtree index refresh"]
fn compare_quick_open_subtree_refresh() {
    let workspace = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(workspace.path()).unwrap();
    for module in 0..40 {
        let dir = root.join(format!("module_{module:03}/src"));
        fs::create_dir_all(&dir).unwrap();
        for n in 0..75 {
            fs::write(dir.join(format!("file_{n:03}.rs")), "x").unwrap();
        }
    }
    let journal = QuickOpenWatchJournal::new(root.clone());
    journal.activate();
    let mut samples = [Vec::new(), Vec::new()];
    for round in 0..7 {
        let initial = start(&workspace);
        let new_path = root.join(format!("module_005/src/added_{round}.rs"));
        fs::write(&new_path, "x").unwrap();
        journal.record(&[new_path.clone()]);
        let mut results = [None, None];
        for offset in 0..2 {
            let mode = (round + offset) % 2;
            let started = Instant::now();
            let session = if mode == 0 {
                start(&workspace)
            } else {
                let (session, incremental) = refresh_workspace_quick_open_session(
                    root.to_string_lossy().into_owned(),
                    vec![],
                    initial.clone(),
                )
                .unwrap();
                assert!(incremental);
                session
            };
            samples[mode].push(started.elapsed().as_secs_f64() * 1000.0);
            results[mode] = Some(index_fingerprint(&session));
            stop_workspace_quick_open_session(session);
        }
        assert_eq!(results[0], results[1]);
        stop_workspace_quick_open_session(initial);
        fs::remove_file(&new_path).unwrap();
        journal.record(&[new_path]);
    }
    for (mode, times) in ["full", "subtree"].into_iter().zip(samples.iter_mut()) {
        times.sort_by(f64::total_cmp);
        println!("QUICK_OPEN_SUBTREE files=3000 changed_subtree=75 mode={mode} median_ms={:.3} samples={times:?}", times[3]);
    }
    journal.deactivate();
}

#[test]
fn refreshed_snapshot_reuses_unchanged_records_without_stale_paths_or_flags() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(root.join("keep.txt"), "old").unwrap();
    fs::write(root.join("remove.txt"), "old").unwrap();
    fs::write(root.join("ignored.txt"), "old").unwrap();
    fs::create_dir(root.join("folder")).unwrap();
    let first = start(&workspace);
    let original = snapshot(&first);
    // Content-only changes must still perform discovery, but reuse the
    // entire verified ranking index when the path snapshot is identical.
    fs::write(root.join("keep.txt"), "new content").unwrap();
    let second = start(&workspace);
    let same = snapshot(&second);
    assert_ne!(first.id, second.id);
    assert!(Arc::ptr_eq(&original, &same));

    fs::rename(root.join("remove.txt"), root.join("added.txt")).unwrap();
    fs::write(root.join(".gitignore"), "keep.txt\n").unwrap();
    fs::remove_dir(root.join("folder")).unwrap();
    fs::write(root.join("folder"), "replaced directory").unwrap();
    let third = start(&workspace);
    let changed = snapshot(&third);
    assert!(!Arc::ptr_eq(&same, &changed));
    let find = |index: &QuickOpenIndex, path: &str| {
        index
            .files
            .iter()
            .find(|file| file.relative_path == path)
            .unwrap()
            .clone()
    };
    assert!(Arc::ptr_eq(
        &find(&same, ".gitignore"),
        &find(&changed, ".gitignore")
    ));
    assert!(!Arc::ptr_eq(
        &find(&same, "keep.txt"),
        &find(&changed, "keep.txt")
    ));
    assert!(!Arc::ptr_eq(
        &find(&same, "ignored.txt"),
        &find(&changed, "ignored.txt")
    ));
    assert!(!Arc::ptr_eq(
        &find(&same, "folder"),
        &find(&changed, "folder")
    ));
    assert_eq!(paths(&third, "added.txt", 10), ["added.txt"]);
    assert!(paths(&third, "remove.txt", 10).is_empty());
    assert!(paths(&third, "keep.txt", 10).is_empty());
    assert_eq!(paths(&third, "ignored.txt", 10), ["ignored.txt"]);
    assert_eq!(paths(&third, "folder", 10), ["folder"]);
    // Existing session snapshots remain immutable.
    assert_eq!(paths(&first, "remove.txt", 10), ["remove.txt"]);
    let excluded = start_with_exclusions(&workspace, vec!["unused".into()]);
    assert!(!Arc::ptr_eq(&changed, &snapshot(&excluded)));
    let strict = start_workspace_quick_open_session_without_symlinks(
        root.to_string_lossy().into_owned(),
        Vec::new(),
    )
    .unwrap();
    assert!(!Arc::ptr_eq(&changed, &snapshot(&strict)));
    for session in [first, second, third, excluded, strict] {
        stop_workspace_quick_open_session(session);
    }
}

#[test]
fn reused_ranking_index_matches_fresh_index_for_all_modes() {
    let descriptors = |changed: bool| {
        let mut paths = vec![
            QuickOpenPath::new("src/src/src/main.rs".into(), false, false),
            QuickOpenPath::new("src/src/README.md".into(), false, false),
            QuickOpenPath::new("src/中文/中文🙂.rs".into(), false, false),
            QuickOpenPath::new("src/中文".into(), changed, true),
            QuickOpenPath::new("lib/ReadMe.md".into(), true, false),
            QuickOpenPath::new("lib/readme.md".into(), false, false),
        ];
        if changed {
            paths.push(QuickOpenPath::new("src/src/new.rs".into(), false, false));
        }
        paths.sort_by(|a, b| {
            a.normalized_path
                .cmp(&b.normalized_path)
                .then_with(|| a.relative_path.cmp(&b.relative_path))
        });
        paths
    };
    let original = QuickOpenIndex::from_paths(descriptors(false), None);
    let reused = QuickOpenIndex::from_paths(descriptors(true), Some(&original));
    let fresh = QuickOpenIndex::from_paths(descriptors(true), None);
    for query in [
        "", "src", "s", "src/src", "readme", "rm", "中文", "🙂", "new", "absent",
    ] {
        for limit in [1, 2, 5, 100] {
            for ignored in [false, true] {
                for directories in [false, true] {
                    let a = ranking::search(&reused, query, limit, ignored, directories);
                    let b = ranking::search(&fresh, query, limit, ignored, directories);
                    assert_eq!(format!("{a:?}"), format!("{b:?}"));
                }
            }
        }
    }
}

#[test]
#[ignore = "compare fresh and reused CPU index construction on a large path snapshot"]
fn compare_quick_open_snapshot_reuse() {
    use std::hint::black_box;
    let descriptors = |changed: bool| {
        let mut paths = (0..20000)
            .map(|n| {
                QuickOpenPath::new(
                    format!("packages/module_{:03}/src/src/component_{n:05}.rs", n % 200),
                    n % 13 == 0,
                    false,
                )
            })
            .collect::<Vec<_>>();
        if changed {
            paths[123] = QuickOpenPath::new("packages/new/src/changed.rs".into(), false, false);
        }
        paths.sort_by(|a, b| {
            a.normalized_path
                .cmp(&b.normalized_path)
                .then_with(|| a.relative_path.cmp(&b.relative_path))
        });
        paths
    };
    let baseline = QuickOpenIndex::from_paths(descriptors(false), None);
    let mut samples = [Vec::new(), Vec::new(), Vec::new()];
    for round in 0..7 {
        for offset in 0..3 {
            let mode = (round + offset) % 3;
            // Exclude descriptor construction/sort and disk discovery.
            let input = descriptors(mode == 2);
            let start = Instant::now();
            let index = QuickOpenIndex::from_paths(input, (mode != 0).then_some(&baseline));
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if mode == 1 {
                assert!(Arc::ptr_eq(&baseline, &index));
            }
            if mode == 2 {
                assert_eq!(
                    index
                        .files
                        .iter()
                        .filter(|file| {
                            baseline
                                .files
                                .binary_search_by(|old| {
                                    old.normalized_path
                                        .cmp(&file.normalized_path)
                                        .then_with(|| old.relative_path.cmp(&file.relative_path))
                                })
                                .ok()
                                .is_some_and(|n| Arc::ptr_eq(&baseline.files[n], file))
                        })
                        .count(),
                    19999
                );
            }
            samples[mode].push(elapsed);
            black_box(index);
        }
    }
    for (name, values) in ["fresh", "unchanged", "one_changed"]
        .into_iter()
        .zip(samples.iter_mut())
    {
        values.sort_by(f64::total_cmp);
        println!(
            "QUICK_OPEN_INDEX mode={name} paths=20000 median_ms={:.6}",
            values[3]
        );
    }
}

#[test]
#[ignore = "measure actual Windows filesystem snapshot refresh with and without index reuse"]
fn compare_quick_open_filesystem_refresh() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    for module in 0..40 {
        let dir = root.join(format!("module_{module:03}/src/src"));
        fs::create_dir_all(&dir).unwrap();
        for file in 0..75 {
            fs::write(dir.join(format!("component_{file:04}.rs")), "source").unwrap();
        }
    }
    let mut times = [Vec::new(), Vec::new()];
    for round in 0..7 {
        for offset in 0..2 {
            let mode = (round + offset) % 2;
            let previous = (mode == 1).then(|| start(&workspace));
            let old_snapshot = previous.as_ref().map(snapshot);
            let start_time = Instant::now();
            let next = start(&workspace);
            let ms = start_time.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(next.indexed_file_count, 3000);
            if let Some(old) = &old_snapshot {
                assert!(Arc::ptr_eq(old, &snapshot(&next)));
            }
            times[mode].push(ms);
            stop_workspace_quick_open_session(next);
            if let Some(previous) = previous {
                stop_workspace_quick_open_session(previous);
            }
        }
    }
    for (mode, values) in ["fresh", "unchanged"].into_iter().zip(times.iter_mut()) {
        values.sort_by(f64::total_cmp);
        println!(
            "QUICK_OPEN_TREE mode={mode} files=3000 median_ms={:.3} samples={values:?}",
            values[3]
        );
    }
}

#[test]
fn parallel_metadata_validation_preserves_order_and_quota_selection() {
    let workspace = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(workspace.path()).unwrap();
    fs::write(root.join(".gitignore"), "ignored/\n*.tmp\n!keep.tmp\n").unwrap();
    for folder in ["src", "ignored", "node_modules", "nested/deep"] {
        let dir = root.join(folder);
        fs::create_dir_all(&dir).unwrap();
        for index in 0..45 {
            fs::write(dir.join(format!("file-{index:02}.txt")), "text").unwrap();
        }
        fs::write(dir.join("keep.tmp"), "keep").unwrap();
        fs::write(dir.join("skip.tmp"), "skip").unwrap();
    }
    let exclusions = QuickOpenExclusions::new(vec!["node_modules".into()]);
    for symlinks in [false, true] {
        for (files, bytes) in [(0, 0), (1, 20), (12, 200), (200, 1000), (500, 100000)] {
            let a = collect_quick_open_files_with_workers(
                &root,
                files,
                bytes,
                symlinks,
                &exclusions,
                1,
            )
            .unwrap();
            let b = collect_quick_open_files_with_workers(
                &root,
                files,
                bytes,
                symlinks,
                &exclusions,
                4,
            )
            .unwrap();
            assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
    }
}

#[test]
#[ignore = "compare ordered metadata discovery with bounded workers on real filesystem"]
fn compare_quick_open_parallel_metadata_discovery() {
    for (folders, per_folder) in [(2, 20), (40, 75)] {
        let workspace = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(workspace.path()).unwrap();
        for n in 0..folders {
            let dir = root.join(format!("module_{n:03}/src"));
            fs::create_dir_all(&dir).unwrap();
            for f in 0..per_folder {
                fs::write(dir.join(format!("file_{f:03}.rs")), "text").unwrap();
            }
        }
        let exclusions = QuickOpenExclusions::new(Vec::new());
        let run = |workers| {
            collect_quick_open_files_with_workers(
                &root,
                MAX_QUICK_OPEN_INDEXED_FILES,
                MAX_QUICK_OPEN_INDEXED_PATH_BYTES,
                true,
                &exclusions,
                workers,
            )
            .unwrap()
        };
        let reference = format!("{:?}", run(1));
        let mut times = [Vec::new(), Vec::new()];
        for round in 0..7 {
            for offset in 0..2 {
                let mode = (round + offset) % 2;
                let start = Instant::now();
                let result = run(if mode == 0 { 1 } else { 4 });
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(format!("{result:?}"), reference);
                times[mode].push(elapsed);
            }
        }
        for (label, values) in ["sequential", "parallel"].into_iter().zip(times.iter_mut()) {
            values.sort_by(f64::total_cmp);
            println!(
                "QUICK_OPEN_DISCOVERY files={} mode={label} median_ms={:.3} samples={values:?}",
                folders * per_folder,
                values[3]
            );
        }
    }
}

#[test]
fn indexes_and_ranks_workspace_files() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir(workspace.path().join("lib")).unwrap();
    fs::write(workspace.path().join("lib/main.dart"), "void main() {}").unwrap();
    fs::write(workspace.path().join("readme.md"), "Alera").unwrap();
    let session = start(&workspace);
    assert_eq!(paths(&session, "main", 20)[0], "lib/main.dart");
    stop_workspace_quick_open_session(session);
}

#[test]
fn nested_linked_worktrees_are_not_indexed_even_when_including_ignored_files() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let repo = git2::Repository::init(root).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(".gitignore"), ".worktrees/\n").unwrap();
    fs::write(root.join("src/local_target.dart"), "local").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(".gitignore")).unwrap();
    index.add_path(Path::new("src/local_target.dart")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Alera tests", "tests@alera.build").unwrap();
    let oid = repo
        .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .unwrap();
    drop(tree);

    let head = repo.find_commit(oid).unwrap();
    let reference = repo
        .branch("feature/nested", &head, false)
        .unwrap()
        .into_reference();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    fs::create_dir(root.join(".worktrees")).unwrap();
    let nested = root.join(".worktrees/feature");
    repo.worktree("feature", &nested, Some(&options)).unwrap();
    fs::write(nested.join("nested_target.dart"), "should not appear").unwrap();

    // A similarly located ordinary gitignored directory remains searchable.
    fs::create_dir_all(root.join(".worktrees/ordinary")).unwrap();
    fs::write(
        root.join(".worktrees/ordinary/ordinary_target.dart"),
        "visible",
    )
    .unwrap();

    let session = start(&workspace);
    let quick_open = paths_including_gitignored(&session, "target", 100);
    assert!(quick_open.contains(&"src/local_target.dart".to_string()));
    assert!(quick_open.contains(&".worktrees/ordinary/ordinary_target.dart".to_string()));
    assert!(!quick_open
        .iter()
        .any(|path| path.starts_with(".worktrees/feature")));

    let explorer =
        search_workspace_explorer_filter_session(session.clone(), "feature".into(), 100, true)
            .unwrap();
    assert!(!explorer
        .iter()
        .any(|item| item.relative_path.starts_with(".worktrees/feature")));
    stop_workspace_quick_open_session(session);

    // A selected linked worktree still searches its own files.
    let child_session =
        start_workspace_quick_open_session(nested.to_string_lossy().into_owned(), Vec::new())
            .unwrap();
    assert_eq!(
        paths(&child_session, "nested_target", 20),
        vec!["nested_target.dart"]
    );
    stop_workspace_quick_open_session(child_session);
}

#[test]
fn explorer_indexes_unloaded_empty_deep_folders_without_changing_quick_open() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("src/deep/empty-config")).unwrap();
    fs::create_dir_all(workspace.path().join("src/deep/empty-config/nested")).unwrap();
    fs::write(workspace.path().join("src/deep/file-config.dart"), "hello").unwrap();
    let session = start(&workspace);
    assert_eq!(session.indexed_file_count, 1);

    let matches =
        search_workspace_explorer_filter_session(session.clone(), "config".into(), 20, false)
            .unwrap();
    assert!(matches
        .iter()
        .any(|item| item.relative_path == "src/deep/empty-config" && item.is_directory));
    assert!(matches
        .iter()
        .any(|item| item.relative_path == "src/deep/empty-config/nested" && item.is_directory));
    assert!(matches
        .iter()
        .any(|item| item.relative_path == "src/deep/file-config.dart" && !item.is_directory));

    // Even if a directory ranks first, Ctrl+P must return only real files.
    let quick_open =
        search_workspace_quick_open_session(session.clone(), "config".into(), 1, false).unwrap();
    assert_eq!(quick_open.len(), 1);
    assert_eq!(quick_open[0].relative_path, "src/deep/file-config.dart");
    assert!(!quick_open[0].is_directory);
    assert!(!paths(&session, "", 20)
        .iter()
        .any(|path| path == "src/deep/empty-config"));
    stop_workspace_quick_open_session(session);
}

#[test]
fn explorer_folder_index_respects_gitignore_and_exclusions() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join(".gitignore"), "ignored_folder/\n").unwrap();
    fs::create_dir_all(workspace.path().join("ignored_folder/empty")).unwrap();
    fs::create_dir_all(workspace.path().join("good_folder/empty")).unwrap();
    fs::create_dir_all(workspace.path().join("node_modules/blocked_folder")).unwrap();
    let session = start_with_exclusions(&workspace, vec!["node_modules".into()]);

    let hidden =
        search_workspace_explorer_filter_session(session.clone(), "folder".into(), 100, false)
            .unwrap();
    assert!(hidden
        .iter()
        .any(|item| item.relative_path == "good_folder" && item.is_directory));
    assert!(!hidden
        .iter()
        .any(|item| item.relative_path.starts_with("ignored_folder")));
    assert!(!hidden
        .iter()
        .any(|item| item.relative_path.starts_with("node_modules")));

    let all = search_workspace_explorer_filter_session(session.clone(), "folder".into(), 100, true)
        .unwrap();
    assert!(all
        .iter()
        .any(|item| item.relative_path == "ignored_folder" && item.is_directory));
    assert!(!all
        .iter()
        .any(|item| item.relative_path.starts_with("node_modules")));
    stop_workspace_quick_open_session(session);
}

#[test]
fn skips_files_removed_while_building_a_quick_open_index() {
    let workspace = tempfile::tempdir().unwrap();
    let missing = workspace.path().join("removed.txt");
    let root = dunce::canonicalize(workspace.path()).unwrap();

    let result = quick_open_relative_path(&root, &missing, true, &QuickOpenExclusions::new(vec![]));

    assert!(result.unwrap().is_none());
}

#[test]
fn skips_invalid_metadata_errors_without_breaking_the_index() {
    let workspace = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(workspace.path()).unwrap();
    let invalid = root.join("invalid\0file");
    // An invalid path produces an error beyond NotFound/PermissionDenied.
    // As with Windows `nul`, it must not abort the shared index.
    let error = fs::symlink_metadata(&invalid).unwrap_err();
    assert!(!matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
    ));
    assert!(
        quick_open_relative_path(&root, &invalid, true, &QuickOpenExclusions::new(vec![]))
            .unwrap()
            .is_none()
    );
}

#[cfg(windows)]
#[test]
fn skips_windows_reserved_device_paths_without_aborting_other_searches() {
    let workspace = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(workspace.path()).unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/working_file.rs"), "fn main() {}").unwrap();

    // A checkout made on another OS can contain names like `nul` which
    // Windows cannot query as ordinary files (often os error 1).
    let invalid = root.join("nul");
    assert!(
        quick_open_relative_path(&root, &invalid, true, &QuickOpenExclusions::new(vec![]))
            .unwrap()
            .is_none()
    );

    // Ctrl+P and Explorer share one index and must both retain valid entries.
    let session = start(&workspace);
    assert!(paths(&session, "working_file", 20).contains(&"src/working_file.rs".to_string()));
    assert!(
        search_workspace_explorer_filter_session(session.clone(), "src".into(), 20, false)
            .unwrap()
            .iter()
            .any(|item| item.relative_path == "src" && item.is_directory)
    );
    stop_workspace_quick_open_session(session);
}

#[test]
fn indexes_gitignored_files_once_and_filters_them_at_query_time() {
    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(workspace.path().join("visible.txt"), "visible").unwrap();
    fs::write(workspace.path().join("ignored.txt"), "ignored").unwrap();

    let session = start(&workspace);
    assert!(!paths(&session, "ignored", 20).contains(&"ignored.txt".to_string()));
    assert!(
        paths_including_gitignored(&session, "ignored", 20).contains(&"ignored.txt".to_string())
    );
    stop_workspace_quick_open_session(session);
}

#[test]
fn never_indexes_common_dependency_directories() {
    let workspace = tempfile::tempdir().unwrap();
    for relative in [
        "node_modules/pkg/index.js",
        ".dart_tool/package_config.json",
        "vendor/package/file.php",
        "vendor-bin/phpunit/vendor/bin/phpunit",
        ".phpunit.cache/test-results",
        "coverage/index.html",
        "Pods/Library/file.m",
        ".gradle/cache.bin",
        ".venv/lib/site.py",
        "target/debug/app.exe",
        "bin/Debug/app.dll",
        "obj/project.assets.json",
        ".vs/cache/file.bin",
        "packages/Newtonsoft.Json/file.dll",
        "TestResults/results.trx",
        "BenchmarkDotNet.Artifacts/results/report.md",
        "artifacts/bin/app.dll",
    ] {
        let path = workspace.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, relative).unwrap();
    }
    fs::create_dir_all(workspace.path().join("src")).unwrap();
    fs::write(workspace.path().join("src/main.rs"), "fn main() {}").unwrap();

    let session = start_with_exclusions(
        &workspace,
        [
            "node_modules",
            ".dart_tool",
            "vendor",
            "vendor-bin",
            ".phpunit.cache",
            "coverage",
            "Pods",
            ".gradle",
            ".venv",
            "target",
            "bin",
            "obj",
            ".vs",
            "packages",
            "TestResults",
            "BenchmarkDotNet.Artifacts",
            "artifacts",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    );
    let indexed = paths_including_gitignored(&session, "", 100);

    assert!(indexed.contains(&"src/main.rs".to_string()));
    for excluded in [
        "node_modules/pkg/index.js",
        ".dart_tool/package_config.json",
        "vendor/package/file.php",
        "vendor-bin/phpunit/vendor/bin/phpunit",
        ".phpunit.cache/test-results",
        "coverage/index.html",
        "Pods/Library/file.m",
        ".gradle/cache.bin",
        ".venv/lib/site.py",
        "target/debug/app.exe",
        "bin/Debug/app.dll",
        "obj/project.assets.json",
        ".vs/cache/file.bin",
        "packages/Newtonsoft.Json/file.dll",
        "TestResults/results.trx",
        "BenchmarkDotNet.Artifacts/results/report.md",
        "artifacts/bin/app.dll",
    ] {
        assert!(
            !indexed.contains(&excluded.to_string()),
            "indexed {excluded}"
        );
    }
    stop_workspace_quick_open_session(session);
}

#[test]
fn preserves_exact_path_segment_ranking() {
    let workspace = tempfile::tempdir().unwrap();
    for relative in [
        "src/file.dart",
        "lib/src/other.dart",
        "lib/src/src/repeated.dart",
        "lib/src-prefix.dart",
    ] {
        let path = workspace.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, relative).unwrap();
    }
    let session = start(&workspace);
    let matches =
        search_workspace_quick_open_session(session.clone(), "src".into(), 20, false).unwrap();
    assert_eq!(matches[0].relative_path, "src/file.dart");
    assert_eq!(matches[0].score, 90_000);
    assert_eq!(matches[1].relative_path, "lib/src/other.dart");
    assert_eq!(matches[1].score, 89_999);
    assert_eq!(matches[2].relative_path, "lib/src/src/repeated.dart");
    assert_eq!(matches[2].score, 89_999);
    assert_eq!(matches[3].relative_path, "lib/src-prefix.dart");
    assert!(matches[3].score < matches[2].score);
    stop_workspace_quick_open_session(session);
}

#[test]
fn breaks_normalized_path_ties_with_the_original_path() {
    let mut files = vec![
        QuickOpenFile::new("a.dart".to_string()),
        QuickOpenFile::new("A.dart".to_string()),
    ];
    sort_quick_open_files(&mut files);
    assert_eq!(
        files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["A.dart", "a.dart"]
    );
}

#[test]
fn fuzzy_scoring_prefers_path_boundaries_and_contiguous_matches() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir(workspace.path().join("foo")).unwrap();
    fs::write(workspace.path().join("foo/bar.txt"), "boundary").unwrap();
    fs::write(workspace.path().join("farboo.txt"), "gap").unwrap();
    let session = start(&workspace);

    let matches =
        search_workspace_quick_open_session(session.clone(), "fb".into(), 20, false).unwrap();

    assert_eq!(matches[0].relative_path, "foo/bar.txt");
    assert_eq!(matches[1].relative_path, "farboo.txt");
    assert!(matches[0].score > matches[1].score);
    stop_workspace_quick_open_session(session);
}

#[cfg(unix)]
#[test]
fn preserves_literal_backslashes_as_distinct_unix_file_names() {
    let workspace = tempfile::tempdir().unwrap();
    fs::create_dir(workspace.path().join("foo")).unwrap();
    fs::write(workspace.path().join("foo/bar.txt"), "nested").unwrap();
    fs::write(workspace.path().join("foo\\bar.txt"), "literal").unwrap();
    let session = start(&workspace);

    let indexed = paths(&session, "", 20);

    assert!(indexed.contains(&"foo/bar.txt".to_string()));
    assert!(indexed.contains(&"foo\\bar.txt".to_string()));
    stop_workspace_quick_open_session(session);
}

#[cfg(unix)]
fn create_file_symlink(source: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(source, link)
}

#[cfg(windows)]
fn create_file_symlink(source: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(source, link)
}

#[cfg(not(any(unix, windows)))]
fn create_file_symlink(source: &Path, link: &Path) -> io::Result<()> {
    let _ = (source, link);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "file symlinks are unsupported on this platform",
    ))
}

#[test]
fn includes_local_file_symlinks_but_never_external_targets() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let local = workspace.path().join("local.txt");
    let external = outside.path().join("external.txt");
    fs::write(&local, "local").unwrap();
    fs::write(&external, "external").unwrap();
    if create_file_symlink(&local, &workspace.path().join("local-link.txt")).is_err()
        || create_file_symlink(&external, &workspace.path().join("external-link.txt")).is_err()
    {
        return;
    }
    let session = start(&workspace);
    let indexed = paths(&session, "", 20);
    assert!(indexed.contains(&"local.txt".to_string()));
    assert!(indexed.contains(&"local-link.txt".to_string()));
    assert!(!indexed.contains(&"external-link.txt".to_string()));
    stop_workspace_quick_open_session(session);
}

#[test]
fn strict_sessions_exclude_all_file_symlinks() {
    let workspace = tempfile::tempdir().unwrap();
    let local = workspace.path().join("local.txt");
    fs::write(&local, "local").unwrap();
    if create_file_symlink(&local, &workspace.path().join("local-link.txt")).is_err() {
        return;
    }
    let session = start_workspace_quick_open_session_without_symlinks(
        workspace.path().to_string_lossy().into_owned(),
        Vec::new(),
    )
    .expect("start strict session");
    let indexed = paths(&session, "", 20);
    assert!(indexed.contains(&"local.txt".to_string()));
    assert!(!indexed.contains(&"local-link.txt".to_string()));
    stop_workspace_quick_open_session(session);
}

#[test]
fn prunes_idle_and_excess_sessions() {
    let now = Instant::now();
    let index = || Arc::new(QuickOpenIndex::new(Vec::new()));
    let mut entries = HashMap::from([
        (
            "expired".to_string(),
            QuickOpenSessionEntry {
                index: index(),
                scope: test_scope(),
                baseline: None,
                last_accessed: now - QUICK_OPEN_SESSION_IDLE_TTL - Duration::from_secs(1),
            },
        ),
        (
            "live".to_string(),
            QuickOpenSessionEntry {
                index: index(),
                scope: test_scope(),
                baseline: None,
                last_accessed: now,
            },
        ),
    ]);
    prune_sessions(&mut entries, now);
    assert!(!entries.contains_key("expired"));
    assert!(entries.contains_key("live"));
    for offset in 0..=MAX_QUICK_OPEN_SESSIONS {
        entries.insert(
            format!("session-{offset}"),
            QuickOpenSessionEntry {
                index: index(),
                scope: test_scope(),
                baseline: None,
                last_accessed: now + Duration::from_secs(offset as u64),
            },
        );
    }
    enforce_session_limit(&mut entries);
    assert_eq!(entries.len(), MAX_QUICK_OPEN_SESSIONS);
    assert!(!entries.contains_key("live"));
}

#[test]
fn refreshes_a_requested_session_before_pruning_idle_entries() {
    let now = Instant::now();
    let expired_at = now - QUICK_OPEN_SESSION_IDLE_TTL - Duration::from_secs(1);
    let index = || Arc::new(QuickOpenIndex::new(Vec::new()));
    let mut entries = HashMap::from([
        (
            "requested".to_string(),
            QuickOpenSessionEntry {
                index: index(),
                scope: test_scope(),
                baseline: None,
                last_accessed: expired_at,
            },
        ),
        (
            "abandoned".to_string(),
            QuickOpenSessionEntry {
                index: index(),
                scope: test_scope(),
                baseline: None,
                last_accessed: expired_at,
            },
        ),
    ]);

    let requested = access_session(&mut entries, "requested", now).unwrap();

    assert!(Arc::ptr_eq(&requested, &entries["requested"].index));
    assert_eq!(entries["requested"].last_accessed, now);
    assert!(!entries.contains_key("abandoned"));
}

#[test]
fn uses_the_rarest_character_index_and_bounds_ranked_results() {
    let mut files = (0..100)
        .map(|index| QuickOpenFile::new(format!("lib/common-{index}.dart")))
        .collect::<Vec<_>>();
    files.push(QuickOpenFile::new("lib/unique-zebra.dart".to_string()));
    let index = QuickOpenIndex::new(files);
    let query_code_units = "zebra".encode_utf16().collect::<Vec<_>>();
    let query_counts = character_counts(&query_code_units);
    assert_eq!(
        index
            .character_index
            .candidates(&query_counts)
            .unwrap()
            .len(),
        1
    );
    let workspace = tempfile::tempdir().unwrap();
    for index in 0..20 {
        fs::write(workspace.path().join(format!("match-{index}.txt")), "match").unwrap();
    }
    let session = start(&workspace);
    assert_eq!(paths(&session, "match", 3).len(), 3);
    stop_workspace_quick_open_session(session);
}

#[test]
fn returns_a_bounded_partial_index_when_budgets_are_exhausted() {
    let workspace = tempfile::tempdir().unwrap();
    for relative in ["one.txt", "two.txt", "three.txt"] {
        fs::write(workspace.path().join(relative), relative).unwrap();
    }

    let exclusions = QuickOpenExclusions::new(Vec::new());
    let files =
        collect_quick_open_files(workspace.path(), 2, usize::MAX, true, &exclusions).unwrap();
    assert_eq!(
        files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["one.txt", "three.txt"]
    );

    let files =
        collect_quick_open_files(workspace.path(), usize::MAX, 8, true, &exclusions).unwrap();
    assert!(files.len() <= 1);
    assert!(files.iter().all(|file| file.relative_path.len() <= 8));
}

#[test]
fn serializes_concurrent_index_builds() {
    const WORKERS: usize = 8;
    let start = Arc::new(Barrier::new(WORKERS));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let workers = (0..WORKERS)
        .map(|_| {
            let start = Arc::clone(&start);
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            thread::spawn(move || {
                start.wait();
                with_quick_open_build_gate(|| {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(current, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(5));
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .unwrap();
            })
        })
        .collect::<Vec<_>>();

    for worker in workers {
        worker.join().unwrap();
    }

    assert_eq!(maximum.load(Ordering::SeqCst), 1);
}

#[cfg(unix)]
#[test]
fn keeps_accessible_files_when_a_nested_directory_is_unreadable() {
    use std::os::unix::fs::PermissionsExt;

    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("visible.txt"), "visible").unwrap();
    let denied = workspace.path().join("denied");
    fs::create_dir(&denied).unwrap();
    fs::write(denied.join("hidden.txt"), "hidden").unwrap();
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o0)).unwrap();

    let exclusions = QuickOpenExclusions::new(Vec::new());
    let result =
        collect_quick_open_files(workspace.path(), usize::MAX, usize::MAX, true, &exclusions);

    fs::set_permissions(&denied, fs::Permissions::from_mode(0o700)).unwrap();
    let files = result.unwrap();
    assert!(files.iter().any(|file| file.relative_path == "visible.txt"));
}
