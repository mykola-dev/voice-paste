# Changelog

## [Unreleased]

- Added an Apple Silicon macOS launch preview with a local build script that handles Rosetta terminals and downloads Rust only when needed, with approval.
- Added shared Settings, a native menu-bar icon and opt-in sign-in startup to the macOS preview, with configuration stored in Application Support.
- Added opt-in macOS microphone capture preview with explicit permission, recording status and temporary-audio cleanup, without transcription or paste.

## [2.0.0]

- Rewritten in Rust, replacing the Python/.NET application and transcription worker.
- Rebuilt Settings with GPU-accelerated GPUI components and the One Dark theme.
- Added local transcription statistics and Whisper large-v3 model support.
- Rebuilt the recording overlay in GPUI with animated waveforms and a blurred translucent background.
- Improved overlay animation, waveform sensitivity and placement on mixed-DPI monitors.
- Unified Windows portable builds and added verified ZIP packages with SHA-256 checksums.
