use std::collections::VecDeque;
#[cfg(test)]
use std::fs;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use grep_matcher::Matcher;
use ignore::WalkBuilder;

use super::compile::CompiledSearch;
use super::content_cache::{self, FileStamp, Lookup, RootCache};
use super::globs::matches_globs;
use super::literal_index::{IndexedText, LiteralQuery, Probe};
use super::paths::{
    content_token, is_protected_relative_path, open_search_file, relative_string, should_walk_entry,
};
use super::{
    WorkspaceSearchError, WorkspaceSearchErrorKind, WorkspaceSearchFileResult,
    WorkspaceSearchMatch, WorkspaceSearchResult, MAX_LINE_CONTENT_LENGTH, MAX_MATCHES_PER_FILE,
    MAX_TEXT_FILE_BYTES,
};

pub(super) fn run_search(
    compiled: &CompiledSearch,
    full_lines: bool,
    cancellation: Option<&AtomicBool>,
) -> Result<WorkspaceSearchResult, WorkspaceSearchError> {
    // Replacement previews and execution must always use fresh disk bytes
    // and the original sequential order.
    run_search_impl(compiled, full_lines, cancellation, None, 1)
}

pub(super) type ProgressObserver<'a> =
    &'a mut dyn FnMut(&[WorkspaceSearchFileResult], u32, bool) -> Result<(), WorkspaceSearchError>;

pub(super) fn run_readonly_search(
    compiled: &CompiledSearch,
    cancellation: Option<&AtomicBool>,
) -> Result<WorkspaceSearchResult, WorkspaceSearchError> {
    run_readonly_search_observed(compiled, cancellation, None)
}

pub(super) fn run_readonly_search_observed(
    compiled: &CompiledSearch,
    cancellation: Option<&AtomicBool>,
    progress: Option<ProgressObserver<'_>>,
) -> Result<WorkspaceSearchResult, WorkspaceSearchError> {
    ensure_not_cancelled(cancellation)?;
    let cache = content_cache::for_root(&compiled.root);
    let workers = std::thread::available_parallelism().map_or(1, |count| count.get().min(4));
    run_search_impl_observed(
        compiled,
        false,
        cancellation,
        cache.as_deref(),
        workers,
        progress,
    )
}

fn run_search_impl(
    compiled: &CompiledSearch,
    full_lines: bool,
    cancellation: Option<&AtomicBool>,
    cache: Option<&RootCache>,
    workers: usize,
) -> Result<WorkspaceSearchResult, WorkspaceSearchError> {
    run_search_impl_observed(compiled, full_lines, cancellation, cache, workers, None)
}

fn run_search_impl_observed(
    compiled: &CompiledSearch,
    full_lines: bool,
    cancellation: Option<&AtomicBool>,
    cache: Option<&RootCache>,
    workers: usize,
    progress: Option<ProgressObserver<'_>>,
) -> Result<WorkspaceSearchResult, WorkspaceSearchError> {
    ensure_not_cancelled(cancellation)?;
    let mut walker = WalkBuilder::new(&compiled.root);
    let root = compiled.root.clone();
    // Do not walk other linked worktrees, even when ignored files are included.
    let nested_worktrees = alera_core::workspace_files::nested_git_worktree_paths(&root);
    walker
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .filter_entry(move |entry| {
            should_walk_entry(&root, entry.path()) && !nested_worktrees.contains(entry.path())
        });
    if compiled.include_ignored {
        walker
            .ignore(false)
            .git_global(false)
            .git_ignore(false)
            .git_exclude(false);
    }
    let walker = walker.build();
    if workers > 1 {
        if let Some(permit) = ReadAheadPermit::acquire(workers) {
            return scan_parallel(
                walker,
                compiled,
                full_lines,
                cancellation,
                cache,
                permit.0,
                progress,
            );
        }
    }
    scan_sequential(walker, compiled, full_lines, cancellation, cache, progress)
}

struct CandidateFile {
    path: std::path::PathBuf,
    relative_path: String,
}

fn candidate_file(
    entry: Result<ignore::DirEntry, ignore::Error>,
    compiled: &CompiledSearch,
) -> Option<CandidateFile> {
    let entry = entry.ok()?;
    let path = entry.path();
    if path == compiled.root {
        return None;
    }
    let file_type = entry.file_type()?;
    if !file_type.is_file() || file_type.is_symlink() {
        return None;
    }
    let relative_path = relative_string(&compiled.root, path)?;
    if is_protected_relative_path(&relative_path)
        || !matches_globs(&relative_path, &compiled.include, true)
        || matches_globs(&relative_path, &compiled.exclude, false)
    {
        return None;
    }
    Some(CandidateFile {
        path: path.to_path_buf(),
        relative_path,
    })
}

