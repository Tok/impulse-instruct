#!/usr/bin/env bash
# ─── Liquid Drum & Bass Demo ────────────────────────────────────────────────
# 174 BPM, liquid feel — aggressive chopped amen up front, slow sustained
# sub-bass and jazzy piano stabs sitting underneath.  The contrast IS the
# genre: the break does the work, the bass + piano just float.
#
# Setup is deliberately minimal:
#   - AmenSampler   — drum backbone (chopped + edited by the agent)
#   - AcidBass      — sub-bass: long release, slide-into notes, low cutoff
#   - SampleInstrument — Salamander Grand A4 sample, pitched across the
#                        keyboard via ratio resampling
#   - FxReverb / FxDelay — long plate wash + dotted-eighth tape echo
#   - DNB agent     — writes the amen break ONLY
#   - MC + NeuTTS   — short shout-out at the tail
#
# DESIGN DECISION — bass and piano are HARDCODED, not LLM-written.
# Earlier passes asked the agent to write the bass pattern with rules
# like "exactly 4-5 active steps with slides on all of them, A2-E3
# range, A minor only".  In practice the model wrote 14 dense steps
# wandering chromatically, slides as `[0,1,2,3]` (interpreting the
# slide format in three different ways), and tried repeatedly to
# overwrite the locked bass.waveform with Supersaw.  Every run came
# back chaotic and off-key.  The LLM is great at drum-pattern
# variation (its strongest lane) but the liquid-bass brief — sparse,
# legato, on-key, long-sustain — is too narrow for it to hit
# reliably.
#
# So: the bass + piano patterns are written deterministically via
# `api_params` at setup, the relevant `sequencer.bass_*` and
# `sequencer.sample_*` paths are LOCKED, and the agent only gets to
# rewrite the amen.  The bass MANIPULATION (filter sweeps, drive
# bumps, pattern shifts) is also scripted — every run sounds the
# same musically but the timbre evolves visibly across scenes.
#
# `scale_snap` + Natural Minor at A is on globally, so any LLM
# attempt to write a bass / piano note still gets snapped onto key
# even if the locks somehow leak.  Belt and suspenders.
#
# Reference: edmprod / mixedinkey / musicradar liquid-DnB production
# guides — sub-sine bass + glide + long sustained notes are timbre
# choices, not melodic ones.

scene_count 11

# ── Scene 1: Intro ──────────────────────────────────────────────────────────

scene "Intro"

reset_all
set_style drum_and_bass

say "Impulse Instruct — liquid drum and bass at 174. Chopped amen up front, slow sub-bass and piano underneath."
wait_seconds 1

# ── Scene 2: Setup ──────────────────────────────────────────────────────────

scene "Setup"

amen_id=$(add_instrument amen)
add_instrument bass
sample_id=$(add_instrument sample)
reverb_id=$(add_effect reverb)
delay_id=$(add_effect delay)

# Parallel sends: amen and piano both feed the reverb wash; piano also
# runs through the dotted-eighth delay for that classic D&B echo trail.
[ -n "$amen_id" ] && [ -n "$reverb_id" ] && api_rack_cable "$amen_id" "$reverb_id" "audio"
[ -n "$sample_id" ] && [ -n "$reverb_id" ] && api_rack_cable "$sample_id" "$reverb_id" "audio"
[ -n "$sample_id" ] && [ -n "$delay_id" ] && api_rack_cable "$sample_id" "$delay_id" "audio"

# Piano: Salamander Grand A4v12 single FLAC, ratio-resampled across
# the keyboard.  Salamander's V3 SFZ uses ARIA #include macros that
# our parser doesn't expand (logs `parsed to zero playable regions`)
# — single-sample is reliable.
load_sample "samples/instruments/SalamanderGrandPiano/Samples/A4v12.flac"

