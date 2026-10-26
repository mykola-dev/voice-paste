# Configuration

Settings are saved to `%AppData%/VoicePaste/config.json`. Open **Settings** from the tray
and press **Save** to apply changes. Cancel leaves the running configuration unchanged.

## Keep in memory

```json
{
  "model": "large-v3-turbo",
  "device": "cudaAuto",
  "keepInMemory": false
}
```

**Keep in memory** is off by default, including when the field is absent in an older config.
When enabled, the selected transcription model loads in the background and stays in RAM/VRAM
between recordings. When disabled, the model preloads during recording and unloads after transcription.
Disabling or changing transcription settings during a dictation takes effect after it finishes.
Exiting VoicePaste always releases the worker.

Residency reduces repeated loading/initialization, especially for short dictations, but occupies
memory while idle and may reduce memory available to games. CUDA uses FP16; CPU uses INT8.

The live footprint below the checkbox reports the **running** worker's resident RAM and, when
available, dedicated CUDA VRAM. It includes runtime overhead and buffers, not only model weights.
Measurements change during transcription. When per-process GPU queries are unsupported,
the VRAM entry is hidden; CPU mode shows RAM only. A different selection is not loaded until Save.

See [Transcription](05-transcription.md) for worker lifecycle and measurement details.

## Recording animation

Under **Overlay → Recording animation**, choose:

- **Level bar** (`"recordingAnimation": "levelBar"`) — the original horizontal level meter.
- **Waveform** (`"recordingAnimation": "waveform"`) — a continuous filled volume envelope, smoothly
  moving right to left, with a live right edge and a single configurable color (no color gradient).
  Waveform has an additional 2x height gain over the shared level response.
  There are no columns or gaps. Timestamped points translate at a constant speed between frames,
  with smooth contours and ~1.6 seconds of visible history; silence leaves a thin baseline.
  Opacity varies locally from 50% at silence to 100% at full displayed amplitude. Intermediate
  levels blend smoothly, and transparency travels with each fragment rather than changing the
  entire waveform at once. There is no position-based left/right fade.
  Rendering uses a reusable DPI-aware surface at 2x physical resolution per axis, then downsamples
  with edge antialiasing. Local alpha is included in the pixels, avoiding hundreds of WPF gradient
  objects per frame. Size and movement speed are unchanged; no quality setting is needed.

Level bar is the default for old configs, missing fields and unrecognized animation values.
Unknown animation values do not reset unrelated configuration. Save applies the selected style
immediately, including during recording, without restarting the model or recording timer.
Cancel does not apply a selection. The transcription/download spinner is unchanged.

Both styles have immediate rise (no attack ramp), a 140 ms release time constant and the same
display-only gain. Capture uses 30 ms buffers; rendering follows WPF composition frames
(typically 60 fps) only while recording, with no independent animation timer.
The overlay is 224 × 100 DIPs, with a 30-DIP waveform area and more spacing below the header.
The transcription spinner centers in its own layer, unaffected by collapsed recording controls.
Neither style changes recorded sound or adds automatic gain control.

## Indicator color

**Overlay → Indicator color → Choose...** opens the standard Windows color dialog, owned by
Settings. No additional UI package is required. The swatch and hex label show the current choice.
**Reset** selects the original orange; press **Save** to apply either change. Cancelling the color
dialog keeps the previous choice; cancelling Settings does not apply it.

`"indicatorColor": "#FF8C00"` is the default, including for older configs. Values are opaque
`#RRGGBB` colors; invalid values fall back to orange. The saved color applies immediately to the
level bar, waveform and transcription/download spinner, without restarting the worker.
The red recording-status dot is intentionally unchanged.
