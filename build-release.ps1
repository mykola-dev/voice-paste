# VoicePaste Release Build Script
# Produces a portable folder: app/ (published .NET + transcribe worker + config) + python/ (embedded runtime)

param(
    [string]$OutputDir = "build\VoicePaste-Release",
    [switch]$IncludePython = $true,
    [switch]$IncludeModels = $false,
    [switch]$DownloadDefaultModel = $false,
    [switch]$Incremental = $false,
    [string]$PythonVersion = "3.12.10",
    [string]$Runtime = "win-x64",
    [string]$DefaultModel = "large-v3-turbo",
    [string[]]$AdditionalModels = @(),
    [switch]$CacheLargeModels = $false
)

$ErrorActionPreference = "Stop"

function Write-Step([string]$message) {
    Write-Host $message -ForegroundColor Cyan
}

Write-Step "=== VoicePaste Release Build ==="

$publishDir = Join-Path $OutputDir "_publish"
$pythonDir = Join-Path $OutputDir "python"
$modelsDir = Join-Path $OutputDir "models"

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
Remove-Item -Recurse -Force (Join-Path $publishDir "models") -ErrorAction SilentlyContinue
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

function Cache-Models([string[]]$modelsToCache) {
    if ($modelsToCache.Count -eq 0) {
        return
    }

    if (-not (Test-Path $modelsDir)) {
        New-Item -ItemType Directory -Path $modelsDir -Force | Out-Null
    }

    $pythonExe = Join-Path $pythonDir "python.exe"
    if (-not (Test-Path $pythonExe)) {
        throw "Embedded Python not found at $pythonExe. Build with -IncludePython first."
    }

    $quotedModels = $modelsToCache | ForEach-Object { "'$($_)'" }
    $modelListLiteral = "[" + ($quotedModels -join ", ") + "]"

    $script = @"
import os
import sys

models = ${modelListLiteral}

# Force HF cache to the bundled folder
os.environ['HF_HOME'] = os.path.abspath(r'$modelsDir')
os.environ['HF_HUB_DISABLE_TELEMETRY'] = '1'

try:
    from faster_whisper import WhisperModel
except Exception as e:
    print(f\"[VoicePaste] ERROR importing faster_whisper: {e}\", file=sys.stderr)
    raise

for model in models:
    print(f\"[VoicePaste] Caching model: {model}\")
    try:
        WhisperModel(model, device='cpu')
    except Exception as e:
        print(f\"[VoicePaste] WARNING: Failed to cache {model}: {e}\", file=sys.stderr)

print(\"[VoicePaste] Model caching complete.\")
"@

    & $pythonExe -c $script
}

if ($DownloadDefaultModel) {
    Write-Step "Caching default model ($DefaultModel) into bundled models/..."
    Cache-Models @($DefaultModel)
}

if ($CacheLargeModels) {
    $AdditionalModels = @($AdditionalModels + @("large-v3", "large-v2") | Select-Object -Unique)
}

if ($AdditionalModels.Count -gt 0) {
    Write-Step "Caching additional models: $($AdditionalModels -join ', ')"
    Cache-Models $AdditionalModels
}

if ($IncludeModels) {
    if ($Incremental -and (Test-Path $modelsDir)) {
        Write-Host "Reusing existing bundled models (incremental)." -ForegroundColor Yellow
    } else {
        Write-Step "Copying cached models from user HF cache (if present)..."
        $modelCache = "$env:USERPROFILE\\.cache\\huggingface\\hub"
        if (Test-Path $modelCache) {
            if (-not (Test-Path $modelsDir)) {
                New-Item -ItemType Directory -Path $modelsDir -Force | Out-Null
            }
            Copy-Item -Recurse "$modelCache\\*faster-whisper*" $modelsDir -Force
            Write-Host "HF cache copied into bundled models/." -ForegroundColor Yellow
        } else {
            Write-Host "No HuggingFace cache found; skipping extra models." -ForegroundColor Yellow
        }
    }
}

Write-Step "=== Build Complete ==="
Write-Host "Output: $OutputDir" -ForegroundColor Green
Write-Host "Run: $OutputDir\VoicePaste.exe" -ForegroundColor Green
