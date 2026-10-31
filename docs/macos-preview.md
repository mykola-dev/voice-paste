# Apple Silicon macOS preview

Development branch: `macOS-2`, based on `ab6c864` from `master`.
The launch-only milestone is complete: the user confirmed a locally compiled
Rust preview opens on the corporate Mac. The current development build wires
the complete dictation path described below; automated compilation/contracts
and real-device acceptance are tracked separately. It is not a production release.
No Developer ID membership is available: previews are ad-hoc signed and **not
notarized**. Managed Macs may block them. Do not disable security controls or
work around company policy to launch a preview.

## Current development implementation

- Normal launch is a resident menu-bar app (no Dock window); left-click opens
  shared GPUI Settings and right-click opens recording/Settings/Quit commands.
  `--settings` opens Settings without loading a model or enabling capture.
- The existing shared Rust runtime/application state machine now owns macOS
  recording, debounce, session cancellation, stale worker results, settings-at-idle,
  model residency and statistics. The capture-only worker fork was removed.
- Use **Download selected model…** for the **saved** selection. Its native
  confirmation explicitly approves Hugging Face weight/VAD downloads. Initial
  startup, draft dropdown queries and builds do not download model weights.
  **Cancel model download** retains the partial; a later approved request resumes.
  Exact size/SHA-256 verification precedes publication. A complete verified partial
  can be published offline. Source manifest: `crates/core/assets/macos-model-artifacts.json`.
- Full FP16 GGML large-v2, large-v3 and large-v3-turbo use separate pinned artifacts,
  never Windows CT2 files or silent smaller/quantized replacements. Model storage
  is `~/Library/Application Support/VoicePaste/data/models-ggml/`.
- The bundled worker pins whisper-rs 0.16.0 / whisper-rs-sys 0.15.0 / whisper.cpp
  1.8.3 with embedded Metal shaders. It supports Metal-only (default) or CPU, not
  an unagreed Auto policy. Initialization fallback diagnostics fail closed in
  Metal-only mode; the host validates reported device before accepting readiness.
  No models are loaded in the GUI process.
  Cargo forces portable GGML CPU compilation (`GGML_NATIVE=OFF`, ARMv8.4-A with
  FP16/dot-product support) rather than the CI runner's newer native instruction
  set. M1 runtime/performance qualification is still required.
- Silero v6.2.0 GGML runs before log-mel preparation, using the shared threshold,
  hysteresis, 2000-ms silence and 400-ms padding policy. VAD failure is an error,
  not an instruction to bypass VAD. Beam 1–10, decoder fallback/context/seek,
  combined initial prompt and Ukrainian cleanup remain enabled. Native backend
  output is not claimed to be byte-identical to CTranslate2.
- Microphone permission is requested only on Start. **Enable recording keys…**
  explicitly requests Accessibility access and retries tap creation. Denied or
  corporate-restricted access is reported, not bypassed. With permission granted,
  Right Control (default), Right Option, F8 and F12 are standalone recording keys;
  repeat/chords do not create extra toggles. Escape consumes its recording-cancel
  sequence and carries the original session identity through the bounded inbox.
- Shared GPUI draws the animated waveform/busy indicator. Native NSPanel policy
  handles nonactivation, work-area placement and a separately owned backdrop.
  The public AppKit backdrop uses **NSVisualEffectView system blur plus white
  tint**, clipped at 16 DIP, not a claim of an identical Windows Gaussian kernel.
  Reduce Transparency or unavailable blur retains the shared translucent body.
  Visual/nonactivation/mixed-display acceptance remains unperformed.
- Cmd+V copies Unicode, attempts original-app restoration and falls back to
  current focus if restoration is unavailable. Posting permission/allocation
  failures leave text for manual paste. Posting events does not prove the target
  accepted them; clipboard contents are not restored. Shutdown is checked again
  after asynchronous focus restoration before posting input.
- Menu artwork changes with recording. SQLite statistics distinguish the GGML
  revision and actual Metal/CPU device, VAD/log-mel/decode/load phases; RAM queries
  measure the host and owned worker separately. Logs retain no transcript/prompt.
- Settings/prompt persistence, atomic Save/Apply/Cancel and owned current-user
  LaunchAgent behavior remain as documented in the historical Settings milestone.
  Temp recording ownership removes files on completion/cancel/orderly shutdown;
  abrupt-crash recovery is still an open task, not an implemented guarantee.

