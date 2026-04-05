#!/usr/bin/env bash
# ─── run-llm-tests.sh ─────────────────────────────────────────────────────────
# Run the LLM integration suite (src/llm_suite.rs) against one or all models.
#
# By default loops through every *.gguf in models/ — pass a specific model path
# to test only that one.
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
#   ./scripts/build-bonsai-server.sh   (builds .llama-build/bin/llama-server, PrismML fork)
#   ./scripts/build-llama-server.sh    (builds .llama-official-build/bin/llama-server, standard)
#   ./scripts/download-models.sh       (downloads models/Bonsai-8B.gguf)
#
# Usage:
#   ./scripts/run-llm-tests.sh                                    # all models, all suites
#   ./scripts/run-llm-tests.sh models/Bonsai-8B.gguf              # single model
#   ./scripts/run-llm-tests.sh models/Bonsai-8B.gguf acid         # single model + filter
#   ./scripts/run-llm-tests.sh "" darker                          # all models + filter
#   ./scripts/run-llm-tests.sh --verbose                          # full JSON, no truncation
#   ./scripts/run-llm-style.sh                                    # style suite only
#   ./scripts/run-llm-theory.sh                                   # theory suite only
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")/.."

# ── Suite selection ───────────────────────────────────────────────────────────
# LLM_SUITE env selects which test module to run (prefix-matched by cargo test):
#   llm_suite          → all three suites (llm_suite + llm_suite_style + llm_suite_theory)
#   llm_suite_style    → artist/genre reference tests only
#   llm_suite_theory   → music theory + producer lingo tests only
SUITE="${LLM_SUITE:-llm_suite}"

# ── Args ──────────────────────────────────────────────────────────────────────
MODEL_ARG="${1:-}"   # specific model path, "" = all, or a flag
FILTER="${2:-}"      # cargo test name filter

for arg in "$@"; do
  if [[ "$arg" == "--verbose" ]]; then
    export LLM_SUITE_VERBOSE=1
    echo "→ Verbose mode: full JSON output (no truncation)"
  fi
done