struct LoadedFile {
    relative_path: String,
    content_token: String,
    text: Arc<IndexedText>,
    cacheable: bool,
}

fn load_file(
    root: &std::path::Path,
    candidate: CandidateFile,
    cache: Option<&RootCache>,
    literal_query: Option<&LiteralQuery>,
) -> Option<LoadedFile> {
    let CandidateFile {
        path,
        relative_path,
    } = candidate;
    // All hits still undergo the same path containment, symlink, size and
    // regular-file validation. Open the actual file even on hits so cached
    // content is never used to bypass the current read permissions.
    let opened = open_search_file(root, &path, &relative_path)?;
    let metadata = opened.metadata;
    let mut file = opened.file;
    let key = path.strip_prefix(root).ok()?;
    let stamp = cache
        .filter(|_| metadata.len() <= content_cache::MAX_CACHED_FILE_BYTES)
        .and_then(|_| {
            if !opened.handle_verified {
                let before = FileStamp::from_metadata(&metadata)?;
                let opened_metadata = file.metadata().ok()?;
                if FileStamp::from_metadata(&opened_metadata)? != before {
                    return None;
                }
            }
            FileStamp::from_file(&file, &metadata)
        });
    let mut cache_generation = None;
    if let (Some(cache), Some(stamp)) = (cache, stamp.as_ref()) {
        match cache.lookup(key, stamp) {
            Lookup::Hit(text) => {
                let text = text?;
                if literal_query.is_some_and(|query| text.probe(query) != Probe::Scan) {
                    return None;
                }
                return Some(LoadedFile {
                    relative_path,
                    content_token: content_token(&metadata),
                    text,
                    cacheable: true,
                });
            }
            Lookup::Miss(generation) => cache_generation = Some(generation),
            Lookup::Disabled => {}
        }
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    (&mut file)
        .take(MAX_TEXT_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_TEXT_FILE_BYTES {
        return None;
    }
    let length = bytes.len() as u64;
    let text = if bytes.contains(&0) {
        None
    } else {
        String::from_utf8(bytes)
            .ok()
            .map(IndexedText::new)
            .map(Arc::new)
    };
    let mut cacheable = false;
    if let (Some(cache), Some(stamp), Some(generation)) = (cache, stamp, cache_generation) {
        // Never publish a read that raced an observed watch event, changed
        // handle metadata or a file growing/shrinking during the read.
        if length == metadata.len()
            && file
                .metadata()
                .ok()
                .and_then(|m| FileStamp::from_file(&file, &m))
                .as_ref()
                == Some(&stamp)
        {
            cache.insert(key.to_path_buf(), stamp, text.clone(), generation);
            cacheable = true;
        }
    }
    text.map(|text| LoadedFile {
        relative_path,
        content_token: content_token(&metadata),
        text,
        cacheable,
    })
}

#[flutter_rust_bridge::frb(ignore)]
#[derive(Default)]
struct SearchAccumulator {
    files: Vec<WorkspaceSearchFileResult>,
    total_matches: u32,
    truncated: bool,
}

impl SearchAccumulator {
    fn consume(
        &mut self,
        loaded: Option<LoadedFile>,
        compiled: &CompiledSearch,
        full_lines: bool,
        cancellation: Option<&AtomicBool>,
    ) -> Result<(), WorkspaceSearchError> {
        ensure_not_cancelled(cancellation)?;
        let Some(loaded) = loaded else {
            return Ok(());
        };
        let matches = matches_in_file(
            &loaded.relative_path,
            loaded.text.as_str(),
            compiled,
            full_lines,
            &mut self.total_matches,
            &mut self.truncated,
            cancellation,
        )?;
        if matches.is_empty() {
            // consume is called only before the global cap, and the exact
            // matcher returned successfully. Never record a partial scan.
            if loaded.cacheable {
                if let Some(query) = &compiled.literal_query {
                    loaded.text.remember_absent(query);
                }
            }
        } else {
            self.files.push(WorkspaceSearchFileResult {
                relative_path: loaded.relative_path,
                content_token: loaded.content_token,
                matches,
            });
        }
        Ok(())
    }

    fn notify(
        &self,
        progress: &mut Option<ProgressObserver<'_>>,
    ) -> Result<(), WorkspaceSearchError> {
        if let Some(observer) = progress.as_mut() {
            observer(&self.files, self.total_matches, self.truncated)?;
        }
        Ok(())
    }

    fn finish(self) -> WorkspaceSearchResult {
        WorkspaceSearchResult {
            files: self.files,
            total_matches: self.total_matches,
            truncated: self.truncated,
        }
    }
}

fn scan_sequential(
    walker: ignore::Walk,
    compiled: &CompiledSearch,
    full_lines: bool,
    cancellation: Option<&AtomicBool>,
    cache: Option<&RootCache>,
    mut progress: Option<ProgressObserver<'_>>,
) -> Result<WorkspaceSearchResult, WorkspaceSearchError> {
    let mut out = SearchAccumulator::default();
    for entry in walker {
        ensure_not_cancelled(cancellation)?;
        if out.total_matches >= compiled.max_results {
            out.truncated = true;
            break;
        }
        let loaded = candidate_file(entry, compiled).and_then(|candidate| {
            load_file(
                &compiled.root,
                candidate,
                cache,
                compiled.literal_query.as_deref(),
            )
        });
        out.consume(loaded, compiled, full_lines, cancellation)?;
        out.notify(&mut progress)?;
    }
    Ok(out.finish())
}

// Limit both the number of active read-ahead workers in the process and the
// number of slots in a single search. At most 8 * 5 MiB read-ahead text bytes
// per search (plus temporary decoding/allocations) before ordered matching.
const READ_AHEAD_SLOTS: usize = 8;
const MAX_READ_AHEAD_WORKERS: usize = 8;
static ACTIVE_READ_AHEAD_WORKERS: AtomicUsize = AtomicUsize::new(0);

struct ReadAheadPermit(usize);
impl ReadAheadPermit {
    fn acquire(wanted: usize) -> Option<Self> {
        let mut active = ACTIVE_READ_AHEAD_WORKERS.load(Ordering::Relaxed);
        loop {
            let count = wanted.min(MAX_READ_AHEAD_WORKERS.saturating_sub(active));
            if count < 2 {
                return None;
            }
            match ACTIVE_READ_AHEAD_WORKERS.compare_exchange_weak(
                active,
                active + count,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(Self(count)),
                Err(current) => active = current,
            }
        }
    }
}
impl Drop for ReadAheadPermit {
    fn drop(&mut self) {
        ACTIVE_READ_AHEAD_WORKERS.fetch_sub(self.0, Ordering::AcqRel);
    }
}

struct ReadJob {
    slot: usize,
    candidate: CandidateFile,
}

fn scan_parallel(
    mut walker: ignore::Walk,
    compiled: &CompiledSearch,
    full_lines: bool,
    cancellation: Option<&AtomicBool>,
    cache: Option<&RootCache>,
    workers: usize,
    mut progress: Option<ProgressObserver<'_>>,
) -> Result<WorkspaceSearchResult, WorkspaceSearchError> {
    use std::sync::mpsc;
    let (job_tx, job_rx) = mpsc::channel::<ReadJob>();
    let jobs = Mutex::new(job_rx);
    let (result_tx, result_rx) = mpsc::channel::<(usize, Option<LoadedFile>)>();
    let root = &compiled.root;
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let mut spawned = 0;
        for _ in 0..workers {
            let jobs = &jobs;
            let results = result_tx.clone();
            let stop = &stop;
            if std::thread::Builder::new()
                .name("alera-search-read".into())
                .spawn_scoped(scope, move || loop {
                    let job = jobs
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .recv();
                    let Ok(job) = job else {
                        break;
                    };
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let loaded = if ensure_not_cancelled(cancellation).is_ok() {
                        load_file(
                            root,
                            job.candidate,
                            cache,
                            compiled.literal_query.as_deref(),
                        )
                    } else {
                        None
                    };
                    if results.send((job.slot, loaded)).is_err() {
                        break;
                    }
                })
                .is_ok()
            {
                spawned += 1;
            }
        }
        drop(result_tx);
        if spawned == 0 {
            drop(job_tx);
            return scan_sequential(walker, compiled, full_lines, cancellation, cache, progress);
        }
        let result = (|| {
            let mut out = SearchAccumulator::default();
            // A sliding window overlaps matching with subsequent I/O. Slots
            // represent physical walker entries, including filtered entries,
            // so result ordering and the existing truncation contract stay
            // unchanged. Outer None is in-flight, Some(None) is skipped.
            let mut slots: VecDeque<Option<Option<LoadedFile>>> =
                VecDeque::with_capacity(READ_AHEAD_SLOTS);
            let mut head = 0_usize;
            let mut exhausted = false;
            loop {
                ensure_not_cancelled(cancellation)?;
                if out.total_matches >= compiled.max_results {
                    out.truncated |= !slots.is_empty() || (!exhausted && walker.next().is_some());
                    break;
                }
                while slots.len() < READ_AHEAD_SLOTS && !exhausted {
                    ensure_not_cancelled(cancellation)?;
                    let Some(entry) = walker.next() else {
                        exhausted = true;
                        break;
                    };
                    let slot = head + slots.len();
                    if let Some(candidate) = candidate_file(entry, compiled) {
                        slots.push_back(None);
                        job_tx.send(ReadJob { slot, candidate }).map_err(|_| {
                            WorkspaceSearchError::new(
                                WorkspaceSearchErrorKind::Io,
                                "search read workers stopped",
                            )
                        })?;
                    } else {
                        slots.push_back(Some(None));
                    }
                }
                if slots.is_empty() {
                    break;
                }
                // Wait only until the next entry is ready, not for the
                // slowest read in the entire window.
                while slots.front().is_some_and(Option::is_none) {
                    ensure_not_cancelled(cancellation)?;
                    let (slot, loaded) = result_rx.recv().map_err(|_| {
                        WorkspaceSearchError::new(
                            WorkspaceSearchErrorKind::Io,
                            "search read result missing",
                        )
                    })?;
                    let target = slots.get_mut(slot - head).expect("live read-ahead slot");
                    *target = Some(loaded);
                }
                let loaded = slots
                    .pop_front()
                    .expect("nonempty window")
                    .expect("ready slot");
                head += 1;
                out.consume(loaded, compiled, full_lines, cancellation)?;
                out.notify(&mut progress)?;
            }
            Ok(out.finish())
        })();
        // Cancel queued speculation at cap/error/cancellation, then join.
        stop.store(true, Ordering::Relaxed);
        drop(job_tx);
        result
    })
}

