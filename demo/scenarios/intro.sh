#!/usr/bin/env bash
# ─── Intro Demo ─────────────────────────────────────────────────────────────
# Quick overview of Impulse Instruct. Sound within 30 seconds.
# Two parts: single agent, then multi-agent band.

scene_count 14

# ═══════════════════════════════════════════════════════════════════════════════
# PART 1: Quick start — rack + agent + sound
# ═══════════════════════════════════════════════════════════════════════════════

scene "Setup"

reset_rack

say "Impulse Instruct. AI-controlled audio synthesizers. Let's build a rack."

say "Adding bass."
add_instrument bass
look_at bass
wait_seconds 1

say "Drums."
add_instrument 808
add_instrument 909
look_at 808
wait_seconds 1

say "Reverb and delay."
add_effect reverb
add_effect delay
look_at reverb
wait_seconds 1

say "Adding an AI agent."
add_agent PULSE gemma
look_at console
wait_seconds 2
wait_for_model

# ── Scene 2: First sound ────────────────────────────────────────────────────

scene "First sound"

look_at sequencer

say "Asking the agent for a pattern."

# Send prompt BEFORE playing — pattern loads while sequencer is stopped.
ask "acid groove, kick and hats, short bass line with gaps. set pan positions for stereo width, add subtle chorus" "" 5

play
wait_seconds 3

screenshot "v${VERSION}-intro-sequencer"

# ── Scene 3: Tighten the beat ───────────────────────────────────────────────

scene "Tighten the beat"

ask "add a clap on two and four, a couple more bass notes, set an accent on beat one"

focus_on 808
wait_seconds 3

say "Drums refined. Now the filter."

# ── Scene 4: Acid pad — cutoff/resonance sweep ─────────────────────────────

scene "Acid filter sweep"

focus_on bass
wait_seconds 1

say "Cutoff and resonance."

sweep_pad 11

wait_seconds 1

screenshot "v${VERSION}-intro-bass-detail"

# ── Scene 5: Ramp the filter via AI ────────────────────────────────────────

scene "AI-controlled ramp"

ask "slowly sweep the filter open over 4 bars" 14

say "The AI ramps parameters over bars. Smooth, tempo-synced."
wait_seconds 2

# ── Scene 6: Show cables — after wiring is visible ─────────────────────────

scene "Control cables"

# Scroll to console area where the agent sits, then flip
look_at console
wait_seconds 1
show_cables
say "Back panel. Control cables connect the agent to each instrument."
# Tour every zone top-to-bottom so all cables are on screen at some point.
tour_rack 1.2
screenshot "v${VERSION}-intro-cables"
show_knobs
wait_seconds 1

# ── Scene 7: LFO — living modulation while the track keeps playing ─────────

scene "LFO modulation"

# Drop an LFO module into the rack and scroll to it so the module card
# is on screen before we talk about what it does.
add_effect lfo
look_at lfo
wait_seconds 1

say "An LFO moves a parameter hands-free. Any knob can be a target."

# ── LFO 1: classic sine on the bass filter cutoff ──
say "Sine wave on the bass cutoff — classic acid breath."
api_params '{"lfo": [{"enabled": true, "target": "BassCutoff", "waveform": "Sine", "rate": 0.18, "depth": 0.55}]}'
focus_on bass
wait_seconds 4

# ── LFO 2: slower triangle on reverb mix — different waveform + target ──
say "A second LFO, triangle wave, on the reverb mix."
api_params '{"lfo": [{}, {"enabled": true, "target": "ReverbMix", "waveform": "Triangle", "rate": 0.06, "depth": 0.35}]}'
look_at lfo
wait_seconds 4

say "Filter pulses fast, reverb breathes slow."
wait_seconds 3

# Release both LFOs before we tear down the single-agent rack.
api_params '{"lfo": [{"enabled": false}, {"enabled": false}]}'

# ═══════════════════════════════════════════════════════════════════════════════
# PART 2: Multi-agent band
# ═══════════════════════════════════════════════════════════════════════════════

scene "Multi-agent setup"

wait_seconds 2
stop
wait_seconds 1

say "Split the AI into specialists."
say "Each one controls its own instruments."

reset_rack
add_instrument bass
look_at bass
wait_seconds 0.5
add_instrument 808
add_instrument 909
look_at 808
wait_seconds 0.5
add_effect reverb
add_effect delay

# Add agents and scroll to console to show them appearing
look_at console
wait_seconds 0.5
add_agent BASS bonsai bass
wait_seconds 0.5
add_agent DRUMS bonsai "kit_a,kit_b"
wait_seconds 0.5
add_agent FX bonsai fx
wait_for_model

# Brief cable view — agents are now wired, so cables are visible
show_cables
say "Each agent has its own control cables."
# Scroll top-to-bottom so every agent's cable run is visible.
tour_rack 1.2
show_knobs
wait_seconds 1

# ── Scene 9: Band jam ─────────────────────────────────────────────────────

scene "Band jam"

play
wait_seconds 1

ask "acid bass line with syncopation, squelchy, cutoff low, resonance high, pan center" BASS
ask "kick on steps 0,4,8,12,16,20,24,28 pan center. hihat on 2,6,10,14,18,22,26,30 pan 0.3. clap on 4,12,20,28 pan -0.3. open hihat on 6,14,22,30" DRUMS
ask "reverb mix 0.12, reverb size 0.5, delay mix 0.08, chorus mix 0.1, stereo_width 0.6" FX

look_at console
wait_seconds 3

say "Three agents. Each handled its own part."

# Show FX briefly so effects are visible
focus_on reverb
wait_seconds 2

# ── Scene 10: Melody rewrite — agent rearranges over the same groove ────

scene "Melody rewrite"

say "The bass agent rewrites the melody on demand."
ask "rewrite the bass melody from scratch, keep the rhythm, more movement, new phrase" BASS 8
focus_on bass
wait_seconds 3
say "Same groove, different line."

# ── Scene 11: Scoped control ─────────────────────────────────────────────

scene "Scoped control"

ask "more resonance, darker, add slide" BASS

focus_on bass
wait_seconds 2

say "Bass changed. Drums and FX untouched."

# ── Scene 11: Creative direction ──────────────────────────────────────────

scene "Full energy"

show_all

ask "more energy" BASS
ask "more energy, busier" DRUMS
ask "bigger space" FX 10

wait_seconds 4

# ── Scene 12: Live filter with the band ──────────────────────────────────

scene "Live filter"

focus_on bass
sweep_pad 11
wait_seconds 1

# ── Scene 13: Outro ───────────────────────────────────────────────────────

scene "End"

show_all

say "Impulse Instruct."
say "Build your rack, wire AI agents and generate music with words."
say "Everything runs locally."

stop
