@echo off
REM VoicePaste Full Portable Release Builder (clickable)
REM Runs build-release.ps1 with ExecutionPolicy Bypass.

setlocal

cd /d "%~dp0"

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build-release.ps1" %*

if errorlevel 1 (
  echo.
  echo Build failed. See output above.
  pause
  exit /b 1
)

echo.
echo Build finished.
pause
