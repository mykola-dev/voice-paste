# Isolated integration checks: no real process termination, app launches, downloads or publishing.
# Run: powershell -NoProfile -ExecutionPolicy Bypass -File tests/test_build_portable.ps1
$ErrorActionPreference = "Stop"
$buildScript = Join-Path (Split-Path $PSScriptRoot -Parent) "build-portable.ps1"
$tempBase = [IO.Path]::GetTempPath()
$tempRoot = Join-Path $tempBase ("voicepaste-build-tests-" + [Guid]::NewGuid().ToString("N"))
$global:VoicePasteBuildTestState = @{
    Events = [Collections.Generic.List[string]]::new()
    Running = $false; FailPublish = $false; FailStop = $false; MissingPath = $false
    Output = ''; LaunchPath = ''; LaunchDirectory = ''
}

function Assert-True([bool]$condition, [string]$message) {
    if (-not $condition) { throw $message }
}

# These functions shadow external commands only in this test process and child script scopes.
function Get-CimInstance {
    param([string]$ClassName, [string]$Filter)
    Assert-True ($ClassName -eq 'Win32_Process' -and $Filter -eq "Name = 'VoicePaste.exe'") "Only VoicePaste processes should be queried."
    if ($global:VoicePasteBuildTestState.Running) {
        $path = if ($global:VoicePasteBuildTestState.MissingPath) { $null } else { Join-Path $global:VoicePasteBuildTestState.Output 'VoicePaste.exe' }
        [pscustomobject]@{ ProcessId = 12345; ExecutablePath = $path }
    }
    [pscustomobject]@{ ProcessId = 99999; ExecutablePath = 'C:\unrelated\VoicePaste.exe' }
}

function Get-Process {
    param([int]$Id, [string]$ErrorAction)
    Assert-True ($Id -eq 12345) "Must not query/stop an unrelated process."
    # Reproduce the missing Path returned by a 32-bit host querying a 64-bit process.
    $process = [pscustomobject]@{ Id = 12345; Path = $null; HasExited = $false }
    $process | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($timeout) return -not $global:VoicePasteBuildTestState.Running }
    $process
}

function taskkill.exe {
    $global:VoicePasteBuildTestState.Events.Add("stop")
    Assert-True (($args -join ' ') -eq '/PID 12345 /T /F') "Only the target instance and its children may be stopped."
    if ($global:VoicePasteBuildTestState.FailStop) { $global:LASTEXITCODE = 1; return }
    $global:VoicePasteBuildTestState.Running = $false
    $global:LASTEXITCODE = 0
}

function dotnet {
    if ($args[0] -eq '--list-sdks') {
        $global:LASTEXITCODE = 0
        return '8.0.404 [mock SDK]'
    }
    Assert-True ($args[0] -eq 'publish') "Unexpected dotnet invocation."
    Assert-True (-not $global:VoicePasteBuildTestState.Running) "Publish must not begin before the target process exits."
    $global:VoicePasteBuildTestState.Events.Add("publish")
    if ($global:VoicePasteBuildTestState.FailPublish) { $global:LASTEXITCODE = 1; return }
    $stage = $args[[Array]::IndexOf($args, '-o') + 1]
    Set-Content -LiteralPath (Join-Path $stage "VoicePaste.exe") -Value 'new mock binary'
    $global:LASTEXITCODE = 0
}

function Start-Process {
    param([string]$FilePath, [string]$WorkingDirectory)
    Assert-True (-not $global:VoicePasteBuildTestState.Running) "Old process must exit before the replacement starts."
    Assert-True ((Get-Content -LiteralPath $FilePath -Raw).Trim() -eq 'new mock binary') "Must launch the newly published executable."
    Assert-True (Test-Path -LiteralPath (Join-Path $WorkingDirectory 'config.json')) "Config must be installed before launch."
    Assert-True (Test-Path -LiteralPath (Join-Path $WorkingDirectory 'transcribe\transcribe.py')) "Worker must be installed before launch."
    $global:VoicePasteBuildTestState.Events.Add("start")
    $global:VoicePasteBuildTestState.LaunchPath = $FilePath
    $global:VoicePasteBuildTestState.LaunchDirectory = $WorkingDirectory
}

try {
    New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
    foreach ($scenario in @('first-build', 'restart', 'incremental', 'publish-failure', 'stop-failure', 'missing-path', 'no-launch')) {
        $state = $global:VoicePasteBuildTestState
        $state.Output = Join-Path $tempRoot ("portable output " + $scenario)
        $state.Running = $scenario -ne 'first-build'
        $state.FailPublish = $scenario -eq 'publish-failure'
        $state.FailStop = $scenario -eq 'stop-failure'
        $state.MissingPath = $scenario -eq 'missing-path'
        $state.Events.Clear()
        $state.LaunchPath = ""
        New-Item -ItemType Directory -Path $state.Output -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $state.Output 'VoicePaste.exe') -Value 'old mock binary'
        $options = @{ OutputDir = $state.Output; SkipPython = $true }
        if ($scenario -eq 'no-launch') { $options.NoLaunch = $true }
        if ($scenario -eq 'incremental') {
            $options.Incremental = $true
            Set-Content -LiteralPath (Join-Path $state.Output 'keep-python-marker') -Value 'preserve'
        }
        $failed = $false
        try { & $buildScript @options }
        catch {
            if (-not $state.FailPublish -and -not $state.FailStop -and -not $state.MissingPath) { throw }
            if ($state.MissingPath) { Assert-True ($_.Exception.Message -like '*Cannot determine the executable path*') "Missing path must produce a clear error." }
            $failed = $true
        }
        if ($state.MissingPath) {
            Assert-True $failed "An unknown executable path must fail clearly, not silently skip the app."
            Assert-True ($state.Events.Count -eq 0) "Unknown process identity must abort before termination or publishing."
        } elseif ($state.FailPublish) {
            Assert-True $failed "A failed publish must fail the script."
            Assert-True (($state.Events -join ',') -eq 'stop,publish') "A failed publish must never launch a stale app."
        } elseif ($state.FailStop) {
            Assert-True $failed "A failed stop must abort the update."
            Assert-True (($state.Events -join ',') -eq 'stop') "Must not publish or launch after a stop failure."
            Assert-True ((Get-Content -LiteralPath (Join-Path $state.Output 'VoicePaste.exe') -Raw).Trim() -eq 'old mock binary') "Failed stop must leave old files untouched."
        } elseif ($scenario -eq 'no-launch') {
            Assert-True (($state.Events -join ',') -eq 'stop,publish') "-NoLaunch must skip only the final launch."
        } else {
            $expected = if ($scenario -eq 'first-build') { 'publish,start' } else { 'stop,publish,start' }
            Assert-True (($state.Events -join ',') -eq $expected) "Incorrect stop/publish/start order."
            Assert-True ($state.LaunchPath -eq (Join-Path $state.Output 'VoicePaste.exe')) "Wrong executable launched."
            Assert-True ($state.LaunchDirectory -eq $state.Output) "Wrong launch working directory."
            if ($scenario -eq 'incremental') {
                Assert-True (Test-Path -LiteralPath (Join-Path $state.Output 'keep-python-marker')) "Incremental build must preserve existing files."
            }
        }
        Write-Host "PASS: $scenario" -ForegroundColor Green
    }
} finally {
    # Only this test's uniquely named sandbox is removed.
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force }
    Remove-Variable -Name VoicePasteBuildTestState -Scope Global
}
