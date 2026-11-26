use super::*;
use crate::api::workspace_search::{
    compile::compile_search, preview_workspace_replace, WorkspaceReplaceOptions,
    WorkspaceSearchOptions,
};
use std::path::Path;
use std::time::{Duration, Instant};

fn options(root: &Path, query: &str, limit: u32) -> WorkspaceSearchOptions {
    WorkspaceSearchOptions {
        workspace_path: root.to_string_lossy().into_owned(),
        query: query.into(),
        case_sensitive: false,
        whole_word: false,
        use_regex: false,
        include_pattern: None,
        exclude_pattern: None,
        include_ignored: false,
        max_results: Some(limit),
    }
}

fn assert_same(left: &WorkspaceSearchResult, right: &WorkspaceSearchResult) {
    assert_eq!(format!("{left:?}"), format!("{right:?}"));
}

#[test]
fn ordered_prefetch_preserves_results_caps_and_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("empty.txt"), "").unwrap();
    fs::write(dir.path().join("binary.dat"), b"needle\0needle").unwrap();
    fs::write(dir.path().join("invalid.dat"), b"needle\xff").unwrap();
    for index in 0..25 {
        let path = dir.path().join("nested").join(format!("{index:02}.txt"));
        fs::write(
            path,
            if index == 7 {
                "needle \u{4e2d}\u{6587}\r\n".repeat(105)
            } else {
                format!("prefix {index}\r\nNEEDLE\r\nneedle tail\n")
            },
        )
        .unwrap();
    }
    for limit in [1, 2, 7, 99, 100, 101, 149, 2000] {
        let compiled = compile_search(&options(dir.path(), "needle", limit)).unwrap();
        let sequential = run_search_impl(&compiled, false, None, None, 1).unwrap();
        let parallel = run_search_impl(&compiled, false, None, None, 4).unwrap();
        assert_same(&sequential, &parallel);
    }
    let compiled = compile_search(&options(dir.path(), "needle", 2000)).unwrap();
    let cancelled = AtomicBool::new(true);
    let error = run_search_impl(&compiled, false, Some(&cancelled), None, 4).unwrap_err();
    assert_eq!(error.kind, WorkspaceSearchErrorKind::Cancelled);
}

#[test]
fn watched_cache_reuses_content_and_observes_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let path = root.join("sample.txt");
    fs::write(&path, "old needle\n").unwrap();
    fs::write(root.join("binary.dat"), b"\0needle").unwrap();
    let original_modified = fs::metadata(&path).unwrap().modified().unwrap();
    let before_file = fs::File::open(&path).unwrap();
    let old_stamp = FileStamp::from_file(&before_file, &before_file.metadata().unwrap()).unwrap();
    drop(before_file);
    let compiled = compile_search(&options(&root, "needle", 2000)).unwrap();
    let cache = RootCache::new(&root).expect("private temporary recursive watcher");
    let first = run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap();
    let (_, cached, _) = cache.stats();
    assert_eq!(cached, 2, "text and binary-negative entry");
    let second = run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap();
    assert_same(&first, &second);
    assert_eq!(cache.stats().0, 2, "both unchanged file contents reused");

    // Same-size edit plus restored modification time. The OS watcher must
    // invalidate the content even when a timestamp/size-only cache would hit.
    fs::write(&path, "new needle\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(original_modified))
        .unwrap();
    let changed_file = fs::File::open(&path).unwrap();
    let changed_stamp =
        FileStamp::from_file(&changed_file, &changed_file.metadata().unwrap()).unwrap();
    assert_ne!(
        old_stamp, changed_stamp,
        "restoring mtime must not restore the cache stamp"
    );
    drop(changed_file);
    let deadline = Instant::now() + Duration::from_secs(5);
    while cache.stats().1 == 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        cache.stats().1 < 2,
        "real watcher did not invalidate changed content"
    );
    let new = run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap();
    assert_eq!(new.files[0].matches[0].line_content, "new needle");

    fs::rename(&path, root.join("renamed.txt")).unwrap();
    fs::write(root.join("added.txt"), "added needle\n").unwrap();
    let after_rename = run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap();
    assert_eq!(after_rename.total_matches, 2);
    assert!(after_rename
        .files
        .iter()
        .any(|f| f.relative_path == "renamed.txt"));
    assert!(!after_rename
        .files
        .iter()
        .any(|f| f.relative_path == "sample.txt"));
    fs::remove_file(root.join("renamed.txt")).unwrap();
    let fresh = run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap();
    assert_eq!(fresh.total_matches, 1);
    assert_eq!(fresh.files[0].relative_path, "added.txt");
}

