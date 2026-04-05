#!/usr/bin/env bash
# ─── build-bonsai-server.sh ───────────────────────────────────────────────────
# Clone and build PrismML's llama.cpp fork, which supports the Q1_0_g128
# quantisation used by Bonsai-8B (ggml type 41 — not in upstream llama.cpp).
#
# Output: .llama-build/bin/llama-server
#
# Usage:
#   ./build-bonsai-server.sh
#
# Requires: git cmake build-essential (cmake usually already present)
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")/.."

SRC_DIR=".llama-src-prismml"
BUILD_DIR=".llama-build"
TARGET="$BUILD_DIR/bin/llama-server"

if [[ -f "$TARGET" ]]; then
  echo "✓ llama-server already built: $TARGET"
  exit 0
fi

echo "══════════════════════════════════════════"
echo "  Building PrismML llama-server…"
echo "══════════════════════════════════════════"
echo "(one-time build — takes ~3 min)"
echo ""

if [[ ! -d "$SRC_DIR" ]]; then
  git clone --depth 1 https://github.com/PrismML-Eng/llama.cpp "$SRC_DIR"
else
  echo "Source already cloned, skipping clone."
fi

cmake -B "$BUILD_DIR" -S "$SRC_DIR" \
  -DCMAKE_BUILD_TYPE=Release \
  -DGGML_CUDA=ON \
  -DLLAMA_CURL=OFF \
  -DBUILD_SHARED_LIBS=OFF \
  -DLLAMA_BUILD_TESTS=OFF \
  -DLLAMA_BUILD_EXAMPLES=OFF \
  2>&1 | tail -5

cmake --build "$BUILD_DIR" --target llama-server -j"$(nproc)" 2>&1 | tail -5

echo ""
echo "✓ Done: $TARGET"
echo ""
echo "Next: ./download-models.sh"
echo "Then: cargo run --release"
