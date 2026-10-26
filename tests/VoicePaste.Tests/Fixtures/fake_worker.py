"""Deterministic protocol peer. No Whisper, GPU, downloads or audio required."""
import json
import os
from pathlib import Path
import sys
import time

device, mode = sys.argv[1:3]
if mode == "init-exit" and device == "cuda":
    print("CUDA initialization failed", file=sys.stderr, flush=True)
    sys.exit(1)
if mode == "hang-init":
    print(f"Loading on {device}", file=sys.stderr, flush=True)
    time.sleep(30)
if mode == "init-fallback":
    device = "cpu"
print(json.dumps({"type": "ready", "device": device}), flush=True)
print(f"Model loaded on {device}", file=sys.stderr, flush=True)
for count, line in enumerate(sys.stdin, 1):
    text = Path(line.strip()).read_text(encoding="utf-8")
    if text == "EXIT":
        sys.exit(1)
    if text == "MALFORMED":
        print("not-json", flush=True)
        continue
    if text == "TIMEOUT":
        time.sleep(5)
    if text == "SLOW":
        time.sleep(0.5)
    if text == "ERROR" or (text == "CUDA_ERROR" and device == "cuda"):
        print(json.dumps({"type": "error", "message": "CUDA failed" if text == "CUDA_ERROR" else "Invalid audio"}), flush=True)
        continue
    result = "" if text == "EMPTY" else f"{os.getpid()}|{count}|{device}|{text}"
    print(json.dumps({"type": "result", "text": result}, ensure_ascii=False), flush=True)
