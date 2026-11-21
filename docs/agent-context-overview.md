# Agent Context Overview (Classic + Native GPUI)

This feature tracks the overall purpose of each **verified Host Agent terminal**
instead of using the current tool name, tool arguments, or the last “done/unread”
badge as the task summary.

## Model and update contract

- On the first meaningful user Hook prompt, capture an **overall goal**. Later
  prompts appear as milestones; they do not silently replace the goal.
- Hook assistant reports update **latest progress** and add report milestones.
  A report is a statement by the Agent, not independently verified evidence of
  implementation or successful tests.
- Milestones are **newest-first by event time**. Late Hook delivery is recorded
  chronologically and cannot overwrite a newer progress report.
- Exact terminal handle, Workspace ID, Tab ID and Agent adapter identify the
  scope. A new Agent adapter on a recycled handle does not inherit the old goal.
- Bounds: 600 characters for the goal, 420 for the latest progress, 260 per
  milestone, 16 retained milestones. Unbounded tool transcript and tool input
  are intentionally excluded.

## User interface

- **Classic**: the **default-off** Agent Settings switch **Show Agent Task Summaries in Sidebar** opts into task summaries; otherwise the original concise single-line row (and separate Tab Titles setting) is preserved. When enabled, the left Agent row shows up to four lines of overall goal plus
  two lines of recent progress. Users can expand full text. The right sidebar
  adds **Agent Context**, showing goal, progress and dated milestones; switching
  focused Tab changes the Agent whose summary is shown.
- **Native GPUI**: the same Rust Host-owned switch is exposed in its Agents Settings catalog, also default-off. The authenticated Host terminal inventory provides the same
  bounded overview. The right sidebar adds an Agent Context tab bound to the
  focused mixed-pane Host tab (checked against the full live identity). The
  left Runtime / Agents list shows multiline objective and progress.
- Native view selection is persisted with a backward-compatible optional
  sidebar-layout field.

## Safety and performance

- Hook status events and authenticated terminal inventories reuse the existing
  Host connection; no new polling loop or model calls are introduced.
- The status-only sidecar sanitizer **must not** forward the new context data.
- The summary is currently an in-memory, Hook-derived overview. It is **not**
  yet a semantic whole-transcript AI summary, a user-editable locked goal, or
  a durable record spanning Runtime Host restarts. Those require separate
  review of persistence, sensitive-text handling and rate-limited generation.
- The 16-event upper bound prevents history from growing during long sessions.

## Relevant validation

- Flutter: `test/unit/agent_context_overview_test.dart` and existing Agent
  status/runtime synchronization/projection unit tests.
- Rust Host: `cargo test --manifest-path rust/Cargo.toml -p alera-cli agent_context`
  and `agent_presence_push_is_bounded_redacted_and_retains_exact_host_identity`.
- Native GPUI: `cargo test --manifest-path desktop_gpui/Cargo.toml --lib agent_context_overview_tests`.
