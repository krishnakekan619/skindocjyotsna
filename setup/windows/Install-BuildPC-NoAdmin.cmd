@echo off
rem Double-click to set up a PRODUCTION BUILD PC WITHOUT admin rights (or where installers are blocked).
rem Rust (exact locked version) is unpacked into your user folder; other tools must already be installed.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install-FromBundle.ps1" -Role Build -NoAdmin %*
set RESULT=%ERRORLEVEL%
echo.
pause
exit /b %RESULT%
