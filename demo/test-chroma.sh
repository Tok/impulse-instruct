#!/usr/bin/env bash
# Quick chroma offset test — captures 3s of the app window with different
# encoding approaches. Compare the output files to find which has no offset.
#
# Usage: start the app, then run: ./demo/test-chroma.sh
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="/tmp/chroma_tests"
rm -rf "$OUT" && mkdir -p "$OUT"

# Find the app window
WID=$(xdotool search --name "Impulse Instruct" 2>/dev/null | head -1 || true)
[ -z "$WID" ] && WID=$(xdotool search --name "impulse" 2>/dev/null | head -1 || true)
[ -z "$WID" ] && { echo "ERROR: App window not found. Start the app first."; exit 1; }
WID_DEC=$(printf '%d' "0x$(printf '%x' "$WID")")
echo "Window: $WID (dec: $WID_DEC)"

COMMON="-f x11grab -window_id $WID_DEC -framerate 15 -draw_mouse 0 -i ${DISPLAY} -t 3"

echo "Generating test clips in $OUT/ ..."
echo ""

# 1. Plain yuv420p (baseline — likely has offset)
echo -n "  1_plain_420p.mp4 ... "
ffmpeg -y $COMMON -pix_fmt yuv420p \
    -c:v libx264 -preset ultrafast "$OUT/1_plain_420p.mp4" \
    </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

# 2. yuv444p intermediate → 420p
echo -n "  2_444_intermediate.mp4 ... "
ffmpeg -y $COMMON -vf format=yuv444p -pix_fmt yuv420p \
    -sws_flags "lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
    -c:v libx264 -preset ultrafast "$OUT/2_444_intermediate.mp4" \
    </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

# 3. Full-range via scale filter
echo -n "  3_fullrange.mp4 ... "
ffmpeg -y $COMMON \
    -vf "scale=in_range=full:out_range=full:flags=lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
    -pix_fmt yuv420p -color_range pc \
    -c:v libx264 -preset ultrafast -x264-params fullrange=1 \
    "$OUT/3_fullrange.mp4" \
    </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

# 4. Explicit chroma location + colorspace
echo -n "  4_chromaloc.mp4 ... "
ffmpeg -y $COMMON -pix_fmt yuv420p \
    -chroma_sample_location left \
    -colorspace bt709 -color_primaries bt709 -color_trc bt709 \
    -c:v libx264 -preset ultrafast "$OUT/4_chromaloc.mp4" \
    </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

# 5. gbrp intermediate
echo -n "  5_gbrp_intermediate.mp4 ... "
ffmpeg -y $COMMON -vf format=gbrp -pix_fmt yuv420p \
    -sws_flags "lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
    -c:v libx264 -preset ultrafast "$OUT/5_gbrp_intermediate.mp4" \
    </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

# 6. Lossless RGB (reference — no YUV conversion)
echo -n "  6_lossless_rgb.mkv ... "
ffmpeg -y $COMMON -c:v libx264rgb -crf 0 \
    "$OUT/6_lossless_rgb.mkv" \
    </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

# 7. Full-range 420p with limited→full scale
echo -n "  7_scale_tv2pc.mp4 ... "
ffmpeg -y $COMMON \
    -vf "scale=in_range=pc:out_range=pc:flags=lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
    -pix_fmt yuv420p -color_range pc \
    -c:v libx264 -preset ultrafast -x264-params fullrange=1 \
    "$OUT/7_scale_tv2pc.mp4" \
    </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

# 8. yuv444p output (no 420 subsampling at all — compatibility test)
echo -n "  8_yuv444p.mp4 ... "
ffmpeg -y $COMMON -pix_fmt yuv444p -profile:v high444p \
    -c:v libx264 -preset ultrafast "$OUT/8_yuv444p.mp4" \
    </dev/null 2>/dev/null && echo "OK" || echo "FAIL"

echo ""
echo "Files in $OUT/:"
ls -lhS "$OUT/"
echo ""
echo "Compare these in a media player."
echo "  6_lossless_rgb.mkv = reference (no color conversion)"
echo "  8_yuv444p.mp4 = no chroma subsampling (should match reference)"
echo "  The first one that matches reference without offset is the fix."
