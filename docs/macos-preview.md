# Apple Silicon macOS preview

Development branch: `macOS-2`, based on `ab6c864` from `master`.
The first milestone is a **launch-only** `.app`, not a working dictation port.
No Developer ID membership is available: previews are ad-hoc signed and **not
notarized**. Managed Macs may block them. Do not disable security controls or
work around company policy to launch a preview.

## First launch gate

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

The gate uses the production executable and pinned GPUI family, sharing the
existing One Dark theme/font installation. It does not request permissions,
write application data, access the microphone/clipboard, download models, or
pretend that dictation is connected. Menu bar, Settings, overlay and STT remain
unimplemented on macOS. The window says so explicitly. Existing Windows code
and public defaults remain selected unchanged on Windows.

Build locally **on an Apple Silicon development Mac** with
`cargo xtask macos-preview --release`. No developer tools are needed on the
user's test Mac. The task creates `build/release/VoicePaste.app`, archives and
checksums in `build/package/`, and stages under `build/cache/`. It refuses to
overwrite an existing `.app` that might be running; explicitly move that app
aside before rebuilding. Debug uses `build/debug/`. No personal data is archived.

The package checks ARM64 architecture, system-only dynamic dependencies, plist,
and signature integrity. These checks do not prove GUI launch, Gatekeeper trust,
TCC permission persistence, or actual GPU inference.

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
