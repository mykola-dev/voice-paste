# VoicePaste Portable Build Script
# Stops the target portable instance, updates its files, then launches the new build.

param(
    [string]$OutputDir = "build\VoicePaste-Portable",
    [switch]$SkipPython = $false,
    [switch]$Incremental = $false,
    [switch]$NoLaunch = $false,
    [string]$PythonVersion = "3.12.10",
    [string]$Runtime = "win-x64",
    [string]$DotNetVersion = "8.0.404"
)

$ErrorActionPreference = "Stop"

function Write-Step([string]$message) {
    Write-Host $message -ForegroundColor Cyan
}

function Stop-PortableApp([string]$appPath) {
    $matched = $false
    # Get-Process.Path is empty for a 64-bit app queried from 32-bit Windows PowerShell.
    # CIM returns ExecutablePath independently of the script host's bitness.
    foreach ($instance in @(Get-CimInstance -ClassName Win32_Process -Filter "Name = 'VoicePaste.exe'")) {
        if ([string]::IsNullOrWhiteSpace($instance.ExecutablePath)) {
            throw "Cannot determine the executable path for VoicePaste PID $($instance.ProcessId). Close it manually or run this script with the same administrator permissions as the app."
        }
        # Do not terminate unrelated debug builds or other portable installations.
        if (-not [string]::Equals($instance.ExecutablePath, $appPath, [StringComparison]::OrdinalIgnoreCase)) {
            Write-Host "Leaving another VoicePaste instance (PID $($instance.ProcessId)): $($instance.ExecutablePath)" -ForegroundColor DarkGray
            continue
        }
        $process = Get-Process -Id $instance.ProcessId -ErrorAction SilentlyContinue
        if (-not $process) { continue } # It may have exited since the CIM snapshot.
        $matched = $true
        Write-Step "Stopping VoicePaste (PID $($process.Id)) and its worker processes..."
        & taskkill.exe /PID $process.Id /T /F | Out-Host
        if ($LASTEXITCODE -ne 0 -and -not $process.HasExited) {
            throw "Could not stop VoicePaste (PID $($process.Id)). Close it and retry."
        }
        if (-not $process.WaitForExit(10000)) {
            throw "VoicePaste did not exit before the update."
        }
    }
    if (-not $matched) { Write-Step "No running VoicePaste found at: $appPath" }
}

function Ensure-DotNetSdk {
    $systemDotnet = Get-Command dotnet -ErrorAction SilentlyContinue
    if ($systemDotnet) {
        $sdkCheck = & dotnet --list-sdks 2>$null
        if ($sdkCheck) {
            Write-Host "Using system .NET SDK" -ForegroundColor Green
            return "dotnet"
        }
    }

    $localDotnetDir = Join-Path $PSScriptRoot "build\dotnet"
    $localDotnetExe = Join-Path $localDotnetDir "dotnet.exe"

    if (Test-Path $localDotnetExe) {
        Write-Host "Using local .NET SDK: $localDotnetDir" -ForegroundColor Green
        return $localDotnetExe
    }

    Write-Step "Downloading .NET SDK $DotNetVersion (no SDK found on system)..."

    $installScriptUrl = "https://dot.net/v1/dotnet-install.ps1"
    $installScriptPath = Join-Path $env:TEMP "dotnet-install.ps1"

    Invoke-WebRequest -Uri $installScriptUrl -OutFile $installScriptPath -UseBasicParsing

    if (-not (Test-Path $localDotnetDir)) {
        New-Item -ItemType Directory -Path $localDotnetDir -Force | Out-Null
    }

    & powershell -NoProfile -ExecutionPolicy Bypass -File $installScriptPath `
        -Channel 8.0 `
        -Version $DotNetVersion `
        -InstallDir $localDotnetDir `
        -NoPath
    if ($LASTEXITCODE -ne 0) { throw ".NET SDK installation failed (exit code $LASTEXITCODE)." }

    Remove-Item $installScriptPath -ErrorAction SilentlyContinue

    if (Test-Path $localDotnetExe) {
        Write-Host ".NET SDK installed to: $localDotnetDir" -ForegroundColor Green
        return $localDotnetExe
    }

    throw "Failed to install .NET SDK"
}

Write-Step "=== VoicePaste Portable Build ==="

