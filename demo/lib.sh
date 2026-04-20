#!/usr/bin/env bash
# ─── demo/lib.sh ── shared helpers for demo recording ────────────────────────
# Sourced by record-demo.sh and scenario.sh. Not meant to run standalone.

API="http://127.0.0.1:8765"
DEMO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TTS_DIR="${TTS_DIR:-$DEMO_DIR/tts_cache}"
NARRATION_LIST="${NARRATION_LIST:-$DEMO_DIR/.narration_playlist}"
XDO="python3 $DEMO_DIR/xdo.py"

# TTS settings — NeuTTS Air for demo narration
# Setup: ./scripts/setup-neutts.sh
PROJECT_DIR="${DEMO_DIR}/.."
NEUTTS_PORT="${NEUTTS_PORT:-8770}"
NEUTTS_URL="http://127.0.0.1:${NEUTTS_PORT}"
NEUTTS_VENV="${PROJECT_DIR}/.neutts-venv"
NEUTTS_REF_AUDIO="${NEUTTS_REF_AUDIO:-${PROJECT_DIR}/voices/narrator.wav}"
NEUTTS_REF_TEXT="${NEUTTS_REF_TEXT:-${PROJECT_DIR}/voices/narrator.txt}"
NEUTTS_PID=""

# Window ID — set by record-demo.sh after finding the app
APP_WINDOW_ID="${APP_WINDOW_ID:-0}"

# ─── API helpers ──────────────────────────────────────────────────────────────

api_play()  { curl -sf -X POST "$API/api/sequencer/play"  >/dev/null 2>&1 || true; }
api_stop()  { curl -sf -X POST "$API/api/sequencer/stop"  >/dev/null 2>&1 || true; }
api_state() { curl -sf "$API/api/state"; }

api_params() {
    # Usage: api_params '{"bass": {"cutoff": 0.8}}'
    curl -sf -X POST "$API/api/params" \
        -H "Content-Type: application/json" \
        -d "{\"params\": $1}" >/dev/null 2>&1 || true
}

api_prompt() {
    # Usage: api_prompt "make it acid"
    #        api_prompt "make it acid" "BASS"   (target specific agent)
    local prompt="$1"
    local agent="${2:-}"
    local agent_json=""
    if [ -n "$agent" ]; then
        agent_json=", \"agent\": \"$agent\""
    fi
    curl -sf -X POST "$API/api/prompt" \
        -H "Content-Type: application/json" \
        -d "{\"prompt\": \"$prompt\" $agent_json}" >/dev/null 2>&1 || true
}

api_lock() {
    local paths=""
    for p in "$@"; do
        [ -n "$paths" ] && paths="$paths,"
        paths="$paths\"$p\""
    done
    curl -sf -X POST "$API/api/lock" \
        -H "Content-Type: application/json" \
        -d "{\"paths\": [$paths]}" >/dev/null 2>&1 || true
}

api_unlock() {
    local paths=""
    for p in "$@"; do
        [ -n "$paths" ] && paths="$paths,"
        paths="$paths\"$p\""
    done
    curl -sf -X POST "$API/api/unlock" \
        -H "Content-Type: application/json" \
        -d "{\"paths\": [$paths]}" >/dev/null 2>&1 || true
}

api_scroll() {
    # Scroll the UI to a specific zone or module.
    # Usage: api_scroll "voice"  |  api_scroll "fx"  |  api_scroll "global"
    curl -sf -X POST "$API/api/scroll" \
        -H "Content-Type: application/json" \
        -d "{\"target\": \"$1\"}" >/dev/null 2>&1 || true
}

api_focus() {
    # Scroll to a zone/module AND collapse others for focus mode.
    # Usage: api_focus "bass"  |  api_focus "808"
    curl -sf -X POST "$API/api/scroll" \
        -H "Content-Type: application/json" \
        -d "{\"target\": \"$1\", \"collapse_others\": true}" >/dev/null 2>&1 || true
}

api_collapse() {
    # Collapse/expand zones.
    # Usage: api_collapse "all" | api_collapse "none" | api_collapse "voice"
    curl -sf -X POST "$API/api/rack/collapse" \
        -H "Content-Type: application/json" \
        -d "{\"action\": \"$1\"}" >/dev/null 2>&1 || true
}

api_flip_back() {
    # Show the back panel (cables + ports)
    curl -sf -X POST "$API/api/flip" \
        -H "Content-Type: application/json" \
        -d '{"show_back": true}' >/dev/null 2>&1 || true
}

api_flip_front() {
    # Show the front panel (knobs + controls)
    curl -sf -X POST "$API/api/flip" \
        -H "Content-Type: application/json" \
        -d '{"show_back": false}' >/dev/null 2>&1 || true
}

api_rack_reset() {
    # Reset rack to minimal: sequencer + master + LLM console only
    curl -sf -X POST "$API/api/rack/reset" >/dev/null 2>&1 || true
}

api_state_reset() {
    # Full AppState wipe — everything back to defaults (Empty rack preset),
    # preserving only the currently-loaded model path.  Guarantees a blank
    # slate even when attaching to an already-running app (LFO off, seq
    # stopped, no active style, no leftover agents, all params default).
    curl -sf -X POST "$API/api/state/reset" >/dev/null 2>&1 || true
}

api_set_style() {
    # Set the global active style and propagate to all agents.
    # Usage: api_set_style drum_and_bass
    #        api_set_style ""              # clear active style
    curl -sf -X POST "$API/api/style" \
        -H "Content-Type: application/json" \
        -d "{\"id\": \"$1\"}" >/dev/null 2>&1 || true
}

api_rack_add() {
    # Add a module to the rack. Returns JSON with "id" field.
    # Usage: id=$(api_rack_add "808")
    curl -sf -X POST "$API/api/rack/add" \
        -H "Content-Type: application/json" \
        -d "{\"kind\": \"$1\"}" 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo ""
}

