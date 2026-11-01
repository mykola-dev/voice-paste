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

### macOS branch status

On `macOS-2`, local compilation is the accepted test route after the user
confirmed the Rust launch preview opens on the corporate Mac. Downloaded CI
artifacts remain blocked there. The current implementation connects the shared
dictation runtime/application, GPUI Settings/overlay, statistics and explicit
`SettingsPlatform` choices to Application Support persistence and native adapters.
`native/macos-host.m` is a narrow AppKit
C ABI adapter, not a Swift/Python application fork; menu commands/labels reuse
the shared menu description. Native callbacks only coalesce intents; no Rust
callback borrows or Settings IO run in menu tracking. A separately owned,
bounded dictation thread commits local saves and joins outside GPUI on shutdown.
Accepted queued saves are flushed without a dead UI consumer. Startup
errors use a native alert so Finder failures are visible.

`macos-input.m` owns a bounded global event tap, session-tagged Escape cancellation,
foreground snapshot and clipboard/Cmd+V effects. `macos-overlay.m` owns only the
nonactivating foreground policy and separate system-blur/white-tint backdrop;
shared GPUI still owns presentation, waveform and frame sampling. Its material is
AppKit NSVisualEffectView, not an identical Windows Gaussian kernel. Native window
mutations run after App/entity borrows unwind. AVFoundation permission is requested
only on explicit Start; native completion retains native state, never borrowed Rust.

The macOS worker uses target-scoped whisper-rs 0.16.0 / whisper-rs-sys 0.15.0 with
vendored whisper.cpp 1.8.3, embedded Metal shaders and an isolated RAII native
engine boundary. Precomputed-log-mel decoding separates VAD/preparation/inference
measurements. CPU and Metal-only remain distinct; pinned native GPU fallback
diagnostics fail closed before Ready/results. The host validates actual device,
request/session/generation and deadline through the same worker coordinator as
Windows. Shared text/VAD policies preserve prompts, cleanup and segmentation.
GGML and VAD have a separate pinned artifact manifest; startup verifies offline,
and only explicit startup/recording-retry confirmation or `--download-model` enables downloads.
macOS selects the separate GitHub `Models` release; Windows retains its original
HTTPS source. Neither uses Git LFS fallback or installs download tools.
`model_parts.rs` owns ordered raw-part delivery, resumable staging, per-part and
complete artifact verification, bounded reassembly and atomic publication.
Native curl transfers one asset; it does not implement a parallel provisioning lifecycle.
Preparation cancellation carries a generation captured by the progress control,
so delayed clicks/queued commands cannot cancel a replacement model or retry.
Terminal cancellation/results use reliable bounded event delivery; download-error
dialogs never suspend the shared Stop/Escape pump during a live recording.
No fake-success adapters or CT2 artifact reuse are provided.
The Windows host/backend selection and defaults remain unchanged. See
[current preview scope and acceptance](macos-preview.md).

## UI and platform boundaries

`adapters/ui_session.rs` now owns GPUI initialization/platform lifetime and the
outer runtime shutdown owner for both targets. Its `SettingsSession` owns the saved
settings projection, draft generations, pending Save obligation, stale-completion
filtering, Settings reply routing and bounded model/statistics/memory queries.
Native hotkey staging/commit/rollback stays outside App/entity borrows; successful
Save replies update the shared overlay appearance. A closed draft cannot release
an accepted Save or reopen with old saved values while its completion is pending.
The shared frame-wake wait preserves an independent 20-ms runtime deadline.
`RuntimeSession` owns the bounded 64-event reply pump, state/model projection and
shared overlay updates. Runtime borrows end before GPUI updates; native reply
effects run after App borrows unwind. Buffered replies (including accepted Save
completions) are delivered before a disconnect error. Model-status replies queue
an availability refresh for the open draft's selection, retaining that query under
command backpressure. macOS no longer overwrites this readiness hint every tick
with menu/permission/backend status text.
Both targets now execute the same `platform/session.rs` coordinator for native
intents, permission handoff, Settings staging/completion, overlay lifecycle calls
and orderly shutdown. Compile-time-selected session hosts own only native menus,
event taps/hotkey registrations, permission queries, status and tray/DPI effects;
they do not own a second application loop. `RecordingInput` retains the original
permission target, rejects duplicate starts while permission is pending, gives
Cancel precedence and retries it under command backpressure. Quit suppresses new
work while delivering an accepted Save's native commit/rollback before teardown.
Native overlay adapters retain their distinct surface/material primitives.
Boundary-audit details and remaining hardware qualification are tracked in `TODO.md`.
Settings presentation uses a selected data profile for native permission/startup
explanations and backend/catalog weight descriptions, not OS switches in rendering.
`config_publication.rs` owns staged write/sync/cleanup; application Settings owns
startup apply → config commit → rollback ordering. Run-value and LaunchAgent
snapshots/ownership and replacement primitives remain native. A failed staging
creation never authorizes removal of an existing colliding temporary file.
`bootstrap.rs` shares CLI routing, provisioning, offline file transcription and
runtime/session startup. Native entry modules select paths/dependencies; data
preserves the existing check-command, batch-WAV and stdout contracts. File mode
verifies pinned local artifacts on both backends and never enables downloads.

