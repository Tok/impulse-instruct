#!/usr/bin/env bash
# ─── J.S. Bach — Italian Concerto III (MIDI score import) ───────────────────
# Plays a 2-voice piano MIDI of Bach's third movement on two 303 voices
# at its intended 240 BPM, start to end (~3:30 of playback).
#
# Deliberately simple: score playback + filter sweeps + FX, not a jam.
#
#   - Bass score + sequencer stay LOCKED; nobody writes to them.
#   - Bass voices start on ENGINE DEFAULTS (known-good sound from
#     `./start.sh` → import MIDI).  During the 30 s intro, three
#     direct api_params nudges move cutoff / resonance a little so
#     there's audible filter motion before the drums arrive — no
#     LFOs (every LFO variant on the bass filter clicked on note
#     retriggers).
#   - 808 arrives at 30 s; a KIT agent writes a 32-step half-time
#     pulse and is re-asked twice during the outro to evolve the
#     groove.  Scope=kit_a keeps it away from the bass sequencer.
#   - FX outro is SCRIPTED, not agent-driven: three api_params nudges
#     ramp reverb + bitcrush in three stages.  Gemma's FX lane kept
#     emitting only reverb / ring_mod and never bitcrush keys, so
#     the bitcrush stayed at 0 in every prior take — scripted keeps
#     it predictable.  The whole bus passes through FX, so drums get
#     bitcrushed too.
#   - All 4 LFO slots are locked so no agent's mod lane can sneak a
#     modulation onto bass params; every non-reverb / non-bitcrush
#     FX channel is locked off so nothing else audible creeps in.
#   - Camera tracks each addition: focus_on the new module, linger a
#     few seconds, then return to the score.

scene_count 5

BACH_MIDI="${BACH_MIDI:-demo/scenarios/bach-italian-3rd.mid}"

# Helper: scroll to `$1`, pause `$2` seconds (default 4) so the viewer
# can see the module, then return to the sequencer view for continuity.
# Used after every add_instrument / add_effect so additions read as
# deliberate cuts instead of fly-bys.
show_then_return() {
    local target="$1"
    local linger="${2:-4}"
    focus_on "$target"
    wait_seconds "$linger"
    look_at sequencer
}

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_all
set_style ""

# Bach-specific Huth-per-component pinning: everything grayscale
# except the sequencer step dots + event-stream history, which are
# Huth-coloured unconditionally.  `show_thinking_in_log` is on so
# the KIT / FX agents' reasoning traces are visible.
set_ui_prefs '{"huth_piano": false, "huth_bar_osc": false, "huth_ring_osc": false, "huth_spectrum": false, "show_thinking_in_log": true, "enable_thinking": true}'

# Master volume stays at its default — the 0.60 override we had here
# was adding clicky transients on the bass with no audible gain in
# headroom (the FX agent ramps its own wet/depth, which bounds the
# outro loudness itself).

add_instrument bass
look_at ai

say "Bach's Italian Concerto, third movement. Two acid basses, two hands."

if ! load_midi "$BACH_MIDI"; then
    say "The MIDI file didn't load. Check that the app is running and the file exists."
    stop
    exit 1
fi

PLAY_DURATION="$MIDI_DURATION"
PLAY_DURATION_INT=${PLAY_DURATION%.*}
echo "  [scenario] will play for ${PLAY_DURATION_INT}s at ${MIDI_BPM} BPM"

# Starting timbre: DEFAULTS on the bass filter / env.  Earlier takes
# that tweaked env_mod / decay or enabled BassCutoff / BassResonance
# LFOs came back clicky.  `./start.sh` → import MIDI → default 303
# is the known-good sound; this scenario reproduces it.  Cutoff and
# resonance move during Scene 3 via `sweep_pad` (scripted api_params,
# no LFO — the helper has been proven clean by the intro demo).

# Pan LFO only: slow stereo drift on the bass bus.  Pan-only LFOs
# don't retrigger with the filter envelope and have never caused the
# clicks we saw with BassCutoff / BassResonance LFOs.  Locked below
# so nothing rewires it.
api_params '{
  "lfo": [
    { "enabled": true, "target": "BassPan", "waveform": "Sine", "rate": 0.28, "depth": 0.70, "phase_offset": 0.00 }
  ]
}'

