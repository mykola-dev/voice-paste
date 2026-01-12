# VoicePaste

A Windows resident tray application for voice dictation with no silence auto-stop.

## Current Status

**Development Progress:**

- ✅ **Phase 1: MVP** - Core recording + transcription + paste
- ✅ **Phase 2: UI Polish** - Tray icon, overlay, visual feedback
- ✅ **Phase 3: Settings** - Configuration UI with hotkey/model/device selection
- ✅ **Phase 4: Robustness** - Error handling, clipboard retry logic, edge cases
- ✅ **Phase 5: Portable Build** - Self-contained build with embedded Python
- ⚠️ **Phase 5: Distribution** - Packaging/installer (in progress)

**Ready for Production Use!**

The application is fully functional with:
- Continuous voice recording (no auto-stop on silence)
- GPU-accelerated transcription (CUDA) with CPU fallback
- Bilingual support (English + Ukrainian, auto-detect)
- System tray with recording overlay
- Settings UI for customization
- Clipboard retry logic for reliability
- Portable build with no dependencies

## Project Structure

```
voicepaste/
├── src/
│   ├── app/                          # C# WPF Application
│   │   ├── VoicePaste.csproj
│   │   ├── App.xaml(.cs)
│   │   ├── MainWindow.xaml(.cs)
│   │   ├── VoicePasteController.cs   # Main state machine
│   │   ├── Audio/
│   │   │   └── AudioRecorder.cs      # WASAPI capture
│   │   ├── Hotkey/
│   │   │   └── GlobalHotkeyManager.cs
│   │   ├── Paste/
│   │   │   └── ClipboardPaster.cs
│   │   └── Transcription/
│   │       └── TranscriptionService.cs
│   │
│   └── transcribe/                   # Python STT Worker
│       ├── transcribe.py
│       └── requirements.txt
│
├── config/
│   └── config.json                   # Default settings
├── docs/                             # Full specifications
└── tests/                            # Tests (TBD)
```

## Building

### Debug build + run

```bat
./dev-debug.bat
```

### Portable release (smart)

```bat
./release-portable.bat
```

Output runs directly (no launcher script): `build/VoicePaste-Release/VoicePaste.exe`

### Prerequisites
- Windows 10/11
- .NET 8.0 SDK
- Python 3.10+
- NVIDIA GPU with CUDA (optional, for GPU acceleration)

### Setup

1. **Install Python dependencies:**
   ```bash
   cd src/transcribe
   pip install -r requirements.txt
   ```

2. **Build C# application:**
   ```bash
   cd src/app
   dotnet build
   ```

3. **Run (Debug):**
   ```bash
   dotnet run --project src/app/VoicePaste.csproj
   ```

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

## Development Phases

- [x] **Phase 1: MVP** - Core recording + transcription + paste
- [x] **Phase 2: UI Polish** - Tray icon, overlay, visual feedback
- [x] **Phase 3: Settings** - Configuration UI
- [x] **Phase 4: Robustness** - Error handling, edge cases
- [x] **Phase 5: Portable Build** - Self-contained deployment
- [ ] **Phase 5: Distribution** - Installer, packaging (in progress)

## License

TBD
