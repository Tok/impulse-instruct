#!/usr/bin/env bash
# ─── Intro Demo ─────────────────────────────────────────────────────────────
# Tour of Impulse Instruct. Sound within 30 seconds.
#
# What this version showcases:
#   - Two bass voices (V1 + V2 complementary lines)
#   - Full FX roster: delay, phaser, chorus, ring-mod
#   - LFO modulation
#   - Multi-agent band (BASS / DRUMS / FX specialists)
#
# NOTE: MC / NeuTTS is intentionally NOT part of this scenario — it doesn't
# fit an acid demo. It'll ship in the D&B / jungle scenario instead.

scene_count 13

# ═══════════════════════════════════════════════════════════════════════════════
# PART 1: Single agent — rack, sound, bass voices, FX parade, modulation
# ═══════════════════════════════════════════════════════════════════════════════

# ── Scene 1: Setup ──────────────────────────────────────────────────────────

scene "Setup"

reset_rack

say "Impulse Instruct."
pause 0.4
say "A smart synthesizer."
pause 0.6
say "Let's build a rack."

say "Drums."
bass_id=""
_808_id=$(add_instrument 808)
look_at 808
wait_seconds 1

say "Adding bass."
bass_id=$(add_instrument bass)
look_at bass
wait_seconds 1

say "More drums."
_909_id=$(add_instrument 909)
look_at 909
wait_seconds 1

say "Reverb."
reverb_id=$(add_effect reverb)
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
# Classic acid = sparse, squelchy, lots of space between notes.  Earlier
# runs were coming back with 10-14 bass hits / 32 steps which reads as
# a techno bassline, not acid.  Cap to 6-8 hits, call out slides +
# accents as the defining feature, and tell the drum agent to keep
# kicks on the 4 and hats skipping so the bass carries the groove.
ask "classic acid groove. sparse squelchy bass line, 6-8 hits across the full 32-step bank, 2-3 distinct pitches, heavy slides + accents for the 303 squelch. kicks on the 4 (steps 0,4,8,12,16,20,24,28), closed hats offbeat (2,6,10,...), open hats only on the pickup before each bar end. pan hats and snares wide for stereo image" "" 6

# Nudge the mix so the bass reads over the drums by default, and
# spread the kit across the stereo field so the track doesn't fold
# to a narrow centre.  The app's default kit volumes (kick 0.65,
# snare 0.60, hihats 0.75) dominate the bass (0.80) perceptually
# because the drum transients hit harder per-voice.  Previous runs
# still showed CLIPPING throughout the recording at 0.4 / 0.45 kit
# volumes + 0.85 master; dropping another ~25 % plus pulling the
# master down to 0.65 gives the sum enough headroom to stay below
# -1 dBFS even when 2 kits play dense 8ths simultaneously.  Pan
# offsets on hats/snare/clap widen the stereo image; `fx.stereo_width`
# at 0.85 amplifies the effect globally for delay/chorus returns.
api_params '{"kit_a":{"kick":{"volume":0.3,"pan":0.0},"snare":{"volume":0.28,"pan":-0.15},"hihat_closed":{"volume":0.32,"pan":0.35},"hihat_open":{"volume":0.32,"pan":-0.3}},"kit_b":{"kick":{"volume":0.3,"pan":0.0},"snare":{"volume":0.28,"pan":0.15},"hihat_closed":{"volume":0.32,"pan":-0.35},"hihat_open":{"volume":0.32,"pan":0.3},"clap":{"volume":0.3,"pan":-0.25}},"fx":{"master_volume":0.65,"stereo_width":0.85}}'

play
# Give the bass1 lane time to land — inference on a fresh acid-groove
# prompt can take 10–20 s and scene 3 used to fire before any bass
# notes were written, so the "cutoff and resonance" demo played over
# silent bass steps.  Waiting here keeps the pattern audible when we
# reach the filter-sweep scenes.
wait_seconds 10

screenshot "v${VERSION}-intro-sequencer"

# ── Scene 3: Two bass voices ────────────────────────────────────────────────

scene "Two bass voices"

# Scroll to the sequencer first so the viewer sees the V2 step row
# materialise as the agent enables the second voice — otherwise the
# narration describes a change that hasn't surfaced on screen yet.
look_at sequencer
wait_seconds 1

say "One bass is fine. Two is better."

# Enable voice 2 *via the API* before asking the agent to write for it.
# Why: the planner's `default_plan` skips `Bass(1)` when voice 2 is
# disabled, and the heuristic planner only accepts prompts ≤ 120 chars
# of raw text — so a long "enable voice 2" prompt either lands in an
# LLM planner that returns empty or a fallback plan with no bass2
# lane, and nothing ever writes `bass.enabled[1] = true`.  Flipping
# the flag here short-circuits that.  Pans are set at the same time
# for stereo separation; the ask then only needs to generate the
# actual pattern content the planner's bass2 lane can write.
api_params '{"bass_voices":[{"enabled":true,"pan":-0.25},{"enabled":true,"pan":0.25}]}'
pause 1

