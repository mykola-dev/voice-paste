"""
Helper script to add CUDA libraries to PATH and test transcription.
"""
import os
import sys

# Add CUDA libraries to PATH
cuda_lib_path = r"C:\Users\mykola\AppData\Roaming\Python\Python314\site-packages\nvidia\cublas\bin"
if os.path.exists(cuda_lib_path):
    os.environ['PATH'] = cuda_lib_path + os.pathsep + os.environ['PATH']
    print(f"Added to PATH: {cuda_lib_path}")
else:
    print(f"WARNING: CUDA lib path not found: {cuda_lib_path}")

# Now import and run transcription
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent.parent / "src" / "transcribe"))

from transcribe import transcribe_audio

# Test with English sample
audio_file = Path(__file__).parent / "samples" / "en_sample.wav"
print(f"\nTesting GPU transcription with: {audio_file}")
print("="*60)

try:
    result = transcribe_audio(audio_file, model_size="medium", device="cuda")
    print(f"\nSUCCESS!")
    print(f"Transcript: {result['text']}")
    print(f"Language: {result['language']} ({result['language_prob']:.2%})")
except Exception as e:
    print(f"\nERROR: {e}")
    import traceback
    traceback.print_exc()
