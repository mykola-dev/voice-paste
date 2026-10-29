@echo off
setlocal
rem One Rust build implementation; no PowerShell/Bash/Python required.
pushd "%~dp0"
if errorlevel 1 exit /b 1
set "cargo=cargo.exe"
rem Preserve the existing repository-local toolchain without requiring a global install.
if exist "%~dp0build\cache\rust-tools\cargo\bin\cargo.exe" (
    set "CARGO_HOME=%~dp0build\cache\rust-tools\cargo"
    set "RUSTUP_HOME=%~dp0build\cache\rust-tools\rustup"
    set "cargo=%~dp0build\cache\rust-tools\cargo\bin\cargo.exe"
    set "PATH=%~dp0build\cache\rust-tools\cargo\bin;%PATH%"
)
"%cargo%" xtask package --release %*
set "result=%errorlevel%"
popd
if not "%result%"=="0" (
    echo Build failed. See the error above.
) else (
    echo Build completed. Output paths are listed above.
)
if not defined CI pause
exit /b %result%
