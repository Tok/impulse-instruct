#!/usr/bin/env bash
# ─── demo/lib.sh ── shared helpers for demo recording ────────────────────────
# Sourced by record-demo.sh and scenario.sh. Not meant to run standalone.

API="http://127.0.0.1:8765"
DEMO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TTS_DIR="$DEMO_DIR/tts_cache"
NARRATION_LIST="$DEMO_DIR/.narration_playlist"
XDO="python3 $DEMO_DIR/xdo.py"

# TTS settings — CoquiTTS preferred, espeak-ng fallback
TTS_MODEL="${TTS_MODEL:-}"      # CoquiTTS model (empty = default)
TTS_VOICE="${TTS_VOICE:-en-us+f3}"  # espeak-ng voice (fallback)
TTS_SPEED="${TTS_SPEED:-155}"   # espeak-ng words per minute
TTS_PITCH="${TTS_PITCH:-45}"    # espeak-ng pitch

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

# ─── TTS helpers ──────────────────────────────────────────────────────────────

tts_generate() {
    # Pre-generate a TTS clip. Returns the wav path.
    # Tries CoquiTTS first (better quality), falls back to espeak-ng.
    # Usage: tts_generate "clip_id" "Text to speak"
    local id="$1" text="$2"
    local outfile="$TTS_DIR/${id}.wav"
    mkdir -p "$TTS_DIR"
    if [ ! -f "$outfile" ]; then
        if command -v tts >/dev/null 2>&1; then
            # CoquiTTS — high-quality neural TTS
            tts --text "$text" --out_path "$outfile" \
                ${TTS_MODEL:+--model_name "$TTS_MODEL"} 2>/dev/null
        fi
        # Fallback to espeak-ng if CoquiTTS failed or isn't installed
        if [ ! -f "$outfile" ]; then
            espeak-ng -v "$TTS_VOICE" -s "$TTS_SPEED" -p "$TTS_PITCH" \
                -w "$outfile" "$text" 2>/dev/null
        fi
    fi
    echo "$outfile"
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
    local blocking=0
    if [ "$1" = "--wait" ]; then blocking=1; shift; fi

    local id="$1" text="$2"
    local wavfile
    wavfile=$(tts_generate "$id" "$text")
    local dur
    dur=$(tts_duration "$wavfile")
    local start_sec
    start_sec=$(demo_elapsed)

    echo "${start_sec}|${dur}|${text}" >> "$NARRATION_LIST"

    if [ "$blocking" -eq 1 ]; then
        paplay "$wavfile" 2>/dev/null || aplay "$wavfile" 2>/dev/null
    else
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
    pw-cli ls Node 2>/dev/null | awk '
        /^[[:space:]]*id [0-9]+/ { id=$2; gsub(/,/,"",id) }
        /application.name.*[Ii]mpulse/ || /node.name.*impulse/ { print id; exit }
    '
}

start_pw_capture() {
    local outfile="$1"
    local node_id
    node_id=$(find_app_pw_node)

    if [ -n "$node_id" ]; then
        echo "  Capturing audio from PipeWire node $node_id"
        pw-record --target "$node_id" "$outfile" &
        echo $!
    else
        echo "  WARNING: App PipeWire node not found, capturing default output" >&2
        local sink
        sink=$(pactl info 2>/dev/null | awk -F: '/Default Sink:/{gsub(/^[ \t]+/,"",$2); print $2".monitor"}')
        pw-record --target "${sink:-default}" "$outfile" &
        echo $!
    fi
}

# ─── Timing helpers ───────────────────────────────────────────────────────────

pause() {
    sleep "$1"
}

# ─── Subtitle generation ─────────────────────────────────────────────────────

generate_srt() {
    local outfile="$DEMO_DIR/demo_subtitles.srt"
    local idx=0

    if [ ! -f "$NARRATION_LIST" ]; then
        echo "No narration entries found" >&2
        return 1
    fi

    > "$outfile"
    while IFS='|' read -r start_sec dur text; do
        idx=$((idx + 1))
        local end_sec
        end_sec=$(echo "$start_sec + $dur" | bc)

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

secs_to_srt() {
    local total="$1"
    local h m s ms
    h=$(echo "$total / 3600" | bc)
    m=$(echo "($total - $h * 3600) / 60" | bc)
    s=$(echo "($total - $h * 3600 - $m * 60) / 1" | bc)
    ms=$(echo "scale=0; ($total - $h * 3600 - $m * 60 - $s) * 1000 / 1" | bc)
    printf "%02d:%02d:%02d,%03d" "$h" "$m" "$s" "$ms"
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
    api_rack_reset
    api_flip_front
    pause 1
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
    #        add_agent BASS bonsai bass
    #        add_agent DRUMS bonsai "kit_a,kit_b"
    #        add_agent MC bonsai "" mc tts    (MC mode with TTS enabled)
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

# ── AI prompts ───────────────────────────────────────────────────────────────

ask() {
    # Send a natural language prompt to the AI and wait for it to respond.
    # Usage: ask "make it acid"
    #        ask "set cutoff to 0.3" BASS
    local prompt="$1"
    local agent="${2:-}"
    api_prompt "$prompt" "$agent"
    pause "${3:-10}"
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
