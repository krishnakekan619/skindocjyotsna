@echo off
rem Double-click to set up a DEVELOPER PC from this offline bundle (Git, Node.js, Rust, VS Build Tools, WebView2).
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install-FromBundle.ps1" -Role Dev %*
set RESULT=%ERRORLEVEL%
echo.
pause
exit /b %RESULT%
