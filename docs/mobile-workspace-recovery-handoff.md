# Mobile workspace text recovery handoff (2026-10-11)

## Scope and commit

- Isolated worktree: `.worktrees/mobile-fork-native`, branch `feat/mobile-fork-native`.
- Implementation commit: `176c3ef5` (`feat(mobile): add recoverable workspace text trash and restore`).
- Do not modify the main worktree, merge or push without an explicit release/integration decision.

## Behavior and protocol

- Capability: `mobileWorkspaceTextTrashV1`. Old runtimes do not show the new phone actions.
- Authenticated and workspace-scoped mobile requests:
  - `mobile.workspaceFile.trash`: `{ workspaceId, relativePath, expectedSha256 }`; a mutation, requiring a strict UTF-8 file at most 256 KiB, unchanged SHA-256 and a registered workspace.
  - `mobile.workspaceTrash.list`: `{ workspaceId }`; returns paths, backup IDs, checksums, sizes and timestamps, **not file contents**.
  - `mobile.workspaceTrash.restore`: `{ workspaceId, trashId }`; a mutation, restoring at the original path only with atomic create-new.
- Phone: file preview → confirmed **Move to recovery**; directory browser → **Recoverable files** → confirmed **Restore**. Both operations refresh the folder view. Neither screen exposes permanent deletion.

## Data integrity and permissions

- Backup storage is `<runtime-dir>/mobile-workspace-trash/<sha256(workspace-id)>/<uuid>.json`, explicitly rejected if the Runtime directory resolves inside the selected workspace.
- Uses `cap_std` directory handles with no-follow traversal, strictly validated file names and UUIDs, a SHA-256 content precondition, backup file sync and (on Unix) parent directory sync before removing the source path.
- Recoverable records are persisted across Runtime restarts and verified for checksum, path, workspace scope and version before listing/restore. On conflicts or corrupted records, existing workspace files are never overwritten.
- No automatic purge/expiry. Capacity is **64 records / 16 MiB per workspace**; when full, the source is retained. Recovered records are removed only after successful no-clobber restore.
- Other noncooperating programs can still race changes to filesystem names between last check and unlink; robust cross-process atomic unlink-by-file-ID is not guaranteed by this portable implementation. Keep deletion limited to SHA-guarded small text files and do not expose arbitrary permanent deletion.

## Verified tests

- `cargo test --manifest-path rust/Cargo.toml -p alera-core --locked workspace_files::trash::tests`: **4 passed**. Includes restore and restart persistence, workspace isolation, invalid/protected paths, capacity, corrupted backup and conflict refusal.
- `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked mobile_codex_file_surface_tests`: **3 passed**; capability advertisement and mobile request allowlist.
- `flutter test --no-pub test/mobile_workspace_trash_client_test.dart test/mobile_workspace_rename_client_test.dart test/mobile_workspace_create_client_test.dart`: **3 passed**, including exact RPC payloads and conflict decoding.
- Entire Flutter test suite: **464 passed, 4 skipped, 0 failed**. `flutter analyze --no-pub`: **No issues**. Rustfmt, Dart format and `git diff --check` pass.
- Rust Cargo build target used for these tests: `D:/alera-build/mobile-fork-native-cargo-isolated`. Do not delete other workers' Cargo caches.

## Release blockers and next batches

- `flutter doctor -v` on Windows identifies an Android 14 emulator but reports Android SDK 36, Build Tools 28.0.3, and some Android SDK licenses missing. Restore Android prerequisites and validate a debug APK/device pairing; accepting license agreements requires the operator's action. iOS signing and APNs require a macOS host.
- Verify Tailscale/NetBird and optional relay RPC operation against physical paired devices before release, including recovery after connection loss.
- Remaining desktop parity: guarded cross-directory move/rename, binary/large-file management, syntax-highlighted and intraline/side-by-side diff, non-terminal tab content, broader Git history and signed mobile builds.
- Prefer isolated tested slices and separate commits. Do not enable unbounded host filesystem operations, arbitrary Git commands, or auto-purge of backups as a shortcut.
