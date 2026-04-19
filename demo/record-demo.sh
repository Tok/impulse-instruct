#!/usr/bin/env bash
set -u  # warn on unset vars, but don't exit on errors (kills return non-zero)
# ─── demo/record-demo.sh ── main orchestrator ────────────────────────────────
#
# Usage:
#   ./demo/record-demo.sh                          # record intro (auto-detects running app)
#   ./demo/record-demo.sh --scenario style-acid    # run a specific scenario
#   ./demo/record-demo.sh --trim-start 5           # cut first 5s (default: 3s)
#   ./demo/record-demo.sh --no-trim                # keep full recording
#   ./demo/record-demo.sh --skip-build             # skip cargo build
#   ./demo/record-demo.sh --skip-video             # run scenario without recording
#   ./demo/record-demo.sh --skip-narration         # run without TTS narration
#   ./demo/record-demo.sh --app-running            # force skip build+launch
#   ./demo/record-demo.sh --dry-run                # just pre-generate TTS, don't record
#   ./demo/record-demo.sh --reuse-audio            # reuse TTS clips from the shared cache (skip regen)
#   ./demo/record-demo.sh --seed 42                # pin LLM sampling seed for reproducible runs
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
SKIP_VIDEO=0
REUSE_AUDIO=0
DEMO_SEED=""
SCENARIO="intro"
TRIM_START=0        # seconds to cut from the beginning (0 = no trim)

for arg in "$@"; do
    case "$arg" in
        --skip-build)      SKIP_BUILD=1 ;;
        --skip-video)      SKIP_VIDEO=1 ;;
        --skip-narration)  NO_TTS=1 ;;
        --no-tts)          NO_TTS=1 ;;
        --no-subtitles)    NO_SUBTITLES=1 ;;
        --app-running)     APP_RUNNING=1 ;;
        --dry-run)         DRY_RUN=1 ;;
        --reuse-audio)     REUSE_AUDIO=1 ;;
        --trim-start)      :  ;;  # value handled below
        --trim-start=*)    TRIM_START="${arg#*=}" ;;
        --no-trim)         TRIM_START=0 ;;
        --scenario)        :  ;;  # value handled below
        --scenario=*)      SCENARIO="${arg#*=}" ;;
        --seed)            :  ;;  # value handled below
        --seed=*)          DEMO_SEED="${arg#*=}" ;;
        -h|--help)
            head -18 "$0" | tail -15
            exit 0
            ;;
        *)
            # Check if previous arg was --scenario, --trim-start, or --seed
            if [ "${prev_arg:-}" = "--trim-start" ]; then
                TRIM_START="$arg"
            elif [ "${prev_arg:-}" = "--scenario" ]; then
                SCENARIO="$arg"
            elif [ "${prev_arg:-}" = "--seed" ]; then
                DEMO_SEED="$arg"
            else
                echo "Unknown flag: $arg" >&2; exit 1
            fi
            ;;
    esac
    prev_arg="$arg"
done

# Seed — pass through to the app (llama-server) and to bash so scenario-level
# randomness stays in sync with LLM sampling. Scenarios can read $DEMO_SEED.
if [ -n "$DEMO_SEED" ]; then
    export IMPULSE_LLM_SEED="$DEMO_SEED"
    export DEMO_SEED
    RANDOM="$DEMO_SEED"
    echo "Seed: $DEMO_SEED (IMPULSE_LLM_SEED + bash RANDOM)"
fi

SCENARIO_FILE="$DEMO_DIR/scenarios/${SCENARIO}.sh"
if [ ! -f "$SCENARIO_FILE" ]; then
    echo "ERROR: Scenario not found: $SCENARIO_FILE" >&2
    echo "Available scenarios:" >&2
    ls "$DEMO_DIR/scenarios/"*.sh 2>/dev/null | sed 's|.*/||; s|\.sh$||' | sed 's/^/  /' >&2
    exit 1
fi

# Auto-detect running app — skip build+launch if API is reachable
if [ "$APP_RUNNING" -eq 0 ] && curl -sf "http://127.0.0.1:8765/api/state" >/dev/null 2>&1; then
    echo "App already running (API reachable) — skipping build+launch"
    APP_RUNNING=1
    SKIP_BUILD=1
fi

