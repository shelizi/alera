# Classic Terminal optional code-copy overlay — 2026-11-24

## Why this is a performance hot spot

The Classic Terminal writes PTY output into xterm with an adaptive frame budget.
The independent code-copy overlay observes each terminal change and scans
rendered rows for fenced or canonical Claude source blocks. Each lookup can
scan a bounded but substantial scrollback range. At sustained output rates,
this optional overlay work can repeat even when the copy controls have not
materially changed.

## Changes

- Keep interactive terminal parsing, PTY sequencing, visibility and the
  configured outputRefreshFps completely unchanged.
- Refresh the first new output immediately, then pace subsequent
  output-triggered copy-overlay scans to a minimum spacing of 100 ms; while
  output continues, retain a pending trailing refresh rather than losing the
  latest visible code block.
- User-scroll, canonical source notifications and session changes continue
  to schedule an immediate refresh. Pending output timers are cancelled
  when an immediate refresh wins, when the observed terminal changes, or
  when the widget unmounts.
- Fetch Claude source blocks once per viewport refresh instead of repeatedly
  allocating the same list for every visible candidate row. At click time
  copy content is still re-read and revalidated to prevent stale copies.
- Add a widget regression with a counter of completed overlay scans
  (not frame rendering): the first output refreshes immediately and 60
  subsequent fast PTY chunks coalesce to one scan after the throttle window.

## Verification

- A first attempt paced the initial terminal output and caused nine existing
  code-copy Widget regressions; the implementation was corrected so the first
  output after an idle terminal is always immediate.
- Terminal Surface Widget suite: **64/64 passed** after this correction,
  including canonical Claude source, large code blocks, scrollback, Composer,
  terminal handoffs, and active code-copy controls.
- Terminal Runtime native unit suite: **143 passed, 2 skipped, 0 failed**.
  Covers PTY output/backpressure, session teardown and handoffs,
  snapshot restoration, renderer callbacks, and parser-worker behavior.
- The new Widget test verifies **first-output immediate** plus one trailing
  overlay scan for **60 rapid PTY line updates** (including scrollback).
  This is a deterministic test observation, not a measured desktop frame rate.
- Dart formatter completed for all three modified Dart files (0 changes).
  Staged Git whitespace check completed successfully.
- Targeted `dart analyze` on the modified Terminal runtime part file reached
  the 120-second process time limit with only "Analyzing..." output.
  **Inconclusive**, not a successful analyzer run or reported diagnostics.
  Both Flutter test suites compiled and exercised the modified source.

## Limitations

100 ms applies to background-output-triggered **copy-overlay scans**, not
terminal rendering or keyboard echo; these retain their existing cadence.
Code-copy controls may update within the next 100 ms under sustained output.
The optimization does not speed up xterm parsing itself.

No physical Windows GUI baseline/candidate frame-time, CPU or memory
comparison has been performed. Test counts and source-level complexity are
not a substitute for a matched native Windows A/B profile.
