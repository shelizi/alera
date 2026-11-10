# GPUI Agent Quotas / Classic compatibility (2026-11-10)

## Implementation

- Continue in the existing `integrate/gpui-p0p1p2-20261108` worktree; do not create or switch another worktree.
- Native uses the existing Rust Runtime Host `agentQuota.snapshot` contract. There is no new provider-specific authentication, scraping, environment parsing or quota storage in GPUI.
- `alera quota --no-start --json snapshot` is a read-only, bounded (60 seconds server request, 70 seconds native CLI) Host adapter. `--provider` and `--account-id` support single-account refresh without replacing unrelated rows. Missing/incompatible Host is a visible error, never a reason to start another Host.
- Native renders the same ten provider families as Classic, including account-specific Claude/Codex/OpenCode entries, Antigravity's five buckets, Fable, MiniMax model labels, remaining percentages, credit amounts and reset descriptions.
- Status bar refreshes automatically every 15 minutes, with a 45-second retry on failure. Clicking the status bar Quotas control opens a scrollable detail overview with whole-list and per-account refresh.
- Host remains the sole source of enabled providers and configured account aliases. A query failure preserves the last good readings and explicitly marks them stale.

## Scope and differences

- This batch delivers quota **display** and refresh. Classic's account switching, Codex reset-credit redemption, detailed usage/cost report and status-bar pin preferences are **not** added to Native here. In particular, Classic's `unpinnedQuotaKeys` is currently a Flutter-local preference and cannot be synchronized by inventing a second GPUI-local setting; migration into a shared Rust owner requires a separate contract change.
- Native opens the overview on click instead of Flutter's hover card. These are intentional UI differences and should be checked on a real Windows display.
- Local Host quotas are included; remote SSH quota forwarding remains a Classic-only feature.

## Verification

- `cargo test --offline --lib` from `desktop_gpui`: all Native library tests.
- `cargo check --offline --bin alera-gpui-desktop` from `desktop_gpui`: Windows native UI compile.
- `cargo test --offline -p alera-cli agent_quota_commands` from `rust`: no-start CLI contract.
- Manual visual acceptance still required: open Classic and Native against the same Host, compare provider/account list, percentage ordering and reset descriptions; confirm missing/offline Host and refresh do not freeze the GUI.

## Safety

- No Host start or credential file read from Native; no web request or provider CLI launch from GPUI.
- No changes to Classic Flutter screen, production deployment scripts, or the `main` branch.
