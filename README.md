<p align="center">
  <img src="apps/voicepaste/assets/icon.png" alt="Voice Paste" width="96" height="96">
</p>

<h1 align="center">Voice Paste 2</h1>

<p align="center">
  <em>Speak. Stop. Paste.</em>
</p>

<p align="center">
  Built to make interacting with agentic tools several times faster, VoicePaste lets you dictate prompts, code, commands and messages for vibe coding, computer use, and chats with AI agents. Whisper transcribes locally and pastes your words wherever you're typing.
</p>

<p align="center">
  <a href="https://github.com/mykola-dev/voice-paste/releases"><img src="https://img.shields.io/badge/Download-Windows-0078D6?logo=windows&logoColor=white" alt="Download for Windows"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Built_with-Rust-CE422B?logo=rust&logoColor=white" alt="Built with Rust"></a>
</p>

https://github.com/user-attachments/assets/fab84116-ae55-4019-8e6d-b59ed1ddcb68

## 🎙️ Your voice, wherever you type

Press a hotkey, speak naturally, press it again — your words are transcribed
and pasted into the active app. Pause as long as you need: you decide when to stop.

- 🌍 English, Ukrainian and bilingual dictation.
- ⚡ Local Whisper transcription with NVIDIA CUDA on Windows or Metal on Apple Silicon.
- 🫥 Lives in the system tray or menu bar; the recording overlay stays out of your way.
- 📦 Portable Rust app. No Python or .NET installation.

## 🚀 Quick start: Windows

1. Download the **Windows x64 ZIP** from [Releases](https://github.com/mykola-dev/voice-paste/releases).
2. Extract it to a writable folder and run **`VoicePaste.exe`**. Keep `runtime/` alongside it.
3. Allow the first model download to finish — internet is needed for setup.
4. Focus your editor, chat or terminal. Press <kbd>ScrollLock</kbd>, speak,
   then press <kbd>ScrollLock</kbd> again to transcribe and paste.

Press <kbd>Esc</kbd> during recording to discard it without pasting.
Open **Settings** from the tray to change the hotkey, model, language or appearance.

See [Changelog](CHANGELOG.md) for release notes.

**Requirements:** Windows x64. The default GPU mode needs a compatible NVIDIA GPU
and driver. To configure the device before loading a model, run `VoicePaste.exe --settings`.

Your settings and models live in `data/` beside the app. Keep that folder when
upgrading; quit the app before replacing files.

## 🛠️ Build it yourself

Run **`build-portable-windows.bat`** from the repository root. It builds a Release
portable app in `build/release/` without creating a ZIP or starting or stopping the app.
Requires Rust and MSVC/Windows SDK; native runtime files are acquired automatically.
No PowerShell or Bash is needed. See the
[Windows build guide](docs/build-windows.md#build-and-checks-windows-x64).

### macOS beta: build and install

The Apple Silicon port is a **beta**, built locally from the `master` branch.
It requires macOS 26 or later, an Apple Silicon Mac, Git, and full Xcode with
its Metal tools selected and set up. Intel Macs are not supported. Allow disk
space for the build and a full-size Whisper model (roughly 1.6–3.1 GB, plus
temporary files). The user reports that macOS is working; the complete device
acceptance and distribution qualification remain in progress.

In Terminal:

```sh
git clone https://github.com/mykola-dev/voice-paste.git
cd voice-paste
./build-macos-preview.sh
open build/release/VoicePaste.app
```

The script asks before downloading a repository-local Rust toolchain or CMake
if needed; Cargo also fetches the locked build dependencies. It does not install
Xcode or download speech models. On first launch, the selected model downloads
automatically as on Windows; retries do not require download confirmation.
Grant Accessibility access for the recording hotkey and paste,
and Microphone access when starting a recording. The default key is
<kbd>Right Option</kbd>; press it again to stop, or <kbd>Esc</kbd> to cancel.
Settings and Quit are available from the menu-bar icon.

This beta is ad-hoc signed, not notarized, and macOS may block it. Do not
change security policies to run it. After rebuilding, an old Accessibility entry
may need to be removed and the new app added, subject to your device's policy.
Settings and models are stored under `~/Library/Application Support/VoicePaste/`.
To update, quit the app, preserve that data, and move the existing
`build/release/VoicePaste.app` aside before pulling and running the build script
again. See the [macOS beta guide](docs/macos-preview.md) for details and known limits.

## 🔒 Privacy and local data

- Transcription runs locally. Downloading model files requires a network connection;
  the download server can see the request and your network address.
- Dictation uses the system clipboard. The transcript remains there after pasting
  and may be retained by clipboard history, other apps or system sync.
- Recordings use temporary local WAV files. Failed recordings may be retained for
  recovery, and a crash can leave a temporary file behind.
- Settings, including custom prompts, are stored locally. Local statistics include
  metadata such as model, language, timing and word counts, not the transcript or audio.
- Native worker diagnostics are not filtered for sensitive content: on Windows they
  go to the host log (with best-effort rotation), and on macOS to host stderr,
  whose destination depends on the launcher. A diagnostic containing a transcript
  or prompt has not been demonstrated, but cannot be ruled out. Inspect diagnostics
  before sharing them.

## 🧰 Stack & acknowledgements

Built with **Rust**, using [GPUI](https://www.gpui.rs/) for GPU-accelerated UI
and [GPUI Kit](https://github.com/longbridge/gpui-kit) for components and assets.
Huge thanks to the Zed team, the GPUI Kit developers and all contributors for
making these open-source tools available! ❤️

## 🗺️ Roadmap

- [x] Apple Silicon macOS port — **beta**; device acceptance and distribution remain in progress.
- [ ] Linux port.

See [TODO.md](TODO.md) for known issues and remaining beta work.

<p align="center">
  <a href="docs/README.md">📖 Documentation</a> ·
  <a href="TODO.md">🚧 Known issues &amp; unfinished work</a> ·
  <a href="https://github.com/mykola-dev/voice-paste/issues">🐛 Report a bug</a>
</p>
