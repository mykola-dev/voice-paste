<p align="center">
  <img src="apps/voicepaste/assets/icon.png" alt="Voice Paste" width="96" height="96">
</p>

<h1 align="center">Voice Paste 2</h1>

<p align="center">
  <em>Speak. Transcribe. Done.</em><br>
  Windows voice dictation that doesn't stop on silence.
</p>

<p align="center">
  <a href="https://github.com/mykola-dev/voice-agent/releases"><img src="https://img.shields.io/badge/Download-Windows-0078D6?logo=windows&logoColor=white" alt="Download for Windows"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Built_with-Rust-CE422B?logo=rust&logoColor=white" alt="Built with Rust"></a>
</p>

https://github.com/user-attachments/assets/68783c53-69fd-4d9c-a363-f89770ae0b15

## 🎙️ Your voice, wherever you type

Press a hotkey, speak naturally, press it again — your words are transcribed
and pasted into the active app. Pause as long as you need: you decide when to stop.

- 🌍 English, Ukrainian and bilingual dictation.
- ⚡ Local Whisper transcription with NVIDIA CUDA acceleration.
- 🫥 Lives in the tray; the recording overlay stays out of your way.
- 📦 Portable Rust app. No Python or .NET installation.

## 🚀 Quick start

1. Download the **Windows x64 ZIP** from [Releases](https://github.com/mykola-dev/voice-agent/releases).
2. Extract it to a writable folder and run **`VoicePaste.exe`**. Keep `runtime/` alongside it.
3. Allow the first model download to finish — internet is needed for setup.
4. Focus your editor, chat or terminal. Press <kbd>ScrollLock</kbd>, speak,
   then press <kbd>ScrollLock</kbd> again to transcribe and paste.

Press <kbd>Esc</kbd> during recording to discard it without pasting.
Open **Settings** from the tray to change the hotkey, model, language or appearance.

**Requirements:** Windows x64. The default GPU mode needs a compatible NVIDIA GPU
and driver. To configure the device before loading a model, run `VoicePaste.exe --settings`.

Your settings and models live in `data/` beside the app. Keep that folder when
upgrading; quit the app before replacing files.

## 🛠️ Build it yourself

Run **`build-portable-windows.bat`** from the repository root. It builds a Release
portable folder and ZIP under `build/release/` without starting or stopping the app.
Build tools and native dependencies must already be available — see the
[Windows build guide](docs/build-windows.md#build-and-checks-windows-x64).

<p align="center">
  <a href="docs/README.md">📖 Documentation</a> ·
  <a href="RustMigration/TODO.md">🚧 Known issues &amp; unfinished work</a> ·
  <a href="https://github.com/mykola-dev/voice-agent/issues">🐛 Report a bug</a>
</p>
