# Project Overview

## What is VoicePaste?

A Windows resident tray application for voice dictation that:
- Records voice via hotkey toggle
- Transcribes using Whisper (GPU-first)
- Pastes text into the currently focused window

## Core Requirements

### Must Have
1. **No auto-stop on silence** - Recording continues until manual stop
2. **Bilingual auto-detect** - English + Ukrainian without switching
3. **GPU-first transcription** - NVIDIA CUDA, with CPU fallback
4. **Paste to focused window** - Works with any app (terminals, editors, browsers)
5. **Resident tray app** - Always running, global hotkey activation
6. **Portable deployment** - Self-contained build that works on any Windows PC without installation

### User Preferences (Locked)
- Default hotkey: `ScrollLock` (configurable)
- Default paste shortcut: `Ctrl+Shift+V` (configurable)
- Default model: Whisper `large-v3` (best accuracy for RTX 4070 Ti)
- Overlay: Always-on-top, visible only during recording/transcribing
- Auto-paste immediately after transcription (no confirmation)

## Why This Design?

### Problem
Windows built-in dictation (Win+H) stops on silence, which interrupts natural speech with pauses.

### Solution
Manual start/stop control via hotkey - user decides when dictation ends.

### Why Not Modify opencode?
Originally considered integrating with opencode's `/editor` command, but decided to:
- Keep opencode unchanged
- Build standalone tool that works with any application
- Paste directly into focused window via clipboard

## Target Environment

- Windows 10/11
- NVIDIA GPU with CUDA (RTX 4070 Ti or similar for large-v3 model)
- Works with both native Windows and WSL terminals
- Python 3.12 for transcription worker (embedded in portable build)

## Deployment

- **Portable build**: Self-contained folder with embedded Python and all dependencies
- Can be copied to any Windows PC and run without installation
- Includes .NET runtime, Python 3.12, faster-whisper, and all native libraries
- Optionally includes pre-cached Whisper models (~3.5GB with large-v3)
- See `docs/09-portable-build.md` for build instructions

## Non-Goals

- Voice commands (only text dictation)
- Real-time/streaming transcription (batch after stop)
- Cross-platform support (Windows only)
- Cloud STT services (local Whisper only)
