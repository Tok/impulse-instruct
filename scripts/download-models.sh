#!/usr/bin/env bash
# ─── download-models.sh ───────────────────────────────────────────────────────
# Download GGUF models for Impulse Instruct.
#
# Usage:
#   ./scripts/download-models.sh                  # Gemma 4 E4B (default, ~4.6 GB, best overall)
#   ./scripts/download-models.sh deepseek-r1-7b   # DeepSeek-R1-Distill-Qwen-7B (~5 GB, CoT, fast)
#   ./scripts/download-models.sh deepseek-r1-14b  # DeepSeek-R1-Distill-Qwen-14B (~9 GB, CoT, accurate)
#   ./scripts/download-models.sh qwen3            # Qwen3-8B Q4_K_M (~5 GB, optional)
#   ./scripts/download-models.sh qwen3-14b        # Qwen3-14B Q4_K_M (~9 GB, optional)
#
# NOTE: A free HuggingFace account is required.
#   Sign up at https://huggingface.co/join
#   Then log in: huggingface-cli login
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
printf "\n  \033[38;2;110;110;110m▁▂▄▅▇██▇▅▄▂▁▁▂▄▅▇█\033[38;2;160;160;160m I M P U L S E • I N S T R U C T \033[38;2;110;110;110m█▇▅▄▂▁▁▂▄▅▇██▇▅▄▂▁\033[0m\n\n"
cd "$(dirname "$0")/.."

MODEL_DIR="models"
mkdir -p "$MODEL_DIR"

# ── Model selection ───────────────────────────────────────────────────────────
MODEL="${1:-gemma4}"

