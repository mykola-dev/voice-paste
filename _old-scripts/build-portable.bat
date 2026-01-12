@echo off
REM Deprecated: use build-release-full.bat or build-release-incremental.bat
REM Keeping this file as a thin wrapper for backward compatibility.

call "%~dp0build-release-full.bat" %*
