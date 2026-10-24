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
exit /b %ERRORLEVEL%
