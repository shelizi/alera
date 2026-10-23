@echo off
setlocal
call "%~dp0build_alera_shared_debug.cmd" >"C:\a_mv_20261022\shared_build.log" 2>&1
set "CODE=%ERRORLEVEL%"
echo NATIVE_BUILD_EXIT=%CODE%
powershell.exe -NoProfile -Command "$r=Get-Content 'C:/a_mv_20261022/shared_build.log'; $r | Select-String -Pattern 'FAILED:|error C[0-9]+|error LNK[0-9]+|fatal error|ninja: error|CMake Error|\berror:|undefined reference|No rule to make' | Select-Object -Last 35 | ForEach-Object { $_.Line }; Write-Output '---LAST_LINES---'; $r | Select-Object -Last 8"
exit /b %CODE%
