#!/usr/bin/env bash
# ─── build-llama-server.sh ────────────────────────────────────────────────────
# Build the official upstream llama.cpp server for standard GGUF models.
#
# Use this for: Qwen3, Qwen3-14B, Gemma 4, and any other
# standard GGUF model that the PrismML fork doesn't support.
#
# Output: .llama-official-build/bin/llama-server
#
# Usage:
#   ./build-llama-server.sh
#
# Requires: git cmake build-essential cuda-toolkit-12-x
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")/.."

SRC_DIR=".llama-src-official"
BUILD_DIR=".llama-official-build"
TARGET="$BUILD_DIR/bin/llama-server"

if [[ -f "$TARGET" ]]; then
  echo "✓ official llama-server already built: $TARGET"
  exit 0
fi

echo "══════════════════════════════════════════"
echo "  Building official llama.cpp server…"
echo "══════════════════════════════════════════"
echo "(one-time build — takes ~3 min)"
echo ""

if [[ ! -d "$SRC_DIR" ]]; then
  git clone --depth 1 https://github.com/ggml-org/llama.cpp "$SRC_DIR"
else
  echo "Source already cloned."
fi

cmake -B "$BUILD_DIR" -S "$SRC_DIR" \
  -DCMAKE_BUILD_TYPE=Release \
  -DGGML_CUDA=ON \
  -DLLAMA_CURL=OFF \
  -DLLAMA_BUILD_TESTS=OFF \
  -DLLAMA_BUILD_EXAMPLES=OFF \
  -DLLAMA_BUILD_SERVER=ON

cmake --build "$BUILD_DIR" --target llama-server -j "$(nproc)"

echo ""
echo "✓ Built: $TARGET"
echo ""
echo "This server handles: Qwen3, Qwen3-14B, Gemma 4, and all"
echo "standard GGUF models. The PrismML server (.llama-build/) is still"
echo "required for Bonsai 8B (1-bit Q1_0_g128 format)."
echo ""
echo "Impulse Instruct picks the right binary automatically based on model name."
