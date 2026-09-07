# Windows Final-Link Kit

This workflow splits the Alera Windows release into two stages.

## 1. Build/export machine

The build machine has the normal Alera toolchain (Flutter, Rust, Zig, native compiler toolchain). Run:

```powershell
powershell -ExecutionPolicy Bypass -File tool/windows-finalizer/export_final_link_kit.ps1 -Force
```

The exporter performs a normal Windows Release build unless `-SkipBuild` is supplied. It then creates `build/final-link-kit/Alera-Final-Link-Kit.zip` containing:

- Flutter AOT/runtime payload and `flutter_assets`
- Rust `alera_native` and runtime-host sidecar
- Ghostty/native assets and plugin DLLs
- Flutter engine/wrapper headers
- prebuilt plugin import/static libraries
- the small Windows runner C++ sources/resources
- a standalone MSBuild project
- `finalize_windows.ps1`
- SHA-256 hashes for transfer-integrity verification

The original build-machine `Alera.exe` is deliberately **not** included in the payload.

If the repository lives below a path containing non-ASCII characters, the exporter temporarily uses a `SUBST` drive for the Flutter build and removes it afterwards.

Useful options:

```powershell
# Package an already-successful Release build without rebuilding it.
.\tool\windows-finalizer\export_final_link_kit.ps1 -SkipBuild -Force

# Explicitly point at pinned tools when they are not on PATH.
.\tool\windows-finalizer\export_final_link_kit.ps1 `
  -FlutterPath .\.tools\flutter\bin\flutter.bat `
  -ZigPath C:\tools\zig-0.16.0\zig.exe `
  -Force
```

## 2. Restricted/target Windows machine

Extract the kit locally. The target machine only needs:

- Visual Studio/Build Tools with **Desktop development with C++**
- a Windows 10 or Windows 11 SDK
- Windows PowerShell

It does **not** need Flutter, Dart, Rust/Cargo, rustup, Zig, LLVM, Vulkan SDK, Git, CMake, Ninja, NSIS, 7-Zip, or a signing certificate.

First verify the transferred kit and linker environment:

```powershell
powershell -ExecutionPolicy Bypass -File .\finalize_windows.ps1 -VerifyOnly
```

Then create the local executable:

```powershell
powershell -ExecutionPolicy Bypass -File .\finalize_windows.ps1
```

The result is `out\Alera\Alera.exe` plus the already-prebuilt runtime files beside it. Alera is a Flutter desktop application, so the support DLLs, `data`, and `resources` directories must remain next to the EXE.

### MSVC compatibility

The kit records the platform toolset used to build the precompiled libraries and uses that toolset by default for the final runner link. A different toolset can be requested with `-PlatformToolset`, but mixing an older MSVC toolset with libraries produced by a newer one is not guaranteed to be ABI/runtime compatible. For a controlled deployment, export the kit using the same MSVC toolset that is available on the target machine.

No signing step is performed by either script.