fn matches_in_file(
    relative_path: &str,
    text: &str,
    compiled: &CompiledSearch,
    full_lines: bool,
    total_matches: &mut u32,
    truncated: &mut bool,
    cancellation: Option<&AtomicBool>,
) -> Result<Vec<WorkspaceSearchMatch>, WorkspaceSearchError> {
    let mut out = Vec::new();
    let mut all_lines = text.lines().enumerate();
    let mut seek_offset = 0;
    let mut seek_line_index = 0;
    loop {
        ensure_not_cancelled(cancellation)?;
        let next = if compiled.seek_literal_lines {
            if seek_offset >= text.len() {
                break;
            }
            // Preserve the original truncation contract, including a
            // remaining nonmatching line after the global result limit.
            if *total_matches >= compiled.max_results {
                *truncated = true;
                break;
            }
            let next = seek_literal_line(text, compiled, &mut seek_offset, &mut seek_line_index)?;
            ensure_not_cancelled(cancellation)?;
            next
        } else {
            all_lines.next()
        };
        let Some((line_index, line)) = next else {
            break;
        };
        if *total_matches >= compiled.max_results {
            *truncated = true;
            break;
        }
        let line_bytes = line.as_bytes();
        let mut file_count = out.len() as u32;
        // Advance character columns from the previous nonoverlapping match.
        // Avoid repeatedly counting the entire prefix of long/minified lines.
        let mut previous_end = 0;
        let mut previous_end_chars = 0;
        let mut line_chars = None;
        compiled
            .matcher
            .find_iter(line_bytes, |m| {
                let start = m.start();
                let end = m.end();
                // The byte-oriented iterator can advance through the middle
                // of UTF-8 after an empty regex match. Such offsets cannot
                // be represented by editor character columns.
                if !line.is_char_boundary(start) || !line.is_char_boundary(end) {
                    return true;
                }
                if *total_matches >= compiled.max_results || file_count >= MAX_MATCHES_PER_FILE {
                    *truncated = true;
                    return false;
                }
                let start_chars = previous_end_chars + line[previous_end..start].chars().count();
                let matched_chars = line[start..end].chars().count();
                previous_end = end;
                previous_end_chars = start_chars + matched_chars;
                let column = start_chars as u32 + 1;
                let match_length = matched_chars as u32;
                let line_number = line_index as u32 + 1;
                let clamped = if full_lines {
                    LinePreview {
                        line_content: line.to_string(),
                        display_column: None,
                        display_match_length: None,
                    }
                } else {
                    // Most source lines fit without counting or allocating
                    // a temporary Vec<char>. Long lines count once per line.
                    let count = *line_chars.get_or_insert_with(|| {
                        if line.len() <= MAX_LINE_CONTENT_LENGTH {
                            line.len()
                        } else {
                            line.chars().count()
                        }
                    });
                    clamp_line_context(line, start, start_chars, matched_chars, count)
                };
                let id = format!("{relative_path}:{line_number}:{column}:{}", out.len());
                out.push(WorkspaceSearchMatch {
                    id,
                    line: line_number,
                    column,
                    match_length,
                    line_content: clamped.line_content,
                    display_column: clamped.display_column,
                    display_match_length: clamped.display_match_length,
                    replacement_preview: None,
                });
                *total_matches += 1;
                file_count += 1;
                true
            })
            .map_err(|error| {
                WorkspaceSearchError::new(
                    WorkspaceSearchErrorKind::InvalidPattern,
                    error.to_string(),
                )
            })?;
        // Once this file is capped and truncation is already known, no
        // additional line can change the returned matches or truncation bit.
        if out.len() as u32 >= MAX_MATCHES_PER_FILE && *truncated {
            break;
        }
    }
    Ok(out)
}

