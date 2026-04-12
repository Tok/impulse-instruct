#!/usr/bin/env bash
# ─── generate-voices.sh ──────────────────────────────────────────────────────
# Generate NeuTTS reference voice clips from espeak-ng.
# Run once after setup — creates voices/*.wav + *.txt pairs that NeuTTS Air
# uses as voice-cloning references.
#
# Each voice is a 10-15s clip of espeak-ng with a specific character, plus
# its exact transcript. NeuTTS clones the timbre from these references.
#
# Users can replace any .wav with their own recording (keep the .txt in sync)
# for a custom voice. Requirements: mono, 16-48 kHz, 3-15 seconds.
#
# Usage:
#   ./scripts/generate-voices.sh           # generate all voices
#   ./scripts/generate-voices.sh default   # generate only "default"
#   ./scripts/generate-voices.sh --force   # regenerate even if files exist
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail
cd "$(dirname "$0")/.."

VOICES_DIR="voices"
mkdir -p "$VOICES_DIR"

if ! command -v espeak-ng >/dev/null 2>&1; then
    echo "ERROR: espeak-ng not found. Install: sudo apt install espeak-ng"
    exit 1
fi

FORCE=0
TARGET=""
for arg in "$@"; do
    case "$arg" in
        --force) FORCE=1 ;;
        *) TARGET="$arg" ;;
    esac
done

generate_voice() {
    local name="$1" voice="$2" pitch="$3" speed="$4" text="$5"
    local wav="$VOICES_DIR/${name}.wav"
    local txt="$VOICES_DIR/${name}.txt"

    if [[ -f "$wav" ]] && [[ "$FORCE" -eq 0 ]]; then
        echo "  $name: exists (use --force to regenerate)"
        return
    fi

    espeak-ng -v "$voice" -p "$pitch" -s "$speed" -w "$wav" "$text"
    echo "$text" > "$txt"

    local dur
    dur=$(ffprobe -v error -show_entries format=duration \
        -of default=noprint_wrappers=1:nokey=1 "$wav" 2>/dev/null || echo "?")
    echo "  $name: ${dur}s ($(du -h "$wav" | cut -f1))"
}

echo "Generating NeuTTS voice references..."
echo ""

# ── Voice definitions ────────────────────────────────────────────────────────
# name          espeak voice    pitch  speed  transcript
# The transcript should be 10-15 seconds of natural speech.

VOICES=(
    "default|en|50|140|Welcome to Impulse Instruct. This is an AI powered music production studio with modular synthesis, drum machines, and intelligent agents that create music together in real time."
    "mc|en+m3|65|170|Selector! Big up the junglist massive! We have the bass, we have the breaks, we have the riddim. Pull up and rewind! This one goes out to all the ravers in the building tonight."
    "dj|en+m4|40|130|Good evening ladies and gentlemen. Welcome to the show. Tonight we have a very special set lined up for you. Sit back, relax, and let the music take you on a journey through sound."
    "robot|en+m7|30|120|System initialised. Audio synthesis module online. All parameters nominal. Beginning music generation sequence. Standby for output."
)

for entry in "${VOICES[@]}"; do
    IFS='|' read -r name voice pitch speed text <<< "$entry"
    if [[ -n "$TARGET" ]] && [[ "$name" != "$TARGET" ]]; then
        continue
    fi
    generate_voice "$name" "$voice" "$pitch" "$speed" "$text"
done

echo ""
echo "Voice references in $VOICES_DIR/"
echo "Replace any .wav with your own recording (mono, 16-48 kHz, 3-15s)"
echo "and update the .txt with the exact transcript."
