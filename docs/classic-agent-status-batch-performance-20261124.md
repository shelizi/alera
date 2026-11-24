# Classic Agent Hook and workspace activity batching (2026-11-24)

## Changes

- Copy the Agent status map lazily on the first accepted mutation in a Hook
  batch. Apply the remaining events to that private map in their original
  order, including resets, closes and interrupts. Publish the final map once.
  A batch with no accepted mutations keeps the previous map.
- Reuse equal existing Agent entries when replacing a runtime presence
  snapshot. A change in one session no longer replaces unrelated entries and
  their context maps. Preserve the existing stale snapshot and local-clear
  protection.
- Collect discrete Agent state transitions in one workspace activity batch.
  Retain the newest timestamp per workspace and update the activity map at
  most once. Tool metadata changes alone do not publish activity updates.
  Existing synchronous single-activity calls and the debounced persistence
  queue remain intact.

No Hook normalization, summary generation, provider detection, or user
interface behavior was changed. A real change to Agent status still reaches
its existing consumers; this is not a blanket suppression of status updates.

## Verification

- Core Hook, context overview, Agent controller, runtime presence and
  workspace activity/persistence suites: 67 passed.
- Downstream host forwarding, notification, desktop presence, sidebar
  ranking, status dot, compact summary and Dashboard suites: 48 passed.
- New regressions cover a 200-tool-event burst with 100 unrelated sessions,
  ordered close/interruption/restart and completed report handling, identity
  preservation of unchanged snapshot entries, and a 100-workspace transition
  batch with one activity publication and one persistence write.
- Riverpod code generation completed once. Kept the two affected provider
  generated files; excluded unrelated generated-file drift.

Operation-count regressions establish avoided work, not measured Windows
CPU, FPS or process memory gains. The installed Alera and running sessions
were not changed.

Targeted Dart analysis exited successfully with one pre-existing informational
annotate_overrides finding on recordActivity (no errors or warnings).
Formatter and Git whitespace checks passed.
