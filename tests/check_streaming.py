"""Check faster-whisper streaming capabilities"""
from faster_whisper import WhisperModel
import inspect

model = WhisperModel("tiny", device="cpu")

print("=== WhisperModel.transcribe signature ===")
sig = inspect.signature(model.transcribe)
print(sig)

print("\n=== Transcribe parameters ===")
for k, v in sig.parameters.items():
    print(f"{k}: {v.annotation} = {v.default}")

print("\n=== Testing segment streaming ===")
# The transcribe method returns an iterator of segments!
# This means we CAN stream results as they become available

import sys
sys.exit(0)
