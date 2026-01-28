# VoicePaste

A Windows resident tray application for voice dictation with no silence auto-stop.


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

## Documentation

See `docs/` for detailed specifications:
- `docs/01-overview.md` - Project goals
- `docs/02-architecture.md` - System design
- `docs/03-ux-spec.md` - User experience
- `docs/04-audio-capture.md` - Audio implementation
- `docs/05-transcription.md` - Whisper integration
- `docs/06-paste-mechanism.md` - Paste behavior
- `docs/07-configuration.md` - Settings
- `docs/08-milestones.md` - Development progress

## License

MIT
