#!/usr/bin/env bash
set -euo pipefail

MODEL_DIR="${1:-}"
CONFIG_PATH="$HOME/Library/Application Support/VoicePaste/mac.config.json"

if [[ -z "$MODEL_DIR" ]]; then
  echo "Usage: src/mac/use-local-mlx-model.sh /path/to/mlx-model-folder" >&2
  exit 2
fi

MODEL_DIR="$(cd "$MODEL_DIR" && pwd)"

if [[ ! -f "$MODEL_DIR/config.json" ]]; then
  echo "Missing config.json in: $MODEL_DIR" >&2
  exit 1
fi

if [[ ! -f "$MODEL_DIR/weights.safetensors" && ! -f "$MODEL_DIR/weights.npz" ]]; then
  echo "Incomplete MLX model: missing weights.safetensors or weights.npz" >&2
  echo "Folder: $MODEL_DIR" >&2
  echo >&2
  echo "If this is a HuggingFace git clone, install Git LFS and run:" >&2
  echo "  cd '$MODEL_DIR'" >&2
  echo "  git lfs install" >&2
  echo "  git lfs pull" >&2
  exit 1
fi

mkdir -p "$(dirname "$CONFIG_PATH")"
if [[ ! -f "$CONFIG_PATH" ]]; then
  cp "$(cd "$(dirname "$0")/../.." && pwd)/config/mac.config.json" "$CONFIG_PATH"
fi

plutil -replace model -string "$MODEL_DIR" "$CONFIG_PATH"
echo "VoicePaste configured to use local MLX model: $MODEL_DIR"
echo "Restart VoicePaste: pkill -f VoicePasteMac; open build/VoicePasteMac/VoicePaste.app"
