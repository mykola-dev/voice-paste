# Self-contained structural regression tests; no native inputs/build/app launch.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$work = Join-Path $root ('build/package-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $work -Force | Out-Null
Add-Type -AssemblyName System.IO.Compression.FileSystem
try {
    . (Join-Path $root 'scripts/portable-output.ps1')
    # Both build and package publish through this same boundary.
    foreach ($profile in @('debug', 'release')) {
        $source = Join-Path $work "$profile-source"
        $destination = Join-Path $work "$profile-output"
        New-Item -ItemType Directory $source -Force | Out-Null
        'first binary' | Set-Content (Join-Path $source 'VoicePaste.exe')
        'stale managed file' | Set-Content (Join-Path $source 'old.txt')
        function Write-TestManifest {
            $files = @(Get-ChildItem $source -File | Where-Object Name -ne 'manifest.json' | ForEach-Object {
                @{ path = $_.Name; sha256 = (Get-FileHash $_.FullName).Hash }
            })
            @{ profile = $profile; files = $files } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $source 'manifest.json')
        }
        Write-TestManifest
        # A one-time migration may leave only data/ before first publication.
        New-Item -ItemType Directory (Join-Path $destination 'data') -Force | Out-Null
        'migrated settings' | Set-Content (Join-Path $destination 'data/migrated.txt')
        Publish-PortableFolder $source $destination
        New-Item -ItemType Directory (Join-Path $destination 'data/models/model'), (Join-Path $destination 'data/sessions') -Force | Out-Null
        '{"customInitialPrompt":"personal prompt — Україна"}' | Set-Content (Join-Path $destination 'data/config.json')
        'model weights' | Set-Content (Join-Path $destination 'data/models/model/weights.bin')
        'session' | Set-Content (Join-Path $destination 'data/sessions/recording.wav')
        $before = @(Get-ChildItem (Join-Path $destination 'data') -Recurse -File | Get-FileHash | Select-Object Path, Hash | ConvertTo-Json)
        'second binary' | Set-Content (Join-Path $source 'VoicePaste.exe')
        Remove-Item (Join-Path $source 'old.txt')
        Write-TestManifest
        Publish-PortableFolder $source $destination
        Publish-PortableFolder $source $destination
        $after = @(Get-ChildItem (Join-Path $destination 'data') -Recurse -File | Get-FileHash | Select-Object Path, Hash | ConvertTo-Json)
        if ($before -cne $after) { throw "$profile rebuild changed user data" }
        if (Test-Path (Join-Path $destination 'old.txt')) { throw 'Stale managed file was not removed' }
        if ((Get-Content (Join-Path $destination 'VoicePaste.exe')) -ne 'second binary') { throw 'Binary not updated' }
        foreach ($unsafePath in @('data/config.json', 'DATA/models/file', 'data\config.json', '../outside.txt', 'C:/outside.txt')) {
            @{ files = @(@{ path = $unsafePath; sha256 = 'invalid' }) } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $source 'manifest.json')
            $rejected = $false
            try { Publish-PortableFolder $source $destination } catch { $rejected = $true }
            if (-not $rejected) { throw "Unsafe managed path accepted: $unsafePath" }
        }
        Write-TestManifest
        # Validate before changing any published file, including an old manifest
        # that attempts to treat user settings as stale package content.
        $originalManifest = Get-Content -Raw (Join-Path $destination 'manifest.json')
        @{ files = @(@{ path = 'data/config.json'; sha256 = 'invalid' }) } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $destination 'manifest.json')
        $rejected = $false
        try { Publish-PortableFolder $source $destination } catch { $rejected = $true }
        if (-not $rejected) { throw 'Unsafe old manifest accepted' }
        $originalManifest | Set-Content (Join-Path $destination 'manifest.json')
        'corrupted staging' | Set-Content (Join-Path $source 'VoicePaste.exe')
        $rejected = $false
        try { Publish-PortableFolder $source $destination } catch { $rejected = $true }
        if (-not $rejected) { throw 'Corrupt staging accepted' }
        if ((Get-Content (Join-Path $destination 'VoicePaste.exe')) -ne 'second binary') { throw 'Rejected publication changed the binary' }
        $after = @(Get-ChildItem (Join-Path $destination 'data') -Recurse -File | Get-FileHash | Select-Object Path, Hash | ConvertTo-Json)
        if ($before -cne $after) { throw 'Rejected publication changed user data' }
        Write-Output "PASS: $profile repeated rebuild preserves settings/prompts/models/sessions"
    }
    $nativeFiles = (Import-PowerShellDataFile (Join-Path $root 'scripts/native-runtime.psd1')).Files
    if ($nativeFiles -contains 'cudnn64_9.dll' -or $nativeFiles -notcontains 'msvcp140_1.dll') { throw 'Minimal runtime dependency regression.' }
    foreach ($scenario in @('valid', 'valid-debug', 'corrupt', 'private', 'private-data', 'debug', 'missing-worker', 'missing-ort-crt')) {
        $payload = Join-Path $work $scenario
        New-Item -ItemType Directory (Join-Path $payload 'runtime') -Force | Out-Null
        $names = @('VoicePaste.exe', 'runtime/voicepaste-stt-worker.exe') + @((Import-PowerShellDataFile (Join-Path $root 'scripts/native-runtime.psd1')).Files | ForEach-Object { "runtime/$_" })
        if ($scenario -eq 'missing-worker') { $names = @($names | Where-Object { $_ -ne 'runtime/voicepaste-stt-worker.exe' }) }
        if ($scenario -eq 'missing-ort-crt') { $names = @($names | Where-Object { $_ -ne 'runtime/msvcp140_1.dll' }) }
        if ($scenario -eq 'private') { $names += 'config.json' }
        if ($scenario -eq 'private-data') { $names += 'data/config.json' }
        $entries = @($names | ForEach-Object {
            $path = Join-Path $payload $_
            New-Item -ItemType Directory -Path (Split-Path $path) -Force | Out-Null
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
