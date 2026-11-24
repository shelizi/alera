# Classic Terminal buffer-budget recheck optimization (2026-11-24)

## Problem

The budget counts resident UI buffers. A parser-worker terminal whose UI
replica has already been evicted reports zero bytes. Previously it could be
selected again when a visible terminal alone exceeded the budget. The session
owner would fall through to detachment because that replica was already
evicted, discarding the retained parser path without releasing any additional
budgeted UI memory.

The budget also enumerated all live sessions when configured as unbounded,
and measured every scrollback when all sessions were protected by visibility.

## Change

- Exclude zero-byte usage from eviction candidates; keep total accounting
  and positive-byte least-recently-visible eviction order intact.
- Return before enumerating sessions when the budget is unbounded.
- Return before measuring buffer storage when all sessions are visible.
- Preserve the existing explicit close/release policy and parser hard/soft
  eviction safety checks. This does not claim that the retained worker or
  serialized state itself uses zero process memory.

## Verification

- The new budget regression reproduced the bug on the original algorithm:
  expected only the cold positive-byte tab, but also selected the previously
  evicted tab.
- Corrected asynchronous test fixture to send bounded chunks and await worker
  completion; the first native test attempt was stopped due to the fixture's
  synchronous drain loop.
- Budget and native terminal suites: 158 passed, 2 skipped, zero failures.
  Includes soft and hard parser rechecks with a visible terminal exceeding
  the budget, retained handle identity, restored history and live PTY.
- Terminal surface widget suite: 64/64 passed.

No running installation or agent was changed. Native Windows process-memory,
CPU and latency improvements remain to be measured.

Targeted Dart analysis reached the 60-second limit without diagnostics; no static analyzer pass is claimed. Formatter (5 files) and Git whitespace checks passed.
