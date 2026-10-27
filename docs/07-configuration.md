# Configuration

Settings are saved to `%AppData%/VoicePaste/config.json`. Open **Settings** from the tray
and press **Save** to apply changes. Cancel leaves the running configuration unchanged.

## Cancel recording

**Esc** cancels an active recording globally, discards the temporary audio and hides the overlay,
without transcription or pasting. No extra setting is required. The consumed press, its repeats
and release do not reach the focused desktop application. A fresh Esc press outside recording
works normally, including during transcription (it does not cancel transcription).

The keyboard hook runs on its own thread, independently of UI/model cleanup, and is removed when
no longer needed. UAC/lock-screen secure desktops and special raw-input/system mechanisms are
outside this guarantee. If Esc was already held when recording started, release it and press again.

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
  Waveform's extra height gain eases from 1x to 2x over the lowest 0–10% of the shared level response
  using smoothstep, then remains at 2x (clamped). Above that range the linear amplitude response is
  unchanged; there is no exponential amplitude remapping or hard noise gate.
  There are no columns or gaps. Timestamped points translate at a constant speed between frames,
  with smooth contours and ~1.6 seconds of visible history; silence leaves a 1-DIP baseline,
  independently of gain (the physical pixel count depends on monitor DPI and edge coverage).
  Points follow actual audio callback times, not repeated held values from every render frame.
  Shape-preserving cubic interpolation joins rising measurements without forcing a flat tangent
  at each point or overshooting peaks; the newest interval can refine as its next neighbor arrives.
  The live edge remains immediate, with no interpolation lookahead or added attack delay.
  Opacity varies locally from 50% at silence to 100% at 90% displayed amplitude, staying at 100%
  above that. Opacity is calculated from the same interpolated amplitude as the contour, so
  transitions follow its shape rather than separate opacity steps and travel with each fragment.
  The experimental whole-wave pulse is removed: live input no longer changes older segments'
  opacity. There is no position-based left/right fade.
  Rendering uses a reusable DPI-aware surface at 2x physical resolution per axis, then downsamples
  with edge antialiasing. Local alpha is included in the pixels, avoiding hundreds of WPF gradient
  objects per frame or an extra WPF opacity layer. Size and movement speed are unchanged;
  no quality setting is needed.
  Recording frames reuse the retained drawing and update its bitmap directly, without redoing WPF
  layout. Bitmap uploads do not wait for a busy compositor; the next frame uses current history,
  not a queued backlog. This does not reduce supersampling quality or alter the waveform curve.

Level bar is the default for old configs, missing fields and unrecognized animation values.
Unknown animation values do not reset unrelated configuration. Save applies the selected style
immediately, including during recording, without restarting the model or recording timer.
Cancel does not apply a selection. The transcription/download spinner is unchanged.

Both styles have immediate rise (no attack ramp), a 140 ms release time constant and the same
display-only gain. Capture uses 30 ms buffers; rendering follows WPF composition frames
(typically 60 fps) only while recording, with no independent animation timer.
The overlay is 224 × 100 DIPs, with a 30-DIP waveform area and more spacing below the header.
Its recording header shows only the pulsing red dot and timer, without the "Recording" label.
The transcription spinner centers in its own layer, unaffected by collapsed recording controls.
Neither style changes recorded sound or adds automatic gain control.

### Production frame diagnostics

The real tray application now enables recording-frame diagnostics by default (including portable
Release builds). No test window or special launcher is required. After rebuilding/restarting,
record through the lag for 20–30 seconds and stop normally. The separate JSON-lines log is:

```text
%TEMP%\VoicePaste\overlay-frames-<PID>.jsonl
```

`%TEMP%\VoicePaste\voicepaste.log` also prints the exact diagnostics path on application startup.
The log records the executable/assembly path and module build ID to distinguish an outdated
portable binary from the tested build. Each recording has a separate session ID, a start/stop
entry, and a metrics entry about every two seconds:

- `CallbackFps`, gap p50/p95/max, gaps over 50/100 ms, duplicate callbacks, WPF presentation-clock
  gap and UI timer heartbeat gap.
- `EstimatedMissedCallbacks`: estimated missed WPF callback slots, **not measured GPU/display
  dropped frames**. The default baseline is the fast portion of up to 120 startup WPF
  presentation-clock intervals (not jitter in wall-clock callback arrival),
  frozen afterward so sustained slowdown cannot redefine itself as normal. If the app is slow
  from the start, set `VOICEPASTE_FRAME_TARGET_FPS` to the expected cadence (for example `120`)
  before launching. `OngoingEstimatedMissedCallbacks` describes a still-open no-frame gap;
  it overlaps across reports and must not be added to completed-gap counts.
