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
- `mobile.workspaceFile.rename` (`mobileWorkspaceTextRenameV1`): confirmed same-folder rename of already-read, writable UTF-8 files up to 256 KiB; SHA-256 optimistic precondition, pinned no-follow directories, hard-link no-clobber destination reservation and source/destination file-ID verification. Existing names or stale file content cause a conflict. Small text file moves to another existing directory are supported by `mobileWorkspaceTextMoveV1`; directory moves and large/binary renames still require separate review. Small text file recovery is implemented through `mobileWorkspaceTextTrashV1`.
- `mobile.workspaceFile.create` (`mobileWorkspaceTextCreateV1`): creates only new UTF-8 files up to 256 KiB under already-existing, no-follow workspace directories. Atomic `create_new` refuses conflicts and never overwrites existing text or symlink targets; phone confirms destination and preserves drafts on failure. Same-directory small text rename and recoverable text removal are implemented separately; arbitrary moves and irreversible deletion remain unavailable.
- Regression tests cover paginated directory security, existing-file save and stale-digest refusal, WebSocket capabilities/payloads, strict UTF-8 mobile baseline and editor size limitations. Runtime and release verification remain separate gates.
- Passive selectable Markdown formatting with raw-view toggle, without loading untrusted remote images or executing workspace HTML/JS. Git unified diff highlights additions/deletions/hunks plus bounded Unicode-safe intraline changed character spans for unambiguous equal-sized +/- groups, while retaining literal copyable patch text.
- Git per-file `mobile.workspaceGit.stage` (`mobileWorkspaceGitStageV1`): user-confirmed Stage/Unstage via registered workspace and exact path, no shell commands, no resetting workspace content; rename/conflict paths require desktop review. Network Git operations, Commit and Push remain unavailable on phone.
- `mobile.workspaceGit.history` (`mobileWorkspaceGitHistoryV1`) offers read-only HEAD history from the registered workspace, bounded to 100 commits; the phone displays summary, author, date, full message and parent SHA, and can copy a commit hash. Arbitrary ref lookup, reset, commit, push and history rewrites are not exposed.
- `mobile.workspaceGit.compare` and `mobile.workspaceGit.revisionPatch` (`mobileWorkspaceGitCompareV1`) let the phone view a commit's changes versus its first parent (or an empty initial tree), select any two listed commits as base/target, browse up to 200 changed files, and open a colored, selectable exact-file historical patch truncated at 512 KiB. All requested commit hashes must be full SHA-1 IDs reachable from the selected worktree HEAD; pathspec matching is disabled and protected paths are rejected.
- Git history file comparisons and reusable patch view were committed as `31e77cef` in the isolated `feat/mobile-fork-native` branch. The shared Cargo target cache issue was eliminated by running the repeat authorization and Git integration regressions with an isolated target directory on drive D.

The latest committed `main` was incorporated into this isolated worktree as
merge commit `9dec1b90` without modifying the main worktree's pending changes.

## Validation

### 2026-10-11 read-only Git diff intraline highlighting

- Detailed current handoff for the two follow-up feature commits and Android environment: [`mobile-native-move-diff-handoff.md`](mobile-native-move-diff-handoff.md). Cross-directory commit `a8c2a953`; intraline commit `daa5daa5`.

- The existing `MobileGitPatchView` now highlights changed Unicode characters within **unambiguous equal-count removed/added runs**, up to 8 lines per run and at most 4096 UTF-16 units per line. Pairing and start/end indices respect Unicode runes. Unequal groups, long lines, unchanged paired content and file headers keep the previous whole-line styling.
- The underlying patch bytes/text, `+`/`-` markers, selectable copy and 512 KiB patch-size cap remain unchanged. This is presentation-only; no additional host RPC or arbitrary Git access.
- `flutter test --no-pub test/mobile_git_patch_lines_test.dart test/mobile_git_patch_view_test.dart`: **8 passed**. `flutter test --no-pub` complete suite: **471 passed, 4 skipped, 0 failed**. `flutter analyze --no-pub`: **no issues**.
- Android build smoke: `flutter build apk --debug --no-pub` timed out at Gradle `assembleDebug` after ~311 seconds with no detailed Gradle error and **no `app-debug.apk` output**. Windows SDK platforms remain 33/34/35 and build tools 30.0.3/34.0.0/35.0.0; app `compileSdk=37`, so the Android environment is not yet release-ready. Do not report a successful APK or attribute the timeout to an unverified single cause. License acceptance remains an operator step.

