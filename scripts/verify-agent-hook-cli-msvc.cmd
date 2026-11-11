@echo off
setlocal EnableExtensions DisableDelayedExpansion
rem Run H6 CLI verification in an isolated Visual Studio C++ build environment.
rem Do not change global PATH, install tools, launch or write to Runtime Host.
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VCVARS="
if exist "%VSWHERE%" (
  for /f "usebackq delims=" %%I in (`"%VSWHERE%" -latest -products * -property installationPath`) do (
    if exist "%%I\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%%I\VC\Auxiliary\Build\vcvars64.bat"
  )
)
if not defined VCVARS (
  echo Visual Studio x64 C++ toolchain is not installed or cannot be found. 1>&2
  exit /b 1
)
call "%VCVARS%" >nul
if errorlevel 1 exit /b 1
where cl.exe >nul 2>&1
if errorlevel 1 (echo MSVC cl.exe not found after vcvars64. 1>&2 & exit /b 1)
where link.exe >nul 2>&1
if errorlevel 1 (echo MSVC link.exe not found after vcvars64. 1>&2 & exit /b 1)
if not defined CARGO_TARGET_DIR (
  if exist "D:\c\alera-cli" (
    rem Reuse the existing Windows CLI compilation cache on this machine.
    set "CARGO_TARGET_DIR=D:\c\alera-cli"
  ) else (
    rem Other developer machines need no special drive or global settings.
    set "CARGO_TARGET_DIR=%TEMP%\alera-cli-agent-hook"
  )
)
cd /d "%~dp0..\rust"
if errorlevel 1 exit /b 1
if /i "%~1"=="check" (
  cargo check --offline --locked -p alera-cli --bin alera -j 2
) else if /i "%~1"=="check-tests" (
  rem Validate H6 unit-test code without linking the full Windows test EXE.
  cargo check --offline --locked -p alera-cli --bin alera --tests -j 2
) else if /i "%~1"=="test" (
  cargo test --offline --locked -p alera-cli --bin alera agent_hook_readback -j 2
) else (
  echo Usage: verify-agent-hook-cli-msvc.cmd check^|check-tests^|test 1>&2
  exit /b 64
)
exit /b %errorlevel%
