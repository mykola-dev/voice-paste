# setup-python.ps1
# This script sets up an embedded Python environment for VoicePaste

$PythonVersion = "3.12.10"
$PythonZip = "python-$PythonVersion-embed-amd64.zip"
$PythonUrl = "https://www.python.org/ftp/python/$PythonVersion/$PythonZip"
$InstallDir = Join-Path (Get-Location) "python"

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir
}

Write-Host "Downloading Python $PythonVersion..."
Invoke-WebRequest -Uri $PythonUrl -OutFile (Join-Path $InstallDir $PythonZip)

Write-Host "Extracting Python..."
Expand-Archive -Path (Join-Path $InstallDir $PythonZip) -DestinationPath $InstallDir -Force
Remove-Item (Join-Path $InstallDir $PythonZip)

# Enable site-packages in the embedded distribution
$PthFile = Join-Path $InstallDir "python312._pth"
if (Test-Path $PthFile) {
    $Content = Get-Content $PthFile
    $Content = $Content -replace "#import site", "import site"
    Set-Content -Path $PthFile -Value $Content
}

Write-Host "Downloading get-pip.py..."
Invoke-WebRequest -Uri "https://bootstrap.pypa.io/get-pip.py" -OutFile (Join-Path $InstallDir "get-pip.py")

Write-Host "Installing pip..."
& (Join-Path $InstallDir "python.exe") (Join-Path $InstallDir "get-pip.py") --no-warn-script-location

Write-Host "Installing dependencies (faster-whisper)..."
& (Join-Path $InstallDir "python.exe") -m pip install faster-whisper --no-warn-script-location

Write-Host "Cleaning up..."
Remove-Item (Join-Path $InstallDir "get-pip.py")

Write-Host "Embedded Python setup complete in: $InstallDir"
