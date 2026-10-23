@echo off
setlocal
set "ALERA_E2E_DESKTOP=1"
set "ALERA_E2E_REUSE_CLI=1"
set "CommonProgramFiles=C:\Program Files\Common Files"
set "CommonProgramW6432=C:\Program Files\Common Files"
set "CommonProgramFiles(x86)=C:\Program Files (x86)\Common Files"
set "PROGRAMDATA=C:\ProgramData"
set "ALLUSERSPROFILE=C:\ProgramData"
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %ERRORLEVEL%
for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
cmake -S "%ROOT%\windows" -B "C:\a_mv_20261022" -G "Ninja Multi-Config" -DFLUTTER_TARGET_PLATFORM=windows-x64
exit /b %ERRORLEVEL%