# ── Pin timbres + global music settings ────────────────────────────────────
# Bass: sub-bass character + long sustain.  Piano: short attack, long
# release for chord wash.  Sequencer: A natural minor key with
# scale_snap on so any stray note (LLM or otherwise) lands on key.
api_params '{
    "sequencer": {
        "scale_snap": true,
        "scale": "NaturalMinor",
        "root_note": 9
    },
    "bass": {
        "waveform": "Saw",
        "cutoff": 0.32,
        "resonance": 0.20,
        "env_mod": 0.0,
        "distortion": 0.0,
        "supersaw_voices": 1,
        "supersaw_detune": 0.0,
        "sub_osc_level": 0.65,
        "amp_attack": 0.05,
        "amp_sustain": 1.0,
        "amp_release": 0.92,
        "portamento_time": 0.5,
        "volume": 1.0
    },
    "sample": {
        "attack": 0.005,
        "decay": 0.3,
        "sustain": 0.85,
        "release": 0.7,
        "volume": 0.75
    },
    "amen": {
        "volume": 0.55,
        "gate": 0.85,
        "loop_mode": true
    }
}'

# ── Pre-write the bass pattern (hardcoded, not LLM-driven) ────────────────
# 4 active steps spread evenly around the bar, slides on all of them
# (so the voice glides legato into each note), notes drawn from
# A natural minor — A2 (45), E3 (52), A2 (45), C3 (48).
#
# `bass_notes` is per-position (32 entries); inactive positions get
# the same A2 fallback so any glitch step still plays in key.
api_params '{
    "sequencer": {
        "bass_steps":  [0, 8, 16, 24],
        "bass_slides": [0, 8, 16, 24],
        "bass_notes":  [45,45,45,45,45,45,45,45, 52,52,52,52,52,52,52,52, 45,45,45,45,45,45,45,45, 48,48,48,48,48,48,48,48],
        "bass_accents": [0],
        "bass_pans":   [0.0,0,0,0,0,0,0,0, 0.18,0,0,0,0,0,0,0, 0.0,0,0,0,0,0,0,0, -0.15,0,0,0,0,0,0,0]
    }
}'

# ── Pre-write the piano pattern (hardcoded) ───────────────────────────────
# 5 active steps at 0 / 6 / 12 / 20 / 28 — sparse, syncopated against
# the break.  Notes are an A minor 9 voicing in the upper register:
# A4 (69), E5 (76), G5 (79), C5 (72), B5 (83).  sample_steps takes
# bool arrays only (no index-list path), so spell out all 32 entries.
api_params '{
    "sample": {
        "sample_steps": [true,false,false,false,false,false,true,false,false,false,false,false,true,false,false,false,false,false,false,false,true,false,false,false,false,false,false,false,true,false,false,false],
        "sample_notes": [69,69,69,69,69,69,76,69,69,69,69,69,79,69,69,69,69,69,69,69,72,69,69,69,69,69,69,69,83,69,69,69]
    }
}'

# ── Default amen pattern (fallback in case the agent leaves it empty) ─────
# Classic two-step: kick on 0/16, snare on 8/24, ghost hits in between.
# `amen_steps` as an index-list (< 16 entries) clears all and
# activates listed positions.  Slices vary across hits so the break
# isn't linear.
api_params '{
    "sequencer": {
        "amen_steps":  [0, 7, 8, 14, 16, 22, 24, 30],
        "amen_slices": [1, 5, 2, 7, 3, 8, 4, 6]
    }
}'

# ── Lock everything we want stable ────────────────────────────────────────
# Timbre fields + bass + piano patterns.  The agent can still rewrite
# the AMEN step + slice arrays (its strength).  cutoff and distortion
# are also locked here but get briefly UNLOCKED in scenes 5/6 for the
# manipulation showcase, then re-locked.
lock "bass.waveform" "bass.supersaw_voices" "bass.supersaw_detune" \
     "bass.sub_osc_level" "bass.env_mod" \
     "bass.amp_attack" "bass.amp_sustain" "bass.amp_release" \
     "bass.portamento_time" "bass.cutoff" "bass.resonance" "bass.distortion" \
     "bass.volume" \
     "sequencer.bass_steps" "sequencer.bass_notes" "sequencer.bass_slides" \
     "sequencer.bass_accents" "sequencer.bass_pans" \
     "sequencer.sample_steps" "sequencer.sample_notes" \
     "sequencer.amen_steps" "amen.volume" "amen.gate"

add_agent DNB gemma
wait_for_model

set_bpm 174

