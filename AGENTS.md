# VoicePaste — Rust agent guidelines

## Scope and sources of truth

- Continue development on branch `master`; leave changes uncommitted/unpushed unless requested.
- Production source lives in `apps/` and `crates/`. The C#/WPF/Python application
  and its build scripts were removed; do not restore legacy dependencies.
- Follow `docs/engineering-principles.md` and `docs/architecture.md`.
  Preserve the behavior contract in `docs/behavior.md` and public defaults
  in `crates/core/assets/default-settings.json`. Preserve
  existing saved user prompts; public defaults intentionally contain none.
  Numbered legacy specifications in `docs/` are historical; the named Rust guides
  are current. Prototype source was removed; open work is `TODO.md`
  (`RustMigration/TODO.md` is a compatibility link).
- Record defects instead of copying them or silently changing visible behavior.

## Engineering rules

- Domain/application stay independent of GUI, Win32, IO and native ML libraries.
- Use cohesive modules, narrow consumer-owned ports and explicit composition;
  concrete adapters depend on inner contracts, never the reverse.
- Apply SOLID through Rust composition/contracts; DRY shared rules and sources of
  truth without merging unrelated policies. KISS/YAGNI: no speculative traits,
  crates, plugin systems, event buses or DI containers.
- Resources/state/background tasks have explicit owners; handle cancellation,
  stale results, bounded queues, errors and shutdown deliberately.
- Isolate unsafe/FFI in adapters; callbacks stay bounded and nonblocking.
- Test at the proper level; run fmt/clippy and never claim unrun checks.
- Keep prototypes bounded; temporary scripts/probes/generated debugging code go
  in ignored `build/` or `%LOCALAPPDATA%/Temp/opencode`, not product
  `scripts/`, `apps/` or `crates/`. Keep reusable regression tests separate.

## Cross-platform readiness

Linux/macOS are future targets, not authorization to implement ports. Shared UI
state/layout and domain/application must not contain native OS handles. Isolate
OS behavior/effects in platform-specific modules selected with `cfg`, target-scoped
native dependencies and narrow consumer-owned boundaries. No fake successful
adapters or speculative platform framework. Unsupported blur retains translucency.

## Settings/UI rules

Read `docs/settings-ui-engineering.md` before changing Settings,
layout, scrolling, rendering, DPI or UI event/lifetime handling.

- Use the coherent pinned GPUI Kit/component pipeline (0.8.1 / GPUI-pre 0.3.10),
  not per-control HWND layout/rendering or generic dark Win32 controls. Preserve
  Zed One Dark. Overlay foreground/state/animation use the shared GPUI pipeline;
  native nonactivation/placement/backdrop stay in the Windows adapter. Preserve
  Gaussian blur + white tint and matching 16-DIP rounded clipping; unsupported
  effects retain translucency. macOS/Linux overlay adapters are out of scope.
- Actual Windows scale **175%** is the primary target. Check EXE/live-window DPI
  awareness, separate logical units/device pixels and handle DPI changes. No OS
  bitmap upscaling for text/icons.
- Present completed frames. Full backbuffer redraw is fine; background erase
  followed by independent child paints is not. No whole-window
  `RDW_ERASE | RDW_ALLCHILDREN` per wheel/thumb event.
- Content/constraint-driven reflow, clipped viewport and one shared scroll state;
  resize updates layout, surface and hit-testing.
- Never hold RefCell/lock guards across reentrant Win32/framework calls, including
  DefWindowProc. Defer/coalesce events with eventual delivery, do not silently drop
  resize/close/completion on failed borrow.
- UI work stays bounded/nonblocking in nested resize/menu/scroll loops. Preserve
  draft/input identity, caret, focus, Unicode/IME and Save/Cancel.
- Screenshots/pixel equality prove final pixels, not flicker-free interaction.
  User performs visual acceptance; do not expand interactive UI harnesses.
  User-reported flicker/hangs reopen issues regardless of unit tests.

## Build, data and safety

- `cargo xtask build`, `cargo xtask fmt`, `cargo xtask clippy`,
  `cargo xtask test`; see README/worker README for native prerequisites.
- Ready-to-run portable builds have one canonical location per profile:
  `build/debug/` (default) and `build/release/` (`--release`). Rebuild through
  `cargo xtask build [--release]` to update those folders;
  `build --no-deploy` only compiles into the internal Cargo cache, not a runnable package.
  `build-portable-windows.bat` updates the same Release folder and writes ZIP/checksum
  to `build/package/` (both profiles; debug archives have a `-debug` suffix).
  Keep archives separate from the runnable `debug/` and `release/` folders.
  All compilation caches, native inputs, local tools and temporary staging belong
  under `build/cache/` (Cargo target: `build/cache/target/`). Never deploy to
  `build/voicepaste-rust/`, a Cargo target directory or a version-named subfolder.
  Only run/deliver the complete app from `build/debug/` or `build/release/`, not
  raw Cargo EXEs. Use `--no-deploy` deliberately when the target app is running;
  explicitly report that the runnable folder was NOT updated.
  Rebuilds preserve `data/`; never archive personal data.
- Native inputs are pinned in `xtask/native-inputs.json` and prepared into ignored
  `build/cache/prepared-native/` (override: `--native-runtime-source`). Never use pip,
  an installed Python environment or the legacy app. Wheels are archives only.
- Run Windows commands with hidden console windows and without focus theft.
- Preserve application-local config/models/session data; logs belong at
  `%TEMP%/VoicePaste/voicepaste.log`. Do not change saved defaults during builds.
- Ask before model downloads, SDK installation or stopping unrelated apps.
- Do not interrupt recording or discard open Settings drafts during updates.
- Do not modify installed legacy applications/system runtimes. The former local
  legacy portable build was moved outside the repository at the user's request.
