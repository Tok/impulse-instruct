#!/usr/bin/env bash
set -euo pipefail
# ─── demo/test-recording.sh ── quick recording pipeline test ────────────────
#
# Tests video capture, audio capture, subtitle generation, and encoding
# in a 10-second clip. App must be running with --api.
#
# Usage: ./demo/test-recording.sh
#        ./demo/test-recording.sh --app-running   (skip launch)
# ─────────────────────────────────────────────────────────────────────────────

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$DEMO_DIR/.." && pwd)"
OUTPUT_DIR="$DEMO_DIR/output"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

source "$DEMO_DIR/lib.sh"

APP_RUNNING=0
[ "${1:-}" = "--app-running" ] && APP_RUNNING=1

mkdir -p "$OUTPUT_DIR"

echo "=== Recording Pipeline Test ==="
echo ""

# ── Step 1: Find or launch app ───────────────────────────────────────────────

APP_PID=""
if [ "$APP_RUNNING" -eq 0 ]; then
    echo "[1/7] Launching app..."
    cd "$PROJECT_DIR"
    if [ -f "target/release/impulse-instruct" ]; then
        ./target/release/impulse-instruct --skip-wizard --mock --log trace &
    else
        echo "  No release build. Run: cargo build --release"
        exit 1
    fi
    APP_PID=$!
    wait_for_api 15
else
    echo "[1/7] Using running app"
    wait_for_api 5
fi

