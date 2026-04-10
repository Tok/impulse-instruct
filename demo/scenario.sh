#!/usr/bin/env bash
# ─── demo/scenario.sh ── the demo sequence ───────────────────────────────────
# Sourced by record-demo.sh after the app is running and recording has started.
#
# Demo structure:
#   Part 1 — Build up from minimal rack: add instruments, wire an AI agent
#   Part 2 — Multi-agent band: add specialist agents, show control cables
# ─────────────────────────────────────────────────────────────────────────────

# ═══════════════════════════════════════════════════════════════════════════════
# PART 1: BUILD UP — from empty rack to AI-controlled acid house
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo "  ┌────────────────────────────────────────────────────┐"
echo "  │  PART 1: Building the rack from scratch            │"
echo "  └────────────────────────────────────────────────────┘"

# ─── Scene 1: Intro + reset to minimal rack ──────────────────────────────────

echo "  [Scene 1/16] intro — reset rack, wait for Gemma 4..."

api_rack_reset
api_flip_front
pause 1

api_scroll "sequencer"
pause 0.5

narrate --wait "s01_welcome" \
    "Welcome to Impulse Instruct. A smart synthesizer with a virtual production team inside it."
pause 1

narrate --wait "s01_minimal" \
    "We're starting with a minimal rack. Just a sequencer, master output, and the AI console."
pause 1

wait_for_llm 120

# ─── Scene 2: Add instruments ────────────────────────────────────────────────

echo "  [Scene 2/16] add_instruments — adding three oh three, eight oh eight, nine oh nine..."

narrate --wait "s02_add_intro" \
    "Let's add some instruments. First, a three oh three acid bass."
pause 0.5

BASS_ID=$(api_rack_add "bass")
pause 1

api_scroll "bass"
pause 0.5

narrate --wait "s02_bass_added" \
    "The three oh three appears in the voice zone. Now an eight oh eight drum machine."
pause 0.5

KICK_ID=$(api_rack_add "808")
pause 1

narrate --wait "s02_808_added" \
    "And a nine oh nine for claps and percussion."
pause 0.5

CLAP_ID=$(api_rack_add "909")
pause 1

api_scroll "808"
pause 0.5

narrate --wait "s02_instruments_ready" \
    "Three classic instruments, ready to be wired up."
pause 2

# ─── Scene 3: Add an LLM agent ───────────────────────────────────────────────

echo "  [Scene 3/16] add_agent — adding PULSE agent with Gemma 4..."

api_scroll "console"
pause 0.5

narrate --wait "s03_agent_intro" \
    "Now the magic. Let's add an AI agent to control everything."
pause 0.5

AGENT_ID=$(api_rack_agent "PULSE" '[]' "gemma")
pause 1

narrate --wait "s03_agent_added" \
    "Agent PULSE is connected. With an empty scope, it controls all instruments."
pause 1

# ─── Scene 4: Show the wiring — flip to back panel ───────────────────────────

echo "  [Scene 4/16] show_wiring — flip rack to show control cables..."

narrate --wait "s04_flip_intro" \
    "Let's flip the rack to see how the agent is wired."
pause 0.5

api_flip_back
pause 1

api_scroll "console"
pause 0.5

narrate --wait "s04_control_cables" \
    "The thin cables are control links. They connect PULSE to every instrument in its scope."
pause 3

api_scroll "bass"
pause 0.5

narrate --wait "s04_cable_detail" \
    "Each control cable defines what the agent can drive. This is unique to Impulse Instruct."
pause 3

api_flip_front
pause 1

# ─── Scene 5: Start the sequencer and send first prompt ──────────────────────

echo "  [Scene 5/16] first_prompt — start sequencer, ask AI to build a pattern..."

api_scroll "sequencer"
pause 0.5

narrate --wait "s05_start_seq" \
    "Let's start the sequencer and ask the AI to build us a pattern."
pause 0.5

api_play
pause 2

api_scroll "console"
pause 0.5

api_prompt "set up a classic acid house groove with a driving bass line, four on the floor kick, and off-beat hats"
pause 8

