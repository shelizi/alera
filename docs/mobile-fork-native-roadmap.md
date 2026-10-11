# Alera mobile fork-first expansion

Updated: 2026-10-11

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

### October 11 follow-up implementation

- Protected `mobile.workspaceDirectory.list` RPC (`mobileWorkspaceDirectoryBrowseV1`): paged folder navigation, directories sorted first, optional hidden files, no VCS metadata or symlinks, 10,000-entry scan ceiling. Legacy phones/hosts continue using the Quick Open picker.
- `mobile.workspaceFile.write` RPC (`mobileWorkspaceTextEditV1`): only existing UTF-8 files up to 256 KiB, explicit mobile confirmation, SHA-256 optimistic concurrency, typed `mobile_workspace_file_changed` conflict, atomic no-follow sibling replacement and permission preservation. The source file is never overwritten if the checksum differs.
- Regression tests cover paginated directory security, existing-file save and stale-digest refusal, WebSocket capabilities/payloads, strict UTF-8 mobile baseline and editor size limitations. Runtime and release verification remain separate gates.
- Passive selectable Markdown formatting with raw-view toggle, without loading untrusted remote images or executing workspace HTML/JS. Git unified diff highlights additions/deletions/hunks while retaining literal patch text.
- Git per-file `mobile.workspaceGit.stage` (`mobileWorkspaceGitStageV1`): user-confirmed Stage/Unstage via registered workspace and exact path, no shell commands, no resetting workspace content; rename/conflict paths require desktop review. Network Git operations, Commit and Push remain unavailable on phone.
- `mobile.workspaceGit.history` (`mobileWorkspaceGitHistoryV1`) offers read-only HEAD history from the registered workspace, bounded to 100 commits; the phone displays summary, author, date, full message and parent SHA, and can copy a commit hash. Arbitrary ref lookup, reset, commit, push and history rewrites are not exposed.

The latest committed `main` was incorporated into this isolated worktree as
merge commit `9dec1b90` without modifying the main worktree's pending changes.

## Validation

### 2026-10-11 follow-up

- `flutter test --no-pub` (entire Flutter suite): **451 passed, 4 skipped, 0 failed**; includes the new Stage/Unstage and Git history WebSocket contract tests.
- `flutter analyze --no-pub` (entire mobile project): **exit 0, no issues**; touched-file `dart analyze` also passed.
- `dart format --output=none --set-exit-if-changed` on touched Dart code: **no changes required**.
- `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked mobile_git_`: **2 passed**. Verifies index-only Stage/Unstage preserving worktree contents and bounded HEAD history with parent hashes.
- Signed APK, runtime-to-physical-phone and iOS smoke checks remain separate release gates.

### Earlier baseline validation

- `dart test test/mobile_workspace_git_status_test.dart test/mobile_workspace_text_preview_test.dart`: **6 passed**.
- `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked workspace_git_`: **3 passed** (two Git RPC regressions and one existing cleanup regression).
- After integrating the latest `main`, `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked mobile_codex_file_surface_tests`: **3 passed** (mobile.hello capability advertisement and mobile request allowlist). An extra post-merge repeat of `workspace_git_` exceeded a 120-second Windows Rust recompilation limit before tests began; the three `workspace_git_` assertions had already passed before the merge and did not report a new assertion failure.
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
  text, Git status at most 200 changed paths, and history at most 100 commits.
  They do not execute arbitrary Git commands. Scoped stage/unstage affects only
  the index; reset, commit, delete and push are not exposed.
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
4. **Richer file review**: tree is available with folder-by-folder pagination;
   further work includes large-file **editing**, syntax-highlighted content,
   binary/image preview, side-by-side/intraline diff,
   history beyond the latest 100 commits, and arbitrary two-revision comparisons.
5. **Mutating Git actions**: explicit-confirmation per-file Stage/Unstage is now
   available, while commits, branch checkout in-place and network operations
   need independent runtime authorization, clean-worktree
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
