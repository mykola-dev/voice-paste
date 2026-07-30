# macOS Port

VoicePaste for macOS is a native menu bar app for Apple Silicon Macs. Intel Macs and CPU fallback are intentionally unsupported.

## Stack

- App: Swift/AppKit menu bar app
- Audio: AVFoundation `AVAudioEngine`
- Hotkey: Carbon `RegisterEventHotKey`; Right Option uses a CGEvent tap
- Paste: `NSPasteboard` + `CGEvent` `Cmd+V`
- Transcription: OpenAI Whisper on Apple MPS by default; MLX Whisper is still supported for local/HF MLX models

## Build

```bash
src/mac/build-mac.sh
```

Output:

```text
build/VoicePasteMac/VoicePaste.app
```

Run:

```bash
open build/VoicePasteMac/VoicePaste.app
```

## Development Run

```bash
python3 -m venv build/VoicePasteMac/python
build/VoicePasteMac/python/bin/python3 -m pip install -r src/mac/requirements.txt
VOICEPASTE_MAC_PYTHON="$PWD/build/VoicePasteMac/python/bin/python3" swift run --package-path src/mac/VoicePasteMac
```

## Permissions

VoicePaste needs:

- Microphone: recording dictation audio.
- Accessibility: synthetic `Cmd+V` paste into the previously focused app.

Use the menu bar item `VP` -> `Check Permissions` if paste does not work.

## Defaults

- Hotkey: Right Option
- Model: `openai-whisper:large-v3-turbo`
- Language: bilingual English + Ukrainian
- Config: `~/Library/Application Support/VoicePaste/mac.config.json`

## Model Download

The default macOS model is `openai-whisper:large-v3-turbo`. It downloads from OpenAI's Azure CDN and runs on Apple MPS acceleration, avoiding HuggingFace.

MLX models are also supported. If your network blocks `huggingface.co`, download the MLX model on another network and set `Model` in Settings to the local model folder path.

You can also try a HuggingFace-compatible mirror:

```bash
src/mac/download-model.sh
```

The script defaults to `https://hf-mirror.com`, downloads the MLX model into `build/VoicePasteMac/models/`, and updates `~/Library/Application Support/VoicePaste/mac.config.json` to use the local folder. Some mirrors still redirect to HuggingFace and may be blocked by corporate filtering.

To use another endpoint:

```bash
HF_ENDPOINT="https://your-mirror.example" src/mac/download-model.sh mlx-community/whisper-large-v3-turbo
```

Example local model path:

```text
/Users/you/Models/whisper-large-v3-turbo-mlx
```

Validate and enable a local MLX model:

```bash
src/mac/use-local-mlx-model.sh /Users/you/Models/whisper-large-v3-turbo-mlx
```

If the folder came from `git clone`, it must include Git LFS files. A folder with only `config.json`, `README.md`, and `.gitattributes` is incomplete.

Correct clone flow on a network that allows HuggingFace:

```bash
brew install git-lfs
git lfs install
git clone https://huggingface.co/mlx-community/whisper-large-v3-turbo-4bit
cd whisper-large-v3-turbo-4bit
git lfs pull
ls -lh weights.safetensors weights.npz 2>/dev/null
```

## Notes

- The Windows WPF app remains unchanged.
- macOS transcription uses MLX, not CUDA/faster-whisper.
- The app fails fast on non-Apple-Silicon machines.
