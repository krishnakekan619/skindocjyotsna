@echo off
rem Double-click to set up a CLINIC PC from this offline bundle (WebView2 + SkinDocJyotsna).
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install-FromBundle.ps1" -Role Clinic %*
set RESULT=%ERRORLEVEL%
echo.
pause
exit /b %RESULT%
