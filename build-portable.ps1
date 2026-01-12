# Deprecated: use build-release.ps1
# Keeping this file as a thin wrapper for backward compatibility.

param(
    [string]$OutputDir = "build\VoicePaste-Release",
    [switch]$IncludeModels = $false,
    [switch]$Incremental = $false
)

& "$PSScriptRoot\build-release.ps1" -OutputDir $OutputDir -IncludeModels:$IncludeModels -Incremental:$Incremental
