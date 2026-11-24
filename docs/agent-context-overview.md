# Agent Context Overview (Classic + Native GPUI)

This feature tracks the overall purpose of each **verified Host Agent terminal**
instead of using the current tool name, tool arguments, or the last “done/unread”
badge as the task summary.

## Model and update contract

- Capture the first meaningful user Hook prompt as the **task anchor**. Later
  substantive instructions update the **current focus** inside the visible overall
  goal, while simple `繼續`/`continue` acknowledgments do not reset it. Explicit
  goal switches (`改成`/`switch to`) replace the anchor. Late Hook delivery cannot
  overwrite a newer focus.
- Native Claude/Codex completed-turn Hook fields (`last_assistant_message`,
  `last_agent_message`, plus compatible camel-case forms) update **latest work
  report** and add Agent-report milestones. When a native Stop Hook omits the
  final answer, the Host may read the last model-authored text from the exact
  registered and source-verified provider Session transcript; tool output,
  sidechains and other Worktrees/UUIDs never qualify. A report is a statement
  by the Agent, not independently verified implementation or test evidence.
- Milestones are **newest-first by event time**. Late Hook delivery is recorded
  chronologically and cannot overwrite a newer progress report.
- Exact terminal handle, Workspace ID, Tab ID and Agent adapter identify the
  scope. A new Agent adapter on a recycled handle does not inherit the old goal.
- Bounds: 600 characters for the evolving goal, 420 for the latest reply
  excerpt, 260 per milestone, 16 retained milestones. Long final answers
  retain a short opening and conclusion instead of losing their status at the
  truncation boundary. Tool input/output and hidden reasoning are excluded;
  native fallback reads at most 1 MiB per verified Stop event.

## Optional AI Assist report summaries

Classic Settings > AI Assist > Agent Work Reports configures the opt-in automatic summary, independent Agent/model/reasoning and instructions. The default is off. Automatic summaries currently support Codex, Claude Code and Pi. The Host sends only bounded completed model replies and goal excerpts to the selected provider; these may contain conversation details. It runs the existing AI Assist CLI in a separate job directory, never passes PTY output or tool arguments, strips Hook identity, and does not use a custom shell command. The raw report appears immediately, remains in the timeline and is the fallback on failure or unsupported configuration. At most four concurrent/queued session jobs are admitted; newer reports coalesce per session and stale completions cannot replace newer reports or another conversation. Classic and GPUI read the same Host summary; GPUI direct settings controls are not yet implemented. Updating or enabling the feature requires a compatible running Rust Host. This does not independently verify work claims.

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
  Host connection; no new polling loop or external model calls are introduced.
  Provider-native report fallback is only attempted for missing final answers
  on exact authenticated Stop events and runs on a bounded blocking worker.
- The status-only sidecar sanitizer **must not** forward private Context text.
  Host deltas attach only a non-sensitive `contextChanged` boolean when the
  task objective, latest report, or timeline changes. Native GPUI checks the
  exact running Session/Workspace/Tab identity, then reads a fresh authenticated
  Host inventory only on real Context changes. The existing periodic fallback
  still covers old Hosts, but no additional polling loop or unbounded refreshes
  are introduced.
- This remains an in-memory, Hook-derived, **model-authored reply excerpt**
  rather than a separate semantic whole-transcript AI summary. It is not yet a
  user-editable locked goal or durable across Runtime Host restarts. Those
  require a separate persistence, sensitive-text and rate-limited generation
  review.
- The 16-event upper bound prevents history from growing during long sessions.

## Relevant validation

- Flutter: `test/unit/agent_context_overview_test.dart` and existing Agent
  status/runtime synchronization/projection unit tests.
- Rust Host: `cargo test --manifest-path rust/Cargo.toml -p alera-cli agent_context`
  and `agent_presence_push_is_bounded_redacted_and_retains_exact_host_identity`.
- Native GPUI: `cargo test --manifest-path desktop_gpui/Cargo.toml --lib agent_context_overview_tests`.

## 2026-11-22 implementation verification

- Native Host evolving-goal, long-response excerpt and source-verified Claude/Codex history fallback: 7/7 tests passed; two-turn authenticated Host Hook report/goal regression passed.
- Original provider completion-field normalization and Stop-to-user-prompt guard both passed.
- Classic task summary, Agent controller and Hook normalizer regression suite: 51/51 passed; focused Dart static analysis: no issues.
- GPUI native Context parser: 3/3 passed, including goal + two model reports; status-only stream boolean privacy/schema regression passed; GPUI and Host production-source compile checks passed.
- Native sidecar redaction regression passed: sensitive prompt, tool input, reply and Context text never cross the status-only stream.
- Still pending: physical Windows GUI click-through, true provider-version compatibility and history backfill after Host restart. This is Hook-derived model-reply excerpts, **not** a separate semantic AI summarizer or durable long-term context store.

## 2026-11-24 native Hook report repair

- Live inspection before the repair: the Classic runtime Host exposed no
  contextOverview field, and two Codex sessions marked Done had no final
  assistant message. Both Hook endpoint listeners were alive and the Stop
  definitions existed. The registered Codex script was the older Classic
  alera-codex-hook.cmd, while the Host uses alera-runtime-agent-hook.cmd.
  This establishes the stale-host/data gap and a competing installer path;
  it does not by itself prove the cause of every missing provider reply.
- In production Classic, Codex reconciliation is now read-only. The Rust
  Host owns the native CODEX_HOME integration on startup, settings changes,
  and terminal launch. Legacy explicit installers remain available for
  compatibility tests. Turning off a Host's Hook processing must not erase
  another Host profile's shared Codex configuration.
- Completed-report deduplication compares the recorded Context, not only
  the lastAssistantMessage status field. A working-state text event can
  precede completion without losing the final report. Native Claude/Codex
  event aliases and camel-case transcript metadata are accepted.
- HTTP 204 now means the authenticated, parsed envelope entered the Host
  actor queue. Invalid body/envelope returns 413/400, unavailable actor
  returns 503, and invalid authorization returns 403. Actor diagnostics
  distinguish unknown session, tab/workspace mismatch, disabled Hooks,
  unsupported events, stale provider identity and missing Stop replies.
  Logs contain no prompt, report, transcript path, or authentication token.
- AI Assist auto summaries stay opt-in and disabled by default. Raw native
  Hook reports are independently tested with summaries disabled.

Validation on the isolated Windows worktree: 73/73 Flutter regressions
passed, including the actual managed Windows Codex Hook script, and
36/36 Rust regressions passed, including authenticated HTTP Hook delivery,
raw report generation with AI summaries disabled, verified transcript
fallback, and completion retry behavior.

Deployment requires rebuilding both the Host and Classic UI. No live
provider hooks or runtime settings were rewritten during this investigation;
existing Agent processes remain undisturbed. Verify with fresh Codex and
Claude turns on the updated Host before marking production recovery complete.