### 2026-10-11 safe cross-directory mobile text move

- `mobile.workspaceFile.move` (`mobileWorkspaceTextMoveV1`) is a separately negotiated, authenticated Runtime mutation. Phone opens a paginated **folder-only** picker, confirms the source and destination, and sends `{workspaceId, relativePath, destinationDirectory, expectedSha256}`. Workspace root uses an explicit empty destination string.
- Only unchanged writable UTF-8 files up to **256 KiB** may move between existing directories in the same registered workspace. Rust `cap_std` handles reject symlink traversal and VCS metadata. The destination is reserved with a **no-clobber hard link**, file identities and SHA-256 are rechecked before the old name is removed, and metadata is preserved. Different filesystems fail without a copy/delete fallback; arbitrary large/binary-file and directory moves remain unavailable. As with trash, external noncooperating filesystem writers can race an unlink; this is not an atomic OS-level move-if-identity operation.
- Rust core `workspace_files::move_file::tests`: **2 passed** (existing-name conflict, stale SHA, missing and protected folders, safe round-trip and retained OS file identity, root folder destination). Runtime Gateway `mobile_codex_file_surface_tests`: **3 passed** including capability advertisement and mobile allowlist. Flutter move/rename/trash WebSocket tests: **3 passed**. Destination-picker Flutter widget test: **1 passed**, showing only directories, returning a selected folder, supporting the workspace root, and disabling the current source folder. Full Flutter suite: **466 passed, 4 skipped, 0 failed**. `flutter analyze --no-pub`: **No issues**.
- Signed APK/Android emulator and real paired Android/iOS smoke tests remain later release gates. Never expose an unguarded cross-volume copy-and-unlink fallback.

### 2026-10-11 recoverable mobile workspace text deletion

- Detailed implementation/release handoff: [`mobile-workspace-recovery-handoff.md`](mobile-workspace-recovery-handoff.md). Feature commit: `176c3ef5`.

- `mobile.workspaceFile.trash`, `mobile.workspaceTrash.list`, `mobile.workspaceTrash.restore` (`mobileWorkspaceTextTrashV1`) operate only on an authorized workspace ID. The phone confirms before removal and restore, and never exposes a permanent-delete button. Old runtimes do not display unsupported actions.
- The host requires a complete SHA-256-matched writable UTF-8 file at most 256 KiB, a real registered workspace root and no-follow path traversal. A verified copy with its SHA, original path, ID and timestamp is fsynced under `<runtime-dir>/mobile-workspace-trash/<sha256(workspace-id)>/` **before** removing the source name. Other workspaces cannot list or restore it. Backup contents do not travel to the phone in the listing endpoint.
- Recovery only uses atomic create-new at the original path; if another agent has recreated that name, restore returns a conflict and retains its backup. Capacity is bounded to 64 entries / 16 MiB per workspace; when full, deletion is rejected before any unlink. No background expiration/purge is enabled because this would silently destroy the recovery path.
- `cargo test --manifest-path rust/Cargo.toml -p alera-core --locked workspace_files::trash::tests` with isolated `CARGO_TARGET_DIR`: **4 passed**. Covers durable restoration, stale checksum and existing-path conflict, workspace ID isolation, reject protected/traversal paths, Runtime directory outside repository, reopening persisted backups, full 64-entry refusal without source loss, and rejection of corrupted manifests.
- `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked mobile_codex_file_surface_tests`: **3 passed** (Gateway mobile capability and authenticated route allowlist; test build completed). `flutter test --no-pub test/mobile_workspace_trash_client_test.dart test/mobile_workspace_rename_client_test.dart test/mobile_workspace_create_client_test.dart`: **3 passed**. `flutter test --no-pub`: **464 passed, 4 skipped, 0 failed**; `flutter analyze --no-pub`: **no issues**. Rustfmt, Dart format and Git diff checks passed.
- `flutter doctor -v` on Windows confirms Android 14 emulator, but reports Android SDK 36, Android Build Tools 28.0.3 and license acceptance prerequisites missing. Signed APK, physical Android/iOS and runtime-to-phone E2E remain unverified release gates. iOS signed builds require a macOS host.

### 2026-10-11 guarded mobile text rename

