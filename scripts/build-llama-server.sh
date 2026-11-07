#!/usr/bin/env bash
# ─── build-llama-server.sh ────────────────────────────────────────────────────
# Build the official upstream llama.cpp server for standard GGUF models.
#
# Use this for: Gemma 4, Qwen3, Qwen3-14B, and any other standard GGUF model.
#
# Output: .llama-official-build/bin/llama-server
#
# Usage:
#   ./build-llama-server.sh            # build if missing or broken
#   ./build-llama-server.sh --rebuild  # force a clean rebuild
#
# An existing binary whose shared libraries no longer resolve (e.g. after
# a CUDA toolkit upgrade: libcudart.so.12 → .so.13) is rebuilt
# automatically.  Rebuilds wipe the build dir because CMakeCache.txt pins
# the CUDA toolkit path it was first configured with.
#
# Requires: git cmake build-essential cuda-toolkit (12.x or 13.x)
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
printf "\n  \033[38;2;110;110;110m▁▂▄▅▇██▇▅▄▂▁▁▂▄▅▇█\033[38;2;160;160;160m I M P U L S E • I N S T R U C T \033[38;2;110;110;110m█▇▅▄▂▁▁▂▄▅▇██▇▅▄▂▁\033[0m\n\n"
cd "$(dirname "$0")/.."

SRC_DIR=".llama-src-official"
BUILD_DIR=".llama-official-build"
TARGET="$BUILD_DIR/bin/llama-server"

REBUILD=0
[[ "${1:-}" == "--rebuild" ]] && REBUILD=1

if [[ -f "$TARGET" && $REBUILD -eq 0 ]]; then
  missing=$(ldd "$TARGET" 2>/dev/null | awk '/not found/ {print $1}' || true)
  if [[ -z "$missing" ]]; then
    echo "✓ official llama-server already built: $TARGET"
    echo "  (pass --rebuild to force a clean rebuild)"
    exit 0
  fi
  echo "⚠ existing llama-server has unresolved libraries:"
  echo "$missing" | sed 's/^/    /'
  echo "  (CUDA toolkit upgraded?) — rebuilding."
  REBUILD=1
fi

if [[ $REBUILD -eq 1 && -d "$BUILD_DIR" ]]; then
  echo "Removing stale build dir $BUILD_DIR …"
  rm -rf "$BUILD_DIR"
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
echo "This server handles: Qwen3, Qwen3-14B, Gemma 4, and all standard GGUF models."
