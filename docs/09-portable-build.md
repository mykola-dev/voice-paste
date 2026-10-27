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

Before updating files, the script stops the running instance from the exact output folder and its
child workers. It then builds/updates that folder and launches the new `VoicePaste.exe`. Other
installations are reported and left running. Use `-NoLaunch` to skip the final launch.

Process paths are resolved through CIM so that a 32-bit PowerShell host can still identify the
64-bit application. The batch wrapper selects 64-bit Windows PowerShell, including when launched
from a 32-bit terminal. If a process path is inaccessible (for example due to administrator
permissions), the script stops with an explanation rather than silently leaving an unknown instance
running. Close that instance manually or run the script with matching permissions.

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
    [switch]$NoLaunch = $false,
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

## Script regression checks

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tests/test_build_portable.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tests/test_build_portable_processes.ps1
```

The first checks deployment ordering and failure handling with mocked commands. The second uses
temporary renamed `cmd.exe` fixtures and real process termination from both 64-bit and 32-bit
PowerShell hosts, verifying that children stop and an unrelated copy stays running. It does not
build or stop the user's actual VoicePaste installation. Run the second test from 64-bit PowerShell.

## Notes

- Whisper/faster-whisper model downloads are handled by the app at runtime.
- GPU failures fall back to CPU when configured as CUDA auto-fallback.
- Settings live in `config.json` and can be edited before distribution.
