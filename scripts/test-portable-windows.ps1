# Read-only artifact regression check; never runs packaged executables.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ArchivePath,
    [ValidateSet('release', 'debug')][string]$ExpectedProfile = 'release'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead([IO.Path]::GetFullPath($ArchivePath))
try {
    $manifestEntries = @($archive.Entries | Where-Object FullName -Match '^[^/]+/manifest\.json$')
    if ($manifestEntries.Count -ne 1) { throw 'Expected one root package manifest.' }
    $reader = [IO.StreamReader]::new($manifestEntries[0].Open())
    try { $manifest = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
    $suffix = if ($ExpectedProfile -eq 'debug') { '-debug' } else { '' }
    $prefix = "VoicePaste-$($manifest.version)-windows-x86_64$suffix/"
    if ($manifest.profile -ne $ExpectedProfile -or $manifest.target -ne 'x86_64-pc-windows-msvc') { throw "Not a Windows x64 $ExpectedProfile package." }
    if ($manifestEntries[0].FullName -ne "${prefix}manifest.json") { throw 'Package name/version mismatch.' }
    $expected = @{}
    foreach ($file in $manifest.files) {
        if ($file.path -match '(^/|\\|:|(^|/)\.\.(/|$)|(^|/)data(/|$)|(^|/)config\.json$)' -or $expected.ContainsKey($file.path)) { throw 'Unsafe, private or duplicate manifest path.' }
        $expected[$file.path] = $file
    }
    $nativeFiles = (Import-PowerShellDataFile (Join-Path $PSScriptRoot 'native-runtime.psd1')).Files
    foreach ($required in (@('VoicePaste.exe', 'runtime/voicepaste-stt-worker.exe') + @($nativeFiles | ForEach-Object { "runtime/$_" }))) {
        if (-not $expected.ContainsKey($required)) { throw "Missing executable: $required" }
    }
    if ($archive.Entries.Count -ne $expected.Count + 1) { throw 'Unexpected archive entries.' }
    $seen = @{}
    foreach ($entry in $archive.Entries) {
        if (-not $entry.FullName.StartsWith($prefix, [StringComparison]::Ordinal)) { throw 'Archive entry outside package root.' }
        $path = $entry.FullName.Substring($prefix.Length)
        if ($path -eq 'manifest.json') { continue }
        if (-not $expected.ContainsKey($path) -or $seen.ContainsKey($path)) { throw "Unexpected or duplicate entry: $path" }
        $seen[$path] = $true
        $file = $expected[$path]
        if ($entry.Length -ne $file.bytes -or $entry.Length -eq 0) { throw "Size mismatch: $path" }
        $stream = $entry.Open()
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $hash = [Convert]::ToHexString($sha.ComputeHash($stream)) } finally { $sha.Dispose(); $stream.Dispose() }
        if ($hash -ne $file.sha256) { throw "Archive hash mismatch: $path" }
    }
    Write-Output "Archive verified: $($expected.Count) files; $ExpectedProfile x64; no user data."
} finally { $archive.Dispose() }