api_scroll "sequencer"
pause 0.5

narrate --wait "s05_pattern_done" \
    "The AI programmed a full pattern from a single sentence."
pause 4

# ─── Scene 6: Show voice changes ─────────────────────────────────────────────

echo "  [Scene 6/16] voice_changes — scroll to bass to show AI changes..."

api_scroll "bass"
pause 0.5

narrate --wait "s06_bass_view" \
    "The three oh three has a bass line with filter settings the AI chose."
pause 3

api_scroll "808"
pause 0.5

narrate --wait "s06_drums_view" \
    "The eight oh eight has a kick and hat pattern."
pause 2

# ─── Scene 7: Tweak via LLM ──────────────────────────────────────────────────

echo "  [Scene 7/16] acid_tweak — making it more acid..."

api_scroll "console"
pause 0.5

narrate --wait "s07_tweak_prompt" \
    "Let's push the sound harder."
pause 0.5

api_prompt "crank the resonance, make the bass line more acid and squelchy"
pause 8

api_scroll "bass"
pause 0.5

narrate --wait "s07_tweak_result" \
    "The filter resonance jumped. That's the classic acid sound."
pause 3

# ─── Scene 8: Add FX modules ─────────────────────────────────────────────────

echo "  [Scene 8/16] add_fx — adding reverb and delay..."

narrate --wait "s08_fx_intro" \
    "Let's add some effects. Reverb and delay."
pause 0.5

api_rack_add "reverb"
api_rack_add "delay"
pause 1

api_scroll "reverb"
pause 0.5

narrate --wait "s08_fx_added" \
    "Two new modules in the effects zone. Let's ask the AI to use them."
pause 1

api_scroll "console"
pause 0.5

api_prompt "add some spacious reverb and a synced delay to the bass"
pause 8

api_scroll "reverb"
pause 0.5

narrate --wait "s08_fx_active" \
    "The AI activated the effects and dialed in settings."
pause 3

# ─── Scene 9: Parameter locking ──────────────────────────────────────────────

echo "  [Scene 9/16] param_locking — lock kick, send conflicting prompt..."

api_scroll "808"
pause 0.5

narrate --wait "s09_lock_intro" \
    "You can lock parameters so the AI can't change them. Let's lock the kick."
pause 0.5

api_lock "sequencer.kick_a_steps"
pause 0.5

api_scroll "console"
pause 0.5

api_prompt "switch to a minimal techno style, strip everything back"
pause 8

api_scroll "808"
pause 0.5

narrate --wait "s09_lock_result" \
    "The kick stayed locked while the AI changed everything else."
pause 2

api_unlock "sequencer.kick_a_steps"

# ═══════════════════════════════════════════════════════════════════════════════
# PART 2: MULTI-AGENT — specialist agents with control cables
# ═══════════════════════════════════════════════════════════════════════════════

echo ""
echo "  ┌────────────────────────────────────────────────────┐"
echo "  │  PART 2: Multi-agent band — specialist AI agents   │"
echo "  └────────────────────────────────────────────────────┘"

# ─── Scene 10: Remove PULSE, add specialist agents ────────────────────────────

echo "  [Scene 10/16] add_specialists — replacing single agent with band..."

api_stop
pause 1

api_scroll "console"
pause 0.5

narrate --wait "s10_band_intro" \
    "Now let's split the AI into specialist agents. Each one controls its own instruments."
pause 1

# Remove the PULSE agent
if [ -n "$AGENT_ID" ]; then
    api_rack_remove "$AGENT_ID"
    pause 0.5
fi

narrate --wait "s10_add_bass_agent" \
    "A bass agent running Bonsai, a lightweight model, just for the three oh three."
pause 0.5

BASS_AGENT=$(api_rack_agent "BASS" '["bass"]' "bonsai")
pause 1

narrate --wait "s10_add_drum_agent" \
    "A drum agent for the eight oh eight and nine oh nine."
pause 0.5

DRUM_AGENT=$(api_rack_agent "DRUMS" '["kit_a", "kit_b"]' "bonsai")
pause 1

