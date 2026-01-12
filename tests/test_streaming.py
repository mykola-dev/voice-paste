#!/usr/bin/env python3
"""
Real-time streaming transcription with faster-whisper.
Demonstrates how to transcribe audio as segments arrive.
"""
import os
os.environ['PATH'] = r"C:\Users\mykola\AppData\Roaming\Python\Python314\site-packages\nvidia\cublas\bin" + os.pathsep + os.environ['PATH']

from faster_whisper import WhisperModel
from pathlib import Path
import time

def stream_transcribe(audio_file: Path, model_size: str = "medium", device: str = "cuda"):
    """
    Transcribe audio file and yield segments as they're generated.
    This simulates real-time transcription.
    """
    print(f"Loading model: {model_size} on {device}")
    model = WhisperModel(model_size, device=device, compute_type="float16" if device == "cuda" else "int8")
    
    print(f"Transcribing: {audio_file.name}")
    print("=" * 60)
    print("Streaming segments as they arrive:\n")
    
    # transcribe() returns a generator of segments!
    segments, info = model.transcribe(
        str(audio_file),
        language=None,  # Auto-detect
        beam_size=5,
        vad_filter=False,
        word_timestamps=False  # Can enable for word-level streaming
    )
    
    print(f"Detected language: {info.language} ({info.language_probability:.2%})\n")
    
    # Iterate through segments as they're generated
    for i, segment in enumerate(segments):
        # Each segment arrives as it's decoded
        text = segment.text.strip()
        
        # Print with timing
        start_time = segment.start
        end_time = segment.end
        
        print(f"[{start_time:.1f}s - {end_time:.1f}s] {text}")
        
        # In real app: paste this text immediately!
        yield {
            'text': text,
            'start': start_time,
            'end': end_time,
            'segment_id': i
        }
    
    print("\n" + "=" * 60)
    print("Transcription complete!")


if __name__ == "__main__":
    # Test with sample file
    audio_file = Path(__file__).parent / "samples" / "en_sample.wav"
    
    if not audio_file.exists():
        print(f"ERROR: Sample file not found: {audio_file}")
        exit(1)
    
    print("STREAMING TRANSCRIPTION TEST")
    print("=" * 60)
    print()
    
    start_time = time.time()
    
    # Process segments as they arrive
    segments_received = []
    for segment_data in stream_transcribe(audio_file, model_size="medium", device="cuda"):
        segments_received.append(segment_data)
        # In real app, we would paste segment_data['text'] here!
    
    elapsed = time.time() - start_time
    
    print(f"\nReceived {len(segments_received)} segments in {elapsed:.2f}s")
    print("\nFull transcript:")
    print(" ".join(s['text'] for s in segments_received))
