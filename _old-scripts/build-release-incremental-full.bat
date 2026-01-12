@echo off
REM VoicePaste Incremental Full Builder (no re-download)
REM Calls build-release.ps1 in incremental mode.

setlocal
cd /d "%~dp0"

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build-release.ps1" -Incremental %*

if errorlevel 1 (
  echo.
  echo Build failed. See output above.
  pause
  exit /b 1
)

echo.
echo Build finished.
pause