/// Find a matching physical line using the same grep matcher over the
/// remaining file. Only used for nonempty plain literals without CR/LF
/// or word-boundary mode. Re-run the matcher on that exact str::lines()
/// slice to retain the existing match, column and replacement semantics.
fn seek_literal_line<'a>(
    text: &'a str,
    compiled: &CompiledSearch,
    offset: &mut usize,
    line_index: &mut usize,
) -> Result<Option<(usize, &'a str)>, WorkspaceSearchError> {
    let found = compiled
        .matcher
        .find_at(text.as_bytes(), *offset)
        .map_err(|error| {
            WorkspaceSearchError::new(WorkspaceSearchErrorKind::InvalidPattern, error.to_string())
        })?;
    let Some(found) = found else {
        *offset = text.len();
        return Ok(None);
    };
    let skipped = &text[*offset..found.start()];
    *line_index += skipped.bytes().filter(|&byte| byte == b'\n').count();
    let start = skipped
        .rfind('\n')
        .map_or(*offset, |index| *offset + index + 1);
    let remainder = &text[start..];
    let length = remainder
        .find('\n')
        .map_or(remainder.len(), |index| index + 1);
    let line = remainder[..length].lines().next().expect("matching line");
    let current = *line_index;
    *offset = start + length;
    *line_index += 1;
    Ok(Some((current, line)))
}

