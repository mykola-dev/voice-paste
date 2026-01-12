@echo off
REM VoicePaste Incremental Release Builder
REM Rebuilds only the .NET app into build/VoicePaste-Release/app.
REM Keeps embedded python/models from a previous full build.

setlocal enabledelayedexpansion

set OUTPUT_DIR=build\VoicePaste-Release
set APP_DIR=%OUTPUT_DIR%\app

echo === VoicePaste Incremental Release Build ===
echo.

REM Clean only app output (keep python/models)
if exist "%APP_DIR%" (
    echo Cleaning app output directory...
    rmdir /s /q "%APP_DIR%"
)

if not exist "%OUTPUT_DIR%" mkdir "%OUTPUT_DIR%"
mkdir "%APP_DIR%"

echo Publishing .NET application...
dotnet publish src\app\VoicePaste.csproj ^
    -c Release ^
    -r win-x64 ^
    --self-contained true ^
    -p:PublishSingleFile=false ^
    -p:IncludeNativeLibrariesForSelfExtract=true ^
    -o "%APP_DIR%"

if errorlevel 1 (
    echo Publish failed!
    exit /b 1
)

REM Copy transcription worker and default config into app folder
if not exist "%APP_DIR%\transcribe" mkdir "%APP_DIR%\transcribe"
copy /y "src\transcribe\transcribe.py" "%APP_DIR%\transcribe\" >nul
copy /y "config\config.json" "%APP_DIR%\" >nul

echo.
echo === Build Complete ===
echo Output: %OUTPUT_DIR%
echo.

echo Tip: run "%OUTPUT_DIR%\VoicePaste.bat"

endlocal