# Start the sequencer NOW, before the rest of the setup narration.
# In earlier runs the sequencer didn't start until scene 3, leaving
# scenes 1+2 (~26 s of narration) completely silent — operators
# routinely pressed the amen panel's ▶ button thinking the break
# was broken.  Starting playback at the end of setup means the
# hardcoded amen / bass / piano patterns are audibly running as soon
# as the rack is built, and every later scene narration plays over
# music rather than over silence.
play

say "Amen, bass tuned to sub character, piano with a long tail, reverb and delay. Bass and piano patterns are hardcoded; the AI handles the break."

# ── Scene 3: First break ────────────────────────────────────────────────────

scene "First break"

look_at sequencer

wait_seconds 2

# Default amen pattern (from setup) is already audible.  The agent's
# only job here is to vary the slice picks via `sequencer.amen_slices`
# — the step pattern itself is locked because previous runs had the
# agent write 16-element bool arrays (mid-bar overwrite of our 32-step
# pattern) or empty arrays that the safeguard had to drop.  Leaving it
# free for slice variation gives the chop life without risking the
# break going silent.
ask "vary the chop on the amen break.  Write ONLY sequencer.amen_slices as a 32-element array of slice indices (1..=8) — pick a different slice on every active step so the break sounds chopped and edited rather than linear.  Do NOT touch sequencer.amen_steps (it's locked).  Do NOT touch bass or sample lanes."

say "Chopped amen running. Two-step backbone with rolling fills."
wait_seconds 4

# ── Scene 4: Sub-bass character ────────────────────────────────────────────

scene "Sub-bass character"

focus_on bass

# No new ask — the bass is already playing the hardcoded liquid line.
# Let the viewer see and hear it under the break.
say "Bass underneath — four notes around the bar, slides between every one, long release tail. A minor: root, fifth, root, minor third."
wait_seconds 5

# ── Scene 5: Filter sweep ──────────────────────────────────────────────────
# Open the bass low-pass filter slowly across ~10 s, then close it
# back down.  Same notes the whole time; the FILTER is what evolves.
# Done as a ramp of api_params calls so the knob visibly moves.

scene "Filter sweep"

# Briefly unlock cutoff so the sweep can write through.
unlock "bass.cutoff"

say "Opening the filter — same notes, more harmonic content."

