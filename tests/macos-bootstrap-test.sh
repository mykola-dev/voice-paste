#!/bin/bash
# Exercise toolchain selection without installing Rust or requiring macOS.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/project/build/cache/macos-rust/cargo/bin" "$TMP/bin"
cp "$ROOT/build-macos-preview.sh" "$ROOT/rust-toolchain.toml" "$TMP/project/"
cat > "$TMP/bin/uname" <<'EOF'
#!/bin/bash
[[ "$1" == -s ]] && echo Darwin || echo arm64
EOF
cat > "$TMP/bin/xcode-select" <<'EOF'
#!/bin/bash
exit 0
EOF
cat > "$TMP/bin/xcrun" <<'EOF'
#!/bin/bash
exit 0
EOF
cat > "$TMP/bin/cmake" <<'EOF'
#!/bin/bash
exit 0
EOF
cat > "$TMP/bin/rustc" <<'EOF'
#!/bin/bash
exit 1
EOF
cp "$TMP/bin/rustc" "$TMP/bin/cargo"
cat > "$TMP/proxy" <<'EOF'
#!/bin/bash
[[ "$RUSTUP_TOOLCHAIN" == "$EXPECTED_TOOLCHAIN" ]] || { echo 'wrong toolchain' >&2; exit 1; }
case "${0##*/}:$1" in
  rustc:--version) echo "rustc ${EXPECTED_TOOLCHAIN%%-aarch64*} (test)" ;;
  rustc:-vV) printf 'rustc test\nhost: aarch64-apple-darwin\n' ;;
  cargo:--version) echo 'cargo test' ;;
  cargo:xtask) echo 'mock build complete' ;;
  *) exit 1 ;;
esac
EOF
cat > "$TMP/project/build/cache/macos-rust/cargo/bin/rustup" <<'EOF'
#!/bin/bash
[[ "$RUSTUP_TOOLCHAIN" == "$EXPECTED_TOOLCHAIN" ]] || { echo 'wrong toolchain' >&2; exit 1; }
if [[ "$1" == toolchain && "$2" == install && "$3" == "$EXPECTED_TOOLCHAIN" ]]; then
  cp "$TEST_PROXY" "$CARGO_HOME/bin/rustc"
  cp "$TEST_PROXY" "$CARGO_HOME/bin/cargo"
elif [[ "$1" == run && "$2" == "$EXPECTED_TOOLCHAIN" ]]; then
  if [[ "${TEST_FAIL:-}" == 1 ]]; then echo 'simulated compiler failure' >&2; exit 1; fi
  shift 2
  "$CARGO_HOME/bin/$@"
else
  echo 'unexpected rustup invocation' >&2; exit 1
fi
EOF
chmod +x "$TMP/bin/"* "$TMP/proxy" "$TMP/project/build/cache/macos-rust/cargo/bin/rustup"
export PATH="$TMP/bin:$PATH" LIBCLANG_PATH="$TMP" TEST_PROXY="$TMP/proxy"
export EXPECTED_TOOLCHAIN="$(sed -n 's/^channel = "\([^"]*\)".*/\1/p' "$ROOT/rust-toolchain.toml")-aarch64-apple-darwin"
export RUSTUP_TOOLCHAIN=hostile-inherited-override
bash "$TMP/project/build-macos-preview.sh" --yes > "$TMP/success" 2>&1
grep -q 'mock build complete' "$TMP/success"
# Force reinstallation while keeping the owned rustup executable.
rm "$TMP/project/build/cache/macos-rust/cargo/bin/rustc" "$TMP/project/build/cache/macos-rust/cargo/bin/cargo"
if TEST_FAIL=1 bash "$TMP/project/build-macos-preview.sh" --yes > "$TMP/failure" 2>&1; then
  echo 'expected a compiler failure' >&2; exit 1
fi
grep -q 'simulated compiler failure' "$TMP/failure"
grep -q 'local Rust compiler could not start' "$TMP/failure"
echo 'macOS bootstrap tests passed'