Real microphone/TCC/hotkey/paste/Metal performance, M1 memory and visual behavior
have **not** been accepted on a Mac. User requested completing development and
automatic checks first, then one combined manual pass. Do not ask for slice-by-slice
manual tests or describe a green CI as hardware acceptance.

## First launch gate (historical `aa24bde`)

The `macos-preview.yml` workflow builds on an ARM64 `macos-26` runner with
Xcode 26.6. The deployment minimum is macOS 26; current/previous stable versions
were rechecked against Apple's OS list on 2026-10-30 (27/26). Compilation on
one runner is not runtime qualification of both systems or of M1 hardware.

1. Download `VoicePaste-macos-arm64-launch-preview-<commit>` from the Actions run.
2. Extract the Actions artifact, then the inner ZIP to obtain `VoicePaste.app`.
3. Open the app in Finder. Report any security/MDM message exactly as displayed.
4. If it opens, check text sharpness, resizing, and closing the window exits.
5. Include build SHA, chip, RAM and macOS version in the report. Do not send
   transcripts, personal prompts or audio.

The original `aa24bde` gate used the production executable and pinned GPUI
family without data/permission effects. The current preview uses the **same
Settings view** as Windows, with macOS-specific choices, and a real AppKit
menu-bar icon. Existing Windows public defaults remain unchanged.

## Historical Settings/menu milestone (`e21d25d`)

The following describes that earlier slice, not current feature limitations.

- Left-click the lips icon to open/reveal Settings; right-click for the shared
  menu description and Quit. Start recording remains disabled until connected.
- Settings use the shared One Dark master–detail UI, draft identity, prompt,
  geometry, OK/Apply/Cancel and validation. macOS choices are Right Control
  (default), Right Option, F8, F12, Cmd+V, and Metal-only (default) / CPU. These
  are saved preferences, not a claim that native hotkeys or inference run yet.
  An Auto policy is not introduced before its behavior is agreed.
- Config is `~/Library/Application Support/VoicePaste/config.json`; logs are
  `~/Library/Logs/VoicePaste/voicepaste.log`. Existing legacy `mac.config.json`
  is not imported, overwritten or deleted. Saved prompts are preserved.
- Saves run on an owned background thread, use a nonblocking advisory lock and
  atomic same-directory replacement. Cancel does not undo a completed Apply.
- Opt-in startup writes only the current user's
  `~/Library/LaunchAgents/dev.mykola.voicepaste.plist`, for the executable's
  current path, on Save. No `launchctl` bootstrap or immediate launch occurs;
  the item takes effect next sign-in. Disabling removes only the app-owned file.
  Existing unrelated files are rejected, and failed config publication restores
  the prior login-item contents. Moving the app later requires re-saving the
  login preference; corporate policy can still prevent login launch.
- Host resident memory is a real Mach task query. Worker memory, GGML artifact
  sizes and transcription statistics explicitly remain unavailable; no CT2
  sizes/revisions are presented as macOS models.
- Normal launch makes no model downloads or microphone/accessibility requests.
  Dictation, hotkeys, overlay, transcription and paste remain pending.

## Historical capture integration slice (`dc5a857`)

`VoicePaste.app/Contents/MacOS/VoicePaste --capture-preview` selects the same
explicit capture-only mode already available on Windows. Only in this mode,
the menu enables **Start microphone capture (no transcription)**. Selecting it
requests microphone permission asynchronously through AVFoundation; a denied
or restricted permission is reported in the menu without changing security
controls or retrying prompts. Merely opening Settings/the menu never requests it.

CoreAudio capture uses the shared CPAL adapter, bounded PCM queue and writer,
mono conversion and 16-kHz PCM16 WAV resampling. Streams and writers belong to
a capture thread, not the UI. Stop finishes and discards the temporary audio;
Cancel and Quit discard it too. Files are created exclusively with Unix mode
0600 in the app's private `temp/` folder under Application Support. No transcript,
clipboard, inference, hotkey or overlay behavior is claimed by this mode. The
menu shows recording/completion/error status; Settings retains the ordinary
dictation-not-connected notice. No automatic recovery of files left by a process
crash is implemented yet. Escape cancellation waits for the hotkey adapter.

