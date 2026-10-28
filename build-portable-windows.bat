@echo off
setlocal
where pwsh.exe >nul 2>nul
if errorlevel 1 (
    echo PowerShell 7 x64 is required. Install it and run this file again.
    if not defined CI pause
    exit /b 1
)
pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-rust.ps1" -Action package -Release %*
set "result=%errorlevel%"
if not "%result%"=="0" (
    echo Build failed. See the error above.
) else (
    echo Portable release is ready in "%~dp0build\release".
    echo ZIP and checksum are in "%~dp0build\packages".
)
if not defined CI pause
exit /b %result%