#[test]
fn replacement_preview_never_uses_cached_content() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let path = root.join("sample.txt");
    fs::write(&path, "old needle\n").unwrap();
    let compiled = compile_search(&options(&root, "needle", 2000)).unwrap();
    let cache = RootCache::new(&root).unwrap();
    run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap();
    let before = cache.stats().0;
    fs::write(&path, "new needle\n").unwrap();
    let preview = preview_workspace_replace(WorkspaceReplaceOptions {
        search: options(&root, "needle", 2000),
        replacement: "other".into(),
        preserve_case: false,
    })
    .unwrap();
    assert_eq!(
        preview.result.files[0].matches[0].line_content,
        "new needle"
    );
    assert_eq!(cache.stats().0, before);
}

#[test]
fn incremental_index_matches_fresh_scan_and_invalidates_on_edit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let ascii = root.join("ascii.txt");
    let old = "ordinary source buffer\n".repeat(3500);
    fs::write(&ascii, &old).unwrap();
    fs::write(
        root.join("unicode.txt"),
        "\u{212a}\u{017f}x \u{4e16}\u{754c}\n".repeat(9000),
    )
    .unwrap();
    let cache = RootCache::new(&root).unwrap();
    let queries = [
        "needle",
        "needle_more",
        "missing_one",
        "missing_two",
        "missing_three",
        "ksx",
        "ordinary",
        "\u{4e16}\u{754c}",
    ];
    for query in queries {
        let compiled = compile_search(&options(&root, query, 2000)).unwrap();
        let reference = run_search_impl(&compiled, false, None, None, 1).unwrap();
        let indexed = run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap();
        assert_same(&reference, &indexed);
    }
    let (built, refinements, trigrams) = cache.index_stats();
    assert!(
        built >= 1 && refinements >= 2 && trigrams >= 1,
        "expected both proven refinement and trigram pruning: {built}/{refinements}/{trigrams}"
    );
    let original_time = fs::metadata(&ascii).unwrap().modified().unwrap();
    let updated = format!("needle_more{}", &old["needle_more".len()..]);
    assert_eq!(old.len(), updated.len());
    fs::write(&ascii, updated).unwrap();
    fs::File::options()
        .write(true)
        .open(&ascii)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(original_time))
        .unwrap();
    let compiled = compile_search(&options(&root, "needle_more", 2000)).unwrap();
    // No watcher wait: the handle's stable stamp must independently reject
    // stale negative evidence even if delivery of the watch event is delayed.
    let reference = run_search_impl(&compiled, false, None, None, 1).unwrap();
    let indexed = run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap();
    assert_eq!(reference.total_matches, 1);
    assert_same(&reference, &indexed);
    for (query, case_sensitive, whole_word, regex) in [
        ("KSX", false, false, false),
        ("ksx", true, false, false),
        ("\u{212a}\u{017f}x", false, false, false),
        ("ordinary", false, true, false),
        ("needle_.*", true, false, true),
    ] {
        let mut opts = options(&root, query, 11);
        opts.case_sensitive = case_sensitive;
        opts.whole_word = whole_word;
        opts.use_regex = regex;
        let compiled = compile_search(&opts).unwrap();
        assert_same(
            &run_search_impl(&compiled, false, None, None, 1).unwrap(),
            &run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap(),
        );
    }
}

