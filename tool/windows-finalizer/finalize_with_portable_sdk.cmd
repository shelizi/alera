@echo off
setlocal EnableExtensions EnableDelayedExpansion

set "SDK_ROOT=D:\Windows Kits\10"
set "SDK_VERSION=10.0.26100.0"
set "PLATFORM_TOOLSET=v143"

if defined ALERA_WINDOWS_SDK_ROOT set "SDK_ROOT=!ALERA_WINDOWS_SDK_ROOT!"
if defined ALERA_WINDOWS_SDK_VERSION set "SDK_VERSION=!ALERA_WINDOWS_SDK_VERSION!"
if defined ALERA_PLATFORM_TOOLSET set "PLATFORM_TOOLSET=!ALERA_PLATFORM_TOOLSET!"

set "VERIFY_ARG="
if /I "%~1"=="verify" set "VERIFY_ARG=-VerifyOnly"

if not exist "!SDK_ROOT!\Include\!SDK_VERSION!" goto :sdk_missing
if not exist "!SDK_ROOT!\Include\!SDK_VERSION!\um\Windows.h" goto :headers_missing

goto :run

:sdk_missing
echo ERROR: Selected SDK version directory does not exist:
echo   !SDK_ROOT!\Include\!SDK_VERSION!
exit /b 2

:headers_missing
echo ERROR: Windows SDK headers were not found at:
echo   !SDK_ROOT!\Include\!SDK_VERSION!\um\Windows.h
echo.
echo Expected default portable SDK root: D:\Windows Kits\10
exit /b 2

:run
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0finalize_windows.ps1" -WindowsSdkRoot "!SDK_ROOT!" -WindowsSdkVersion "!SDK_VERSION!" -PlatformToolset "!PLATFORM_TOOLSET!" !VERIFY_ARG!
exit /b !ERRORLEVEL!
