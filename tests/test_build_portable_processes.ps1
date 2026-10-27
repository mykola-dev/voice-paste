# Real stop/tree/wait regression checks using disposable renamed cmd.exe fixtures.
# Never builds, launches or stops the user's actual VoicePaste installation.
# Run from 64-bit PowerShell: powershell -NoProfile -ExecutionPolicy Bypass -File tests/test_build_portable_processes.ps1
param([switch]$StopOnly, [string]$TargetExecutable)

$ErrorActionPreference = 'Stop'
if ($StopOnly) {
    $buildScript = Join-Path (Split-Path $PSScriptRoot -Parent) 'build-portable.ps1'
    $parseErrors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($buildScript, [ref]$null, [ref]$parseErrors)
    if ($parseErrors.Count) { throw 'Portable build script failed to parse.' }
    foreach ($name in @('Write-Step', 'Stop-PortableApp')) {
        $definition = $ast.Find({ param($node)
            $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name
        }, $true)
        if (-not $definition) { throw "Function missing from build script: $name" }
        # Execute the real helper, without running the rest of the build/deployment script.
        Invoke-Expression $definition.Extent.Text
    }
    Write-Host "Regression host: 64-bit=$([Environment]::Is64BitProcess)"
    Stop-PortableApp $TargetExecutable
    exit 0
}

if (-not [Environment]::Is64BitProcess) { throw 'Run this regression runner from a 64-bit PowerShell host.' }
$tempBase = [IO.Path]::GetTempPath()
$sandbox = Join-Path $tempBase ('voicepaste-process-tests-' + [Guid]::NewGuid().ToString('N'))
$fixtures = [Collections.Generic.List[Diagnostics.Process]]::new()

function Start-Fixture([string]$folder) {
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    $exe = Join-Path $folder 'VoicePaste.exe'
    Copy-Item -LiteralPath (Join-Path $env:WINDIR 'System32\cmd.exe') -Destination $exe
    $app = Start-Process -FilePath $exe -ArgumentList '/d /c ping.exe -t 127.0.0.1 >nul' -WindowStyle Hidden -PassThru
    $fixtures.Add($app)
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
        $child = Get-CimInstance -ClassName Win32_Process -Filter "ParentProcessId = $($app.Id) AND Name = 'PING.EXE'"
        if ($child) { return @{ App = $app; Child = (Get-Process -Id $child.ProcessId); Exe = $exe } }
        Start-Sleep -Milliseconds 50
    }
    throw 'Disposable app fixture did not create its child process.'
}

try {
    New-Item -ItemType Directory -Path $sandbox -Force | Out-Null
    $unrelated = Start-Fixture (Join-Path $sandbox 'unrelated copy')
    foreach ($hostFolder in @('System32', 'SysWOW64')) {
        $target = Start-Fixture (Join-Path $sandbox "target $hostFolder")
        $hostExe = Join-Path $env:WINDIR "$hostFolder\WindowsPowerShell\v1.0\powershell.exe"
        & $hostExe -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -StopOnly -TargetExecutable $target.Exe
        if ($LASTEXITCODE -ne 0) { throw "Stopping fixture failed in $hostFolder PowerShell." }
        if (-not $target.App.WaitForExit(5000)) { throw 'Target fixture was not terminated.' }
        if (-not $target.Child.WaitForExit(5000)) { throw 'Target fixture child was not terminated.' }
        if ($unrelated.App.HasExited -or $unrelated.Child.HasExited) { throw 'An unrelated fixture was incorrectly terminated.' }
        Write-Host "PASS: $hostFolder host stops the target and child, preserving the unrelated copy." -ForegroundColor Green
    }
} finally {
    foreach ($fixture in $fixtures) {
        if (-not $fixture.HasExited) {
            & taskkill.exe /PID $fixture.Id /T /F | Out-Null
            $fixture.WaitForExit(5000) | Out-Null
        }
        $fixture.Dispose()
    }
    if (Test-Path -LiteralPath $sandbox) { Remove-Item -LiteralPath $sandbox -Recurse -Force }
}