# Short, bass2-targeted prompt — under 120 chars so the heuristic
# planner routes it straight to the Bass(1) lane instead of the
# fallback plan which doesn't include bass2.
api_prompt "rewrite bass 2: counter-line, octave down, fewer notes, different rhythm from voice 1"
pause 3
say "Two voices. Different rhythms."
say "One pattern, played as a duet."
wait_seconds 6

screenshot "v${VERSION}-intro-two-voices"

# ── Scene 4: Acid filter sweep ──────────────────────────────────────────────
#
# Moved here (was scene 3) so the bass is actually playing when we
# demonstrate cutoff + resonance.  Scene 2's 10 s warm-up plus the
# two-bass-voices scene give bass1 and bass2 lanes time to land
# before the viewer sees the pad sweep.

scene "Acid filter sweep"

focus_on bass
wait_seconds 1

say "Cutoff and resonance."

sweep_pad 11

wait_seconds 1

screenshot "v${VERSION}-intro-bass-detail"

# ── Scene 5: AI-controlled ramp ─────────────────────────────────────────────

scene "AI-controlled ramp"

ask "slowly sweep the filter open over 4 bars" "" 14

say "The AI ramps parameters over bars."
say "Smooth, locked to the tempo."
wait_seconds 2

screenshot "v${VERSION}-intro-ramp"

# ── Scene 6: FX parade — delay, phaser, chorus, ring-mod ────────────────────
#
# Flow per effect: add module + scroll to it FIRST, then narrate the
# name while the fresh panel is on screen, then fire the ask so the
# audio change lands while the viewer's eyes are already on the right
# module.  The previous "say name → add → scroll → ask" order
# announced each effect before it was visible.

scene "FX parade"

show_all
say "Now the effects."

# Per-effect flow: add + scroll to panel (front), speak the name, fire
# the ask, then flip to the back for ~1.5 s to show the auto-wired
# cable bracing into the master chain, then flip back to knobs to see
# the next effect land.  The old flow only flipped to cables after all
# four FX were added, so the viewer couldn't associate each module
# with its new cable.

# Delay first — wired into the master chain automatically on add.
delay_id=$(add_effect delay)
look_at delay
say "Delay."
api_prompt "delay mix 0.22, delay time 0.375, feedback 0.35" FX
wait_seconds 3
show_cables
look_at delay
wait_seconds 2
show_knobs

phaser_id=$(add_effect phaser)
look_at phaser
say "Phaser."
api_prompt "phaser mix 0.5, rate 0.15, depth 0.6" FX
wait_seconds 3
show_cables
look_at phaser
wait_seconds 2
show_knobs

chorus_id=$(add_effect chorus)
look_at chorus
say "Chorus."
api_prompt "chorus mix 0.35, rate 0.2, depth 0.5" FX
wait_seconds 3
show_cables
look_at chorus
wait_seconds 2
show_knobs

ringmod_id=$(add_effect ringmod)
look_at ringmod
say "Ring modulator."
api_prompt "ringmod mix 0.15, frequency 220" FX
wait_seconds 3
show_cables
look_at ringmod
wait_seconds 2
show_knobs

# Front-side overview of the full FX lane.
look_at fxmod
wait_seconds 1
screenshot "v${VERSION}-intro-fx-rack"

# Flip to back — auto-wired audio chain is visible now.
say "The back panel shows how it's all wired."
show_cables
# Capture the rack immediately after the flip, before the tour starts
# panning — lets the changelog show the initial wired-up view.
pause 0.8
screenshot "v${VERSION}-intro-cables-flip"
tour_rack 1.4
screenshot "v${VERSION}-intro-cables"
show_knobs
wait_seconds 1

# Ease the ring-mod back so it doesn't dominate into the LFO scene.
api_params '{"ringmod": {"mix": 0.05}}'

# ── Scene 7: LFO modulation ─────────────────────────────────────────────────

scene "LFO modulation"

add_effect lfo
look_at lfo
wait_seconds 1

say "An LFO moves a parameter on its own."
say "Any knob can be a target."

# LFO 1 — sine on the bass cutoff.
#
# PULSE's `mod` lane jams constantly during playback and its outputs
# frequently use target strings the state enum doesn't accept
# ("filter_cutoff", "Filter Cutoff"), which silently reset the slot
# to `LfoTarget::None`.  Lock the slot immediately after setting so
# the jam can't overwrite while the demo is talking over it; unlock
# at end of the scene before tearing the rack down.
say "Sine on the bass cutoff."
api_params '{"lfo": [{"enabled": true, "target": "BassCutoff", "waveform": "Sine", "rate": 0.18, "depth": 0.55}]}'
lock "lfo[0]"
focus_on bass
wait_seconds 4

# LFO 2 — triangle on reverb mix (different slot, different waveform, different target).
add_effect lfo
look_at lfo
wait_seconds 1
say "A second LFO."
say "Triangle on the reverb mix."
api_params '{"lfo": [{}, {"enabled": true, "target": "ReverbMix", "waveform": "Triangle", "rate": 0.06, "depth": 0.35}]}'
lock "lfo[1]"
wait_seconds 4

say "Filter pulses fast. Reverb breathes slow."
wait_seconds 3