# ── Resolve model list ────────────────────────────────────────────────────────
if [[ -z "$MODEL_ARG" || "$MODEL_ARG" == --* ]]; then
  # No model specified — scan models/ directory
  mapfile -t MODELS < <(ls models/*.gguf 2>/dev/null | sort)
  if [[ ${#MODELS[@]} -eq 0 ]]; then
    echo "ERROR: no *.gguf files found in models/ — run ./download-models.sh"
    exit 1
  fi
else
  MODELS=("$MODEL_ARG")
fi

# ── Helper: select server binary for a given model path ──────────────────────
pick_server_bin() {
  local model_lower="${1,,}"
  if [[ "$model_lower" == *"bonsai"* ]]; then
    echo ".llama-build/bin/llama-server"
  elif [[ -f ".llama-official-build/bin/llama-server" ]]; then
    echo ".llama-official-build/bin/llama-server"
  elif command -v llama-server &>/dev/null; then
    echo "llama-server"
  else
    echo ""
  fi
}

server_label() {
  local bin="$1"
  case "$bin" in
    *.llama-build*)          echo "PrismML fork (Q1_0_g128 / Bonsai)" ;;
    *.llama-official-build*) echo "official llama.cpp" ;;
    *)                       echo "\$PATH llama-server" ;;
  esac
}

echo "══════════════════════════════════════════"
echo "  LLM integration suite  [suite: ${SUITE}]"
echo "  models to test: ${#MODELS[@]}"
echo "══════════════════════════════════════════"
printf "  %-40s  %-8s  %s\n" "Model" "Size" "Server"
echo "  ──────────────────────────────────────────────────────────────────"
for m in "${MODELS[@]}"; do
  name="$(basename "$m")"
  if [[ -f "$m" ]]; then
    size_bytes=$(stat -c%s "$m" 2>/dev/null || stat -f%z "$m" 2>/dev/null || echo 0)
    size_gb=$(awk "BEGIN {printf \"%.1fG\", $size_bytes/1073741824}")
  else
    size_gb="missing"
  fi
  sbin="$(pick_server_bin "$m")"
  slabel="$(server_label "$sbin")"
  printf "  %-40s  %-8s  %s\n" "$name" "$size_gb" "$slabel"
done
echo ""

# ── Compile once up front ─────────────────────────────────────────────────────
echo "→ Compiling test binary…"
cargo build --features llm-tests --tests -q
echo ""

# ── Per-model results accumulator ─────────────────────────────────────────────
declare -a SUMMARY_LINES=()
OVERALL_EXIT=0

# ── Main loop ─────────────────────────────────────────────────────────────────
for MODEL in "${MODELS[@]}"; do
  MODEL_NAME="$(basename "$MODEL")"
  SERVER_BIN="$(pick_server_bin "$MODEL")"

  echo "══════════════════════════════════════════"
  echo "  Model: $MODEL_NAME"
  echo "  Server: $(server_label "$SERVER_BIN")"
  echo "══════════════════════════════════════════"

  if [[ -z "$SERVER_BIN" ]]; then
    echo "ERROR: no llama-server binary found"
    echo "  For Bonsai models:  ./build-bonsai-server.sh"
    echo "  For other models:   ./build-llama-server.sh"
    SUMMARY_LINES+=("  SKIP   $MODEL_NAME  (no server binary)")
    continue
  fi
  if [[ ! -f "$SERVER_BIN" && "$SERVER_BIN" != "llama-server" ]]; then
    echo "ERROR: $SERVER_BIN not found"
    SUMMARY_LINES+=("  SKIP   $MODEL_NAME  (server binary missing)")
    continue
  fi
  if [[ ! -f "$MODEL" ]]; then
    echo "SKIP: $MODEL not found"
    SUMMARY_LINES+=("  SKIP   $MODEL_NAME  (model file missing)")
    continue
  fi

  # Kill stale servers for this model
  STALE=$(pgrep -f "llama-server.*$(basename "$MODEL")" 2>/dev/null || true)
  if [[ -n "$STALE" ]]; then
    echo "→ Killing stale llama-server: $STALE"
    kill $STALE 2>/dev/null || true
    sleep 1
  fi

  # Pick a free port
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
  trap "kill $SERVER_PID 2>/dev/null || true; unset SERVER_PID" EXIT

  # Wait for server ready (up to 120s, check body for "ok")
  echo -n "→ Waiting for server to be ready (model loading)"
  READY=0
  for i in $(seq 1 240); do
    HEALTH_BODY="$(curl -sf "${LLAMA_SERVER_URL}/health" 2>/dev/null || true)"
    if echo "$HEALTH_BODY" | grep -q '"ok"'; then
      echo " ready (${i}500ms)"
      READY=1
      break
    fi
    echo -n "."
    sleep 0.5
  done
  if [[ $READY -eq 0 ]]; then
    echo ""
    echo "ERROR: server did not become ready within 120s"
    tail -20 "$SERVER_LOG"
    kill $SERVER_PID 2>/dev/null || true
    SUMMARY_LINES+=("  SKIP   $MODEL_NAME  (server did not become ready)")
    continue
  fi

  # Show GPU lines
  echo "→ GPU:"
  grep -i "cuda\|offload\|model buffer" "$SERVER_LOG" 2>/dev/null | head -5 \
    || echo "   (no GPU lines)"
  echo ""

  # Run test suite (unit tests already passed at compile time)
  echo "→ Running ${SUITE} tests…"
  echo ""
  if cargo test --features llm-tests -- "$SUITE" --nocapture --test-threads 1 \
       ${FILTER:+$FILTER} 2>&1; then
    SUMMARY_LINES+=("  ✓ PASS  $MODEL_NAME")
  else
    SUMMARY_LINES+=("  ✗ FAIL  $MODEL_NAME")
    OVERALL_EXIT=1
  fi

  # Stop this model's server before loading the next
  echo ""
  echo "→ Stopping server (pid $SERVER_PID)"
  kill $SERVER_PID 2>/dev/null || true
  wait $SERVER_PID 2>/dev/null || true
  unset SERVER_PID
  trap - EXIT
  echo ""
done

# ── Final summary ─────────────────────────────────────────────────────────────
echo "══════════════════════════════════════════"
echo "  Results summary"
echo "══════════════════════════════════════════"
for line in "${SUMMARY_LINES[@]}"; do
  echo "$line"
done
echo ""

exit $OVERALL_EXIT
