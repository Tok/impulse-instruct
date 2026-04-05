#!/usr/bin/env bash
# ─── start.sh ─────────────────────────────────────────────────────────────────
# Launch Impulse Instruct.
# Usage:
#   ./start.sh               # run with mock LLM, no API
#   ./start.sh --api         # enable HTTP/MCP API on port 8765
#   ./start.sh --api --port 9000
#   ./start.sh --model models/my-model.gguf
#   ./start.sh --dev         # debug build + verbose logging
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")"
printf "\n  \033[38;2;110;110;110m▁▂▄▅▇██▇▅▄▂▁▁▂▄▅▇█\033[38;2;160;160;160m I M P U L S E • I N S T R U C T \033[38;2;110;110;110m█▇▅▄▂▁▁▂▄▅▇██▇▅▄▂▁\033[0m\n\n"

BUILD_MODE="release"
CARGO_FEATURES=""
LOG_LEVEL="info"
EXTRA_ARGS=()

for arg in "$@"; do
  case "$arg" in
    --dev)
      BUILD_MODE="debug"
      LOG_LEVEL="debug"
      ;;
    --llm)
      CARGO_FEATURES="--features llm"
      ;;
    *)
      EXTRA_ARGS+=("$arg")
      ;;
  esac
done

# Build if binary is missing or source is newer
BINARY="target/${BUILD_MODE}/impulse-instruct"
if [[ "$BUILD_MODE" == "release" ]]; then
  CARGO_BUILD_FLAG="--release"
else
  CARGO_BUILD_FLAG=""
fi

echo "→ Building (${BUILD_MODE})…"
cargo build $CARGO_BUILD_FLAG $CARGO_FEATURES 2>&1

echo "→ Starting Impulse Instruct…"
RUST_LOG="${LOG_LEVEL}" exec "./${BINARY}" --log "${LOG_LEVEL}" "${EXTRA_ARGS[@]}"
