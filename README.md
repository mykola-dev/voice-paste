<div align="center">

# 🎙️ VoicePaste

### *Speak. Transcribe. Done.*

[![License: MIT](https://img.shields.io/badge/License-MIT-FF8C00.svg)](LICENSE)
[![.NET 8.0](https://img.shields.io/badge/.NET-8.0-512BD4?logo=dotnet&logoColor=white)](https://dotnet.microsoft.com/)
[![Windows](https://img.shields.io/badge/Windows-10%2F11-0078D6?logo=windows&logoColor=white)](https://www.microsoft.com/windows)
[![Python](https://img.shields.io/badge/Python-3.12-3776AB?logo=python&logoColor=white)](https://python.org)
[![CUDA](https://img.shields.io/badge/NVIDIA-CUDA-76B900?logo=nvidia&logoColor=white)](https://developer.nvidia.com/cuda)

https://github.com/user-attachments/assets/68783c53-69fd-4d9c-a363-f89770ae0b15


**Windows voice dictation that doesn't stop on silence** 🚀

[📥 Download Latest](https://github.com/mykola-dev/voice-agent/releases) · [📖 Documentation](docs/) · [🐛 Report Bug](https://github.com/mykola-dev/voice-agent/issues)

</div>

---

## ✨ What is VoicePaste?

VoicePaste is a **resident system tray application** for Windows that lets you dictate text anywhere using your voice—without annoying auto-stop on silence.

🔴 **Press ScrollLock** → 🎙️ **Speak naturally** → 🟢 **Press ScrollLock again** → 📋 **Text appears instantly**

### 🌟 Key Features

| Feature | Description |
|---------|-------------|
| 🎙️ **Manual Control** | Recording starts/stops only when YOU decide (no silence auto-stop) |
| ⚡ **GPU-Powered** | NVIDIA CUDA acceleration with automatic CPU fallback |
| 🌍 **Bilingual** | Auto-detects English + Ukrainian (no language switching needed) |
| 📋 **Universal Paste** | Works with any app: terminals, editors, browsers, IDEs |
| 🎯 **Always Available** | Runs in system tray—ready whenever you need it |
| 📦 **Portable** | Self-contained build—no installation required |

---

## 🚀 Quick Start

### Prerequisites
- Windows 10 or 11
- NVIDIA GPU with CUDA (optional, for GPU acceleration)

### Installation

#### Option 1: Download Pre-built Release ⭐ Recommended
```bash
# Download from releases page and extract
VoicePaste.exe
```

#### Option 2: Build from Source
```bash
# Clone the repository
git clone https://github.com/mykola-dev/voice-agent.git
cd voice-agent

# Debug build + run
./dev-debug.bat

# Or update the portable build and launch it automatically
./build-portable.bat
# Output: build/VoicePaste-Portable/VoicePaste.exe
```

`build-portable.bat` stops the VoicePaste instance running from the target output folder
(including its worker processes), updates the portable files, then starts the new executable.
This interrupts any active dictation. Other installations/debug instances are left alone.
If publishing or dependency setup fails, the app is not started. Use `-NoLaunch` for a build
without the final launch; this option still stops the target instance before updating it.

---

## 🎮 How to Use

1. **Launch** VoicePaste (minimizes to system tray) 📌
2. **Press** <kbd>ScrollLock</kbd> to start recording 🔴
3. **Speak naturally** with pauses—recording continues! 🎙️
4. **Press** <kbd>ScrollLock</kbd> again to stop and transcribe ⏹️
5. **Text auto-pastes** into your currently focused window 📋

Changed your mind while recording? Press **Esc** to discard the audio without transcription or
pasting. That Esc press is consumed; outside recording, Esc works normally.

### ⚙️ Configuration

Right-click the tray icon → **Settings** to customize:

| Setting | Default | Options |
|---------|---------|---------|
| **Hotkey** | `ScrollLock` | Any key |
| **Paste Shortcut** | `Ctrl+Shift+V` | `Ctrl+V`, `Shift+Insert`, etc. |
| **Model** | `large-v3-turbo` | `large-v3`, `medium`, `small` |
| **Device** | `cuda` | `cuda` (GPU) / `cpu` (CPU) |

---

## 🛠️ Tech Stack

<div align="center">

[![C#](https://img.shields.io/badge/C%23-239120?logo=c-sharp&logoColor=white)]()
[![WPF](https://img.shields.io/badge/WPF-5C2D91?logo=windows&logoColor=white)]()
[![Python](https://img.shields.io/badge/Python-3776AB?logo=python&logoColor=white)]()
[![Whisper](https://img.shields.io/badge/Whisper-FF8C00?logo=openai&logoColor=white)]()

</div>

- **Frontend**: C# / WPF (.NET 8.0)
- **STT Engine**: OpenAI Whisper via [faster-whisper](https://github.com/SYSTRAN/faster-whisper)
- **Audio Capture**: WASAPI (NAudio)
- **Platform**: Windows 10/11

---

## 📚 Documentation

| Document | Description |
|----------|-------------|
| [📖 Overview](docs/01-overview.md) | Project goals & requirements |
| [🏗️ Architecture](docs/02-architecture.md) | System components & tech stack |
| [🎨 UX Spec](docs/03-ux-spec.md) | UI behavior & user flows |
| [🎙️ Audio Capture](docs/04-audio-capture.md) | WASAPI recording details |
| [📝 Transcription](docs/05-transcription.md) | Whisper integration |
| [📋 Paste Mechanism](docs/06-paste-mechanism.md) | Clipboard & SendInput |
| [⚙️ Configuration](docs/07-configuration.md) | Settings & config file |
| [🧪 Testing](docs/10-testing.md) | Automated and manual checks |

---

## 🤝 Contributing

Contributions are welcome! Feel free to:
- 🐛 [Report bugs](https://github.com/mykola-dev/voice-agent/issues)
- 💡 [Suggest features](https://github.com/mykola-dev/voice-agent/issues)
- 🔧 [Submit pull requests](https://github.com/mykola-dev/voice-agent/pulls)

---

## 📄 License

This project is licensed under the **MIT License** - see the [LICENSE](LICENSE) file for details.

---

<div align="center">

Made with ❤️ and 🔊

**[⬆ Back to top](#-voicepaste)**

</div>
