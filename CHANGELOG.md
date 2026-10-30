# Changelog

## [Unreleased]

- Added an Apple Silicon macOS launch preview with a local build script that downloads Rust only when needed, with approval.

## [2.0.0]

- Rewritten in Rust, replacing the Python/.NET application and transcription worker.
- Rebuilt Settings with GPU-accelerated GPUI components and the One Dark theme.
- Added local transcription statistics and Whisper large-v3 model support.
- Rebuilt the recording overlay in GPUI with animated waveforms and a blurred translucent background.
- Improved overlay animation, waveform sensitivity and placement on mixed-DPI monitors.
- Unified Windows portable builds and added verified ZIP packages with SHA-256 checksums.
