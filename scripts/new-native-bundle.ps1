# Package only reviewed, inventoried inputs. Extra local files are never included.
[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory)][string]$NativeRuntimeSource,
    [Parameter(Mandatory)][string]$ArchivePath
)
$ErrorActionPreference = 'Stop'
$source = [IO.Path]::GetFullPath($NativeRuntimeSource)
$archive = [IO.Path]::GetFullPath($ArchivePath)
if ((Test-Path -LiteralPath $archive) -or (Test-Path -LiteralPath "$archive.sha256")) { throw 'Native bundle output already exists; choose a new immutable filename.' }
& (Join-Path $PSScriptRoot 'test-native-inputs.ps1') -NativeRuntimeSource $source
if (-not $PSCmdlet.ShouldProcess($archive, 'Create reviewed native input ZIP and SHA-256')) { return }
$inventoryPath = Join-Path $source 'notices/inventory.json'
$inventoryBytes = [IO.File]::ReadAllBytes($inventoryPath)
$inventory = [Text.Encoding]::UTF8.GetString($inventoryBytes) | ConvertFrom-Json
$parent = Split-Path $archive
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$temporary = Join-Path $parent ('.native-' + [guid]::NewGuid().ToString('N') + '.zip')
try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::Open($temporary, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($path in @(@($inventory.files.path) + 'notices/inventory.json' | Sort-Object)) {
            $entry = $zip.CreateEntry($path, [IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = [DateTimeOffset]::new(2000, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
            $stream = $entry.Open()
            try {
                if ($path -eq 'notices/inventory.json') { $stream.Write($inventoryBytes) }
                else {
                    $inputFile = [IO.File]::OpenRead((Join-Path $source $path))
                    try {
                        $sha = [Security.Cryptography.SHA256]::HashData($inputFile)
                        $expected = $inventory.files | Where-Object path -EQ $path
                        if ([Convert]::ToHexString($sha) -ne $expected.sha256 -or $inputFile.Length -ne $expected.bytes) { throw "Input changed during bundling: $path" }
                        $inputFile.Position = 0
                        $inputFile.CopyTo($stream)
                    } finally { $inputFile.Dispose() }
                }
            } finally { $stream.Dispose() }
        }
    } finally { $zip.Dispose() }
    Move-Item -LiteralPath $temporary -Destination $archive
    "$((Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($archive))" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
    Write-Output "Native bundle: $archive"
} finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary } }
