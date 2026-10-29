# Rust application — implementation in progress

This is application source, not another disposable probe. It does **not yet
replace legacy VoicePaste**. Production migration was explicitly authorized after
the initial GPU feasibility checks.

Implemented: native tray host, configurable global hotkey with 300 ms debounce,
recording-only session-tagged Escape cancellation, background microphone capture,
bounded nonblocking audio handoff, mono conversion, anti-aliased resampling to
16 kHz PCM16 WAV, tail drain/cleanup, portable configuration seeded from all frozen
settings. Pure core and application do not depend on Win32/audio/native ML.

**Native dictation is now connected:** stop submits the WAV to a resident native
CT2 worker, then Unicode clipboard + configured SendInput pastes the result.
Escape discards only the recording. Worker preload/residency, Silero v6 CPU VAD,
UA/English/auto/bilingual policy, custom prompt, token suppression, temperature
fallback and timestamp-based long-audio seeking are implemented. The host does
not link native ML libraries. See [worker details](../stt-worker/README.md).

**Settings now uses GPUI with Zed One Dark.** The defective per-child-HWND form
has been replaced, not recolored. The pinned Kit/component 0.8.1 + GPUI-pre 0.3.10
pipeline owns rendering, input, a clipped shared scroll offset and adaptive flex
layout. Official pinned One Dark colors, IBM Plex Sans and vector component icons
are embedded locally with licenses. The tray, hotkeys, native overlay and worker
remain in place; GPUI owns the single host message loop.

Save/Cancel, all settings, prompt resize, color editing/picker, status/memory and
logical saved geometry are retained. Narrow windows stack labels above controls;
the requested master–detail view has compact left navigation for Hotkeys
(hotkey/paste), Transcription (options/prompt), Appearance (overlay color), System
(startup/residency/memory/logging). Horizontal tabs were rejected and removed.
Owned inputs and individual page scroll handles survive navigation; Save/Cancel
apply to the whole draft. The detail panel reflows based on its available width,
not the full window width. The System memory report uses four readable rows and
explains combined resident RAM/shared-page double counting without `WS sum` jargon.
Each detail page has just one bold heading; fields/groups do not repeat section
headings. Keep in memory and the readable memory report both live under System.
The live model status appears below the Model dropdown in Transcription, not in
the common footer. Tray activation explicitly reveals/restores Settings and makes
a best-effort foreground request outside GPUI borrows, without synthetic Alt or
other keyboard input. Launchers must suppress consoles with CreateNoWindow rather
than pass SW_HIDE to the GUI; the latter can suppress user-requested windows.
Prompt height is persisted on Save and restored from optional `initialPromptHeight`.
The footer remains outside the viewport. The live isolated EXE confirmed
PerMonitorV2. At test time Windows reported **144 DPI / 150%**, not 175%; no system
scale was changed. Headless scale/reflow checks include **175%**, but physical
175% and mixed-monitor visual acceptance remain user checks. A bounded native
wheel/thumb/corner/maximize/restore run sampled 203 frames without a sampled
whole-surface clear or a missed 500 ms response deadline. This is not proof of
every intermediate frame. The user reports it works much better; further visual
acceptance is left to them, rather than expanding disposable UI harnesses.

Mandatory implementation/review rules:
[Zed-source-backed Settings UI engineering](../../docs/settings-ui-engineering.md).

Save validates centrally, reserves a new hotkey before persisting and rolls back
the reservation on failure. Config replacement is atomic and exclusively locked.
Active dictation retains its settings snapshot; new worker options/model/residency
apply at the next idle boundary before another recording starts. Cancel never
saves. Live VRAM reports unavailable until a native per-process provider is added.

Latest feedback corrections: hotkey is a dropdown (ScrollLock, Numpad0, Right Alt,
Right Ctrl, F8, F12); existing custom config keys are preserved until changed.
Standalone right modifiers use the existing bounded keyboard hook, with owned
cross-Rust-instance reservations, side distinction and autorepeat suppression.
Left tray click opens Settings; right click opens the unchanged context menu.
The open-mouth raster `apps/voicepaste/assets/icon.ico` is embedded as resource 1,
which GPUI's Windows backend also uses for its window/taskbar icon. Default tray
states use a toothy smile (idle), open mouth (recording), and bitten lip
(transcribing), at native DPI sizes; provenance in `assets/tray/README.md`.

Settings includes opt-in **Start VoicePaste when I sign in**. Save writes a quoted
absolute EXE command to current-user `HKCU/.../CurrentVersion/Run/VoicePasteRust`;
no elevation/shell/legacy registration changes. Cancel does nothing. A failed
config replacement restores the prior Run value. Old/frozen configs default off.
RAM now always measures the host plus any worker, separately, with a labeled
working-set sum (shared pages can overlap). This is a reporting fix, not savings.
Logs use `%TEMP%/VoicePaste/voicepaste.log`, explicitly requested instead of the
portable log path. Overlay body alpha is 200/255 instead of 224/255; text,
waveform and red-dot opacity/timing are unchanged.

UI-open host-only sample (settings-only, no worker): 78.18 MiB working set,
104.41 MiB private commit. GPUI adds UI cost; this is not an inference-memory
optimization or a comparable Rust-vs-legacy total-memory benchmark.

Normal startup verifies the selected local model in the background and, if missing,
downloads only that pinned revision. Fresh defaults select **large-v2**, not turbo.
Status is available in Settings; interrupted `.partial` files resume using HTTP
Range. The unrequested permanent Retry/Cancel controls were removed from Settings
and the tray. Restart retries failed provisioning; the existing explicit
`--download-model` CLI route is also retained. Exact size/SHA-256 are required before publication.
Resource locks coordinate multiple hosts; repaired corrupt final artifacts retain
a backup. All model data stays under the portable folder, without an HF cache.
After a full successful check, an atomic `.verified.json` receipt records the
pinned manifest plus each file's size/creation/write timestamps. Unchanged files
reuse this receipt; missing/malformed receipts or changed fingerprints require
hash verification again. This cache detects ordinary file changes, not deliberate
tampering that also restores timestamps. Verified handles deny writes/deletion
until receipt publication. Local checks/loading do not show a download overlay;
only actual network provisioning does. Debug SHA-256 is optimized independently.

**Recording overlay is connected:** native, nonactivating 224 x 100 DIP top-right
layered window, rounded translucent body, red pulsing dot, timer, timestamped
3.24 s waveform and independent five-bar busy spinner. Bounded, session-tagged
real RMS measurements never modify saved PCM. History, release, Hermite contour
and local alpha are pure Rust; Win32 owns DPI, pixels and window resources.
Idle stops animation. No new overlay research/probes. Exact visual/mixed-DPI
parity and full behavioral parity are not claimed.

```powershell
cargo xtask fetch                 # pinned Cargo.lock dependencies
cargo xtask build                 # native preparation + dev build; does not launch
cargo xtask build --no-deploy --offline # compile only, no dependency downloads
cargo xtask test
cargo xtask clippy
./build/debug/VoicePaste.exe --check # config-only; no mic/hotkey/tray
./build/debug/VoicePaste.exe --settings # setup only; no model loading/download
./build/debug/VoicePaste.exe --download-model # explicit selected-model provisioning
./build/debug/VoicePaste.exe --capture-preview # interactive, opt-in
./build/debug/VoicePaste.exe # normal tray dictation; microphone used only on Start
./build/debug/VoicePaste.exe --transcribe-file '<wav>' # same worker, no clipboard/mic
```

Rust 1.98.1 is pinned in `rust-toolchain.toml`. The root BAT supports the existing
local toolchain or installed Cargo/rustup. Outputs/config/models/session data stay under
`build/debug/` (or `build/release/` with `--release`); native build inputs live in `build/cache/prepared-native/`. Requested
exceptions are diagnostics in Windows Temp and opt-in current-user startup.
The builder neither stops apps nor downloads speech models. The same `xtask`
also creates portable ZIPs; see [the build guide](../../docs/build-windows.md).

First `--check` seeds `data/config.json`. If legacy owns ScrollLock, interactive
launch fails clearly rather than taking over; quit legacy yourself or change
**only the Rust hotkey** (for example to F8) using `--settings`. This setup mode
does not reserve the old hotkey or start model work; Save checks the chosen key
and exits after applying settings. No automatic rebind or legacy termination.

