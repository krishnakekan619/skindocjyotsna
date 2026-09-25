@echo off
rem Double-click to set up a PRODUCTION BUILD PC from this offline bundle.
rem Node.js and Rust must match tools.lock exactly, so release installers are reproducible.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install-FromBundle.ps1" -Role Build %*
set RESULT=%ERRORLEVEL%
echo.
pause
exit /b %RESULT%
