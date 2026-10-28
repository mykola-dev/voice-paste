# Transcription Specification

> Historical Python-worker specification. Production uses the [native Rust worker](../apps/stt-worker/README.md), not Python.

## STT Engine: faster-whisper

### Why faster-whisper?
- CTranslate2 backend = fast inference
- Native CUDA support
- Same quality as OpenAI Whisper
- Multilingual auto-detection works well

### Model Choice

| Model | Size | Speed | Ukrainian Quality |
|-------|------|-------|-------------------|
| tiny | 75MB | Very fast | Poor |
| base | 142MB | Fast | Acceptable |
| small | 466MB | Medium | Good |
| **medium** | 1.5GB | Slower | **Very Good** |
| large-v3 | 3GB | Slow | Best |

**Default: `medium`** - Best balance for EN+UK quality on consumer GPUs.

## Worker Interface

### CLI Specification

```bash
python transcribe.py --input <path> [--model medium] [--device cuda]
```

**Arguments:**
- `--input` (required): Path to WAV file
- `--model`: Model size (default: medium)
- `--device`: cuda or cpu (default: cuda)

**Output:**
- Transcript text to stdout
- Errors to stderr
- Exit code 0 on success

### Implementation

```python
#!/usr/bin/env python3
import argparse
import sys
from faster_whisper import WhisperModel

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True)
    parser.add_argument("--model", default="medium")
    parser.add_argument("--device", default="cuda")
    args = parser.parse_args()
    
    try:
        model = WhisperModel(
            args.model,
            device=args.device,
            compute_type="float16" if args.device == "cuda" else "int8"
        )
    except Exception as e:
        if args.device == "cuda":
            # Fallback to CPU
            print(f"CUDA failed, using CPU: {e}", file=sys.stderr)
            model = WhisperModel(args.model, device="cpu", compute_type="int8")
        else:
            raise
    
    segments, _ = model.transcribe(
        args.input,
        language=None,  # Auto-detect
        beam_size=5,
        vad_filter=False  # Don't filter silence
    )
    
    text = " ".join(seg.text.strip() for seg in segments)
    print(text)

if __name__ == "__main__":
    main()
```

## GPU Configuration

### CUDA Requirements
- NVIDIA GPU with CUDA support
- CUDA Toolkit 11.x or 12.x
- cuDNN (bundled with faster-whisper wheel)

### Compute Types

| Device | Compute Type | Notes |
|--------|--------------|-------|
| CUDA | float16 | Fast, good quality |
| CUDA | int8_float16 | Faster, slightly lower quality |
| CPU | int8 | Reasonable speed |

### Fallback Logic

```
1. Try CUDA with float16
2. If CUDA fails → retry with CPU int8
3. If CPU fails → report error
```

## Language Handling

### Auto-Detection
- Don't pass `language` parameter
- Whisper auto-detects from audio
- Works well for EN/UK mixed speech

### No Manual Override
- Users requested no language switching
- Auto-detect handles bilingual naturally

## Model Caching

### Optional in-memory residency

`keepInMemory` defaults to `false` (also for older configuration files).

- **Off:** preload when recording starts and terminate the worker immediately after transcription,
  including empty results and failures. Cancelling a recording also releases its worker.
- **On:** load the selected model in the background at app startup or after enabling and saving.
  Keep one worker/model between dictations. This retains the model and CUDA context, but does not
  run a synthetic inference warm-up or change precision. The first inference may still be slower.
- Changing model, device or decoding options replaces the worker when the current dictation finishes.
  Hotkey/paste/UI changes do not reload it. Disabling during a dictation waits for it to finish.
- Exit always terminates the worker. Failed/timed-out workers are discarded; subsequent dictations
  can start a fresh worker. CUDA auto-fallback can retain the CPU worker when residency is enabled.

### Server protocol (`--wait`)

One audio path per stdin line; `QUIT` or EOF exits. Stdout contains one JSON object per line:

```json
{"type":"ready","device":"cuda"}
{"type":"result","text":"Recognized text"}
{"type":"result","text":""}
{"type":"error","message":"Transcription failed"}
```

`device` reports the actual device after constructor fallback. Diagnostics go to stderr.
Each request gets a new result task. An empty transcript is a successful response, not a timeout.
The non-server CLI still emits plain transcript text.

### Live footprint in Settings

The status below **Keep in memory** samples the running worker every two seconds while Settings
is open, even when residency is off. It names the actual running model/device (not an unsaved selection)
and distinguishes loading from ready/not loaded.

- RAM is the process's resident working set, including Python/model/buffers, in MiB.
- CUDA VRAM is process-specific `nvidia-smi` compute-app memory, summed across matching GPU rows.
  Missing tools, unsupported drivers, WDDM `N/A`, query failures or absent PID rows hide the VRAM
  entry, rather than showing unavailable, zero or whole-card usage. CPU workers do not show VRAM.
- Polling stops on window close; queries are asynchronous, cancellable and bounded to two seconds.

These are live process measurements, not model-file sizes or isolated weight-memory estimates.

### Location
- Default: `~/.cache/huggingface/hub/`
- First run downloads model (1.5GB for medium)

### First-Run Experience
- Show "Downloading model..." in overlay (future)
- MVP: just show "Transcribing..." (may be slow first time)

## Error Handling

| Error | Response |
|-------|----------|
| Model download fails | Show error, suggest retry |
| CUDA OOM | Fallback to CPU |
| Corrupt audio | Return empty string, log error |
| Timeout (>60s) | Kill worker, show error |
