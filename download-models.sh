#!/usr/bin/env bash
# ─── download-models.sh ───────────────────────────────────────────────────────
# Download GGUF models for Impulse Instruct.
#
# Usage:
#   ./download-models.sh              # Bonsai-8B (default, 1-bit Q1, ~1.1 GB)
#   ./download-models.sh qwen3        # Qwen3-8B Q4_K_M (~5 GB, ~5× better quality)
#
# NOTE: A free HuggingFace account is required.
#   Sign up at https://huggingface.co/join
#   Then log in: huggingface-cli login
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")"

MODEL_DIR="models"
mkdir -p "$MODEL_DIR"

# ── Model selection ───────────────────────────────────────────────────────────
MODEL="${1:-bonsai}"

case "$MODEL" in
  bonsai|"")
    HF_REPO="prism-ml/Bonsai-8B-gguf"
    MODEL_FILE="Bonsai-8B.gguf"
    MODEL_DESC="Bonsai-8B Q1_0_g128 (PrismML, ~1.1 GB) — default, fastest"
    ;;
  qwen3)
    HF_REPO="bartowski/Qwen_Qwen3-8B-GGUF"
    MODEL_FILE="Qwen_Qwen3-8B-Q4_K_M.gguf"
    MODEL_DESC="Qwen3-8B Q4_K_M (bartowski, ~5 GB) — ~5× better quality, supports /think"
    ;;
  *)
    echo "Unknown model: '$MODEL'"
    echo "Available options: bonsai (default), qwen3"
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

  # ── Ensure huggingface-cli is available ────────────────────────────────────
  if ! command -v huggingface-cli &>/dev/null; then
    if command -v pip3 &>/dev/null || command -v pip &>/dev/null; then
      echo "huggingface-cli not found — installing huggingface_hub…"
      pip install -q huggingface_hub 2>/dev/null || pip3 install -q huggingface_hub
    fi
  fi

  if command -v huggingface-cli &>/dev/null; then
    # Check login; prompt if not authenticated
    if ! huggingface-cli whoami &>/dev/null; then
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
      huggingface-cli login || { echo "Login cancelled. Re-run after logging in."; exit 1; }
    fi
    echo "Using huggingface-cli…"
    huggingface-cli download "$HF_REPO" "$MODEL_FILE" \
      --local-dir "$MODEL_DIR" \
      --local-dir-use-symlinks False

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
      echo "ERROR: No download tool found (need huggingface-cli, wget, or curl)."
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
  qwen3)
    echo "Qwen3-8B is released under the Qwen Research License by Alibaba Cloud."
    echo "Quantisation by bartowski. See: https://huggingface.co/${HF_REPO}"
    ;;
esac
echo "─────────────────────────────────────────────────────────────────────────────"
