# VoicePaste

A Windows resident tray application for voice dictation with no silence auto-stop.

![VoicePaste Screenshot](pic.png)

## Building

### Debug build + run

```bat
./dev-debug.bat
```

### Portable release

```bat
./release-portable.bat
```

Output runs directly (no launcher script): `build/VoicePaste-Release/VoicePaste.exe`

### Prerequisites
- Windows 10/11
- .NET 8.0 SDK
- Python 3.10+
- NVIDIA GPU with CUDA (optional, for GPU acceleration)


## Usage

1. Launch VoicePaste (runs in system tray)
2. Press **ScrollLock** to start recording
3. Speak naturally (no silence detection)
4. Press **ScrollLock** again to stop and transcribe
5. Text auto-pastes into focused window

## Configuration

Default settings in `config/config.json`:
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

**Available Models:**
- `large-v3` (default) - Best accuracy, slower, recommended for GPU
- `medium` - Balanced accuracy/speed
- `small` - Fastest, lower accuracy

**Device Options:**
- `cuda` (default) - GPU acceleration (requires NVIDIA GPU)
- `cpu` - CPU-only (automatic fallback if CUDA unavailable)

Settings can be changed through the UI (right-click tray icon → Settings).

## License

MIT
