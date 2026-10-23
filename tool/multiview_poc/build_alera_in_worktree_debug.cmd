@echo off
setlocal
set "ALERA_E2E_DESKTOP=1"
set "ALERA_E2E_REUSE_CLI=1"
set "CommonProgramFiles=C:\Program Files\Common Files"
set "CommonProgramW6432=C:\Program Files\Common Files"
set "CommonProgramFiles(x86)=C:\Program Files (x86)\Common Files"
set "PROGRAMDATA=C:\ProgramData"
set "ALLUSERSPROFILE=C:\ProgramData"
if not defined FLUTTER_ROOT for /f "delims=" %%F in ('where flutter.bat 2^>nul') do if not defined FLUTTER_ROOT for %%D in ("%%~dpF..") do set "FLUTTER_ROOT=%%~fD"
if not exist "%FLUTTER_ROOT%\bin\cache\dart-sdk\bin\dart.exe" exit /b 2
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %ERRORLEVEL%
for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
set "EXPECTED_E2E_SUPPORT=%ROOT:\=/%/build/windows/shared_gui_ninja/e2e_support"
rem The native E2E App ID alone does not isolate Drift and the app logger.
rem Fail closed unless Flutter compiled the designated test support directory.
set "GENERATED_CONFIG=%ROOT%\windows\flutter\ephemeral\generated_config.cmake"
if not exist "%GENERATED_CONFIG%" (
    echo ERROR: missing Flutter generated_config.cmake. Generate using --dart-define=ALERA_E2E_SUPPORT_DIR=%EXPECTED_E2E_SUPPORT%
    exit /b 7
)
powershell.exe -NoProfile -Command "$token = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('ALERA_E2E_SUPPORT_DIR=' + $env:EXPECTED_E2E_SUPPORT)); if (-not [System.IO.File]::ReadAllText($env:GENERATED_CONFIG).Contains($token)) { exit 8 }"
if errorlevel 1 (
    echo ERROR: refusing to build potentially unsafe GUI; ALERA_E2E_SUPPORT_DIR was not compiled for isolated test data.
    exit /b 8
)
set "BUILD=%ROOT%\build\windows\shared_gui_ninja"
if not exist "%BUILD%" mkdir "%BUILD%"
cmake -S "%ROOT%\windows" -B "%BUILD%" -G "Ninja Multi-Config" -DFLUTTER_TARGET_PLATFORM=windows-x64
if errorlevel 1 exit /b %ERRORLEVEL%
ninja.exe -C "%BUILD%" -f build-Debug.ninja -j 4 runner/Debug/alera-dev.exe
if errorlevel 1 exit /b %ERRORLEVEL%
ninja.exe -C "%BUILD%" -f build-Debug.ninja -j 2 plugins/alera_native/Debug/alera_native.dll plugins/code_forge/Debug/code_forge.dll plugins/flutter_local_notifications_windows/shared/Debug/flutter_local_notifications_windows.dll plugins/jni/shared/Debug/dartjni.dll
if errorlevel 1 exit /b %ERRORLEVEL%
cmake --install "%BUILD%" --config Debug
if errorlevel 1 exit /b %ERRORLEVEL%
if not exist "%BUILD%\runner\Debug\alera-dev.exe" exit /b 3
if not exist "%BUILD%\runner\Debug\data\flutter_assets\kernel_blob.bin" exit /b 4
if not exist "%BUILD%\runner\Debug\alera_native.dll" exit /b 5
if not exist "%BUILD%\runner\Debug\code_forge.dll" exit /b 6
echo ISOLATED_DEBUG_BUNDLE_READY=%BUILD%\runner\Debug
exit /b 0
