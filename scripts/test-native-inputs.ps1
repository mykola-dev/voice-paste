# Validate explicit release inputs; never discovers installed packages or runs DLLs.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$NativeRuntimeSource)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = [IO.Path]::GetFullPath($NativeRuntimeSource).TrimEnd([IO.Path]::DirectorySeparatorChar)
if ((Get-Item -LiteralPath $root).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Native input root is a link.' }
$inventoryPath = Join-Path $root 'notices/inventory.json'
$inventory = Get-Content -Raw -LiteralPath $inventoryPath | ConvertFrom-Json
if ($inventory.schemaVersion -ne 1 -or $inventory.ct2Revision -cne '226c95d94e660c48b11c62e108886b7ef76d589d') {
    throw 'Native inventory schema/CT2 revision mismatch.'
}
if ($inventory.reviewed -isnot [bool] -or -not $inventory.reviewed) { throw 'Native inventory needs maintainer review; do not fabricate approval.' }
$required = (Import-PowerShellDataFile (Join-Path $PSScriptRoot 'native-runtime.psd1')).Files
$seen = @{}
foreach ($entry in $inventory.files) {
    $path = [string]$entry.path
    if (-not $path -or $path -match '(^/|\\|:|(^|/)\.{1,2}(/|$)|//|[<>"|?*\x00-\x1f]|[ .](/|$))' -or $path.EndsWith('/') -or $seen.ContainsKey($path)) {
        throw "Unsafe or duplicate native inventory path: $path"
    }
    if ($path -eq 'notices/inventory.json' -or ($path -notin $required -and $path -notmatch '^(include|notices)/')) {
        throw "Unexpected native inventory file: $path"
    }
    $seen[$path] = $entry
    $full = Join-Path $root $path
    $part = Get-Item -LiteralPath $full
    while ($part.FullName -ne $root) {
        if ($part.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Native input is a link: $path" }
        $part = Get-Item -LiteralPath ([IO.Path]::GetDirectoryName($part.FullName))
    }
    $file = Get-Item -LiteralPath $full
    if ($file.PSIsContainer -or $entry.bytes -le 0 -or $file.Length -ne $entry.bytes -or $entry.sha256 -notmatch '^[a-fA-F0-9]{64}$' -or (Get-FileHash -LiteralPath $full).Hash -ne $entry.sha256) {
        throw "Native input hash/size mismatch: $path"
    }
    if ([string]::IsNullOrWhiteSpace($entry.version) -or $entry.origin -notmatch '^https://[^/]+/' -or [string]::IsNullOrWhiteSpace($entry.license)) {
        throw "Native input lacks version, HTTPS origin or license: $path"
    }
}
foreach ($path in @($required) + @('include/ctranslate2/models/whisper.h')) {
    if (-not $seen.ContainsKey($path)) { throw "Native inventory missing required file: $path" }
}
foreach ($entry in $inventory.files) {
    if ($entry.licensePath -notmatch '^notices/' -or -not $seen.ContainsKey($entry.licensePath)) {
        throw "Missing inventoried license text for $($entry.path)"
    }
}
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $root 'notices') -Recurse -File) {
    $relative = [IO.Path]::GetRelativePath($root, $file.FullName).Replace('\', '/')
    if ($relative -ne 'notices/inventory.json' -and -not $seen.ContainsKey($relative)) { throw "Uninventoried notice file: $relative" }
}
Write-Output "Native inputs verified: $($seen.Count) files; pinned CT2; hashes and license coverage."
