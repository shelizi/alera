@echo off
setlocal
set "ALERA_E2E_DESKTOP=1"
set "ALERA_E2E_REUSE_CLI=1"
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %ERRORLEVEL%
ninja.exe -C "C:\a_mv_20261022" -f build-Debug.ninja -j 4 runner/Debug/alera-dev.exe
if errorlevel 1 exit /b %ERRORLEVEL%
cmake --install "C:\a_mv_20261022" --config Debug
exit /b %ERRORLEVEL%