echo "Scenario: $SCENARIO ($SCENARIO_FILE)"
echo "Trim start: ${TRIM_START}s"

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

check_dep python3     "sudo apt install python3"
check_dep bc          "sudo apt install bc"
if [ "$SKIP_VIDEO" -eq 0 ]; then
    check_dep ffmpeg      "sudo apt install ffmpeg"
    check_dep xwininfo    "sudo apt install x11-utils"
    check_dep pw-record   "sudo apt install pipewire (should already be there)"
fi
if [ "$NO_TTS" -eq 0 ]; then
    check_dep paplay "sudo apt install pulseaudio-utils"
fi

# Verify xdo.py works (only needed for video mode)
if [ "$SKIP_VIDEO" -eq 0 ]; then
    if ! python3 "$DEMO_DIR/xdo.py" getactivewindow >/dev/null 2>&1; then
        echo "ERROR: xdo.py failed. Is libxdo3 installed? (sudo apt install libxdo3)" >&2
        exit 1
    fi
fi

# ─── Batch output directory ──────────────────────────────────────────────────
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
VERSION=$(grep '^version' "$PROJECT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
BATCH_DIR="$OUTPUT_DIR/${TIMESTAMP}"
BASENAME="v${VERSION}-${SCENARIO}"
mkdir -p "$BATCH_DIR"
# TTS clip cache location — per-batch by default so each recording owns its
# clips. --reuse-audio points at the shared persistent cache so unchanged
# narration lines from prior runs skip re-synthesis (tts_generate already
# caches by id.wav filename; changed text produces a new id and will regen).
if [ "$REUSE_AUDIO" -eq 1 ]; then
    export TTS_DIR="$DEMO_DIR/tts_cache"
    echo "  [--reuse-audio] Using shared TTS cache: $TTS_DIR"
else
    export TTS_DIR="$BATCH_DIR/tts"
fi
mkdir -p "$TTS_DIR"

# ─── Log file ────────────────────────────────────────────────────────────────
LOGFILE="$BATCH_DIR/${BASENAME}.log"
exec > >(tee -a "$LOGFILE") 2>&1
echo "=== Log: $LOGFILE ==="
echo "  Date: $(date)"
echo "  Scenario: $SCENARIO"
echo "  Flags: skip_build=$SKIP_BUILD no_tts=$NO_TTS skip_video=$SKIP_VIDEO"
echo ""

# ─── Build the app ───────────────────────────────────────────────────────────
#
# Ordering rationale: Build → launch app (background) → pre-gen TTS.
# Launching the app before TTS generation lets llama-server load Gemma 4 onto
# the GPU while NeuTTS is still busy synthesising clips on CPU + GPU.  By the
# time the scenario runs and adds an agent, the model pool already has a warm
# server — no 60-120s cold-start stall.

if [ "$SKIP_BUILD" -eq 0 ] && [ "$APP_RUNNING" -eq 0 ]; then
    echo "[1/6] Building Impulse Instruct..."
    cd "$PROJECT_DIR"
    cargo build --release 2>&1 | tail -3
    echo "  Build complete."
else
    echo "[1/6] Build skipped."
fi

# ─── Cleanup trap ─────────────────────────────────────────────────────────────

APP_PID=""
FFMPEG_PID=""
PW_RECORD_PID=""

cleanup() {
    echo ""
    echo "Cleaning up..."
    if [ -n "$FFMPEG_PID" ]; then
        kill -INT "$FFMPEG_PID" 2>/dev/null
        sleep 1
        kill -9 -"$FFMPEG_PID" 2>/dev/null || kill -9 "$FFMPEG_PID" 2>/dev/null
        wait "$FFMPEG_PID" 2>/dev/null
    fi
    if [ -n "$PW_RECORD_PID" ]; then
        kill -INT "$PW_RECORD_PID" 2>/dev/null
        sleep 1
        kill -9 "$PW_RECORD_PID" 2>/dev/null
        wait "$PW_RECORD_PID" 2>/dev/null
    fi
    if [ -n "$APP_PID" ]; then
        # SIGTERM first, give the app a moment to run Drop handlers
        # (LlamaServerBackend's Drop SIGKILLs its child llama-server).
        kill -TERM "$APP_PID" 2>/dev/null
        for _ in 1 2 3 4 5 6; do
            kill -0 "$APP_PID" 2>/dev/null || break
            sleep 0.5
        done
        kill -KILL "$APP_PID" 2>/dev/null
        wait "$APP_PID" 2>/dev/null
    fi
    # Drop doesn't run on SIGKILL, and even SIGTERM can race — sweep
    # any orphaned llama-server spawned by our app. Match on --model so
    # we don't touch unrelated llama-server instances the user may run.
    pkill -KILL -f 'llama-server .* --model' 2>/dev/null
    stop_neutts_server 2>/dev/null
    # Clean up virtual recording sink
    destroy_recording_sink 2>/dev/null
    rm -f "$NARRATION_LIST"
    echo "Done."
}
trap cleanup EXIT

# ─── Launch the app (background — llama-server warms while TTS runs) ─────────

if [ "$DRY_RUN" -eq 1 ]; then
    # Dry run: skip app launch entirely, just pre-gen TTS and exit.
    echo "[2/6] App launch skipped (--dry-run)."
elif [ "$APP_RUNNING" -eq 0 ]; then
    echo ""
    echo "[2/6] Launching app with --skip-wizard..."
    cd "$PROJECT_DIR"
    # Remove stale session so the app starts with a clean rack
    rm -f session.json
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
    # --log trace: full diagnostics captured in the log file (JSON output, etc.)
    if [ -n "$MODEL_PATH" ]; then
        ./target/release/impulse-instruct --skip-wizard --fresh-session --model "$MODEL_PATH" --log trace &
    else
        ./target/release/impulse-instruct --skip-wizard --fresh-session --mock --log trace &
    fi
    APP_PID=$!
    echo "  App launched (pid=$APP_PID) — llama-server will warm up during TTS pre-gen."
else
    echo "[2/6] Using already-running app."
fi

# ─── Pre-generate TTS clips (concurrent with llama-server warm-up) ───────────

echo ""
echo "[3/6] Pre-generating TTS clips..."
if [ "$NO_TTS" -eq 0 ]; then
    start_neutts_server || { echo "  Falling back to no-TTS mode"; NO_TTS=1; }
fi
if [ "$NO_TTS" -eq 0 ]; then
    clip_id=""
    scene_num=0
    tts_ok=0
    tts_fail=0
    tts_failed_ids=""
    _gen_one() {
        # $1=id, $2=text; reports and tracks success/failure.
        if tts_generate "$1" "$2" >/dev/null; then
            tts_ok=$((tts_ok + 1))
            echo "  [$tts_ok] Generated: $1"
        else
            tts_fail=$((tts_fail + 1))
            tts_failed_ids="${tts_failed_ids}$1\n"
        fi
    }
    while IFS= read -r line; do
        # narrate "id" \        (two-line form: text on next line)
        if [[ "$line" =~ narrate[[:space:]]+(--wait[[:space:]]+)?\"([^\"]+)\"[[:space:]]+\\?$ ]]; then
            clip_id="${BASH_REMATCH[2]}"
        elif [[ "$line" =~ ^[[:space:]]+\"(.+)\"$ ]] && [ -n "${clip_id:-}" ]; then
            _gen_one "$clip_id" "${BASH_REMATCH[1]}"
            clip_id=""
        # narrate "id" "text"
        elif [[ "$line" =~ narrate[[:space:]]+(--wait[[:space:]]+)?\"([^\"]+)\"[[:space:]]+\"(.+)\" ]]; then
            _gen_one "${BASH_REMATCH[2]}" "${BASH_REMATCH[3]}"
            clip_id=""
        # scene "name"  (track scene counter for say auto-IDs)
        elif [[ "$line" =~ ^[[:space:]]*scene[[:space:]]+\".*\" ]]; then
            scene_num=$((scene_num + 1))
        # say "text"  (expands to narrate --wait "auto_NNN_<slug>" "text")
        elif [[ "$line" =~ ^[[:space:]]*say[[:space:]]+\"(.+)\"[[:space:]]*$ ]]; then
            text="${BASH_REMATCH[1]}"
            slug=$(echo "$text" | tr ' ' '_' | tr -cd 'a-zA-Z0-9_' | head -c 30)
            auto_id=$(printf 'auto_%03d_%s' "$scene_num" "$slug")
            _gen_one "$auto_id" "$text"
        fi
    done < "$SCENARIO_FILE"
    echo "  TTS clips cached in $TTS_DIR ($tts_ok ok, $tts_fail failed)"
    if [ "$tts_fail" -gt 0 ]; then
        echo "  Missing clips (all retries exhausted):"
        printf "%b" "$tts_failed_ids" | sed 's/^/    /'
    fi
    # IMPORTANT: don't stop the NeuTTS server here.  The in-app TTS
    # module (used for runtime mc_line synthesis when an MC agent is
    # spawned during the scenario) shares the same Python server on
    # port 8770 — stopping it would make scenario-time MC lines fail
    # silently.  Memory is fine: NeuTTS Air is ~800 MB (Q8) and Gemma
    # ~5 GB, comfortably under most GPU budgets.  The cleanup trap
    # stops the server after the scenario finishes.

    # Pre-generate SRT from scenario + actual clip durations. Timings are
    # approximate (API/UI calls take variable time at runtime) but the SRT
    # contains the full intended text with reading-time-friendly durations,
    # independent of runtime execution or TTS truncation.
    PREGEN_SRT="$BATCH_DIR/${BASENAME}.pregen.srt"
    pregenerate_srt "$SCENARIO_FILE" "$PREGEN_SRT" >/dev/null || PREGEN_SRT=""
else
    echo "  Skipped (--no-tts)"
    # Override narrate to still log subtitles but skip audio playback
    narrate() {
        local blocking=0
        if [ "$1" = "--wait" ]; then shift; fi
        local id="$1" text="$2"
        local start_sec
        start_sec=$(demo_elapsed 2>/dev/null || echo "0")
        echo "${start_sec}|2.0|${text}" >> "$NARRATION_LIST"
    }
    wait_narration() { :; }
fi

if [ "$DRY_RUN" -eq 1 ]; then
    echo ""
    echo "Dry run complete. TTS clips are in $TTS_DIR"
    exit 0
fi

# ─── Wait for app API to be reachable ────────────────────────────────────────
# App was launched before TTS pre-gen; by now it's almost certainly up, but
# keep a short safety wait in case TTS finished very fast.
if [ "$APP_RUNNING" -eq 0 ]; then
    wait_for_api 30
else
    wait_for_api 5
fi

# Guarantee a blank slate — even when attaching to a running app that may
# have LFO on, agents loaded, sequencer running, etc.  --fresh-session on
# launch only handles fresh starts; this covers the reuse case too.
# Safe to do post-warmup: api_state_reset only mutates AppState, it doesn't
# touch the LLM thread's LlamaServerPool, so the pre-acquired warm server
# for --model stays alive.  The scenario's add_agent will reuse it.
echo "  Resetting app state to defaults..."
api_state_reset
sleep 1

# Wait for llama-server to finish loading the model BEFORE we start the video
# recording.  Otherwise the first seconds of the recording are dead air while
# the scenario's initial ask/wait_for_model blocks the clip timeline.  The
# app's LLM thread pre-acquired the server at startup (concurrent with TTS
# pre-gen), so by now it's usually already warm — wait_for_llm returns
# immediately if llm_initializing is already False.  Safe in mock mode too
# (mock flips llm_initializing=False on startup).
wait_for_llm 300

# ─── Find the app window + start capture ─────────────────────────────────────

if [ "$SKIP_VIDEO" -eq 0 ]; then
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
    fi

    # Ensure even dimensions (required by h264_nvenc)
    GRAB_W=$(( GRAB_W / 2 * 2 ))
    GRAB_H=$(( GRAB_H / 2 * 2 ))

    # ─── Start ISOLATED per-app audio capture via virtual PipeWire sink ──
    #
    # Strategy: create a dedicated virtual sink, capture from its monitor.
    # The app gets routed to this sink after sequencer starts (via api_play
    # hook). This guarantees NO browser/system audio leaks into the recording.

    APP_AUDIO="$BATCH_DIR/${BASENAME}-audio.wav"
    RAW_VIDEO="$BATCH_DIR/${BASENAME}-raw.mkv"
    NARRATION_AUDIO="$BATCH_DIR/${BASENAME}-narration.wav"
    RECORDING_SINK_ID=""

    # Try to create a virtual recording sink
    RECORDING_SINK_ID=$(create_recording_sink)
    if [ -n "$RECORDING_SINK_ID" ]; then
        echo "  Recording sink: node $RECORDING_SINK_ID (isolated virtual sink)"
        # Record from the sink's monitor
        pw-record --target "$RECORDING_SINK_ID" "$APP_AUDIO" &
        PW_RECORD_PID=$!
    else
        echo "  WARN: Could not create virtual sink — trying direct capture"
        APP_PW_NODE=$(find_app_pw_node)
        if [ -n "$APP_PW_NODE" ]; then
            echo "  App audio: PipeWire stream $APP_PW_NODE"
            pw-record --target "$APP_PW_NODE" "$APP_AUDIO" &
            PW_RECORD_PID=$!
        else
            echo "  App audio: not available yet, will retry after play"
        fi
    fi

    # ─── Start screen recording ──────────────────────────────────────────

    echo ""
    echo "[5/6] Recording demo..."
    echo "  Capturing region ${GRAB_W}x${GRAB_H} at +${GRAB_X}+${GRAB_Y}"
    echo "  Raw output: $RAW_VIDEO"

    # Region capture via x11grab — reliable across X11 and XWayland.
    # -window_id looked nicer on paper but silently degrades to full-screen
    # capture when xcb can't map the wmctrl-reported window handle (common
    # on Wayland compositors running XWayland), which was the regression.
    # The GRAB_X/Y/W/H rect is computed from wmctrl geometry and already
    # even-sized, so we capture the window's current position directly.
    # Caveat: if the window is moved mid-recording the capture stays on the
    # original rect. Our automation never moves the window after the initial
    # wmctrl focus/raise, so this is fine.
    #
    # setsid puts ffmpeg in its own process group so kill -9 propagates.
    # -t 600 is a 10-minute safety net in case SIGINT fails.
    setsid ffmpeg -y \
        -f x11grab \
        -video_size "${GRAB_W}x${GRAB_H}" \
        -framerate 30 \
        -draw_mouse 0 \
        -i "${DISPLAY}+${GRAB_X},${GRAB_Y}" \
        -t 600 \
        -pix_fmt yuv420p \
        -c:v h264_nvenc -preset p4 -cq 18 \
        -an \
        "$RAW_VIDEO" \
        </dev/null >/dev/null 2>&1 &
    FFMPEG_PID=$!

    # Let ffmpeg settle before starting the scenario clock. The 1-second
    # wait + the DEMO_START_NS timestamp immediately after it pin the
    # scenario's t=0 to approximately when ffmpeg actually started
    # recording frames — narration log timestamps are then video-relative
    # and SRT subtitles stay in sync with the recorded audio.
    sleep 1
else
    echo "[4/6] Skipping capture (--skip-video)"
    echo "[5/6] Running scenario without recording..."
fi

# Clear narration log
rm -f "$NARRATION_LIST"

# Record start time (nanoseconds) — captured right after ffmpeg settles
# so narrate() timestamps are video-relative.
export DEMO_START_NS
DEMO_START_NS=$(date +%s%N)

echo ""
echo "  Running scenario: $SCENARIO"
echo ""

# ─── Run the demo scenario ───────────────────────────────────────────────────

# Hook: after api_play, try to start per-app capture if we don't have it yet
if [ "$SKIP_VIDEO" -eq 0 ]; then
    _app_routed=0
    api_play() {
        curl -sf -X POST "$API/api/sequencer/play" >/dev/null 2>&1
        # Route app to our recording sink on first play
        if [ "$_app_routed" -eq 0 ]; then
            sleep 1  # give cpal time to create the PW stream
            for _try in 1 2 3 4 5; do
                APP_PW_NODE=$(find_app_pw_node)
                if [ -n "$APP_PW_NODE" ]; then
                    if [ -n "${RECORDING_SINK_ID:-}" ]; then
                        route_app_to_sink "$APP_PW_NODE" "$RECORDING_SINK_ID"
                        echo "  [auto] Routed app stream $APP_PW_NODE → recording sink $RECORDING_SINK_ID"
                    elif [ -z "${PW_RECORD_PID:-}" ]; then
                        echo "  [auto] Direct capture from stream $APP_PW_NODE"
                        pw-record --target "$APP_PW_NODE" "$APP_AUDIO" &
                        PW_RECORD_PID=$!
                    fi
                    _app_routed=1
                    break
                fi
                sleep 1
            done
            [ "$_app_routed" -eq 0 ] && echo "  [auto] Could not find app stream after 5 retries"
        fi
    }
fi

# Disable ALL strict modes during the scenario — API calls, TTS playback,
# and bc/ffprobe can return non-zero or reference empty variables.
set +eu
set +o pipefail
trap 'echo "  !! FAILED at line $LINENO: $BASH_COMMAND (exit $?)" >&2' ERR
source "$SCENARIO_FILE"
SCENARIO_EXIT=$?
trap - ERR
# Stay in relaxed mode — kills, waits, ffprobe all return non-zero legitimately

if [ "$SCENARIO_EXIT" -ne 0 ]; then
    echo "  WARNING: Scenario exited with code $SCENARIO_EXIT"
fi

# Wait for any narration audio playback to finish (NOT bare `wait` which
# would also block on the tee subprocess from exec)
sleep 2

echo ""
echo "  Scenario complete."

if [ "$SKIP_VIDEO" -eq 0 ]; then
    echo "  Stopping recording..."

    # ─── Stop recording ──────────────────────────────────────────────────

    sleep 1

    # Stop screen recording — try graceful 'q' via proc, then SIGKILL group
    kill -INT "$FFMPEG_PID" 2>/dev/null
    sleep 2
    # Kill entire process group (setsid gives ffmpeg its own)
    kill -9 -"$FFMPEG_PID" 2>/dev/null || kill -9 "$FFMPEG_PID" 2>/dev/null
    wait "$FFMPEG_PID" 2>/dev/null || true
    FFMPEG_PID=""

    # Stop audio recording
    if [ -n "$PW_RECORD_PID" ]; then
        kill -INT "$PW_RECORD_PID" 2>/dev/null
        sleep 1
        kill "$PW_RECORD_PID" 2>/dev/null
        wait "$PW_RECORD_PID" 2>/dev/null || true
        PW_RECORD_PID=""
    fi

    echo "  Raw video: $RAW_VIDEO"
    [ -f "$APP_AUDIO" ] && echo "  App audio: $APP_AUDIO"
fi

if [ "$SKIP_VIDEO" -eq 0 ]; then
    # ─── Post-process: merge video + audio + subtitles ────────────────────

    echo ""
    echo "[6/6] Post-processing..."

    AUDIO_COUNT=0
    if [ -f "$APP_AUDIO" ] && [ -s "$APP_AUDIO" ]; then
        AUDIO_COUNT=1
        echo "  App audio: $APP_AUDIO ($(du -h "$APP_AUDIO" | cut -f1))"
    else
        echo "  App audio: not captured"
    fi
    echo "  Raw video: $(du -h "$RAW_VIDEO" 2>/dev/null | cut -f1)"

    # Base name for output variants
    BASE="${BATCH_DIR}/${BASENAME}"

    # Prefer RUNTIME-timestamped subtitles (from narrate() logging actual
    # start_sec when each clip plays) — these stay in sync with what's
    # actually in the recording. The scenario's API/inference/UI calls have
    # variable runtime and the pre-gen SRT's estimated timings drift.
    # Pre-generated SRT is only used as a fallback when no runtime log
    # exists (e.g. recording aborted before the scenario finished).
    SRT_FILE=""
    if [ -f "$NARRATION_LIST" ] && [ -s "$NARRATION_LIST" ]; then
        SRT_NAME="${BASE}.srt" generate_srt >/dev/null
        SRT_FILE="${BASE}.srt"
        # Shift SRT timestamps back by TRIM_START so they line up with the
        # trimmed video timeline (ffmpeg's -ss before -i resets output PTS
        # to 0 but doesn't adjust the external subtitles file).
        if [ "$TRIM_START" -gt 0 ] 2>/dev/null; then
            srt_shift_seconds "$SRT_FILE" "$TRIM_START" || true
        fi
        echo "  Subtitles (runtime): $SRT_FILE"
    elif [ -n "${PREGEN_SRT:-}" ] && [ -s "${PREGEN_SRT:-/dev/null}" ]; then
        cp "$PREGEN_SRT" "${BASE}.srt"
        SRT_FILE="${BASE}.srt"
        if [ "$TRIM_START" -gt 0 ] 2>/dev/null; then
            srt_shift_seconds "$SRT_FILE" "$TRIM_START" || true
        fi
        echo "  Subtitles (pre-generated fallback): $SRT_FILE"
    else
        echo "  Subtitles: none (no narration entries)"
    fi
    FINAL_VIDEO="${BASE}.mp4"
    FINAL_VIDEO_SUBS="${BASE}-subs.mp4"

    echo "  Encoding videos (this may take a minute)..."
    [ "$TRIM_START" -gt 0 ] 2>/dev/null && echo "  Trimming first ${TRIM_START}s"

    # ── Trim + encode args ────────────────────────────────────────────
    TRIM_ARGS=""
    [ "$TRIM_START" -gt 0 ] 2>/dev/null && TRIM_ARGS="-ss $TRIM_START"

    ENCODE_AUDIO=""
    if [ "$AUDIO_COUNT" -gt 0 ]; then
        ENCODE_AUDIO="$TRIM_ARGS -i $APP_AUDIO -c:a aac -b:a 192k -map 0:v:0 -map 1:a:0 -shortest"
    fi

    # ── Video without embedded subtitles (for YouTube — upload SRT separately)
    echo -n "  Encoding clean video..."
    ffmpeg -y \
        $TRIM_ARGS -i "$RAW_VIDEO" \
        $ENCODE_AUDIO \
        -sws_flags "lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
        -c:v h264_nvenc -preset p4 -cq 22 \
        "$FINAL_VIDEO" \
        </dev/null 2>/dev/null || \
    ffmpeg -y \
        $TRIM_ARGS -i "$RAW_VIDEO" \
        $ENCODE_AUDIO \
        -sws_flags "lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
        -c:v libx264 -preset fast -crf 23 \
        "$FINAL_VIDEO" \
        </dev/null 2>/dev/null || true
    [ -f "$FINAL_VIDEO" ] && echo " OK" || echo " FAIL"

    # ── Video with burned-in subtitles (for Telegram, social media)
    if [ -n "$SRT_FILE" ] && [ -f "$SRT_FILE" ]; then
        echo -n "  Encoding subtitled video..."
        ffmpeg -y \
            $TRIM_ARGS -i "$RAW_VIDEO" \
            $ENCODE_AUDIO \
            -sws_flags "lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
            -vf "subtitles=${SRT_FILE}:force_style='FontSize=22,FontName=monospace,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,Outline=2,MarginV=40'" \
            -c:v h264_nvenc -preset p4 -cq 22 \
            "$FINAL_VIDEO_SUBS" \
            </dev/null 2>/dev/null || \
        ffmpeg -y \
            $TRIM_ARGS -i "$RAW_VIDEO" \
            $ENCODE_AUDIO \
            -sws_flags "lanczos+accurate_rnd+full_chroma_int+full_chroma_inp" \
            -vf "subtitles=${SRT_FILE}:force_style='FontSize=22,FontName=monospace'" \
            -c:v libx264 -preset fast -crf 23 \
            "$FINAL_VIDEO_SUBS" \
            </dev/null 2>/dev/null || true
        [ -f "$FINAL_VIDEO_SUBS" ] && echo " OK" || echo " FAIL"
    fi

    echo ""
    echo "=== Demo recording complete ==="
    echo "  Clean video: $FINAL_VIDEO"
    ls -lh "$FINAL_VIDEO" 2>/dev/null | awk '{print "  Size:       "$5}'
    if [ -f "$FINAL_VIDEO_SUBS" ]; then
        echo "  With subs:   $FINAL_VIDEO_SUBS"
        ls -lh "$FINAL_VIDEO_SUBS" 2>/dev/null | awk '{print "  Size:       "$5}'
    fi
    if [ -n "$SRT_FILE" ] && [ -f "$SRT_FILE" ]; then
        echo "  SRT file:    $SRT_FILE"
    fi
    ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 \
        "$FINAL_VIDEO" 2>/dev/null | awk '{printf "  Duration:    %.1fs\n", $1}'
else
    echo ""
    echo "=== Scenario run complete (no video) ==="
fi
echo ""
