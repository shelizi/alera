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

Phase 2 by itself did **not** destroy the parser worker. T8e-P3 below closes that remaining boundary by exporting/importing the parser-worker emulator state for an explicitly eligible safe subset, while failing closed to the Phase 2 soft-eviction path for unsupported or ambiguous parser state.

### Phase 2 validation

- full `terminal_runtime_native_test.dart`: **136 passed, 2 Windows/POSIX platform skips**;
- full `terminal_host_pty_session_test.dart`: **23/23 passed**;
- soft-eviction park/reveal focused regression: PASS;
- Rust `create_or_attach_*` cursor contracts: **3/3 passed**;
- Rust output-resume contracts: **6/6 passed**;
- touched-file Dart analyzer: no errors/warnings (one style-only info for a null-aware collection element);
- `git diff --check --ignore-submodules=all`: clean.

## T8e Phase 3 - Eligible parser-worker hard eviction

Status: **complete**.

Phase 3 adds a structured retained-state contract to `TerminalXtermWorker` and uses it to release the parser-worker isolate after a hidden session has already entered the Phase 2 parked-output state.

### Runtime flow

For a ready hidden parser-worker session:

1. the duplicate UI replica buffer is discarded exactly as in Phase 1;
2. terminal-host output is parked exactly as in Phase 2;
3. the worker command tail exports a retained emulator checkpoint;
4. if the checkpoint is eligible, the runtime stores the compact checkpoint and closes the parser-worker isolate;
5. reveal, or any later parser command, starts a fresh worker from that retained checkpoint;
6. the existing packed worker snapshot hydrates the UI replica;
7. output accumulated behind the parked host cursor continues through the Phase 2 delta-resume path.

The hard-eviction export is serialized on the same worker command tail as parsing, resizing, and reveal. Generation, terminal-identity, eviction-state, and visibility checks are repeated after the asynchronous export. A reveal that wins the race therefore keeps the live worker rather than closing it underneath the visible session.

### Retained checkpoint

The retained state covers the emulator state required by the currently validated shell/TUI paths:

- main and alternate screen buffers;
- cursor positions and saved cursor state;
- vertical and horizontal margins;
- current SGR cursor style;
- active main/alternate buffer;
- insert, line-feed, cursor-key, origin, wrap/reverse-wrap, keypad, mouse, focus, bracketed-paste, cursor-visibility/blink, cursor-line-highlight, left-right-margin, Kitty-keyboard, modify-other-keys, protection, and related interaction modes;
- title/icon title and focus state;
- preceding code point used by `CSI b` repeat-character semantics.

Buffer cells are retained as packed `Uint32List` payloads with five 32-bit words per cell plus two row-metadata words and sparse combining-character strings. This deliberately avoids replacing the released worker's xterm object graph with another per-cell Dart object graph in the UI isolate.

### Eligibility / fail-closed boundary

The worker is hard-evicted only when the export reports no blockers. Current blockers include:

- parser not in ground state, for example a split CSI/OSC/DCS sequence;
- active synchronized update;
- ambiguous pending wrap at the last column;
- custom color overrides;
- hyperlink or semantic shell-integration state;
- custom tab stops or charsets;
- saved DEC mode stacks;
- Kitty keyboard stacks;
- title stacks;
- cursor extended semantic/hyperlink state.

These cases keep the Phase 2 parser worker alive. They still release the duplicate UI replica and keep host output parked, so correctness falls back to the already validated soft-eviction behavior instead of approximating missing state.

### Correctness validation

Worker-level round-trip coverage closes the critical continuation property: a control worker and an exported worker receive the same prefix, the exported worker is closed, a fresh worker imports the retained checkpoint, both receive the same subsequent ANSI stream, and packed buffers, cursor state, interaction modes, cell attributes, and `CSI b` continuation remain equal.

Additional gates verify that a split CSI is rejected until the parser returns to ground state and that OSC 8 hyperlink state rejects hard eviction.

Validation on 2026-09-18:

- `terminal_xterm_worker_test.dart`: **25/25 passed**;
- parser-worker focused runtime regression: **14/14 passed**;
- full `terminal_runtime_native_test.dart`: **137 passed + 2 Windows/POSIX platform skips**;
- runtime analyzer: **No issues found**;
- worker analyzer: no errors/warnings; one existing `use_super_parameters` style info;
- `git diff --check --ignore-submodules=all`: clean.

### Measurement boundary

Phase 3 establishes correctness and a compact retained representation, but this batch does **not** claim a measured process-RSS reduction yet. The next evidence step, if required, is a native Windows multi-session RSS comparison of P2 soft UI eviction with the parser worker retained versus P3 eligible hard eviction with only the packed retained checkpoint, plus reveal/startup latency from the packed checkpoint.

Do not infer an RSS percentage from the packed payload shape alone.
