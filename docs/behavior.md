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
- Models: `large-v2`, `large-v3` and `large-v3-turbo`, pinned by the core model catalog.
  The Settings dropdown shows them with a Whisper prefix; saved configs and
  on-disk folders keep the short ids.
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
  explicit. Remaining lifecycle defects/qualification are in [TODO](../TODO.md).

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
- Tray artwork: lips-source2 artwork at idle; open mouth during recording, transcription
  and model preparation. Outside the tray the open-mouth application icon is
  stable. Preserve artwork proportions/corners with no extra inset in square assets.
- System orders sections as sign-in startup, residency with readable host/worker
  memory (no combined total), debug logging, then transcription statistics.
  Memory and transcription reports share smaller muted labels and larger values.
  System includes selected model.bin file size from the catalog (not RAM/VRAM usage), logging
  and optional sign-in startup. Model size follows the draft selection without
  downloading files or saving settings; GPU memory is not displayed.
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

## Transcription performance statistics

- Each matching successful, non-empty worker transcription adds a local SQLite
  measurement to `<exe-dir>/data/transcription-stats.sqlite`. Store model/revision,
  beam size, VAD, language mode, actual CPU/CUDA device, timestamp, inference
  microseconds, original 16 kHz audio sample count, word and Unicode character
  counts. Never store transcript, prompt or audio in this database. Failed,
  empty and mismatched results do not contribute; paste failure does not erase a
  successful transcription measurement. Database errors are logged without
  blocking delivery of the transcription.
- Measure with a monotonic worker clock after WAV reading, VAD, feature and
  initial-prompt preparation, immediately before the model's window loop, until
  final text is ready. Include every window, language detection, decoding fallback
  and text cleanup; exclude model loading, transport, clipboard/paste and database IO.
- Also measure VAD filtering and audio-to-log-mel preparation separately on the
  same successful non-empty transcriptions. Disabled VAD is absent, never a zero
  duration. Model loading measures only the successful native STT engine open,
  not worker launch, tokenizer/VAD initialization or downloads; persist once per
  validated worker Ready, including preloads without a transcription. Its count
  and mean are independent of the transcription count and use the same filters.
  Resident reuse never adds zero loading samples. Failed opens do not contribute.
- Upgrade the local SQLite schema transactionally without clearing history.
  Historical VAD/preparation timings stay NULL and do not dilute their averages;
  each new phase has its own measurement count. Show `No measurements yet` until
  a phase has data, and `Off` for VAD disabled. Hide model loading time and count
  whenever draft Keep in memory is enabled; continue collecting real loads.
- System displays lifetime count, arithmetic mean transcription milliseconds,
  mean words and input audio seconds, and total inference milliseconds divided by
  total original audio seconds. A word is a whitespace-delimited item containing
  a Unicode letter or digit; characters include spaces and are not UTF-8 bytes.
  Display mean milliseconds, words and audio seconds rounded to integers; keep
  processing milliseconds per audio second at one decimal. The configuration
  line uses the value font; device count rows read `CUDA Transcriptions: 2`
  (or CPU) with the same label/value styling as the memory report.
  VAD, preparation and loading means use integer milliseconds and the same row
  styling. The statistics explanatory paragraph is omitted.
- Queries follow draft model/revision, beam size, VAD, language and device without
  applying settings or provisioning models. Auto shows CPU and CUDA separately;
  CPU/CUDA-only show only their actual device. Configuration changes do not reset
  history. Ignore stale query results and refresh while Settings is open. No-data
  and database-error states are explicit; statistics updates preserve draft inputs.
- The final `Reset Stats` button immediately clears all transcription/phase and
  model-load samples across every model/revision/device/beam/VAD/language, not
  just the current filter. Both tables are deleted in one transaction; failure
  preserves history and displays an error with retry available. Disable duplicate
  reset requests and ignore statistics query completions while reset is pending;
  refresh after success. Reset is independent of Apply/Cancel, never changes
  settings, models or the resident worker, and new completions accumulate normally.