api_rack_agent() {
    # Add an LLM agent. Returns JSON with "id" field.
    # Usage: id=$(api_rack_agent "BASS" '["bass"]' "gemma")
    local persona="$1"
    local scope="${2:-[]}"
    local model="${3:-}"
    local model_json=""
    if [ -n "$model" ]; then
        model_json=", \"model\": \"$model\""
    fi
    curl -sf -X POST "$API/api/rack/agent" \
        -H "Content-Type: application/json" \
        -d "{\"persona\": \"$persona\", \"scope\": $scope $model_json}" 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo ""
}

api_rack_cable() {
    # Connect two modules. Default: control cable.
    # Usage: api_rack_cable $from_id $to_id [control|audio]
    local kind="${3:-control}"
    curl -sf -X POST "$API/api/rack/cable" \
        -H "Content-Type: application/json" \
        -d "{\"from\": $1, \"to\": $2, \"kind\": \"$kind\"}" >/dev/null 2>&1 || true
}

api_rack_remove() {
    # Remove a module by ID
    curl -sf -X POST "$API/api/rack/remove" \
        -H "Content-Type: application/json" \
        -d "{\"id\": $1}" >/dev/null 2>&1 || true
}

api_preset() {
    # Apply an agent preset: Solo, Duo, Swarm, Band, Voices, Lite
    # Usage: api_preset "Band"
    curl -sf -X POST "$API/api/preset" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"$1\"}" >/dev/null 2>&1 || true
}

