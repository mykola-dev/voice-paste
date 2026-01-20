#!/usr/bin/env python3
"""
VoicePaste - Transcription Worker
Transcribes audio using faster-whisper with GPU support.
"""
import argparse
import sys
import io
from pathlib import Path
from faster_whisper import WhisperModel

# Force UTF-8 output on Windows (critical for Cyrillic)
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')


def transcribe_audio(
    audio_path: Path,
    model_size: str = "medium",
    device: str = "cuda",
    language_mode: str = "auto",
) -> dict:
    """
    Transcribe audio file using faster-whisper.
    
    Args:
        audio_path: Path to WAV file (16kHz mono)
        model_size: Whisper model (tiny/base/small/medium/large-v3/large-v3-turbo)
        device: Device to use (cuda/cpu)
    
    Returns:
        Dict with 'text', 'language', 'language_prob' keys
    
    Raises:
        FileNotFoundError: Audio file not found
        RuntimeError: Transcription failed
    """
    if not audio_path.exists():
        raise FileNotFoundError(f"Audio file not found: {audio_path}")
    
    # Determine compute type based on device
    compute_type = "float16" if device == "cuda" else "int8"
    
    try:
        model = WhisperModel(
            model_size,
            device=device,
            compute_type=compute_type
        )
    except Exception as e:
        # Fallback to CPU if CUDA fails
        if device == "cuda":
            print(f"CUDA failed, falling back to CPU: {e}", file=sys.stderr)
            model = WhisperModel(model_size, device="cpu", compute_type="int8")
        else:
            raise RuntimeError(f"Failed to load model: {e}") from e
    
    # Determine language and initial_prompt based on mode
    language = None
    initial_prompt = None
    
    if language_mode == "en":
        language = "en"
    elif language_mode in ("ua", "uk"):
        language = "uk"
    elif language_mode == "bilingual":
        # Bilingual mode: auto-detect but guide the model to prefer English and Ukrainian
        # This helps prevent Whisper from misidentifying Ukrainian as Russian.
        # We include Ukrainian-specific characters that don't exist in Russian (ґ, є, і, ї).
        language = None  # Allow auto-detection
        initial_prompt = (
            "English and Ukrainian. Transcribe in these languages. "
            "Англійська та українська мови. ґ, є, і, ї. "
            "Слава Україні! Героям слава!"
        )
        

    segments, info = model.transcribe(
        str(audio_path),
        language=language,
        beam_size=5,
        vad_filter=False,  # Don't filter silence - user controls recording
        initial_prompt=initial_prompt,
    )
    
    # [Bilingual Fix] If we detected Russian but we're in bilingual mode (EN/UA),
    # it's highly likely it should have been Ukrainian.
    # We can't easily "re-transcribe" here without losing performance, 
    # but Whisper often picks RU for UA speech.
    # The best way to "fix" this is to force language=uk if detection picked ru
    # but the user said they are speaking EN/UA.
    
    if language_mode == "bilingual" and info.language == "ru":
        # If detection failed and picked Russian, we re-run with forced Ukrainian.
        # This only happens if the first pass picked RU.
        print("[Language Guard] Detected Russian in Bilingual mode. Re-transcribing as Ukrainian...", file=sys.stderr)
        segments, info = model.transcribe(
            str(audio_path),
            language="uk",
            beam_size=5,
            vad_filter=False,
            initial_prompt=initial_prompt,
        )
    
    # Log detected language for debugging
    #print(f"[Language Detection] Detected: {info.language} (probability: {info.language_probability:.2f})", file=sys.stderr)
    
    # Combine all segments into single text
    text = " ".join(seg.text.strip() for seg in segments)
    
    return {
        "text": text,
        "language": info.language,
        "language_prob": info.language_probability
    }


def main():
    """CLI entry point."""
    parser = argparse.ArgumentParser(
        description="Transcribe audio using faster-whisper"
    )
    parser.add_argument(
        "--input",
        required=True,
        type=Path,
        help="Path to WAV file"
    )
    parser.add_argument(
        "--model",
        default="medium",
        choices=[
            "tiny",
            "base",
            "small",
            "medium",
            "large-v2",
            "large-v3",
            "large-v3-turbo",
            "turbo",
            "distil-large-v2",
            "distil-large-v3",
            "distil-large-v3.5",
        ],
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

    args = parser.parse_args()

    try:
        result = transcribe_audio(
            args.input,
            args.model,
            args.device,
            language_mode=args.language_mode,
        )
        # Print only text for C# to parse
        print(result["text"])
        return 0
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
