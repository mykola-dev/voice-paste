@echo off
REM VoicePaste - Debug Build Runner
REM Builds and runs VoicePaste in debug mode with console output

REM Set UTF-8 encoding for Cyrillic support
chcp 65001 >nul

REM Force Python UTF-8 mode
set PYTHONIOENCODING=utf-8
set PYTHONUTF8=1

echo ========================================
echo VoicePaste - Debug Build
echo ========================================
echo.

REM Check if we're in the correct directory
if not exist "src\app\VoicePaste.csproj" (
    echo ERROR: VoicePaste.csproj not found!
    echo Please run this script from the project root directory.
    pause
    exit /b 1
)

echo [1/3] Building VoicePaste...
echo.
dotnet build src\app\VoicePaste.csproj --configuration Debug

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo ========================================
    echo Build FAILED!
    echo ========================================
    pause
    exit /b %ERRORLEVEL%
)

echo.
echo ========================================
echo Build SUCCESS!
echo ========================================
echo.
echo [2/3] Stopping any running instances...
taskkill /F /IM VoicePaste.exe 2>nul
timeout /t 1 /nobreak >nul

echo.
echo [3/3] Starting VoicePaste in debug mode...
echo.
echo ========================================
echo Console Output (Ctrl+C to stop):
echo ========================================
echo.

REM Run the application (console will show output)
dotnet run --project src\app\VoicePaste.csproj --configuration Debug

echo.
echo ========================================
echo VoicePaste stopped
echo ========================================
pause
