# VoicePaste - Development Scripts

## Quick Start

### Option 1: Build and Run (Recommended for Development)
```batch
dev-debug.bat
```
This script will:
1. Build the project in Debug configuration
2. Stop any running instances
3. Run VoicePaste with console output visible

**Use this when**: You've made code changes and want to see the latest build.

### Option 2: Quick Run

Removed in favor of keeping a single entry point.
Use `dev-debug.bat`.

## What You'll See

When running in debug mode, you'll see console output like:

```
[CUDA] Added to PATH: C:\Users\...\nvidia\cublas\bin
VoicePaste is running. Press ScrollLock to record.
Log file: C:\Users\...\AppData\Local\Temp\VoicePaste\voicepaste.log

[Hotkey] Registering hotkey - Key: 0x91, Modifiers: 0, Handle: 123456
[Hotkey] Hotkey registered successfully with ID: 12345

[Hotkey] WM_HOTKEY received! ID: 12345
[Controller] Hotkey pressed! Current state: Idle
[Controller] Starting recording...
[Controller] Recording started

[Controller] Hotkey pressed! Current state: Recording
[Controller] Stopping recording...
[Controller] Recording saved to: C:\Users\...\Temp\VoicePaste\recording_20260112_130755.wav
[Controller] Starting transcription...
[Transcribe] Attempting transcription with device: cuda
[Transcribe] Hello, this is a test
[Controller] Transcription complete. Text: 'Hello, this is a test'
[Controller] Pasting text...
[Paste] Text to paste: 'Hello, this is a test'
[Paste] Sending paste shortcut: Ctrl+Shift+V
[Controller] Paste complete
```

## How to Use VoicePaste

1. **Start Recording**: Press **ScrollLock** (or click "Start Recording" in tray menu)
2. **Speak**: Talk into your microphone (recording continues even during silence)
3. **Stop & Transcribe**: Press **ScrollLock** again
4. **Text Appears**: Transcribed text pastes into your focused window

## Features

- ✅ **GPU Transcription**: Uses NVIDIA CUDA for fast transcription
- ✅ **Bilingual**: Auto-detects English and Ukrainian
- ✅ **Tray Icon**: Shows recording state (gray/red/orange)
- ✅ **Recording Overlay**: Top-right window with timer and audio meter
- ✅ **Debug Output**: See what's happening in real-time

## Troubleshooting

### ScrollLock Not Working
Check the console for:
```
[Hotkey] Hotkey registered successfully with ID: 12345
```
If you see "Failed to register hotkey", another app is using ScrollLock.

### CUDA Not Working
Check the console for:
```
[CUDA] Added to PATH: C:\Users\...\nvidia\cublas\bin
```
If missing, CUDA libraries aren't installed. Run:
```bash
pip install nvidia-cublas-cu12
```

### No Audio / Microphone Not Working
1. Check Windows Settings → Privacy → Microphone access
2. Verify default input device in Sound settings
3. Watch the green audio level bar in the overlay - should move when speaking

### Transcription Fails
Check console for Python errors. Make sure faster-whisper is installed:
```bash
pip install faster-whisper
```

## Log File Location

Full logs are saved to:
```
%TEMP%\VoicePaste\voicepaste.log
```

Or typically:
```
C:\Users\YourName\AppData\Local\Temp\VoicePaste\voicepaste.log
```

## Stop the App

- **From Console**: Press `Ctrl+C`
- **From Tray**: Right-click tray icon → Quit
- **Force Stop**: `taskkill /F /IM VoicePaste.exe`

## Building Manually

If you prefer to build manually:

```bash
# Build
dotnet build src/app/VoicePaste.csproj

# Run
dotnet run --project src/app/VoicePaste.csproj
```

## Next: Phase 3 - Settings

Coming soon:
- Settings dialog (hotkey configuration, model selection, etc.)
- Config file persistence
- More configuration options

Enjoy dictating! 🎤