- `mobile.workspaceFile.rename` (`mobileWorkspaceTextRenameV1`) is a separate, authenticated and conflicting runtime mutation. The phone offers it only for fully read, valid UTF-8 text files up to 256 KiB with a SHA-256 baseline; the new name stays in the same directory. The Rust core reserves the name using a capability-directory hard link, checks original/target file IDs and content, and removes the old name only when validation passes. A destination collision never overwrites a file; renamed directories, binary files, moves and permanent deletion are not supported.
- `cargo test --manifest-path rust/Cargo.toml -p alera-core --locked mobile_`: **4 passed**, including real filesystem rename no-clobber/stale-hash protection, safe existing edits/create, DOS device/Windows alternate-stream names and Unicode support. The mobile filename validator mirrors the server restrictions.
- Mobile filename/rename/create focused tests: **4 passed**; `flutter test --no-pub` complete suite after filename hardening: **463 passed, 4 skipped, 0 failed**; `flutter analyze --no-pub`: **No issues**. Runtime Gateway mobile capability/allowlist `mobile_codex_file_surface_tests`: **3 passed** with isolated Cargo target. Signed app/device integration remains a separate gate.
- Recoverable deletion is handled by the separately negotiated `mobileWorkspaceTextTrashV1` (see below). It never exposes an irreversible delete RPC.

### 2026-10-11 image preview

- PNG, JPEG, GIF and WebP workspace images can be previewed up to **4 MiB** using Flutter in-memory decoding with pan/zoom. SVG/HTML/PDF/other binary types remain unsupported, and no remote images or WebViews execute.
- Ranged image loader verifies every chunk's path, MIME type, total size, requested length and offsets, and validates minimal format signature before decoding. `flutter test --no-pub test/mobile_workspace_image_preview_test.dart`: **4 passed**. Full Flutter regression with the image loader tests: **460 passed, 4 skipped, 0 failed**. Targeted `dart analyze` on the image loader and viewer: **No issues found**.
- Physical-device image decoding, APK/iOS signing and real network latency verification are pending separate release checks.

### 2026-10-11 create-only mobile text files

- `cargo test --manifest-path rust/Cargo.toml -p alera-core --locked mobile_create_new_text_file_never_overwrites_or_leaves_workspace`: **1 passed**. Verifies no clobber, VCS metadata and traversal protection, existing-parent requirement, NUL/256 KiB limits and no-follow traversal on Unix.
- `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked mobile_codex_file_surface_tests` with isolated `CARGO_TARGET_DIR`: **3 passed**. Includes authenticated `mobile.workspaceFile.create` allowlist and capability advertisement. All mutating requests are marked as conflicting with Runtime state changes.
- `flutter test --no-pub` full mobile suite: **456 passed, 4 skipped, 0 failed**. Create RPC and existing-write focused tests: **2 passed**.
- `flutter analyze --no-pub`: **no issues**. Physical Android/iOS device and signed build remain separate release gates.

### 2026-10-11 historical Git comparisons

- Isolated `CARGO_TARGET_DIR=D:/alera-build/mobile-fork-native-cargo-isolated` resolves the previous shared Cargo artifact mismatch; `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked mobile_codex_file_surface_tests`: **3 passed**, including new comparison/patch allowlist and capability checks.
- `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked mobile_git_`: **3 passed** (history, single-file Stage/Unstage, and exact-commit file listing / historical patch, including root/reverse changes, traversal, protected metadata, wildcard paths and unreachable commit rejection).
- `flutter test --no-pub` full suite: **453 passed, 4 skipped, 0 failed**. New selectable Git patch view widget tests run separately: **2 passed**. Focused comparison/history/parser tests: **5 passed**.
- `flutter analyze --no-pub`: **no issues**; `rustfmt --check`, `dart format --output=none --set-exit-if-changed`, and `git diff --check`: **passed**.
- Signed APK, real Android/iOS and Firebase/APNs smoke testing remain unverified release gates.

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
  text, Git status and commit comparisons at most 200 changed paths, historical
  patches at most 512 KiB, and history at most 100 commits.
  Workspace text create/save require explicit mobile confirmation, guard the
  working directory against protected metadata and symlink traversal, and never
  overwrite an existing path on create. They do not execute arbitrary Git commands.
  Scoped stage/unstage affects only
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
   history beyond the latest 100 commits, and comparisons against unrelated
   branches or arbitrary refs not reachable from the selected worktree HEAD.
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
