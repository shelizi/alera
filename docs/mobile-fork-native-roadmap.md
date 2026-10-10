# Alera mobile fork-first expansion

Updated: 2026-10-10

## Strategy and boundary

Maintain the mobile companion as a Flutter client of the Alera Rust runtime.
Do not wholesale-copy another mobile application or bypass the runtime's
allowlisted authenticated WebSocket RPCs. New additive features use capability
strings advertised in `mobile.hello`; the strict-equality mobile protocol
version is intentionally unchanged. Build and commit each slice in
`feat/mobile-fork-native`, leaving `main` and its pending local edits intact.

## Completed implementation slices

| Slice | Commit | Result |
|---|---|---|
| 1 | `93ed8648` | Independent mobile quota-card refresh without clearing unrelated cards |
| 2 | `1ac93fa5` | Account-scoped quota query using the existing Rust `agentQuota.snapshot` provider/accountId filters, gated by `agentQuotaPartialRefreshV1`; old-runtime fallback |
| 3 | `74575821` | Search local branches, reuse a matching active workspace or create a managed sibling, and browse protected workspace files with bounded UTF-8 text previews |
| 4 | `1f937098` | Read-only, authenticated, bounded HEAD-to-worktree single-file Git patches gated by `mobileWorkspaceFileDiffV1` |
| 5 | `b87ec31f` | Source Control changed-file list gated by `mobileWorkspaceGitStatusV1`, max 200 entries, plus correct diff patch line prefixes and mobile failure logging |
| 6 | `bbb63980` | Fix capability-probe WebSocket test teardown races and document mobile feature/release scope |

The latest committed `main` was incorporated into this isolated worktree as
merge commit `9dec1b90` without modifying the main worktree's pending changes.

## Validation

- `dart test test/mobile_workspace_git_status_test.dart test/mobile_workspace_text_preview_test.dart`: **6 passed**.
- `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked workspace_git_`: **3 passed** (two Git RPC regressions and one existing cleanup regression).
- `dart analyze` targeted mobile Git/status/diff/preview/controller files: **no issues**.
- `flutter test --no-pub test/workspace_branch_picker_test.dart`: **2 passed**.
- `flutter test --no-pub test/agent_quota_controller_test.dart test/mobile_runtime_client_test.dart test/workspace_branch_picker_test.dart test/mobile_workspace_text_preview_test.dart test/mobile_workspace_git_status_test.dart`: **21 passed** (including real loopback WebSocket RPC negotiation). Two pre-existing capability checks were made deterministic by draining their post-auth background `status.get` probe before test socket teardown.
- `flutter analyze --no-pub` for the whole mobile app: **no issues**.
- `flutter test --no-pub` for the whole mobile app on Windows: **439 passed, 4 skipped, 0 failed**. The terminal WebSocket harness now waits for server upgrade registration before emitting output resync events; all eight of its isolated tests pass.
- The direct OpenAI settings golden is explicitly skipped on Windows only:
  Windows rasterization differs by 1.03% (3,402 pixels) from the checked-in
  golden. The exact screenshot comparison remains active on Linux CI, and
  all functional AI dictation settings widget assertions still run on Windows.
  The other three skips are the existing relay/edge integration fixtures.
- Android build, signed release and physical-device smoke tests remain
  separate release gates, not implied by these green tests.

## Compatibility, security and limits

- Runtime validates the paired device and routes all three new capabilities
  through the existing mobile allowlist; an older host simply omits capability
  advertisement and the phone hides unsupported UI or falls back for quotas.
- The Git review RPCs accept a registered workspace id, validate the canonical
  Git root and reject path traversal. The patch returns at most 512 KiB of
  text, Git status at most 200 changed paths. They do not execute arbitrary Git
  commands and cannot stage, reset, commit, delete or push files.
- New/untracked files in Source Control open text previews; they are not in
  the HEAD-to-worktree tracked diff. File preview itself allows only bounded
  runtime-owned reads and refuses binary-to-text rendering.
- Existing account/session, Tailscale/NetBird, relay and notification pathways
  are reused without new access to secret credential values.

## Remaining release gates and subsequent parity work

1. **Runtime + phone integration**: test one real Android/iOS device using a
   current Rust host and one older host; verify pairing, reconnect, quotas,
   branch selection, source control and file path isolation.
2. **Android build prerequisites on `home-rust`**: `flutter doctor -v` reports
   Android SDK 35, while Flutter 3.47.2 requires SDK 36 and Build Tools 28.0.3;
   some Android SDK licenses are unaccepted. Fix SDK provisioning/acceptance
   before relying on local APK builds, or use the existing GitHub Actions
   mobile-build workflow. Do not publish a release based on these unit tests.
3. **iOS**: needs macOS runner, Apple signing and provisioning. Background
   notifications still need real Firebase/APNs configuration and device checks.
4. **Richer file review**: folder tree, large-file pagination, syntax-highlit
   read-only content, binary/image preview, side-by-side/intraline diff,
   commit history and arbitrary two-revision comparisons are not in this slice.
5. **Mutating Git actions**: staging, commits, branch checkout in-place and
   network operations need independent runtime authorization, clean-worktree
   checks, confirmation UX and dedicated tests before being offered to mobile.
6. **Non-terminal tabs**: mobile still cannot render desktop-only tab contents.

## Commands

Run from repository root unless indicated:

```bash
dart format mobile/lib/src/features/workbench
git diff --check
cd mobile && flutter pub get && flutter analyze && flutter test
cd .. && cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked workspace_git_
```

Never merge to or deploy from `main` until the required release gates above
have been validated. Existing main worktree changes are not part of this branch.