Latest validation: build/fmt/clippy, **39 tests** plus ignored opt-in native-menu animation and
interactive paste tests,
`--check`, and earlier successive JFK
requests through the actual host/worker using untouched saved defaults (including
VAD=true, UA, custom prompt, CUDA-only). Reference words matched both times;
owned temp WAVs and worker were gone after exit. File-mode evidence:
`build/cache/previous-build/voicepaste-rust/evidence/resident-file-test.json`.

New tests exercise hidden native form values/Save/Cancel without showing a window
or stealing focus, atomic-save failures/locking, deferred settings, HTTP Range on
loopback only, partial-file verification/publication and corrupt-file backup.
Follow-up exercised visible Settings: occupied ScrollLock was rejected without
changing config, then F8 saved through the real UI. Selected local model hashes
were verified before startup; no model download. On the updated host, injected
F8 started actual microphone capture and the visible overlay, with foreground
unchanged in that observation. Session-tagged cancellation removed the WAV
without STT/paste. Screenshot: `build/cache/previous-build/voicepaste-rust/evidence/recording-overlay-live.png`.
This does not qualify physical-key delivery, Escape interception or edge clicks.
Only the Rust runtime config selects F8; frozen defaults still select ScrollLock.
Debug logs record toggle/state and local/cache/download status, not audio/transcript.

User-feedback fixes: Shift+Insert now marks Insert down/up as an extended key;
real clipboard/SendInput inserted Ukrainian text into an owned native edit.
Shell callbacks are forwarded from the window proc into the application queue,
including synchronous delivery; a real popup click opened Settings and Cancel
left configuration unchanged. The user also confirmed the menu works. Timer text
uses a cached 3x grayscale mask; animation uses the minimum Win32 timer interval
instead of a 16-ms interval that can round to half-rate. Busy bars use the selected
indicator color. Latest injected F8/actual microphone capture followed by injected
Escape hid the overlay and removed its WAV, without changing foreground in the
observation. Screenshot: `build/cache/previous-build/voicepaste-rust/evidence/recording-overlay-aa-fixed.png`.
Exact subjective smoothness/visual parity still needs user confirmation.
First receipt-producing local check took about 2.06 s; the next normal start
reused the receipt without rewriting it, with `network_download=false` and no
startup overlay. This is not an end-to-end readiness/latency benchmark.

2026-10-28 feedback corrections:
- Dev and release hosts use the GUI PE subsystem. Explicit CLI modes reuse an
  existing caller console without allocating one, preserving redirected output.
  The delivered debug EXE passed redirected `--check` (exit 0); normal launch had
  a native worker but no `conhost` child. GUI startup errors remain dialog-visible.
- Tray commands are Start/Stop, separator, Settings, separator, Quit; the actual
  native menu was enumerated and Settings opened through its selection path.
  Permanent Settings download controls and their footer space were removed.
- Busy bars use 0.24-s half-cycles / 0.10-s stagger (2x speed). A single boxed,
  UI-thread-owned frame context drains bounded meter input and renders from the
  host window procedure, including inside `TrackPopupMenu`, rather than from the
  outer message loop. It detaches before destruction. The opt-in native modal
  test delivered 34 frames in ~0.75 s without microphone/model/paste.
- The Settings patch batches sibling moves, clips inputs and invalidates the
  complete parent/child area, **but failed user acceptance**: that whole-window
  erase can be visible during interaction. Earlier final-pixel matches to forced
  repaint/roundtrip top and hidden control-geometry/prompt tests did not cover
  flicker, actual native resize delivery, DPI sharpness or a temporary hang.
  Evidence `build/cache/previous-build/voicepaste-rust/evidence/settings-scroll-*.png` is static only,
  not proof of corrected scrolling. Replace this renderer rather than declaring
  another global invalidation patch a completed fix.
- **Historical pre-GPUI status: One Dark replacement and STT memory optimization were open.** The lower
  freshly loaded worker reading after restart is not savings: the earlier sample
  was from an already-used worker. No model/options/residency change was made.

Full microphone → target insertion, physical Escape and complete focus behavior
remain unqualified. Explorer restart, full hotkey/geometry migration, complete visible UI and
complete shutdown qualification remain unfinished. Existing legacy source,
runtime and configuration were not changed.
