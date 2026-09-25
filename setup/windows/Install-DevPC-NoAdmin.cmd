@echo off
rem Double-click to set up a DEVELOPER PC WITHOUT admin rights (or where installers are blocked).
rem Rust is unpacked into your user folder; other tools must already be installed.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install-FromBundle.ps1" -Role Dev -NoAdmin %*
set RESULT=%ERRORLEVEL%
echo.
pause
exit /b %RESULT%
