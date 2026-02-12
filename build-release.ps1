# VoicePaste Release Build Script
# Produces a portable folder: app/ (published .NET + transcribe worker + config) + python/ (embedded runtime)

param(
    [string]$OutputDir = "build\VoicePaste-Release",
    [switch]$IncludePython = $true,
    [switch]$Incremental = $false,
    [string]$PythonVersion = "3.12.10",
    [string]$Runtime = "win-x64",
    [string]$DotNetVersion = "8.0.404"
)

$ErrorActionPreference = "Stop"

function Write-Step([string]$message) {
    Write-Host $message -ForegroundColor Cyan
}

function Ensure-DotNetSdk {
    $dotnetExe = $null
    
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
    
    Remove-Item $installScriptPath -ErrorAction SilentlyContinue
    
    if (Test-Path $localDotnetExe) {
        Write-Host ".NET SDK installed to: $localDotnetDir" -ForegroundColor Green
        return $localDotnetExe
    }
    
    throw "Failed to install .NET SDK"
}

Write-Step "=== VoicePaste Release Build ==="

$publishDir = Join-Path $OutputDir "_publish"
$pythonDir = Join-Path $OutputDir "python"

if (-not $Incremental -and (Test-Path $OutputDir)) {
    Write-Step "Cleaning output directory..."
    Remove-Item -Recurse -Force $OutputDir
}

# For incremental builds: keep python/ and models/; we republish into staging then sync to output.

if (Test-Path $publishDir) {
    Remove-Item -Recurse -Force $publishDir
}
New-Item -ItemType Directory -Path $publishDir -Force | Out-Null

$dotnetCmd = Ensure-DotNetSdk

Write-Step "Publishing .NET app to staging..."
& $dotnetCmd publish "src/app/VoicePaste.csproj" `
    -c Release `
    -r $Runtime `
    --self-contained true `
    -p:PublishSingleFile=false `
    -p:IncludeNativeLibrariesForSelfExtract=true `
    -o $publishDir

# Ensure staging doesn't ship extra folders
Remove-Item -Recurse -Force (Join-Path $publishDir "python") -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force (Join-Path $publishDir "transcribe") -ErrorAction SilentlyContinue

Write-Step "Syncing published app into output folder..."
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}
Copy-Item -Path (Join-Path $publishDir "*") -Destination $OutputDir -Recurse -Force

Write-Step "Copying transcription worker + config..."
New-Item -ItemType Directory -Path (Join-Path $OutputDir "transcribe") -Force | Out-Null
Copy-Item "src/transcribe/transcribe.py" (Join-Path $OutputDir "transcribe") -Force
Copy-Item "src/transcribe/requirements-vad.txt" (Join-Path $OutputDir "transcribe") -Force
Copy-Item "config/config.json" $OutputDir -Force

if ($IncludePython) {
    $pythonExe = Join-Path $pythonDir "python.exe"

    $needsPython = -not (Test-Path $pythonExe)
    if (-not $needsPython -and -not $Incremental) {
        # In non-incremental mode we always rebuild everything.
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
        Remove-Item $getpipPath

        Write-Step "Installing Python deps (faster-whisper)..."
        & $pythonExe -m pip install -r "src/transcribe/requirements.txt" --no-warn-script-location
    } else {
        Write-Host "Reusing existing embedded Python (incremental)." -ForegroundColor Yellow
    }
}


Write-Step "=== Build Complete ==="
Write-Host "Output: " -ForegroundColor Green
Write-Host "Run: \VoicePaste.exe" -ForegroundColor Green
