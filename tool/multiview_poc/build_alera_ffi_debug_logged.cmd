@echo off
setlocal
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %ERRORLEVEL%
set "ALERA_E2E_DESKTOP=1"
set "ALERA_E2E_REUSE_CLI=1"
rem Isolated Ninja runs do not inherit FLUTTER_ROOT from flutter_tools.
rem Cargokit uses it to find the actual Dart executable.
if not defined FLUTTER_ROOT for /f "delims=" %%F in ('where flutter.bat 2^>nul') do if not defined FLUTTER_ROOT for %%D in ("%%~dpF..") do set "FLUTTER_ROOT=%%~fD"
if not exist "%FLUTTER_ROOT%\bin\cache\dart-sdk\bin\dart.exe" (
    echo ERROR: FLUTTER_ROOT is not a valid Flutter SDK path
    exit /b 1
)
echo FLUTTER_ROOT=%FLUTTER_ROOT%
ninja.exe -C "C:\a_mv_20261022" -f build-Debug.ninja -j 2 plugins/alera_native/Debug/alera_native.dll plugins/code_forge/Debug/code_forge.dll >"C:\a_mv_20261022\ffi_build.log" 2>&1
set "CODE=%ERRORLEVEL%"
if "%CODE%"=="0" if not exist "C:\a_mv_20261022\plugins\alera_native\Debug\alera_native.dll" set "CODE=2"
if "%CODE%"=="0" if not exist "C:\a_mv_20261022\plugins\code_forge\Debug\code_forge.dll" set "CODE=2"
echo FFI_BUILD_EXIT=%CODE%
powershell.exe -NoProfile -Command "$r=Get-Content 'C:/a_mv_20261022/ffi_build.log'; $r | Select-String -Pattern 'FAILED:|error C[0-9]+|error LNK[0-9]+|fatal error|ninja: error|CMake Error|\berror:|No rule' | Select-Object -Last 22 | ForEach-Object { $_.Line }; Write-Output '---TAIL---'; $r | Select-Object -Last 12"
exit /b %CODE%
