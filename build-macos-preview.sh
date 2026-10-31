#!/bin/bash
# Bootstrap only; Rust xtask owns compilation, signing and packaging.
set -euo pipefail

fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }
ROOT="$(cd "$(dirname "$0")" && pwd -P)"
cd "$ROOT"

ASSUME_YES=false
case "${1:-}" in
  '') ;;
  --yes) ASSUME_YES=true; shift ;;
  *) fail 'Usage: bash build-macos-preview.sh [--yes]' ;;
esac
[[ $# -eq 0 ]] || fail 'Usage: bash build-macos-preview.sh [--yes]'
[[ "$(uname -s)" == Darwin ]] || fail 'Requires macOS; run this script on the Mac, not Windows or Linux.'
if [[ "$(uname -m)" != arm64 ]]; then
  # A translated Terminal reports x86_64 even on Apple Silicon. Restart only
  # this script natively; do not change Terminal settings or install Rosetta.
  [[ "$(/usr/sbin/sysctl -n hw.optional.arm64 2>/dev/null || true)" == 1 ]] || fail 'This Mac reports no ARM64 support. Intel Macs are not supported by this preview.'
  [[ "${VOICEPASTE_NATIVE_RESTART:-}" != 1 ]] || fail 'ARM64 restart did not take effect. Open a native Terminal and retry.'
  printf 'Terminal reports a translated architecture; restarting this build script as ARM64.\n'
  if [[ "$ASSUME_YES" == true ]]; then
    exec /usr/bin/env VOICEPASTE_NATIVE_RESTART=1 /usr/bin/arch -arm64 /bin/bash "$ROOT/build-macos-preview.sh" --yes
  else
    exec /usr/bin/env VOICEPASTE_NATIVE_RESTART=1 /usr/bin/arch -arm64 /bin/bash "$ROOT/build-macos-preview.sh"
  fi
fi
[[ -z "${CARGO_BUILD_TARGET:-}" ]] || fail 'Unset CARGO_BUILD_TARGET before building the native preview.'

# Validate Apple tools BEFORE offering a Rust download. Never install an SDK,
# accept a license, change xcode-select, or request sudo from this script.
xcode-select -p >/dev/null 2>&1 || fail 'Xcode is not selected. Install/select Xcode yourself, then retry.'
for tool in clang metal lipo otool; do
  xcrun --find "$tool" >/dev/null 2>&1 || fail "Xcode tool '$tool' is unavailable. Full Xcode with Metal tools is required; Command Line Tools alone may not suffice."
done
xcrun clang --version >/dev/null 2>&1 || fail 'Xcode is not ready. Open Xcode and complete its setup, then retry.'
[[ ! -e build/release/VoicePaste.app ]] || fail 'build/release/VoicePaste.app already exists. Close it and explicitly move it aside before rebuilding; this script will not replace a possibly running app.'

VERSION="$(sed -n 's/^channel = "\([^"]*\)".*/\1/p' rust-toolchain.toml)"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail 'Expected an exact Rust version in rust-toolchain.toml.'
CACHE="$ROOT/build/cache"
mkdir -p "$CACHE/tmp"
export TMPDIR="$(cd "$CACHE/tmp" && pwd -P)/"
export CARGO_TARGET_DIR="$CACHE/target"
if [[ -z "${LIBCLANG_PATH:-}" ]]; then
  CLANG_LIB="$(dirname "$(xcrun --find clang)")/../lib"
  [[ -f "$CLANG_LIB/libclang.dylib" ]] || fail 'Xcode libclang.dylib is missing; whisper.cpp bindings cannot be built.'
  export LIBCLANG_PATH="$CLANG_LIB"
fi
# whisper.cpp is built from the locked Cargo source; CMake is the only extra
# build tool. Reuse installed CMake, otherwise ask before a repository-local
# official Kitware download. Never install Homebrew, an SDK or a global package.
if ! command -v cmake >/dev/null 2>&1; then
  CMAKE_VERSION=4.4.5
  CMAKE_HOME="$CACHE/macos-tools/cmake-$CMAKE_VERSION"
  CMAKE_BIN="$CMAKE_HOME/CMake.app/Contents/bin"
  if [[ ! -x "$CMAKE_BIN/cmake" ]]; then
    printf 'CMake is required to build the Metal worker. Download official CMake %s into %s?\n' "$CMAKE_VERSION" "$CMAKE_HOME"
    if [[ "$ASSUME_YES" != true ]]; then
      [[ -t 0 ]] || fail 'CMake download approval requires Terminal or explicit --yes.'
      read -r -p 'Download repository-local CMake? [y/N] ' answer
      [[ "$answer" == y || "$answer" == Y || "$answer" == yes ]] || fail 'CMake download cancelled.'
    fi
    mkdir -p "$CMAKE_HOME"
    CMAKE_ARCHIVE="$CACHE/macos-tools/cmake-$CMAKE_VERSION.tar.gz.partial"
    curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --retry 2 "https://github.com/Kitware/CMake/releases/download/v$CMAKE_VERSION/cmake-$CMAKE_VERSION-macos-universal.tar.gz" -o "$CMAKE_ARCHIVE"
    CMAKE_SHA=679e77a96b9ef9484cc6c1445255fc4f84206b48a65420efbedf16bdab55fbff
    [[ "$(shasum -a 256 "$CMAKE_ARCHIVE" | awk '{print $1}')" == "$CMAKE_SHA" ]] || fail 'CMake SHA-256 mismatch; archive was not extracted.'
    tar -xzf "$CMAKE_ARCHIVE" --strip-components=1 -C "$CMAKE_HOME"
    [[ -x "$CMAKE_BIN/cmake" ]] || fail 'Official CMake archive did not contain the expected application.'
  fi
  export PATH="$CMAKE_BIN:$PATH"
fi
# Checking a global rustup proxy must not trigger an unapproved global install.
export RUSTUP_AUTO_INSTALL=0

has_pinned_rust() {
  local actual
  actual="$(rustc --version 2>/dev/null)" || return 1
  [[ "$actual" == "rustc $VERSION "* ]] || return 1
  # A global Intel Rust installation may still be on PATH after native restart.
  rustc -vV 2>/dev/null | grep -qx 'host: aarch64-apple-darwin' || return 1
  cargo --version >/dev/null 2>&1
}

if ! has_pinned_rust; then
  # Keep macOS bootstrap inputs separate from any Windows tool cache.
  export CARGO_HOME="$CACHE/macos-rust/cargo"
  export RUSTUP_HOME="$CACHE/macos-rust/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
  if ! has_pinned_rust; then
    printf 'Rust %s is missing. Download it from static.rust-lang.org into:\n%s\n' "$VERSION" "$CACHE/macos-rust"
    printf 'No sudo, shell-profile changes or global Rust installation. Cargo will also fetch locked build dependencies.\n'
    if [[ "$ASSUME_YES" != true ]]; then
      [[ -t 0 ]] || fail 'Download approval requires an interactive Terminal, or explicit --yes.'
      read -r -p 'Download the local Rust toolchain? [y/N] ' answer
      [[ "$answer" == y || "$answer" == Y || "$answer" == yes ]] || fail 'Download cancelled; no toolchain installed.'
    fi
    if [[ ! -x "$CARGO_HOME/bin/rustup" ]]; then
      BOOTSTRAP="$CACHE/macos-rust/bootstrap"
      mkdir -p "$BOOTSTRAP"
      URL=https://static.rust-lang.org/rustup/dist/aarch64-apple-darwin/rustup-init
      # Download to partial files, verify the official SHA-256 before execution.
      curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --retry 2 "$URL" -o "$BOOTSTRAP/rustup-init.partial"
      curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 --retry 2 "$URL.sha256" -o "$BOOTSTRAP/rustup-init.sha256"
      EXPECTED="$(awk 'NR == 1 { print $1 }' "$BOOTSTRAP/rustup-init.sha256")"
      [[ "$EXPECTED" =~ ^[[:xdigit:]]{64}$ ]] || fail 'Invalid rustup checksum response.'
      ACTUAL="$(shasum -a 256 "$BOOTSTRAP/rustup-init.partial" | awk '{print $1}')"
      [[ "$ACTUAL" == "$EXPECTED" ]] || fail 'Rustup checksum mismatch; installer was NOT executed.'
      mv "$BOOTSTRAP/rustup-init.partial" "$BOOTSTRAP/rustup-init"
      chmod +x "$BOOTSTRAP/rustup-init"
      "$BOOTSTRAP/rustup-init" -y --no-modify-path --default-toolchain none
    fi
    "$CARGO_HOME/bin/rustup" toolchain install "$VERSION" --profile minimal --component rustfmt --component clippy
    has_pinned_rust || fail 'The pinned local Rust toolchain is not usable.'
  fi
fi

printf 'Building VoicePaste and its Metal worker with Rust %s. Locked source dependencies are fetched on demand; no model weights are downloaded.\n' "$VERSION"
cargo xtask macos-preview --release
printf '\nBuilt: %s/build/release/VoicePaste.app\nModel downloads and Accessibility access are opt-in from its menu.\n' "$ROOT"
