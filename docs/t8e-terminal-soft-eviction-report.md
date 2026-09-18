# T8e Phase 1 - Parser Worker Soft Eviction

Date: 2026-09-18

## Goal

Post-T8d profiling showed that full ANSI snapshot parsing inside the parser worker remained the stable dominant restore cost at roughly 0.92-0.95 seconds median. T8e Phase 1 removes that parse from the common in-process buffer-eviction/reveal path.

## Design

When a terminal is hidden, the parser worker is already the authoritative terminal model:

- hidden PTY output is parsed by the worker;
- the UI replica is intentionally allowed to become stale;
- reveal already has a packed worker-to-replica full-buffer synchronization path.

T8e-P1 changes buffer-budget eviction for a ready parser-worker session from hard eviction to **soft UI-buffer eviction**:

1. retain the existing session handle;
2. retain the PTY attachment;
3. retain the parser worker and its authoritative xterm state;
4. detach and dispose the populated UI replica;
5. replace it with a fresh empty replica and report its UI-buffer usage as zero;
6. continue parsing hidden PTY output in the retained worker;
7. on reveal, hydrate the fresh replica from one packed structured worker snapshot.

No ANSI snapshot reparse is required on this path.

## Fallback boundary

Soft eviction is used only when all of these are true:

- parser-worker backend is enabled;
- the worker has completed startup;
- the session is hidden;
- the session is not disposed;
- the UI buffer has not already been softly evicted.

Otherwise the existing hard-eviction behavior remains unchanged. Hard eviction still detaches the local PTY/session handle and relies on the host full snapshot on reattach.

This phase intentionally does **not** change durable host snapshot storage or the Rust terminal-host cursor protocol.

## Memory semantics

T8e-P1 releases the duplicate **UI replica cell buffer**. The parser worker's authoritative terminal buffer remains resident. Therefore:

- configured UI buffer-budget accounting treats the softly evicted replica as zero resident UI-buffer bytes;
- real process memory is reduced by the UI-side duplicate buffer, not by the worker-side authoritative state;
- this is not equivalent to a complete terminal-memory eviction.

A future hard-eviction structured snapshot needs a host resume cursor or equivalent durable structured state if the worker itself must also be released.

## Correctness coverage

Regression coverage verifies that after soft eviction:

- the same runtime session handle remains registered;
- the PTY is not disposed;
- the UI buffer is empty and reports zero budget bytes;
- hidden output received during eviction still reaches the parser worker;
- reveal restores both pre-eviction and during-eviction output;
- reveal clears the soft-eviction state.

Existing non-worker and not-ready eviction paths continue to use hard eviction and their previous tests remain unchanged.

## Native Windows performance

Workload uses the same 2.56 MB / 420k ESC snapshot used by the restore benchmark. The full ANSI parse is performed once before measurement. Each measured sample then performs:

soft UI eviction -> visibility acquire -> worker packed full-buffer snapshot -> UI replica apply -> framework pump

### Run A

- samples: 526.33, 545.91, 204.90, 259.37, 278.83 ms
- median: **278.83 ms**
- p95/max: **545.91 ms**
- <= 3 s: **5/5**

### Run B

- samples: 234.78, 155.77, 163.22, 137.00, 195.12 ms
- median: **163.22 ms**
- p95/max: **234.78 ms**
- <= 3 s: **5/5**

All 10 measured samples passed the 3 second target.

Compared with the two clean post-T8d full-restore medians of about 1.60 seconds, the soft-eviction reveal path reduces median latency by roughly 82-90%, while avoiding the previously measured 0.92-0.95 second full ANSI parse.

## Validation

- full terminal_runtime_native_test.dart: **136 passed, 2 Windows platform skips**
- eviction-focused tests: **11 passed**
- T8e focused soft-eviction regression: PASS
- benchmark analyzer: clean
- touched-file analyzer: clean
- git diff --check --ignore-submodules=all: clean
- native Windows soft-reveal benchmark: 10/10 samples under 3 seconds

## Next phase

**T8e-P2 - hard-eviction / reconnect structured resume**

If full hard eviction must also avoid reparsing the complete ANSI scrollback, preserve a structured snapshot together with an absolute host output cursor. Reattach can then:

1. hydrate the structured snapshot locally;
2. ask the host for output after the retained cursor;
3. parse only the delta accumulated while detached;
4. fall back to the existing full ANSI snapshot when the cursor is outside the host ring or incompatible.

That phase crosses the Flutter/Rust terminal-host protocol boundary and should remain a separate commit from T8e-P1.

## T8e Phase 2 - Cursor-aware reconnect resume

Status: **resume protocol complete**.

Phase 2 keeps the Phase 1 parser worker as the authoritative structured terminal state, but changes what happens while that UI replica is softly evicted:

1. the runtime parks terminal-host output for the evicted hidden session;
2. the host pause reply returns the exact absolute output cursor actually delivered to that client;
3. Dart retains that cursor only for the parked checkpoint;
4. if the host attachment survives, reveal uses the existing host delta-resume path;
5. if the attachment is lost while parked, `createOrAttach` sends `resumeCursor`;
6. the Rust host accepts the cursor only when the complete gap is still in the scrollback ring and sends only that delta on the terminal lane;
7. stale/future cursors, or terminal-lane backpressure while sending the gap, fail closed to the existing full ANSI snapshot.

The absolute cursor is deliberately **not** retained for an ordinary visible attachment. Visible output continues to advance parser state, so an attachment-response cursor would immediately become stale and must not later be reused as a structured checkpoint.

### Correctness and fallback boundary

The reconnect delta is ordered ahead of the control reply by the existing sequenced terminal/control lanes. A pause reply records the delivered cursor after already accepted terminal frames; on reattach the host validates the cursor against `[ring_base, stream_end]`. If the range cannot be satisfied exactly, the client gets a full snapshot rather than a partial splice.

This phase still does **not** destroy the parser worker. The packed worker snapshot is sufficient to rebuild the UI replica, but not yet sufficient to recreate every parser-semantic state needed to continue parsing arbitrary future bytes safely. Full worker hard eviction therefore remains deferred until xterm exposes/imports the required state, including SGR attributes, scrolling margins, saved cursor state, alternate-screen and interaction modes, and equivalent parser state.

### Phase 2 validation

- full `terminal_runtime_native_test.dart`: **136 passed, 2 Windows/POSIX platform skips**;
- full `terminal_host_pty_session_test.dart`: **23/23 passed**;
- soft-eviction park/reveal focused regression: PASS;
- Rust `create_or_attach_*` cursor contracts: **3/3 passed**;
- Rust output-resume contracts: **6/6 passed**;
- touched-file Dart analyzer: no errors/warnings (one style-only info for a null-aware collection element);
- `git diff --check --ignore-submodules=all`: clean.