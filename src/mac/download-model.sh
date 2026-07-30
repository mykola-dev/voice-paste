#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
BUILD_DIR="$ROOT_DIR/build/VoicePasteMac"
VENV_DIR="$BUILD_DIR/python"
MODELS_DIR="$BUILD_DIR/models"
MODEL="${1:-mlx-community/whisper-large-v3-turbo}"
ENDPOINT="${HF_ENDPOINT:-https://hf-mirror.com}"
TARGET_DIR="$MODELS_DIR/${MODEL//\//__}"
CONFIG_PATH="$HOME/Library/Application Support/VoicePaste/mac.config.json"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "VoicePaste Mac models require Apple Silicon macOS." >&2
  exit 1
fi

if [[ ! -d "$VENV_DIR" ]]; then
  python3 -m venv "$VENV_DIR"
fi

"$VENV_DIR/bin/python3" -m pip install --upgrade pip >/dev/null
"$VENV_DIR/bin/python3" -m pip install -r "$ROOT_DIR/src/mac/requirements.txt" >/dev/null

mkdir -p "$TARGET_DIR"

echo "Downloading $MODEL"
echo "Endpoint: $ENDPOINT"
echo "Target: $TARGET_DIR"

HF_ENDPOINT="$ENDPOINT" "$VENV_DIR/bin/python3" - "$MODEL" "$TARGET_DIR" <<'PY'
import sys
from huggingface_hub import snapshot_download

model = sys.argv[1]
target = sys.argv[2]

snapshot_download(
    repo_id=model,
    local_dir=target,
    local_dir_use_symlinks=False,
    resume_download=True,
)
print(target)
PY

mkdir -p "$(dirname "$CONFIG_PATH")"
if [[ -f "$CONFIG_PATH" ]]; then
  plutil -replace model -string "$TARGET_DIR" "$CONFIG_PATH"
else
  cp "$ROOT_DIR/config/mac.config.json" "$CONFIG_PATH"
  plutil -replace model -string "$TARGET_DIR" "$CONFIG_PATH"
fi

echo "Model downloaded and configured: $TARGET_DIR"
echo "Restart VoicePaste: pkill -f VoicePasteMac; open '$ROOT_DIR/build/VoicePasteMac/VoicePaste.app'"
