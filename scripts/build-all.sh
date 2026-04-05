#!/usr/bin/env bash
# ─── build-all.sh ─────────────────────────────────────────────────────────────
# Build release binaries for Linux and Windows.
#
# Linux build:   target/x86_64-unknown-linux-gnu/release/impulse-instruct
# Windows build: target/x86_64-pc-windows-msvc/release/impulse-instruct.exe
#
# Prerequisites (Windows cross-compile):
#   rustup update stable               (needs rustc 1.89+)
#   cargo install cargo-xwin
#   sudo apt install clang lld cmake ninja-build
# Note: Ubuntu installs LLVM tools as llvm-lib-18 etc. (versioned). This script
# creates unversioned symlinks in /usr/local/bin automatically when they're missing.
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")/.."

FEATURES="${FEATURES:-}"  # pass FEATURES=llm to include real LLM inference
FEATURE_FLAGS=""
[[ -n "$FEATURES" ]] && FEATURE_FLAGS="--features ${FEATURES}"

DIST="dist"
mkdir -p "$DIST"

# ── Linux ─────────────────────────────────────────────────────────────────────
echo "══════════════════════════════════════════"
echo "  Building Linux (x86_64)…"
echo "══════════════════════════════════════════"
cargo build --release --bin impulse-instruct $FEATURE_FLAGS

LINUX_BIN="target/release/impulse-instruct"
cp "$LINUX_BIN" "${DIST}/impulse-instruct-linux-x86_64"
strip "${DIST}/impulse-instruct-linux-x86_64"
cp scripts/dist-start.sh "${DIST}/start.sh"
chmod +x "${DIST}/start.sh"
echo "✓ Linux: ${DIST}/impulse-instruct-linux-x86_64 ($(du -sh "${DIST}/impulse-instruct-linux-x86_64" | cut -f1))"

# ── Windows ───────────────────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════"
echo "  Building Windows (x86_64-pc-windows-msvc)…"
echo "══════════════════════════════════════════"

WIN_TARGET="x86_64-pc-windows-msvc"
rustup target add "$WIN_TARGET" 2>/dev/null || true

if ! command -v cargo-xwin &>/dev/null; then
  # cargo-xwin latest requires rustc 1.89+; update if needed
  RUSTC_MINOR=$(rustc --version | grep -oP '1\.\K[0-9]+')
  if [[ "${RUSTC_MINOR:-0}" -lt 89 ]]; then
    echo "  rustc 1.89+ required for cargo-xwin. Updating toolchain…"
    rustup update stable
  fi
  echo "  Installing cargo-xwin…"
  cargo install cargo-xwin
fi

if ! command -v clang &>/dev/null; then
  echo "  WARNING: clang not found. Install with: sudo apt install clang lld cmake"
  echo "  Skipping Windows build."
else
  # cc-rs (used by ring and other C deps) looks for llvm-lib and llvm-dlltool without
  # a version suffix. Ubuntu installs them as llvm-lib-18 etc. Create unversioned
  # symlinks in /usr/local/bin if they're missing so cc-rs can find them.
  missing_symlinks=()
  for tool in llvm-lib llvm-dlltool llvm-ar; do
    if ! command -v "$tool" &>/dev/null; then
      versioned=$(ls /usr/bin/${tool}-* 2>/dev/null | sort -V | tail -1)
      if [[ -n "$versioned" ]]; then
        if sudo ln -sf "$versioned" "/usr/local/bin/$tool" 2>/dev/null; then
          echo "  Symlinking $versioned -> /usr/local/bin/$tool"
        else
          missing_symlinks+=("sudo ln -sf $versioned /usr/local/bin/$tool")
        fi
      fi
    fi
  done
  if [[ ${#missing_symlinks[@]} -gt 0 ]]; then
    echo ""
    echo "  ERROR: Windows build requires unversioned LLVM tool aliases."
    echo "  Ubuntu installs them as llvm-lib-18 etc. but cc-rs expects plain names."
    echo "  Run these once, then re-run build-all.sh:"
    echo ""
    for cmd in "${missing_symlinks[@]}"; do
      echo "    $cmd"
    done
    echo ""
    exit 1
  fi

  cargo xwin build --release --bin impulse-instruct --target "$WIN_TARGET" $FEATURE_FLAGS

  WIN_BIN="target/${WIN_TARGET}/release/impulse-instruct.exe"
  cp "$WIN_BIN" "${DIST}/impulse-instruct-windows-x86_64.exe"
  cp scripts/dist-start.bat "${DIST}/start.bat"
  echo "✓ Windows: ${DIST}/impulse-instruct-windows-x86_64.exe ($(du -sh "${DIST}/impulse-instruct-windows-x86_64.exe" | cut -f1))"
fi

echo ""
echo "══════════════════════════════════════════"
echo "  Build complete. Output in ./${DIST}/"
ls -lh "${DIST}/"
echo "══════════════════════════════════════════"
