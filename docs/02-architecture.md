# System Architecture

## Component Overview

```
┌─────────────────────────────────────────────────────────┐
│                    VoicePaste                           │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐ │
│  │  Tray Host  │  │   Overlay   │  │    Settings     │ │
│  │   (WPF)     │  │   Window    │  │     Dialog      │ │
│  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘ │
│         │                │                   │          │
│  ┌──────┴────────────────┴───────────────────┴───────┐ │
│  │              Core State Machine                    │ │
│  │         idle → recording → transcribing → idle     │ │
│  └──────┬────────────────┬───────────────────┬───────┘ │
│         │                │                   │          │
│  ┌──────┴──────┐  ┌──────┴──────┐  ┌────────┴────────┐ │
│  │   Hotkey    │  │    Audio    │  │      Paste      │ │
│  │  Handler    │  │   Capture   │  │    (Clipboard   │ │
│  │             │  │  (WASAPI)   │  │   + SendInput)  │ │
│  └─────────────┘  └──────┬──────┘  └─────────────────┘ │
└──────────────────────────┼──────────────────────────────┘
                           │ temp.wav
                    ┌──────┴──────┐
                    │ Transcribe  │
                    │   Worker    │
                    │  (Python)   │
                    └─────────────┘
```

## Tech Stack

| Component | Technology | Why |
|-----------|------------|-----|
| Tray App | C# WPF (.NET 8) | Best Windows integration for tray/hotkeys/overlay |
| Audio Capture | NAudio + WASAPI | Native Windows, low latency |
| STT Engine | faster-whisper | GPU acceleration, multilingual |
| GPU Backend | CTranslate2 + CUDA | Fast inference on NVIDIA |
| IPC | Subprocess + stdout | Simple, no dependencies |

## Directory Structure

```
voicepaste/
├── src/
│   ├── app/                      # C# WPF Application
│   │   ├── VoicePaste.csproj
│   │   ├── App.xaml(.cs)
│   │   ├── MainWindow.xaml(.cs)  # Hidden main window
│   │   ├── TrayIcon/
│   │   │   └── TrayIconManager.cs
│   │   ├── Overlay/
│   │   │   ├── OverlayWindow.xaml(.cs)
│   │   │   └── OverlayViewModel.cs
│   │   ├── Settings/
│   │   │   ├── SettingsWindow.xaml(.cs)
│   │   │   └── SettingsManager.cs
│   │   ├── Audio/
│   │   │   └── AudioRecorder.cs
│   │   ├── Hotkey/
│   │   │   └── GlobalHotkeyManager.cs
│   │   ├── Paste/
│   │   │   └── ClipboardPaster.cs
│   │   └── Transcription/
│   │       └── TranscriptionService.cs
│   │
│   └── transcribe/               # Python STT Worker
│       ├── transcribe.py
│       └── requirements.txt
│
├── config/
│   └── default-config.json
├── docs/
└── tests/
```

## Data Flow

1. **Hotkey Press (Start)**
   - GlobalHotkeyManager detects ScrollLock
   - State → Recording
   - Show overlay, start AudioRecorder

2. **Recording**
   - WASAPI captures mic → PCM buffer
   - Write to temp WAV file (16kHz mono)
   - Overlay shows timer + level meter

3. **Hotkey Press (Stop)**
   - State → Transcribing
   - Finalize WAV file
   - Spawn `python transcribe.py --input temp.wav`

4. **Transcription**
   - Worker loads faster-whisper model
   - Transcribe with CUDA (or CPU fallback)
   - Print text to stdout

5. **Paste**
   - Read transcript from worker stdout
   - Save current clipboard
   - Set clipboard to transcript
   - SendInput: Ctrl+Shift+V
   - Restore clipboard after delay
   - State → Idle, hide overlay