This slice reuses the application debounce/session/cancellation rules; it is not
a second dictation state machine. Host contract tests do not request TCC access
or use a live microphone. Real-device permission, capture and audio-device-loss
acceptance remain pending; tests on the Mac will be requested after development.

Model-delivery metadata and the limits of an official Hugging Face Git fallback
are recorded in [macos-model-delivery.md](macos-model-delivery.md); no weights
or optional Git extensions were downloaded/installed for that investigation.

For an update, quit the old preview explicitly and **move the existing
`build/release/VoicePaste.app` aside** before running the build script again.
Do not delete Application Support data. The script does not stop the old app
or overwrite it automatically.

Manual acceptance for this slice: open/close/reopen Settings from the icon,
check right-click/Quit, change a prompt and Apply, then Cancel subsequent edits
and reopen/relaunch to check persistence. Check resizing/Unicode/scrolling and
that Save failures leave the draft editable. Sign-in startup is a separate
opt-in test, not a reason to modify corporate policy. CI contracts do not prove
these interactions or actual login launch.

Build locally **on an Apple Silicon development Mac**, from a checkout of
`macOS-2`, with:

```bash
bash build-macos-preview.sh
```

When started from a Rosetta-translated Terminal on Apple Silicon, the script
restarts itself in native ARM64 mode without changing Terminal settings. Intel
Macs remain unsupported.

The script checks Xcode (including the Metal compiler and libclang) before downloading
anything. It never installs/selects Xcode, accepts its license or requests sudo.
If the pinned Rust version is already available, it reuses it. Otherwise it
reuses a repository-local toolchain or asks before downloading Rust into
`build/cache/macos-rust/`. The official rustup installer is SHA-256 checked before
execution; Rust does not modify shell profiles or install globally. `--yes`
explicitly approves that toolchain download for noninteractive use. Missing
If CMake is unavailable, it separately asks before downloading official Kitware
CMake 4.4.5 into `build/cache/macos-tools/`, verified against its pinned SHA-256.
It does not install system packages or change shell profiles. `--yes` explicitly
approves these local build-tool downloads. Cargo dependencies are fetched according to `Cargo.lock`; subsequent builds
reuse the caches. A network restriction produces an error, not a TLS bypass.
No models, Python, Homebrew or system transcription runtimes are installed.

With Rust already prepared, the underlying command remains
`cargo xtask macos-preview --release`. Developer tools are required for local
compilation, but not merely for running a prebuilt app. The task creates
`build/release/VoicePaste.app`, archives and
checksums in `build/package/`, and stages under `build/cache/`. It refuses to
overwrite an existing `.app` that might be running; explicitly move that app
aside before rebuilding. Debug uses `build/debug/`. No personal data is archived.

The package checks ARM64 architecture, system-only dynamic dependencies, plist,
and signature integrity. These checks do not prove GUI launch, Gatekeeper trust,
TCC permission persistence, or actual GPU inference.

The user reports that the old `origin/MacOS` Swift port, built directly on the
same corporate Mac, launches with an ad-hoc signature. Its `build-mac.sh` did
not remove quarantine or change Gatekeeper rules. This establishes a useful
local-build test route, not proof that this Rust preview will launch. Downloaded
CI apps remain blocked there; local compilation is the next launch gate.

## Portability audit: first implementation slice

| Area | Confirmed blocker / next boundary | Windows protection |
| --- | --- | --- |
| Composition | Main now selects the macOS launch gate; full shared session still requires real native Host/UI lifecycle adapters | Windows runtime/composition unchanged |
| Shared presentation | Settings/overlay contain shared views, but require runtime commands and native activation/placement | Reuse views, do not duplicate Settings or state machine |
| Settings | CUDA/Windows hotkeys and paste enums/defaults need explicit portable extensions | Keep existing JSON/defaults and user prompts unchanged |
| STT | CT2 encoded/generation pipeline and unconditional MSVC bridge cannot implement Metal | Keep CT2 on Windows; introduce backend-specific transcription and pinned GGML artifacts |
| Provisioning | WinHTTP/file publication/receipt timestamps are Windows-selected | Preserve hash/resume/cancel; invalidate incompatible receipts on macOS |
| Worker | Process ownership currently uses Job Objects and `.exe` paths | Add real macOS process owner; keep bounded protocol and stale-result policy |
| OS integration | Focus/paste, right-modifier hotkeys, login items, memory, launch errors, tray and blur require native implementations | No fake-success adapters or portable OS handles |
| Packaging | Windows xtask path requires MSVC/native DLL preparation | Separate preview task; Windows production workflow unchanged |

