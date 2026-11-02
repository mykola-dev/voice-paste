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
- Windows `cargo xtask package --offline --no-deploy --release` also passed:
  optimized host/worker build and verified ZIP with **40 files, no user data**.
  Archive/checksum are in `build/package/`; this did not deploy or publish a release.
- First macOS CI for `b2664b6e378abef0739c8928ea1e8b3c0e9229ca`:
  [41225393925](https://github.com/mykola-dev/voice-paste/actions/runs/41225393925).
  Release build/package/signature and clippy passed; host contracts had 70 passes
  and 2 failures. The new UI fixtures used Windows CUDA defaults on macOS. The
  fixtures were corrected to select host defaults before the final validation below.
- Final code commit `30df76d792cfc3d931c3958ba732dc2b0fdf6fe5` passed
  [macOS CI 41226390965](https://github.com/mykola-dev/voice-paste/actions/runs/41226390965):
  fmt/portable contracts, Release build/package, app/worker ad-hoc signature
  verification, host/worker clippy, **75 host tests and 3 worker tests**. The prior
  failed fixtures are now passing. Native Settings activation is performed outside
  GPUI borrows; staging cleanup releases file handles before removal.

## Not established

No model/VAD weights were downloaded. No recording, clipboard injection, real
login-item/registry mutation, M1 inference, memory/performance or visual acceptance
was performed. Downloaded-artifact trust on the tested Mac remains unresolved; locally
compiled previews remain the established route. Use the single combined checklist
in [macos-preview.md](macos-preview.md) only after final automatic checks pass.
Worker IO hard deadlines and abrupt-crash audio recovery remain open in TODO.
