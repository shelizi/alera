@echo off
setlocal
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b 1
for %%I in ("%~dp0..\..") do set "ROOT=%%~fI"
set "MVD_INCLUDE=%LOCALAPPDATA%\Pub\Cache\hosted\pub.dev\multiview_desktop-2.0.2\windows\include"
set "FLUTTER_INCLUDE=%ROOT%\tool\multiview_poc\windows\flutter\ephemeral\cpp_client_wrapper\include"
set "FLUTTER_ROOT=%ROOT%\tool\multiview_poc\windows\flutter\ephemeral"
cl /nologo /std:c++17 /Zs /EHsc /DUNICODE /D_UNICODE /DNOMINMAX /D_HAS_EXCEPTIONS=1 /DALERA_APP_NAME=L\"Alera\" /DALERA_APP_ID=L\"dev.leynier.alera.poc\" /I"%ROOT%\windows" /I"%ROOT%\windows\runner" /I"%FLUTTER_INCLUDE%" /I"%FLUTTER_ROOT%" /I"%MVD_INCLUDE%" "%ROOT%\windows\runner\flutter_window.cpp" "%ROOT%\windows\runner\main.cpp"
exit /b %ERRORLEVEL%
