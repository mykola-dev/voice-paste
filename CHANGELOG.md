# Changelog

## [Unreleased]

## [2.0.1] - 2026-11-02

- The Windows portable build script updates the Release folder without creating a ZIP, and shows build progress.
- Avoid briefly showing the overlay when switching to an already downloaded model.
- Model downloads start automatically and retries no longer require confirmation on Windows or macOS.
- Windows avoids redundant idle tray updates and retries restoring its tray icon after Explorer restarts.
- macOS model downloads no longer automatically read the user's curl configuration or inherit TLS key logging.
- Avoid recording filename collisions without overwriting retained audio.
- Preserve failed recordings and offer a retry without VAD when transcription fails with VAD enabled.
- macOS uses a transparent Acrylic overlay without exposing material controls; model download prompts and progress are simpler on both platforms.

- macOS reuses verification for unchanged model files instead of rechecking them on every startup.
- Acrylic overlays retain blur at 100% transparency while the white tint follows the full transparency range on Windows and macOS.
- The macOS tray menu follows the system appearance instead of the menu bar's appearance.
- Windows tray menus can follow the system light or dark theme on supported Windows versions.
- macOS no longer opens Accessibility settings automatically on top of the system permission prompt.
- Removed the permanent macOS permission reminder from Settings.
- Preserved macOS menu-bar icon proportions and changed the default waveform amplification to 10× without overwriting saved values.
- Improved macOS keyboard-tap recovery and Cmd+V event delivery, and report when global recording keys cannot be attached.
- macOS now defaults to Right Option and offers Right Command instead of Right Control for recording.
- macOS model downloads now use verified, resumable GitHub Models assets instead of Hugging Face or Git LFS; Windows retains its original HTTPS source without fallback.
- Model downloads show progress with cancellation and error dialogs, and missing macOS models can be retried by starting recording.
- Matched the macOS recording menu to Windows and corrected the overlay's top-right placement.
- macOS now requests Accessibility at startup, reconnects recording keys after approval and opens microphone privacy settings after denied recording access.

- Added an Apple Silicon macOS launch preview with a local build script that handles Rosetta terminals and downloads Rust only when needed, with approval.
- Added shared Settings, a native menu-bar icon and opt-in sign-in startup to the macOS preview, with configuration stored in Application Support.
- Connected macOS dictation to the shared recording/runtime, single-key controls, animated overlay, local Metal/CPU worker, full-size model downloads, Cmd+V and statistics.
- Fixed macOS Settings-only mode staying open after closing Settings, and refreshed its statistics while the window remains open.
- Fixed model-readiness hints after preparation and made macOS report unexpected dictation-thread exits instead of leaving stale UI.
- Made macOS model publication honor cancellation after verification, sync staged weights and retain replaced corrupt artifacts as backups.
- Fixed downloaded macOS models remaining unpublished and prevented duplicate recording starts while microphone permission is pending.
- Fixed macOS local builds incorrectly rejecting and repeatedly offering to download an installed Rust toolchain; toolchain failures now show diagnostics.
- Addressed a macOS Touch Bar crash when closing Settings using an upstream accessibility fix.
- Made Windows file transcription verify pinned local model artifacts before inference, without enabling downloads.

## [2.0.0]

- Rewritten in Rust, replacing the Python/.NET application and transcription worker.
- Rebuilt Settings with GPU-accelerated GPUI components and the One Dark theme.
- Added local transcription statistics and Whisper large-v3 model support.
- Rebuilt the recording overlay in GPUI with animated waveforms and a blurred translucent background.
- Improved overlay animation, waveform sensitivity and placement on mixed-DPI monitors.
- Unified Windows portable builds and added verified ZIP packages with SHA-256 checksums.
