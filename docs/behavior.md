# Application behavior contract

## Dictation and paste

- Resident tray app without a normal-use console. Hotkey toggles manual recording;
  silence never auto-stops it. Debounce is 300 ms; busy transcription ignores toggle.
- UI hotkey choices: ScrollLock, Numpad0, Right Alt, Right Ctrl, F8 and F12. Preserve
  existing parser-supported config combinations, conflict handling and rollback.
- Escape cancels only the current recording, consuming the relevant key sequence;
  cancellation must not transcribe, paste or change the clipboard.
- Capture the target focus before recording. The overlay must never activate or
  steal focus. Restore the target before paste; on restoration failure, use current
  focus. Click-through is optional, not a new research requirement.
- Unicode clipboard plus Ctrl+V, Ctrl+Shift+V or Shift+Insert. Do not restore the
  old clipboard; failed injection leaves the result available for manual paste.
  Clean up injected modifiers and report errors rather than pretending success.
- Startup/audio/STT/no-speech errors return to a usable state. Stale session/worker
  results must never mutate the clipboard or paste.

## Audio, model and worker policy

- Default microphone; transcription input is 16 kHz mono PCM16 WAV. Display gain
  never changes saved PCM. Stream capture to owned temporary files and clean up
  after success, failure or cancel; no fixed recording-duration/silence timeout.
- Models: `large-v2` and `large-v3-turbo`, pinned by the core model catalog.
  Fresh startup provisions only large-v2; alternatives require explicit save or
  provisioning request. Dropdown edits and availability checks are read-only.
- CPU stays CPU; CUDA-only fails instead of silently falling back. Auto mode may
  fall back on eligible errors and reports the actual device.
- Keep-in-memory retains/preloads the worker; disabling it releases after use.
  Settings/model changes apply at idle, never replace an active session's policy.
- English, Ukrainian, bilingual and automatic language modes; preserve VAD,
  language/token suppression, prompt combination, Ukrainian cleanup, decoding
  fallback/seek and long-audio policy. Beam size is 1–10. No accuracy/model/VAD/
  residency reductions merely to manufacture memory savings.
- IPC is serialized, bounded, versioned and session/request/generation-tagged;
  readiness/result deadlines, bounded stderr, process ownership and shutdown are
  explicit. Remaining lifecycle defects/qualification are in [TODO](../RustMigration/TODO.md).

## Settings and appearance

- One Dark master–detail: Hotkeys, Transcription, Appearance and System; one bold
  page heading and common draft. Footer uses OK / Cancel / Apply with equal 12 DIP
  padding and button gaps. OK saves and closes only on success; Apply saves without
  closing or resetting inputs; Cancel discards only unapplied edits (not prior
  successful Apply). Save errors keep the editable draft open. Preserve input identity, Unicode/IME,
  caret/focus, per-page scroll, window geometry and prompt height (80–600 DIP,
  absent value 140). Physical 175% DPI is the primary acceptance target.
- Model hint directly below Model follows draft selection: “The model is ready”
  or “The model needs to be downloaded”. Queries ignore stale results and never
  load/download/save a model. No permanent model Retry/Cancel controls.
- Left tray click opens Settings; right opens Start/Stop, Settings, Quit menu.
  Menu tracking must not stall recording/busy animation.
- System contains residency, readable host/worker/combined memory with shared-page
  caveat, separate/unavailable GPU memory, logging and optional sign-in startup.
  Startup defaults off, uses the current-user Run value `VoicePasteRust`, applies
  on Save and rolls back on failed config replacement.
- Defaults are [core assets](../crates/core/assets/default-settings.json), not
  legacy docs. No personal prompt is committed; absent prompt defaults to empty.
  Existing app-local prompts/config are preserved verbatim. Save failure retains
  the draft; Cancel does not apply it. Never log prompt/transcript by default.
- Overlay: 224 × 100 logical pixels, top-right work area with 20 DIP margin,
  16 DIP corners, one-pixel border, default indicator `#E9563D`. Keep the sharp
  timer, pulsing red dot, timestamped 3.24-second waveform (capacity 512), 140 ms
  release and selected-color five-bar busy layer independent of the background.
- Acrylic defaults on; one shared transparency defaults to 57%. 0% is opaque white
  tint, intermediate Acrylic retains blur, 100% removes the whole background.
  Unsupported blur uses ordinary translucency at the same alpha. Save applies
  on next overlay show; Cancel has no effect. No live preview is required.
- App-local data only; logs at `%TEMP%/VoicePaste/voicepaste.log`. No automatic
  legacy-config import, AppData fallback, system dependency installation or model
  download as a side effect of build/check/UI editing.
