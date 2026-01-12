@echo off
REM VoicePaste - Portable Release (smart)
REM Double-click friendly: reuses existing python/ and models/ when present.

setlocal
cd /d "%~dp0"

set OUT_DIR=build\VoicePaste-Release

REM Decide whether we can do incremental reuse
set PS_ARGS=
if exist "%OUT_DIR%\python\python.exe" (
  set PS_ARGS=-Incremental
)

REM Always bundle default model (large-v3-turbo) into models/
REM Add more on demand inside build-release.ps1 via -AdditionalModels
set PS_ARGS=%PS_ARGS% -DownloadDefaultModel -CacheLargeModels

REM Optional: also copy any existing HF cache (add-on models) into models/
set HF_CACHE=%USERPROFILE%\.cache\huggingface\hub
if exist "%OUT_DIR%\models" (
  set PS_ARGS=%PS_ARGS% -IncludeModels
) else (
  if exist "%HF_CACHE%" (
    set PS_ARGS=%PS_ARGS% -IncludeModels
  )
)

echo ========================================
echo VoicePaste - Portable Release Build
if "%PS_ARGS%"=="" (
  echo Mode: full (first time)
) else (
  echo Mode: incremental reuse %PS_ARGS%
)
echo ========================================
echo.

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build-release.ps1" %PS_ARGS%

if errorlevel 1 (
  echo.
  echo Build failed. See output above.
  pause
  exit /b 1
)
echo.

echo Done. Run: %OUT_DIR%\VoicePaste.exe
pause
