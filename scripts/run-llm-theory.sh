#!/usr/bin/env bash
# ─── scripts/run-llm-theory.sh ───────────────────────────────────────────────
# Run the music theory + producer lingo test suite (llm_suite_theory) against models.
#
# Tests cover: four-on-the-floor, backbeat, half-time, one-drop, offbeat hihats,
# D minor triad, A minor pentatonic, natural minor scale, Reese bass, sub bass,
# chord stabs, slow attack pad.
#
# Usage:
#   ./scripts/run-llm-theory.sh                        # all models in models/
#   ./scripts/run-llm-theory.sh models/gemma-4-E4B-it-Q4_K_M.gguf  # single model
#   ./scripts/run-llm-theory.sh --verbose               # full JSON output
# ─────────────────────────────────────────────────────────────────────────────
LLM_SUITE=llm_suite_theory exec "$(dirname "$0")/run-llm-tests.sh" "$@"