pub(super) fn ensure_not_cancelled(
    cancellation: Option<&AtomicBool>,
) -> Result<(), WorkspaceSearchError> {
    if cancellation.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Err(WorkspaceSearchError::new(
            WorkspaceSearchErrorKind::Cancelled,
            "search cancelled",
        ));
    }
    Ok(())
}

struct LinePreview {
    line_content: String,
    display_column: Option<u32>,
    display_match_length: Option<u32>,
}

fn clamp_line_context(
    line: &str,
    start_byte: usize,
    start: usize,
    match_len: usize,
    line_chars: usize,
) -> LinePreview {
    if line_chars <= MAX_LINE_CONTENT_LENGTH {
        return LinePreview {
            line_content: line.to_string(),
            display_column: None,
            display_match_length: None,
        };
    }
    let clamped_match_len = match_len.min(MAX_LINE_CONTENT_LENGTH);
    let remaining = MAX_LINE_CONTENT_LENGTH.saturating_sub(clamped_match_len);
    let left_budget = remaining / 2;
    let mut window_start = start.saturating_sub(left_budget);
    let window_end = (window_start + MAX_LINE_CONTENT_LENGTH).min(line_chars);
    window_start = window_end.saturating_sub(MAX_LINE_CONTENT_LENGTH);
    // The context window starts at or before the match. Walk backwards
    // at most 500 characters, then forwards at most 500, instead of
    // allocating a vector proportional to the entire source line.
    let start_byte = if window_start == start {
        start_byte
    } else {
        line[..start_byte]
            .char_indices()
            .rev()
            .nth(start - window_start - 1)
            .expect("context start")
            .0
    };
    let end_byte = line[start_byte..]
        .char_indices()
        .nth(window_end - window_start)
        .map_or(line.len(), |(index, _)| start_byte + index);
    let mut snippet = line[start_byte..end_byte].to_string();
    let mut display_column = start.saturating_sub(window_start) as u32 + 1;
    if window_start > 0 {
        snippet.insert(0, '…');
        display_column += 1;
    }
    if window_end < line_chars {
        snippet.push('…');
    }
    LinePreview {
        line_content: snippet,
        display_column: Some(display_column),
        display_match_length: Some(clamped_match_len as u32),
    }
}

#[cfg(test)]
#[path = "engine_perf_tests.rs"]
mod perf_tests;

#[cfg(test)]
#[path = "engine_cache_tests.rs"]
mod cache_tests;