## Subsequent milestones

- Connect shared Settings/menu/overlay through real macOS adapters and test
  required permissions and focus behavior early.
- Add whisper.cpp + Metal with stable logical model IDs, distinct pinned model
  artifacts, VAD, beam/language/prompt parity and actual device reporting.
- Complete recording → transcribe → restore target → Cmd+V, cancellation,
  residency, startup, statistics and crash/shutdown behavior.
- macOS recording presets: **Right Control (default), Right Option, F8, F12**.
  No multi-key presets. Validate modifier-only chord/repeat handling on hardware.
- Data: `~/Library/Application Support/VoicePaste`; Windows portable data stays
  app-local. Eight-GB RAM compatibility is a measurement target, not a promise;
  16 GB is recommended pending measurements. No silent quality reductions.
- Investigate official HF Git/LFS/Xet weight delivery. Successful cloning alone
  proves nothing about blocked blob downloads; do not add Git/Python runtime
  prerequisites or weaken TLS. No model download is part of this launch gate.
- Integrate production release workflows only after platform acceptance and a
  separately authorized signing/distribution solution. No stable release yet.

Detailed ordered work plan remains in
`C:\Users\mykola\.opencode\plan\voicepaste-macos-2.md` on the development machine.

## Validation of the first slice

Windows: `cargo xtask build --check --no-deploy --offline` passed (fmt,
144 tests passed, 1 interactive paste test ignored, clippy with warnings denied,
Debug compilation). Used the existing repository-local Rust/native inputs.
Neither runnable Windows profile was updated; no running app was stopped.
macOS compilation and real-device launch are tracked separately by the preview
Actions run and the user's report; they are not implied by the Windows checks.
The first macOS run passed formatting and inner-layer tests but exposed that
packaging fixtures reject macOS's `/var` symlink in the default temp path.
The preview job now uses a physical `build/cache/tmp/` directory; symlink safety
checks remain unchanged, and the failed tests are not skipped.

## Launch blocker reported on macOS 26.7.1

The user launches the original `aa24bde` app from Downloads. Gatekeeper says
Apple could not verify it is free of malware; only **Done / Move to Bin** are
shown, including after **Open Anyway** in Privacy & Security. Launch acceptance
is blocked. This is not evidence of detected malware or proof of an MDM rule.

The workflow's `audit_original_artifact` dispatch mode downloads artifact
`12984205097` from run `41065476513` (no rebuild), validates the original ZIP
checksum and extracted executable permissions/plist/signature, and records
`spctl` / `syspolicy_check distribution` results. It does not modify signatures,
quarantine, trust rules or system security. A valid ad-hoc signature is integrity
evidence, not Developer ID trust or notarization. Runner policy checks cannot
reproduce this managed Mac's local approval state.

Audit run `41072481772` on macOS 26.6.2 verified the original checksum,
executable `0755` mode, arm64 / minimum macOS 26.0, system-only linked libraries,
plist and intact ad-hoc signature after download/extraction. `spctl` rejected
the app (exit 3). `syspolicy_check` exited 70 and reported **Adhoc Signed App**,
**Notary Ticket Missing** and **Internal Xprotect Error**. The latter is not a
named malware detection and must not be ignored or called proof of malware.
The successful audit job means evidence collection succeeded, NOT distribution
approval. The corporate Mac's failed override remains unexplained. A bounded
follow-up compares policy results with a trivial locally compiled ad-hoc control
app and records runner policy logs; no security controls are changed.

Follow-up run `41072723117` reproduced the same Internal Xprotect Error and
exit 70 for both VoicePaste and a freshly compiled `int main(void) { return 0; }`
ad-hoc control bundle. Thus the runner error is not specific evidence against
VoicePaste's payload; it is not a clean malware scan either. The runner had no
receipts under the two probed XProtect package IDs (not proof the components
are entirely absent). Stop expanding runner probes: the remaining useful
diagnostic is a read-only `syspolicy_check distribution <app-path>` on the
affected Mac or an IT assessment. No package corruption was found; a new ZIP
or DMG alone cannot provide Developer ID trust/notarization. The exact reason
the user's per-app GUI override fails remains unverified.
