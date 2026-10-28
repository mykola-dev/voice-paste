[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Url,
    [Parameter(Mandatory)][string]$Sha256,
    [Parameter(Mandatory)][string]$Destination
)
$ErrorActionPreference = 'Stop'
if ($Url -notmatch '^https://[^/]+/' -or $Sha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'Native bundle needs an HTTPS URL and a pinned SHA-256.' }
$target = [IO.Path]::GetFullPath($Destination)
if (Test-Path -LiteralPath $target) { throw 'Native destination already exists; refusing to overwrite it.' }
$parent = Split-Path $target
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$work = Join-Path $parent ('.download-native-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $archive = Join-Path $work 'native.zip'
    Invoke-WebRequest -Uri $Url -OutFile $archive -TimeoutSec 600
    if ((Get-FileHash -LiteralPath $archive).Hash -ne $Sha256) { throw 'Native bundle SHA-256 mismatch.' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $seen = @{}
        $total = 0L
        foreach ($entry in $zip.Entries) {
            $path = $entry.FullName
            if (-not $path -or $path -match '(^/|\\|:|(^|/)\.{1,2}(/|$)|//|[<>"|?*\x00-\x1f]|[ .](/|$))' -or $path.EndsWith('/') -or $seen.ContainsKey($path) -or (($entry.ExternalAttributes -shr 16) -band 0xF000) -eq 0xA000) {
                throw "Unsafe, duplicate or non-file native ZIP entry: $path"
            }
            $seen[$path] = $true
            $total += $entry.Length
            if ($total -gt 4GB -or $zip.Entries.Count -gt 10000) { throw 'Native ZIP exceeds extraction limits.' }
        }
    } finally { $zip.Dispose() }
    $expanded = Join-Path $work 'expanded'
    [IO.Compression.ZipFile]::ExtractToDirectory($archive, $expanded)
    & (Join-Path $PSScriptRoot 'test-native-inputs.ps1') -NativeRuntimeSource $expanded
    $inventory = Get-Content -Raw -LiteralPath (Join-Path $expanded 'notices/inventory.json') | ConvertFrom-Json
    if (@(Get-ChildItem -LiteralPath $expanded -Recurse -File).Count -ne @($inventory.files).Count + 1) { throw 'Native ZIP includes uninventoried files.' }
    Move-Item -LiteralPath $expanded -Destination $target
    Write-Output "Verified native bundle extracted: $target"
} finally { if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force } }
