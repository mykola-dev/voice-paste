# Self-contained structural regression tests; no native inputs/build/app launch.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$work = Join-Path $root ('build/package-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $work -Force | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem
try {
    foreach ($scenario in @('valid', 'valid-debug', 'corrupt', 'private', 'debug', 'missing-worker')) {
        $payload = Join-Path $work $scenario
        New-Item -ItemType Directory (Join-Path $payload 'runtime') -Force | Out-Null
        $names = @('VoicePaste.exe', 'runtime/voicepaste-stt-worker.exe') + @((Import-PowerShellDataFile (Join-Path $root 'scripts/native-runtime.psd1')).Files | ForEach-Object { "runtime/$_" })
        if ($scenario -eq 'missing-worker') { $names = @($names | Where-Object { $_ -ne 'runtime/voicepaste-stt-worker.exe' }) }
        if ($scenario -eq 'private') { $names += 'config.json' }
        $entries = @($names | ForEach-Object {
            $path = Join-Path $payload $_
            [IO.File]::WriteAllText($path, 'fixture')
            @{ path = $_; bytes = (Get-Item $path).Length; sha256 = (Get-FileHash $path).Hash }
        })
        @{ version = '2.0.0'; target = 'x86_64-pc-windows-msvc'; profile = $(if ($scenario -in @('debug', 'valid-debug')) { 'debug' } else { 'release' }); files = $entries } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $payload 'manifest.json')
        if ($scenario -eq 'corrupt') { [IO.File]::WriteAllText((Join-Path $payload 'VoicePaste.exe'), 'changed') }
        $wrapper = Join-Path $work "$scenario-wrapper"
        New-Item -ItemType Directory $wrapper | Out-Null
        $suffix = if ($scenario -eq 'valid-debug') { '-debug' } else { '' }
        Move-Item $payload (Join-Path $wrapper "VoicePaste-2.0.0-windows-x86_64$suffix")
        $archive = Join-Path $work "$scenario.zip"
        [IO.Compression.ZipFile]::CreateFromDirectory($wrapper, $archive)
        $failed = $false
        $profile = if ($scenario -eq 'valid-debug') { 'debug' } else { 'release' }
        try { & (Join-Path $root 'scripts/test-portable-windows.ps1') -ArchivePath $archive -ExpectedProfile $profile } catch { $failed = $true }
        if ($failed -ne ($scenario -notin @('valid', 'valid-debug'))) { throw "Unexpected verification outcome: $scenario" }
        Write-Output "PASS: $scenario"
    }
} finally { Remove-Item -LiteralPath $work -Recurse -Force }
