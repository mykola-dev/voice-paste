# Shared-session and provisioning verification — 2026-10-31

Scope: `macOS-2` architecture consolidation; no production release or master merge.

## Shared implementation

- Both native compositions execute one UI-session coordinator, including runtime
  replies, Settings generations/Save, permission handoff, native intents, overlay
  lifecycle calls, backpressure and shutdown. Native hosts own no runtime loop.
- Provisioning jobs, partial recovery/resume/reset, hashing, verification and
  publication share one lifecycle. Catalogs, transport, receipts/locks and native
  filesystem primitives remain distinct. Successful macOS downloads now publish
  verified staging rather than leaving only `.partial` weights.
- Settings rendering consumes presentation data rather than OS switches.
  Config staging/cleanup and startup commit/rollback ordering are shared.
  CLI routing, preparation, offline file inference and runtime startup are shared;
  existing check/batch/stdout contracts are explicit data. File mode verifies
  pinned local artifacts without enabling downloads on either backend.

## Automatic evidence

- Windows: `cargo xtask fmt --offline`, `cargo xtask test --offline` (**189 passed,
  1 interactive ignored**), `cargo xtask clippy --offline`, and
  `cargo xtask build --offline --no-deploy` passed. `git diff --check` passed.
- Windows runnable `build/debug/` and `build/release/` were **not updated**.
- First macOS CI for `b2664b6e378abef0739c8928ea1e8b3c0e9229ca`:
  [41225393925](https://github.com/mykola-dev/voice-paste/actions/runs/41225393925).
  Release build/package/signature and clippy passed; host contracts had 70 passes
  and 2 failures. The new UI fixtures used Windows CUDA defaults on macOS. The
  fixtures now select host defaults; final CI remains required for the new code.

## Not established

No model/VAD weights were downloaded. No recording, clipboard injection, real
login-item/registry mutation, M1 inference, memory/performance or visual acceptance
was performed. Corporate downloaded-artifact trust remains unresolved; locally
compiled previews remain the established route. Use the single combined checklist
in [macos-preview.md](macos-preview.md) only after final automatic checks pass.
Worker IO hard deadlines and abrupt-crash audio recovery remain open in TODO.