# Lock the imported score and every FX bus channel except the two we
# want audible (reverb + bitcrush).  The fx state is a flat struct —
# delay / chorus / phaser / ring_mod / compressor / distortion /
# tape / waveshaper / eq / autotune all run in the DSP regardless of
# which modules are in the rack.  Last run the FX agent kept writing
# ring_mod_mix despite the prompt saying reverb+bitcrush only, and
# since no ring-mod module was visible, it sounded like "invisible
# FX".  Locking their mix/gain paths here means the agent can't turn
# any of them on, so what's audible matches what's in the rack.
# All four lfo slots are locked too — the planner keeps picking the
# mod lane for any scoped agent, and an unlocked slot lets an LFO
# onto something like `bass_voices.1.env_mod` that then reads as
# "invisible LFO making the bass bleeP".
lock \
    "sequencer.bass_pattern" \
    "sequencer.bass_patterns" \
    "sequencer.bass_steps" \
    "sequencer.bass2_steps" \
    "sequencer.bass3_steps" \
    "sequencer.bass4_steps" \
    "sequencer.bass_voice_steps" \
    "sequencer.bass_notes" \
    "sequencer.bass_accents" \
    "sequencer.bass_slides" \
    "sequencer.bass_pans" \
    "sequencer.bass2_notes" \
    "sequencer.bass2_accents" \
    "sequencer.bass2_slides" \
    "sequencer.bass2_pans" \
    "sequencer.bass3_notes" \
    "sequencer.bass3_accents" \
    "sequencer.bass3_slides" \
    "sequencer.bass3_pans" \
    "sequencer.bass4_notes" \
    "sequencer.bass4_accents" \
    "sequencer.bass4_slides" \
    "sequencer.bass4_pans" \
    "sequencer.bpm" \
    "sequencer.steps" \
    "lfo[0]" "lfo[1]" "lfo[2]" "lfo[3]" \
    "fx.delay_mix" "fx.chorus_mix" "fx.phaser_mix" "fx.ring_mod_mix" \
    "fx.waveshaper_mix" "fx.distortion_mix" "fx.compressor_mix" \
    "fx.tape_mix" \
    "fx.eq_low_gain" "fx.eq_mid_gain" "fx.eq_hi_gain"

show_then_return bass 4

# ── Scene 2: Play ──────────────────────────────────────────────────────────
# No PAD agent.  The three LFOs configured in Setup sweep pan /
# cutoff / resonance natively — that's the filter performance.
# AI narrative for this scenario lives on the KIT + FX agents added
# below.

scene "Play"

look_at sequencer
play

# ── Scene 3: Drums arrive (+60s) ────────────────────────────────────────────
# KIT writes a 32-step half-time pulse under the 240 BPM bass, and
# gets re-asked twice during the outro so the groove evolves rather
# than loops unchanged.  Scope=kit_a keeps it away from the bass
# sequencer.  chain_advance_preserve_non_bass carries the drums
# across bank swaps so they play for the rest of the piece.

scene "Drums arrive"

# 30 s of Bach before the 808 layers in.  Smoothly sweep the filter
# pad via the shared `sweep_pad` helper — same motion the intro /
# default acid demo uses, lerped keyframes at ~7.5 fps so cutoff and
# resonance move together in a visible arc on the XY pad.  No LFO
# (LFOs on BassCutoff kept clicking on note retriggers); this is
# pure scripted api_params inside the helper, which we've proven
# doesn't click.  `sweep_pad` writes to voice 0 only (matches the
# acid demo) — voice 1 stays at defaults so the left-hand line keeps
# a stable timbre under the sweeping right hand.
look_at bass
wait_seconds 4
sweep_pad 22
look_at sequencer
wait_seconds 4

add_instrument 808
show_then_return 808 5

# Drums below Bach + stretch kit_a voices to 32 steps so the kick /
# hat / snare rows span two bars of the 64-step bass pattern instead
# of looping inside a 16-step window (which read as "only half a
# bar" under the moving score).
api_params '{
  "kit_a": {"kick": {"volume": 0.45}, "hihat_closed": {"volume": 0.30}},
  "sequencer": {"drum_lengths": {"kick_a": 32, "snare_a": 32, "hihat_a": 32, "hihat_a_open": 32}}
}'

add_agent KIT gemma "kit_a"
wait_for_model

# First ask: sparse 32-step half-time pulse.  Explicit step targets so
# the model has concrete positions instead of drifting to a dense
# 4-on-the-floor that'd bury Bach.
ask "KIT: half-time drums under a 240 BPM Bach score.  Kit_a only, 32-step loop (two bars).  Kick on steps 0, 8, 16, 24.  Closed hi-hat on steps 4, 12, 20, 28.  NO snare on 2 and 4 — this is not pop.  Add two or three quiet ghost hits somewhere unusual for texture across the 32 steps (a soft hat on a weird step, a single tom ping, a muted kick).  Everything quiet, velocity ~0.4 max.  Half the speed of the bass — a slow pulse under a running melodic line, not a dance beat.  DO NOT touch kit_b / hoover / an1x / bass — kit_a only." KIT 0

