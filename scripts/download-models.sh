#!/usr/bin/env bash
# ─── download-models.sh ───────────────────────────────────────────────────────
# Download GGUF models for Impulse Instruct.
#
# Usage:
#   ./download-models.sh              # Bonsai-8B (default, 1-bit Q1, ~1.1 GB)
#   ./download-models.sh qwen3        # Qwen3-8B Q4_K_M (~5 GB, ~5× better quality)
#   ./download-models.sh qwen3-14b    # Qwen3-14B Q4_K_M (~9 GB, best musical reasoning)
#   ./download-models.sh gemma4       # Gemma 4 4B Q4 (~3 GB, fast + strong JSON)
#   ./download-models.sh llama31      # Llama 3.1 8B Q4_K_M (~5 GB, excellent JSON)
#
# NOTE: A free HuggingFace account is required.
#   Sign up at https://huggingface.co/join
#   Then log in: huggingface-cli login
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")/.."

MODEL_DIR="models"
mkdir -p "$MODEL_DIR"

# ── Model selection ───────────────────────────────────────────────────────────
MODEL="${1:-bonsai}"

case "$MODEL" in
  bonsai|"")
    HF_REPO="prism-ml/Bonsai-8B-gguf"
    MODEL_FILE="Bonsai-8B.gguf"
    MODEL_DESC="Bonsai-8B Q1_0_g128 (PrismML, ~1.1 GB) — default, fastest but weakest"
    ;;
  qwen3)
    HF_REPO="bartowski/Qwen_Qwen3-8B-GGUF"
    MODEL_FILE="Qwen_Qwen3-8B-Q4_K_M.gguf"
    MODEL_DESC="Qwen3-8B Q4_K_M (bartowski, ~5 GB) — ~5× better than Bonsai, supports /think"
    ;;
  qwen3-14b)
    HF_REPO="bartowski/Qwen_Qwen3-14B-GGUF"
    MODEL_FILE="Qwen_Qwen3-14B-Q4_K_M.gguf"
    MODEL_DESC="Qwen3-14B Q4_K_M (bartowski, ~9 GB) — best musical reasoning, needs 12 GB VRAM"
    ;;
  gemma4)
    HF_REPO="unsloth/gemma-4-E4B-it-GGUF"
    MODEL_FILE="gemma-4-E4B-it-Q4_K_M.gguf"
    MODEL_DESC="Gemma 4 E4B Q4_K_M (unsloth, ~5 GB) — fast, strong structured output"
    ;;
  llama31)
    HF_REPO="bartowski/Meta-Llama-3.1-8B-Instruct-GGUF"
    MODEL_FILE="Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"
    MODEL_DESC="Llama 3.1 8B Q4_K_M (bartowski, ~5 GB) — excellent JSON compliance"
    ;;
  *)
    echo "Unknown model: '$MODEL'"
    echo "Available: bonsai (default), qwen3, qwen3-14b, gemma4, llama31"
    exit 1
    ;;
esac

OUTPUT_PATH="${MODEL_DIR}/${MODEL_FILE}"

echo "Model: ${MODEL_DESC}"
echo ""

if [[ -f "$OUTPUT_PATH" ]]; then
  echo "✓ Model already present: ${OUTPUT_PATH}"
  echo "  Delete it to re-download."
else
  echo "Downloading ${HF_REPO} → ${OUTPUT_PATH}"
  echo ""

  # ── Ensure hf / huggingface-cli is available ─────────────────────────────
  if ! command -v hf &>/dev/null && ! command -v huggingface-cli &>/dev/null; then
    if command -v pip3 &>/dev/null || command -v pip &>/dev/null; then
      echo "hf not found — installing huggingface_hub…"
      pip install -q huggingface_hub 2>/dev/null || pip3 install -q huggingface_hub
    fi
  fi

  # Prefer 'hf' (new CLI name); fall back to 'huggingface-cli' (old name)
  HF_CMD=""
  if command -v hf &>/dev/null; then
    HF_CMD="hf"
  elif command -v huggingface-cli &>/dev/null; then
    HF_CMD="huggingface-cli"
  fi

  if [[ -n "$HF_CMD" ]]; then
    # 'hf' uses 'hf auth whoami' / 'hf auth login'; legacy CLI uses flat subcommands
    if [[ "$HF_CMD" == "hf" ]]; then
      HF_WHOAMI="$HF_CMD auth whoami"
      HF_LOGIN="$HF_CMD auth login"
    else
      HF_WHOAMI="$HF_CMD whoami"
      HF_LOGIN="$HF_CMD login"
    fi

    # Check login; prompt if not authenticated
    if ! $HF_WHOAMI &>/dev/null; then
      echo ""
      echo "  ════════════════════════════════════════════════════"
      echo "   HuggingFace login required"
      echo "  ════════════════════════════════════════════════════"
      echo "   A free account is needed to download this model."
      echo ""
      echo "   1. Sign up (free): https://huggingface.co/join"
      echo "   2. Get your token: https://huggingface.co/settings/tokens"
      echo "      (Create a token with Read permissions)"
      echo "   3. Paste it below when prompted."
      echo "  ════════════════════════════════════════════════════"
      echo ""
      $HF_LOGIN || { echo "Login cancelled. Re-run after logging in."; exit 1; }
    fi
    echo "Using ${HF_CMD}…"
    $HF_CMD download "$HF_REPO" "$MODEL_FILE" \
      --local-dir "$MODEL_DIR"

  # Last resort: direct wget/curl from HuggingFace CDN
  else
    HF_URL="https://huggingface.co/${HF_REPO}/resolve/main/${MODEL_FILE}"
    echo "Falling back to direct download from:"
    echo "  ${HF_URL}"
    echo ""
    if command -v wget &>/dev/null; then
      wget --continue --progress=bar:force -O "$OUTPUT_PATH" "$HF_URL"
    elif command -v curl &>/dev/null; then
      curl -L --continue-at - -o "$OUTPUT_PATH" "$HF_URL"
    else
      echo "ERROR: No download tool found (need hf / huggingface-cli, wget, or curl)."
      exit 1
    fi
  fi
fi

if [[ -f "$OUTPUT_PATH" ]]; then
  SIZE=$(du -sh "$OUTPUT_PATH" | cut -f1)
  echo ""
  echo "✓ Model ready: ${OUTPUT_PATH} (${SIZE})"
  echo ""
  echo "Run with real LLM inference:"
  echo "  cargo run --release"
else
  echo "ERROR: Download failed. File not found at ${OUTPUT_PATH}"
  exit 1
fi

echo ""
echo "─── License notice ─────────────────────────────────────────────────────────"
case "$MODEL" in
  bonsai)
    echo "Bonsai 8B is released under the Apache License 2.0 by prism-ml."
    echo "See: https://huggingface.co/${HF_REPO}"
    ;;
  qwen3|qwen3-14b)
    echo "Qwen3 is released under the Qwen Research License by Alibaba Cloud."
    echo "Quantisation by bartowski. See: https://huggingface.co/${HF_REPO}"
    ;;
  gemma4)
    echo "Gemma 4 E4B is released under the Gemma Terms of Use by Google DeepMind."
    echo "Quantisation by unsloth. See: https://huggingface.co/${HF_REPO}"
    ;;
  llama31)
    echo "Llama 3.1 is released under the Meta Llama 3.1 Community License by Meta."
    echo "Quantisation by bartowski. See: https://huggingface.co/${HF_REPO}"
    ;;
esac
echo "─────────────────────────────────────────────────────────────────────────────"
