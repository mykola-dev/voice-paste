#!/usr/bin/env python3
"""VoicePaste macOS transcription worker using MLX Whisper on Apple Silicon."""
from __future__ import annotations

import argparse
import contextlib
import io
import json
import platform
import sys
import time
import wave
from pathlib import Path
from typing import Any


RUSSIAN_ONLY_REPLACEMENTS = {
    "ы": "и",
    "Ы": "И",
    "э": "е",
    "Э": "Е",
    "ё": "е",
    "Ё": "Е",
    "ъ": "'",
    "Ъ": "'",
}


def require_apple_silicon() -> None:
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise RuntimeError("VoicePaste Mac requires Apple Silicon (Darwin arm64). Intel/CPU fallback is intentionally unsupported.")


def import_mlx_whisper():
    try:
        import mlx_whisper  # type: ignore
    except Exception as exc:  # pragma: no cover - depends on host deps
        raise RuntimeError("mlx-whisper is not installed. Run src/mac/build-mac.sh or install src/mac/requirements.txt.") from exc
    return mlx_whisper


def import_openai_whisper():
    try:
        import torch  # type: ignore
        import whisper  # type: ignore
    except Exception as exc:  # pragma: no cover - depends on host deps
        raise RuntimeError("openai-whisper is not installed. Run src/mac/build-mac.sh or install src/mac/requirements.txt.") from exc
    if not torch.backends.mps.is_available():
        raise RuntimeError("Apple MPS acceleration is unavailable. CPU fallback is intentionally unsupported.")
    return whisper, torch


def normalize_text(text: str, language_mode: str) -> str:
    text = " ".join(text.split())
    if language_mode in {"ua", "uk", "bilingual"}:
        for source, replacement in RUSSIAN_ONLY_REPLACEMENTS.items():
            text = text.replace(source, replacement)
    return text.strip()


def format_error(exc: Exception) -> str:
    message = str(exc)
    lowered = message.lower()
    if "403" in message and ("huggingface" in lowered or "category_denied" in lowered or "not allowed to browse" in lowered):
        return (
            "HuggingFace model download is blocked by the current network policy. "
            "Connect through a network that allows huggingface.co, or set Model in Settings "
            "to a local MLX Whisper model folder that is already downloaded. "
            f"Original error: {message}"
        )
    return message


def transcribe(args: argparse.Namespace) -> str:
    require_apple_silicon()
    if args.model.startswith("openai-whisper:"):
        return transcribe_openai_whisper(args)

    validate_local_mlx_model(args.model)
    mlx_whisper = import_mlx_whisper()

    if not args.input.exists():
        raise FileNotFoundError(f"Audio file not found: {args.input}")

    language: str | None = None
    if args.language_mode == "en":
        language = "en"
    elif args.language_mode in {"ua", "uk"}:
        language = "uk"

    prompt = args.initial_prompt.strip() if args.initial_prompt else ""
    if args.language_mode == "bilingual":
        bilingual_prompt = "English and Ukrainian. Українська мова: ґ, є, і, ї. Do not use Russian spelling."
        prompt = f"{bilingual_prompt} {prompt}".strip()

    kwargs: dict[str, Any] = {
        "path_or_hf_repo": args.model,
        "verbose": False,
    }
    if language:
        kwargs["language"] = language
    if prompt:
        kwargs["initial_prompt"] = prompt

    start = time.perf_counter()
    audio = load_pcm_wav(args.input)
    captured_stdout = io.StringIO()
    captured_stderr = io.StringIO()
    with contextlib.redirect_stdout(captured_stdout), contextlib.redirect_stderr(captured_stderr):
        result = mlx_whisper.transcribe(audio, **kwargs)
    diagnostics = (captured_stdout.getvalue() + captured_stderr.getvalue()).strip()
    if diagnostics:
        print(diagnostics, file=sys.stderr, flush=True)
    duration_ms = int((time.perf_counter() - start) * 1000)

    if isinstance(result, dict):
        text = str(result.get("text", ""))
        language_result = result.get("language")
    else:
        text = str(result)
        language_result = None

    text = normalize_text(text, args.language_mode)
    print(json.dumps({"duration_ms": duration_ms, "language": language_result}, ensure_ascii=False), file=sys.stderr, flush=True)
    return text


