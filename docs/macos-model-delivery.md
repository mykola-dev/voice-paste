# macOS model delivery: current source and historical evidence

## Current delivery revision: GitHub Models assets

Published: [Models](https://github.com/mykola-dev/voice-paste/releases/tag/models).
All eight assets (six model parts, manifest and license notices) passed remote
size/digest comparison with local files before publication. Streaming the ordered
local parts reconstructed all four original pinned SHA-256 values. Unauthenticated
public range downloads of every part and a complete VAD download were verified
locally. Code `a918028` passed macOS build/package, clippy and host/worker tests in
[CI 41333232913](https://github.com/mykola-dev/voice-paste/actions/runs/41333232913).
These checks do not establish delivery on the tested Mac or real inference acceptance.

The approved replacement for the historical Hugging Face/Git delivery below is
the separate `Models` GitHub release (`models` tag). It is not an application
release and does not change the application version. The original full-size
GGML and Silero files retain their existing pinned sizes and SHA-256 identities.
Release files are raw ordered parts, not compressed archives; concatenating them
reconstructs the original bytes. Each part and the complete artifact must pass
size/SHA-256 validation before publication to the application cache.

macOS selects this source; Windows keeps its original HTTPS source without a
Git LFS fallback. The part lifecycle is shared code, while curl/WinHTTP are native
transport effects. No Git LFS installation is required. Actual macOS device
large-asset delivery and inference remain device acceptance, not CI claims.

macOS automatically downloads the saved model during initial preparation and after
a saved model change, like Windows; retry after cancellation or failure does not
require confirmation on either platform. Draft queries, builds, Settings-only mode
and offline file transcription do not download models. User reports that macOS is working do not by themselves
qualify delivery on the tested Mac or every model/device.

The following sections document historical investigation and are not the new
source-selection policy.

Metadata reviewed 2026-10-30; Git LFS weight delivery tested locally on Windows
on 2026-11-01 with user approval (see below). macOS uses
whisper.cpp + Metal, not Windows CTranslate2 artifacts. No smaller-model,
quantized-model, beam-size or VAD substitution is authorized.

## Official whisper.cpp model repository

The upstream model card links `ggerganov/whisper.cpp` for converted GGML
weights. Hugging Face's revision API returned commit
`5359861c739e955e79d9a303bcbc70fb988958b1`. At that exact revision, the `raw/`
URLs below return **Git LFS pointer text**, not weights:

| File | Bytes from pointer | SHA-256 from pointer |
| --- | ---: | --- |
| `ggml-large-v2.bin` | 3094623691 | `9a423fe4d40c82774b6af34115b8b935f34152246eb19e80e376071d3f999487` |
| `ggml-large-v3.bin` | 3095033483 | `64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2` |
| `ggml-large-v3-turbo.bin` | 1624555275 | `1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69` |

These are the non-`q*` GGML artifacts, not the separately listed Core ML encoder
archives. The model card's 40-character SHA column is not a SHA-256 checksum;
use the pointer's 64-character `oid sha256` for future integrity verification.
This metadata does **not** prove model compatibility with a future pinned worker,
successful delivery on the tested Mac or actual inference quality.
Before runtime integration, put verified metadata in the canonical backend-specific
artifact manifest; do not parse this document as application configuration.

Sources:

- [Model card](https://huggingface.co/ggerganov/whisper.cpp/blob/5359861c739e955e79d9a303bcbc70fb988958b1/README.md)
- [Revision API](https://huggingface.co/api/models/ggerganov/whisper.cpp/revision/5359861c739e955e79d9a303bcbc70fb988958b1)
- [large-v2 pointer](https://huggingface.co/ggerganov/whisper.cpp/raw/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v2.bin)
- [large-v3 pointer](https://huggingface.co/ggerganov/whisper.cpp/raw/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3.bin)
- [large-v3-turbo pointer](https://huggingface.co/ggerganov/whisper.cpp/raw/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo.bin)

## Git fallback is not an independent weight mirror

The [official Xet Git guide](https://huggingface.co/docs/hub/xet/using-xet-storage)
requires Git LFS and optionally Git Xet for the large-file transfer. The
[official compatibility description](https://huggingface.co/docs/hub/xet/legacy-git-lfs)
states that Xet-backed repositories retain Git LFS pointers, and legacy clients
obtain reconstructed file URLs through an LFS bridge. Consequently:

- A successful `git clone`/`ls-remote`, README fetch or pointer fetch is not a
  successful model download.
- Plain Git cannot reconstruct the weights from the small pointer file. LFS/Xet
  still needs reachable weight-storage endpoints, potentially the blocked ones.
- This can be a different client/protocol route, **not a guaranteed solution** to
  network restrictions. Do not alter proxy/TLS/security controls.
- Do not silently install Git LFS/Xet, Python, Homebrew or SDKs. Any optional
  tools and actual weight downloads need explicit approval.
- The fallback must select only the requested pinned artifact (not clone
  every multi-GB variant), own/cancel its child process and staging files, reject
  pointers as completed weights, verify exact size/SHA-256, and atomically publish
  into the same model cache as the primary download path.

Historical implementation at that investigation stage: an explicit approved curl download/resume path was
connected, with these full-size GGML artifacts and pinned Silero v6.2.0 from
`ggml-org/whisper-vad`, revision `9ffd54a1e1ee413ddf265af9913beaf518d1639b`.
Its 885098-byte `ggml-silero-v6.2.0.bin` has SHA-256
`2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`.
The canonical runtime metadata is now `crates/core/assets/macos-model-artifacts.json`.
The explicit download now tries direct HTTPS first, then Git LFS once if HTTPS
fails or returns invalid bytes. Both sources use the same pinned metadata and
shared verification/publication scenario. Cancellation never starts another source.
The Git scenario is shared by both catalogs (`adapters/model_git.rs`), including
the pinned revision/file selection, source staging, verification and cancellable
copy into the already-open staging handle. The shared source-switch policy lives
in `model_preparation.rs`. Only child creation/cleanup is target-selected:
Windows hidden suspended-until-job-owned child trees versus macOS process groups
and Finder executable discovery. WinHTTP and curl remain native HTTP transports,
not independent provisioning state machines.
Git/LFS subprocess trees are owned and terminated on cancellation/error, with
network low-speed timeouts, bounded diagnostics and a 30-minute command deadline.
Only already installed Git/LFS are used (including conventional Homebrew paths
when launched from Finder); no installation or security-control changes occur.
Git uses a private temporary checkout and selects only the requested LFS file.
Its staging is removed on failure/cancel; the independent HTTPS partial survives
until Git weights are fully verified. Copying verified weights truncates that
partial first, so cancellation during the copy retains only a valid prefix of
the pinned artifact, never mixed source bytes. Git retries start a fresh checkout.
During Git transfer the menu reports the source and total size, not a fabricated
byte-progress counter. Verified Git bytes replace only HTTP staging, then shared
publication rechecks size/hash/cancellation before replacing the live model.

At that stage, after a failed startup offline verification, the shared session offered the native
download confirmation once when idle (outside framework/runtime borrows). Cancel
does not enable networking; the menu remains available for retry. Ready local
models, preview and Settings-only modes do not prompt. Consent is tied to the
selected model and retained under command backpressure.

## Local Git delivery evidence — 2026-11-01

The disposable `build/git-model-probe.ps1` simulated primary unavailability and
downloaded only `ggml-large-v3-turbo.bin` at the pinned revision through existing
Git 2.45.1 and Git LFS 3.5.1 on Windows. Actual weights verified as
1624555275 bytes, SHA-256
`1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69`.
Partial-clone lazy fetching failed (`expected packfile`); a shallow Git fetch
without blob filtering succeeded, so the production fallback uses that route.
This was one approved model download, not VAD delivery or macOS device
acceptance, and not an end-to-end production adapter test.

Worker source is locked by
Cargo: whisper-rs 0.16.0 / whisper-rs-sys 0.15.0, including whisper.cpp 1.8.3.
