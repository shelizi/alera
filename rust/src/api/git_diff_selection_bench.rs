use super::*;
use std::time::{Duration, Instant};

// Run separately from the parallel correctness suite. No timing assertion:
// hardware and filesystem cache state affect elapsed times.
#[test]
#[ignore = "manual batch-diff performance benchmark"]
fn batch_selection_benchmark() {
    const TRACKED: usize = 512;
    const CHANGED: usize = 64;
    let dir = tempfile::tempdir().expect("temporary benchmark repository");
    let repo = Repository::init(dir.path()).expect("init benchmark repository");
    let mut index = repo.index().expect("index");
    let name = |i: usize| format!("file-{i:04}.txt");
    for i in 0..TRACKED {
        let name = name(i);
        std::fs::write(dir.path().join(&name), format!("baseline {i}\n")).expect("write baseline");
        index.add_path(Path::new(&name)).expect("stage baseline");
    }
    index.write().expect("write index");
    let tree = repo
        .find_tree(index.write_tree().expect("write tree"))
        .expect("tree");
    let signature =
        git2::Signature::now("Diff Benchmark", "benchmark@example.invalid").expect("signature");
    repo.commit(Some("HEAD"), &signature, &signature, "baseline", &tree, &[])
        .expect("commit baseline");
    drop(tree);

    let mut file_paths = Vec::with_capacity(CHANGED);
    for i in 0..CHANGED {
        let name = name(i);
        std::fs::write(
            dir.path().join(&name),
            format!("baseline {i}\nstaged {i}\n"),
        )
        .expect("write staged change");
        if i % 2 == 0 {
            index.add_path(Path::new(&name)).expect("stage change");
        }
        std::fs::write(
            dir.path().join(&name),
            format!("baseline {i}\nstaged {i}\nworktree {i}\n"),
        )
        .expect("write worktree change");
        file_paths.push(name);
    }
    index.write().expect("persist staged changes");
    let paths = GitPathContext::new(&repo, dir.path().to_str().expect("UTF-8 fixture path"))
        .expect("path context");

    let run = |cached: bool| {
        let start = Instant::now();
        let mut selection_cache = cached.then(DiffSelectionCache::default);
        let mut files = Vec::new();
        let mut total_bytes = 0;
        let mut truncated = false;
        for path in &file_paths {
            if append_combined_diff_for_path_with_whitespace(
                &repo,
                &paths,
                path,
                GitDiffWhitespaceMode::Normal,
                selection_cache.as_mut(),
                &mut files,
                &mut total_bytes,
                &mut truncated,
            )
            .expect("render batch")
            {
                break;
            }
        }
        (start.elapsed(), files, truncated)
    };

    let check = |before: &(Duration, Vec<GitDiffFile>, bool),
                 after: &(Duration, Vec<GitDiffFile>, bool)| {
        assert!(!before.2 && !after.2);
        assert_eq!(before.1.len(), CHANGED + CHANGED / 2);
        assert_eq!(before.1.len(), after.1.len());
        for (left, right) in before.1.iter().zip(&after.1) {
            assert_eq!(
                (
                    &left.path,
                    &left.old_path,
                    left.area,
                    left.status,
                    left.added,
                    left.removed,
                    left.is_binary
                ),
                (
                    &right.path,
                    &right.old_path,
                    right.area,
                    right.status,
                    right.added,
                    right.removed,
                    right.is_binary
                )
            );
            assert_eq!(left.lines.len(), right.lines.len());
            for (left, right) in left.lines.iter().zip(&right.lines) {
                assert_eq!((&left.text, left.kind), (&right.text, right.kind));
            }
        }
    };

    // Warm both code paths before measuring; alternate execution order.
    check(&run(false), &run(true));
    let mut baseline = Vec::new();
    let mut optimized = Vec::new();
    for i in 0..3 {
        let (before, after) = if i % 2 == 0 {
            (run(false), run(true))
        } else {
            let after = run(true);
            (run(false), after)
        };
        check(&before, &after);
        baseline.push(before.0);
        optimized.push(after.0);
    }
    baseline.sort();
    optimized.sort();
    println!(
        "BATCH_DIFF_BENCH tracked={TRACKED} changed={CHANGED} rows={} baseline_median_ms={:.2} indexed_median_ms={:.2} speedup={:.2}x",
        CHANGED + CHANGED / 2,
        baseline[1].as_secs_f64() * 1000.0,
        optimized[1].as_secs_f64() * 1000.0,
        baseline[1].as_secs_f64() / optimized[1].as_secs_f64()
    );
}
