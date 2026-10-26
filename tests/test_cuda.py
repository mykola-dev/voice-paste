"""
Helper script to add CUDA libraries to PATH and test transcription.
"""
import os
from pathlib import Path
import site
import sys

# Find CUDA libraries installed for the current Python interpreter.
for packages_dir in [*site.getsitepackages(), site.getusersitepackages()]:
    cuda_lib_path = Path(packages_dir) / "nvidia" / "cublas" / "bin"
    if cuda_lib_path.is_dir():
        os.environ['PATH'] = str(cuda_lib_path) + os.pathsep + os.environ.get('PATH', '')
        print(f"Added to PATH: {cuda_lib_path}")
        break
else:
    print("WARNING: CUDA libraries not found in Python site-packages")

# Now import and run transcription
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
