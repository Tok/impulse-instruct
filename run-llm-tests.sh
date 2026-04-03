#!/usr/bin/env bash
# ─── run-llm-tests.sh ─────────────────────────────────────────────────────────
# Run the real Bonsai integration suite (src/llm_suite.rs).
#
# Starts a single shared llama-server, exports LLAMA_SERVER_URL so every test
# connects to it instead of spawning its own instance.  This avoids the VRAM
# exhaustion / 30-second timeout you get when each test binary races to load
# the model independently.
#
# Each test fires a prompt 10 times and passes if ≥7 (directional) or ≥9
# (clearing/schema) responses satisfy the assertion.  A failing test means
# the model needs better tuning or the system prompt needs adjustment.
#
# Prerequisites:
#   ./build-bonsai-server.sh   (builds .llama-build/bin/llama-server)
#   ./download-models.sh       (downloads models/Bonsai-8B.gguf)
#
# Usage:
#   ./run-llm-tests.sh                   # full suite
#   ./run-llm-tests.sh darker            # only tests matching "darker"
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")"

FILTER="${1:-}"

MODEL="models/Bonsai-8B.gguf"
SERVER_BIN=".llama-build/bin/llama-server"

echo "══════════════════════════════════════════"
echo "  Bonsai LLM integration suite"
echo "══════════════════════════════════════════"

# ── Preflight ─────────────────────────────────────────────────────────────────
if [[ ! -f "$SERVER_BIN" ]]; then
  echo "ERROR: $SERVER_BIN not found — run ./build-bonsai-server.sh"
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

SERVER_LOG="$(mktemp /tmp/bonsai-server-XXXXXX.log)"
echo "→ Starting Bonsai server on port ${PORT} (log: ${SERVER_LOG})…"
"$SERVER_BIN" \
  --model "$MODEL" \
  --host 127.0.0.1 \
  --port "$PORT" \
  --ctx-size 4096 \
  --n-gpu-layers 99 \
  2>"$SERVER_LOG" &
SERVER_PID=$!

# Ensure server is killed when the script exits (success or failure)
trap "echo '→ Stopping Bonsai server (pid $SERVER_PID)'; kill $SERVER_PID 2>/dev/null || true" EXIT

# ── Wait for server to become healthy ─────────────────────────────────────────
echo -n "→ Waiting for server"
for i in $(seq 1 60); do
  if curl -sf "${LLAMA_SERVER_URL}/health" >/dev/null 2>&1; then
    echo " ready (${i}00ms)"
    break
  fi
  echo -n "."
  sleep 0.1
  if [[ $i -eq 60 ]]; then
    echo ""
    echo "ERROR: server did not become ready within 6s"
    echo "Server log (last 20 lines):"
    tail -20 "$SERVER_LOG"
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