narrate --wait "s10_add_fx_agent" \
    "And an effects agent to manage reverb and delay."
pause 0.5

FX_AGENT=$(api_rack_agent "FX" '["fx"]' "bonsai")
pause 1

wait_for_llm 90

narrate --wait "s10_band_ready" \
    "Three specialist agents, each running Bonsai. They share one lightweight model server."
pause 1

# ─── Scene 11: Show agent wiring ─────────────────────────────────────────────

echo "  [Scene 11/16] agent_wiring — flip rack to show control cables..."

api_flip_back
pause 1

api_scroll "console"
pause 0.5

narrate --wait "s11_wiring_overview" \
    "On the back panel, each agent has its own control cables. BASS connects to the three oh three only."
pause 3

api_scroll "bass"
pause 0.5

narrate --wait "s11_wiring_detail" \
    "DRUMS connects to the eight oh eight and nine oh nine. FX to the effects modules."
pause 3

api_flip_front
pause 1

# ─── Scene 12: Band jam ──────────────────────────────────────────────────────

echo "  [Scene 12/16] band_jam — starting multi-agent jam..."

api_scroll "console"
pause 0.5

narrate --wait "s12_jam_start" \
    "Let's start playing and give the band a direction."
pause 0.5

api_play
pause 2

# Send prompts to EACH agent individually so they all respond
api_prompt "lay down a deep mellow bass line" "BASS"
pause 8

api_prompt "build a four on the floor kick with off-beat hats" "DRUMS"
pause 8

api_prompt "add warm reverb and a gentle delay" "FX"
pause 8

narrate --wait "s12_jam_playing" \
    "Each agent handled its own part. Bass laid down a line. Drums built the beat. FX added space."
pause 3

# ─── Scene 13: Watch agents work ─────────────────────────────────────────────

echo "  [Scene 13/16] agents_working — agents evolving their parts..."

api_scroll "bass"
pause 0.5

narrate --wait "s13_bass_intro" \
    "Let's tell each agent to evolve independently."
pause 0.5

api_prompt "make the bass more acid, add some slides" "BASS"
pause 8

api_scroll "808"
pause 0.5

api_prompt "add a clap on two and four, vary the hat pattern" "DRUMS"
pause 8

narrate --wait "s13_agents_done" \
    "Each agent responds within its scope. Bass only touches the three oh three. Drums only the kits."
pause 3

# ─── Scene 14: Creative direction ────────────────────────────────────────────

echo "  [Scene 14/16] creative_direction — acid breakdown..."

api_scroll "console"
pause 0.5

narrate --wait "s14_creative" \
    "Now let's give each agent a creative direction."
pause 0.5

api_prompt "acid filter sweep, drop the cutoff low then sweep up" "BASS"
pause 8

api_prompt "open the hats, add some fills" "DRUMS"
pause 8

api_prompt "crank the reverb, add heavy delay feedback" "FX"
pause 8

narrate --wait "s14_result" \
    "Each specialist shaped its own instruments based on the same creative vision."
pause 3

# ─── Scene 15: Speed it up ───────────────────────────────────────────────────

echo "  [Scene 15/16] tempo_push — speeding up..."

api_scroll "sequencer"
pause 0.5

narrate --wait "s15_tempo" \
    "Let's push the tempo."
pause 0.5

api_prompt "speed it up to 140 BPM, peak-time energy"
pause 8

narrate --wait "s15_tempo_result" \
    "140 B P M. The full band driving together."
pause 3

# ─── Scene 16: Outro ─────────────────────────────────────────────────────────

echo "  [Scene 16/16] outro..."

api_scroll "sequencer"
pause 0.5

narrate --wait "s16_summary" \
    "That's Impulse Instruct. Build your rack, wire up AI agents, and make music with natural language."
pause 1

narrate --wait "s16_modes" \
    "One agent or a full band. Each with its own scope and model. Everything runs locally."
pause 1

narrate --wait "s16_thanks" \
    "Thanks for watching."
pause 2

api_stop
pause 1

echo ""
echo "  All 16 scenes complete."
