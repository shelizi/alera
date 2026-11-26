//! Ordered, bounded delta batches for read-only search. The final batch
//! carries the authoritative count/truncation flag; it may contain no files.
use super::*;
use std::time::{Duration, Instant};

const BATCH_FILES: usize = 16;
const BATCH_AGE: Duration = Duration::from_millis(75);

struct Publisher<F> {
    send: F,
    sent_files: usize,
    last_emit: Instant,
}

impl<F: FnMut(WorkspaceSearchBatch) -> Result<(), WorkspaceSearchError>> Publisher<F> {
    fn new(send: F) -> Self {
        Self {
            send,
            sent_files: 0,
            last_emit: Instant::now(),
        }
    }

    fn publish(
        &mut self,
        files: &[WorkspaceSearchFileResult],
        total_matches: u32,
        truncated: bool,
        done: bool,
        now: Instant,
    ) -> Result<(), WorkspaceSearchError> {
        let pending = &files[self.sent_files..];
        // Emit the first matching file immediately. Subsequent updates
        // carry only new files, bounded by file count and a quiet cadence.
        if !done
            && (pending.is_empty()
                || (self.sent_files != 0
                    && pending.len() < BATCH_FILES
                    && now.duration_since(self.last_emit) < BATCH_AGE))
        {
            return Ok(());
        }
        (self.send)(WorkspaceSearchBatch {
            files: pending.to_vec(),
            total_matches,
            truncated,
            done,
            error: None,
        })?;
        self.sent_files = files.len();
        self.last_emit = now;
        Ok(())
    }
}

pub(super) fn run(
    options: &WorkspaceSearchOptions,
    cancellation: &AtomicBool,
    send: impl FnMut(WorkspaceSearchBatch) -> Result<(), WorkspaceSearchError>,
) -> Result<(), WorkspaceSearchError> {
    engine::ensure_not_cancelled(Some(cancellation))?;
    let compiled = compile::compile_search(options)?;
    let mut publisher = Publisher::new(send);
    let result = engine::run_readonly_search_observed(
        &compiled,
        Some(cancellation),
        Some(&mut |files, total, truncated| {
            engine::ensure_not_cancelled(Some(cancellation))?;
            publisher.publish(files, total, truncated, false, Instant::now())
        }),
    )?;
    engine::ensure_not_cancelled(Some(cancellation))?;
    publisher.publish(
        &result.files,
        result.total_matches,
        result.truncated,
        true,
        Instant::now(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::Ordering;

    fn options(root: &std::path::Path, limit: u32) -> WorkspaceSearchOptions {
        WorkspaceSearchOptions {
            workspace_path: root.to_string_lossy().into_owned(),
            query: "needle".into(),
            case_sensitive: false,
            whole_word: false,
            use_regex: false,
            include_pattern: None,
            exclude_pattern: None,
            include_ignored: false,
            max_results: Some(limit),
        }
    }

    #[test]
    fn deltas_reconstruct_exact_search_with_caps_and_empty_terminal() {
        let root = tempfile::tempdir().unwrap();
        for n in 0..42 {
            fs::write(root.path().join(format!("{n:02}.txt")), "needle\nneedle\n").unwrap();
        }
        for limit in [1, 2, 17, 84, 2000] {
            let options = options(root.path(), limit);
            let expected = search_workspace(options.clone()).unwrap();
            let mut batches = Vec::new();
            run(&options, &AtomicBool::new(false), |batch| {
                batches.push(batch);
                Ok(())
            })
            .unwrap();
            assert_eq!(batches[0].files.len(), 1);
            assert!(!batches[0].done);
            assert!(batches.last().unwrap().done);
            assert_eq!(batches.iter().filter(|b| b.done).count(), 1);
            let mut files = Vec::new();
            let mut total = 0;
            for batch in &batches {
                assert!(batch.error.is_none());
                assert!(batch.files.len() <= BATCH_FILES);
                total += batch
                    .files
                    .iter()
                    .map(|file| file.matches.len() as u32)
                    .sum::<u32>();
                assert_eq!(batch.total_matches, total);
                files.extend(batch.files.clone());
            }
            let final_batch = batches.last().unwrap();
            assert_eq!(format!("{files:?}"), format!("{:?}", expected.files));
            assert_eq!(final_batch.total_matches, expected.total_matches);
            assert_eq!(final_batch.truncated, expected.truncated);
        }
        let mut no_match = options(root.path(), 2000);
        no_match.query = "not_present_at_all".into();
        let mut batches = Vec::new();
        run(&no_match, &AtomicBool::new(false), |batch| {
            batches.push(batch);
            Ok(())
        })
        .unwrap();
        assert_eq!(batches.len(), 1);
        assert!(batches[0].done && batches[0].files.is_empty());
    }

    #[test]
    fn publisher_flushes_on_age_count_and_completion_without_replaying_files() {
        let mut batches = Vec::new();
        let mut publisher = Publisher::new(|batch| {
            batches.push(batch);
            Ok(())
        });
        let start = Instant::now();
        let files = (0..22)
            .map(|n| WorkspaceSearchFileResult {
                relative_path: format!("{n}.txt"),
                content_token: String::new(),
                matches: vec![],
            })
            .collect::<Vec<_>>();
        publisher
            .publish(&files[..1], 1, false, false, start)
            .unwrap();
        publisher
            .publish(
                &files[..3],
                3,
                false,
                false,
                start + Duration::from_millis(20),
            )
            .unwrap();
        publisher
            .publish(&files[..3], 3, false, false, start + BATCH_AGE)
            .unwrap();
        publisher
            .publish(&files[..19], 19, false, false, start + BATCH_AGE)
            .unwrap();
        publisher
            .publish(&files[..22], 22, true, true, start + BATCH_AGE)
            .unwrap();
        assert_eq!(
            batches.iter().map(|b| b.files.len()).collect::<Vec<_>>(),
            [1, 2, 16, 3]
        );
        assert_eq!(batches.last().unwrap().total_matches, 22);
        assert!(batches.last().unwrap().done && batches.last().unwrap().truncated);
    }

    #[test]
    fn cancellation_after_first_batch_stops_without_success_terminal() {
        let root = tempfile::tempdir().unwrap();
        for n in 0..20 {
            fs::write(root.path().join(format!("{n}.txt")), "needle").unwrap();
        }
        let flag = AtomicBool::new(false);
        let mut batches = 0;
        let error = run(&options(root.path(), 2000), &flag, |batch| {
            assert!(!batch.done);
            batches += 1;
            flag.store(true, Ordering::Relaxed);
            Ok(())
        })
        .unwrap_err();
        assert_eq!(batches, 1);
        assert_eq!(error.kind, WorkspaceSearchErrorKind::Cancelled);
    }

    #[test]
    fn closed_consumer_stops_read_ahead_and_propagates_failure() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("a.txt"), "needle").unwrap();
        let mut calls = 0;
        let error = run(&options(root.path(), 2000), &AtomicBool::new(false), |_| {
            calls += 1;
            Err(WorkspaceSearchError::new(
                WorkspaceSearchErrorKind::Cancelled,
                "consumer closed",
            ))
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert_eq!(error.kind, WorkspaceSearchErrorKind::Cancelled);
    }
}
