#!/usr/bin/env bash
# Setup NeuTTS Air — creates Python venv, installs deps, downloads GGUF model.
#
# Usage:  ./scripts/setup-neutts.sh
#
# Prerequisites: Python 3.10+, espeak-ng (system package)

set -eu
cd "$(dirname "$0")/.."
PROJECT_DIR="$(pwd)"

VENV_DIR="$PROJECT_DIR/.neutts-venv"
MODEL_DIR="$PROJECT_DIR/models"
MODEL_FILE="$MODEL_DIR/neutts-air-q4.gguf"

echo "=== NeuTTS Air Setup ==="
echo ""

# ── 1. Check prerequisites ──────────────────────────────────────────────────
if ! command -v espeak-ng >/dev/null 2>&1; then
    echo "ERROR: espeak-ng not found.  Install it:"
    echo "  Ubuntu/Debian: sudo apt install espeak-ng"
    echo "  macOS: brew install espeak-ng"
    echo "  Windows: choco install espeak-ng"
    exit 1
fi
echo "[1/4] espeak-ng: OK"

PYTHON=""
for py in python3.12 python3.11 python3.10 python3; do
    if command -v "$py" >/dev/null 2>&1; then
        PYTHON="$py"
        break
    fi
done
if [ -z "$PYTHON" ]; then
    echo "ERROR: Python 3.10+ not found"
    exit 1
fi
echo "[1/4] Python: $($PYTHON --version)"

# ── 2. Create venv + install neutts ────────────────────────────────────────
if [ ! -d "$VENV_DIR" ]; then
    echo "[2/4] Creating Python venv at $VENV_DIR ..."
    "$PYTHON" -m venv "$VENV_DIR"
else
    echo "[2/4] Venv already exists at $VENV_DIR"
fi

echo "[2/4] Installing neutts[llama] ..."
"$VENV_DIR/bin/pip" install --quiet --upgrade pip
"$VENV_DIR/bin/pip" install --quiet "neutts[llama]" soundfile numpy
echo "[2/4] neutts installed"

# ── 3. Download GGUF model ─────────────────────────────────────────────────
mkdir -p "$MODEL_DIR"
if [ -f "$MODEL_FILE" ]; then
    echo "[3/4] Model already present: $MODEL_FILE ($(du -h "$MODEL_FILE" | cut -f1))"
else
    echo "[3/4] Downloading NeuTTS Air Q4 GGUF (~527 MB) ..."
    if command -v huggingface-cli >/dev/null 2>&1; then
        huggingface-cli download neuphonic/neutts-air-q4-gguf \
            neutts-air-Q4_0.gguf \
            --local-dir "$MODEL_DIR" \
            --local-dir-use-symlinks False
        # Rename to our expected name
        [ -f "$MODEL_DIR/neutts-air-Q4_0.gguf" ] && mv "$MODEL_DIR/neutts-air-Q4_0.gguf" "$MODEL_FILE"
    elif command -v wget >/dev/null 2>&1; then
        wget -q --show-progress -O "$MODEL_FILE" \
            "https://huggingface.co/neuphonic/neutts-air-q4-gguf/resolve/main/neutts-air-Q4_0.gguf"
    elif command -v curl >/dev/null 2>&1; then
        curl -L --progress-bar -o "$MODEL_FILE" \
            "https://huggingface.co/neuphonic/neutts-air-q4-gguf/resolve/main/neutts-air-Q4_0.gguf"
    else
        echo "ERROR: No download tool found (huggingface-cli, wget, or curl)"
        exit 1
    fi
    echo "[3/4] Model downloaded: $(du -h "$MODEL_FILE" | cut -f1)"
fi

# ── 4. Verify ──────────────────────────────────────────────────────────────
echo "[4/4] Verifying installation ..."
"$VENV_DIR/bin/python" -c "from neutts import NeuTTS; print('  neutts import: OK')"
echo ""
echo "=== Setup complete ==="
echo ""
echo "To start the TTS server:"
echo "  $VENV_DIR/bin/python scripts/neutts-server.py"
echo ""
echo "To test one-shot synthesis:"
echo "  $VENV_DIR/bin/python scripts/neutts-server.py --oneshot \\"
echo "    --text 'Hello world' --out /tmp/test.wav \\"
echo "    --ref-audio voices/default.wav --ref-text voices/default.txt"
