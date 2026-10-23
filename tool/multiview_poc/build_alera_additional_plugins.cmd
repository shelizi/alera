@echo off
setlocal
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %ERRORLEVEL%
ninja.exe -C "C:\a_mv_20261022" -f build-Debug.ninja -j 4 plugins/flutter_local_notifications_windows/shared/Debug/flutter_local_notifications_windows.dll plugins/jni/shared/Debug/dartjni.dll >"C:\a_mv_20261022\plugin_native_build.log" 2>&1
set "CODE=%ERRORLEVEL%"
if "%CODE%"=="0" if not exist "C:\a_mv_20261022\plugins\flutter_local_notifications_windows\shared\Debug\flutter_local_notifications_windows.dll" set "CODE=2"
if "%CODE%"=="0" if not exist "C:\a_mv_20261022\plugins\jni\shared\Debug\dartjni.dll" set "CODE=2"
echo PLUGIN_NATIVE_BUILD_EXIT=%CODE%
powershell.exe -NoProfile -Command "$r=Get-Content 'C:/a_mv_20261022/plugin_native_build.log'; $r | Select-String -Pattern 'FAILED:|error C[0-9]+|error LNK[0-9]+|fatal error|ninja: error|CMake Error|\berror:' | Select-Object -Last 18 | ForEach-Object { $_.Line }; Write-Output '---TAIL---'; $r | Select-Object -Last 8"
exit /b %CODE%
