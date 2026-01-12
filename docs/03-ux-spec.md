# UX Specification

## User Flow

```
[Idle] ──ScrollLock──> [Recording] ──ScrollLock──> [Transcribing] ──auto──> [Idle]
                            │                            │
                          Cancel                       (paste)
                            │                            │
                            v                            v
                         [Idle]                    Focused Window
```

## Overlay Window

### Appearance
- Size: ~320x120 pixels
- Position: Top-right corner of screen
- Always on top
- Semi-transparent background
- Modern, minimal design

### States

**Recording**
```
┌────────────────────────────┐
│  ● Recording    00:12      │
│  ████████░░░░░░░░░░        │
│  [Stop]         [Cancel]   │
└────────────────────────────┘
```
- Red recording indicator
- Timer (mm:ss format)
- Audio level meter (RMS visualization)
- Stop button = same as hotkey
- Cancel button = discard recording

**Transcribing**
```
┌────────────────────────────┐
│  ⟳ Transcribing...         │
│                            │
│                            │
└────────────────────────────┘
```
- Spinner animation
- No buttons (non-interruptible in MVP)

### Visibility Rules
- Hidden during idle state
- Appears on recording start
- Stays visible during transcription
- Hides after paste completes

## Tray Icon

### Icon States
- Idle: Microphone icon (gray)
- Recording: Microphone icon (red/animated)
- Transcribing: Microphone icon (yellow)

### Context Menu
```
┌─────────────────┐
│ Start/Stop      │  ← Same as hotkey
│ ─────────────── │
│ Settings...     │
│ ─────────────── │
│ Quit            │
└─────────────────┘
```

## Settings Dialog

### Layout
```
┌─────────────────────────────────────────┐
│  VoicePaste Settings                    │
├─────────────────────────────────────────┤
│  Hotkey:        [ScrollLock    ▼]       │
│  Paste Key:     [Ctrl+Shift+V  ▼]       │
│                                         │
│  ─────────────────────────────────────  │
│                                         │
│  Model:         [medium        ▼]       │
│  Device:        [CUDA (Auto)   ▼]       │
│                                         │
│  ─────────────────────────────────────  │
│                                         │
│  ☑ Restore clipboard after paste        │
│                                         │
│            [Save]      [Cancel]         │
└─────────────────────────────────────────┘
```

### Hotkey Options
- ScrollLock (default)
- F8, F9, F10
- Ctrl+Alt+Space
- Custom combination

### Paste Shortcut Options
- Ctrl+Shift+V (default, for terminals)
- Ctrl+V
- Shift+Insert

### Device Options
- CUDA (Auto) - try GPU, fallback to CPU
- CUDA - GPU only, fail if unavailable
- CPU - always use CPU

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| ScrollLock | Toggle recording (configurable) |
| Esc | Cancel recording (when overlay focused) |

## Error States

**CUDA Unavailable**
- Show tray notification: "GPU unavailable, using CPU"
- Continue with CPU transcription

**Transcription Failed**
- Show tray notification with error
- Don't paste anything
- Return to idle

**No Microphone**
- Show settings with error message
- Prevent recording until resolved
