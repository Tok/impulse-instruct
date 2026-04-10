#!/usr/bin/env bash
set -euo pipefail
# ─── demo/record-demo.sh ── main orchestrator ────────────────────────────────
#
# Usage:
#   ./demo/record-demo.sh                  # build + record full demo
#   ./demo/record-demo.sh --skip-build     # skip cargo build
#   ./demo/record-demo.sh --no-tts         # record without narration
#   ./demo/record-demo.sh --no-subtitles   # skip subtitle burn-in
#   ./demo/record-demo.sh --app-running    # don't launch app (already running)
#   ./demo/record-demo.sh --dry-run        # just pre-generate TTS, don't record
#
# Output: demo/output/impulse_demo_<timestamp>.mp4
# ─────────────────────────────────────────────────────────────────────────────

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$DEMO_DIR/.." && pwd)"
OUTPUT_DIR="$DEMO_DIR/output"

# ─── Parse flags ──────────────────────────────────────────────────────────────

SKIP_BUILD=0
NO_TTS=0
NO_SUBTITLES=0
APP_RUNNING=0
DRY_RUN=0

for arg in "$@"; do
    case "$arg" in
        --skip-build)    SKIP_BUILD=1 ;;
        --no-tts)        NO_TTS=1 ;;
        --no-subtitles)  NO_SUBTITLES=1 ;;
        --app-running)   APP_RUNNING=1 ;;
        --dry-run)       DRY_RUN=1 ;;
        -h|--help)
            head -14 "$0" | tail -11
            exit 0
            ;;
        *) echo "Unknown flag: $arg" >&2; exit 1 ;;
    esac
done

# ─── Source helpers ───────────────────────────────────────────────────────────

source "$DEMO_DIR/lib.sh"

# ─── Pre-flight checks ───────────────────────────────────────────────────────

echo "=== Impulse Instruct Demo Recorder ==="
echo ""

check_dep() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "ERROR: '$1' not found. Install it first." >&2
        echo "  $2" >&2
        exit 1
    fi
}

check_dep ffmpeg      "sudo apt install ffmpeg"
check_dep bc          "sudo apt install bc"
check_dep xwininfo    "sudo apt install x11-utils"
check_dep pw-record   "sudo apt install pipewire (should already be there)"
check_dep python3     "sudo apt install python3"
if [ "$NO_TTS" -eq 0 ]; then
    check_dep espeak-ng  "sudo apt install espeak-ng"
    check_dep paplay     "sudo apt install pulseaudio-utils"
fi

# Verify xdo.py works
if ! python3 "$DEMO_DIR/xdo.py" getactivewindow >/dev/null 2>&1; then
    echo "ERROR: xdo.py failed. Is libxdo3 installed? (sudo apt install libxdo3)" >&2
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

# ─── Pre-generate TTS clips ──────────────────────────────────────────────────

