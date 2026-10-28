# Universal Windows build/check/package entry point. Never stops or launches apps.
[CmdletBinding(SupportsShouldProcess)]
param(
    [ValidateSet('build', 'package', 'test', 'fmt', 'clippy', 'fetch')][string]$Action = 'build',
    [switch]$Release,
    [switch]$NoDeploy,
    [string]$ModelSource,
    [string]$NativeRuntimeSource,
    [string]$Ct2Include,
    [switch]$RequireNotices
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
function Invoke-PortablePackage {
    if (-not $IsWindows -or -not [Environment]::Is64BitProcess) { throw 'PowerShell 7 x64 on Windows is required.' }
    $versionMatch = [regex]::Match((Get-Content (Join-Path $root 'Cargo.toml') -Raw), '(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"')
    if (-not $versionMatch.Success) { throw 'Workspace version missing.' }
    $version = $versionMatch.Groups[1].Value
    $profile = if ($Release) { 'release' } else { 'debug' }
    $suffix = if ($Release) { '' } else { '-debug' }
    $name = "VoicePaste-$version-windows-x86_64$suffix"
    $output = Join-Path $root "build/$profile"
    $destination = Join-Path $output $name
    $archive = Join-Path $output "$name.zip"
    $checksum = "$archive.sha256"
    foreach ($path in @($destination, $archive, $checksum)) {
        if (Test-Path -LiteralPath $path) { throw "Output exists; move the previous package elsewhere first: $path" }
    }
    $nativeSource = if ($NativeRuntimeSource) { [IO.Path]::GetFullPath($NativeRuntimeSource) } else { Join-Path $root 'build/native-runtime' }
    $files = (Import-PowerShellDataFile (Join-Path $PSScriptRoot 'native-runtime.psd1')).Files
    foreach ($file in $files) {
        if (-not (Test-Path -LiteralPath (Join-Path $nativeSource $file) -PathType Leaf)) { throw "Missing native input: $file" }
    }
    $notices = Join-Path $nativeSource 'notices'
    if ($RequireNotices) {
        & (Join-Path $PSScriptRoot 'test-native-inputs.ps1') -NativeRuntimeSource $nativeSource
    }
    if (-not $PSCmdlet.ShouldProcess($destination, "Build $profile host/worker, stage and ZIP portable package")) { return }
    # Reuse this entry point's compilation/toolchain path, without dev deployment.
    & $PSCommandPath -Action build -Release:$Release -NoDeploy -NativeRuntimeSource $nativeSource -Ct2Include $Ct2Include
    $stage = Join-Path $output ('.stage-' + [guid]::NewGuid().ToString('N'))
    $payload = Join-Path $stage $name
    $stagedZip = Join-Path $stage "$name.zip"
    $published = @()
    try {
        New-Item -ItemType Directory -Path (Join-Path $payload 'runtime'), (Join-Path $payload 'notices/ui') -Force | Out-Null
        Copy-Item (Join-Path $root "build/voicepaste-rust/target/$profile/voicepaste.exe") (Join-Path $payload 'VoicePaste.exe')
        Copy-Item (Join-Path $root "build/voicepaste-rust/target/$profile/voicepaste-stt-worker.exe") (Join-Path $payload 'runtime')
        foreach ($file in $files) { Copy-Item -LiteralPath (Join-Path $nativeSource $file) -Destination (Join-Path $payload 'runtime') }
        Copy-Item (Join-Path $root 'README.md') $payload
        New-Item -ItemType Directory (Join-Path $payload 'apps/voicepaste/assets') -Force | Out-Null
        Copy-Item (Join-Path $root 'apps/voicepaste/assets/icon.png') (Join-Path $payload 'apps/voicepaste/assets/icon.png')
        Copy-Item (Join-Path $root 'apps/voicepaste/assets/README.md') (Join-Path $payload 'notices/ui')
        Copy-Item (Join-Path $root 'apps/voicepaste/assets/tray/README.md') (Join-Path $payload 'notices/ui/Tray-Artwork-Provenance.md')
        Copy-Item (Join-Path $root 'apps/voicepaste/assets/one/LICENSE') (Join-Path $payload 'notices/ui/One-Dark-MIT.txt')
        Copy-Item (Join-Path $root 'apps/voicepaste/assets/fonts/OFL.txt') (Join-Path $payload 'notices/ui/IBM-Plex-Sans-OFL.txt')
        $cargoHome = if (Test-Path (Join-Path $root 'build/rust-migration-tools/cargo')) { Join-Path $root 'build/rust-migration-tools/cargo' } elseif ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME '.cargo' }
        $lucide = @(Get-ChildItem "$cargoHome/registry/src/*/gpui-kit-assets-0.8.1/LICENSE-LUCIDE" -File)
        if ($lucide.Count -ne 1) { throw 'Pinned Lucide license missing or ambiguous.' }
        Copy-Item $lucide[0].FullName (Join-Path $payload 'notices/ui/Lucide.txt')
        if (Test-Path $notices) { Copy-Item $notices (Join-Path $payload 'notices/native') -Recurse }
        else { 'LOCAL EVALUATION ONLY: native license inventory and redistribution qualification are incomplete.' | Set-Content (Join-Path $payload 'notices/NATIVE-NOTICES-PENDING.txt') }
        $entries = @(Get-ChildItem $payload -Recurse -File | Sort-Object FullName | ForEach-Object {
            if ($_.Length -eq 0) { throw "Empty package artifact: $($_.Name)" }
            @{ path = [IO.Path]::GetRelativePath($payload, $_.FullName).Replace('\', '/'); bytes = $_.Length; sha256 = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
        })
        @{ version = $version; target = 'x86_64-pc-windows-msvc'; profile = $profile; files = $entries } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $payload 'manifest.json') -Encoding utf8
        foreach ($entry in $entries) {
            if ((Get-FileHash (Join-Path $payload $entry.path)).Hash -ne $entry.sha256) { throw "Staging hash mismatch: $($entry.path)" }
        }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [IO.Compression.ZipFile]::CreateFromDirectory($payload, $stagedZip, [IO.Compression.CompressionLevel]::Optimal, $true)
        & (Join-Path $PSScriptRoot 'test-portable-windows.ps1') -ArchivePath $stagedZip -ExpectedProfile $profile
        "$( (Get-FileHash $stagedZip).Hash.ToLowerInvariant() )  $name.zip" | Set-Content (Join-Path $stage "$name.zip.sha256") -Encoding ascii
        foreach ($pair in @(@($payload, $destination), @($stagedZip, $archive), @((Join-Path $stage "$name.zip.sha256"), $checksum))) {
            Move-Item -LiteralPath $pair[0] -Destination $pair[1]
            $published += $pair[1]
        }
        Write-Output "Portable $profile package: $destination"
        Write-Output "Archive: $archive"
    } catch {
        foreach ($path in $published) { Move-Item -LiteralPath $path -Destination $stage }
        throw
    } finally {
        if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    }
}
if ($Action -eq 'package') {
    if ($ModelSource -or $NoDeploy) { throw 'Package excludes user models and never deploys; omit ModelSource/NoDeploy.' }
    Invoke-PortablePackage
    return
}
if ($RequireNotices) { throw 'RequireNotices applies only to -Action package.' }
if (-not $PSCmdlet.ShouldProcess($root, "Rust $Action")) { return }
$saved = @{}
Get-ChildItem Env: | ForEach-Object { $saved[$_.Name] = $_.Value }
try {
    $localTools = Join-Path $root 'build/rust-migration-tools'
    $cargo = Join-Path $localTools 'cargo/bin/cargo.exe'
    if (Test-Path -LiteralPath $cargo) {
        $env:CARGO_HOME = Join-Path $localTools 'cargo'
        $env:RUSTUP_HOME = Join-Path $localTools 'rustup'
    } else { $cargo = (Get-Command cargo.exe -ErrorAction Stop).Source }
    $env:RUSTUP_TOOLCHAIN = '1.98.1'
    $env:CARGO_TARGET_DIR = Join-Path $root 'build/voicepaste-rust/target'
    $destination = Join-Path $root 'build/VoicePaste-Rust'
    $runtime = Join-Path $destination 'runtime'
    if (-not $NativeRuntimeSource) { $NativeRuntimeSource = Join-Path $root 'build/native-runtime' }
    if ($Action -notin @('fmt', 'fetch')) {
        $vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
        $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'Existing MSVC C++ tools required.' }
        Import-Module (Join-Path $vs 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')
        Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' | Out-Null
        $env:VOICEPASTE_CT2_INCLUDE = if ($Ct2Include) { [IO.Path]::GetFullPath($Ct2Include) } else { Join-Path $localTools 'native-gpu/CTranslate2-226c95d94e660c48b11c62e108886b7ef76d589d/include' }
        $env:VOICEPASTE_CT2_LIB = Join-Path $root 'build/voicepaste-rust/native'
        if (-not (Test-Path -LiteralPath $env:VOICEPASTE_CT2_INCLUDE)) { throw 'Matching CT2 4.7.1 headers required; see apps/stt-worker/README.md.' }
        New-Item -ItemType Directory -Path $env:VOICEPASTE_CT2_LIB -Force | Out-Null
        # Explicit flat native input: no Python installation/package discovery or
        # dependency on the removed WPF portable tree. Validate before any copies.
        $nativeFiles = (Import-PowerShellDataFile (Join-Path $PSScriptRoot 'native-runtime.psd1')).Files
        foreach ($name in $nativeFiles) {
            if (-not (Test-Path -LiteralPath (Join-Path $NativeRuntimeSource $name) -PathType Leaf)) {
                throw "Missing native input: $NativeRuntimeSource/$name. See apps/stt-worker/README.md. Nothing downloaded."
            }
        }
        $exports = & dumpbin.exe /nologo /exports (Join-Path $NativeRuntimeSource 'ctranslate2.dll')
        if ($LASTEXITCODE -ne 0) { throw 'CT2 exports inspection failed.' }
        $symbols = foreach ($line in $exports) { if ($line -match '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]+\s+(\S+)') { $Matches[1] } }
        if (-not $symbols -or $symbols.Count -lt 100) { throw 'Unexpected CT2 export table.' }
        $definition = Join-Path $env:VOICEPASTE_CT2_LIB 'ctranslate2.def'
        @('LIBRARY ctranslate2.dll', 'EXPORTS') + $symbols | Set-Content -LiteralPath $definition -Encoding ascii
        & lib.exe /nologo /machine:x64 "/def:$definition" "/out:$(Join-Path $env:VOICEPASTE_CT2_LIB 'ctranslate2.lib')"
        if ($LASTEXITCODE -ne 0) { throw 'CT2 import-library generation failed.' }
        function Copy-Runtime([string]$Source) {
            $target = Join-Path $runtime ([IO.Path]::GetFileName($Source))
            if (-not (Test-Path -LiteralPath $target) -or (Get-FileHash -LiteralPath $target).Hash -ne (Get-FileHash -LiteralPath $Source).Hash) {
                Copy-Item -LiteralPath $Source -Destination $target
            }
        }
        if ($Action -eq 'build' -and -not $NoDeploy) {
            New-Item -ItemType Directory -Path $runtime -Force | Out-Null
            foreach ($name in $nativeFiles) { Copy-Runtime (Join-Path $NativeRuntimeSource $name) }
        }
        $env:PATH = "$NativeRuntimeSource;$env:PATH"
    }
    $arguments = @($Action, '--manifest-path', (Join-Path $root 'Cargo.toml'))
    if ($Action -eq 'fmt') { $arguments += @('--all', '--', '--check') }
    elseif ($Action -eq 'fetch') {
        if (Test-Path -LiteralPath (Join-Path $root 'Cargo.lock')) { $arguments += '--locked' }
    } else {
        $arguments += @('--workspace', '--locked', '--offline', '--jobs', '2')
        if ($Release) { $arguments += '--release' }
        if ($Action -eq 'clippy') { $arguments += @('--all-targets', '--', '-D', 'warnings') }
    }
    & $cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw "Rust $Action failed ($LASTEXITCODE)." }
    if ($Action -eq 'build' -and -not $NoDeploy) {
        $profile = if ($Release) { 'release' } else { 'debug' }
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$profile/voicepaste.exe") -Destination (Join-Path $destination 'VoicePaste.exe')
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$profile/voicepaste-stt-worker.exe") -Destination $runtime
        $licenses = Join-Path $destination 'package/ui-assets'
        New-Item -ItemType Directory -Path $licenses -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $root 'apps/voicepaste/assets/README.md') -Destination $licenses
        Copy-Item -LiteralPath (Join-Path $root 'apps/voicepaste/assets/tray/README.md') -Destination (Join-Path $licenses 'Tray-Artwork-Provenance.md')
        Copy-Item -LiteralPath (Join-Path $root 'apps/voicepaste/assets/one/LICENSE') -Destination (Join-Path $licenses 'One-Dark-MIT.txt')
        Copy-Item -LiteralPath (Join-Path $root 'apps/voicepaste/assets/fonts/OFL.txt') -Destination (Join-Path $licenses 'IBM-Plex-Sans-OFL.txt')
        $registry = Join-Path $env:CARGO_HOME 'registry/src'
        $lucide = @(Get-ChildItem -Path "$registry/*/gpui-kit-assets-0.8.1/LICENSE-LUCIDE" -File)
        if ($lucide.Count -ne 1) { throw 'Pinned Lucide license not found.' }
        Copy-Item -LiteralPath $lucide[0].FullName -Destination (Join-Path $licenses 'Lucide.txt')
        if ($ModelSource) {
            $manifest = Get-Content -Raw -LiteralPath (Join-Path $root 'crates/core/assets/model-artifacts.json') | ConvertFrom-Json
            $model = $manifest.models | Where-Object name -eq 'large-v2'
            $modelTarget = Join-Path $destination "data/models/$($model.name)/$($model.revision)"
            New-Item -ItemType Directory -Path $modelTarget -Force | Out-Null
            foreach ($file in $model.files) {
                $source = Join-Path $ModelSource $file.path
                if ((Get-Item -LiteralPath $source).Length -ne $file.bytes -or (Get-FileHash -LiteralPath $source).Hash -ne $file.sha256) { throw "Model source mismatch: $($file.path)" }
                $target = Join-Path $modelTarget $file.path
                if (Test-Path -LiteralPath $target) {
                    if ((Get-FileHash -LiteralPath $target).Hash -eq $file.sha256) { continue }
                    throw "Existing model artifact differs; not overwriting: $target"
                }
                $partial = "$target.copy-$PID"
                Copy-Item -LiteralPath $source -Destination $partial
                if ((Get-FileHash -LiteralPath $partial).Hash -ne $file.sha256) { throw 'Model copy verification failed.' }
                Move-Item -LiteralPath $partial -Destination $target
            }
        }
        Get-ChildItem -LiteralPath $runtime -File | ForEach-Object { [PSCustomObject]@{ name=$_.Name; bytes=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName).Hash } } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination 'runtime-manifest.json')
        "Built host + resident native worker: $destination. Not launched; no models downloaded."
    }
} finally {
    foreach ($item in @(Get-ChildItem Env:)) {
        if (-not $saved.ContainsKey($item.Name)) { Remove-Item -LiteralPath "Env:$($item.Name)" }
    }
    foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
}
