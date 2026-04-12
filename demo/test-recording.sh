#!/usr/bin/env bash
# ─── demo/test-recording.sh ── quick recording pipeline test ────────────────
#
# Tests video capture, audio capture, subtitle generation, and encoding
# in a 10-second clip. App must be running or will be launched in mock mode.
#
# Usage: ./demo/test-recording.sh              (auto-detects or launches app)
#        ./demo/test-recording.sh --app-running (skip launch)
# ─────────────────────────────────────────────────────────────────────────────

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$DEMO_DIR/.." && pwd)"
OUTPUT_DIR="$DEMO_DIR/output"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
VERSION=$(grep '^version' "$PROJECT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
BATCH_DIR="$OUTPUT_DIR/${TIMESTAMP}"
BASENAME="v${VERSION}-test"

# Clean up old test batches (keep last 3)
mkdir -p "$OUTPUT_DIR"
ls -1dt "$OUTPUT_DIR"/20* 2>/dev/null | tail -n +4 | xargs rm -rf 2>/dev/null || true

source "$DEMO_DIR/lib.sh"

APP_RUNNING=0
[ "${1:-}" = "--app-running" ] && APP_RUNNING=1

# Auto-detect running app
if [ "$APP_RUNNING" -eq 0 ] && curl -sf "$API/api/state" >/dev/null 2>&1; then
    echo "App already running — skipping launch"
    APP_RUNNING=1
fi

mkdir -p "$BATCH_DIR"

echo "=== Recording Pipeline Test ==="
echo ""

# ── Step 1: Find or launch app ───────────────────────────────────────────────

APP_PID=""
if [ "$APP_RUNNING" -eq 0 ]; then
    echo "[1/7] Launching app (mock mode)..."
    cd "$PROJECT_DIR"
    if [ -f "target/release/impulse-instruct" ]; then
        ./target/release/impulse-instruct --skip-wizard --mock --log info &
    else
        echo "  No release build. Run: cargo build --release"
        exit 1
    fi
    APP_PID=$!
    wait_for_api 15 || { echo "FAIL: API not reachable"; exit 1; }
else
    echo "[1/7] Using running app"
    wait_for_api 5 || { echo "FAIL: API not reachable"; exit 1; }
fi

FFMPEG_PID=""
PW_PID=""

cleanup() {
    echo ""
    echo "Cleaning up..."
    [ -n "$FFMPEG_PID" ] && kill -9 "$FFMPEG_PID" 2>/dev/null
    [ -n "$PW_PID" ] && kill -9 "$PW_PID" 2>/dev/null
    [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null
    wait 2>/dev/null
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

RAW_VIDEO="$BATCH_DIR/${BASENAME}-raw.mkv"
echo "[3/7] Starting video capture → $(basename "$RAW_VIDEO")"

# -t 15 hard stops after 15s even if SIGINT fails
setsid ffmpeg -y \
    -f x11grab \
    -window_id "$WINDOW_ID_DEC" \
    -framerate 30 \
    -draw_mouse 0 \
    -i "${DISPLAY}" \
    -t 15 \
    -pix_fmt yuv420p \
    -c:v h264_nvenc -preset p4 -cq 20 \
    -an \
    "$RAW_VIDEO" \
    </dev/null >/dev/null 2>&1 &
FFMPEG_PID=$!
sleep 1

if ! kill -0 "$FFMPEG_PID" 2>/dev/null; then
    echo "  NVENC failed, trying libx264..."
    setsid ffmpeg -y \
        -f x11grab \
        -window_id "$WINDOW_ID_DEC" \
        -framerate 30 \
        -draw_mouse 0 \
        -i "${DISPLAY}" \
        -t 15 \
        -pix_fmt yuv420p \
        -c:v libx264 -preset ultrafast -crf 23 \
        -an \
        "$RAW_VIDEO" \
        </dev/null >/dev/null 2>&1 &
    FFMPEG_PID=$!
    sleep 1
    kill -0 "$FFMPEG_PID" 2>/dev/null || { echo "  FAIL: ffmpeg won't start"; exit 1; }
    echo "  Using libx264"
fi
echo "  ffmpeg PID: $FFMPEG_PID"

# ── Step 4: Start audio capture ──────────────────────────────────────────────

APP_AUDIO="$BATCH_DIR/${BASENAME}-audio.wav"
echo "[4/7] Starting audio capture..."

api_play
sleep 1

APP_PW_NODE=$(find_app_pw_node)
if [ -n "$APP_PW_NODE" ]; then
    pw-record --target "$APP_PW_NODE" "$APP_AUDIO" &
    PW_PID=$!
    echo "  PipeWire node: $APP_PW_NODE (PID: $PW_PID)"
else
    echo "  WARN: No app PipeWire node — skipping audio"
fi

# ── Step 5: Record 10 seconds ───────────────────────────────────────────────

echo "[5/7] Recording 10 seconds..."

export DEMO_START_NS
DEMO_START_NS=$(date +%s%N)
rm -f "$NARRATION_LIST"

# Write fake narration entries for SRT test
echo "2.0|2.0|Test subtitle line one" >> "$NARRATION_LIST"
echo "5.0|2.0|Test subtitle line two" >> "$NARRATION_LIST"
echo "8.0|2.0|Recording pipeline test" >> "$NARRATION_LIST"

sleep 10
api_stop

# ── Step 6: Stop captures ───────────────────────────────────────────────────
# Disable strict mode — kills return non-zero and that's fine
set +eu

echo "[6/7] Stopping captures..."

echo -n "  ffmpeg: "
kill -INT "$FFMPEG_PID" 2>/dev/null
sleep 2
if kill -0 "$FFMPEG_PID" 2>/dev/null; then
    echo -n "SIGKILL..."
    kill -9 -"$FFMPEG_PID" 2>/dev/null
    kill -9 "$FFMPEG_PID" 2>/dev/null
fi
wait "$FFMPEG_PID" 2>/dev/null
FFMPEG_PID=""
echo "stopped"

if [ -n "$PW_PID" ]; then
    echo -n "  pw-record: "
    kill -INT "$PW_PID" 2>/dev/null
    sleep 1
    kill -9 "$PW_PID" 2>/dev/null
    wait "$PW_PID" 2>/dev/null
    PW_PID=""
    echo "stopped"
fi

set -eu

# ── Step 7: Encode + verify ─────────────────────────────────────────────────

echo "[7/7] Encoding + verification..."

# SRT
SRT_FILE="$BATCH_DIR/${BASENAME}.srt"
SRT_NAME="$SRT_FILE" generate_srt >/dev/null 2>&1 || true
[ -f "$SRT_FILE" ] && echo "  SRT: OK ($(wc -l < "$SRT_FILE") lines)" || echo "  SRT: FAIL"

FINAL="$BATCH_DIR/${BASENAME}.mp4"
FINAL_SUBS="$BATCH_DIR/${BASENAME}-subs.mp4"

HAS_AUDIO=0
[ -f "$APP_AUDIO" ] && [ -s "$APP_AUDIO" ] && HAS_AUDIO=1

AUDIO_ARGS=""
[ "$HAS_AUDIO" -eq 1 ] && AUDIO_ARGS="-i $APP_AUDIO -c:a aac -b:a 128k -map 0:v:0 -map 1:a:0 -shortest"

SWS_ENCODE="-sws_flags lanczos+accurate_rnd+full_chroma_int+full_chroma_inp"
echo -n "  Clean video: "
if ffmpeg -y -i "$RAW_VIDEO" $AUDIO_ARGS \
    $SWS_ENCODE -c:v h264_nvenc -preset p4 -cq 22 \
    "$FINAL" </dev/null 2>/dev/null; then
    echo "OK ($(du -h "$FINAL" | cut -f1))"
elif ffmpeg -y -i "$RAW_VIDEO" $AUDIO_ARGS \
    $SWS_ENCODE -c:v libx264 -preset fast -crf 23 \
    "$FINAL" </dev/null 2>/dev/null; then
    echo "OK libx264 ($(du -h "$FINAL" | cut -f1))"
else
    echo "FAIL"
fi

# Subtitled version
echo -n "  Subtitled video: "
if [ -f "$SRT_FILE" ] && [ -f "$RAW_VIDEO" ]; then
    if ffmpeg -y -i "$RAW_VIDEO" $AUDIO_ARGS \
        $SWS_ENCODE -vf "subtitles=${SRT_FILE}:force_style='FontSize=22,FontName=monospace,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,Outline=2,MarginV=40'" \
        -c:v h264_nvenc -preset p4 -cq 22 \
        "$FINAL_SUBS" </dev/null 2>/dev/null; then
        echo "OK ($(du -h "$FINAL_SUBS" | cut -f1))"
    elif ffmpeg -y -i "$RAW_VIDEO" $AUDIO_ARGS \
        $SWS_ENCODE -vf "subtitles=${SRT_FILE}:force_style='FontSize=22,FontName=monospace'" \
        -c:v libx264 -preset fast -crf 23 \
        "$FINAL_SUBS" </dev/null 2>/dev/null; then
        echo "OK libx264 ($(du -h "$FINAL_SUBS" | cut -f1))"
    else
        echo "FAIL"
    fi
else
    echo "SKIP (no SRT or raw video)"
fi

# ── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "=== Results ==="
for f in "$RAW_VIDEO" "$APP_AUDIO" "$SRT_FILE" "$FINAL" "$FINAL_SUBS"; do
    if [ -f "$f" ]; then
        sz=$(du -h "$f" | cut -f1)
        dur=$(ffprobe -v error -show_entries format=duration \
            -of default=noprint_wrappers=1:nokey=1 "$f" 2>/dev/null || echo "?")
        printf "  %-50s %6s  %ss\n" "$(basename "$f")" "$sz" "$dur"
    else
        printf "  %-50s  MISSING\n" "$(basename "$f")"
    fi
done

# ── Chroma offset test encodes ────────────────────────────────────────────────
# Re-encode the raw capture with different approaches to find which fixes the
# vertical chroma offset. Compare these side-by-side with the raw.

if [ -f "$RAW_VIDEO" ]; then
    echo ""
    echo "=== Chroma test encodes ==="
    CDIR="$BATCH_DIR/chroma_tests"
    mkdir -p "$CDIR"

    echo -n "  1_plain_420p.mp4 ... "
    ffmpeg -y -i "$RAW_VIDEO" -pix_fmt yuv420p \
        -c:v libx264 -preset fast -crf 20 \
        "$CDIR/1_plain_420p.mp4" </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

    echo -n "  2_fullrange.mp4 ... "
    ffmpeg -y -i "$RAW_VIDEO" \
        -vf "scale=in_range=full:out_range=full:flags=lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
        -pix_fmt yuv420p -color_range pc \
        -c:v libx264 -preset fast -crf 20 -x264-params fullrange=1 \
        "$CDIR/2_fullrange.mp4" </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

    echo -n "  3_chromaloc.mp4 ... "
    ffmpeg -y -i "$RAW_VIDEO" -pix_fmt yuv420p \
        -chroma_sample_location left \
        -colorspace bt709 -color_primaries bt709 -color_trc bt709 \
        -c:v libx264 -preset fast -crf 20 \
        "$CDIR/3_chromaloc.mp4" </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

    echo -n "  4_yuv444p.mp4 ... "
    ffmpeg -y -i "$RAW_VIDEO" -pix_fmt yuv444p -profile:v high444p \
        -c:v libx264 -preset fast -crf 20 \
        "$CDIR/4_yuv444p.mp4" </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

    echo -n "  5_sws_lanczos.mp4 ... "
    ffmpeg -y -i "$RAW_VIDEO" \
        -sws_flags "lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
        -c:v libx264 -preset fast -crf 20 \
        "$CDIR/5_sws_lanczos.mp4" </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

    echo ""
    echo "Compare these files in $CDIR/"
    echo "  4_yuv444p.mp4 = no chroma subsampling (reference)"
    echo "  The first 420p variant matching it is the fix."
fi

echo ""
echo "Test complete."