`adapters/model_preparation.rs` owns the shared provisioning thread, bounded
terminal reply, cancellation/join, progress snapshots and cancellable chunked
SHA-256 for both catalogs. Progress explicitly distinguishes cumulative stream
bytes from an external transport's staging file, preserving the existing display
policies without OS switches. Completion is consumed once; success, failure and
unwinding clear download activity. Catalog selection, download approval, transport,
cache locks and receipts stay in the model adapters. One shared artifact scenario
recovers verified complete partials offline, preserves short resume offsets,
resets complete corrupt/oversized staging only with network approval, and verifies
and publishes successful transfers. Failed/interrupted transfers retain staging;
a retry resets a complete corrupt partial instead of appending to it. Exact-size/SHA-256 verification
and verified publication now share one policy: reject non-file paths, sync staged
weights, recheck cancellation immediately before commit, and retain replaced
completed artifacts under unique corrupt-backup paths. The narrow files adapters
own Windows replacement versus macOS hard-link/rename and directory sync. Cache
writer locks remain required; this is not a claim of hostile-writer race immunity
or fully crash-atomic multi-file model publication.

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
The slide/fade uses a fixed transparent viewport extending from the panel's resting
left edge to the work-area right edge. GPUI translates/fades the 224-DIP panel
inside it; Composition applies the same offset/opacity to its panel-sized material
and rounded clip. Neither HWND moves per animation sample, so observed animated
coordinates cannot feed back into the resting position or cross monitor DPI boundaries.
Slide/fade is sampled once in the GPUI render alongside the waveform, pulse and
busy bars, all from monotonic elapsed time. Next-frame callbacks use the current
window display's `refresh_interval` to gate compositor callbacks (including after
a display/rate change), not a hardcoded FPS or the application's 20-ms poll.
A UI-thread latest-frame notification wakes native lifecycle synchronization
after framework borrows unwind; it coalesces frames without an unbounded queue.
Runtime polling retains its independent deadline. The material consumes the last
rendered sample, never advances its own timeline, and final exit frames trigger
native hide. Callback pacing is not proof of actual presented FPS: the pinned
Windows backend uses DWM-wide vsync, so physical 120/60-Hz mixed-monitor cadence
still requires measurement/visual acceptance.

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
The Windows adapter also explicitly replaces the pinned backend's zero/overlapped
creation style with frameless `WS_POPUP` and refreshes non-client metrics before
show. Disabling DWM decoration alone leaves caption-side/bottom client insets;
the native client and outer viewport must coincide so the GPUI panel and rounded
material have the same height and origin.
Before each show, cross-monitor placement is two synchronous native operations
while hidden: move without resizing so GPUI can finish `WM_DPICHANGED` and its
nested suggested-RECT resize, then apply the canonical target-DPI viewport. A
single move+resize can have its final size overwritten by that DPI handler, so
neither material creation nor swapchain warming/show may precede the second step.
The native regression uses hidden windows on real mixed-DPI monitors and mirrors
the pinned GPUI suggested-RECT handler; it is not live GPUI visual acceptance.
Cold-start show additionally warms the hidden surface, then enforces the
canonical fixed-popup viewport for the current DPI through the normal `WM_SIZE`
path before revealing the material. The Composition material/clip stay
panel-sized in both dimensions, so an oversized host cannot stretch the blur.
The effect is not an exact recreation of system Acrylic noise, nor replaced with
GPUI's visually different accent blur.
Unsupported effects use the same-alpha shared translucent body without an extra
tint over supported material. Composition visuals/dispatcher belong to the host
UI thread and outlive neither the shell nor its window. macOS selects its own
AppKit shell/material described above; hardware qualification of each platform
remains separate from shared-rendering contracts.

