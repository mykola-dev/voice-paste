# VoicePaste Documentation Index

Quick navigation for LLM agents working on this project.

## Documents

| File | Purpose | Read When |
|------|---------|-----------|
| [01-overview.md](01-overview.md) | Project goals, requirements, decisions | Starting work, understanding scope |
| [02-architecture.md](02-architecture.md) | System components, tech stack | Designing features, understanding structure |
| [03-ux-spec.md](03-ux-spec.md) | UI/UX behavior, user flows | Implementing UI, overlay, tray |
| [04-audio-capture.md](04-audio-capture.md) | WASAPI recording, audio format | Working on recording pipeline |
| [05-transcription.md](05-transcription.md) | Whisper/faster-whisper, GPU/CPU | Working on STT integration |
| [06-paste-mechanism.md](06-paste-mechanism.md) | Clipboard, SendInput, shortcuts | Implementing paste functionality |
| [07-configuration.md](07-configuration.md) | Settings, config file, defaults | Adding settings, config changes |
| [08-milestones.md](08-milestones.md) | Dev phases, MVP checklist | Planning work, tracking progress |
| [09-portable-build.md](09-portable-build.md) | Portable deployment, build scripts | Creating deployable builds |
| [10-testing.md](10-testing.md) | Test suite, coverage, CI/CD | Writing and running tests |

## Quick Reference

**Core Flow:** Hotkey → Record → Transcribe → Paste

**Tech Stack:**
- C# WPF (tray app + overlay)
- Python faster-whisper (STT worker)
- WASAPI (audio capture)
- CUDA/CTranslate2 (GPU acceleration)

**Key Files:**
- `src/app/` - C# WPF application
- `src/transcribe/transcribe.py` - Python STT worker
- `config/` - Default configuration

**Critical Requirements:**
1. No auto-stop on silence (record until manual stop)
2. GPU-first, CPU fallback
3. Auto-detect EN/UK language
4. Paste into currently focused window
5. **Portable build requirement**: Must be deployable as a self-contained folder that works on any Windows PC without installation
