# Rust architecture

## Components and dependency direction

- `crates/core`: pure state transitions, settings validation/defaults, model
  catalog and waveform policies; no GUI, OS, IO or native inference dependency.
- `crates/application`: dictation/settings workflows and consumer-owned ports;
  depends on core, not concrete adapters.
- `crates/protocol`: bounded versioned UTF-8 host/worker IPC and request identities.
- `apps/voicepaste`: composition root/runtime, GPUI Settings, Windows tray,
  hotkeys, audio, storage, model provisioning, clipboard/input and worker owner.
- `apps/stt-worker`: separate resident Rust process, frontend/tokenizer/VAD/decoding
  pipeline and narrow C++ C ABI adapter to CTranslate2 **4.7.1**. Only the worker
  loads CT2/CUDA/ONNX Runtime. Normal use requires no Python or .NET.

Concrete adapters depend on inner contracts and are wired explicitly. Resource
owners handle cancellation, stale completions, bounded queues and shutdown.
See [engineering rules](engineering-principles.md).

## UI and platform boundaries

GPUI Kit/component **0.8.1**, matching GPUI-pre **0.3.10**, owns the Settings host
loop/layout/input/rendering. One Dark and IBM Plex Sans assets are embedded;
their provenance/licenses live with the assets. Settings reveal/restore/foreground
requests happen outside framework borrows; avoid synthetic-Alt activation.

The recording overlay is a separate nonactivating GPUI window. Shared presentation
state, layout, text, waveform quads and animation live in `adapters/overlay.rs`
and `adapters/overlay/state.rs`; core supplies a renderer-independent waveform
envelope, not premultiplied platform pixels. GPUI schedules frames, including in
native nested menu loops; inactive-window throttling is disabled for the overlay.
There is no independent Win32 frame timer, DIB/GDI foreground or render thread.

`adapters/overlay/windows.rs` owns only native nonactivation, placement and the
backdrop lifetime. HWND snapshots are taken in GPUI; mutating OS calls happen
after App/entity borrows are released. The existing Windows backdrop uses GPU
Gaussian blur plus white tint, clipped together at 16 DIP by Composition geometry,
behind the transparent, sharp GPUI foreground.
The backdrop must remain below the foreground even after geometry/DPI sync;
placement uses `SWP_NOZORDER`, with explicit stacking only during show. Otherwise
the backdrop samples and blurs the GPUI content itself.
The foreground uses only DirectComposition alpha, not the rectangular native
accent that the pinned GPUI Transparent mode installs by default. Disable that
accent and non-client rendering on both surfaces; the shared GPUI border and
rounded Composition clip are the sole visible frame, without a second native
outline/shadow. Native frame settings are covered by hidden adapter tests, not
by a claim of screenshot/pixel acceptance.
The effect is not an exact recreation of system Acrylic noise, nor replaced with
GPUI's visually different accent blur.
Unsupported effects use the same-alpha shared translucent body without an extra
tint over supported material. Composition visuals/dispatcher belong to the host
UI thread and outlive neither the shell nor its window. Windows is the only
implemented/qualified target; shared rendering is not a claim of working ports.

Keep shared UI state/layout and domain/application free of native handles. Isolate
OS behavior in compile-time-selected modules with target-scoped dependencies and
narrow consumer-owned boundaries. Linux/macOS are future targets: no implemented
port, fake-success adapter or speculative platform/plugin framework is intended.
Remaining platform isolation/qualification is tracked in [TODO](../RustMigration/TODO.md).

## Storage and build inputs

Settings, models and temporary session data are under `<exe-dir>/data/`;
diagnostics are `%TEMP%/VoicePaste/voicepaste.log`. Existing config is never reseeded
on upgrade. Public defaults and the pinned model catalog live in
`crates/core/assets/`; private prompts belong only in app-local configuration.

Transcription measurements are pure core types. The resident worker separately
measures engine loading, VAD filtering, log-mel preparation and the prepared-input-
to-final-text interval, returning numeric metrics over versioned IPC. The host
worker adapter validates request identities and exports numeric metadata; the
dictation runtime persists validated Ready loads and only current-session transcription samples
through the SQLite adapter to `data/transcription-stats.sqlite`. Its connection
belongs to the dictation thread; Settings requests read-only filtered
summaries through runtime commands, never performs database IO on the UI thread,
and rejects summaries for a stale draft. The bundled SQLite library requires no
installed runtime. Statistics failure does not fail transcription or paste.

`cargo xtask` owns native preparation, checks and packaging; the root BAT only
launches the same Rust task used on CI. Its native lock is `xtask/native-inputs.json`.
Complete runnable profiles live only in `build/debug/` and `build/release/`;
archives live in `build/package/`. Cargo/native/tooling/staging files belong to `build/cache/`.
No additional shell interpreter, Python installation or legacy application supplies build logic.
See the [worker prerequisites](../apps/stt-worker/README.md).
Reproducible CUDA source builds, complete redistribution inventory and clean-machine
qualification are still pending; a successful local build does not establish them.
