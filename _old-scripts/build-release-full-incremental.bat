@echo off
REM VoicePaste Full Portable Release Builder (incremental)
REM Rebuilds app/ but reuses existing python/ and models/ when present.

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
