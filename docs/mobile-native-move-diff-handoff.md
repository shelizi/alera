# Alera mobile native parity: safe file move and Git diff continuation

Date: 2026-10-11. Workspace: `.worktrees/mobile-fork-native`. Branch: `feat/mobile-fork-native`. This handoff is confined to the isolated feature branch; do not overwrite the main worktree's uncommitted changes.

## Commits and feature scope

1. `a8c2a953` `feat(mobile): move small text files across workspace folders safely`.
   - Capability `mobileWorkspaceTextMoveV1`; authenticated, conflicting-mutation RPC `mobile.workspaceFile.move` with `{workspaceId, relativePath, destinationDirectory, expectedSha256}`.
   - Flutter file viewer offers **Move to folder** only for a complete, strict UTF-8 SHA-256 baseline. Picker pages authenticated workspace directories, lists folders only, supports root destination (`""`), disables the current source folder, and confirms both paths before writing.
   - Rust core `workspace_files::move_file::move_workspace_text_from_root`: existing paths in the same workspace and filesystem only, strict UTF-8 <=256 KiB, no-follow `cap_std` traversal, `.git`/`.hg`/`.svn` protected, source SHA-256 and identity recheck. Destination reserved with a no-overwrite hard link; source unlinked only after revalidation. Metadata and OS file identity are preserved; there is **no** cross-volume copy/delete fallback.
   - A noncooperating process could still replace a path between validation and final unlink: this is not an OS-level atomic unlink-by-file-ID primitive. Do not remove guards or extend to arbitrary binary/directory files without a separate design review.

2. `daa5daa5` `feat(mobile): highlight bounded intraline Git diff changes`.
   - The same read-only `MobileGitPatchView` now adds finer-grained spans inside equal-sized contiguous removed/added groups, up to eight lines and 4096 UTF-16 code units per line.
   - Character boundaries use Unicode runes and preserve exact source text, line markers and selectable copy. Unequal/ambiguous groups, long lines, unchanged pairs and file headers keep normal whole-line highlighting. No new Runtime permissions/RPCs, patch size remains capped at 512 KiB.

## Test evidence

- Rust core: `cargo test --manifest-path rust/Cargo.toml -p alera-core --locked workspace_files::move_file::tests` **2 passed**, including no overwrite, stale digest, missing/protected folders, same-folder rejection, source round trip and preserved real filesystem identity.
- Rust CLI Gateway: `cargo test --manifest-path rust/Cargo.toml -p alera-cli --locked mobile_codex_file_surface_tests` **3 passed**. Used isolated `CARGO_TARGET_DIR=D:/alera-build/mobile-fork-native-cargo-isolated`.
- Mobile RPCs: move + rename + trash WebSocket contract **3 passed**.
- Mobile destination picker widget: **1 passed**, including navigation, excluding regular files, selecting root and disabling the source folder.
- Git patch parser/widget: **8 passed** (Unicode, ambiguous runs, length bounds, exact selectable text).
- Full mobile Flutter test suite after intraline feature: **471 passed, 4 skipped, 0 failed**. `flutter analyze --no-pub` **no issues**. Rustfmt, Dart format and Git diff checks passed in each code slice.

## Android and iOS release prerequisites

- `mobile/android/app/build.gradle.kts` sets **compileSdk 37** (for the secure storage plugin); the Windows host currently has Android SDK platforms **33, 34, 35**, and Build Tools **30.0.3, 34.0.0, 35.0.0**. `flutter doctor -v` also requests SDK 36, Build Tools 28.0.3 and some unaccepted Android licenses.
- The operator must install SDK 37 and required Build Tools/NDK, and personally accept any outstanding SDK licenses. This assistant does not silently accept third-party license agreements.
- A Debug APK build attempt was made in the isolated mobile directory: `flutter build apk --debug --no-pub` **timed out after 311 seconds** while reporting `Running Gradle task 'assembleDebug'...`; Gradle emitted no actionable error before the command timeout. Afterward `mobile/build/app/outputs/flutter-apk/app-debug.apk` **did not exist** and the Android SDK still contained only platforms 33/34/35 and Build Tools 30.0.3/34.0.0/35.0.0. This is a blocked smoke test, not a verified SDK-specific Gradle exception or a successful APK build.
- Following successful Android tooling setup, run debug APK/build and emulator smoke tests, then physical Android pairing and network-loss recovery tests. iOS signing and APNs require an appropriate macOS host.

## Next slices

- Mobile code syntax-highlighting viewer and optional read-only side-by-side Git diff, preserving bounded patch contents and copyable exact lines.
- Broader host-side file management needs separate safety reviews for large/binary files, cross-volume moves and directories. Never add unrestricted filesystem or arbitrary Git commands to paired-phone RPCs.
- Non-terminal desktop tab content, broader Git history/UI parity and real device/relay integration tests remain outstanding.
