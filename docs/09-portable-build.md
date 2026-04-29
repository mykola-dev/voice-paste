# VoicePaste Portable Build

VoicePaste portable builds are produced in `build\VoicePaste-Portable` and include:

- Self-contained .NET app
- Embedded Python 3.12 runtime
- Python transcription dependencies
- Transcription worker script
- Default configuration

## Entry Points

Use one of these two commands:

```cmd
build-portable.bat
```

```powershell
PowerShell -ExecutionPolicy Bypass -File build-portable.ps1
```

`build-portable.bat` is only a double-click friendly wrapper. `build-portable.ps1` is the canonical implementation script.

## Incremental Build

Reuse the embedded Python folder and only republish app files:

```powershell
PowerShell -ExecutionPolicy Bypass -File build-portable.ps1 -Incremental
```

`build-portable.bat` automatically uses `-Incremental` when `build\VoicePaste-Portable\python\python.exe` already exists.

## Output

```text
build/VoicePaste-Portable/
├── VoicePaste.exe
├── config.json
├── transcribe/
│   ├── transcribe.py
│   └── requirements-vad.txt
└── python/
    ├── python.exe
    ├── python312.dll
    └── Lib/
```

Run:

```cmd
build\VoicePaste-Portable\VoicePaste.exe
```

## Script API

`build-portable.ps1` supports:

```powershell
param(
    [string]$OutputDir = "build\VoicePaste-Portable",
    [switch]$SkipPython = $false,
    [switch]$Incremental = $false,
    [string]$PythonVersion = "3.12.10",
    [string]$Runtime = "win-x64",
    [string]$DotNetVersion = "8.0.404"
)
```

Examples:

```powershell
PowerShell -ExecutionPolicy Bypass -File build-portable.ps1 -OutputDir build\VoicePaste-Test
```

```powershell
PowerShell -ExecutionPolicy Bypass -File build-portable.ps1 -SkipPython
```

## Deployment

1. Build with `build-portable.bat` or `build-portable.ps1`.
2. Copy the entire `build\VoicePaste-Portable` folder to the target machine.
3. Run `VoicePaste.exe`.

Target machine requirements:

- Windows 10/11 x64
- NVIDIA driver for CUDA acceleration
- No separate .NET or Python installation required

## Notes

- Whisper/faster-whisper model downloads are handled by the app at runtime.
- GPU failures fall back to CPU when configured as CUDA auto-fallback.
- Settings live in `config.json` and can be edited before distribution.
