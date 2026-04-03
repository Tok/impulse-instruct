#!/usr/bin/env bash
# ─── download-models.sh ───────────────────────────────────────────────────────
# Download the Bonsai 8B GGUF model for Impulse Instruct.
#
# Model: prism-ml/Bonsai-8B-gguf (Apache 2.0 license)
# Quantizations available (choose one):
#   Q4_K_M  — recommended, ~5.0 GB, good quality/speed balance
#   Q3_K_M  — smaller, ~4.0 GB, slightly lower quality
#   Q5_K_M  — larger, ~6.0 GB, higher quality
#   Q2_K    — smallest, ~2.8 GB, lower quality
#   Q8_0    — near-lossless, ~8.6 GB, slowest
#
# Usage:
#   ./download-models.sh                  # downloads Q4_K_M (recommended)
#   ./download-models.sh Q3_K_M           # specific quantization
#   ./download-models.sh --list           # list available files
#
# NOTE: A free HuggingFace account is required.
#   Sign up at https://huggingface.co/join
#   Then log in: huggingface-cli login
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")"

HF_REPO="prism-ml/Bonsai-8B-gguf"
MODEL_DIR="models"
QUANT="${1:-Q4_K_M}"

mkdir -p "$MODEL_DIR"

# Derive filename (HuggingFace repos vary — adjust if needed)
# Convention: Bonsai-8B-{QUANT}.gguf
MODEL_FILE="Bonsai-8B-${QUANT}.gguf"
OUTPUT_PATH="${MODEL_DIR}/${MODEL_FILE}"
SYMLINK="${MODEL_DIR}/bonsai-8b-q4.gguf"  # default path the app looks for

if [[ "$QUANT" == "--list" ]]; then
  echo "Available quantizations:"
  echo "  Q2_K    ~2.8 GB  (smallest)"
  echo "  Q3_K_M  ~4.0 GB"
  echo "  Q4_K_M  ~5.0 GB  (recommended)"
  echo "  Q5_K_M  ~6.0 GB"
  echo "  Q8_0    ~8.6 GB  (near-lossless)"
  exit 0
fi

if [[ -f "$OUTPUT_PATH" ]]; then
  echo "✓ Model already present: ${OUTPUT_PATH}"
  echo "  Delete it to re-download."
else
  echo "Downloading ${HF_REPO} → ${OUTPUT_PATH}"
  echo "Quantization: ${QUANT}"
  echo ""

  # Try huggingface-cli first (preferred — handles auth + resumable downloads)
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
    huggingface-cli download "$HF_REPO" \
      --include "*${QUANT}*.gguf" \
      --local-dir "$MODEL_DIR" \
      --local-dir-use-symlinks False
    # Find the downloaded file and rename if needed
    FOUND=$(find "$MODEL_DIR" -name "*${QUANT}*.gguf" | head -1)
    if [[ -n "$FOUND" && "$FOUND" != "$OUTPUT_PATH" ]]; then
      mv "$FOUND" "$OUTPUT_PATH"
    fi

  # Fallback: pip install huggingface_hub then retry
  elif command -v pip3 &>/dev/null || command -v pip &>/dev/null; then
    echo "huggingface-cli not found — installing huggingface_hub…"
    pip install -q huggingface_hub 2>/dev/null || pip3 install -q huggingface_hub
    huggingface-cli download "$HF_REPO" \
      --include "*${QUANT}*.gguf" \
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

# Create/update the default symlink the app uses
if [[ -f "$OUTPUT_PATH" ]]; then
  if [[ "$OUTPUT_PATH" != "$SYMLINK" ]]; then
    ln -sf "$(basename "$OUTPUT_PATH")" "$SYMLINK"
    echo "→ Default symlink: ${SYMLINK} → ${MODEL_FILE}"
  fi
  SIZE=$(du -sh "$OUTPUT_PATH" | cut -f1)
  echo ""
  echo "✓ Model ready: ${OUTPUT_PATH} (${SIZE})"
  echo ""
  echo "Run with real LLM inference:"
  echo "  ./start.sh --llm"
  echo "  # or: cargo run --features llm --release"
else
  echo "ERROR: Download failed. File not found at ${OUTPUT_PATH}"
  exit 1
fi

echo ""
echo "─── License notice ────────────────────────────────────────────────────────"
echo "Bonsai 8B is released under the Apache License 2.0 by prism-ml."
echo "See: https://huggingface.co/${HF_REPO}"
echo "────────────────────────────────────────────────────────────────────────────"
