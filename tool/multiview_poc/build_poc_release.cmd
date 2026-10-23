@echo off
setlocal
set "CommonProgramFiles=C:\Program Files\Common Files"
set "CommonProgramW6432=C:\Program Files\Common Files"
set "CommonProgramFiles(x86)=C:\Program Files (x86)\Common Files"
set "PROGRAMDATA=C:\ProgramData"
set "ALLUSERSPROFILE=C:\ProgramData"
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat"
if errorlevel 1 exit /b %ERRORLEVEL%
cd /d "%~dp0"
cmake -S windows -B build/windows/poc_ninja -G "Ninja Multi-Config" -DFLUTTER_TARGET_PLATFORM=windows-x64
if errorlevel 1 exit /b %ERRORLEVEL%
cmake --build build/windows/poc_ninja --config Release --parallel 6
if errorlevel 1 exit /b %ERRORLEVEL%
cmake --install build/windows/poc_ninja --config Release
exit /b %ERRORLEVEL%
