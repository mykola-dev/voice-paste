#!/usr/bin/env python3
"""
VoicePaste - Transcription Worker
Transcribes audio using faster-whisper with GPU support.
"""
import argparse
import io
import json
import os
from pathlib import Path
import site
import sys
import time
import traceback

from faster_whisper import WhisperModel


def configure_cuda_paths() -> None:
    """Prefer this interpreter's CUDA DLLs, then user-installed native DLL packages."""
    package_dirs = [*site.getsitepackages(), site.getusersitepackages()]
    # Embedded Python may rely on native CUDA DLLs installed for another user Python.
    # Discover those folders rather than pinning a username or interpreter version.
    package_dirs.extend(str(directory) for directory in
                        sorted(Path(site.getuserbase()).glob("Python*/site-packages"), reverse=True))
    cuda_paths = []
    found_libraries = set()
    for packages_dir in package_dirs:
        for library in ("cublas", "cudnn"):
            if library in found_libraries:
                continue
            directory = Path(packages_dir) / "nvidia" / library / "bin"
            if directory.is_dir() and str(directory) not in cuda_paths:
                cuda_paths.append(str(directory))
                found_libraries.add(library)
    if cuda_paths:
        current_paths = [entry for entry in os.environ.get("PATH", "").split(os.pathsep)
                         if entry and entry not in cuda_paths]
        os.environ["PATH"] = os.pathsep.join([*cuda_paths, *current_paths])


def debug_log(message: str) -> None:
    """Keep verbose diagnostics off stdout and disabled unless explicitly requested."""
    if os.environ.get("VOICEPASTE_DEBUG", "0").lower() in ("1", "true"):
        print(message, file=sys.stderr, flush=True)


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
        except Exception:
            continue
                
    return suppress_tokens


def transcribe_audio(
    audio_path: Path,
    model: WhisperModel,
    language_mode: str = "auto",
    beam_size: int = 5,
    custom_initial_prompt: str = "",
    enable_vad: bool = False,
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
    # Keep Whisper's default non-speech suppression in every language mode.
    suppress_tokens = [-1]

    def is_vad_dependency_error(err: Exception) -> bool:
        msg = str(err).lower()
        return (
            "vad" in msg
            or "silero" in msg
            or "torch" in msg
            or "onnxruntime" in msg
        )

    vad_enabled = enable_vad

    def run_transcribe(lang: str | None):
        try:
            debug_log(f"[Transcribe] lang={lang}, vad_filter={vad_enabled}, beam_size={beam_size}")
            if vad_enabled:
                return model.transcribe(
                    str(audio_path),
                    language=lang,
                    beam_size=beam_size,
                    vad_filter=True,
                    initial_prompt=initial_prompt,
                    suppress_tokens=suppress_tokens,
                )
            return model.transcribe(
                str(audio_path),
                language=lang,
                beam_size=beam_size,
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
                if os.environ.get("VOICEPASTE_DEBUG", "0").lower() in ("1", "true"):
                    traceback.print_exc(file=sys.stderr)
            raise

    if language_mode == "en":
        language = "en"
    elif language_mode in ("ua", "uk"):
        language = "uk"
        # Force Ukrainian spelling for UK mode too
        suppress_tokens.extend(get_russian_suppress_tokens(model.hf_tokenizer))
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
        suppress_tokens.extend(get_russian_suppress_tokens(model.hf_tokenizer))
        
    # Append custom prompt if provided
    if custom_initial_prompt:
        if initial_prompt:
            initial_prompt = initial_prompt.strip() + " " + custom_initial_prompt.strip()
        else:
            initial_prompt = custom_initial_prompt.strip()

    segments, info = run_transcribe(language)
    
    # Retry likely misidentified Ukrainian speech in bilingual mode.
    
    if language_mode == "bilingual" and info.language == "ru":
        debug_log("[Language Guard] Re-transcribing as Ukrainian with suppression")
        
        segments, info = run_transcribe("uk")
    
    # Combine all segments into single text
    text = " ".join(seg.text.strip() for seg in segments)
    
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
        "--no-fallback",
        action="store_true",
        help="Fail instead of falling back to CPU if CUDA fails"
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
        "--vad",
        action="store_true",
        help="Enable VAD (silence trimming). Requires onnxruntime."
    )
    parser.add_argument(
        "--wait",
        action="store_true",
        help="Server mode: load once, accept paths on stdin, emit JSON ready/result/error messages"
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
    configure_cuda_paths()

    # Handle --check-model flag
    if args.check_model:
        try:
            print(f"Checking model: {args.model}")
            WhisperModel(args.model, device="cpu", compute_type="int8", local_files_only=True)
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
        if args.device == "cuda":
            if args.no_fallback:
                print(f"CUDA_ERROR: {e}", file=sys.stderr)
                return 1
            else:
                print(f"CUDA failed, falling back to CPU: {e}", file=sys.stderr)
                model = WhisperModel(args.model, device="cpu", compute_type="int8")
                args.device = "cpu"
                compute_type = "int8"
        else:
            print(f"Error: {e}", file=sys.stderr)
            return 1

    print(
        f"[Worker] Model loaded: {args.model} on {args.device} (compute_type={compute_type})",
        file=sys.stderr,
        flush=True,
    )

    if args.wait:
        # Server mode
        print(json.dumps({"type": "ready", "device": args.device}), flush=True)
        for line in sys.stdin:
            path_text = line.strip()
            if path_text == "QUIT":
                break
            if not path_text:
                continue
            input_path = Path(path_text)
            
            try:
                result = transcribe_audio(
                    input_path,
                    model,
                    language_mode=args.language_mode,
                    beam_size=args.beam_size,
                    custom_initial_prompt=args.initial_prompt,
                    enable_vad=args.vad
                )
                debug_log(f"[Timer] Transcription took {result['duration_ms']}ms")
                print(json.dumps({"type": "result", "text": result["text"]}, ensure_ascii=False), flush=True)
            except Exception as e:
                print(f"Error: {e}", file=sys.stderr)
                print(json.dumps({"type": "error", "message": str(e)}, ensure_ascii=False), flush=True)
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
                enable_vad=args.vad,
            )
            debug_log(f"[Timer] Transcription took {result['duration_ms']}ms")
            print(result["text"])
            return 0
        except Exception as e:
            print(f"Error: {e}", file=sys.stderr)
            return 1



if __name__ == "__main__":
    # Configure CLI streams only; importing the worker must not replace the caller's streams.
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding="utf-8")
    sys.exit(main())