- UI handler, envelope processing, rasterization, bitmap-lock/copy/unlock, and `OnRender` mean/max
  times. `BitmapBusySkips` counts failed nonblocking uploads independently of callback gaps.
  `BitmapUploads` does not prove physical presentation.
- `Context.Dispatcher` passively observes **all** dispatched UI operations while recording, not
  only our rendering callback. It reports native UI thread ID/CPU-time delta, completed outer
  operation wall time, longest operation and priority, up to eight operations over 2 ms, plus
  elapsed gaps outside observed operations. An operation still open during a report is identified
  separately; a completed operation crossing a reporting boundary appears in the next report.
  Nested message pumps do not double-count outer operation time. A gap outside operations is
  **not proof of idle time**: native window-message processing/waits and OS scheduling are outside
  these hooks. Compare it with UI CPU time and, if necessary, an ETW thread-time trace. No extra
  polling timer, render timer, timer-resolution request, or priority change is introduced.
- Actual bitmap dimensions/buffer bytes/generation, history size, audio callback count/age/gaps,
  pending/drained/overflow/discarded audio measurements, and capture/presentation clock lead.
- DPI, window position, WPF rendering tier/mode, process RAM/CPU and GC counts. Up to eight slow
  frame examples per reporting window include individual stage timings.

All file writes, process sampling, percentile calculations, and JSON formatting run in a bounded
background consumer, not the recording callback. The log rotates at 8 MiB with one `.previous`
backup per PID; no waveform samples, recorded audio, transcribed text or clipboard data are logged.
Set `VOICEPASTE_FRAME_DIAGNOSTICS=0` before launch to disable logging. Callback metrics alone do not
prove physical display presentation.

The small transparent overlay defaults to `HwndTarget.RenderMode = SoftwareOnly`. Production
ETW showed the hardware layered-window path reading back GPU surfaces through D3D/GDI before
presentation; a per-window software-rendering A/B improved actual portable recordings without
changing the waveform. Other windows, bitmap dimensions/quality, and transcription/CUDA settings
are unchanged. `Context.WindowRenderMode` confirms the actual mode after the HWND is created.
For a controlled comparison, set `VOICEPASTE_OVERLAY_SOFTWARE_RENDERING=0` **before launching**
to restore WPF's default hardware-eligible mode. Unset it (or use `1`) and restart to return to
the software overlay. See [Overlay rendering investigation](10-overlay-rendering.md) for the
evidence and remaining limitations.

### Waveform performance diagnostics (development)

Offscreen cost/allocation checks, including raster sizes at 100%, 150% and 200% DPI:

```powershell
$env:VOICEPASTE_BENCHMARK_WAVEFORM = '1'
dotnet test tests/VoicePaste.Tests/VoicePaste.Tests.csproj -c Release --filter 'FullyQualifiedName~WaveformPerformanceTests' --logger 'console;verbosity=detailed'
```

For real WPF composition over time, an opt-in test shows a separate nonactivating, click-through
overlay at the lower left, feeds synthetic audio levels, and reports frame-callback intervals,
update costs, bitmap identity/dimensions, history size, process memory and GC counts every five
seconds. It does not use the microphone, transcribe, paste, or stop the running application:

```powershell
$env:VOICEPASTE_LIVE_WAVEFORM_SECONDS = '30'
dotnet test tests/VoicePaste.Tests/VoicePaste.Tests.csproj -c Release --filter 'FullyQualifiedName~WaveformLiveDiagnosticsTests' --logger 'console;verbosity=detailed'
```

The duration is bounded to 10–120 seconds; by default this test opens no window. Callback intervals
are not a measurement of physical display presentation. If intermittent stalls recur, compare
these live results rather than inferring a bitmap/history leak from short offscreen benchmarks.
To verify the production logger in this synthetic window, also set `VOICEPASTE_LIVE_FRAME_LOG` to
an output JSONL path. Optionally set `VOICEPASTE_LIVE_STALL_MS=200` to inject one deliberate test-only
UI stall and verify that it appears as a callback gap/estimated misses in that log. This option has
no effect on the real tray application.

## Indicator color

**Overlay → Indicator color → Choose...** opens the standard Windows color dialog, owned by
Settings. No additional UI package is required. The swatch and hex label show the current choice.
**Reset** selects the original orange; press **Save** to apply either change. Cancelling the color
dialog keeps the previous choice; cancelling Settings does not apply it.

`"indicatorColor": "#FF8C00"` is the default, including for older configs. Values are opaque
`#RRGGBB` colors; invalid values fall back to orange. The saved color applies immediately to the
level bar, waveform and transcription/download spinner, without restarting the worker.
The red recording-status dot is intentionally unchanged.