wait_for_llm() {
    # Wait until the LLM is no longer initializing (llama-server is ready).
    # Usage: wait_for_llm [max_seconds]
    local max_wait="${1:-120}"
    local waited=0
    echo "  Waiting for LLM to initialize (llama-server startup)..."
    while true; do
        local init
        init=$(curl -sf "$API/api/state" 2>/dev/null | python3 -c "
import sys, json
try:
    s = json.load(sys.stdin)
    print(s.get('llm', {}).get('llm_initializing', True))
except: print('True')
" 2>/dev/null)
        if [ "$init" = "False" ]; then
            echo "  LLM ready (took ${waited}s)"
            return 0
        fi
        sleep 2
        waited=$((waited + 2))
        if [ "$waited" -ge "$max_wait" ]; then
            echo "  WARNING: LLM still initializing after ${max_wait}s — continuing anyway" >&2
            return 1
        fi
    done
}

# ─── Wait for app to be reachable ─────────────────────────────────────────────

wait_for_api() {
    local max_wait="${1:-30}"
    local waited=0
    echo "  Waiting for API on $API ..."
    while ! curl -sf "$API/api/state" >/dev/null 2>&1; do
        sleep 1
        waited=$((waited + 1))
        if [ "$waited" -ge "$max_wait" ]; then
            echo "  ERROR: API not reachable after ${max_wait}s" >&2
            return 1
        fi
    done
    echo "  API is up (took ${waited}s)"
}

# ─── NeuTTS server lifecycle ──────────────────────────────────────────────────

start_neutts_server() {
    if curl -sf "${NEUTTS_URL}/health" >/dev/null 2>&1; then
        echo "  NeuTTS server already running on port ${NEUTTS_PORT}"
        return 0
    fi
    local python="${NEUTTS_VENV}/bin/python"
    [ -x "$python" ] || python="python3"
    echo "  Starting NeuTTS server (port ${NEUTTS_PORT})..."
    "$python" "${PROJECT_DIR}/scripts/neutts-server.py" --port "$NEUTTS_PORT" &
    NEUTTS_PID=$!
    local waited=0
    while ! curl -sf "${NEUTTS_URL}/health" >/dev/null 2>&1; do
        sleep 1
        waited=$((waited + 1))
        if [ "$waited" -ge 30 ]; then
            echo "  ERROR: NeuTTS server failed to start after 30s" >&2
            return 1
        fi
    done
    echo "  NeuTTS server ready (took ${waited}s, PID: $NEUTTS_PID)"
}

stop_neutts_server() {
    if [ -n "$NEUTTS_PID" ]; then
        kill "$NEUTTS_PID" 2>/dev/null
        wait "$NEUTTS_PID" 2>/dev/null
        NEUTTS_PID=""
        echo "  NeuTTS server stopped"
    fi
}

# ─── TTS helpers ──────────────────────────────────────────────────────────────

# Transform subtitle text into a TTS-friendly form.
#
# NeuTTS sometimes mispronounces bare acronyms (reads "AI" as the word "eye",
# runs "LFO" together as "elfo"), so we spell them out with dots for the
# speech pass.  The subtitle keeps the original short form, since that reads
# more naturally on screen.
#
# Word-boundary aware — only whole-word matches are rewritten, so we don't
# mangle "bass" or "fax" into something weird.
tts_speakable() {
    local text="$1"
    text=$(printf '%s' "$text" | sed -E '
        s/\bLFO\b/L.F.O./g;
        s/\bAI\b/A.I./g;
        s/\bFX\b/effects/g;
        s/\bBPM\b/B.P.M./g;
    ')
    printf '%s' "$text"
}

tts_generate() {
    # Pre-generate a TTS clip using NeuTTS Air server. Returns the wav path.
    # Usage: tts_generate "clip_id" "Text to speak"
    # Retries up to `TTS_MAX_ATTEMPTS` times (default 10). NeuTTS can:
    #   - Stall on the first few warm-up calls
    #   - Occasionally fail on specific phonetic inputs
    #   - Segfault outright, leaving the server dead — we detect that via
    #     `/health` and restart it automatically before the next retry so
    #     we don't waste attempts against a corpse.
    #   - Produce RUNAWAY generations that keep going past the input
    #     text (seen e.g. a 5-word line synthesised as 48 s of audio).
    #     We validate clip length against a generous 3×-reading-time
    #     upper bound and treat an over-long clip as a failed attempt
    #     — otherwise a single bad cached clip blocks a whole demo run.
    local id="$1" text="$2"
    local outfile="$TTS_DIR/${id}.wav"
    local max_attempts="${TTS_MAX_ATTEMPTS:-10}"
    mkdir -p "$TTS_DIR"
    # Upper bound: 3× reading-time estimate, floored at 8 s so short
    # one-word lines (e.g. "Delay.") don't get false-positive rejects
    # from ffprobe rounding or trailing silence.
    local est
    est=$(min_subtitle_dur "$text")
    local max_dur
    max_dur=$(printf '%s * 3\n' "$est" | bc 2>/dev/null)
    # Guard against bc failure / empty max_dur.
    case "$max_dur" in
        ''|*[!0-9.]*) max_dur="30" ;;
    esac
    # Floor at 8.0 s (lines as short as "Ring modulator." read ~1.5 s).
    if [ "$(printf '%s < 8.0\n' "$max_dur" | bc 2>/dev/null)" = "1" ]; then
        max_dur="8.0"
    fi
    if [ -f "$outfile" ] && [ -s "$outfile" ]; then
        local cached_dur
        cached_dur=$(tts_duration "$outfile")
        case "$cached_dur" in
            ''|*[!0-9.]*) cached_dur="0" ;;
        esac
        if [ "$(printf '%s > %s\n' "$cached_dur" "$max_dur" | bc 2>/dev/null)" = "1" ]; then
            echo "  WARN: cached TTS $id is ${cached_dur}s (> ${max_dur}s ceiling) — regenerating" >&2
            rm -f "$outfile"
        else
            echo "$outfile"
            return 0
        fi
    fi
    # Build JSON payload safely via jq (handles quotes, escapes, unicode).
    local json
    if command -v jq >/dev/null 2>&1; then
        json=$(jq -nc \
            --arg text "$text" \
            --arg ref_audio "$NEUTTS_REF_AUDIO" \
            --arg ref_text "$NEUTTS_REF_TEXT" \
            --arg out "$outfile" \
            '{text:$text, ref_audio:$ref_audio, ref_text:$ref_text, out_path:$out}')
    else
        local esc
        esc=$(printf '%s' "$text" | sed 's/\\/\\\\/g; s/"/\\"/g')
        json=$(printf '{"text":"%s","ref_audio":"%s","ref_text":"%s","out_path":"%s"}' \
            "$esc" "$NEUTTS_REF_AUDIO" "$NEUTTS_REF_TEXT" "$outfile")
    fi
    local attempt=0
    local resp_code=""
    while [ "$attempt" -lt "$max_attempts" ]; do
        attempt=$((attempt + 1))
        rm -f "$outfile"
        resp_code=$(curl -s -w "%{http_code}" \
            --max-time 120 \
            -X POST "${NEUTTS_URL}/synthesize" \
            -H "Content-Type: application/json" \
            --data-binary "$json" \
            -o "$outfile" 2>/dev/null || echo "000")
        if [ -s "$outfile" ]; then
            # Length sanity-check: NeuTTS occasionally produces a runaway
            # clip that drones on well past the input text.  A single such
            # clip playing under `say` (blocking) can stall the entire
            # scenario — we saw a 5-word line synthesised as 48 s of
            # audio, which stalled the demo before `play` was ever
            # called.  Treat over-length output as a failed attempt.
            local gen_dur
            gen_dur=$(tts_duration "$outfile")
            case "$gen_dur" in
                ''|*[!0-9.]*) gen_dur="0" ;;
            esac
            if [ "$(printf '%s > %s\n' "$gen_dur" "$max_dur" | bc 2>/dev/null)" = "1" ]; then
                echo "  WARN: NeuTTS attempt $attempt/$max_attempts produced ${gen_dur}s for ${est}s line (> ${max_dur}s ceiling): $text" >&2
                rm -f "$outfile"
            else
                echo "$outfile"
                return 0
            fi
        else
            echo "  WARN: NeuTTS attempt $attempt/$max_attempts failed (HTTP $resp_code) for: $text" >&2
        fi
        # Check if the server is even alive — if not (crashed, segfaulted,
        # OOM'd), restart it before burning more retries.
        if ! curl -sf --max-time 3 "${NEUTTS_URL}/health" >/dev/null 2>&1; then
            echo "  [tts] NeuTTS server unreachable — restarting…" >&2
            NEUTTS_PID=""
            start_neutts_server || {
                echo "  [tts] restart failed, will retry again" >&2
            }
        fi
        # Back off slightly between attempts (1s, then 2s, then 2s cap).
        if [ "$attempt" -lt 3 ]; then
            sleep 1
        else
            sleep 2
        fi
    done
    echo "  ERROR: NeuTTS synth gave up after $max_attempts attempts for: $text" >&2
    rm -f "$outfile"
    echo "$outfile"
    return 1
}

tts_duration() {
    local file="$1"
    ffprobe -v error -show_entries format=duration \
        -of default=noprint_wrappers=1:nokey=1 "$file" 2>/dev/null
}

# ─── Narration + subtitle tracking ───────────────────────────────────────────

demo_elapsed() {
    local now
    now=$(date +%s%N)
    echo "scale=3; ($now - $DEMO_START_NS) / 1000000000" | bc
}