Keep shared UI state/layout and domain/application free of native handles. Isolate
OS behavior in compile-time-selected modules with target-scoped dependencies and
narrow consumer-owned boundaries. Apple Silicon macOS implementation is authorized
on `macOS-2`; Linux remains a future target. No fake-success adapter or speculative
platform/plugin framework is intended.
Remaining reliability/release qualification is tracked in [TODO](../TODO.md).
Feature inventory and staged platform isolation are in
[cross-platform readiness](cross-platform-roadmap.md). `platform/menu.rs` owns
the complete menu description and typed commands; `platform/inbox.rs` owns the
bounded batch policy (neither depends on GUI/OS APIs). `platform/windows.rs`
renders the native menu and activates Settings. `platform/windows/host.rs` owns
the hidden HWND, icons, timer, hook and staged hotkey registrations; its handles
and bindings are private. `platform/windows/events.rs` bridges native callbacks
to a UI-thread-local inbox without entering GPUI or borrowing the host/runtime.
Hotkeys retain FIFO order up to eight pending entries, excess toggles set an
overflow flag; Quit/Cancel have separate slots. Draining takes the complete batch
and resets the inbox before any application work or native modal call.
If native posting fails, same-UI-thread callbacks retain hotkey/Cancel/tray work
directly in that same bounded inbox and coalesce a warning for the next host drain.
Overflow still rejects excess toggles explicitly. No callback performs IO or UI
activation. Detach clears its inbox and Escape state. This remains a single-host,
single-UI-thread callback bridge, not multi-instance process-global coordination.

`adapters/mod.rs` selects Windows or macOS native composition at the module
boundary. `platform/session.rs` coordinates the same GPUI session through either
selected host/UI lifecycle adapter, with no concrete Windows module paths, Win32
API calls or binding-field access. Native menu IDs are decoded in the Windows adapter;
the host validates hotkey registration IDs. Host Drop preserves the existing
detach → unhook → release registrations → kill timer/remove tray/destroy HWND →
release icons → unregister owned window class sequence. Class ownership also
covers partial setup failure. Timer creation failure is reported; DPI icon size
is committed only after both replacement icons exist, so a failed allocation is
eligible for retry. Native menu construction uses RAII and reports Create/Append/
cursor failures instead of treating them as dismissal; regular labels, commands,
capture-before-activation and dismissal behavior stay unchanged. The GPUI platform still outlives app globals and runtime
shutdown remains outside the UI task. Shared source/CI contracts do not establish
native-device acceptance; Linux remains unimplemented.

`platform/windows/ui.rs` now owns the GPUI/native lifecycle seam. Settings exposes
only its GPUI `AnyWindowHandle`; Windows `SettingsActivation` privately snapshots
the HWND under App, then reveals/restores/requests foreground outside App borrows.
The one-poll intent is consumed only for the same, still-open Settings generation,
so a Save completion that closed the draft does not reactivate it. No synthetic
Alt activation is introduced. Native identity representation is absent from both
shared Settings and session coordination. macOS snapshots its AppKit view under
GPUI and performs restore/reveal/activation in the native adapter after releasing
framework borrows, with the same generation check.

`OverlayLifecycle` owns the native shell and indicator-enabled flag. Attach
snapshots under App and configures the hidden foreground after releasing it.
Sync preserves hide/reprepare on a pending show, prepare → shared material update
→ warm/show → shared fallback ordering, and geometry sync otherwise. Every native
mutation is outside App/entity/runtime borrows. Indicator failure disables/hides
only the indicator and clears its shared presentation, not dictation. Normal close
hides and drops the shell (subclass/material/context) before removing the GPUI
foreground. Task cancellation retains Shell's existing non-reentrant Drop path;
this extraction does not add native foreground calls during App teardown.

## Paste-target ownership

Application contracts `Dictation<C, T>`, `Submission<A, T>`,
`Transcription<A, T>`, `TranscriptionEvent<T>` and `Paste<T>` carry an uninterpreted
adapter-owned target by value; they impose no native representation, `Copy`,
`Clone`, `Default`, GUI or OS dependency. A recording retains its Start target;
Stop's current target is discarded. Debounced/busy/rejected commands, cancellation,
shutdown, failed submission and stale results release their targets normally.

Windows `paste/windows.rs::FocusTarget` privately stores a borrowed foreground
snapshot. It is captured before tray activation or in the hotkey callback, moved
through the inbox/runtime/active recording/pending worker submission, then returned
with a matching completion to `ClipboardPaste`. It never crosses worker IPC or
settings serialization. Worker failure/release drops its pending target alongside
audio. It does not own or keep another application's window alive. Restoration
is best-effort via Windows validation; failure preserves the existing current-focus
fallback. A null/current target requests no restoration. Clipboard retries,
Unicode handling, modifier cleanup and session checks are unchanged. Handle reuse
is not prevented by this snapshot; this refactor does not strengthen OS identity
guarantees. Numeric representation is confined to the Windows focus adapter.
The clipboard owner, Unicode allocation, retry/backoff and SendInput implementation
now live in `paste/windows/clipboard.rs`, selected by `paste.rs`. Native hotkey
registration/hook state is selected through `hotkey.rs` from `hotkey/windows.rs`.