cleanup() {
    echo ""
    echo "Cleaning up..."
    [ -n "${FFMPEG_PID:-}" ] && kill -9 "$FFMPEG_PID" 2>/dev/null; wait "$FFMPEG_PID" 2>/dev/null || true
    [ -n "${PW_PID:-}" ] && kill -9 "$PW_PID" 2>/dev/null; wait "$PW_PID" 2>/dev/null || true
    [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null; wait "$APP_PID" 2>/dev/null || true
    echo "Done."
}
trap cleanup EXIT

# ── Step 2: Find window ─────────────────────────────────────────────────────

echo "[2/7] Finding app window..."
sleep 2
export APP_WINDOW_ID
APP_WINDOW_ID=$(find_app_window)
if [ -z "$APP_WINDOW_ID" ] || [ "$APP_WINDOW_ID" = "0" ]; then
    echo "  FAIL: Could not find app window"
    wmctrl -l 2>/dev/null || true
    exit 1
fi
echo "  Window: $APP_WINDOW_ID"
WINDOW_ID_DEC=$(printf '%d' "$APP_WINDOW_ID")

read -r GRAB_X GRAB_Y GRAB_W GRAB_H <<< "$(get_window_geometry "$APP_WINDOW_ID")"
echo "  Geometry: ${GRAB_W}x${GRAB_H}"

# ── Step 3: Start video capture ──────────────────────────────────────────────

RAW_VIDEO="$OUTPUT_DIR/test_raw_${TIMESTAMP}.mkv"
echo "[3/7] Starting video capture → $RAW_VIDEO"

# Use -t as a hard safety stop — prevents infinite recording if SIGINT fails
ffmpeg -y \
    -f x11grab \
    -window_id "$WINDOW_ID_DEC" \
    -framerate 30 \
    -draw_mouse 0 \
    -i "${DISPLAY}" \
    -t 15 \
    -c:v h264_nvenc -preset p4 -cq 20 \
    -pix_fmt yuv420p \
    -an \
    "$RAW_VIDEO" \
    </dev/null >/dev/null 2>&1 &
FFMPEG_PID=$!
sleep 1

if ! kill -0 "$FFMPEG_PID" 2>/dev/null; then
    echo "  FAIL: ffmpeg exited immediately (NVENC unavailable?)"
    echo "  Retrying with libx264..."
    ffmpeg -y \
        -f x11grab \
        -window_id "$WINDOW_ID_DEC" \
        -framerate 30 \
        -draw_mouse 0 \
        -i "${DISPLAY}" \
        -t 15 \
        -c:v libx264 -preset ultrafast -crf 23 \
        -pix_fmt yuv420p \
        -an \
        "$RAW_VIDEO" \
        </dev/null >/dev/null 2>&1 &
    FFMPEG_PID=$!
    sleep 1
    if ! kill -0 "$FFMPEG_PID" 2>/dev/null; then
        echo "  FAIL: ffmpeg with libx264 also failed"
        exit 1
    fi
    echo "  Using libx264"
fi
echo "  ffmpeg PID: $FFMPEG_PID"

# ── Step 4: Start audio capture ──────────────────────────────────────────────

APP_AUDIO="$OUTPUT_DIR/test_audio_${TIMESTAMP}.wav"
PW_PID=""
echo "[4/7] Starting audio capture..."

api_play
sleep 1

APP_PW_NODE=$(find_app_pw_node)
if [ -n "$APP_PW_NODE" ]; then
    pw-record --target "$APP_PW_NODE" "$APP_AUDIO" &
    PW_PID=$!
    echo "  PipeWire node: $APP_PW_NODE (PID: $PW_PID)"
else
    echo "  WARN: No app PipeWire node found — skipping audio"
fi

# ── Step 5: Generate test narration entries ──────────────────────────────────

echo "[5/7] Running 10-second test scenario..."

export DEMO_START_NS
DEMO_START_NS=$(date +%s%N)
rm -f "$NARRATION_LIST"

api_prompt '{"prompt": "random acid pattern"}' 2>/dev/null || true
sleep 2

# Write fake narration entries for SRT test
echo "2.0|2.0|Test subtitle line one" >> "$NARRATION_LIST"
echo "5.0|2.0|Test subtitle line two" >> "$NARRATION_LIST"
echo "8.0|2.0|Recording pipeline test" >> "$NARRATION_LIST"

sleep 8
api_stop

# ── Step 6: Stop captures ───────────────────────────────────────────────────

echo "[6/7] Stopping captures..."

echo -n "  ffmpeg: SIGINT..."
kill -INT "$FFMPEG_PID" 2>/dev/null
for i in 1 2 3 4 5; do
    if ! kill -0 "$FFMPEG_PID" 2>/dev/null; then
        echo " stopped after ${i}s"
        break
    fi
    sleep 1
done
if kill -0 "$FFMPEG_PID" 2>/dev/null; then
    echo " SIGKILL (hung)"
    kill -9 "$FFMPEG_PID" 2>/dev/null
    wait "$FFMPEG_PID" 2>/dev/null || true
fi
FFMPEG_PID=""

if [ -n "$PW_PID" ]; then
    echo -n "  pw-record: SIGINT..."
    kill -INT "$PW_PID" 2>/dev/null
    sleep 1
    kill -9 "$PW_PID" 2>/dev/null
    wait "$PW_PID" 2>/dev/null || true
    PW_PID=""
    echo " stopped"
fi

# ── Step 7: Encode + verify ─────────────────────────────────────────────────

echo "[7/7] Encoding + verification..."

SRT_FILE="$OUTPUT_DIR/test_${TIMESTAMP}.srt"
SRT_NAME="$SRT_FILE" generate_srt >/dev/null

FINAL="$OUTPUT_DIR/test_${TIMESTAMP}.mp4"
FINAL_SUBS="$OUTPUT_DIR/test_${TIMESTAMP}_subs.mp4"

HAS_AUDIO=0
[ -f "$APP_AUDIO" ] && [ -s "$APP_AUDIO" ] && HAS_AUDIO=1

AUDIO_ARGS=""
[ "$HAS_AUDIO" -eq 1 ] && AUDIO_ARGS="-i $APP_AUDIO -c:a aac -b:a 128k -map 0:v:0 -map 1:a:0 -shortest"

# Clean version
echo -n "  Clean video: "
if ffmpeg -y -i "$RAW_VIDEO" $AUDIO_ARGS \
    -c:v h264_nvenc -preset p4 -cq 22 \
    "$FINAL" </dev/null 2>/dev/null; then
    echo "OK ($(du -h "$FINAL" | cut -f1))"
else
    # Fallback to libx264
    ffmpeg -y -i "$RAW_VIDEO" $AUDIO_ARGS \
        -c:v libx264 -preset fast -crf 23 \
        "$FINAL" </dev/null 2>/dev/null && echo "OK (libx264)" || echo "FAIL"
fi

# Subtitled version
echo -n "  Subtitled video: "
if [ -f "$SRT_FILE" ]; then
    if ffmpeg -y -i "$RAW_VIDEO" $AUDIO_ARGS \
        -vf "subtitles=${SRT_FILE}:force_style='FontSize=22,FontName=monospace,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,Outline=2,MarginV=40'" \
        -c:v h264_nvenc -preset p4 -cq 22 \
        "$FINAL_SUBS" </dev/null 2>/dev/null; then
        echo "OK ($(du -h "$FINAL_SUBS" | cut -f1))"
    else
        echo "FAIL (subtitle burn-in)"
    fi
else
    echo "SKIP (no SRT)"
fi

# ── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "=== Results ==="
for f in "$RAW_VIDEO" "$APP_AUDIO" "$SRT_FILE" "$FINAL" "$FINAL_SUBS"; do
    if [ -f "$f" ]; then
        local_size=$(du -h "$f" | cut -f1)
        local_dur=$(ffprobe -v error -show_entries format=duration \
            -of default=noprint_wrappers=1:nokey=1 "$f" 2>/dev/null || echo "?")
        printf "  %-50s %6s  %ss\n" "$(basename "$f")" "$local_size" "$local_dur"
    else
        printf "  %-50s  MISSING\n" "$(basename "$f")"
    fi
done

echo ""
echo "Test complete."