narrate() {
    # Play TTS and log subtitle entry.
    # Usage: narrate "clip_id" "Text that is also the subtitle"
    #        narrate --wait "clip_id" "Text..."   (blocks until done)
    #
    # Missing-WAV handling: if tts_generate couldn't produce audio (server
    # down / retries exhausted / segfault), the scenario still proceeds —
    # a reading-time estimate is written to NARRATION_LIST so the
    # subtitle still appears in the SRT, and `--wait` blocks for that
    # estimate so scenario pacing stays roughly intact.
    local blocking=0
    if [ "$1" = "--wait" ]; then blocking=1; shift; fi

    local id="$1" text="$2"
    local speak_text
    speak_text=$(tts_speakable "$text")
    local wavfile
    wavfile=$(tts_generate "$id" "$speak_text")
    local start_sec
    start_sec=$(demo_elapsed)
    local est
    est=$(min_subtitle_dur "$text")

    if [ ! -s "$wavfile" ]; then
        # No audio available — emit a silent subtitle cue so the SRT
        # still shows the line, and keep the scenario timeline intact.
        echo "${start_sec}|${est}|${text}" >> "$NARRATION_LIST"
        echo "  [narrate] NO WAV: $id (subtitle only, ${est}s) — \"$text\"" >&2
        if [ "$blocking" -eq 1 ]; then
            sleep "$est"
        fi
        return 1
    fi

    local dur
    dur=$(tts_duration "$wavfile")
    # Guard against ffprobe returning empty / garbage.
    case "$dur" in
        ''|*[!0-9.]*) dur="$est" ;;
    esac

    echo "${start_sec}|${dur}|${text}" >> "$NARRATION_LIST"
    echo "  [narrate] $id (${dur}s): \"$text\"" >&2

    # Capture-sink fan-out: when the demo recorder created the
    # `impulse-record` null-sink, we ALSO play the clip to that sink so
    # the narration lands in the recorded mp4 alongside the app audio.
    # Without this, parecord on `impulse-record.monitor` only catches
    # the synth and the voice-over is missing from the final file.  The
    # second paplay still goes to the default sink so the operator
    # hears narration live while recording.  `pactl list short sinks`
    # is the cheap liveness check — the sink exists whether or not we
    # tracked the module id, and the grep is robust to stale indexes.
    local capture_sink=""
    if pactl list short sinks 2>/dev/null | awk '{print $2}' | grep -qx 'impulse-record'; then
        capture_sink="impulse-record"
    fi

    if [ "$blocking" -eq 1 ]; then
        # Synchronous: paplay should block until the stream drains. If it
        # exits non-zero (sink busy, no default sink, etc.), fall back to
        # aplay. Small sleep after playback gives the PipeWire/ALSA stream
        # time to release before the next clip opens a new one — without it
        # rapid consecutive plays can silently drop.
        if [ -n "$capture_sink" ]; then
            paplay --device="$capture_sink" "$wavfile" 2>/dev/null &
            local cap_pid=$!
        fi
        if ! paplay "$wavfile" 2>&1; then
            aplay "$wavfile" 2>&1 || {
                echo "  [narrate] PLAY FAILED: $id — sleeping ${dur}s to keep timing" >&2
                sleep "$dur"
            }
        fi
        # Let the capture-sink copy finish too; same wav, so it should
        # already be done or within a few ms.
        [ -n "${cap_pid:-}" ] && wait "$cap_pid" 2>/dev/null
        sleep 0.15
    else
        if [ -n "$capture_sink" ]; then
            (paplay --device="$capture_sink" "$wavfile" 2>/dev/null) &
        fi
        (paplay "$wavfile" 2>/dev/null || aplay "$wavfile" 2>/dev/null) &
    fi
}

wait_narration() {
    wait
}

# ─── UI interaction helpers (via xdo.py + libxdo3) ────────────────────────────

ui_focus() {
    $XDO windowfocus "$APP_WINDOW_ID" 2>/dev/null
    sleep 0.2
}

ui_type() {
    local text="$1"
    local delay="${2:-30000}"
    ui_focus
    $XDO type "$text" "$delay" 2>/dev/null
}

ui_key() {
    ui_focus
    $XDO key "$1" 2>/dev/null
}

ui_click() {
    local x="$1" y="$2" btn="${3:-1}"
    $XDO mousemove --window "$APP_WINDOW_ID" "$x" "$y" 2>/dev/null
    sleep 0.1
    $XDO click "$btn" 2>/dev/null
}

ui_scroll_down() {
    ui_focus
    $XDO scroll_down "${1:-3}" 2>/dev/null
}

ui_scroll_up() {
    ui_focus
    $XDO scroll_up "${1:-3}" 2>/dev/null
}

ui_drag() {
    local x1="$1" y1="$2" x2="$3" y2="$4" steps="${5:-20}"
    python3 -c "
import sys; sys.path.insert(0, '$DEMO_DIR')
from xdo import Xdo
import time
x = Xdo()
x.mouse_move($x1, $y1, window=$APP_WINDOW_ID)
x.mouse_down(1, window=$APP_WINDOW_ID)
for i in range($steps + 1):
    t = i / $steps
    cx = int($x1 + ($x2 - $x1) * t)
    cy = int($y1 + ($y2 - $y1) * t)
    x.mouse_move(cx, cy, window=$APP_WINDOW_ID)
    time.sleep(0.02)
x.mouse_up(1, window=$APP_WINDOW_ID)
" 2>/dev/null
}

# ─── PipeWire per-app audio helpers ───────────────────────────────────────────

find_app_pw_node() {
    # Find the app's PipeWire playback STREAM node by application name.
    # Filters to Stream/Output (playback) nodes only — never matches sinks/monitors.
    pw-cli ls Node 2>/dev/null | awk '
        /^[[:space:]]*id [0-9]+/ { id=$2; gsub(/,/,"",id); is_stream=0; is_impulse=0 }
        /media.class.*Stream\/Output/ { is_stream=1 }
        /application.name.*[Ii]mpulse/ || /node.name.*impulse/ { is_impulse=1 }
        is_stream && is_impulse { print id; exit }
    '
}

