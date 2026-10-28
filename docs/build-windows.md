# Windows build and release guide

Windows tray voice dictation, implemented in **Rust** with a separate native
GPU transcription worker. Portable Windows x64 release.

Press the configured hotkey to record, speak with pauses, then press it again
to transcribe and paste. Recording never auto-stops on silence; Escape cancels it.
English, Ukrainian and bilingual modes are supported.

## Application

- GPUI Settings: Hotkeys, Transcription, Appearance and System; shared OK/Cancel/Apply.
- Native nonactivating recording overlay with sharp timer/waveform, optional
  frosted Acrylic-style background and one shared background-transparency slider.
- CTranslate2 **4.7.1** CUDA inference, Silero v6 VAD and a resident Rust worker.
- Unicode clipboard and configurable paste shortcut; configurable hotkey and
  optional Windows sign-in startup.
- App-local configuration, models and session data in `<exe-dir>/data/`.
  Logs: `%TEMP%/VoicePaste/voicepaste.log`.

Public default settings are defined by the retained
[settings defaults](../crates/core/assets/default-settings.json), not the old
documentation. The public snapshot contains no personal prompt; existing local
`customInitialPrompt` values are loaded unchanged, and the Settings field remains
available. Fresh startup provisions `large-v2`; other models are only
provisioned after explicit selection/save. Merely changing a dropdown does not
save or download a model.

## Run the portable release

Download `VoicePaste-2.0.0-windows-x86_64.zip` from GitHub Releases and extract it
to a writable folder. Run `VoicePaste.exe`; keep `runtime/` alongside it.
The default CUDA-only configuration requires a compatible NVIDIA GPU/driver.
Model weights are not included: first normal startup provisions the selected
model and needs internet access and sufficient disk space. Use `--settings` to
open Settings without loading a model. Defaults, including device/model options,
are documented in the linked settings snapshot above.

To upgrade or move the application, quit it first and copy your existing `data/`
folder into the new extracted folder. This preserves settings, custom prompts and
downloaded models. Never overwrite an active recording or an open Settings draft.

## Build and checks (Windows x64)

Requires PowerShell **7 x64**, Rust **1.98.1**, MSVC C++/Windows SDK tools, matching CT2
headers and native runtime inputs. The development script uses the local toolchain
under `build/rust-migration-tools/` when available, otherwise installed Cargo.

Run commands below from the repository root. Native inputs default to
`build/native-runtime/`; an explicit folder can be passed
with `-NativeRuntimeSource`. See the [worker build prerequisites](../apps/stt-worker/README.md).
This script does not install prerequisites or download models.

```powershell
./build-portable-windows.bat
# Or provide explicit native runtime and matching CT2 headers:
./build-portable-windows.bat -NativeRuntimeSource D:/native-runtime -Ct2Include D:/CTranslate2/include
```

Double-click `build-portable-windows.bat` to build a portable Release; its window
stays open so you can read the result. It only forwards parameters to the single
universal builder, `scripts/build-rust.ps1`. GitHub Actions uses the same builder.

```powershell
./scripts/build-rust.ps1 -Action build -NoDeploy          # debug binaries only
./scripts/build-rust.ps1 -Action build -Release -NoDeploy # Release binaries only
./scripts/build-rust.ps1 -Action package                 # debug portable + ZIP
./scripts/build-rust.ps1 -Action package -Release        # Release portable + ZIP
```

The BAT launcher always requests Release host/worker using the locked dependencies.
Output is the portable app directly in `build/release/`, plus a Windows-named
ZIP and `.zip.sha256` in `build/packages/`. Compilation caches remain outside the package. Models,
personal config and session data are never packaged and the existing `data/`
folder is preserved. Rebuilding overwrites executables, runtime files, notices,
docs and the manifest/archive; quit the app first because a running binary locks
its files. No application is started or stopped by the builder. `-WhatIf` checks inputs and describes the operation without
building. Debug portable apps go directly to `build/debug/`; their archives in
`build/packages/` have a `-debug` filename suffix. ZIPs are always made from clean
staging, never from a runnable folder containing personal data.
Local builds with incomplete native notices are for evaluation only.

```powershell
./scripts/build-rust.ps1 -Action fetch # explicit Cargo dependency fetch
./scripts/build-rust.ps1 -NoDeploy    # build without changing the dev installation
./scripts/build-rust.ps1 -Action fmt
./scripts/build-rust.ps1 -Action clippy
./scripts/build-rust.ps1 -Action test
```

