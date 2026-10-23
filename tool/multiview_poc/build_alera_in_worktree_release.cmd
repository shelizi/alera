@echo off
setlocal
rem Isolated Release-only verification. NEVER build against a real Alera profile.
set "ALERA_E2E_DESKTOP=1"
set "ALERA_E2E_REUSE_CLI=0"
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
set "ALERA_CARGOKIT_TEMP_DIR=%ROOT%\build\r"
set "EXPECTED_E2E_SUPPORT=%ROOT:\=/%/build/windows/shared_release_ninja/e2e_support"
set "GENERATED_CONFIG=%ROOT%\windows\flutter\ephemeral\generated_config.cmake"
if not exist "%GENERATED_CONFIG%" (
    echo ERROR: missing Flutter release generated config. Build once using --dart-define=ALERA_E2E_SUPPORT_DIR=%EXPECTED_E2E_SUPPORT%
    exit /b 7
)
python -c "import os,base64,sys; token=base64.b64encode(('ALERA_E2E_SUPPORT_DIR='+os.environ['EXPECTED_E2E_SUPPORT']).encode('utf-8')).decode('ascii'); sys.exit(0 if token in open(os.environ['GENERATED_CONFIG'],encoding='utf-8').read() else 8)"
if errorlevel 1 (
    echo ERROR: refusing unsafe Release build without isolated support directory.
    exit /b 8
)
set "BUILD=%ROOT%\build\windows\shared_release_ninja"
if not exist "%BUILD%" mkdir "%BUILD%"
cmake -S "%ROOT%\windows" -B "%BUILD%" -G "Ninja Multi-Config" -DFLUTTER_TARGET_PLATFORM=windows-x64
if errorlevel 1 exit /b %ERRORLEVEL%
ninja.exe -C "%BUILD%" -f build-Release.ninja -j 4 runner/Release/alera-dev.exe
if errorlevel 1 exit /b %ERRORLEVEL%
ninja.exe -C "%BUILD%" -f build-Release.ninja -j 2 plugins/alera_native/Release/alera_native.dll plugins/code_forge/Release/code_forge.dll plugins/flutter_local_notifications_windows/shared/Release/flutter_local_notifications_windows.dll plugins/jni/shared/Release/dartjni.dll
if errorlevel 1 exit /b %ERRORLEVEL%
cmake --install "%BUILD%" --config Release
if errorlevel 1 exit /b %ERRORLEVEL%
if not exist "%BUILD%\runner\Release\alera-dev.exe" exit /b 3
if not exist "%BUILD%\runner\Release\data\app.so" exit /b 4
if not exist "%BUILD%\runner\Release\alera_native.dll" exit /b 5
if not exist "%BUILD%\runner\Release\code_forge.dll" exit /b 6
if not exist "%BUILD%\runner\Release\flutter_windows.dll" exit /b 9
if not exist "%BUILD%\runner\Release\multiview_desktop_plugin.dll" exit /b 10
if not exist "%BUILD%\runner\Release\resources\alera\alera.exe" exit /b 11
echo ISOLATED_RELEASE_BUNDLE_READY=%BUILD%\runner\Release
exit /b 0
