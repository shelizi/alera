@echo off
setlocal
set "ALERA_E2E_DESKTOP=1"
set "ALERA_E2E_REUSE_CLI=1"
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %ERRORLEVEL%
for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
ninja.exe -C "%ROOT%\build\windows\shared_gui_ninja" -f build-Debug.ninja -j 4 runner/Debug/alera-dev.exe
if errorlevel 1 exit /b %ERRORLEVEL%
cmake --install "%ROOT%\build\windows\shared_gui_ninja" --config Debug
if errorlevel 1 exit /b %ERRORLEVEL%
rem The isolated MCP desktop acceptance requires a sidecar built from this
rem feature worktree. A stale reused CLI may silently lack the mcp verb.
"%ROOT%\build\windows\shared_gui_ninja\runner\Debug\resources\alera\alera.exe" mcp --help >nul 2>nul
if errorlevel 1 (
  echo ERROR: E2E Rust CLI lacks MCP. Rebuild with ALERA_E2E_REUSE_CLI=0.
  exit /b 14
)
exit /b 0