case "$MODEL" in
  gemma4|"")
    HF_REPO="unsloth/gemma-4-E4B-it-GGUF"
    MODEL_FILE="gemma-4-E4B-it-Q4_K_M.gguf"
    MODEL_DESC="Gemma 4 E4B Q4_K_M (unsloth, ~4.6 GB) — default, best accuracy + speed"
    ;;
  qwen3)
    HF_REPO="bartowski/Qwen_Qwen3-8B-GGUF"
    MODEL_FILE="Qwen_Qwen3-8B-Q4_K_M.gguf"
    MODEL_DESC="Qwen3-8B Q4_K_M (bartowski, ~5 GB) — optional, supports /think chain-of-thought"
    ;;
  qwen3-14b)
    HF_REPO="bartowski/Qwen_Qwen3-14B-GGUF"
    MODEL_FILE="Qwen_Qwen3-14B-Q4_K_M.gguf"
    MODEL_DESC="Qwen3-14B Q4_K_M (bartowski, ~9 GB) — optional large, needs 12 GB VRAM"
    ;;
  deepseek-r1-7b)
    HF_REPO="bartowski/DeepSeek-R1-Distill-Qwen-7B-GGUF"
    MODEL_FILE="DeepSeek-R1-Distill-Qwen-7B-Q4_K_M.gguf"
    MODEL_DESC="DeepSeek-R1-Distill-Qwen-7B Q4_K_M (~5 GB) — chain-of-thought, Qwen2.5 base; newer distills may exist"
    ;;
  deepseek-r1-14b)
    HF_REPO="bartowski/DeepSeek-R1-Distill-Qwen-14B-GGUF"
    MODEL_FILE="DeepSeek-R1-Distill-Qwen-14B-Q4_K_M.gguf"
    MODEL_DESC="DeepSeek-R1-Distill-Qwen-14B Q4_K_M (~9 GB) — CoT, needs ~12 GB VRAM; newer distills may exist"
    ;;
  neutts)
    HF_REPO="neuphonic/neutts-air-q4-gguf"
    MODEL_FILE="neutts-air-q4.gguf"
    HF_FILE="neutts-air-Q4_0.gguf"
    MODEL_DESC="NeuTTS Air Q4 GGUF (~527 MB) — neural TTS voice cloning"
    ;;
  *)
    echo "Unknown model: '$MODEL'"
    echo "Available: gemma4 (default), deepseek-r1-7b, deepseek-r1-14b, qwen3, qwen3-14b, neutts"
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
    DL_FILE="${HF_FILE:-$MODEL_FILE}"
    $HF_CMD download "$HF_REPO" "$DL_FILE" \
      --local-dir "$MODEL_DIR"
    # Rename if the HF filename differs from our expected name
    if [[ -n "${HF_FILE:-}" ]] && [[ "$HF_FILE" != "$MODEL_FILE" ]] && [[ -f "${MODEL_DIR}/${HF_FILE}" ]]; then
      mv "${MODEL_DIR}/${HF_FILE}" "$OUTPUT_PATH"
    fi

  # Last resort: direct wget/curl from HuggingFace CDN
  else
    DL_FILE="${HF_FILE:-$MODEL_FILE}"
    HF_URL="https://huggingface.co/${HF_REPO}/resolve/main/${DL_FILE}"
    echo "Falling back to direct download from:"
    echo "  ${HF_URL}"
    echo ""
    if command -v wget &>/dev/null; then
      wget --continue --progress=bar:force -O "$OUTPUT_PATH" "$HF_URL"
    elif command -v curl &>/dev/null; then
      curl -L --continue-at - -o "$OUTPUT_PATH" "$HF_URL"
    else
      echo ""
      echo "══════════════════════════════════════════════════════════════════════"
      echo "  MANUAL DOWNLOAD RECOMMENDED"
      echo "══════════════════════════════════════════════════════════════════════"
      echo ""
      echo "  No download tool was found (hf / huggingface-cli / wget / curl)."
      echo "  Rather than installing one, just download in your browser:"
      echo ""
      echo "  1. Open: ${HF_URL}"
      echo "  2. Sign in at https://huggingface.co/join if prompted."
      echo "  3. Click the download arrow next to '${DL_FILE}'."
      echo "  4. Move the file to:  $(pwd)/${OUTPUT_PATH}"
      echo ""
      echo "══════════════════════════════════════════════════════════════════════"
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
  gemma4)
    echo "Gemma 4 E4B is released under the Gemma Terms of Use by Google DeepMind."
    echo "Quantisation by unsloth. See: https://huggingface.co/${HF_REPO}"
    ;;
  qwen3|qwen3-14b)
    echo "Qwen3 is released under the Qwen Research License by Alibaba Cloud."
    echo "Quantisation by bartowski. See: https://huggingface.co/${HF_REPO}"
    ;;
  deepseek-r1-7b|deepseek-r1-14b)
    echo "DeepSeek-R1-Distill is released under the MIT License by DeepSeek AI."
    echo "Quantisation by bartowski. See: https://huggingface.co/${HF_REPO}"
    ;;
  neutts)
    echo "NeuTTS Air is released by Neuphonic under the NeuTTS License."
    echo "See: https://huggingface.co/${HF_REPO}"
    ;;
esac
echo "─────────────────────────────────────────────────────────────────────────────"

# ── NeuTTS Air setup function ─────────────────────────────────────────────────
NEUTTS_VENV=".neutts-venv"
NEUTTS_MODEL="${MODEL_DIR}/neutts-air-q4.gguf"