# Snapshot the LFO panel with both modulators live.
screenshot "v${VERSION}-intro-lfo"

# Release both LFOs before tearing down the single-agent rack.
unlock "lfo[0]" "lfo[1]"
api_params '{"lfo": [{"enabled": false}, {"enabled": false}]}'

# ═══════════════════════════════════════════════════════════════════════════════
# PART 2: Multi-agent band + TTS MC
# ═══════════════════════════════════════════════════════════════════════════════

# ── Scene 8: Multi-agent setup ──────────────────────────────────────────────

scene "Multi-agent setup"

wait_seconds 2
stop
wait_seconds 1

say "Split the AI into specialists."
say "Each one controls its own instruments."

reset_rack
add_instrument 808
wait_seconds 0.3
add_instrument bass
look_at bass
wait_seconds 0.5
add_instrument 909
look_at 909
wait_seconds 0.5
add_effect reverb
add_effect delay

# All agents share a single Gemma 4 E4B server (ref-counted in
# LlamaServerPool), so the whole band costs the same VRAM as one agent.
look_at console
wait_seconds 0.5
add_agent BASS gemma bass
wait_seconds 0.5
add_agent DRUMS gemma "kit_a,kit_b"
wait_seconds 0.5
add_agent FX gemma fx
wait_for_model

# Snapshot the three agents wired up before the band jam kicks off.
screenshot "v${VERSION}-intro-agents"

# ── Scene 9: Band jam ───────────────────────────────────────────────────────

scene "Band jam"

play
wait_seconds 1

ask "rewrite the bass melody from scratch: acid line with syncopation, squelchy, cutoff low, resonance high. Enable voice 2 as a counter-line an octave down, distinct rhythm, pan voices wide. Use 4 distinct scale pitches spread across BOTH halves of the bank" BASS
ask "kick on steps 0,4,8,12,16,20,24,28 pan center. hihat on 2,6,10,14,18,22,26,30 pan 0.3. clap on 4,12,20,28 pan -0.3. open hihat on 6,14,22,30" DRUMS
ask "reverb mix 0.12, reverb size 0.5, delay mix 0.08, stereo_width 0.6" FX

look_at console
wait_seconds 3

say "Three agents. Each handled its own part."
wait_seconds 2

screenshot "v${VERSION}-intro-band"

focus_on bass
wait_seconds 3

# ── Scene 10: Scoped control — three agents, three directions ──────────────

scene "Scoped control"

say "Each agent only touches its own zone."

# Three parallel asks: bass evolves (voice 1 AND voice 2 get fresh ideas),
# drums get busier, FX open up. Keeps the track moving.
ask "more energy, more slides, squelchier. rewrite voice 2 with a fresh counter-line, still an octave down but different rhythm" BASS
ask "more energy. add ratchets on the hats, busier clap pattern" DRUMS
ask "bigger space. reverb size 0.7, delay mix 0.15, stereo_width 0.8" FX 10

show_all
wait_seconds 4
say "Bass squelchier. Drums busier. Effects wider."
wait_seconds 3

focus_on bass
wait_seconds 3

# ── Scene 11: Lock / unlock — the user always wins ──────────────────────────

scene "Lock and unlock"

say "I can pin a knob so the AI can't touch it."

lock "bass.cutoff" "bass.resonance"
pause 1

# Freeze the pad at a sweet spot so the "nothing happens" moment is obvious.
api_params '{"bass": {"cutoff": 0.22, "resonance": 0.88}}'
wait_seconds 2

say "Cutoff and resonance are locked."
say "Asking for wild filter movement."
ask "wild filter sweep, cutoff low to high across the bar, max resonance, heavy drive" BASS 10

say "The knobs don't move. The agent can't override a lock."
wait_seconds 3

# Freeze-frame of the padlock state for the changelog / social posts.
screenshot "v${VERSION}-intro-locked"

say "Unlocking."
unlock "bass.cutoff" "bass.resonance"
pause 1
ask "wild filter sweep, cutoff low to high across the bar, max resonance" BASS 10
say "Now it can."
wait_seconds 3

# ── Scene 12: Live filter over the band ─────────────────────────────────────

scene "Live filter"

focus_on bass
say "I can also play the filter myself."
say "Manual performance over the autonomous band."

sweep_pad 14
wait_seconds 1

screenshot "v${VERSION}-intro-live-filter"

# One last bass variation so the outro isn't on a stale groove.
ask "one more pass on voice 1 — fresh phrase, keep the feel" BASS 6
wait_seconds 3

# ── Scene 13: End ───────────────────────────────────────────────────────────

scene "End"

show_all

say "Impulse Instruct."
say "Build your rack."
say "Wire up AI agents."
say "Create music with words."
say "Everything runs locally."
say "Free and open source."

# Don't chop the track off on the last narration line. No more asks will
# fire after this point — let the current bank(s) play out naturally so the
# music ends on a musical boundary, then stop cleanly and leave a short
# silence tail before the recording closes.
#
# ~12 s covers multiple full 32-step banks at any reasonable BPM.
wait_seconds 12
stop
wait_seconds 3
