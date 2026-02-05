#!/usr/bin/env python3
"""
VoicePaste - Transcription Worker
Transcribes audio using faster-whisper with GPU support.
"""
import argparse
import sys
import io
import faulthandler
from pathlib import Path
from faster_whisper import WhisperModel

import time
import traceback
import os

# Force UTF-8 output on Windows (critical for Cyrillic)
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')


def get_russian_suppress_tokens(tokenizer) -> list[int]:
    """Get list of token IDs for Russian-only characters to suppress them."""
    russian_chars = "ыэъёЫЭЪЁ"
    suppress_tokens = []
    
    # Iterate through all token IDs and decode them to check for Russian-only characters
    for i in range(tokenizer.get_vocab_size()):
        try:
            decoded = tokenizer.decode([i])
            if any(char in decoded for char in russian_chars):
                suppress_tokens.append(i)
        except:
            continue
                
    return sorted(list(set(suppress_tokens)))


def transcribe_audio(
    audio_path: Path,
    model: WhisperModel,
    language_mode: str = "auto",
    beam_size: int = 5,
    custom_initial_prompt: str = "",
) -> dict:
    """
    Transcribe audio file using faster-whisper.
    
    Args:
        audio_path: Path to WAV file (16kHz mono)
        model: Loaded WhisperModel instance
        language_mode: Language mode (auto/en/ua/bilingual)
        beam_size: Beam size for transcription
    
    Returns:
        Dict with 'text', 'language', 'language_prob', 'duration_ms' keys
    
    Raises:
        FileNotFoundError: Audio file not found
        RuntimeError: Transcription failed
    """
    if not audio_path.exists():
        raise FileNotFoundError(f"Audio file not found: {audio_path}")
    
    start_time = time.perf_counter()

    # Determine language and initial_prompt based on mode
    language = None
    initial_prompt = None
    suppress_tokens = None

    def is_vad_dependency_error(err: Exception) -> bool:
        msg = str(err).lower()
        return (
            "vad" in msg
            or "silero" in msg
            or "torch" in msg
            or "onnxruntime" in msg
        )

    def run_transcribe(lang: str | None):
        try:
            if os.environ.get("VOICEPASTE_DEBUG", "0") in ("1", "true", "True"):
                print(
                    f"[Transcribe] model.transcribe(lang={lang}, vad_filter=True, beam_size={beam_size})",
                    file=sys.stderr,
                    flush=True,
                )
            faulthandler.dump_traceback_later(30, repeat=True, file=sys.stderr)
            return model.transcribe(
                str(audio_path),
                language=lang,
                beam_size=beam_size,
                vad_filter=True,
                initial_prompt=initial_prompt,
                suppress_tokens=suppress_tokens,
            )
        except (ModuleNotFoundError, ImportError, RuntimeError) as e:
            if is_vad_dependency_error(e):
                print(
                    "[VAD] Missing dependency for vad_filter. "
                    "Install required VAD deps (e.g., torch/onnxruntime).",
                    file=sys.stderr,
                )
                if os.environ.get("VOICEPASTE_DEBUG", "0") in ("1", "true", "True"):
                    traceback.print_exc(file=sys.stderr)
            raise
        finally:
            faulthandler.cancel_dump_traceback_later()

    if language_mode == "en":
        language = "en"
    elif language_mode in ("ua", "uk"):
        language = "uk"
        # Force Ukrainian spelling for UK mode too
        suppress_tokens = get_russian_suppress_tokens(model.hf_tokenizer)
    elif language_mode == "bilingual":
        # Bilingual mode: auto-detect but guide the model to prefer English and Ukrainian
        # This helps prevent Whisper from misidentifying Ukrainian as Russian.
        # We include Ukrainian-specific characters that don't exist in Russian (ґ, є, і, ї).
        language = None  # Allow auto-detection
        initial_prompt = (
            "English and Ukrainian. Transcribe in these languages. "
            "Англійська та українська мови. ґ, є, і, ї. "
        )
        # Suppress Russian tokens in Bilingual mode as requested by user
        suppress_tokens = get_russian_suppress_tokens(model.hf_tokenizer)
        
    # Append custom prompt if provided
    if custom_initial_prompt:
        if initial_prompt:
            initial_prompt = initial_prompt.strip() + " " + custom_initial_prompt.strip()
        else:
            initial_prompt = custom_initial_prompt.strip()

    segments, info = run_transcribe(language)
    
    # [Bilingual Fix] If we detected Russian but we're in bilingual mode (EN/UA),
    # it's highly likely it should have been Ukrainian.
    
    if language_mode == "bilingual" and info.language == "ru":
        print("[Language Guard] Detected Russian in Bilingual mode. Re-transcribing as Ukrainian with suppression...", file=sys.stderr)
        
        # Suppress Russian-only characters (ы, э, ъ, ё) to force Ukrainian spelling
        if suppress_tokens is None:
            suppress_tokens = get_russian_suppress_tokens(model.hf_tokenizer)
        
        segments, info = run_transcribe("uk")
    
    # Combine all segments into single text
    text = " ".join(seg.text.strip() for seg in segments)
    print(f"[Transcribe] Segments joined. Text length={len(text)}", file=sys.stderr, flush=True)
    
    end_time = time.perf_counter()
    duration_ms = int((end_time - start_time) * 1000)
    
    # Character level fallback to catch any Russian letters that leaked through
    if language_mode in ("ua", "uk", "bilingual"):
        # Replace remaining Russian letters with Ukrainian equivalents
        replacements = {
            'ы': 'и', 'Ы': 'И',
            'э': 'е', 'Э': 'Е',
            'ё': 'е', 'Ё': 'Е',
            'ъ': "'", 'Ъ': "'"
        }
        for ru, ua in replacements.items():
            text = text.replace(ru, ua)
    
    return {
        "text": text,
        "language": info.language,
        "language_prob": info.language_probability,
        "duration_ms": duration_ms
    }


