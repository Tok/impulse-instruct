#!/usr/bin/env bash
# ─── run-llm-tests.sh ─────────────────────────────────────────────────────────
# Run the real Bonsai integration suite (src/llm_suite.rs).
#
# Each test fires a prompt 10 times and passes if ≥7 (directional) or ≥9
# (clearing/schema) responses satisfy the assertion.  A failing test means
# the model needs better tuning or the system prompt needs adjustment.
#
# Prerequisites:
#   1. libclang-dev cmake        (for llama-cpp-2)
#        sudo apt install libclang-dev cmake
#   2. Model file
#        ./download-models.sh
#
# If the model file is missing all tests skip cleanly — no failures.
#
# Usage:
#   ./run-llm-tests.sh              # run full suite
#   ./run-llm-tests.sh -- darker    # run only tests matching "darker"
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")"

FILTER=""
if [[ $# -gt 0 && "$1" == "--" ]]; then
  shift
  FILTER="${*}"
fi

echo "══════════════════════════════════════════"
echo "  Bonsai LLM integration suite"
echo "══════════════════════════════════════════"

# llm-tests implies the llm feature (llama-cpp-2). Requires libclang-dev.
cargo test --features llm-tests -- --nocapture ${FILTER:+$FILTER} 2>&1