## Worker transport and native process ownership

`worker.rs` owns readiness/device policy, generation/request/session validation,
deadlines, residency and the pending application submission. It has no Win32 API,
raw handle or native process-field access. Its selected `worker/windows.rs::Process`
owns the child, kill-on-close Job Object, stdin, bounded response receiver and
reader join handles. Its narrow operations are spawn/send/poll/working-set bytes;
native fields are private. `worker/transport.rs` shares bounded protocol response
decoding with a single sixteen-entry limit; overflow stops the producer without
blocking and eventually disconnects after queued responses drain.

Launch remains no-console, from `<exe-dir>/runtime/voicepaste-stt-worker.exe`, with
runtime cwd and System32-only inherited DLL PATH. The child waits for Initialize;
job-assignment failure kills/waits only this owned child. The process owner is now
established before fallible pipe/thread setup, closing a pre-existing partial-setup
reaping gap. Normal release requests Shutdown, closes stdin, waits up to 500 ms
before killing/waiting, closes the job before joining readers, then drops pending
audio/target. **The 500 ms bound is only the post-send wait:** synchronous stdin
write and joins have no independent hard deadline; loaded-worker/fault-path
qualification remains open, not proved by the owned child test fixture.

`memory.rs` formats separate host/worker byte measurements; only
`memory/windows.rs` probes native handles borrowed from `Child` or the current
process. `diagnostics.rs` owns existing best-effort temp-path log rotation;
Storage's logging methods delegate for existing consumers. `launch/windows.rs`
owns parent-console/redirection handling and GUI startup-error dialogs; main
delegates CLI routing to shared bootstrap and contains no native API calls. Artwork/state/AND-mask calculation
stays in `tray_icons.rs`; native HICON allocation and monitor DPI probing live in
`tray_icons/windows.rs`, resource icons in `application_icon/windows.rs`.

## Storage and build inputs

Settings, models and temporary session data are under `<exe-dir>/data/`;
diagnostics are `%TEMP%/VoicePaste/voicepaste.log`. Existing config is never reseeded
on upgrade. Public defaults and the pinned model catalog live in
`crates/core/assets/`; private prompts belong only in app-local configuration.

`files/windows.rs` implements exclusive sharing, stable read handles and durable
publication through a small compile-time-selected `files.rs` boundary. Storage
owns validation, portable paths, lock/temporary-file lifetime and Save ordering;
`startup/windows.rs` owns the exact prior Run-value snapshot/rollback. Config
replacement and model replacement-with-backup remain distinct operations, not a
universal retry/publication policy. Model adapters retain catalogs/receipts/cache
locks; `model_preparation.rs` owns artifact recovery/resume, cancellation,
verification and publication/backup policy. `models/http/windows.rs`
owns synchronous WinHTTP handles/TLS/timeouts on the provisioning thread;
Content-Range validation is shared in `models/http.rs`. Receipt JSON and v1 native
timestamp values are unchanged; `models/windows.rs` extracts the Windows stamps.
Future ports must invalidate/reverify incompatible receipts, not reinterpret them.

All host Win32/Windows/raw-window-handle API imports now reside in Windows-selected
modules, with native dependencies target-scoped. Shared policy/UI modules have a
source-level regression guard (`adapters/boundaries.rs`); it is not a non-Windows
build test. macOS compilation/contracts use the feature-branch preview workflow.
The host explicitly rejects unsupported OS builds rather than providing successful
no-op adapters. GPUI/CPAL/SQLite and ML/native build/packaging still require
real-device qualification; compilation alone does not complete the port.

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
Nested Cargo builds exclude only the already-running, non-product `xtask` binary
to avoid Windows executable self-rebuild locking; workspace tests/clippy still
include it. No copied-xtask launcher is required for this fix.
Complete runnable profiles live only in `build/debug/` and `build/release/`;
archives live in `build/package/`. Cargo/native/tooling/staging files belong to `build/cache/`.
No additional shell interpreter, Python installation or legacy application supplies build logic.
See the [worker prerequisites](../apps/stt-worker/README.md).
Reproducible CUDA source builds, complete redistribution inventory and clean-machine
qualification are still pending; a successful local build does not establish them.