# Create an isolated virtual sink for recording.
# Returns the sink node ID. The app is routed to this sink via pw-link.
#
# We use `pactl load-module module-null-sink` (NOT `pw-cli create-node`):
# pw-cli creates the node in the context of its own PipeWire client
# connection, which dies the moment pw-cli exits — the node vanishes
# with it.  `pactl load-module` runs inside the long-lived
# pipewire-pulse service, so the sink persists for the whole recording.
# The module ID is stashed in /tmp so `destroy_recording_sink` can
# unload it cleanly at shutdown.
create_recording_sink() {
    local module_id
    module_id=$(pactl load-module module-null-sink \
        sink_name=impulse-record \
        sink_properties='device.description=ImpulseRecord' \
        2>/dev/null)
    if [ -z "$module_id" ]; then
        return 1
    fi
    echo "$module_id" > /tmp/impulse-record-sink.module
    # Give pipewire-pulse a beat to register the node, then resolve the
    # sink's node ID by sink name.  Both the sink and its paired
    # `impulse-record.monitor` source appear after the module loads; we
    # return the *sink* id (the app's output gets pw-link'd to it), and
    # stash the monitor name separately so the capturer can target it.
    sleep 0.5
    pw-cli ls Node 2>/dev/null | awk '
        /^[[:space:]]*id [0-9]+/ { id=$2; gsub(/,/,"",id) }
        /node.name = "impulse-record"/ { print id; exit }
    '
}


# Route the app's output to our recording sink
route_app_to_sink() {
    local app_node="$1"
    local sink_node="$2"
    # Link app output ports to sink input ports
    pw-link "${app_node}:output_FL" "${sink_node}:input_FL" 2>/dev/null
    pw-link "${app_node}:output_FR" "${sink_node}:input_FR" 2>/dev/null
}

# Destroy the recording sink by unloading the pactl module that created it.
destroy_recording_sink() {
    local module_id=""
    if [ -f /tmp/impulse-record-sink.module ]; then
        module_id=$(cat /tmp/impulse-record-sink.module 2>/dev/null)
        rm -f /tmp/impulse-record-sink.module
    fi
    if [ -n "$module_id" ]; then
        pactl unload-module "$module_id" 2>/dev/null || true
    else
        # Fallback: scan pactl's loaded modules for any impulse-record sink
        # and unload it. Catches stale sinks from a crashed earlier run.
        pactl list short modules 2>/dev/null \
            | awk '/module-null-sink.*impulse-record/ { print $1 }' \
            | while read -r id; do
                pactl unload-module "$id" 2>/dev/null || true
            done
    fi
}

start_pw_capture() {
    local outfile="$1"
    local node_id
    node_id=$(find_app_pw_node)

    if [ -n "$node_id" ]; then
        echo "  Capturing audio from PipeWire node $node_id (isolated)"
        pw-record --target "$node_id" "$outfile" &
        echo $!
    else
        echo "  WARNING: App PipeWire node not found — skipping audio capture" >&2
        echo ""
    fi
}

# ─── Timing helpers ───────────────────────────────────────────────────────────

pause() {
    sleep "$1"
}

# ─── Subtitle generation ─────────────────────────────────────────────────────

generate_srt() {
    local outfile="${SRT_NAME:-${OUTPUT_DIR:-$DEMO_DIR}/demo_subtitles.srt}"
    local idx=0
    # Stretch each subtitle's display window to 1.5× the TTS clip duration
    # so the line stays on screen comfortably longer than the spoken audio.
    # Override with SRT_DISPLAY_FACTOR env var (1.0 = audio-length only).
    local factor="${SRT_DISPLAY_FACTOR:-1.5}"
    # Subtitles were appearing ~1s ahead of the spoken audio; shift the whole
    # SRT later by this many seconds. Override with SRT_OFFSET_SECS.
    local offset="${SRT_OFFSET_SECS:-1.5}"

    if [ ! -f "$NARRATION_LIST" ]; then
        echo "No narration entries found" >&2
        return 1
    fi

    > "$outfile"
    while IFS='|' read -r start_sec dur text; do
        idx=$((idx + 1))
        local display_dur end_sec
        display_dur=$(awk -v d="$dur" -v f="$factor" 'BEGIN { printf "%.3f", d * f }')
        start_sec=$(echo "$start_sec + $offset" | bc)
        end_sec=$(echo "$start_sec + $display_dur" | bc)

        local start_ts end_ts
        start_ts=$(secs_to_srt "$start_sec")
        end_ts=$(secs_to_srt "$end_sec")

        echo "$idx"          >> "$outfile"
        echo "$start_ts --> $end_ts" >> "$outfile"
        echo "$text"         >> "$outfile"
        echo ""              >> "$outfile"
    done < "$NARRATION_LIST"

    echo "$outfile"
}

min_subtitle_dur() {
    # Rough reading-time estimate for a subtitle: 0.35s/word + 0.5s baseline,
    # clamped to [1.5, 6.0] seconds. Used so subtitles remain on-screen for a
    # readable time even when the corresponding TTS clip is shorter (e.g.
    # truncated or fast-spoken).
    local text="$1"
    local words
    words=$(echo "$text" | wc -w)
    local dur
    dur=$(awk -v w="$words" 'BEGIN { d = w * 0.35 + 0.5; if (d < 1.5) d = 1.5; if (d > 6.0) d = 6.0; printf "%.3f", d }')
    echo "$dur"
}

