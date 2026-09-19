# Windows MSBuild / FileTracker recovery

Use this only after the normal Final-Link exporter fails and the failure matches the FileTracker signature described in `SKILL.md`.

## 1. Confirm the failure is not a missing compiler

Evidence that the compiler/toolchain exists:

- `cl.exe` is located and invoked in the failing log.
- CMake or MSBuild prints the MSVC version.
- A compiler test source is compiled before MSBuild reports `MSB4018`.

The characteristic host-runtime failure is:

```text
MSB4018
Microsoft.Build.Utilities.FileTracker
InitializeCommonApplicationDataPaths
System.ArgumentException / illegal path format
```

When this appears, stop treating the problem as a compiler-installation failure.

## 2. Test the same .NET Framework runtime as MSBuild

Use Windows PowerShell 5.1 explicitly. Do not rely on PowerShell 7 for this diagnosis.

```powershell
C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -NonInteractive -Command "[Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)"
```

Also verify:

```powershell
Test-Path C:\ProgramData
(Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders').'Common AppData'
(Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders').'Common AppData'
```

If `C:\ProgramData` exists and both registry values are correct but Windows PowerShell 5.1 returns an empty path or throws, classify the machine as having a broken .NET Framework special-folder resolution path. Do not keep retrying MSBuild settings.

## 3. Avoid already-proven dead ends

- Setting `ALLUSERSPROFILE`, `CommonProgramFiles`, `CommonProgramFiles(x86)`, `PUBLIC`, or similar process variables does not repair a broken `.NET Framework Environment.GetFolderPath(CommonApplicationData)` result.
- `TrackFileAccess=false` and `MinimalRebuildFromTracking=false` are insufficient for this failure. `FileTracker` can still be initialized by `CL` post-processing or `GetOutOfDateItems`.
- A normal drive-letter cwd instead of `\\?\...` does not fix the failure when the .NET Framework folder probe itself is broken.
- Switching from one Visual Studio MSBuild generator to another does not fix the runtime failure; both VS2022 and newer VS generators can hit the same `FileTracker` exception.

## 4. Locate a secondary-drive VS toolchain efficiently

Search targeted roots first, for example:

```text
F:\Program Files\Microsoft Visual Studio\2022\Community
F:\Program Files (x86)\Microsoft Visual Studio
F:\Program Files (x86)\Windows Kits
F:\ProgramData\Microsoft\VisualStudio\Packages\_Instances
```

Useful files:

```text
VC\Auxiliary\Build\vcvars64.bat
VC\Tools\MSVC\<version>\bin\Hostx64\x64\cl.exe
Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe
Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe
```

When CMake cannot discover a secondary-drive VS installation from the registry, obtain its `installationVersion` from Visual Studio installer metadata and use:

```text
-G "Visual Studio 17 2022"
-A x64
-DCMAKE_GENERATOR_INSTANCE=F:\Program Files\Microsoft Visual Studio\2022\Community,version=<installationVersion>
```

Do not recursively scan the whole drive unless targeted roots fail.

## 5. FileTracker-safe fallback structure

Use an isolated clean worktree pinned to the intended local `main` commit. Never run the fallback from a dirty tracked tree.

1. Generate **production Release Visual Studio metadata** in an isolated build directory without executing MSBuild compiler probes.
   - Set `ALERA_FLAVOR=release` before CMake configure.
   - Preseed the known compiler path, compiler ID (`MSVC`), compiler version, `*_COMPILER_ID_RUN=1`, and compile-feature metadata.
   - Use forward-slash compiler paths such as `F:/Program Files/.../cl.exe`; raw Windows backslashes can be written into `CMakeCXXCompiler.cmake` and fail as invalid escapes such as `\P`.
   - Confirm the result contains `runner/Alera.vcxproj` and `flutter/flutter_wrapper_app.vcxproj` with `TargetName=Alera`, `NDEBUG`, and production `ALERA_APP_*` definitions.

2. Build the actual Release payload with **Ninja + the same MSVC toolchain** after running `vcvars64.bat`.
   - Configure with `-G Ninja -DCMAKE_BUILD_TYPE=Release` and `ALERA_FLAVOR=release`.
   - This bypasses MSBuild/FileTracker while still compiling with MSVC.

3. If Ninja reports that `windows/flutter/ephemeral/flutter_windows.dll.lib` is missing, do not copy an arbitrary library.
   - Locate the library in the project-local Flutter SDK/cache or another verified build from the same Flutter SDK.
   - Compare SHA-256 first.
   - Copy it into the pinned worktree only when the hashes match exactly, then continue the existing Ninja build without unnecessary reconfiguration.

4. Treat the fallback as incomplete until the build exits 0 and the normal Final-Link package checks pass.
   - Do not publish directly from a Ninja output directory.
   - Keep the tracked Final-Link exporter/format as the publishing contract. If using `-SkipBuild`, ensure the exporter sees the verified Release payload plus generated production VS metadata in the paths it expects.
   - Re-run all manifest, ZIP-content, sidecar, `app.so`, SHA-256, and remote atomic-upload checks from `SKILL.md` and `verification-commands.md`.

## 6. Decision rule

Use this order on future failures:

1. Run the normal exporter once.
2. If the compiler is genuinely missing, repair the toolchain.
3. If the FileTracker signature is present, immediately run the Windows PowerShell 5.1 folder probe.
4. If the probe confirms broken .NET Framework special-folder resolution, stop retrying MSBuild workarounds and switch to the FileTracker-safe fallback.
5. Publish only after the normal Release identity and package verification contract passes.
