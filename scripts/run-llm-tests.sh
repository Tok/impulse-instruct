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
# Default model set: Gemma 4 E4B + Bonsai 8B (officially evaluated).
# Qwen and Llama variants are skipped by default — pass --all-models to test everything
# in models/. They may work; the system prompt just isn't tuned for them.
#
# Usage:
#   ./scripts/run-llm-tests.sh                                    # Gemma + Bonsai, all suites
#   ./scripts/run-llm-tests.sh --all-models                       # all *.gguf in models/
#   ./scripts/run-llm-tests.sh models/Bonsai-8B.gguf              # single model
#   ./scripts/run-llm-tests.sh models/Bonsai-8B.gguf acid         # single model + filter
#   ./scripts/run-llm-tests.sh --verbose                          # full JSON, no truncation
#   ./scripts/run-llm-style.sh                                    # style suite only
#   ./scripts/run-llm-theory.sh                                   # theory suite only
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")/.."

# ── Suite selection ───────────────────────────────────────────────────────────
# LLM_SUITE env selects which test module to run (prefix-matched by cargo test):
#   llm_suite          → all suites (core + style + theory + bass)
#   llm_suite_style    → artist/genre reference tests only
#   llm_suite_theory   → music theory + producer lingo tests only
#   llm_suite_bass     → bass directive tests (multi-voice, accent/slide,
#                        subset rule, full-coverage rule)
SUITE="${LLM_SUITE:-llm_suite}"

# ── Args ──────────────────────────────────────────────────────────────────────
MODEL_ARG="${1:-}"   # specific model path, "" = all, or a flag
FILTER="${2:-}"      # cargo test name filter

ALL_MODELS=false  # include Qwen and other optional models
for arg in "$@"; do
  case "$arg" in
    --verbose)    export LLM_SUITE_VERBOSE=1; echo "→ Verbose mode: full JSON output (no truncation)" ;;
    --all-models) ALL_MODELS=true ;;
  esac
done

