# macOS model delivery: evidence and pending integration

Metadata reviewed 2026-10-30; **no model weights were downloaded**. macOS is to
use whisper.cpp + Metal, not Windows CTranslate2 artifacts. No smaller-model,
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
successful delivery through the corporate network or actual inference quality.
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
- This can be a different client/protocol route, **not a guaranteed bypass** of
  corporate network restrictions. Do not alter proxy/TLS/security controls.
- Do not silently install Git LFS/Xet, Python, Homebrew or SDKs. Any optional
  tools and actual weight downloads need explicit approval.
- A future fallback must select only the requested pinned artifact (not clone
  every multi-GB variant), own/cancel its child process and staging files, reject
  pointers as completed weights, verify exact size/SHA-256, and atomically publish
  into the same model cache as the primary download path.

Current implementation: an explicit approved curl download/resume path is now
connected, with these full-size GGML artifacts and pinned Silero v6.2.0 from
`ggml-org/whisper-vad`, revision `9ffd54a1e1ee413ddf265af9913beaf518d1639b`.
Its 885098-byte `ggml-silero-v6.2.0.bin` has SHA-256
`2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987`.
The canonical runtime metadata is now `crates/core/assets/macos-model-artifacts.json`.
No Git fallback is connected or claimed to solve corporate-network delivery.
The assistant has not downloaded model/VAD weights. Worker source is locked by
Cargo: whisper-rs 0.16.0 / whisper-rs-sys 0.15.0, including whisper.cpp 1.8.3.