echo "[1/6] Pre-generating TTS clips..."
if [ "$NO_TTS" -eq 0 ]; then
    idx=0
    clip_id=""
    while IFS= read -r line; do
        if [[ "$line" =~ narrate[[:space:]]+(--wait[[:space:]]+)?\"([^\"]+)\"[[:space:]]+\\?$ ]]; then
            clip_id="${BASH_REMATCH[2]}"
        elif [[ "$line" =~ ^[[:space:]]+\"(.+)\"$ ]] && [ -n "${clip_id:-}" ]; then
            text="${BASH_REMATCH[1]}"
            tts_generate "$clip_id" "$text" >/dev/null
            echo "  Generated: $clip_id"
            clip_id=""
        elif [[ "$line" =~ narrate[[:space:]]+(--wait[[:space:]]+)?\"([^\"]+)\"[[:space:]]+\"(.+)\" ]]; then
            clip_id="${BASH_REMATCH[2]}"
            text="${BASH_REMATCH[3]}"
            tts_generate "$clip_id" "$text" >/dev/null
            echo "  Generated: $clip_id"
            clip_id=""
        fi
    done < "$DEMO_DIR/scenario.sh"
    echo "  TTS clips cached in $TTS_DIR"
else
    echo "  Skipped (--no-tts)"
    narrate() { :; }
    wait_narration() { :; }
fi

if [ "$DRY_RUN" -eq 1 ]; then
    echo ""
    echo "Dry run complete. TTS clips are in $TTS_DIR"
    exit 0
fi

# ─── Build the app ───────────────────────────────────────────────────────────

if [ "$SKIP_BUILD" -eq 0 ] && [ "$APP_RUNNING" -eq 0 ]; then
    echo ""
    echo "[2/6] Building Impulse Instruct..."
    cd "$PROJECT_DIR"
    cargo build --release 2>&1 | tail -3
    echo "  Build complete."
else
    echo "[2/6] Build skipped."
fi

# ─── Cleanup trap ─────────────────────────────────────────────────────────────

APP_PID=""
FFMPEG_PID=""
PW_RECORD_PID=""

cleanup() {
    echo ""
    echo "Cleaning up..."
    [ -n "$FFMPEG_PID" ]    && kill "$FFMPEG_PID"    2>/dev/null && wait "$FFMPEG_PID"    2>/dev/null
    [ -n "$PW_RECORD_PID" ] && kill "$PW_RECORD_PID" 2>/dev/null && wait "$PW_RECORD_PID" 2>/dev/null
    [ -n "$APP_PID" ]       && kill "$APP_PID"       2>/dev/null && wait "$APP_PID"       2>/dev/null
    rm -f "$NARRATION_LIST"
    echo "Done."
}
trap cleanup EXIT

TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# ─── Launch the app ──────────────────────────────────────────────────────────

if [ "$APP_RUNNING" -eq 0 ]; then
    echo ""
    echo "[3/6] Launching app with --skip-wizard..."
    cd "$PROJECT_DIR"
    # Real LLM mode — llama-server starts automatically for the configured model.
    # Pass --model explicitly to ensure Gemma 4 is loaded (session.json may be stale).
    # The API is on by default (port 8765).
    MODEL_PATH="models/gemma-4-E4B-it-Q4_K_M.gguf"
    if [ ! -f "$MODEL_PATH" ]; then
        echo "  WARNING: $MODEL_PATH not found. Run ./scripts/download-models.sh first." >&2
        echo "  Falling back to --mock mode." >&2
        MODEL_PATH=""
    fi
    # Launch WITHOUT redirecting stdout/stderr. Redirecting can crash
    # eframe/egui when GPU drivers write to stderr expecting a terminal.
    # App log output will be visible in the terminal alongside demo progress.
    if [ -n "$MODEL_PATH" ]; then
        ./target/release/impulse-instruct --skip-wizard --model "$MODEL_PATH" --log warn &
    else
        ./target/release/impulse-instruct --skip-wizard --mock --log warn &
    fi
    APP_PID=$!
    wait_for_api 30
else
    echo "[3/6] Using already-running app."
    wait_for_api 5
fi

# ─── Find the app window ─────────────────────────────────────────────────────

echo ""
echo "[4/6] Setting up capture..."

# Give the window time to appear and settle
sleep 3

# Find the app window via wmctrl (reliable window search by title)
export APP_WINDOW_ID
APP_WINDOW_ID=""
for attempt in 1 2 3 4 5; do
    APP_WINDOW_ID=$(find_app_window)
    if [ -n "$APP_WINDOW_ID" ] && [ "$APP_WINDOW_ID" != "0" ]; then
        break
    fi
    echo "  Waiting for window (attempt $attempt/5)..."
    sleep 2
done

if [ -z "$APP_WINDOW_ID" ] || [ "$APP_WINDOW_ID" = "0" ]; then
    echo "  ERROR: Could not find 'Impulse Instruct' window." >&2
    echo "  Windows found:" >&2
    wmctrl -l 2>/dev/null >&2 || true
    exit 1
fi

echo "  Found window: $APP_WINDOW_ID"

# Focus and raise the window
wmctrl -i -a "$APP_WINDOW_ID" 2>/dev/null || true
sleep 0.5

# Get geometry — APP_WINDOW_ID is already hex from wmctrl (e.g. 0x03000003)
read -r GRAB_X GRAB_Y GRAB_W GRAB_H <<< "$(get_window_geometry "$APP_WINDOW_ID")"
echo "  Window geometry: ${GRAB_W}x${GRAB_H} at +${GRAB_X}+${GRAB_Y}"

# Validate: reject if it looks like the full screen (probably wrong window)
SCREEN_W=$(xdpyinfo 2>/dev/null | awk '/dimensions:/{print $2}' | cut -dx -f1)
if [ "${GRAB_W:-0}" -ge "${SCREEN_W:-9999}" ] 2>/dev/null; then
    echo "  WARNING: Window is full-screen size. Using inner frame geometry."
    # For maximized windows, use the frame extents to trim decorations
fi

# Ensure even dimensions (required by h264_nvenc)
GRAB_W=$(( GRAB_W / 2 * 2 ))
GRAB_H=$(( GRAB_H / 2 * 2 ))

# ─── Start per-app audio capture via PipeWire ────────────────────────────────

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
APP_AUDIO="$OUTPUT_DIR/app_audio_${TIMESTAMP}.wav"
RAW_VIDEO="$OUTPUT_DIR/raw_${TIMESTAMP}.mkv"
NARRATION_AUDIO="$OUTPUT_DIR/narration_${TIMESTAMP}.wav"
FINAL_VIDEO="$OUTPUT_DIR/impulse_demo_${TIMESTAMP}.mp4"

# Try to find the app's PipeWire node for isolated audio capture
APP_PW_NODE=$(find_app_pw_node)
if [ -n "$APP_PW_NODE" ]; then
    echo "  App audio: PipeWire node $APP_PW_NODE (isolated capture)"
    pw-record --target "$APP_PW_NODE" "$APP_AUDIO" &
    PW_RECORD_PID=$!
else
    echo "  App audio: PipeWire node not found yet."
    echo "  Will retry after sequencer starts (app creates audio output on first sound)."
    # We'll start capture after the sequencer begins playing — see below
fi

# ─── Start screen recording (video only, or video + system audio fallback) ───

echo ""
echo "[5/6] Recording demo..."
echo "  Capturing ${GRAB_W}x${GRAB_H} from +${GRAB_X},${GRAB_Y}"
echo "  Raw output: $RAW_VIDEO"

# Screen capture — video only (audio captured separately via pw-record)
ffmpeg -y \
    -video_size "${GRAB_W}x${GRAB_H}" \
    -framerate 30 \
    -f x11grab -i "${DISPLAY}+${GRAB_X},${GRAB_Y}" \
    -c:v h264_nvenc -preset p4 -cq 20 \
    -pix_fmt yuv420p \
    -an \
    "$RAW_VIDEO" \
    </dev/null >/dev/null 2>&1 &
FFMPEG_PID=$!

sleep 1  # let ffmpeg settle

# Clear narration log
rm -f "$NARRATION_LIST"

# Record start time (nanoseconds)
export DEMO_START_NS
DEMO_START_NS=$(date +%s%N)

echo "  Recording started. Running scenario..."
echo ""

# ─── Run the demo scenario ───────────────────────────────────────────────────

# Hook: after api_play, try to start per-app capture if we don't have it yet
_original_api_play=$(declare -f api_play)
api_play() {
    curl -sf -X POST "$API/api/sequencer/play" >/dev/null 2>&1
    # If we haven't started per-app audio yet, try now
    if [ -z "$PW_RECORD_PID" ]; then
        sleep 0.5  # give cpal a moment to create the PW node
        APP_PW_NODE=$(find_app_pw_node)
        if [ -n "$APP_PW_NODE" ]; then
            echo "  [auto] Starting per-app audio capture (node $APP_PW_NODE)"
            pw-record --target "$APP_PW_NODE" "$APP_AUDIO" &
            PW_RECORD_PID=$!
        fi
    fi
}

# Disable ALL strict modes during the scenario — API calls, TTS playback,
# and bc/ffprobe can return non-zero or reference empty variables.
set +eu
set +o pipefail
trap 'echo "  !! FAILED at line $LINENO: $BASH_COMMAND (exit $?)" >&2' ERR
source "$DEMO_DIR/scenario.sh"
SCENARIO_EXIT=$?
trap - ERR
set -euo pipefail

if [ "$SCENARIO_EXIT" -ne 0 ]; then
    echo "  WARNING: Scenario exited with code $SCENARIO_EXIT"
fi

wait_narration

echo ""
echo "  Scenario complete. Stopping recording..."

# ─── Stop recording ──────────────────────────────────────────────────────────

sleep 1

# Stop screen recording
kill "$FFMPEG_PID" 2>/dev/null
wait "$FFMPEG_PID" 2>/dev/null || true
FFMPEG_PID=""

# Stop audio recording
if [ -n "$PW_RECORD_PID" ]; then
    kill "$PW_RECORD_PID" 2>/dev/null
    wait "$PW_RECORD_PID" 2>/dev/null || true
    PW_RECORD_PID=""
fi

echo "  Raw video: $RAW_VIDEO"
[ -f "$APP_AUDIO" ] && echo "  App audio: $APP_AUDIO"

# ─── Post-process: merge video + audio + subtitles ────────────────────────────

echo ""
echo "[6/6] Post-processing..."

# Check what we have
AUDIO_COUNT=0
if [ -f "$APP_AUDIO" ] && [ -s "$APP_AUDIO" ]; then
    AUDIO_COUNT=1
    echo "  App audio: $APP_AUDIO ($(du -h "$APP_AUDIO" | cut -f1))"
else
    echo "  App audio: not captured"
fi
echo "  Raw video: $(du -h "$RAW_VIDEO" 2>/dev/null | cut -f1)"
echo "  Encoding final video (this may take a minute)..."

# Check if we have narration clips logged — merge them into one track
if [ "$NO_TTS" -eq 0 ] && [ -f "$NARRATION_LIST" ]; then
    SRT_FILE=$(generate_srt)
    echo "  Subtitles: $SRT_FILE"
fi

if [ "$AUDIO_COUNT" -gt 0 ]; then
    # Mux video + app audio (+ burn subtitles if available)
    if [ "$NO_SUBTITLES" -eq 0 ] && [ -f "${SRT_FILE:-}" ]; then
        ffmpeg -y \
            -i "$RAW_VIDEO" \
            -i "$APP_AUDIO" \
            -vf "subtitles=${SRT_FILE}:force_style='FontSize=22,FontName=monospace,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,Outline=2,MarginV=40'" \
            -c:v h264_nvenc -preset p4 -cq 22 \
            -c:a aac -b:a 192k \
            -map 0:v:0 -map 1:a:0 \
            -shortest \
            "$FINAL_VIDEO" \
            </dev/null
    else
        ffmpeg -y \
            -i "$RAW_VIDEO" \
            -i "$APP_AUDIO" \
            -c:v h264_nvenc -preset p4 -cq 22 \
            -c:a aac -b:a 192k \
            -map 0:v:0 -map 1:a:0 \
            -shortest \
            "$FINAL_VIDEO" \
            </dev/null
    fi
else
    # No separate audio — just re-encode video (+ subtitles)
    if [ "$NO_SUBTITLES" -eq 0 ] && [ -f "${SRT_FILE:-}" ]; then
        ffmpeg -y \
            -i "$RAW_VIDEO" \
            -vf "subtitles=${SRT_FILE}:force_style='FontSize=22,FontName=monospace,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,Outline=2,MarginV=40'" \
            -c:v h264_nvenc -preset p4 -cq 22 \
            "$FINAL_VIDEO" \
            </dev/null
    else
        ffmpeg -y -i "$RAW_VIDEO" \
            -c:v h264_nvenc -preset p4 -cq 22 \
            "$FINAL_VIDEO" \
            </dev/null
    fi
fi

echo ""
echo "=== Demo recording complete ==="
echo "  Output: $FINAL_VIDEO"
ls -lh "$FINAL_VIDEO" 2>/dev/null | awk '{print "  Size:  "$5}'
ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 \
    "$FINAL_VIDEO" 2>/dev/null | awk '{printf "  Duration: %.1fs\n", $1}'
echo ""