# ── Resolve model list ────────────────────────────────────────────────────────
if [[ -z "$MODEL_ARG" || "$MODEL_ARG" == --* ]]; then
  # No model specified — scan models/ directory.
  # Default set: Gemma 4 + Bonsai (officially evaluated).
  # Qwen requires --all-models (works but adds ~2× runtime for marginal gain).
  # Any other *.gguf (including Llama variants) is included when --all-models is set;
  # they may work but the system prompt is not tuned for them.
  ALL_GGUF=()
  mapfile -t ALL_GGUF < <(ls models/*.gguf 2>/dev/null | sort)
  if [[ ${#ALL_GGUF[@]} -eq 0 ]]; then
    echo "ERROR: no *.gguf files found in models/ — run ./scripts/download-models.sh"
    exit 1
  fi
  MODELS=()
  for m in "${ALL_GGUF[@]}"; do
    lower="${m,,}"
    # NeuTTS Air is a TTS model — not a chat LLM, can't respond to the
    # suite prompts at all.  Skip unconditionally, not gated by
    # --all-models.  Same for any other *tts* GGUF a user might drop in.
    [[ "$lower" == *"neutts"* ]] && continue
    [[ "$lower" == *"-tts"*  ]] && continue
    # Skip non-default chat models unless --all-models
    if ! $ALL_MODELS; then
      [[ "$lower" == *"qwen"* ]]  && continue
      [[ "$lower" == *"llama"* ]] && continue
    fi
    MODELS+=("$m")
  done
  if [[ ${#MODELS[@]} -eq 0 ]]; then
    echo "ERROR: no default models found (gemma/bonsai). Use --all-models to include Qwen and others."
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
declare -a MODEL_LOG_FILES=()   # temp files capturing test output per model
declare -a MODEL_LOG_NAMES=()   # display names corresponding to log files
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
    echo "  For Bonsai models:  ./scripts/build-bonsai-server.sh"
    echo "  For other models:   ./scripts/build-llama-server.sh"
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
    --ctx-size 65536 \
    --n-gpu-layers 99 \
    --flash-attn on \
    --cache-type-k q8_0 \
    --cache-type-v q8_0 \
    --cache-reuse 256 \
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
  MODEL_TMP="$(mktemp /tmp/llm-results-XXXXXX.log)"
  MODEL_LOG_FILES+=("$MODEL_TMP")
  MODEL_LOG_NAMES+=("$MODEL_NAME")

  # Disable errexit so a failing test run doesn't abort the whole script.
  # We capture the exit code manually via PIPESTATUS after tee.
  set +e
  cargo test --features llm-tests -- "$SUITE" --nocapture --test-threads 1 \
       ${FILTER:+$FILTER} 2>&1 | tee "$MODEL_TMP"
  TEST_EXIT=${PIPESTATUS[0]}
  set -e

  if [[ $TEST_EXIT -eq 0 ]]; then
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

# ── Per-model comparison table + ASCII bar chart ──────────────────────────────
if [[ ${#MODEL_LOG_FILES[@]} -gt 0 ]]; then
  python3 - "${MODEL_LOG_FILES[@]}" "${MODEL_LOG_NAMES[@]}" \
    --count "${#MODEL_LOG_FILES[@]}" <<'PYEOF'
import sys, re

args = sys.argv[1:]
count_idx = args.index('--count')
n = int(args[count_idx + 1])
files = args[:n]
names = args[n:count_idx]

def shorten(name):
    name = re.sub(r'\.gguf$', '', name)
    name = re.sub(r'gemma-4-E4B-it-Q4_K_M', 'Gemma4-E4B', name)
    name = re.sub(r'[Bb]onsai-8B.*', 'Bonsai-8B', name)
    name = re.sub(r'Qwen_Qwen3-(\w+)-Q4_K_M', r'Qwen3-\1', name)
    return name[:20]

short = [shorten(n) for n in names]

# Parse test output lines:
#   "test module::test_name ...  ✓ 7/10 (need ≥7) ~850ms/req  ok"
# results[test_name] = (passes, total, passed, avg_ms)
all_results = []
for f in files:
    results = {}
    try:
        with open(f) as fh:
            for line in fh:
                m = re.search(r'test \w+::(\w+)', line)
                if not m:
                    continue
                test_name = m.group(1)
                sm = re.search(r'(\d+)/(\d+)', line)
                if not sm:
                    continue
                passes = int(sm.group(1))
                total  = int(sm.group(2))
                tail   = line.split('...', 1)[-1] if '...' in line else line
                passed = tail.strip().endswith('ok')
                tm     = re.search(r'~(\d+)ms/req', line)
                avg_ms = int(tm.group(1)) if tm else None
                results[test_name] = (passes, total, passed, avg_ms)
    except Exception:
        pass
    all_results.append(results)

# Ordered test list (preserves first-seen order across all models)
all_tests = []
seen = set()
for r in all_results:
    for t in r:
        if t not in seen:
            all_tests.append(t)
            seen.add(t)

if not all_tests:
    print("  (no per-test data parsed — check output format)")
    sys.exit(0)

NAME_W = max(len(t) for t in all_tests) + 2
COL_W  = max(max(len(s) for s in short), 10) + 6  # extra room for timing

sep = '  ' + '─' * (NAME_W + (COL_W + 2) * n)

print()
print('══════════════════════════════════════════════════════════════════')
print('  Model comparison')
print('══════════════════════════════════════════════════════════════════')
print()

# Header
hdr = '  ' + 'Test'.ljust(NAME_W)
for s in short:
    hdr += '  ' + s.ljust(COL_W)
print(hdr)
print(sep)

totals      = [0] * n
max_total   = [0] * n
timing_sums = [0] * n   # sum of avg_ms across tests that have timing
timing_cnt  = [0] * n   # count of tests with timing data

for test in all_tests:
    row = '  ' + test.ljust(NAME_W)
    for i, r in enumerate(all_results):
        if test in r:
            passes, total, ok, avg_ms = r[test]
            cell = f'{passes}/{total}'
            cell += ' ✓' if ok else ' ✗'
            if avg_ms is not None:
                cell += f' ~{avg_ms}ms'
                timing_sums[i] += avg_ms
                timing_cnt[i]  += 1
            totals[i]    += passes
            max_total[i] += total
        else:
            cell = '—'
        row += '  ' + cell.ljust(COL_W)
    print(row)

print(sep)

# Totals row
total_row = '  ' + 'TOTAL'.ljust(NAME_W)
for i in range(n):
    mx  = max_total[i] or 1
    pct = totals[i] / mx * 100
    cell = f'{totals[i]}/{max_total[i]} ({pct:.0f}%)'
    total_row += '  ' + cell.ljust(COL_W)
print(total_row)

# Avg response time row (only if any model has timing data)
if any(timing_cnt):
    time_row = '  ' + 'avg ms/req'.ljust(NAME_W)
    for i in range(n):
        if timing_cnt[i]:
            avg = timing_sums[i] // timing_cnt[i]
            cell = f'~{avg}ms'
        else:
            cell = '—'
        time_row += '  ' + cell.ljust(COL_W)
    print(time_row)

# ASCII bar chart — accuracy
BARS = 32
print()
print('──────────────────────────────────────────────────────────────────')
print('  Accuracy')
for i in range(n):
    mx     = max_total[i] or 1
    pct    = totals[i] / mx
    filled = round(pct * BARS)
    bar    = '█' * filled + '░' * (BARS - filled)
    label  = short[i].ljust(20)
    score  = f'{totals[i]}/{max_total[i]} ({pct*100:.0f}%)'
    print(f'  {label}  [{bar}]  {score}')

# ASCII bar chart — speed (lower is better, invert the bar)
if any(timing_cnt):
    all_avgs = [timing_sums[i] // timing_cnt[i] if timing_cnt[i] else None for i in range(n)]
    valid    = [a for a in all_avgs if a is not None]
    if valid:
        max_ms = max(valid)
        print()
        print('  Speed  (longer bar = faster)')
        for i in range(n):
            if all_avgs[i] is None:
                continue
            avg    = all_avgs[i]
            pct    = 1.0 - (avg / max_ms) if max_ms > 0 else 0.0
            filled = round(pct * BARS)
            # Clamp so even the slowest model shows at least a sliver
            filled = max(filled, round(BARS * 0.05))
            bar    = '█' * filled + '░' * (BARS - filled)
            label  = short[i].ljust(20)
            print(f'  {label}  [{bar}]  ~{avg}ms/req')
print()
PYEOF
fi

exit $OVERALL_EXIT
