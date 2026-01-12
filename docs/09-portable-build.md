# VoicePaste - Portable Build Guide

## Overview

VoicePaste can be built as a fully portable package that includes:
- Self-contained .NET runtime (no .NET installation needed)
- Embedded Python 3.12 runtime
- faster-whisper with all dependencies
- Transcription worker script
- Configuration files

## Building Portable Package

### Recommended (2 scripts)

- Debug build + run: `dev-debug.bat`
- Portable release (smart): `release-portable.bat`

### Details

**Fully portable local-folder release (app + embedded Python + deps):**

```powershell
PowerShell -ExecutionPolicy Bypass -File build-release.ps1
```

Incremental (republish app EXE/files, reuse python/models):

```powershell
PowerShell -ExecutionPolicy Bypass -File build-release.ps1 -Incremental
```

Double-click `release-portable.bat`.

It automatically:
- First run: builds everything (downloads embedded Python + installs deps)
- Caches the default model (`large-v3-turbo`) into `models/`
- Optionally cache more models via `build-release.ps1 -AdditionalModels ...`
- Next runs: republish app files and reuse `python/` / `models/` when present

Run it with:

```cmd
build\VoicePaste-Release\VoicePaste.exe
```

### Option 2: Legacy Portable Script (Deprecated)

```powershell
PowerShell -ExecutionPolicy Bypass -File build-release.ps1
```

**Include pre-downloaded Whisper models:**

```powershell
PowerShell -ExecutionPolicy Bypass -File build-release.ps1 -IncludeModels
```

This will:
- Build self-contained .NET app
- Download Python 3.12 embeddable
- Install pip and faster-whisper
- Create launcher script
- Copy all necessary files
- Optionally include cached models

### Option 3: Batch Script (Basic)

(Legacy scripts are moved to `_old-scripts/`.)

Deprecated wrappers still exist for backward compatibility:

```cmd
build-portable.bat
```

```powershell
PowerShell -ExecutionPolicy Bypass -File build-portable.ps1
```

## Output Structure

```
build/VoicePaste-Release/
├── VoicePaste.exe          # Self-contained executable (run this)
├── config.json             # Configuration
├── transcribe/
│   └── transcribe.py       # Transcription worker
├── python/                 # Embedded Python runtime
│   ├── python.exe
│   ├── python312.dll
│   ├── Lib/
│   └── [faster-whisper packages]
└── models/                 # Bundled model cache (default: large-v3-turbo)
```

build/VoicePaste-Release/
├── VoicePaste.bat          # Launcher (use this to run)
├── app/                    # Main application
│   ├── VoicePaste.exe      # Self-contained executable
│   ├── config.json         # Configuration
│   ├── transcribe/
│   │   └── transcribe.py   # Transcription worker
│   └── [.NET runtime files]
├── python/                 # Embedded Python (build-release.ps1)
│   ├── python.exe
│   ├── python312.dll
│   ├── Lib/
│   └── [faster-whisper packages]
└── models/                 # Optional bundled model cache


## Portable Build Features

✅ **No Installation Required**
- All dependencies included
- Copy folder and run
- Works from USB drive

✅ **Self-Contained .NET**
- No .NET SDK/Runtime needed on target PC
- All native libraries included

✅ **Embedded Python**
- Python 3.12 embeddable package
- All packages included in bundle
- No Python installation needed

✅ **Configuration Preserved**
- Settings stored in app folder
- Models cached locally (optional)

## Deployment to Another PC

### Minimum Transfer (500MB)

1. Build with PowerShell script
2. Copy entire `VoicePaste-Portable` folder
3. Run `VoicePaste.bat` on target PC
4. First run will download Whisper model (~3GB)

### Full Transfer with Models (3.5GB)

1. Build with `-IncludeModels` flag
2. Copy entire folder including models
3. Models are pre-cached, no download needed

### Requirements on Target PC

- Windows 10/11 (x64)
- For GPU: NVIDIA drivers with CUDA support
  - Download: https://developer.nvidia.com/cuda-downloads
  - The app auto-detects GPU and falls back to CPU if unavailable

## Size Breakdown

| Component | Size |
|-----------|------|
| .NET app with runtime | ~150MB |
| Python embeddable | ~25MB |
| faster-whisper + dependencies | ~300MB |
| **Base portable build** | **~500MB** |
| Whisper large-v3 model | ~3GB |
| **With models** | **~3.5GB** |

## Configuration

Edit `app/config.json` before deployment:

```json
{
  "hotkey": "ScrollLock",
  "pasteShortcut": "Ctrl+Shift+V",
  "model": "large-v3",
  "device": "cuda",
  "restoreClipboard": true,
  "clipboardRestoreDelayMs": 400
}
```

## Customizing the Build

### Cache additional models into the portable bundle

```powershell
PowerShell -ExecutionPolicy Bypass -File build-release.ps1 -Incremental -AdditionalModels large-v3,large-v2
```

### Use Smaller Model

Edit `app/config.json` after build:
- `large-v3` - Best accuracy (~3GB)
- `medium` - Good balance (~1.5GB)
- `small` - Faster, less accurate (~500MB)
- `base` - Fastest (~150MB)

### CPU-Only Build

Set in `config.json`:
```json
"device": "cpu"
```

No CUDA drivers needed on target PC.

## Updating the Portable Build

To update after code changes:

1. Make changes to source code
2. Re-run build script
3. Copy new `app/` folder over existing
4. Keep `python/` folder (no need to rebuild)
5. Keep models cache (if exists)

## Troubleshooting

### Build Script Issues

**PowerShell execution policy:**
```powershell
Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy Bypass
```

**Python download fails:**
- Check internet connection
- Download manually and extract to `build/VoicePaste-Portable/python/`

### Runtime Issues

**VoicePaste.bat shows "Python not found":**
- Use PowerShell build script, not batch script
- Or manually set up Python per SETUP_PYTHON.md

**CUDA errors on target PC:**
- Install NVIDIA drivers
- Or change config.json to use CPU

**Models downloading slowly:**
- Pre-download with `-IncludeModels`
- Or copy from cache: `%USERPROFILE%\.cache\huggingface\hub\`

## Advanced: Model Pre-caching

To include models in portable build:

1. Run app once to download models
2. Find cache: `%USERPROFILE%\.cache\huggingface\hub\`
3. Look for folders: `models--Systran--faster-whisper-*`
4. Copy to portable build and set `HF_HOME` environment variable

The PowerShell script does this automatically with `-IncludeModels`.

## Testing Portable Build

Before deploying to another PC:

1. Build with PowerShell script
2. Copy to different folder (simulate new PC)
3. Run `VoicePaste.bat`
4. Test recording with ScrollLock
5. Verify transcription works
6. Check GPU is being used (Task Manager)

## License & Distribution

When distributing portable builds:
- Include LICENSE file from project
- Whisper models are MIT licensed
- faster-whisper is MIT licensed
- .NET runtime is MIT licensed
- Python is PSF licensed

All open source and redistributable.
