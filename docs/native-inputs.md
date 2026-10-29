# Native inputs for Windows portable builds

`cargo xtask prepare` and `cargo xtask package --release` use the same acquisition
code and the single lock file [`xtask/native-inputs.json`](../xtask/native-inputs.json).
No private native bundle, pip, Python installation, CUDA SDK installation or legacy
application is used. Official wheels are opened only as ZIP archives of native
DLLs/license text; Python extensions and package dependencies are not installed.

## Inputs and ownership

- CT2 **4.7.1** and Intel OpenMP: pinned official CTranslate2 Windows wheel.
- CT2 headers/license: source revision `226c95d94e660c48b11c62e108886b7ef76d589d`.
- cuBLAS **12.9.1.4**: NVIDIA redistributable archive.
- ONNX Runtime **1.25.1**: official stable Windows x64 archive.
- Intel license text: official MKL **2025.3.0** archive (no MKL DLLs ship).
- Silero v6 ONNX: existing pinned faster-whisper asset, not a speech-model download.
- VC++ Runtime: x64 `Microsoft.VC143.CRT` from the selected Visual Studio install;
  SignTool verifies Authenticode signatures. Its installed version/origin/hash is
  recorded, not falsely presented as pinned across different VS installations.
- Additional license texts have immutable upstream URLs and SHA-256 pins, including
  SIMD Utils BSD-2; original AVX header notices are retained too.

Cache: `build/cache/upstream-native/`. Prepared flat DLLs plus `include/` and `notices/`:
`build/cache/prepared-native/`. An explicit `--native-runtime-source <folder>` is supported.
Preparation uses temporary sibling staging and publishes only after validation.
Failed downloads/hash mismatches never publish partial cache entries. Existing
cache corruption fails rather than silently replacing it; offline mode never fetches.

Existing prepared folders must pass `notices/inventory.json`: schema version 1,
matching `ct2Revision`, unique safe relative paths, nonempty file sizes/SHA-256,
version, HTTPS origin and a tracked `licensePath` under `notices/` for every file.
Runtime files and matching CT2 headers are mandatory. Untracked files, unsafe
Windows paths, symlinks and reparse points fail validation.

## Packaged runtime

The lock file owns `runtimeFiles` and `hostFiles`; do not maintain another DLL list.
The host's VC++ DLL sits next to `VoicePaste.exe`, not just in `runtime/`.
The worker/runtime files live together in `runtime/`. Validated CT2 uses cuBLAS,
not cuDNN. `msvcp140_1.dll` is required by ONNX Runtime.

Packaging checks each EXE/DLL import against adjacent packaged files or an explicit
Windows OS/driver allowlist. Arbitrary DLLs installed on the builder are not
accepted merely because they exist in System32. Dynamic libraries/models also
have explicit required-file checks. ZIP validation checks the root/version/profile,
required host/worker/native entries, unique safe names, byte sizes and SHA-256.
User data is never sourced from `build/release/` or included in the ZIP.

## Acceptance boundaries

Windows system libraries and the NVIDIA display driver are OS/hardware prerequisites,
not redistributable application payload. CPU/auto/CUDA behavior remains unchanged.
The ZIP excludes speech models; the existing first-run model provisioning remains.

Hashes, notice coverage and valid signatures do **not** establish legal approval.
Microsoft's runtime EULA alone is not a standalone redistribution grant; applicable
Visual Studio Distributable Code terms must be reviewed. Artwork rights and complete
native redistribution review remain open, so tag workflows create draft releases.
Clean-machine and hardware/driver testing are distinct from structural ZIP checks.
Local Release evidence: full workspace fmt/tests/clippy and ZIP verification passed.
The extracted ZIP's host passed `--check` with a restricted PATH. Its worker loaded
with VAD off/on; with an existing tokenizer but deliberately absent weights, both
reached the expected CT2 `model.bin` error. The VAD-on route therefore initialized
the stable ORT session before engine loading, without GPU/model downloads or any
change to the running installation. This is startup/session evidence, not actual
transcription, clean-machine or driver qualification. Stable ORT/new CRT still need
real inference qualification before claiming parity with the previous runtime.
Reproducible CT2 CUDA source builds are not implemented by this packaging task.

Build-tool regressions: `cargo test --locked --package xtask` (no network/GPU/models).
Full Windows checks: `cargo xtask check --release`.
