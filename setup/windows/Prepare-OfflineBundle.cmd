@echo off
rem Double-click (on a PC WITH internet) to download the Windows offline setup bundle into dist\.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Prepare-OfflineBundle.ps1" %*
set RESULT=%ERRORLEVEL%
echo.
pause
exit /b %RESULT%
