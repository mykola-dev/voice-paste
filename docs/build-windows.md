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

Requires Rust **1.98.1** (pinned by `rust-toolchain.toml`), Visual Studio C++ Build
Tools, Windows SDK including SignTool, and Windows' built-in `curl.exe`.
Install those developer prerequisites explicitly before building. No PowerShell,
Bash, Python, .NET application runtime or CUDA Toolkit is required by the builder.
Cargo resolves the pinned Rust toolchain; no task installs an SDK or speech model.

From the repository root, run or double-click **`build-portable-windows.bat`**.
It only launches `cargo xtask package --release`; CI invokes the same BAT.
The BAT also supports the existing repository-local Rust installation under
`build/cache/rust-tools/`; otherwise Cargo must be on PATH.

```text
build-portable-windows.bat --check
cargo xtask package --release          # identical packaging task, no BAT needed
cargo xtask package --release --no-deploy # ZIP only; leave running installation alone
cargo xtask build                      # update complete portable Debug folder
cargo xtask build --no-deploy          # debug compilation only; runnable folder NOT updated
cargo xtask build --release            # update portable Release folder, no ZIP
cargo xtask prepare                    # acquire/verify native inputs only
cargo xtask check --release            # fmt, workspace tests, clippy
cargo xtask fmt
cargo xtask test
cargo xtask clippy
cargo xtask fetch
cargo xtask verify --archive build/package/VoicePaste-2.0.0-windows-x86_64.zip
```

`xtask/` is a standalone build-tool crate with cohesive filesystem, native-input,
process and packaging modules. It never depends on application/native ML crates.
Its tests can run on Windows/Linux/macOS (`cargo test --locked -p xtask`), but
application compilation/packaging currently requires Windows x64 with MSVC.
Nested builds exclude the already-running `xtask` executable to avoid Windows
self-rebuild locking; tests/clippy still cover the whole workspace. No copied
build-tool executable or alternate runnable output folder is needed.

Native inputs are SHA-256-pinned in `xtask/native-inputs.json`, cached under
`build/cache/upstream-native/` and prepared into `build/cache/prepared-native/`.
`--native-runtime-source <folder>` selects an explicit complete input folder;
`--ct2-include <folder>` overrides matching headers. Existing input folders must
pass the inventory check and are never silently repaired/overwritten.
`--offline` rejects missing native cache entries and disables Cargo network access
inside the task; use `cargo --offline xtask ... --offline` to make the initial
build-tool compilation offline too. See [native inputs](native-inputs.md).

Ready-to-run outputs: **`build/release/`** and **`build/debug/`**. ZIPs and SHA-256
live separately in **`build/package/`** (debug names have a `-debug` suffix). All technical inputs,
tools and temporary staging live under **`build/cache/`**; Cargo uses
`build/cache/target/`. There are no other canonical output folders.

ZIPs always come from fresh staging, never from a personal runnable folder.
They exclude settings, prompts, models and sessions. Existing `data/` contents
are preserved on rebuild. Publication preflights file locks and refuses unsafe
paths/unmanaged collisions; it never terminates or launches applications.
Quit the target portable instance yourself after saving drafts/finishing recording
before updating it, or use `package --no-deploy` to produce only the ZIP in
`build/package/`. That option does NOT refresh the runnable EXEs, and the
task reports this explicitly. An existing ZIP may likewise predate a later
`build` command; only `package` refreshes it. Archives always use clean staging,
so neither existing ZIPs nor personal data are included recursively.
Keep tools and disposable diagnostics under ignored `build/cache/`.

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
| `xtask/` | Shared Rust build, native preparation and portable packaging |
| `RustMigration/` | One TODO file with unfinished work |
| `docs/` | Current contracts/engineering guides and marked legacy specifications |

## Status and documentation

Windows is the current target. Native host effects are isolated behind Windows
module selection; [cross-platform readiness](cross-platform-roadmap.md) records
the boundaries and validation. Neither Linux nor macOS port is implemented.
Reproducible native source builds, redistribution closure,
equivalent-state memory comparison and some device/model/interaction qualification
remain open. A GPUI shutdown drop-order deadlock is fixed and has a bounded
process regression; loaded-worker/busy shutdown qualification remains open.

Quick shutdown regression (isolated disposable config, no model/network/microphone):

```powershell
./tests/quit-process.Tests.ps1 -Executable ./build/release/VoicePaste.exe
```

Startup is limited to 10 seconds, exit to 5 seconds; timeout fails and cleans up
only the test's own process. It never stops a running user instance.

- [Current host details](../apps/voicepaste/README.md)
- [Native worker and build inputs](../apps/stt-worker/README.md)
- [Behavior contract](behavior.md)
- [Architecture](architecture.md)
- [Outstanding work and validation](../TODO.md)
- [Documentation index](README.md)

## GitHub release automation

Push a tag matching the workspace version, for example `v2.0.0`. The Windows
workflow builds, runs fmt/tests/clippy, uploads a CI artifact and attaches the ZIP
and SHA-256 file to a **draft** GitHub Release for review. Manual workflow runs
produce a CI artifact only. Publishing remains a maintainer decision.

No private native bundle or repository approval variables are required. The
builder acquires pinned official archives and installed MSVC redistributable files,
validates notices/inventory, checks app-local DLL imports, then verifies the ZIP.
The tag must exactly match the workspace version. Version tags currently create
**drafts**, not automatic public releases; maintainers review and publish them.
The workflow never replaces assets of an already published release.

`Source and build-tool checks` tests the build tool on Windows/Linux/macOS without
native ML inputs. The separate Windows release workflow runs full workspace
fmt/tests/clippy and packages Release. Collected notices and signatures are
integrity evidence, not legal approval; redistribution/artwork review and
clean-machine/GPU qualification remain maintainer acceptance tasks.
