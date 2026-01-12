# Development Milestones

## Phase 1: MVP (Recording + Transcription)

### Milestone 1.1: Project Setup ✅ COMPLETED
- [x] Create .NET 8 WPF project
- [x] Create Python transcribe worker
- [x] Setup project structure
- [x] Add dependencies (NAudio, Hardcodet.NotifyIcon.Wpf)

### Milestone 1.2: Hotkey + Recording ✅ COMPLETED
- [x] Global hotkey registration (ScrollLock)
- [x] WASAPI audio capture (16kHz mono)
- [x] Save to temp WAV file
- [x] Basic state machine (idle ↔ recording)

### Milestone 1.3: Transcription Integration ✅ COMPLETED
- [x] Call Python worker from C#
- [x] Parse stdout for transcript
- [x] Handle CUDA/CPU fallback
- [x] State: recording → transcribing → idle

### Milestone 1.4: Paste Mechanism ✅ COMPLETED
- [x] Clipboard manipulation
- [x] SendInput for Ctrl+Shift+V
- [x] Clipboard restoration with delay

**MVP Complete Checklist:** (Ready for testing on Windows)
- [ ] Press ScrollLock → starts recording
- [ ] Press ScrollLock again → transcribes → pastes
- [ ] Works with Windows Terminal
- [ ] GPU transcription works

---

## Phase 2: UI Polish ✅ COMPLETED

### Milestone 2.1: Tray Icon ✅ COMPLETED
- [x] System tray icon with menu
- [x] Start/Stop menu item
- [x] Settings menu item
- [x] Quit menu item
- [x] Icon states (idle/recording/transcribing)

### Milestone 2.2: Recording Overlay ✅ COMPLETED
- [x] Always-on-top window
- [x] Recording timer (mm:ss)
- [x] Audio level meter
- [x] Stop/Cancel buttons
- [x] Show only during recording/transcribing

### Milestone 2.3: Transcribing State ✅ COMPLETED
- [x] Spinner animation
- [x] "Transcribing..." text
- [x] Non-interruptible (MVP)

---

## Phase 3: Settings

### Milestone 3.1: Settings Dialog
- [ ] Hotkey picker
- [ ] Paste shortcut picker
- [ ] Model selection (small/medium/large)
- [ ] Device selection (CUDA/CPU)
- [ ] Restore clipboard toggle

### Milestone 3.2: Config Persistence
- [ ] Load config on startup
- [ ] Save config on settings change
- [ ] Config file in %AppData%

---

## Phase 4: Robustness ✅ COMPLETED

### Milestone 4.1: Error Handling ✅ COMPLETED
- [x] No microphone error detection with user-friendly messages
- [x] CUDA failure → CPU fallback with automatic detection
- [x] Transcription timeout (60s) with detailed error messages
- [x] Empty transcript handling with helpful feedback

### Milestone 4.2: Edge Cases ✅ COMPLETED
- [x] Double hotkey press protection (300ms debouncing)
- [x] App already recording state prevention
- [x] Clipboard non-text content (save/restore all data formats)

### Milestone 4.3: Logging ✅ COMPLETED
- [x] Debug logging to file (Logger utility class)
- [x] Error reporting with detailed context
- [x] VOICEPASTE_DEBUG environment variable support

---

## Phase 5: Portable Build & Distribution

### Milestone 5.1: Portable Build ✅ COMPLETED
- [x] Self-contained .NET build (no runtime needed)
- [x] Embedded Python 3.12 support
- [x] PowerShell build script with Python inclusion
- [x] Batch build script (basic)
- [x] Automatic pip and faster-whisper installation
- [x] Portable launcher script
- [x] Documentation for portable deployment

### Milestone 5.2: Packaging
- [ ] Model pre-caching in portable build
- [ ] Compression/archiving of build
- [ ] Build verification tests
- [ ] Size optimization

### Milestone 5.3: Installer (Future)
- [ ] MSI or MSIX installer
- [ ] Start menu shortcuts
- [ ] Auto-start option

---

## Validation Checklist

### Core Functionality
- [ ] ScrollLock starts recording
- [ ] Recording continues during silence (5+ seconds)
- [ ] ScrollLock stops and transcribes
- [ ] Text pastes into focused window
- [ ] Works in Windows Terminal with Ctrl+Shift+V

### Bilingual Support
- [ ] English transcription accurate
- [ ] Ukrainian transcription accurate
- [ ] Mixed EN/UK in same recording works

### GPU Support
- [ ] CUDA transcription works
- [ ] Falls back to CPU if CUDA unavailable
- [ ] Reasonable speed on medium model

### Reliability
- [ ] Multiple recordings work back-to-back
- [ ] App stays resident after paste
- [ ] No memory leaks after many recordings
- [ ] Handles long recordings (5+ minutes)