After quitting the running Rust application, build without `-NoDeploy` to update
`build/debug/` (default) or `build/release/` (`-Release`). Development and packaging
use the same portable-folder publisher and preserve all existing `data/` contents.
Ordinary builds do not create ZIPs. `-NoDeploy` only updates the internal Cargo
cache at `build/voicepaste-rust/target/`. The script neither stops nor launches applications.
Keep generated tools, binaries, models and disposable diagnostics under ignored
`build/`; do not commit them.

```powershell
./build/debug/VoicePaste.exe --check    # config-only check
./build/debug/VoicePaste.exe --settings # settings without model loading
./build/debug/VoicePaste.exe            # tray application
```

Do not launch a second tray instance or interrupt recording/open Settings drafts
while updating. User acceptance of physical hotkeys, focus, scrolling and 175% DPI
remains distinct from automated tests.

## Source layout

| Directory | Purpose |
| --- | --- |
| `apps/voicepaste/` | Rust tray host, GPUI Settings and Windows adapters |
| `apps/stt-worker/` | Rust transcription pipeline and narrow native C++ bridge |
| `crates/core/` | Domain state, settings and shared policies |
| `crates/application/` | Use cases and consumer-owned ports |
| `crates/protocol/` | Bounded versioned host/worker IPC |
| `scripts/` | Rust development build |
| `RustMigration/` | One TODO file with unfinished work |
| `docs/` | Current contracts/engineering guides and marked legacy specifications |

## Status and documentation

Windows is the current target. Linux/macOS boundaries are being prepared; neither
port is implemented. Reproducible native source builds, redistribution closure,
equivalent-state memory comparison and some device/model/interaction qualification
remain open. A GPUI shutdown drop-order deadlock is fixed and has a bounded
process regression; loaded-worker/busy shutdown qualification remains open.

Quick shutdown regression (isolated disposable config, no model/network/microphone):

```powershell
./tests/quit-process.Tests.ps1 -Executable ./build/voicepaste-rust/target/release/voicepaste.exe
```

Startup is limited to 10 seconds, exit to 5 seconds; timeout fails and cleans up
only the test's own process. It never stops a running user instance.

- [Current host details](../apps/voicepaste/README.md)
- [Native worker and build inputs](../apps/stt-worker/README.md)
- [Behavior contract](behavior.md)
- [Architecture](architecture.md)
- [Outstanding work and validation](../RustMigration/TODO.md)
- [Documentation index](README.md)

## GitHub release automation

Push a tag matching the workspace version, for example `v2.0.0`. The Windows
workflow builds, runs fmt/tests/clippy, uploads a CI artifact and attaches the ZIP
and SHA-256 file to a **draft** GitHub Release for review. Manual workflow runs
produce a CI artifact only. Publishing remains a maintainer decision.

Configure repository variables `VOICEPASTE_NATIVE_BUNDLE_URL` (HTTPS) and
`VOICEPASTE_NATIVE_BUNDLE_SHA256` for a reviewed, immutable native-input ZIP.
See [native input preparation and inventory schema](native-inputs.md) for the
validation/bundle commands and hosting restrictions.
Its root must contain the runtime files listed in `scripts/native-runtime.psd1`,
`include/` from CT2 revision `226c95d94e660c48b11c62e108886b7ef76d589d`,
and `notices/` containing third-party license texts and `inventory.json`
(versions, origins, hashes and redistribution terms). The validated CT2 binary
does not use cuDNN; its minimal runtime list includes the ONNX Runtime dependency
`msvcp140_1.dll`. Dependency changes require requalification; these local checks
do not establish complete clean-machine closure.
Set `VOICEPASTE_REDISTRIBUTION_APPROVED=true` only after reviewing the native
bundle and embedded artwork rights. The workflow fails closed without these
inputs; it never takes native dependencies from a developer machine.

The current native inventory, artwork rights and clean-machine qualification
remain incomplete. Automation does not certify redistribution or feature parity.

`Windows source checks` runs formatting and synthetic packaging/native-input
tests on pushes to `master`/`Rust` and pull requests, including forks, without
needing a native bundle. The separate release workflow verifies bundle contents,
runs workspace tests/clippy, and packages Release binaries. Invalid or missing
release inputs fail explicitly rather than producing misleading empty artifacts.
