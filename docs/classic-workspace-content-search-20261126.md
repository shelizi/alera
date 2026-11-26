# Classic workspace content search performance (2026-11-26)

Base: main at 507a00500. Isolated branch perf/classic-workspace-content-search-20261126. No production switch, merge or release requested.

## Audit
- Classic full-text search in rust/src/api/workspace_search walks a fresh ignore-aware worktree on each query, checks each eligible path and reads up to 5 MiB of each text file sequentially. The Dart controller debounces input for 250 ms, cancels the previous generation and awaits a complete result. No cross-query content cache is used. The same engine supplies replacement preview and fresh replacement validation.
- Quick Open / explorer file-name search is separate and already has a Rust segment-prefix/character index and session caches. WorkspaceFileService retains query/result maps without a local per-entry query cap and eagerly replays historical query keys when rebuilding an index. This is an additional candidate for bounded LRU/recent visible queries, not a measured source of current latency.
- Editor text loading expands tabs into a separate display string even for files without tabs. A no-tab fast path and safe clean-content snapshot reuse are further candidates; do not bypass encoding or unsaved edits.

## Phase 1: exact matching / preview
For nonempty plain literals without CR/LF and without whole-word mode, use the same grep matcher on the remaining file buffer to find a candidate physical line. Count skipped newlines and re-run on the exact str::lines slice to preserve existing case/Unicode semantics, match IDs, line/column and cap behavior. Regex/whole-word and CR/LF queries retain line-based matching. Compute Unicode columns from the preceding nonoverlapping match; short previews avoid temporary Vec<char>, long previews count once per line and walk only a bounded context window, preserving ellipses and display coordinates. Stop a capped file when truncation is already established. No file discovery, metadata checks, read behavior, replace safety, binary exclusion or UI changes in this phase.

The initial independent old-engine oracle exposed a pre-existing byte-iterator panic: zero-width regex matches can advance into a multibyte UTF-8 character. Both optimized and reference matchers now omit unrepresentable byte offsets; a separate explicit test asserts the three valid zero-width character boundaries in a Kelvin-sign/emoji line. Reference otherwise retains the old per-line matching and Vec<char> preview. 17 query configurations × 5 limits × 17 texts × 2 preview modes × 2 initial total/truncation states = 5,780 comparisons of all returned fields and caps, including CRLF, Unicode folding, long lines, whitespace and regex. Explicit cancellation regression also passed.

## Measured matcher/preview phase
Standalone local Release harness contains the exact matching/preview functions, test oracle and direct dependency versions from the candidate (grep-matcher 0.1.9, grep-regex 0.1.14, regex 1.13.1, globset 0.4.20); transitive dependencies are independently offline resolved. Excludes the filesystem walker and Flutter FFI/UI. Three ordinary tests passed, one explicit benchmark passed. Seven alternating old/new rounds per workload; median ms per invocation, case-insensitive literal needle; corpus built before timing.

| Corpus | Bytes | Old ms | New ms |
|---|---:|---:|---:|
| Absent | 2,205,000 | 3.166980 | 0.207360 |
| Sparse | 2,205,022 | 2.607350 | 0.752410 |
| Dense long ASCII | 350,001 | 59.826633 | 0.144567 |
| Long Unicode | 700,840 | 46.125200 | 0.630100 |

These deliberately selected synthetic cases measure matching and snippet construction, not complete workspace search, disk latency or user-perceived speed. Dense tests exercise the 100-match per-file cap and previous repeated full-line allocation.

Actual pinned workspace native library focused regression: cargo +1.98.0 test --locked --offline -p alera_native --lib api::workspace_search -- --test-threads=4, 20 passed, zero failed, one ignored benchmark, 294 filtered, 2.76 s tests / 2m42s build. Includes real filesystem/ignore, nested worktree isolation, replacement conflicts, cancellation and oracle. Rustfmt and git diff checks passed. The broader actual native library regression also passed: 309 passed, zero failed, 6 ignored (46.71 s tests, 18.64 s incremental build).

## Next measurements and priorities
1. Time directory traversal, safe path verification, read, match, and FFI separately on representative synthetic and real workspace trees; preserve fresh and warm cases.
2. Bound Quick Open query/result maps, deduplicate inflight equivalent queries, and refresh recent/visible keys rather than replaying unbounded query history. Preserve both file-only and directory filter queries.
3. Bounded concurrent file reads and matching, preserving stable match IDs/order, cancellation, limits, ignore rules and worktree boundaries. Limit concurrency and bytes, not unbounded tasks.
4. Workspace-scoped memory-budgeted content cache / candidate trigram index with file watcher invalidation, rename/delete, ignore changes, Git checkout and uncertain-event rescan. Validate metadata and content freshness; never use cached results as authority for writes. Keep short strings, regex and unusual Unicode cases on a correct fallback.
5. UI responsiveness: measure first-result time, adaptive debounce, incremental/virtualized result rendering, and no-tab editor fast path. No claims of speedup before controlled tests.