def main():
    """CLI entry point."""
    parser = argparse.ArgumentParser(
        description="Transcribe audio using faster-whisper"
    )
    parser.add_argument(
        "--input",
        type=Path,
        help="Path to WAV file"
    )
    parser.add_argument(
        "--model",
        default="medium",
        help="Whisper model size (default: medium)"
    )
    parser.add_argument(
        "--device",
        default="cuda",
        choices=["cuda", "cpu"],
        help="Device to use (default: cuda)"
    )
    parser.add_argument(
        "--language-mode",
        default="auto",
        choices=["auto", "en", "ua", "bilingual"],
        help="Language mode (default: auto)"
    )
    parser.add_argument(
        "--initial-prompt",
        default="",
        help="Custom initial prompt to append to the default one"
    )
    parser.add_argument(
        "--beam-size",
        type=int,
        default=5,
        help="Beam size for transcription (default: 5)"
    )
    parser.add_argument(
        "--wait",
        action="store_true",
        help="Start in server mode: load model, print READY, and wait for input path on stdin"
    )
    parser.add_argument(
        "--check-model",
        action="store_true",
        help="Check if model is cached locally and exit"
    )
    parser.add_argument(
        "--download-model",
        action="store_true",
        help="Download model to cache and exit"
    )

    args = parser.parse_args()

    # Handle --check-model flag
    if args.check_model:
        try:
            from faster_whisper.utils import available_models
            # Simple check: try to construct model path
            print(f"Checking model: {args.model}")
            model = WhisperModel(args.model, device="cpu", compute_type="int8", download_root=None)
            print("MODEL_PRESENT")
            return 0
        except Exception as e:
            print(f"MODEL_MISSING: {e}", file=sys.stderr)
            return 1

    # Handle --download-model flag
    if args.download_model:
        try:
            print(f"Downloading model: {args.model}", flush=True)
            # Download by initializing model (faster-whisper auto-downloads to HF cache)
            WhisperModel(args.model, device="cpu", compute_type="int8")
            print("DOWNLOAD_COMPLETE", flush=True)
            return 0
        except Exception as e:
            print(f"DOWNLOAD_FAILED: {e}", file=sys.stderr)
            return 1

    # Determine compute type based on device
    compute_type = "float16" if args.device == "cuda" else "int8"
    
    try:
        model = WhisperModel(
            args.model,
            device=args.device,
            compute_type=compute_type
        )
    except Exception as e:
        # Fallback to CPU if CUDA fails
        if args.device == "cuda":
            print(f"CUDA failed, falling back to CPU: {e}", file=sys.stderr)
            model = WhisperModel(args.model, device="cpu", compute_type="int8")
        else:
            print(f"Error: {e}", file=sys.stderr)
            return 1

    print(
        f"[Worker] Model loaded: {args.model} on {args.device} (compute_type={compute_type})",
        file=sys.stderr,
        flush=True,
    )
    if os.environ.get("VOICEPASTE_DEBUG", "0") in ("1", "true", "True"):
        try:
            import torch  # type: ignore
            print(f"[Worker] torch={getattr(torch, '__version__', 'unknown')}", file=sys.stderr, flush=True)
        except Exception as e:
            print(f"[Worker] torch import failed: {e}", file=sys.stderr, flush=True)
        try:
            import onnxruntime  # type: ignore
            print(f"[Worker] onnxruntime={getattr(onnxruntime, '__version__', 'unknown')}", file=sys.stderr, flush=True)
        except Exception as e:
            print(f"[Worker] onnxruntime import failed: {e}", file=sys.stderr, flush=True)

    if args.wait:
        # Server mode
        print("READY", flush=True)
        for line in sys.stdin:
            input_path = Path(line.strip())
            if not input_path or str(input_path) == "QUIT":
                break
            
            try:
                print(f"[Worker] Received path: {input_path}", file=sys.stderr, flush=True)
                try:
                    size = input_path.stat().st_size
                    print(f"[Worker] Audio size: {size} bytes", file=sys.stderr, flush=True)
                except Exception as e:
                    print(f"[Worker] Audio stat failed: {e}", file=sys.stderr, flush=True)
                print("[Worker] Starting transcription...", file=sys.stderr, flush=True)
                result = transcribe_audio(
                    input_path,
                    model,
                    language_mode=args.language_mode,
                    beam_size=args.beam_size,
                    custom_initial_prompt=args.initial_prompt
                )
                print(f"[Timer] Transcription took {result['duration_ms']}ms", file=sys.stderr)
                print(f"[Worker] Done. Text length={len(result['text'])}", file=sys.stderr, flush=True)
                print(result["text"], flush=True)
                # Print separator for multiple requests if needed, 
                # but currently we'll likely restart or use one-at-a-time
            except Exception as e:
                print(f"Error: {e}", file=sys.stderr)
                print("", flush=True) # Empty line for error
    else:
        # One-off mode
        if not args.input:
            print("Error: --input is required in one-off mode", file=sys.stderr)
            return 1
            
        try:
            result = transcribe_audio(
                args.input,
                model,
                language_mode=args.language_mode,
                beam_size=args.beam_size,
                custom_initial_prompt=args.initial_prompt,
            )
            print(f"[Timer] Transcription took {result['duration_ms']}ms", file=sys.stderr)
            print(result["text"])
            return 0
        except Exception as e:
            print(f"Error: {e}", file=sys.stderr)
            return 1



if __name__ == "__main__":
    sys.exit(main())
