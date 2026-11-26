# Fork Classic desktop updates

The `Fork Portable Release` workflow for `shelizi/alera` supports signed Classic desktop updates. The existing manual Windows x64 ZIP, macOS ARM64 tar.gz and Android APK remain available.

## Enable the release channel

Create one persistent Ed25519 key pair outside the repository. Keep the raw 32-byte seed private and back it up securely. Configure these repository settings:

- Actions variable `ALERA_FORK_UPDATE_PUBLIC_KEY`: base64-encoded raw 32-byte public key.
- Actions secret `ALERA_FORK_UPDATE_PRIVATE_KEY`: base64-encoded raw 32-byte private seed.

The built-in key ID is `shelizi-classic-v1`. Never put the seed in source code or a build argument. The workflow checks that the public key corresponds to the seed and verifies each signed schema-v3 descriptor against the actual ZIP bytes before publishing. If neither key is set, the workflow retains manual-only release behavior; a partial key configuration fails before publication.

Publish a new complete stable release using the existing workflow, with a new pubspec version and build number. Existing unsigned or privacy builds must be replaced manually once with the newly signed Classic package. Only the explicitly configured builds will check for updates. No updater secrets are needed at runtime.

## Compatibility

| Runtime | Artifact |
| --- | --- |
| Windows x64 Classic portable | `alera-X.Y.Z-windows-x64.zip` |
| macOS ARM64 Classic | `alera-X.Y.Z-macos-arm64.zip` |
| macOS Intel, Windows ARM64, Linux, GPUI, Android and half-builds | No Classic self-update |

macOS also retains its manual tar.gz artifact. The ZIP is created with `ditto` to preserve the app bundle. Each target has a dedicated index asset `alera-classic-<target>-update-index.json` and signed descriptor `alera-classic-<target>-release.json`. The index URL uses GitHub's latest stable release; if it does not contain the matching Classic index, the update check returns no published index, without using the upstream server or another platform's package. The descriptor pins the versioned GitHub asset URL, byte length, SHA-256, platform and install strategy.

Set `ALERA_DISABLE_UPDATE_CHECKS=1` in the app's launch environment to disable online discovery, including scheduled checks. Development and ordinary privacy portable builds retain offline discovery by default. In supported signed builds, Check for Updates is a manual network action; the Settings switch Automatically check for updates is off by default and its value is persisted in the application support directory. Enabling it checks when the app becomes visible and every 15 minutes while visible. Checks do not install automatically: the user chooses Install Update. Stable version numbers are strictly increasing; a higher build number alone never permits an update. The existing updater stages and verifies the archive and hands off to the native updater, and package-managed installations remain excluded from in-place replacement. Operating-system code signing/notarization and native replacement still need platform validation before rolling out an update to real installations.