# Manual interpolation: 20 keyframes over ~10 s, cosine-ish curve from
# 0.32 (sub) → 0.62 (mid + bite) → 0.32 (back home).  Each step waits
# 0.5 s so the host-to-audio update lands smoothly.  Inline python so
# we don't need to add a helper to lib.sh.
python3 -u -c "
import math, time, subprocess
api = 'http://127.0.0.1:8765/api/params'
total_steps = 20
duration = 10.0
dt = duration / total_steps
for i in range(total_steps + 1):
    t = i / total_steps
    # half-sine: 0 → 1 → 0 over the duration
    s = math.sin(t * math.pi)
    cutoff = 0.32 + s * 0.30
    body = '{\"params\":{\"bass\":{\"cutoff\":%.3f}},\"quiet\":true}' % cutoff
    subprocess.run(['curl', '-sf', '-X', 'POST', api,
        '-H', 'Content-Type: application/json', '-d', body],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(dt)
" 2>/dev/null

# Settle back and re-lock.
api_params '{"bass": {"cutoff": 0.32}}'
lock "bass.cutoff"

say "Filter back down. Pattern never changed."
wait_seconds 2

# ── Scene 6: Drive bump ────────────────────────────────────────────────────
# Push the bass distortion from clean (0) up to a moderate dirty
# (0.40) and back to clean over ~8 s.  Adds harmonic edge during
# the build-up without changing notes.

scene "Drive bump"

unlock "bass.distortion"

say "Pushing the bass into the drive. Adding harmonic grit."

python3 -u -c "
import math, time, subprocess
api = 'http://127.0.0.1:8765/api/params'
total_steps = 16
duration = 8.0
dt = duration / total_steps
for i in range(total_steps + 1):
    t = i / total_steps
    s = math.sin(t * math.pi)
    drive = s * 0.40
    body = '{\"params\":{\"bass\":{\"distortion\":%.3f}},\"quiet\":true}' % drive
    subprocess.run(['curl', '-sf', '-X', 'POST', api,
        '-H', 'Content-Type: application/json', '-d', body],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(dt)
" 2>/dev/null

api_params '{"bass": {"distortion": 0.0}}'
lock "bass.distortion"

say "Back to clean. Same notes throughout."
wait_seconds 2

# ── Scene 7: Bass pattern shift ────────────────────────────────────────────
# Move the active steps onto syncopated positions for variety.  Notes
# stay in A natural minor — root, fifth, minor seventh, fourth (D3).
# Locks are released for one write, then reapplied.

scene "Bass pattern shift"

look_at sequencer

unlock "sequencer.bass_steps" "sequencer.bass_slides" "sequencer.bass_notes" "sequencer.bass_pans"

# 4 syncopated active positions with slides on all of them, walking
# A → E → G → D (root → fifth → min7 → fourth) across the bar.
api_params '{
    "sequencer": {
        "bass_steps":  [2, 11, 18, 27],
        "bass_slides": [2, 11, 18, 27],
        "bass_notes":  [45,45,45,45,45,45,45,45, 52,52,52,55,52,52,52,52, 45,45,50,45,45,45,45,45, 48,48,48,48,52,52,52,52],
        "bass_pans":   [0.1,0,0.1,0,0,0,0,0, 0.0,0,0,-0.1,0,0,0,0, 0.2,0,0.0,0,0,0,0,0, -0.2,0,0,0,0.05,0,0,0]
    }
}'

lock "sequencer.bass_steps" "sequencer.bass_slides" "sequencer.bass_notes" "sequencer.bass_pans"

say "Shifted the bass off the grid — same key, syncopated against the break."
wait_seconds 6

# ── Scene 8: Liquid piano + FX bed ─────────────────────────────────────────
# Pin the FX bed: long plate reverb, dotted-eighth tape delay with
# moderate feedback, ~75 % stereo width.

scene "Liquid piano and FX bed"

look_at fxmod

api_params '{"fx": {
    "reverb_mix": 0.28,
    "reverb_size": 0.72,
    "reverb_damp": 0.4,
    "delay_mix": 0.20,
    "delay_time": 0.1875,
    "delay_feedback": 0.50,
    "delay_saturation": 0.3,
    "stereo_width": 0.75,
    "master_volume": 0.85
}}'

say "Long plate reverb, dotted-eighth tape delay. The piano stabs blur into a chord wash."
wait_seconds 5

# ── Scene 9: Re-chop the break ─────────────────────────────────────────────

scene "Re-chop the break"

look_at sequencer

ask "rewrite ONLY sequencer.amen_slices — pick a fresh 32-element slice-index pattern (1..=8) different from the previous pass.  Lean into edit-era D&B chopping: alternating slices, occasional repeats for stutter feel, no two consecutive steps using the same slice.  Do NOT touch sequencer.amen_steps (locked).  Do NOT touch bass or sample lanes." "" 10

say "Fresh chop. Same backbone, different fill."
wait_seconds 4

# ── Scene 10: MC ───────────────────────────────────────────────────────────

scene "MC on the mic"

add_agent MC gemma "" mc tts
wait_seconds 3

ask "drop one short, smooth shout-out — liquid D&B vibe, single line, atmospheric not hyped" MC 10

say "MC steps up. Local Gemma writing the line, NeuTTS speaking it."
wait_seconds 25

# ── Scene 11: End ──────────────────────────────────────────────────────────

scene "End"

# Release ALL locks so the next session inherits a clean state.
unlock "bass.waveform" "bass.supersaw_voices" "bass.supersaw_detune" \
       "bass.sub_osc_level" "bass.env_mod" \
       "bass.amp_attack" "bass.amp_sustain" "bass.amp_release" \
       "bass.portamento_time" "bass.cutoff" "bass.resonance" "bass.distortion" \
       "bass.volume" \
       "sequencer.bass_steps" "sequencer.bass_notes" "sequencer.bass_slides" \
       "sequencer.bass_accents" "sequencer.bass_pans" \
       "sequencer.sample_steps" "sequencer.sample_notes" \
       "sequencer.amen_steps" "amen.volume" "amen.gate"

say "Liquid drum and bass at 174 — chopped amen, slow sub, jazzy piano. All running locally."
wait_seconds 3

stop
