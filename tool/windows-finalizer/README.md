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
- Flutter engine headers plus the small Flutter C++ client-wrapper sources
- prebuilt plugin **DLL import libraries** (not plugin C++ object code)
- the small Windows runner C++ sources/resources
- a standalone MSBuild project
- `finalize_windows.ps1`
- `finalize_with_portable_sdk.cmd` and `PORTABLE-SDK-README.txt`
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

- **Visual Studio 2022 / Build Tools 2022** with **Desktop development with C++** (`v143`)
- either an installed Windows SDK, or a complete portable `Windows Kits\10` directory
- Windows PowerShell

It does **not** need Flutter, Dart, Rust/Cargo, rustup, Zig, LLVM, Vulkan SDK, Git, CMake, Ninja, NSIS, 7-Zip, or a signing certificate.

The target does compile a small amount of ordinary C++ locally: Flutter's four client-wrapper `.cc` files plus Alera's Windows runner. Flutter AOT, Rust, Ghostty, plugin DLLs, native assets, and application data remain prebuilt.

First verify the transferred kit and linker environment:

```powershell
powershell -ExecutionPolicy Bypass -File .\finalize_windows.ps1 -VerifyOnly
```

Then create the local executable:

```powershell
powershell -ExecutionPolicy Bypass -File .\finalize_windows.ps1
```

### Portable Windows SDK (no SDK installation required)

If VS2022/v143 is installed but the Windows SDK is only copied to disk, use the included launcher. It defaults to `D:\Windows Kits\10`, SDK `10.0.26100.0`, and toolset `v143`:

```cmd
finalize_with_portable_sdk.cmd verify
finalize_with_portable_sdk.cmd
```

The portable SDK must contain matching `Include`, `Lib`, and `bin` trees. The finalizer validates representative headers, x64 libraries, `rc.exe`, and `mt.exe`, then injects the SDK root into MSBuild so Windows SDK registry installation is unnecessary.

The result is `out\Alera\Alera.exe` plus the already-prebuilt runtime files beside it. Alera is a Flutter desktop application, so the support DLLs, `data`, and `resources` directories must remain next to the EXE.

### VS2022 / MSVC compatibility

The exporter targets `v143` by default, even if the build/export machine itself uses a newer Visual Studio. The important distinction is that `flutter_wrapper_app.lib` is **not transferred**: it contains real MSVC/STL object code and is rebuilt from source on the target together with the runner. The plugin `.lib` files carried by the kit are DLL import libraries, while the actual plugin code stays in the prebuilt DLLs.

This allows a VS2022/v143 target to perform the final local compile/link without mixing v145 C++ object code into its executable. To build a kit for another controlled target toolset, pass `-TargetPlatformToolset <toolset>` to the exporter. `finalize_windows.ps1 -PlatformToolset <toolset>` is also available as an explicit override for validation or alternate deployments.

No signing step is performed by either script.
