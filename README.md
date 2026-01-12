# VoicePaste

A Windows resident tray application for voice dictation with no silence auto-stop.

## Current Status

**Phase 1 (MVP Core) - ✅ COMPLETED**

All core components have been implemented:
- ✅ Audio recording (WASAPI, 16kHz mono)
- ✅ Global hotkey (ScrollLock)
- ✅ Transcription service (faster-whisper)
- ✅ Clipboard paste mechanism (Ctrl+Shift+V)
- ✅ State machine (idle → recording → transcribing → idle)

**Next Steps:**
- Test on Windows with .NET 8 and Python 3.10+
- Add tray icon UI (Phase 2)
- Add recording overlay (Phase 2)

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
  "model": "medium",
  "device": "cuda",
  "restoreClipboard": true,
  "clipboardRestoreDelayMs": 400
}
```

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
- [ ] **Phase 2: UI Polish** - Tray icon, overlay, visual feedback
- [ ] **Phase 3: Settings** - Configuration UI
- [ ] **Phase 4: Robustness** - Error handling, edge cases
- [ ] **Phase 5: Distribution** - Installer, packaging

## License

TBD
