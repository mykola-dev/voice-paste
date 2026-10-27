# UX Specification

## User Flow

```
[Idle] ──ScrollLock──> [Recording] ──ScrollLock──> [Transcribing] ──auto──> [Idle]
                            │                            │
                           Esc                         (paste)
                            │                            │
                            v                            v
                         [Idle]                    Focused Window
```

### Cancel recording with Escape

- During recording, global **Esc** stops and discards the audio, hides the overlay and returns
  to idle. It does not transcribe, paste, modify the clipboard or show a "no speech" error.
- A `WH_KEYBOARD_LL` hook consumes that Esc press, repeats and matching key-up so the focused
  application does not receive them. The hook is removed outside recording, except while draining
  the release of an already-consumed press. New Esc presses work normally in idle/transcription.
- Esc held before recording starts is passed through until released; a new press is needed to cancel.
- Hook callbacks run on a dedicated message-pump thread and queue cancellation onto the UI thread.
  No audio/model/file operations run in the hook. Queued requests are tied to their recording session.
- If the hook cannot be installed, recording is aborted and the existing error UI reports it.
- This covers ordinary desktop keyboard input, not UAC/lock-screen secure desktops or every
  raw-input/system-key mechanism. It is not an unconditional system-wide keyboard-security boundary.

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
│          ● 00:12           │
│  ████████░░░░░░░░░░        │
└────────────────────────────┘
```
- Pulsing red recording indicator and timer only; no visible "Recording" label.
- Timer (mm:ss format)
- Recording animation selectable in Settings: **Level bar** (default) or **Waveform**.
- Both use additional display-only gain of 5x over the existing RMS input, clamped to full scale.
  Peaks rise immediately on the next frame, with no attack ramp; falling levels use a 140 ms
  release time constant. Microphone buffers are 30 ms; rendering follows WPF composition frames
  (typically ~60 fps) rather than a separate timer. These capture/render
  intervals still impose natural latency; "instant rise" means no artificial attack smoothing.
- Waveform shows a continuous, vertically centered filled silhouette in one configurable color,
  with smooth cubic contours and no columns, gaps, color gradient or outline.
  Local opacity follows the displayed amplitude: 50% for silence, rising linearly to 100% at 90%
  displayed amplitude and remaining opaque above that. Each fragment retains its opacity as it
  travels. There is no whole-wave audio-reactive opacity multiplier: live input does not dim or
  brighten older history. A reusable premultiplied bitmap replaces per-frame WPF geometry/gradient-stop
  objects; local alpha is folded into its pixels without an extra WPF opacity layer.
  Premultiplied BGRA pixels are stored as packed 32-bit values, without changing contour math,
  opacity, rounding, or edge coverage.
  Steady-state frames update the bitmap referenced by a retained ImageDrawing, without invalidating
  the element's layout or reopening its drawing every frame. Live uploads use a nonblocking bitmap
  lock: if the compositor is still copying, keep the previous image and retry with current history
  on the next frame rather than blocking the UI or queueing stale images. Initial drawing, resizing,
  reset and color changes still publish the required surface normally.
  It is rendered at 2x the monitor's physical pixel resolution in each axis, with subpixel edge
  coverage and high-quality downsampling; size/DPI changes rebuild the surface. This improves
  contour antialiasing without blurring the whole waveform or increasing its on-screen size.
  After the shared meter response, extra height gain rises smoothly from 1x to 2x over input levels
  0–10% using smoothstep (`t*t*(3-2*t)`, `t = clamp(level / 0.1)`). At/above 10% it remains the
  previous linear 2x response, clamped to full height. This is a low-level gain ramp, not an
  exponential amplitude curve or noise gate. The minimum silhouette thickness is independently
  fixed at 1 DIP, with subpixel antialiasing, so gain never thickens digital silence.
   Its drawing area is 30 DIPs high, placed 12 DIPs below the header; the taller window prevents clipping.
   The waveform spans the full window width, without horizontal padding or an inset frame;
   the legacy level bar retains its 16-DIP horizontal padding inside the window border.
   No horizontal edge fades are applied; local amplitude-dependent opacity remains unchanged.
  The right edge is live; timestamped volume history travels right to left (~3.2 s visible).
  Historical points keep their height and move at a constant speed using fractional coordinates,
  not fixed slots or 60 ms steps. A bounded audio/UI queue preserves individual callback timestamps
  and short peaks; held input is not duplicated into history on every presentation frame. Release
  uses measurement-time ballistics with a noncommitted live-edge preview between measurements.
  Shape-preserving cubic Hermite interpolation uses neighboring slopes, with no mandatory horizontal
  tangent at every rising measurement and no overshoot. Height and opacity use the same interpolated
  amplitude. Only the newest interval may refine when its next neighbor arrives; older intervals
  simply translate. The bounded history retains two offscreen predecessors to preserve clip-edge
  tangents. Actual 30 ms measurements still limit temporal detail; real plateaus are preserved.
  Silence leaves a thin baseline. This is real volume history, not a frequency spectrum.
- Neither style changes recorded PCM or performs automatic gain control. Rendering stops and
  both visualizers reset outside recording; switching styles clears previous history.
- Settings → Overlay → Indicator color opens the standard Windows color chooser. The saved
  color applies to both recording visualizers and the transcription/download spinner; the
  recording-status dot remains red. There is no fixed horizontal color/fade gradient.
- Configured hotkey (ScrollLock by default) = stop and transcribe.
- Esc = discard recording; the click-through overlay has no Stop/Cancel buttons.

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