setup_neutts() {
  echo ""
  echo "─── NeuTTS Air voice cloning setup ─────────────────────────────────────────"
  echo ""

  # Check espeak-ng
  if ! command -v espeak-ng &>/dev/null; then
    echo "  espeak-ng not found — needed for NeuTTS phonemisation."
    echo "  Install: sudo apt install espeak-ng"
    echo "  Skipping NeuTTS setup."
    return
  fi

  # Find Python
  PYTHON=""
  for py in python3.12 python3.11 python3.10 python3; do
    if command -v "$py" &>/dev/null; then PYTHON="$py"; break; fi
  done
  if [[ -z "$PYTHON" ]]; then
    echo "  Python 3.10+ not found. Skipping NeuTTS setup."
    return
  fi

  # Create venv + install
  if [[ ! -d "$NEUTTS_VENV" ]]; then
    echo "  Creating Python venv ($NEUTTS_VENV)..."
    "$PYTHON" -m venv "$NEUTTS_VENV"
  fi
  echo "  Installing neutts[llama]..."
  "$NEUTTS_VENV/bin/pip" install --quiet --upgrade pip
  "$NEUTTS_VENV/bin/pip" install --quiet "neutts[llama]" soundfile numpy

  # Download model
  if [[ ! -f "$NEUTTS_MODEL" ]]; then
    echo "  Downloading NeuTTS Air Q4 GGUF (~527 MB)..."
    DL_FILE="neutts-air-Q4_0.gguf"
    if [[ -n "${HF_CMD:-}" ]]; then
      $HF_CMD download "neuphonic/neutts-air-q4-gguf" "$DL_FILE" --local-dir "$MODEL_DIR"
      [[ -f "${MODEL_DIR}/${DL_FILE}" ]] && mv "${MODEL_DIR}/${DL_FILE}" "$NEUTTS_MODEL"
    elif command -v wget &>/dev/null; then
      wget -q --show-progress -O "$NEUTTS_MODEL" \
        "https://huggingface.co/neuphonic/neutts-air-q4-gguf/resolve/main/$DL_FILE"
    elif command -v curl &>/dev/null; then
      curl -L --progress-bar -o "$NEUTTS_MODEL" \
        "https://huggingface.co/neuphonic/neutts-air-q4-gguf/resolve/main/$DL_FILE"
    fi
  fi

  # Generate voice references if missing
  if [[ ! -f "voices/default.wav" ]]; then
    echo "  Generating voice references..."
    ./scripts/generate-voices.sh 2>/dev/null || true
  fi

  # Verify
  if "$NEUTTS_VENV/bin/python" -c "from neutts import NeuTTS" 2>/dev/null; then
    echo ""
    echo "  ✓ NeuTTS Air ready (model: $(du -h "$NEUTTS_MODEL" 2>/dev/null | cut -f1 || echo '?'))"
  else
    echo "  WARNING: neutts import failed. TTS may not work."
  fi
  echo "─────────────────────────────────────────────────────────────────────────────"
}

# If called with explicit 'neutts' target, run setup and exit
if [[ "$MODEL" == "neutts" ]]; then
  setup_neutts
  exit 0
fi

# ── Optional recommended downloads ────────────────────────────────────────────

# NeuTTS Air (voice cloning for TTS modules)
if [[ -f "$NEUTTS_MODEL" ]] && [[ -d "$NEUTTS_VENV" ]]; then
  echo ""
  echo "✓ NeuTTS Air already set up."
else
  echo ""
  echo "NeuTTS Air enables neural voice cloning for TTS modules (~527 MB download)."
  read -r -p "Set up NeuTTS Air? [Y/n] " reply
  reply="${reply:-y}"
  if [[ "$reply" =~ ^[Yy]$ ]] || [[ -z "$reply" ]]; then
    setup_neutts
  else
    echo "  Skipped. Run later: ./scripts/download-models.sh neutts"
  fi
fi

# ── Optional sample packs (not auto-downloaded — just a reminder) ────────────
# These aren't bundled with the app or shipped via HuggingFace.  Point the
# user at archive.org and let them grab whichever packs they like.
echo ""
echo "─── Optional sample packs ──────────────────────────────────────────────────"
echo "  Impulse Instruct doesn't bundle audio samples.  Grab what you want:"
echo ""
echo "  Amen breaks (for the AMEN sampler module)"
echo "    https://archive.org/details/amen-breaks"
echo "    https://archive.org/details/amen-breaks-compilation"
echo "    → extract .wav files into  samples/amen/"
echo ""
echo "  Voice references (for the NeuTTS MC / TTS module)"
echo "    https://archive.org/details/librivoxaudio      (clean, recommended)"
echo "    https://commonvoice.mozilla.org/                (CC0 short clips)"
echo "    → drop a 3–15s mono WAV into  voices/"
echo "      alongside a matching  voices/<name>.txt  transcript"
echo "    (or run ./scripts/generate-voices.sh for robotic espeak-ng defaults)"
echo "────────────────────────────────────────────────────────────────────────────"
