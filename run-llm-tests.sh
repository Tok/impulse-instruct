#!/usr/bin/env bash
# ─── run-llm-tests.sh ─────────────────────────────────────────────────────────
# Run the real LLM integration suite (src/llm_suite.rs) against a model.
#
# Starts a single shared llama-server, exports LLAMA_SERVER_URL so every test
# connects to it instead of spawning its own instance.  This avoids the VRAM
# exhaustion / 30-second timeout you get when each test binary races to load
# the model independently.
#
# Server binary selection (mirrors what the app does on model switch):
#   Bonsai / *bonsai* → .llama-build/bin/llama-server          (PrismML fork, Q1_0_g128)
#   All other models  → .llama-official-build/bin/llama-server  (standard llama.cpp)
#   fallback          → llama-server from $PATH
#
# Each test fires a prompt 10 times and passes if ≥7 (directional) or ≥9
# (clearing/schema) responses satisfy the assertion.  A failing test means
# the model needs better tuning or the system prompt needs adjustment.
#
# Prerequisites:
#   ./build-bonsai-server.sh   (builds .llama-build/bin/llama-server, PrismML fork)
#   ./build-llama-server.sh    (builds .llama-official-build/bin/llama-server, standard)
#   ./download-models.sh       (downloads models/Bonsai-8B.gguf)
#
# Usage:
#   ./run-llm-tests.sh                              # Bonsai, full suite
#   ./run-llm-tests.sh models/Qwen3-8B.gguf         # different model
#   ./run-llm-tests.sh models/Bonsai-8B.gguf darker # model + test filter
#   ./run-llm-tests.sh "" darker                    # default model + filter
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")"

# ── Args: optional MODEL path, optional test FILTER ───────────────────────────
# First arg: model path (defaults to Bonsai). If empty string "", use default.
# Second arg: test name filter (passed through to cargo test).
MODEL="${1:-}"
FILTER="${2:-}"

if [[ -z "$MODEL" ]]; then
  MODEL="models/Bonsai-8B.gguf"
fi

# ── Select server binary based on model name (same logic as the app) ──────────
MODEL_LOWER="${MODEL,,}"
if [[ "$MODEL_LOWER" == *"bonsai"* ]]; then
  SERVER_BIN=".llama-build/bin/llama-server"
  SERVER_LABEL="PrismML fork (Q1_0_g128 / Bonsai)"
  BUILD_HINT="./build-bonsai-server.sh"
elif [[ -f ".llama-official-build/bin/llama-server" ]]; then
  SERVER_BIN=".llama-official-build/bin/llama-server"
  SERVER_LABEL="official llama.cpp"
  BUILD_HINT="./build-llama-server.sh"
elif command -v llama-server &>/dev/null; then
  SERVER_BIN="llama-server"
  SERVER_LABEL="\$PATH llama-server"
  BUILD_HINT=""
else
  echo "ERROR: no llama-server binary found"
  echo "  For Bonsai models:  ./build-bonsai-server.sh"
  echo "  For other models:   ./build-llama-server.sh"
  exit 1
fi

MODEL_NAME="$(basename "$MODEL")"
echo "══════════════════════════════════════════"
echo "  LLM integration suite"
echo "  model:  $MODEL_NAME"
echo "  server: $SERVER_LABEL"
echo "══════════════════════════════════════════"

# ── Preflight ─────────────────────────────────────────────────────────────────
if [[ ! -f "$SERVER_BIN" ]] && [[ "$SERVER_BIN" != "llama-server" ]]; then
  echo "ERROR: $SERVER_BIN not found — run $BUILD_HINT"
  exit 1
fi
if [[ ! -f "$MODEL" ]]; then
  echo "ERROR: $MODEL not found — run ./download-models.sh"
  exit 1
fi

# ── Kill any stale llama-server processes ─────────────────────────────────────
STALE=$(pgrep -f "llama-server.*$MODEL" 2>/dev/null || true)
if [[ -n "$STALE" ]]; then
  echo "→ Killing stale llama-server processes: $STALE"
  kill $STALE 2>/dev/null || true
  sleep 1
fi

# ── Pick a free port and start the server ─────────────────────────────────────
PORT=$(python3 -c "import socket; s=socket.socket(); s.bind(('',0)); p=s.getsockname()[1]; s.close(); print(p)")
export LLAMA_SERVER_URL="http://127.0.0.1:${PORT}"

SERVER_LOG="$(mktemp /tmp/llama-server-XXXXXX.log)"
echo "→ Starting server on port ${PORT} (log: ${SERVER_LOG})…"
"$SERVER_BIN" \
  --model "$MODEL" \
  --host 127.0.0.1 \
  --port "$PORT" \
  --ctx-size 16384 \
  --n-gpu-layers 99 \
  2>"$SERVER_LOG" &
SERVER_PID=$!

# Ensure server is killed when the script exits (success or failure)
trap "echo '→ Stopping server (pid $SERVER_PID)'; kill $SERVER_PID 2>/dev/null || true" EXIT

# ── Wait for server to become healthy ─────────────────────────────────────────
# We check that /health returns HTTP 200 AND body contains "ok" status.
# Some llama-server builds return 200 immediately (HTTP server up) but then
# return 503 on /completion while the model is still loading into VRAM.
# Wait up to 120s to handle large models on CPU or slow NVMe.
echo -n "→ Waiting for server to be ready (model loading)"
for i in $(seq 1 240); do
  HEALTH_BODY="$(curl -sf "${LLAMA_SERVER_URL}/health" 2>/dev/null || true)"
  if echo "$HEALTH_BODY" | grep -q '"ok"'; then
    echo " ready (${i}500ms)"
    break
  fi
  echo -n "."
  sleep 0.5
  if [[ $i -eq 240 ]]; then
    echo ""
    echo "ERROR: server did not become ready within 120s"
    echo "Last /health response: $HEALTH_BODY"
    echo "Server log (last 30 lines):"
    tail -30 "$SERVER_LOG"
    exit 1
  fi
done

# ── Run the tests ─────────────────────────────────────────────────────────────
# Show GPU/CUDA lines from startup so we can confirm layers were offloaded
echo "→ Server startup (GPU lines):"
grep -i "cuda\|gpu\|layer\|offload\|ggml_cuda" "$SERVER_LOG" 2>/dev/null | head -10 || echo "   (no GPU lines found — check ${SERVER_LOG} if inference seems slow)"
echo ""

echo "→ Running tests (LLAMA_SERVER_URL=${LLAMA_SERVER_URL})"
echo ""

# --test-threads 1: llama-server handles one request at a time; parallel threads
# cause 503/timeout flooding and all 10 runs fail.  Serial is slower (~5 min)
# but gives reliable pass/fail signals.
cargo test --features llm-tests -- --nocapture --test-threads 1 ${FILTER:+$FILTER}
