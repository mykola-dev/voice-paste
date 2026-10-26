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
- Size: 224x100 device-independent pixels (30% narrower than the previous 320-wide overlay)
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
- Recording animation selectable in Settings: **Level bar** (default) or **Waveform**.
- Both use additional display-only gain of 5x over the existing RMS input, clamped to full scale.
  Peaks rise immediately on the next frame, with no attack ramp; falling levels use a 140 ms
  release time constant. Microphone buffers are 30 ms; rendering follows WPF composition frames
  (typically ~60 fps) rather than a separate timer. These capture/render
  intervals still impose natural latency; "instant rise" means no artificial attack smoothing.
- Waveform shows a continuous, vertically centered filled silhouette in one configurable color,
  with smooth cubic contours and no columns, gaps, color gradient or outline.
  Local opacity follows the displayed amplitude: 50% for silence, 100% at full height, with smooth
  transitions between them. Each fragment retains its opacity as it travels; the whole wave does
  not pulse in opacity with the live input.
  A reusable premultiplied bitmap replaces per-frame WPF geometry/gradient-stop objects.
  It is rendered at 2x the monitor's physical pixel resolution in each axis, with subpixel edge
  coverage and high-quality downsampling; size/DPI changes rebuild the surface. This improves
  contour antialiasing without blurring the whole waveform or increasing its on-screen size.
  It adds a separate 2x height gain (clamped) after the shared meter response. Its drawing area
  is 30 DIPs high, placed 12 DIPs below the header; the taller window prevents clipping.
  The right edge is live; timestamped volume history travels right to left (~1.6 s visible).
  Historical points keep their height and move at a constant speed using fractional coordinates,
  not fixed slots or 60 ms steps. The bounded history retains one offscreen predecessor for clipping.
  Silence leaves a thin baseline. This is real volume history, not a frequency spectrum.
- Neither style changes recorded PCM or performs automatic gain control. Rendering stops and
  both visualizers reset outside recording; switching styles clears previous history.
- Settings → Overlay → Indicator color opens the standard Windows color chooser. The saved
  color applies to both recording visualizers and the transcription/download spinner; the
  recording-status dot remains red. There is no fixed horizontal color/fade gradient.
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
- Centered horizontally and vertically in a separate full-content layer, independent of
  the hidden recording header/meter layout.
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
