@echo off
setlocal
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b %ERRORLEVEL%
ninja.exe -C "C:\a_mv_20261022" -f build-Debug.ninja -j 4 runner/CMakeFiles/alera-dev.dir/Debug/flutter_window.cpp.obj runner/CMakeFiles/alera-dev.dir/Debug/main.cpp.obj
exit /b %ERRORLEVEL%