if (-not [IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $PSScriptRoot $OutputDir
}
$OutputDir = [IO.Path]::GetFullPath($OutputDir).TrimEnd('\', '/')
if ([string]::Equals($OutputDir, [IO.Path]::GetPathRoot($OutputDir).TrimEnd('\', '/'), [StringComparison]::OrdinalIgnoreCase) -or
    $PSScriptRoot.StartsWith($OutputDir + '\', [StringComparison]::OrdinalIgnoreCase) -or
    [string]::Equals($OutputDir, $PSScriptRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "OutputDir must not be the repository directory or one of its parents."
}
$appExe = Join-Path $OutputDir "VoicePaste.exe"
$publishDir = Join-Path $OutputDir "_publish"
$pythonDir = Join-Path $OutputDir "python"

Stop-PortableApp $appExe

if (-not $Incremental -and (Test-Path $OutputDir)) {
    Write-Step "Cleaning output directory..."
    Remove-Item -Recurse -Force $OutputDir
}

if (Test-Path $publishDir) {
    Remove-Item -Recurse -Force $publishDir
}
New-Item -ItemType Directory -Path $publishDir -Force | Out-Null

$dotnetCmd = Ensure-DotNetSdk

Write-Step "Publishing .NET app to staging..."
& $dotnetCmd publish (Join-Path $PSScriptRoot "src/app/VoicePaste.csproj") `
    -c Release `
    -r $Runtime `
    --self-contained true `
    -p:PublishSingleFile=false `
    -p:IncludeNativeLibrariesForSelfExtract=true `
    -o $publishDir
if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed (exit code $LASTEXITCODE). The app was not restarted." }

Remove-Item -Recurse -Force (Join-Path $publishDir "python") -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force (Join-Path $publishDir "transcribe") -ErrorAction SilentlyContinue

Write-Step "Syncing published app into output folder..."
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}
Copy-Item -Path (Join-Path $publishDir "*") -Destination $OutputDir -Recurse -Force

Write-Step "Copying transcription worker + config..."
New-Item -ItemType Directory -Path (Join-Path $OutputDir "transcribe") -Force | Out-Null
Copy-Item (Join-Path $PSScriptRoot "src/transcribe/transcribe.py") (Join-Path $OutputDir "transcribe") -Force
Copy-Item (Join-Path $PSScriptRoot "src/transcribe/requirements-vad.txt") (Join-Path $OutputDir "transcribe") -Force
Copy-Item (Join-Path $PSScriptRoot "config/config.json") $OutputDir -Force

if (-not $SkipPython) {
    $pythonExe = Join-Path $pythonDir "python.exe"

    $needsPython = -not (Test-Path $pythonExe)
    if (-not $needsPython -and -not $Incremental) {
        $needsPython = $true
    }

    if ($needsPython) {
        Write-Step "Downloading Python embeddable..."
        $pythonUrl = "https://www.python.org/ftp/python/$PythonVersion/python-$PythonVersion-embed-amd64.zip"
        $pythonZip = Join-Path $env:TEMP "voicepaste-python-embed.zip"

        if (Test-Path $pythonDir) {
            Remove-Item -Recurse -Force $pythonDir
        }

        Invoke-WebRequest -Uri $pythonUrl -OutFile $pythonZip
        Expand-Archive -Path $pythonZip -DestinationPath $pythonDir -Force
        Remove-Item $pythonZip

        Write-Step "Enabling site-packages for embedded Python..."
        $pthFile = Get-ChildItem $pythonDir -Filter "*._pth" | Select-Object -First 1
        if ($pthFile) {
            $content = Get-Content $pthFile.FullName
            $content = $content -replace '#import site', 'import site'
            $content | Set-Content $pthFile.FullName
        }

        Write-Step "Installing pip..."
        $getpipUrl = "https://bootstrap.pypa.io/get-pip.py"
        $getpipPath = Join-Path $pythonDir "get-pip.py"
        Invoke-WebRequest -Uri $getpipUrl -OutFile $getpipPath
        & $pythonExe $getpipPath --no-warn-script-location
        if ($LASTEXITCODE -ne 0) { throw "pip setup failed (exit code $LASTEXITCODE). The app was not restarted." }
        Remove-Item $getpipPath

        Write-Step "Installing Python deps..."
        & $pythonExe -m pip install -r (Join-Path $PSScriptRoot "src/transcribe/requirements.txt") --no-warn-script-location
        if ($LASTEXITCODE -ne 0) { throw "Python dependency installation failed (exit code $LASTEXITCODE). The app was not restarted." }
    } else {
        Write-Host "Reusing existing embedded Python (incremental)." -ForegroundColor Yellow
    }
}

Write-Step "=== Build Complete ==="
Write-Host "Output: $OutputDir" -ForegroundColor Green
Write-Host "Run: $OutputDir\VoicePaste.exe" -ForegroundColor Green

if (-not $NoLaunch) {
    if (-not (Test-Path -LiteralPath $appExe -PathType Leaf)) {
        throw "Published executable not found: $appExe"
    }
    Write-Step "Starting updated VoicePaste..."
    Start-Process -FilePath $appExe -WorkingDirectory $OutputDir
}
