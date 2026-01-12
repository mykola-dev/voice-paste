@echo off
REM VoicePaste - Quick Debug Runner
REM Runs the debug build without rebuilding

echo ========================================
echo VoicePaste - Quick Debug Run
echo ========================================
echo.

REM Check if debug build exists
if not exist "src\app\bin\Debug\net8.0-windows\VoicePaste.exe" (
    echo ERROR: Debug build not found!
    echo Please build first using: run-debug.bat
    echo Or run: dotnet build src\app\VoicePaste.csproj
    pause
    exit /b 1
)

echo Stopping any running instances...
taskkill /F /IM VoicePaste.exe 2>nul
timeout /t 1 /nobreak >nul

echo.
echo Starting VoicePaste...
echo Log file: %TEMP%\VoicePaste\voicepaste.log
echo.
echo ========================================
echo Instructions:
echo - Press ScrollLock to start recording
echo - Press ScrollLock again to stop and transcribe
echo - Right-click tray icon for menu
echo - Press Ctrl+C here to stop
echo ========================================
echo.

REM Run the executable directly
src\app\bin\Debug\net8.0-windows\VoicePaste.exe

echo.
echo ========================================
echo VoicePaste stopped
echo ========================================
pause
