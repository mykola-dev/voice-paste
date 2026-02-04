# VoicePaste - Agent Guidelines

## Documentation

| Doc | Topic |
|-----|-------|
| `docs/01-overview.md` | Project goals, requirements |
| `docs/02-architecture.md` | System components, tech stack |
| `docs/03-ux-spec.md` | UI/UX behavior, user flows |
| `docs/04-audio-capture.md` | WASAPI recording spec |
| `docs/05-transcription.md` | Whisper/faster-whisper |
| `docs/06-paste-mechanism.md` | Clipboard + SendInput |
| `docs/07-configuration.md` | Settings, config file |
| `docs/08-milestones.md` | Development phases |

## Project Overview

VoicePaste is a Windows resident tray application for voice dictation that:
- Records voice continuously (no auto-stop on silence) via hotkey toggle
- Transcribes using Whisper (GPU-first, NVIDIA CUDA)
- Pastes transcribed text into the currently focused window
- Supports bilingual input (English + Ukrainian, auto-detect)

## Architecture

```
voicepaste/
├── src/
│   ├── app/                 # WPF tray application (C#)
│   │   ├── TrayIcon/        # System tray functionality
│   │   ├── Overlay/         # Always-on-top recording overlay
│   │   ├── Settings/        # Configuration UI
│   │   ├── Audio/           # WASAPI microphone capture
│   │   ├── Hotkey/          # Global hotkey registration
│   │   └── Paste/           # Clipboard + SendInput paste
│   └── transcribe/          # Python STT worker
│       └── transcribe.py    # faster-whisper CUDA backend
├── config/                  # Default configuration
├── tests/                   # Test files
└── docs/                    # Documentation
```

## Build & Run Commands

### C# WPF Application
```bash
dotnet build                                      # Build solution
dotnet run --project src/app/VoicePaste.csproj   # Run in dev
dotnet publish -c Release -r win-x64 --self-contained  # Release build
dotnet test                                       # Run all tests
dotnet test --filter "FullyQualifiedName~TestClassName.TestMethodName"  # Single test
dotnet test --filter "TestMethodName"             # Filter by name
```

### Python Transcription Worker
```bash
pip install faster-whisper                        # Install deps
python src/transcribe/transcribe.py --input audio.wav --model medium --device cuda
pytest tests/                                     # Run all tests
pytest tests/test_transcribe.py::test_function -v # Single test
pytest -k "test_name_pattern"                     # Filter by pattern
```

## Code Style Guidelines

### C# Naming Conventions
- **Classes/Interfaces**: PascalCase (`AudioRecorder`, `ITranscriptionService`)
- **Methods/Properties**: PascalCase (`StartRecording`, `IsRecording`)
- **Private fields**: _camelCase (`_audioBuffer`, `_isRunning`)
- **Local variables**: camelCase (`wavFilePath`, `transcriptText`)
- **Constants**: PascalCase (`DefaultModel`) or UPPER_SNAKE_CASE (`MAX_BUFFER_SIZE`)

### C# File Organization
```csharp
using System;                    // 1. System imports
using NAudio.Wave;               // 2. Third-party
using VoicePaste.Core;           // 3. Project imports

namespace VoicePaste.Audio;

public class AudioRecorder : IDisposable
{
    private const int SampleRate = 16000;      // Constants
    private readonly ILogger _logger;           // Private fields
    public bool IsRecording { get; private set; } // Properties
    
    public AudioRecorder(ILogger logger) => _logger = logger;
    // Public methods, Private methods, IDisposable
}
```

### C# Error Handling
```csharp
try { await TranscribeAsync(audioPath); }
catch (FileNotFoundException ex) {
    _logger.LogError(ex, "Audio file not found: {Path}", audioPath);
    throw;
}
catch (CudaException ex) {
    _logger.LogWarning(ex, "CUDA failed, falling back to CPU");
    await TranscribeAsync(audioPath, device: "cpu");
}
```

### Python Naming Conventions
- **Functions/Variables**: snake_case (`transcribe_audio`, `model_path`)
- **Classes**: PascalCase (`TranscriptionResult`)
- **Constants**: UPPER_SNAKE_CASE (`DEFAULT_MODEL`, `SAMPLE_RATE`)
- **Private**: underscore prefix (`_load_model`)

### Python Imports & Types
```python
import os                        # 1. Standard library
from faster_whisper import WhisperModel  # 2. Third-party
from .utils import format_timestamp      # 3. Local

def transcribe(audio_path: Path, model: str = "medium", 
               device: str = "cuda") -> str:
    """Transcribe audio file. Type hints required."""
```

### Python Error Handling
```python
try:
    model = WhisperModel(model_size, device=device)
except RuntimeError as e:
    if "CUDA" in str(e):
        model = WhisperModel(model_size, device="cpu")  # Fallback
    else:
        raise TranscriptionError(f"Failed: {e}") from e
```

## Configuration

### Default Settings (`%AppData%/VoicePaste/config.json`)
```json
{
  "hotkey": "ScrollLock",
  "pasteShortcut": "Ctrl+Shift+V",
  "model": "medium",
  "device": "cuda"
}
```

### Environment Variables
- `VOICEPASTE_MODEL_PATH` - Override model location
- `VOICEPASTE_DEVICE` - Force cpu/cuda
- `VOICEPASTE_DEBUG=1` - Enable debug logging

## Key Technical Decisions

- **Audio**: 16kHz mono 16-bit PCM WAV via WASAPI
- **STT**: faster-whisper (CTranslate2), model `medium`, auto-detect language
- **GPU**: CUDA first, CPU fallback on failure
- **Paste**: Clipboard + SendInput with configurable shortcut

## Dependencies

### C# / .NET
- .NET 8.0+, NAudio, Hardcodet.NotifyIcon.Wpf, System.Text.Json

### Python
- Python 3.10+, faster-whisper, CUDA Toolkit 11.x/12.x

## Troubleshooting

### CUDA Not Working
1. Verify NVIDIA drivers: `nvidia-smi`
2. Check CUDA toolkit compatibility
3. Verify CTranslate2 CUDA support

### Microphone Not Detected
1. Windows Settings → Privacy → Microphone access
2. Verify default input device in Sound settings

### Paste Not Working
1. Match paste shortcut to terminal settings (Ctrl+V vs Ctrl+Shift+V)
2. Try alternatives: Shift+Insert