pregenerate_srt() {
    # Parse a scenario file and emit an SRT based on narrate + pause/sleep
    # timings. Uses actual clip duration (if the WAV exists in $TTS_DIR),
    # taking max(clip_dur, reading_time). Non-blocking narrate emits a
    # subtitle at the current cursor time without advancing it; narrate --wait
    # and pause/sleep advance the cursor. Timing is approximate (API calls
    # take variable time) but close enough for subtitles.
    #
    # Usage: pregenerate_srt SCENARIO_FILE OUTFILE
    local scenario="$1" outfile="$2"
    [ -f "$scenario" ] || { echo "  pregenerate_srt: no scenario file: $scenario" >&2; return 1; }
    > "$outfile"
    local t=0.0 idx=0

    local offset="${SRT_OFFSET_SECS:-1.5}"

    _emit_srt() {
        local start="$1" end="$2" text="$3"
        idx=$((idx + 1))
        start=$(awk -v a="$start" -v o="$offset" 'BEGIN { printf "%.3f", a + o }')
        end=$(awk -v a="$end" -v o="$offset" 'BEGIN { printf "%.3f", a + o }')
        local st et
        st=$(secs_to_srt "$start")
        et=$(secs_to_srt "$end")
        {
            echo "$idx"
            echo "$st --> $et"
            echo "$text"
            echo ""
        } >> "$outfile"
    }

    _clip_duration_or_estimate() {
        local id="$1" text="$2"
        local est
        est=$(min_subtitle_dur "$text")
        local wav="$TTS_DIR/${id}.wav"
        if [ -f "$wav" ]; then
            local d
            d=$(tts_duration "$wav" 2>/dev/null)
            # Use max(wav_dur, reading_time).
            if [ -n "$d" ]; then
                awk -v a="$d" -v b="$est" 'BEGIN { if (a > b) printf "%.3f", a; else printf "%.3f", b }'
                return
            fi
        fi
        echo "$est"
    }

    local clip_id=""
    local scene_num=0
    while IFS= read -r line; do
        # Strip leading whitespace for matching
        local trimmed="${line#"${line%%[![:space:]]*}"}"
        # narrate "id" "text"   |   narrate --wait "id" "text"
        if [[ "$trimmed" =~ ^narrate[[:space:]]+(--wait[[:space:]]+)?\"([^\"]+)\"[[:space:]]+\"(.+)\"[[:space:]]*\\?$ ]]; then
            local is_wait="${BASH_REMATCH[1]}" id="${BASH_REMATCH[2]}" text="${BASH_REMATCH[3]}"
            local dur
            dur=$(_clip_duration_or_estimate "$id" "$text")
            local end
            end=$(awk -v a="$t" -v b="$dur" 'BEGIN { printf "%.3f", a + b }')
            _emit_srt "$t" "$end" "$text"
            if [ -n "$is_wait" ]; then
                t="$end"
            fi
            clip_id=""
        # Two-line form: narrate "id" \   (next line is the quoted text)
        elif [[ "$trimmed" =~ ^narrate[[:space:]]+(--wait[[:space:]]+)?\"([^\"]+)\"[[:space:]]+\\$ ]]; then
            clip_id="${BASH_REMATCH[2]}"
            _two_line_is_wait="${BASH_REMATCH[1]}"
        elif [[ -n "$clip_id" && "$trimmed" =~ ^\"(.+)\"[[:space:]]*$ ]]; then
            local text="${BASH_REMATCH[1]}"
            local dur
            dur=$(_clip_duration_or_estimate "$clip_id" "$text")
            local end
            end=$(awk -v a="$t" -v b="$dur" 'BEGIN { printf "%.3f", a + b }')
            _emit_srt "$t" "$end" "$text"
            if [ -n "$_two_line_is_wait" ]; then
                t="$end"
            fi
            clip_id=""
            _two_line_is_wait=""
        # scene "name"   (track scene counter for auto-ID lookup)
        elif [[ "$trimmed" =~ ^scene[[:space:]]+\".*\" ]]; then
            scene_num=$((scene_num + 1))
        # say "text"  (expands to narrate --wait + pause 1)
        elif [[ "$trimmed" =~ ^say[[:space:]]+\"(.+)\"[[:space:]]*$ ]]; then
            local text="${BASH_REMATCH[1]}"
            local slug
            slug=$(echo "$text" | tr ' ' '_' | tr -cd 'a-zA-Z0-9_' | head -c 30)
            local auto_id
            auto_id=$(printf 'auto_%03d_%s' "$scene_num" "$slug")
            local dur
            dur=$(_clip_duration_or_estimate "$auto_id" "$text")
            local end
            end=$(awk -v a="$t" -v b="$dur" 'BEGIN { printf "%.3f", a + b }')
            _emit_srt "$t" "$end" "$text"
            # say always blocks (narrate --wait) then pauses 1s
            t=$(awk -v a="$end" 'BEGIN { printf "%.3f", a + 1.0 }')
        # pause N  |  sleep N  |  wait_seconds N   (advance cursor)
        elif [[ "$trimmed" =~ ^(pause|sleep|wait_seconds)[[:space:]]+([0-9.]+) ]]; then
            local n="${BASH_REMATCH[2]}"
            t=$(awk -v a="$t" -v b="$n" 'BEGIN { printf "%.3f", a + b }')
        fi
    done < "$scenario"

    echo "  Pre-generated SRT: $outfile ($idx entries, estimated end at ${t}s)"
    echo "$outfile"
}

secs_to_srt() {
    local total="$1"
    local h m s ms
    h=$(echo "$total / 3600" | bc)
    m=$(echo "($total - $h * 3600) / 60" | bc)
    s=$(echo "($total - $h * 3600 - $m * 60) / 1" | bc)
    ms=$(echo "scale=0; ($total - $h * 3600 - $m * 60 - $s) * 1000 / 1" | bc)
    printf "%02d:%02d:%02d,%03d" "$h" "$m" "$s" "$ms"
}

srt_time_to_secs() {
    # HH:MM:SS,mmm → seconds (float).
    local t="$1"
    local h m s ms
    IFS=':,' read -r h m s ms <<< "$t"
    awk -v h="$h" -v m="$m" -v s="$s" -v ms="$ms" \
        'BEGIN { printf "%.3f", h*3600 + m*60 + s + ms/1000 }'
}

srt_shift_seconds() {
    # Subtract `offset` seconds from every timestamp in `file` (in place).
    # Cues whose END time falls before 0 are dropped; cues whose START time
    # falls before 0 are clipped to 0 so they still appear at the beginning.
    local file="$1"
    local offset="$2"
    [ -f "$file" ] || return 1
    local tmp
    tmp="$(mktemp)"
    awk -v offset="$offset" '
        function ts_to_s(t,   h, m, s, ms, parts1, parts2) {
            split(t, parts1, ",")
            ms = parts1[2] + 0
            split(parts1[1], parts2, ":")
            h = parts2[1] + 0; m = parts2[2] + 0; s = parts2[3] + 0
            return h*3600 + m*60 + s + ms/1000
        }
        function s_to_ts(x,   h, m, s, ms) {
            if (x < 0) x = 0
            h = int(x/3600); x -= h*3600
            m = int(x/60);   x -= m*60
            s = int(x)
            ms = int((x - s)*1000 + 0.5)
            return sprintf("%02d:%02d:%02d,%03d", h, m, s, ms)
        }
        BEGIN { block=""; drop=0 }
        /^[0-9]+[[:space:]]*$/ && block=="" { idx=$0; block="idx"; next }
        block=="idx" && /-->/ {
            split($0, parts, " --> ")
            a = ts_to_s(parts[1]) - offset
            b = ts_to_s(parts[2]) - offset
            if (b <= 0) { drop=1; block=""; idx=""; next }
            printf "%s\n%s --> %s\n", idx, s_to_ts(a), s_to_ts(b)
            block="text"; next
        }
        block=="text" && /^[[:space:]]*$/ { print ""; block=""; next }
        block=="text" { print; next }
        drop && /^[[:space:]]*$/ { drop=0; next }
        drop { next }
        { print }
    ' "$file" > "$tmp" && mv "$tmp" "$file"
}

# ─── Window helpers (X11) ────────────────────────────────────────────────────

find_app_window() {
    # Use wmctrl (reliable) instead of libxdo search (returns bogus IDs).
    wmctrl -l 2>/dev/null | grep -i "Impulse Instruct" | head -1 | awk '{print $1}'
}

get_window_geometry() {
    local wid="$1"
    xwininfo -id "$wid" 2>/dev/null | awk '
        /Absolute upper-left X:/ {x=$NF}
        /Absolute upper-left Y:/ {y=$NF}
        /Width:/  {w=$NF}
        /Height:/ {h=$NF}
        END {print x, y, w, h}
    '
}

# ─── Screenshot capture ──────────────────────────────────────────────────────

capture_screenshot() {
    # Capture the app window to a PNG file in the batch output dir.
    # Usage: capture_screenshot "v0.7.1-rack-backside"
    local name="$1"

    # Determine output directory — batch dir during recording, fallback to demo/output
    local out_dir="${BATCH_DIR:-${DEMO_DIR}/output}"
    mkdir -p "$out_dir"
    local outfile="$out_dir/${name}.png"

    local wid="${APP_WINDOW_ID:-}"
    if [ -z "$wid" ] || [ "$wid" = "0" ]; then
        wid=$(find_app_window)
    fi
    if [ -z "$wid" ] || [ "$wid" = "0" ]; then
        echo "  WARNING: No app window for screenshot" >&2
        return 1
    fi

    # Use import (ImageMagick) for window capture — handles decorations well
    if command -v import >/dev/null 2>&1; then
        import -window "$wid" "$outfile" 2>/dev/null
    elif command -v scrot >/dev/null 2>&1; then
        # scrot fallback — focused window mode
        wmctrl -i -a "$wid" 2>/dev/null
        sleep 0.3
        scrot -u "$outfile" 2>/dev/null
    elif command -v gnome-screenshot >/dev/null 2>&1; then
        wmctrl -i -a "$wid" 2>/dev/null
        sleep 0.3
        gnome-screenshot -w -f "$outfile" 2>/dev/null
    else
        echo "  WARNING: No screenshot tool found (install imagemagick)" >&2
        return 1
    fi

    echo "  Screenshot: $outfile"
}

# ═══════════════════════════════════════════════════════════════════════════════
# High-level scenario DSL
#
# These functions let demo scripts read like a screenplay.
# Scenarios should use ONLY these — no curl, no raw API calls.
# ═══════════════════════════════════════════════════════════════════════════════

# Internal scene counter
_scene_num=0
_scene_total=0

scene_count() {
    # Declare total scene count at the top of a scenario.
    _scene_total=$1
}

# ── Scene structure ──────────────────────────────────────────────────────────

scene() {
    # Start a named scene. Usage: scene "Building the acid pattern"
    _scene_num=$((_scene_num + 1))
    echo "  [Scene ${_scene_num}/${_scene_total:-?}] $1"
}

say() {
    # Narrate a line (blocking — waits until speech finishes).
    # Usage: say "The filter creates that classic squelch."
    local id
    id="auto_$(printf '%03d' $_scene_num)_$(echo "$1" | tr ' ' '_' | tr -cd 'a-zA-Z0-9_' | head -c 30)"
    narrate --wait "$id" "$1"
    pause 1
}

wait_seconds() {
    # Pause for N seconds (let the music play, let inference finish).
    pause "$1"
}

# ── Rack setup ───────────────────────────────────────────────────────────────

reset_rack() {
    # Clear the rack to minimal (sequencer + master + console).
    # Also ensure clean visual state: front side, all zones expanded.
    api_rack_reset
    api_flip_front
    api_collapse "none"
    pause 1
}

reset_all() {
    # Full state wipe — clears rack, agents, active_style, LFOs, seq transport,
    # all params. Stronger than reset_rack: use at the top of style scenarios
    # to prevent a prior session's active_style (or any leftover state) from
    # bleeding into the new demo.
    api_state_reset
    api_flip_front
    api_collapse "none"
    pause 1
}

set_style() {
    # Pin the global active style so the LLM's system prompt carries the right
    # style brief into every ask.  Call right after reset_all, before the first
    # ask, to guarantee the agent isn't still anchored to a previous style.
    # Usage: set_style drum_and_bass
    api_set_style "$1"
}

add_instrument() {
    # Add an instrument module.  Usage: add_instrument bass
    # Returns the module ID (capture with: id=$(add_instrument bass))
    api_rack_add "$1"
}

add_effect() {
    # Add an effect module.  Usage: add_effect reverb
    api_rack_add "$1"
}

add_agent() {
    # Add an AI agent.
    # Usage: add_agent ACID gemma
    #        add_agent BASS gemma bass
    #        add_agent DRUMS gemma "kit_a,kit_b"
    #        add_agent MC gemma "" mc tts    (MC mode with TTS enabled)
    local persona="$1"
    local model="${2:-gemma}"
    local scope_csv="${3:-}"
    local mode="${4:-}"
    local tts="${5:-}"
    local scope_json="[]"
    if [ -n "$scope_csv" ]; then
        scope_json=$(echo "$scope_csv" | tr ',' '\n' | sed 's/^/"/;s/$/"/' | paste -sd, | sed 's/^/[/;s/$/]/')
    fi
    local extra=""
    if [ -n "$mode" ]; then
        extra="$extra, \"mode\": \"$mode\""
    fi
    if [ "$tts" = "tts" ]; then
        extra="$extra, \"tts\": true"
    fi
    curl -sf -X POST "$API/api/rack/agent" \
        -H "Content-Type: application/json" \
        -d "{\"persona\": \"$persona\", \"scope\": $scope_json, \"model\": \"$model\" $extra}" \
        2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo ""
}

wait_for_model() {
    # Wait until the LLM model server is ready.
    wait_for_llm "${1:-120}"
}

# ── Playback ─────────────────────────────────────────────────────────────────

play()  { api_play; }
stop()  { api_stop; }

set_bpm() {
    # Force sequencer tempo. Usage: set_bpm 170
    api_params "{\"sequencer\":{\"bpm\":$1}}"
}

# ── AI prompts ───────────────────────────────────────────────────────────────

ask() {
    # Send a natural language prompt to the AI and wait for it to respond.
    # Usage: ask "make it acid"
    #        ask "set cutoff to 0.3" BASS
    local prompt="$1"
    local agent="${2:-}"
    api_prompt "$prompt" "$agent"
    pause "${3:-5}"
}

# ── View control ─────────────────────────────────────────────────────────────

look_at() {
    # Scroll the UI to show a section.
    # Usage: look_at sequencer | look_at bass | look_at 808 | look_at console
    api_scroll "$1"
    pause 0.5
}

focus_on() {
    # Scroll to a section AND collapse everything else.
    # Usage: focus_on bass
    api_focus "$1"
    pause 0.5
}

show_all() {
    # Expand all zones.
    api_collapse "none"
    pause 0.5
}

show_cables() {
    # Flip rack to show back panel (cables).
    api_flip_back
    pause 1
}

show_knobs() {
    # Flip rack to show front panel (knobs).
    api_flip_front
    pause 1
}

tour_rack() {
    # Scroll through every zone (AI → MAIN AUDIO → VOICES → FX+MOD → back
    # up) with a short pause on each so viewers can see the full rack
    # contents. Useful while the back panel is flipped so every cable
    # segment is on screen at some point.
    # Usage: tour_rack              # ~6 s total, default pause 1.0 s/zone
    #        tour_rack 2.0          # 2 s per zone (slower)
    local per="${1:-1.2}"
    for tgt in ai global voice fxmod voice global ai; do
        api_scroll "$tgt"
        pause "$per"
    done
}

# ── Parameter control ────────────────────────────────────────────────────────

lock() {
    # Lock parameters so the AI can't change them.
    # Usage: lock "tb303.cutoff" "tb303.resonance"
    api_lock "$@"
}

unlock() {
    # Unlock parameters.
    # Usage: unlock "tb303.cutoff" "tb303.resonance"
    api_unlock "$@"
}

set_params() {
    # Set parameters directly (JSON).
    # Usage: set_params '{"tb303": {"cutoff": 0.3}}'
    api_params "$1"
}

use_preset() {
    # Apply an agent preset: Solo, Duo, Swarm, Band, Voices, Lite
    api_preset "$1"
}

# ── Filter pad sweep (animate cutoff/resonance) ─────────────────────────────

sweep_pad() {
    # Smooth acid filter sweep via many small interpolated steps.
    # Keyframes define the shape, intermediate values are lerped.
    # Usage: sweep_pad [seconds]
    local duration="${1:-8}"
    # Keyframes: cutoff resonance — stay in the top-left quarter of the pad
    # (low cutoff, high resonance) for a focused acid squelch demo.
    local keys=(
        "0.08 0.82"
        "0.15 0.92"
        "0.25 0.88"
        "0.35 0.78"
        "0.42 0.85"
        "0.30 0.95"
        "0.18 0.90"
        "0.10 0.80"
        "0.22 0.96"
        "0.38 0.88"
        "0.28 0.78"
        "0.12 0.92"
        "0.20 0.98"
        "0.33 0.82"
        "0.10 0.88"
    )
    local nkeys=${#keys[@]}
    # ~60 updates per 8 seconds ≈ 7.5fps — smooth enough for visible knob motion
    local total_steps=60
    local interval
    interval=$(echo "scale=4; $duration / $total_steps" | bc)
    python3 -c "
import time, subprocess, sys
keys = [tuple(map(float, k.split())) for k in '''$(printf '%s\n' "${keys[@]}")'''.strip().split('\n')]
n = len(keys)
total = $total_steps
dt = float('$interval')
for i in range(total):
    t = i / (total - 1) * (n - 1)
    idx = int(t)
    frac = t - idx
    if idx >= n - 1:
        c, r = keys[-1]
    else:
        c = keys[idx][0] + (keys[idx+1][0] - keys[idx][0]) * frac
        r = keys[idx][1] + (keys[idx+1][1] - keys[idx][1]) * frac
    subprocess.run(['curl', '-sf', '-X', 'POST', '$API/api/params',
        '-H', 'Content-Type: application/json',
        '-d', '{\"params\":{\"bass\":{\"cutoff\":%.3f,\"resonance\":%.3f}}}' % (c, r)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(dt)
" 2>/dev/null
}

# ── Screenshots ──────────────────────────────────────────────────────────────

screenshot() {
    # Capture the app window to assets/screenshots/<name>.png.
    # Usage: screenshot "v0.7.1-rack-backside"
    pause 0.5
    capture_screenshot "$1"
}
