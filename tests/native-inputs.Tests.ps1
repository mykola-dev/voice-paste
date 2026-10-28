# Synthetic fixtures only. No downloads, native execution or application data.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$work = Join-Path $root ('build/native-input-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $source = Join-Path $work 'source'
    $paths = @((Import-PowerShellDataFile "$root/scripts/native-runtime.psd1").Files) + @('include/ctranslate2/models/whisper.h', 'notices/license.txt')
    $files = @($paths | ForEach-Object {
        $path = Join-Path $source $_
        New-Item -ItemType Directory -Path (Split-Path $path) -Force | Out-Null
        [IO.File]::WriteAllText($path, 'fixture')
        @{path=$_;bytes=7;sha256=(Get-FileHash $path).Hash;version='fixture';origin='https://example.invalid/source';license='MIT';licensePath='notices/license.txt'}
    })
    $baseline = @{schemaVersion=1;ct2Revision='226c95d94e660c48b11c62e108886b7ef76d589d';reviewed=$true;files=$files} | ConvertTo-Json -Depth 8
    function Write-Inventory($value) { $value | ConvertTo-Json -Depth 8 | Set-Content "$source/notices/inventory.json" -Encoding utf8 }
    foreach ($scenario in @('valid','unreviewed','wrong-revision','missing-crt','missing-license','tampered-hash','tampered-size','duplicate','traversal','unexpected-dll','missing-origin','extra-notice')) {
        $inventory = $baseline | ConvertFrom-Json
        switch ($scenario) {
            'unreviewed' { $inventory.reviewed = $false }
            'wrong-revision' { $inventory.ct2Revision = 'wrong' }
            'missing-crt' { $inventory.files = @($inventory.files | Where-Object path -NE 'msvcp140_1.dll') }
            'missing-license' { $inventory.files[0].licensePath = 'notices/absent.txt' }
            'tampered-hash' { $inventory.files[0].sha256 = '0' * 64 }
            'tampered-size' { $inventory.files[0].bytes = 8 }
            'duplicate' { $inventory.files += $inventory.files[0] }
            'traversal' { $inventory.files[0].path = '../outside.dll' }
            'unexpected-dll' { $inventory.files[0].path = 'cudnn64_9.dll' }
            'missing-origin' { $inventory.files[0].origin = '' }
            'extra-notice' { [IO.File]::WriteAllText("$source/notices/private.txt", 'not for distribution') }
        }
        Write-Inventory $inventory
        $failed = $false
        try { & "$root/scripts/test-native-inputs.ps1" -NativeRuntimeSource $source } catch { $failed = $true }
        if ($failed -ne ($scenario -ne 'valid')) { throw "Unexpected native validation outcome: $scenario" }
        if ($scenario -eq 'extra-notice') { Remove-Item "$source/notices/private.txt" }
        "PASS: $scenario"
    }
    Write-Inventory ($baseline | ConvertFrom-Json)
    # Extra cache files must never leak into the portable input ZIP.
    [IO.File]::WriteAllText("$source/private-config.json", 'not for distribution')
    foreach ($name in @('first.zip', 'second.zip')) {
        & "$root/scripts/new-native-bundle.ps1" -NativeRuntimeSource $source -ArchivePath "$work/$name"
    }
    if ((Get-FileHash "$work/first.zip").Hash -ne (Get-FileHash "$work/second.zip").Hash) { throw 'Native bundle bytes are not deterministic.' }
    $zip = [IO.Compression.ZipFile]::OpenRead("$work/first.zip")
    try {
        if ($zip.Entries.FullName -contains 'private-config.json' -or $zip.Entries.Count -ne $files.Count + 1) { throw 'Native bundle includes extra files.' }
    } finally { $zip.Dispose() }
    $failed = $false
    try { & "$root/scripts/new-native-bundle.ps1" -NativeRuntimeSource $source -ArchivePath "$work/first.zip" } catch { $failed = $true }
    if (-not $failed) { throw 'Native bundle overwrite was allowed.' }
    'PASS: deterministic allowlisted ZIP and immutable output'

    # Mock transport only; exercise the real hash/extraction/inventory code offline.
    $script:DownloadFixture = "$work/first.zip"
    function Invoke-WebRequest {
        param($Uri, $OutFile, $TimeoutSec)
        Copy-Item -LiteralPath $DownloadFixture -Destination $OutFile
    }
    $sha = (Get-FileHash "$work/first.zip").Hash
    & "$root/scripts/get-native-inputs.ps1" -Url 'https://example.invalid/native.zip' -Sha256 $sha -Destination "$work/downloaded"
    if (-not (Test-Path "$work/downloaded/msvcp140_1.dll")) { throw 'Verified download was not extracted.' }
    foreach ($scenario in @('wrong-hash','http','existing','zip-traversal','zip-duplicate','zip-symlink','extra-file','unreviewed')) {
        $url = 'https://example.invalid/native.zip'
        $expected = $sha
        $destination = "$work/download-$scenario"
        $script:DownloadFixture = "$work/first.zip"
        switch ($scenario) {
            'wrong-hash' { $expected = '0' * 64 }
            'http' { $url = 'http://example.invalid/native.zip' }
            'existing' { $destination = "$work/downloaded" }
            default {
                $script:DownloadFixture = "$work/$scenario.zip"
                Copy-Item "$work/first.zip" $script:DownloadFixture
                $zip = [IO.Compression.ZipFile]::Open($script:DownloadFixture, [IO.Compression.ZipArchiveMode]::Update)
                try {
                    $name = switch ($scenario) {
                        'zip-traversal' { '../escape.txt' }
                        'zip-duplicate' { 'msvcp140.dll' }
                        'zip-symlink' { 'link' }
                        'extra-file' { 'private-config.json' }
                        'unreviewed' {
                            $zip.GetEntry('notices/inventory.json').Delete()
                            'notices/inventory.json'
                        }
                    }
                    $entry = $zip.CreateEntry($name)
                    if ($scenario -eq 'zip-symlink') { $entry.ExternalAttributes = [int](-1610612736) } # Unix symlink mode 0120000
                    $writer = [IO.StreamWriter]::new($entry.Open())
                    try {
                        if ($scenario -eq 'unreviewed') { $writer.Write($baseline.Replace('"reviewed": true', '"reviewed": false')) }
                        else { $writer.Write('fixture') }
                    } finally { $writer.Dispose() }
                } finally { $zip.Dispose() }
                $expected = (Get-FileHash $script:DownloadFixture).Hash
            }
        }
        $failed = $false
        try { & "$root/scripts/get-native-inputs.ps1" -Url $url -Sha256 $expected -Destination $destination } catch { $failed = $true }
        if (-not $failed -or ($scenario -ne 'existing' -and (Test-Path $destination))) { throw "Unsafe native download accepted: $scenario" }
        "PASS: download-$scenario"
    }
    if (Test-Path "$work/escape.txt") { throw 'ZIP escaped extraction directory.' }
    if (Get-ChildItem $work -Directory -Filter '.download-native-*') { throw 'Download staging was not cleaned up.' }
} finally { Remove-Item -LiteralPath $work -Recurse -Force }
