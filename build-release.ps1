# VoicePaste Release Build Script
# Produces a portable folder: app/ (published .NET + transcribe worker + config) + python/ (embedded runtime)

param(
    [string]$OutputDir = "build\VoicePaste-Release",
    [switch]$IncludePython = $true,
    [switch]$Incremental = $false,
    [string]$PythonVersion = "3.12.10",
    [string]$Runtime = "win-x64"
)

$ErrorActionPreference = "Stop"

function Write-Step([string]$message) {
    Write-Host $message -ForegroundColor Cyan
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

Write-Step "Publishing .NET app to staging..."
dotnet publish "src/app/VoicePaste.csproj" `
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