def transcribe_openai_whisper(args: argparse.Namespace) -> str:
    whisper, _ = import_openai_whisper()
    model_name = args.model.split(":", 1)[1].strip() or "large-v3-turbo"

    if not args.input.exists():
        raise FileNotFoundError(f"Audio file not found: {args.input}")

    language: str | None = None
    if args.language_mode == "en":
        language = "en"
    elif args.language_mode in {"ua", "uk"}:
        language = "uk"

    prompt = args.initial_prompt.strip() if args.initial_prompt else ""
    if args.language_mode == "bilingual":
        bilingual_prompt = "English and Ukrainian. Українська мова: ґ, є, і, ї. Do not use Russian spelling."
        prompt = f"{bilingual_prompt} {prompt}".strip()

    start = time.perf_counter()
    model = load_openai_whisper_mps(whisper, model_name)
    audio = load_pcm_wav(args.input)
    captured_stdout = io.StringIO()
    captured_stderr = io.StringIO()
    with contextlib.redirect_stdout(captured_stdout), contextlib.redirect_stderr(captured_stderr):
        result = model.transcribe(
            audio,
            language=language,
            initial_prompt=prompt or None,
            fp16=False,
            verbose=None,
        )
    duration_ms = int((time.perf_counter() - start) * 1000)
    diagnostics = (captured_stdout.getvalue() + captured_stderr.getvalue()).strip()
    if diagnostics:
        print(diagnostics, file=sys.stderr, flush=True)
    text = normalize_text(str(result.get("text", "")), args.language_mode)
    print(json.dumps({"duration_ms": duration_ms, "language": result.get("language"), "backend": "openai-whisper-mps"}, ensure_ascii=False), file=sys.stderr, flush=True)
    return text


def load_openai_whisper_mps(whisper, model_name: str):
    """Load OpenAI Whisper on Apple MPS without tripping over sparse buffers."""
    model = whisper.load_model(model_name, device="cpu")
    alignment_heads = getattr(model, "alignment_heads", None)
    if alignment_heads is not None and getattr(alignment_heads, "is_sparse", False):
        model.register_buffer("alignment_heads", alignment_heads.to_dense(), persistent=False)
    return model.to("mps")


def load_pcm_wav(audio_path: Path):
    import numpy as np
    from scipy.signal import resample_poly  # type: ignore

    with wave.open(str(audio_path), "rb") as wav:
        channels = wav.getnchannels()
        sample_width = wav.getsampwidth()
        sample_rate = wav.getframerate()
        frames = wav.readframes(wav.getnframes())

    if sample_width != 2:
        raise RuntimeError(f"Expected 16-bit PCM WAV, got sample width {sample_width}")

    audio = np.frombuffer(frames, dtype=np.int16).astype(np.float32) / 32768.0
    if channels > 1:
        audio = audio.reshape(-1, channels).mean(axis=1)
    if sample_rate != 16000:
        audio = resample_poly(audio, 16000, sample_rate).astype(np.float32)
    return audio


def validate_local_mlx_model(model: str) -> None:
    model_path = Path(model).expanduser()
    if not model_path.exists():
        return
    required = model_path / "config.json"
    weights_safetensors = model_path / "weights.safetensors"
    weights_npz = model_path / "weights.npz"
    if not required.exists():
        raise RuntimeError(f"Local MLX model is missing config.json: {model_path}")
    if not weights_safetensors.exists() and not weights_npz.exists():
        raise RuntimeError(
            "Local MLX model folder is incomplete. Expected weights.safetensors or weights.npz. "
            "If you used git clone, run git lfs install && git lfs pull in the model folder, "
            f"or use openai-whisper:large-v3-turbo. Folder: {model_path}"
        )


def preload(args: argparse.Namespace) -> int:
    require_apple_silicon()
    if args.model.startswith("openai-whisper:"):
        whisper, _ = import_openai_whisper()
        model_name = args.model.split(":", 1)[1].strip() or "large-v3-turbo"
        load_openai_whisper_mps(whisper, model_name)
        print("READY", flush=True)
        return 0

    validate_local_mlx_model(args.model)
    import_mlx_whisper()
    # mlx-whisper does not expose a stable public preload API across versions.
    # Keep startup non-blocking and validate the accelerated runtime here; model download/compile happens on first transcription.
    print("MLX runtime ready; model will load on first transcription", file=sys.stderr)
    print("READY", flush=True)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Transcribe audio with MLX Whisper on Apple Silicon")
    parser.add_argument("--input", type=Path, help="Path to WAV/audio file")
    parser.add_argument("--model", default="mlx-community/whisper-large-v3-turbo", help="MLX Whisper model repo/path")
    parser.add_argument("--language-mode", default="bilingual", choices=["auto", "en", "ua", "uk", "bilingual"])
    parser.add_argument("--initial-prompt", default="")
    parser.add_argument("--preload", action="store_true", help="Validate Apple Silicon + MLX and preload model if supported")
    args = parser.parse_args()

    try:
        if args.preload:
            return preload(args)
        if args.input is None:
            raise RuntimeError("--input is required unless --preload is used")
        print(transcribe(args), flush=True)
        return 0
    except Exception as exc:
        print(f"ERROR: {format_error(exc)}", file=sys.stderr, flush=True)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
