#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
MAC_DIR="$ROOT_DIR/src/mac"
APP_DIR="$MAC_DIR/VoicePasteMac"
BUILD_DIR="$ROOT_DIR/build/VoicePasteMac"
VENV_DIR="$BUILD_DIR/python"
APP_BUNDLE="$BUILD_DIR/VoicePaste.app"

mkdir -p "$BUILD_DIR"

if [[ "$(uname -s)" != "Darwin" || "$(uname -m)" != "arm64" ]]; then
  echo "VoicePaste Mac builds require Apple Silicon macOS." >&2
  exit 1
fi

if [[ ! -d "$VENV_DIR" ]]; then
  python3 -m venv "$VENV_DIR"
fi

"$VENV_DIR/bin/python3" -m pip install --upgrade pip
"$VENV_DIR/bin/python3" -m pip install -r "$MAC_DIR/requirements.txt"

swift build --package-path "$APP_DIR" -c release --arch arm64

EXECUTABLE="$APP_DIR/.build/arm64-apple-macosx/release/VoicePasteMac"
if [[ ! -x "$EXECUTABLE" ]]; then
  EXECUTABLE="$APP_DIR/.build/release/VoicePasteMac"
fi

rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS" "$APP_BUNDLE/Contents/Resources"
cp "$EXECUTABLE" "$APP_BUNDLE/Contents/MacOS/VoicePasteMac"
cp "$MAC_DIR/transcribe_mlx.py" "$APP_BUNDLE/Contents/Resources/transcribe_mlx.py"
cp -R "$VENV_DIR" "$APP_BUNDLE/Contents/Resources/python"

cat > "$APP_BUNDLE/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>VoicePasteMac</string>
  <key>CFBundleIdentifier</key>
  <string>dev.voicepaste.mac</string>
  <key>CFBundleName</key>
  <string>VoicePaste</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>1.0.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>VoicePaste records microphone audio while you hold a dictation session.</string>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP_BUNDLE" >/dev/null

echo "Built: $APP_BUNDLE"
echo "Run: open '$APP_BUNDLE'"
