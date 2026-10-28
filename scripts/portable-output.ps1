# Shared publication boundary for Debug and Release. Never publish or remove user data.
function Publish-PortableFolder {
    param([Parameter(Mandatory)][string]$Source, [Parameter(Mandatory)][string]$Destination)
    function Read-ManagedManifest([string]$Directory) {
        $manifest = Get-Content -Raw -LiteralPath (Join-Path $Directory 'manifest.json') | ConvertFrom-Json
        foreach ($entry in $manifest.files) {
            $path = [string]$entry.path
            if (-not $path -or $path -match '(^[/\\]|:|\\|(^|/)\.\.?(/|$)|^data(/|$)|^config\.json$|^manifest\.json$)') {
                throw "Unsafe managed package path: $path"
            }
        }
        return $manifest
    }
    $manifest = Read-ManagedManifest $Source
    $oldManifest = $null
    if (Test-Path -LiteralPath (Join-Path $Destination 'manifest.json')) {
        $oldManifest = Read-ManagedManifest $Destination
    } elseif (Test-Path -LiteralPath $Destination) {
        $unknown = @(Get-ChildItem -LiteralPath $Destination -Force | Where-Object Name -ne 'data')
        if ($unknown.Count) { throw "Destination has unmanaged files; migrate it first: $Destination" }
    }
    foreach ($entry in $manifest.files) {
        if ((Get-FileHash -LiteralPath (Join-Path $Source $entry.path)).Hash -ne $entry.sha256) {
            throw "Staging hash mismatch: $($entry.path)"
        }
    }
    $prefix = [IO.Path]::GetFullPath($Destination).TrimEnd('\', '/') + '\'
    $blockers = @(Get-Process -Name VoicePaste, voicepaste-stt-worker -ErrorAction SilentlyContinue | Where-Object {
        $_.Path -and $_.Path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
    })
    if ($blockers.Count) { throw "Running VoicePaste locks $Destination (PID $($blockers.Id -join ', ')). Finish recording, save Settings and quit via tray Quit. No files changed." }
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    $wanted = @{}
    foreach ($entry in $manifest.files) {
        $wanted[$entry.path] = $true
        $target = Join-Path $Destination $entry.path
        New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $Source $entry.path) -Destination $target -Force
        if ((Get-FileHash -LiteralPath $target).Hash -ne $entry.sha256) { throw "Destination hash mismatch: $($entry.path)" }
    }
    foreach ($entry in $oldManifest.files) {
        if (-not $wanted.ContainsKey($entry.path)) {
            $stale = Join-Path $Destination $entry.path
            if (Test-Path -LiteralPath $stale -PathType Leaf) { Remove-Item -LiteralPath $stale -Force }
        }
    }
    Copy-Item -LiteralPath (Join-Path $Source 'manifest.json') -Destination (Join-Path $Destination 'manifest.json') -Force
}
