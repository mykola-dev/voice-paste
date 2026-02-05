# Deprecated: use build-release.ps1
# Keeping this file as a thin wrapper for backward compatibility.

param(
    [string]$OutputDir = "build\VoicePaste-Release",
    [switch]$Incremental = $false
)

& "$PSScriptRoot\build-release.ps1" -OutputDir $OutputDir -Incremental:$Incremental
