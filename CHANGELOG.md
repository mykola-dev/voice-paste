# Changelog

## [Unreleased]

- Added an Apple Silicon macOS launch preview with a local build script that handles Rosetta terminals and downloads Rust only when needed, with approval.
- Added shared Settings, a native menu-bar icon and opt-in sign-in startup to the macOS preview, with configuration stored in Application Support.
- Connected macOS dictation to the shared recording/runtime, single-key controls, animated overlay, local Metal/CPU worker, approved full-size model downloads, Cmd+V and statistics.
- Fixed macOS Settings-only mode staying open after closing Settings, and refreshed its statistics while the window remains open.
- Fixed model-readiness hints after preparation and made macOS report unexpected dictation-thread exits instead of leaving stale UI.
- Made macOS model publication honor cancellation after verification, sync staged weights and retain replaced corrupt artifacts as backups.
- Fixed downloaded macOS models remaining unpublished and prevented duplicate recording starts while microphone permission is pending.

## [2.0.0]

- Rewritten in Rust, replacing the Python/.NET application and transcription worker.
- Rebuilt Settings with GPU-accelerated GPUI components and the One Dark theme.
- Added local transcription statistics and Whisper large-v3 model support.
- Rebuilt the recording overlay in GPUI with animated waveforms and a blurred translucent background.
- Improved overlay animation, waveform sensitivity and placement on mixed-DPI monitors.
- Unified Windows portable builds and added verified ZIP packages with SHA-256 checksums.