# ── Scene 4: FX outro ───────────────────────────────────────────────────────
# FX is now DRIVEN DIRECTLY from bash via api_params, not by an FX
# agent.  Reason: the FX lane of Gemma 4 reliably emits `reverb_*`
# and `ring_mod_*` but never `bitcrush_*` in its raw output, no
# matter how explicitly the prompt spells the field names.  Three
# consecutive prior recordings all had bitcrush stuck at its default
# (rate = 0, mix = 0) because the agent never wrote the keys.
# Scripted api_params gives a predictable escalation — three nudges,
# roughly 30 s apart, with each one bumping the three bitcrush
# channels together so the crunch builds audibly into the ending.

scene "FX outro"

# Align FX outro so stage 3 ends ~21 s before the MIDI's natural stop
# (previous cut used 15 s of headroom; TTS was still landing 6 s
# after music, so another 6 s earlier).  Stages are 30 + 30 + 32 =
# 92 s; drums phase was 30 s; everything else comes out of this
# pre-FX fill.
FX_START_OFFSET=$(echo "$PLAY_DURATION - 30 - 92 - 21" | bc -l)
# Long stretch between drums-arrive and FX-outro — fill it with
# camera motion instead of staring at the sequencer.
fill_wait "$FX_START_OFFSET"

# Add the visible FX modules.  Both reverb and bitcrush are drawn to
# the rack so what the viewer sees matches what they hear — the bus
# DSP runs reverb unconditionally whenever fx.reverb_mix > 0, so
# without a reverb module visible the previous take had "invisible
# reverb".  Other FX channels are locked above so nothing else
# audible can sneak in.
add_effect reverb
show_then_return reverb 4
enable_fx reverb
add_effect bitcrush
show_then_return bitcrush 4
enable_fx bitcrush

# FX field-name note: state keys are `fx.bitcrush_mix`,
# `fx.bitcrush_bits`, `fx.bitcrush_rate`, `fx.reverb_mix`,
# `fx.reverb_size`.  `bitcrush_bits` is INVERTED: 1.0 = full quality
# (bypass), 0.0 = 1-bit.  So "more crunch" means bits DOWN.

# Stage 1: clearly audible bitcrush + medium reverb.  Previous values
# were too subtle — bumped bitcrush_mix higher and bits lower so the
# crunch is unmistakable from the first stage.  KIT gets nudged to
# evolve so the drums don't loop a single two-bar pattern through
# the whole outro.
api_params '{"fx":{"reverb_mix":0.40,"reverb_size":0.60,"bitcrush_mix":0.45,"bitcrush_bits":0.55,"bitcrush_rate":0.40}}'
ask "KIT: evolve the 32-step groove.  Keep it sparse and half-time (kick on 0/8/16/24 still anchors it), but add a couple of extra ghost kicks or off-grid hats — maybe an anticipation before the downbeat (step 7 or 23), or a pair of 16th hats somewhere in the second bar.  Still no backbeat snare.  32-step loop, velocities ~0.4 max, kit_a only." KIT 0

look_at fx
wait_seconds 30

# Stage 2: heavier crunch + fuller reverb.
api_params '{"fx":{"reverb_mix":0.60,"reverb_size":0.80,"bitcrush_mix":0.75,"bitcrush_bits":0.25,"bitcrush_rate":0.70}}'
ask "KIT: push the groove further.  Stay 32 steps, half-time, kit_a only.  Vary the kick placement (can drop one of 0/8/16/24 for a breath, add one at 11 or 27 for tension), let the hats get a bit more active in the second bar.  One sparse snare ghost is allowed now — somewhere unexpected like 14 or 22, never on 4 or 12.  Velocities still quiet." KIT 0

look_at fx
wait_seconds 30

# Stage 3: full chaos — hard digital crunch + cavernous reverb.
# Stage 3 is 32 s so the final bitcrush values sit long enough to
# actually register as the "collapse" moment of the piece.
api_params '{"fx":{"reverb_mix":0.85,"reverb_size":1.00,"bitcrush_mix":1.00,"bitcrush_bits":0.05,"bitcrush_rate":0.92}}'

look_at fx
wait_seconds 32

# ── Scene 5: End ────────────────────────────────────────────────────────────
# chain_loop=false stops the transport on its own at the end of the
# last bank.  `stop` defensively; the End-scene cleanup (stop_pan_lfo)
# sweeps any stray python loops the scenario didn't start, just in
# case a prior run left one.

scene "End"

stop
stop_pan_lfo

say "Bach, full stop. Eighteenth-century counterpoint, played by two acid basses, smeared by a bitcrusher — all driven by local AI."
wait_seconds 2