#[test]
#[ignore = "measure indexed vs exact warm scans and lazy construction on the same tree"]
fn compare_incremental_literal_index() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let base = "let value = workspace_file.read(options);\n".repeat(6000);
    for n in 0..100 {
        let path = root.join(format!("{n:03}.rs"));
        if n % 25 == 0 {
            fs::write(path, format!("{base}\nneedle_feature_x actual_match\n")).unwrap();
        } else {
            fs::write(path, &base).unwrap();
        }
    }
    let cache = RootCache::new(&root).unwrap();
    let run = |query: &str, indexed: bool| {
        let mut compiled = compile_search(&options(&root, query, 2000)).unwrap();
        if !indexed {
            compiled.literal_query = None;
        }
        let start = Instant::now();
        let result = run_search_impl(&compiled, false, None, Some(&cache), 4).unwrap();
        (result, start.elapsed().as_secs_f64() * 1000.0)
    };
    let (reference, cold) = run("needle_feature", false);
    let (primed, prime) = run("needle_feature", true);
    assert_same(&reference, &primed);
    let mut build_samples = Vec::new();
    for query in ["unrelated_z1", "unrelated_z2", "unrelated_z3"] {
        let (old, exact) = run(query, false);
        let (new, indexed) = run(query, true);
        assert_same(&old, &new);
        build_samples.push((query, exact, indexed));
    }
    let mut times = [Vec::<f64>::new(), Vec::new(), Vec::new(), Vec::new()];
    for n in 0..7 {
        for (offset, query) in ["needle_feature_x", "absent_trigram_qz9"]
            .into_iter()
            .enumerate()
        {
            let (exact, a, indexed, b) = if n % 2 == 0 {
                let (exact, a) = run(query, false);
                let (indexed, b) = run(query, true);
                (exact, a, indexed, b)
            } else {
                let (indexed, b) = run(query, true);
                let (exact, a) = run(query, false);
                (exact, a, indexed, b)
            };
            assert_same(&exact, &indexed);
            times[offset * 2].push(a);
            times[offset * 2 + 1].push(b);
        }
    }
    let (built, refinements, trigrams) = cache.index_stats();
    let (hits, entries, bytes) = cache.stats();
    println!("LITERAL_INDEX files=100 approx_content_bytes={} cold_ms={cold:.3} prime_ms={prime:.3} warm_build_queries={build_samples:?}", base.len()*100);
    for (name, times) in [
        "refinement_exact",
        "refinement_indexed",
        "unrelated_exact",
        "unrelated_indexed",
    ]
    .into_iter()
    .zip(times.iter_mut())
    {
        times.sort_by(f64::total_cmp);
        println!(
            "LITERAL_INDEX mode={name} median_ms={:.3} samples={times:?}",
            times[3]
        );
    }
    println!("LITERAL_INDEX built={built} refinements={refinements} trigram_skips={trigrams} hits={hits} entries={entries} charged_bytes={bytes}");
    assert!(built > 0 && refinements > 0 && trigrams > 0);
}

#[test]
#[ignore = "synthetic complete file-tree I/O benchmark; run serialized"]
fn compare_workspace_read_ahead_and_cache() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let mut bytes = 0_usize;
    for directory in 0..20 {
        let folder = root.join(format!("module-{directory:02}"));
        fs::create_dir(&folder).unwrap();
        for index in 0..60 {
            let line = "let value = workspace_file.read(options);\n";
            let mut content = line.repeat(if index % 10 == 0 { 2000 } else { 200 });
            if index == 27 {
                content.push_str("benchmark_needle and benchmark_needlex\n");
            }
            bytes += content.len();
            fs::write(folder.join(format!("{index:03}.rs")), content).unwrap();
        }
    }
    let compiled = [
        compile_search(&options(&root, "benchmark_needle", 2000)).unwrap(),
        compile_search(&options(&root, "benchmark_needlex", 2000)).unwrap(),
    ];
    let reference = compiled
        .iter()
        .map(|query| run_search_impl(query, false, None, None, 1).unwrap())
        .collect::<Vec<_>>();
    let cache = RootCache::new(&root).unwrap();
    // Prime once and verify the full result, not merely count or elapsed.
    let prime = run_search_impl(&compiled[0], false, None, Some(&cache), 4).unwrap();
    assert_same(&reference[0], &prime);
    let primed = cache.stats();
    println!(
        "SEARCH_TREE files=1200 bytes={bytes} primed_entries={} cache_charge_bytes={}",
        primed.1, primed.2
    );
    let mut times = [Vec::<f64>::new(), Vec::new(), Vec::new()];
    for round in 0..7 {
        let index = round % compiled.len();
        for order in 0..3 {
            let mode = (round + order) % 3;
            let start = Instant::now();
            let result = match mode {
                0 => run_search_impl(&compiled[index], false, None, None, 1),
                1 => run_search_impl(&compiled[index], false, None, None, 4),
                _ => run_search_impl(&compiled[index], false, None, Some(&cache), 4),
            }
            .unwrap();
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            assert_same(&reference[index], &result);
            times[mode].push(ms);
        }
    }
    let end = cache.stats();
    for (label, samples) in ["sequential_fresh", "parallel_fresh", "parallel_cached"]
        .into_iter()
        .zip(times.iter_mut())
    {
        samples.sort_by(f64::total_cmp);
        println!(
            "SEARCH_TREE mode={label} median_ms={:.3} samples={samples:?}",
            samples[3]
        );
    }
    println!("SEARCH_TREE cache_hits={} expected_warm_lookups=8400 retained_entries={} retained_charge_bytes={}", end.0 - primed.0, end.1, end.2);
    assert!(end.0 > primed.0, "warm search never hit cache");
}
