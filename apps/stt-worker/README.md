# Native STT worker

Separate resident Rust executable. On Windows it uses a narrow C++ CTranslate2
**4.7.1** bridge; on Apple Silicon macOS it uses whisper.cpp through whisper-rs
with CPU or Metal. See the [macOS beta guide](../../docs/macos-preview.md) for
backend and qualification details. The following Windows-specific details do not
claim macOS device qualification.
Normal execution uses no Python/.NET interpreter or CUDA SDK. Only this process
loads CT2/ONNX Runtime/cuBLAS. Host owns its pipes, deadlines and kill-on-close Job.

Implemented pipeline: PCM16 mono 16 kHz → optional Silero v6 CPU VAD → global
80/128-mel frontend → language detection/forced language and tokenizer prompt →
timestamp-aware generation/seek → temperature fallback, silence checks and
previous-text conditioning → Ukrainian cleanup → versioned UTF-8 JSON result.
UA/bilingual suppress Russian-only character tokens without losing default
non-speech suppression. CUDA-only never silently switches to CPU; auto mode may
load CPU INT8 and reports the actual device. Saved settings remain unchanged.

On Windows build through `cargo xtask build` or the root `build-portable-windows.bat` launcher.
Native inputs are acquired from SHA-256-pinned official archives into ignored
`build/cache/prepared-native/`; override with `--native-runtime-source <directory>`.
It must contain `ctranslate2.dll` (4.7.1), `libiomp5md.dll`,
`cublas64_12.dll`, `cublasLt64_12.dll`, `onnxruntime.dll`, `silero_vad_v6.onnx`,
`msvcp140.dll`, `msvcp140_1.dll`, `vcruntime140.dll` and `vcruntime140_1.dll`. These are native
dependencies of the Rust worker, not a Python runtime. No installed Python packages,
legacy application or CUDA SDK supply them. Matching source headers are acquired
from CT2 revision `226c95d94e660c48b11c62e108886b7ef76d589d`. Builds never install
SDKs or download speech models. See [native inputs](../../docs/native-inputs.md)
for the lock, inventory and remaining redistribution/clean-machine acceptance.

The validated Windows CT2 4.7.1 binary uses cuBLAS, not cuDNN; do not bundle the
cuDNN dispatcher or its component libraries for this binary. CUDA-only and CPU
model initialization/transcription passed in a separate no-cuDNN folder, with
VAD on/off and app-local CRT/ORT modules verified. Synthetic audio without VAD
exercised CUDA inference and loaded app-local cuBLAS. This is dependency evidence,
not a transcription-quality or clean-machine qualification. Recheck dependencies
if replacing CT2. `msvcp140_1.dll` is an actual ONNX Runtime import; a system-wide
VC++ runtime must not silently supply it in a self-contained package.

Builds/package ZIPs exclude personal speech models and never seed `data/`.
Existing local models remain untouched. Missing model means a clear error in
file-only mode; normal host startup provisions its selected model.
Other models can also be placed in the
corresponding manifest revision folder; turbo is not runtime-tested yet.

The IPC accepts Initialize, Transcribe, Shutdown; checks version/generation/
request/session and limits lines to 1 MiB. Audio must be inside the host-owned
temporary folder. Stdout contains only protocol. On Windows native stderr is
forwarded to the host log with best-effort rotation, not a hard size limit; on
macOS it is forwarded to host stderr, whose destination depends on the launcher.
Native diagnostic text is not filtered for paths or other sensitive content.
On unconfirmed worker failure the host retains the recording and blocks further
worker use in that instance. Cleanup and shutdown can still block; termination
of the complete process tree is not yet confirmed on every failure path.

Historical Windows evidence: verified through real host transport: two consecutive 11-second JFK requests,
unchanged saved settings (CUDA-only, FP16, beam 3, UA, VAD=true, custom prompt),
both reference-word matches. This is not Ukrainian/bilingual quality qualification.
CPU, turbo, long-recording quality and live insertion remain unverified. Current
worker does not impose a fixed recording-duration limit, silently truncate or
auto-stop capture. Like the reference pipeline it currently builds features from
the full request; long-request memory optimization is unfinished. Allocation
failures return errors where handled. Generation policy has basic tests, not an exhaustive
faster-whisper parity certification. Interactive clipboard tests are opt-in only.
