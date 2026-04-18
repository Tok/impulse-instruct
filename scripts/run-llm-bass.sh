#!/usr/bin/env bash
# ─── scripts/run-llm-bass.sh ─────────────────────────────────────────────────
# Run the bass-voice directive suite (llm_suite_bass) against models.
#
# Verifies that the system prompt gets the agent to:
#   • populate every active bass voice (MULTI-VOICE RULE)
#   • emit accents and slides (GROOVE CHECKLIST)
#   • keep accent/slide indices as a subset of bass_steps (SUBSET RULE)
#   • populate drum kits on beat-driven styles (FULL-COVERAGE RULE)
#
# Usage:
#   ./scripts/run-llm-bass.sh                        # all models in models/
#   ./scripts/run-llm-bass.sh models/Bonsai-8B.gguf  # single model
#   ./scripts/run-llm-bass.sh --verbose              # full JSON output
# ─────────────────────────────────────────────────────────────────────────────
LLM_SUITE=llm_suite_bass exec "$(dirname "$0")/run-llm-tests.sh" "$@"
