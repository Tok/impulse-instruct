#!/usr/bin/env bash
# ─── scripts/run-llm-style.sh ────────────────────────────────────────────────
# Run the artist/genre reference test suite (llm_suite_style) against models.
#
# Usage:
#   ./scripts/run-llm-style.sh                        # all models in models/
#   ./scripts/run-llm-style.sh models/gemma-4-E4B-it-Q4_K_M.gguf  # single model
#   ./scripts/run-llm-style.sh --verbose               # full JSON output
# ─────────────────────────────────────────────────────────────────────────────
LLM_SUITE=llm_suite_style exec "$(dirname "$0")/run-llm-tests.sh" "$@"
