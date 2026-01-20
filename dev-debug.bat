@echo off
REM VoicePaste - Debug Build + Run

setlocal
cd /d "%~dp0"

echo ========================================
echo VoicePaste - Dev Environment Setup
echo ========================================
echo.

if not exist "python\python.exe" (
    echo Embedded Python not found in root. Setting it up...
    powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\setup-python.ps1"
    if errorlevel 1 (
        echo.
        echo Python setup failed.
        pause
        exit /b 1
    )
) else (
    echo Embedded Python found in root.
)
echo.

echo ========================================
echo VoicePaste - Debug Build
echo ========================================
echo.

dotnet build src\app\VoicePaste.csproj -c Debug
if errorlevel 1 (
  echo.
  echo Build failed.
  pause
  exit /b 1
)

echo.
echo ========================================
echo Starting VoicePaste (Debug)
echo ========================================
echo.

dotnet run --project src\app\VoicePaste.csproj -c Debug

echo.
echo VoicePaste stopped.
pause
