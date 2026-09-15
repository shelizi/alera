---
name: alera-remote-release
description: Build and publish the Alera Windows Release as the semi-compiled Final-Link Kit to the internal file server. Use when asked to compile/package Alera main for Windows Release, create or refresh the Final-Link/half-compiled package, upload or update the remote Alera package, or verify that the remote package is a real Release build. Use home-node for the Alera workspace, protect uncommitted work, verify Release flavor and kit metadata, compare local/remote SHA-256 and ZIP integrity, and never git-push unless the user explicitly asks.
---

# Alera Remote Release

Use the project's tracked Windows Final-Link workflow to publish the reusable semi-compiled Release package. Treat successful upload as incomplete until Release identity, package contents, hash, and remote ZIP integrity are verified.

## Workflow

1. Connect to the Alera workspace with `home-node`.
   - Follow the connector's bootstrap requirement before project operations.
   - Resolve the current Alera workspace/repository instead of hardcoding a workspace ID.
   - The repository may be nested below the selected workspace; confirm with Git before running commands.

2. Inspect Git state before building.
   - Confirm the local branch and local `main` HEAD.
   - Run `git status --short --branch` and record ahead/behind state relative to `origin/main` when available.
   - Interpret an unqualified request for `main` as the current **local `main`**, not `origin/main`.
   - Do not pull, fetch/rebase, merge, or push unless the user explicitly requests it.
   - Do not include uncommitted tracked changes in a `main` release. If tracked changes are present, build from an isolated clean worktree at local `main` HEAD instead of stashing or overwriting the user's work.
   - Known harmless local history may remain under `docs/history-session/`.
   - `tool/release/build_windows_release.ps1` is a local incremental Release-build helper, not the publishing authority for this workflow. Do not substitute it for the tracked Final-Link exporter below.

3. Build/export with the tracked Final-Link exporter.

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tool/windows-finalizer/export_final_link_kit.ps1 -Force
```

   - This is the authoritative workflow. It performs the Windows Release build and creates `build/final-link-kit/Alera-Final-Link-Kit.zip`.
   - Do not replace it with a hand-written ZIP or with `tool/release/build_windows_release.ps1`.
   - A Flutter progress line mentioning `alera-dev.exe` is **not evidence by itself** that the package is a dev build. The project defines a dev default name before `ALERA_FLAVOR=release` selects the production identity. Determine flavor only from the generated Release project, manifest, and AOT payload checks below.
   - Classify command output by terminal result, not by stream name or wording alone:
     - **Fatal failure**: non-zero process exit code, explicit exporter exception/termination, missing required artifact, hash mismatch, invalid ZIP, or failed Release identity check. Stop and do not publish.
     - **Non-fatal diagnostic**: text written to stderr while the parent command still exits 0 and every required artifact/Release check passes. Report it only when materially useful; do not call the build failed.
     - **Warning**: lines explicitly marked `warning` (including Rust/MSBuild warnings). Do not label these as errors unless they cause a non-zero exit or a required verification failure.
   - Do not decide success while the build is still running. Wait for the terminal exit code, then validate the newly generated artifacts.
   - If the build command has a fatal failure, stop. Never upload a previous/stale package after a failed build.

4. Verify the package is genuinely Release before upload.
   - Confirm `build/windows/x64/runner/Alera.vcxproj` exists.
   - In its `Release|x64` configuration confirm:
     - `TargetName` is `Alera`.
     - compiler definitions contain `NDEBUG`.
     - `ALERA_APP_NAME=L"Alera"`.
     - `ALERA_APP_ID=L"dev.leynier.alera"`.
   - Confirm the newly generated `build/windows/x64/runner/Release/data/app.so` belongs to this build.
   - Read `build/final-link-kit/Alera-Final-Link-Kit/kit-manifest.json` and confirm:
     - `product` is `Alera`.
     - `sourceCommit` equals the intended local `main` HEAD.
     - `outputExecutable` is `Alera.exe`.
     - Release compiler definitions include `NDEBUG`, `ALERA_APP_NAME=L"Alera"`, and `ALERA_APP_ID=L"dev.leynier.alera"`.
   - Inspect the ZIP and confirm:
     - exactly one `payload/data/app.so` exists;
     - the runtime sidecar `payload/resources/alera/alera.exe` exists;
     - the build-machine final runner `Alera.exe` is not included in the transferable payload.
   - Compute the local SHA-256 and record it.
   - See `references/verification-commands.md` for command templates.

5. Upload to the internal file server only after all local checks pass.
   - Use the configured SSH alias `neo-ai`; never embed an IP address, password, or private-key path in the Skill or report.
   - Stable destination:
     `/opt/fileServer-direct/uploads/alera/Alera-Final-Link-Kit.zip`
   - Prefer an atomic replacement:
     1. upload to `/opt/fileServer-direct/uploads/alera/.Alera-Final-Link-Kit.uploading.zip`;
     2. compute its remote SHA-256 and run `unzip -t`;
     3. only when both pass and the hash equals the local hash, rename it to the stable destination with `mv -f`;
     4. verify the stable destination again.
   - Do not update the legacy `Alera-Final-Link-Kit-EWDK-Aware.zip` unless the user explicitly asks. The current standard kit already contains the portable-SDK finalizer workflow.

6. Verify remote state.
   - Treat SSH/SCP diagnostics with the same classification rule: stderr text alone is not failure. Use the command exit code plus hash/ZIP assertions.
   - Remote SHA-256 must exactly equal the local SHA-256.
   - Remote `unzip -t` must succeed and print/return an explicit success marker such as `ZIP_OK`.
   - Use direct remote paths in SSH verification commands. Avoid `$f`-style remote shell variables from a Windows caller because quoting/interpolation mistakes can make verification target the wrong path.
   - If upload or verification fails, do not claim completion. Preserve/report the failure and do not replace the stable file unless the staged artifact has passed verification.

7. Confirm the working tree after publishing.
   - Run `git status --short --branch` again.
   - Confirm the build/upload process did not add tracked modifications.
   - Leave pre-existing untracked files untouched.

## Reporting

Keep the completion report concise and include:

- local `main` commit used;
- confirmation that Release identity checks passed;
- fatal failures separately from non-fatal diagnostics/warnings; never summarize successful warning-only output as a script error;
- package filename and stable remote destination;
- local and remote SHA-256 match;
- remote ZIP integrity result;
- any pre-existing uncommitted files that were excluded;
- explicit note that no Git push occurred unless the user requested one.

Do not expose internal server IP addresses, credentials, or key paths.
