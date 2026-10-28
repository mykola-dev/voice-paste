# Transparent overlay rendering

> Historical WPF rendering notes. The current overlay is native Rust/Windows; see [current documentation](README.md).

## Decision

Use WPF software rendering **only for the 224×100-DIP transparent overlay**. Set the window's
`HwndTarget.RenderMode` at `SourceInitialized`, resolving the source from the HWND: the visual
is not necessarily connected yet, so `PresentationSource.FromVisual` can still return null.
Do not set `RenderOptions.ProcessRenderMode`, alter CUDA, change timer resolution/priority, or
rewrite the approved waveform interpolation/rasterizer.

`VOICEPASTE_OVERLAY_SOFTWARE_RENDERING=0` before launch restores WPF's default rendering for A/B.

## Production evidence (2026-10-27)

On a reference Windows machine with an NVIDIA GPU, a 120-Hz display and 150% DPI:

- Hardware-eligible portable recordings sometimes degraded from ~120 to 60–100 rendering
  callbacks/s, although the waveform callback generally took at most ~0.2 ms.
- Another recording contained 590/421/294/271-ms frame gaps that coincided with long Dispatcher
  operations at `Render` priority, outside the measured waveform callback. No stack trace was
  captured during those particular long pauses; their exact cause remains unproven.
- EventPipe sampling during a later degraded recording showed the UI thread mostly waiting in
  `Dispatcher.GetMessage` during the available slow-frame examples, not computing the waveform.
- Native ETW in the actual portable process showed the WPF render thread using
  `GetRenderTargetData → ReadIntoSysMemBuffer → PresentWithGDI → UpdateLayeredWindow` and NVIDIA
  D3D9 driver work. The largest frame gap in that trace (~29 ms) coincided with ~28 ms in
  `DwmFlush → CRenderTargetManager::WaitForDwm`. Other gaps included render-thread waits for work.
  This identifies presentation/scheduling as a relevant path, not proof that every gap has the
  same cause or that the GPU driver itself is defective.
- With only the overlay switched to `SoftwareOnly`, the first ~63-second portable recording
  mostly held ~120 callbacks/s and the user reported improvement. It still had isolated
  ~37/32-ms gaps and one ~2-second half-rate episode, so this was not a complete elimination of
  all possible stutter.
- A subsequent ~66-second software recording under ETW held ~120 callbacks/s after startup,
  and the user reported smooth scrolling. Because a system trace can affect timing, it was
  followed by an **untraced** recording on the same, already-running process.
- That untraced ~40-second recording also held ~120 callbacks/s after startup (window p95 gaps
  ~8.5–8.7 ms), with two estimated missed callback slots confined to the initial ~25-ms gap.
  The user again reported smooth scrolling. The software mode was then made the overlay default.

## Limits and follow-up

This is a validated local mitigation, not a promise of zero dropped display frames on every
machine. The approved waveform's contour, gain, opacity, timestamped audio history, supersampled
bitmap, and nonblocking publication are unchanged. A busy bitmap can still skip an upload;
callback counts and successful uploads are not measurements of physical presentation.

If stutter returns, preserve the executable/build ID, effective window render mode, and
timestamps. Correlate with thread-time/graphics ETW during the same recording;
do not infer a blocked UI thread from callback gaps alone. Keep native system traces local.
