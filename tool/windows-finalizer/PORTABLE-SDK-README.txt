Alera Final-Link Kit - Portable Windows SDK mode
================================================

Target requirement
------------------
- Visual Studio 2022 / Build Tools 2022 with Desktop development with C++ / v143 installed.
- A portable Windows SDK directory. The SDK itself does NOT need to be installed.

Default target layout
---------------------
The launcher expects:
  D:\Windows Kits\10

and Windows SDK version:
  10.0.26100.0

Required representative files include:
  D:\Windows Kits\10\Include\10.0.26100.0\um\Windows.h
  D:\Windows Kits\10\Include\10.0.26100.0\ucrt\stdio.h
  D:\Windows Kits\10\Lib\10.0.26100.0\um\x64\kernel32.lib
  D:\Windows Kits\10\Lib\10.0.26100.0\ucrt\x64\ucrt.lib
  D:\Windows Kits\10\bin\10.0.26100.0\x64\rc.exe
  D:\Windows Kits\10\bin\10.0.26100.0\x64\mt.exe

Usage
-----
Verify first:
  finalize_with_portable_sdk.cmd verify

Build:
  finalize_with_portable_sdk.cmd

Output:
  out\Alera\Alera.exe

Keep the whole out\Alera directory together; Alera.exe alone is insufficient.

Custom SDK root/version
-----------------------
Set environment variables before running the launcher:
  set ALERA_WINDOWS_SDK_ROOT=E:\Windows Kits\10
  set ALERA_WINDOWS_SDK_VERSION=10.0.26100.0
  finalize_with_portable_sdk.cmd verify
  finalize_with_portable_sdk.cmd

Or call PowerShell directly:
  powershell -ExecutionPolicy Bypass -File .\finalize_windows.ps1 -WindowsSdkRoot "D:\Windows Kits\10" -WindowsSdkVersion "10.0.26100.0"

The finalizer validates the SDK headers, x64 libraries, rc.exe and mt.exe before invoking MSBuild. It injects the portable SDK root into MSBuild, so Windows SDK registry installation is not required.
The Visual Studio installation and v143 toolset are still discovered normally with vswhere/MSBuild; only the Windows SDK is portable.

This kit defaults to Windows SDK 10.0.26100.0 because it matches the Alera source build. A second SDK such as 10.0.22621.0 can remain beside it; the launcher will explicitly select 10.0.26100.0.

The launcher defaults to VS2022 platform toolset v143. ALERA_PLATFORM_TOOLSET is only for controlled validation on another build machine; normal target usage should leave it unset.
