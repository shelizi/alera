@echo off
setlocal
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b 2
for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
set "TEST_BUILD=%ROOT%\build\windows\native_guard_tests"
cmake -S "%ROOT%\windows\runner\tests" -B "%TEST_BUILD%" -G Ninja -DCMAKE_BUILD_TYPE=Debug
if errorlevel 1 exit /b 3
cmake --build "%TEST_BUILD%"
if errorlevel 1 exit /b 4
ctest --test-dir "%TEST_BUILD%" --output-on-failure
exit /b %ERRORLEVEL%
