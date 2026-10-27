@echo off
REM VoicePaste - Portable Build
REM Double-click friendly wrapper around build-portable.ps1.

setlocal
cd /d "%~dp0"

set OUT_DIR=build\VoicePaste-Portable

set PS_ARGS=
if exist "%OUT_DIR%\python\python.exe" (
  set PS_ARGS=-Incremental
)

echo ========================================
echo VoicePaste - Portable Build
if "%PS_ARGS%"=="" (
  echo Mode: full first-time build
) else (
  echo Mode: incremental reuse %PS_ARGS%
)
echo ========================================
echo.

REM Sysnative bypasses System32 redirection when this batch is launched by a 32-bit program.
set "POWERSHELL_EXE=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
if exist "%SystemRoot%\Sysnative\WindowsPowerShell\v1.0\powershell.exe" (
  set "POWERSHELL_EXE=%SystemRoot%\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
)
"%POWERSHELL_EXE%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0build-portable.ps1" %PS_ARGS% %*

if errorlevel 1 (
  echo.
  echo Build failed. See output above.
  if "%~1"=="" pause
  exit /b 1
)
echo.

echo Done. VoicePaste starts automatically unless -NoLaunch was specified.
if "%~1"=="" pause
