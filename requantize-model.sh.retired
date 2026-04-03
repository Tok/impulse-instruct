#!/usr/bin/env bash
# ─── requantize-model.sh ──────────────────────────────────────────────────────
# Re-quantize Bonsai-8B.gguf from NVFP4 (ggml type 41) → Q4_K_M.
#
# Why: the bundled llama.cpp in llama-cpp-rs 0.1.x only supports ggml types
# 0–40. Bonsai-8B.gguf uses NVFP4 (type 41), added in llama.cpp after the
# last submodule update in llama-cpp-rs. Re-quantizing with a fresh llama.cpp
# build produces a Q4_K_M file that the bundled version can load fine.
#
# Q4_K_M stats: ~4.8 bpw, very similar quality and file size to NVFP4.
#
# Usage:
#   ./requantize-model.sh
#
# Requires: git cmake build-essential (cmake is already needed for llama-cpp-rs)
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")"

INPUT="models/Bonsai-8B.gguf"
OUTPUT="models/bonsai-8b-q4km.gguf"
SYMLINK="models/bonsai-8b-q4.gguf"   # path the app looks for

BUILD_DIR=".llama-build"
SRC_DIR=".llama-src"

if [[ ! -f "$INPUT" ]]; then
  echo "ERROR: $INPUT not found. Run ./download-models.sh first."
  exit 1
fi

if [[ -f "$OUTPUT" ]]; then
  echo "✓ Re-quantized model already present: $OUTPUT"
else
  echo "══════════════════════════════════════════"
  echo "  Building llama-quantize from source…"
  echo "══════════════════════════════════════════"
  echo "(this is a one-time build — takes ~2 min)"
  echo ""

  if [[ ! -d "$SRC_DIR" ]]; then
    git clone --depth 1 https://github.com/ggml-org/llama.cpp "$SRC_DIR"
  else
    echo "Source already cloned, skipping clone."
  fi

  cmake -B "$BUILD_DIR" -S "$SRC_DIR" \
    -DCMAKE_BUILD_TYPE=Release \
    -DLLAMA_CURL=OFF \
    -DBUILD_SHARED_LIBS=OFF \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_SERVER=OFF \
    2>&1 | tail -5

  cmake --build "$BUILD_DIR" --target llama-quantize -j"$(nproc)" 2>&1 | tail -5

  echo ""
  echo "══════════════════════════════════════════"
  echo "  Re-quantizing to Q4_K_M…"
  echo "══════════════════════════════════════════"
  "$BUILD_DIR/bin/llama-quantize" "$INPUT" "$OUTPUT" Q4_K_M

  SIZE=$(du -sh "$OUTPUT" | cut -f1)
  echo ""
  echo "✓ Done: $OUTPUT ($SIZE)"
fi

# Update the symlink the app uses
ln -sf "$(basename "$OUTPUT")" "$SYMLINK"
echo "→ Symlink updated: $SYMLINK → $(basename "$OUTPUT")"
echo ""
echo "Run the app:"
echo "  cargo run --release    (or ./start.sh)"
