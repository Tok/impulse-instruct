// ─── llm/prompt.rs ───────────────────────────────────────────────────────────
// Builds the system prompt that grounds the LLM in current synth state.

use crate::llm::styles::StyleCatalog;
use crate::state::{AppState, ConversationMode, ROOT_NAMES, StyleVerbosity};

/// Returns the system prompt. If the user has set a non-empty `system_prompt_override`,
/// that is returned verbatim — giving full control over the model's grounding.
pub fn build_system_prompt(state: &AppState, scope: &[String]) -> String {
    if !state.llm.system_prompt_override.trim().is_empty() {
        return state.llm.system_prompt_override.clone();
    }
    let locked: Vec<&str> = state.llm.locked_params.iter().map(|s| s.as_str()).collect();
    let locked_str = if locked.is_empty() {
        "none".to_string()
    } else {
        locked.join(", ")
    };

    let focused: Vec<&str> = state
        .llm
        .focused_params
        .iter()
        .map(|s| s.as_str())
        .collect();
    let focused_str = if focused.is_empty() {
        String::new()
    } else {
        format!(
            "\nFOCUS (user wants LLM to actively drive these — prioritise movement here): {}\n",
            focused.join(", ")
        )
    };
    let heat = state.llm.heat;
    let heat_pct = (heat * 100.0) as u32;
    let heat_desc = match heat {
        h if h < 0.25 => "cold — subtle incremental changes only, no pattern mutations",
        h if h < 0.5 => "warm — moderate evolution, occasional step changes",
        h if h < 0.75 => "hot — bold sweeps, pattern mutations, noticeable style shifts",
        h if h < 0.9 => "fire — anything goes, dramatic mutations, surprise",
        _ => {
            "CHAOS — maximum disorder; shatter patterns, use extreme settings, ignore convention, be completely unpredictable"
        }
    };

    // Summarise active bass steps so the LLM can see what's playing
    let active_bass: Vec<usize> = state
        .sequencer
        .bass_pattern
        .iter()
        .enumerate()
        .filter(|(_, s)| s.active)
        .map(|(i, _)| i)
        .collect();
    let bass_summary = if active_bass.is_empty() {
        "none (silent)".to_string()
    } else {
        active_bass
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };

    let current_json = serde_json::to_string_pretty(&serde_json::json!({
        "bass": {
            "cutoff": state.bass_voices[0].synth.cutoff,
            "resonance": state.bass_voices[0].synth.resonance,
            "env_mod": state.bass_voices[0].synth.env_mod,
            "decay": state.bass_voices[0].synth.decay,
            "accent_level": state.bass_voices[0].synth.accent_level,
            "waveform": format!("{:?}", state.bass_voices[0].synth.waveform),
            "filter_mode": format!("{:?}", state.bass_voices[0].synth.filter_mode),
            "distortion": state.bass_voices[0].synth.distortion,
            "volume": state.bass_voices[0].synth.volume
        },
        "sequencer": {
            "bpm": state.sequencer.bpm,
            "swing": state.sequencer.swing,
            "root_note": state.sequencer.root_note,
            "scale": state.sequencer.scale.name(),
            "scale_snap": state.sequencer.scale_snap
        },
        "fx": {
            "reverb_mix": state.fx.reverb_mix,
            "delay_mix": state.fx.delay_mix,
            "distortion_drive": state.fx.distortion_drive,
            "distortion_mix": state.fx.distortion_mix,
            "chorus_mix": state.fx.chorus_mix
        },
        "free_eg": {
            "enabled": state.free_eg.enabled,
            "period": state.free_eg.period,
            "depth": state.free_eg.depth,
            "target": format!("{:?}", state.free_eg.target),
            "loop_mode": state.free_eg.loop_mode,
            "values": state.free_eg.values.as_slice()
        }
    }))
    .unwrap_or_default();
    // Bass pattern summary shown separately so the model doesn't treat it as an output field
    let bass_info = format!(
        "Active bass steps (for reference only, not a JSON field): {}",
        bass_summary
    );

    // Resolve active style section (empty string if none set)
    let style_section = match state.llm.active_style.as_deref() {
        None => String::new(),
        Some("__free__") =>
            "\n═══ ACTIVE STYLE ═══\n\nFree mode — no style constraints. \
             Be creative and unpredictable. Experiment freely with sound and rhythm. \
             Surprise the listener. Choose any musical direction that feels interesting \
             and don't hold back.\n".to_string(),
        Some("__custom__") => {
            let desc = state.llm.custom_style_text.trim();
            if desc.is_empty() {
                String::new()
            } else {
                format!(
                    "\n═══ ACTIVE STYLE ═══\n\n{}\n\nUse this as your creative brief. \
                     Evolve the current sound toward this aesthetic — don't reset everything at once.\n",
                    desc
                )
            }
        }
        Some(id) => StyleCatalog::get().find_by_id(id)
            .map(|s| {
                let text = match state.llm.style_verbosity {
                    StyleVerbosity::Brief if !s.brief.is_empty() => s.brief.as_str(),
                    _ => s.description.as_str(),
                };
                let seed = if !s.seed_patterns.is_empty() {
                    format!("\nSeed patterns (concrete starting point — adapt freely):\n{}\n", s.seed_patterns.to_prompt_lines())
                } else {
                    String::new()
                };
                let tonality = match (&s.suggested_root, &s.suggested_scale) {
                    (Some(root), Some(scale)) => {
                        const NAMES: [&str; 12] = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
                        format!("\nSuggested tonality: {} {} — prefer scale-coherent bass notes and set root_note/scale accordingly.\n",
                            NAMES[(*root as usize) % 12], scale)
                    }
                    _ => String::new(),
                };
                format!(
                    "\n═══ ACTIVE STYLE ═══\n\n{}{}{}\nThis is your creative brief. \
                     RESET parameters to fully match this genre — set BPM, patterns, synth params, \
                     and FX from scratch. Do not carry over settings from a previous style.\n",
                    text, seed, tonality
                )
            })
            .unwrap_or_default(),
    };

    // Inject user instructions when set
    let user_instructions_section = {
        let s = state.llm.user_instructions.trim();
        if s.is_empty() {
            String::new()
        } else {
            format!("\n═══ USER INSTRUCTIONS ═══\n{}\n", s)
        }
    };

    let persona = state.llm.persona_name.trim();
    let persona = if persona.is_empty() { "PULSE" } else { persona };

    let scope_section = if scope.is_empty() {
        String::new()
    } else {
        format!(
            "\n═══ SCOPE CONSTRAINT ═══\nYou control ONLY: {}. Do NOT emit top-level keys outside your scope.\n",
            scope.join(", ")
        )
    };

    let autonomy_section = if state.llm.agent_autonomy {
        "\n═══ AUTONOMY ═══\n\
         You may use spawn_agent to invite a friend (another AI agent) to help — \
         announce it in _comment first (e.g. \"bringing in a bass specialist\").\n\
         You may use dismiss to sign off when you feel done — say goodbye in _comment first.\n\
         The last remaining agent cannot dismiss itself.\n"
            .to_string()
    } else {
        String::new()
    };

    // Music theory context — computed once, embedded in the prompt
    let root_note = state.sequencer.root_note;
    let root_name = ROOT_NAMES[root_note as usize % 12];
    let scale_name = state.sequencer.scale.name();
    let scale_c2_c3 = {
        use crate::state::scale_notes;
        let notes = scale_notes(root_note, state.sequencer.scale);
        notes
            .iter()
            .filter(|&&n| (36..=60).contains(&n))
            .map(|&n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!(
        r#"You are {persona} — the AI intelligence inside Impulse Instruct, a hardware-style synthesizer.
Output ONLY valid JSON. No prose, no markdown, no explanation outside the "_comment" field.
{style_section}{user_instructions_section}{scope_section}{autonomy_section}
CURRENT STATE:
{current_json}
{bass_info}

LOCKED (user-owned, never include in output): {locked_str}
{focused_str}
═══ WHAT YOU CAN CONTROL ═══

BASS SYNTHESIZER (all 0.0–1.0):
  bass.cutoff       — filter frequency (0=very dark/closed, 0.5=mid, 1=fully open)
  bass.resonance    — filter resonance / squelch (0.7–0.9 = classic acid character)
  bass.env_mod      — how much the envelope opens the filter (high = dramatic sweep)
  bass.decay        — filter envelope decay time 0–1 → 0.1–5s (low=punchy, high=slow sweep)
  bass.accent_level — accent intensity boost
  bass.waveform          — "Saw" (smooth, warm), "Square" (hollow, buzzy), or "Supersaw" (thick unison)
  bass.filter_mode       — "Lowpass" (default, warm), "Highpass" (thin, cutting), or "Bandpass" (nasal, mid-focus)
  bass.supersaw_detune   — 0–1 spread between supersaw voices (0=tight unison, 1=wide chorus)
  bass.supersaw_voices   — 2–7 unison voices (Supersaw mode only)
  bass.distortion        — internal overdrive (keep low; 0.0–0.15 is enough)
  bass.volume            — bass synth level in mix
  bass.sub_osc_level     — sub-oscillator mix (sine one octave below; adds weight)
  bass.noise_mix         — white noise into oscillator before filter (breath/texture)
  bass.osc_detune        — pitch offset -1..+1 semitones
  bass.fm_depth          — 2-op FM depth (0=off; adds metallic/bell harmonics)
  bass.fm_ratio          — FM modulator ratio (0=0.5x sub; ~0.13=1x unison; ~0.2=2x octave; 1=8x bell)
  REESE BASS PRESET: set waveform="Supersaw", supersaw_voices=2, supersaw_detune=0.3,
                     sub_osc_level=0.5, filter_mode="Highpass", cutoff=0.25

STEP SEQUENCER (16 steps = one 4/4 bar of 16th notes):
  sequencer.steps         — total loop length in steps (8/16/32/64, default 16)
  sequencer.swing         — 0–1 rhythmic swing (0=straight, 0.5=strong shuffle/triplet feel)
  sequencer.root_note     — tonic: 0=C, 1=C#, 2=D, 3=D#, 4=E, 5=F, 6=F#, 7=G, 8=G#, 9=A, 10=A#, 11=B
  sequencer.scale         — "Major"|"Minor"|"Dorian"|"Phrygian"|"Lydian"|"Mixolydian"|"Locrian"|"Pentatonic"|"Blues"|"Chromatic"

  STEP ARRAYS — two compact formats (prefer index lists to save tokens):
    Index list  [0,4,8,12]   — active step indices; all others cleared. SAVES TOKENS — use this.
    Inline      [1,0,0,0,…]  — 16 values, 0/1 (or false/true). Use only when most steps are on.
    Clear       []           — silence all steps for that voice.

  sequencer.bass_steps    — step array for 303 bass trigger
  sequencer.bass_notes    — 16-element MIDI note array (24=C1, 36=C2, 48=C3; acid range 33–48)
  sequencer.kick_a_steps  — Kit A kick steps
  sequencer.snare_a_steps — Kit A snare steps
  sequencer.hihat_a_steps — Kit A closed hihat steps
  sequencer.kick_b_steps  — Kit B kick steps
  sequencer.snare_b_steps — Kit B snare steps
  sequencer.clap_b_steps  — Kit B clap steps
  sequencer.hihat_b_steps — Kit B closed hihat steps

FX (all 0.0–1.0):  ← ONLY valid inside "fx": {{…}}, never inside "sequencer"
  fx.reverb_mix       — reverb wet amount (0=off, 0.3=noticeable)
  fx.reverb_size      — reverb room size
  fx.reverb_gate_time — gated reverb gate close time in seconds (0=off, 0.1–0.5=80s snare effect)
  fx.reverb_freeze    — true = infinite hold, reverb tail loops forever (drone/ambient pads)
  fx.master_pitch_st  — global pitch offset in semitones for melodic voices (-12..+12; vaporwave drift)
  fx.delay_time       — delay time 0–1 → 0–2s (0.375 = dotted 8th at ~130 BPM)
  fx.delay_wow_flutter — tape wow/flutter modulation (0=clean digital, 0.3=subtle tape, 1=wobbly)
  fx.delay_saturation — tape saturation on feedback (0=clean, 0.5=warm, 1=heavy breakup)
  fx.delay_feedback   — delay repeats
  fx.delay_mix        — delay wet amount
  fx.distortion_drive — master bus saturation drive
  fx.distortion_mix   — master bus distortion wet amount
  fx.bitcrush_bits    — bit depth (1.0=clean/bypass, 0.5=8-bit, 0.0=1-bit crunch)
  fx.bitcrush_rate    — sample rate decimation (0=off, 1=extreme lo-fi)
  fx.bitcrush_mix     — bitcrush wet/dry
  fx.chorus_mix       — chorus wet/dry (0=off, 0.3=subtle, 0.6=thick ensemble)
  fx.chorus_rate      — chorus LFO rate (0=slow drift, 1=fast flutter)
  fx.chorus_depth     — chorus modulation depth (0=tight, 1=wide, watery)

LFO (global wireable modulators, 4 slots indexed 0–3):
  lfo[N].enabled     — true/false
  lfo[N].waveform    — "Sine" | "Triangle" | "Saw" | "InvSaw" | "Square" | "SampleAndHold"
  lfo[N].rate        — 0–1 (0.01=glacial sweep, 0.1=slow wobble, 0.5=fast, 1.0=8Hz audio-rate)
  lfo[N].depth       — 0–1 bipolar mod depth
  lfo[N].target      — "BassCutoff" | "BassResonance" | "BassPitch" | "BassVolume"
                       "ReverbMix" | "DelayTime" | "DelayFeedback" | "ChorusMix" | "ChorusRate"
                       "Kick808Pitch" | "None"

FREE EG (drawable arbitrary-shape modulator — 8 draggable levels, looped slowly):
  free_eg.enabled    — true/false
  free_eg.values     — 8-element array 0–1: the envelope shape drawn as 8 level steps
  free_eg.period     — 0–1 (0=0.5s fast, 0.35≈2s, 0.5≈4s, 0.75≈11s, 1.0=32s glacial)
  free_eg.depth      — 0–1 (0.5=no mod, 0=full negative, 1=full positive modulation)
  free_eg.target     — same target strings as lfo[N].target
  free_eg.loop_mode  — true=loop, false=one-shot then hold
  Use for: glacial filter sweeps, slow pitch drift, long evolving textures, breathing effects.
  Example — slow rising cutoff: values=[0,0.1,0.2,0.35,0.5,0.65,0.8,1.0], period=0.7, depth=0.8, target="BassCutoff"

EUCLIDEAN RHYTHMS:
  {{ "euclidean": {{ "voice": "<voice_name>", "pulses": N, "steps": M }} }}
  Distributes N pulses across M steps using the Bjorklund algorithm (maximum evenness).
  voice names: "kick_a", "snare_a", "hihat_a", "hihat_a_open", "kick_b", "snare_b",
               "hihat_b", "hihat_b_open", "clap_b"
  Classic patterns: (4,16)=4-on-floor, (5,16)=clave, (3,8)=basic, (5,8)=tresillo,
                    (7,16)=afro-cuban bell, (3,16)=sparse kick
  LLM trigger: "5-in-16 euclidean kick", "make it a euclidean hi-hat", "add a clave pattern"

RAMP SCHEDULING (smooth transitions over multiple jam cycles):
  Use "ramp" to schedule a smooth transition for a single parameter.
  {{ "ramp": {{ "param": "fx.reverb_mix", "to": 0.6, "cycles": 8 }} }}
  "from" is optional (defaults to current value). "cycles" is how many jam cycles to spread over (default 4).
  Supported params: any dot-path in the fx.*, bass.*, or sequencer.bpm/swing namespaces.
  LLM trigger: "slowly fade in reverb over 8 bars", "ramp up the decay", "ease to full volume"

BEHAVIOUR TEMPLATES (pre-defined energy moods):
  {{ "behaviour": "build" }}     — rising tension: longer reverb, swing, mounting energy
  {{ "behaviour": "drop" }}      — peak energy: tight, loud, driven, no reverb
  {{ "behaviour": "breakdown" }} — stripped back: heavy reverb, quiet, sparse
  {{ "behaviour": "tension" }}   — dark/filtered: low cutoff, high resonance, deep reverb
  {{ "behaviour": "euphoric" }}  — bright/open: high cutoff, chorus, full volume
  All templates scale with the current heat value (higher heat = more extreme).
  LLM trigger: "build to a drop", "add tension", "go euphoric", "break it down"

HEAT-AWARE MUTATION GUIDANCE (follow these rules based on heat):
  heat < 0.3 — stay subtle: only rhythmic variation (velocity, swing, probability), no timbre changes
  heat 0.3–0.7 — balanced: can adjust filter, FX mix, sequence notes; avoid extreme settings
  heat > 0.7 — expressive: timbre sweeps, FX automation, bold note choices; anything goes
  heat = 1.0 — maximum chaos: full range of all params; ramp scheduling encouraged
  Always respect locked params regardless of heat.

INTERNAL MUSIC API (chord/pattern generation helpers):
  Use "music_api" to generate theory-correct patterns. Any combination of chord, amen_pattern, scale_run can appear in one block.
  Chord — write a chord across bass steps 0, 4, 8, 12:
    {{ "music_api": {{ "chord": {{ "root": "E", "quality": "minor" }} }} }}
  Amen break — generate a Amen break into 808 kick/snare/hihat patterns:
    {{ "music_api": {{ "amen_pattern": {{ "heat": 0.7 }} }} }}
    heat=0 is the canonical Amen; heat=1.0 is maximum variation. seed is optional (omit for variety).
  Scale run — fill bass pattern with a stepwise run:
    {{ "music_api": {{ "scale_run": {{ "root": "A", "scale": "NaturalMinor", "direction": "up" }} }} }}
    direction: "up", "down", "updown" (bounce), "random" (shuffled)
  LLM trigger: "play an E minor chord", "give me an Amen break at half heat", "A minor scale run descending"

POLYRHYTHM (per-voice step lengths):
  In the sequencer block, add "drum_lengths" to give each drum voice its own loop length.
  Each voice loops independently — overlapping loops create polyrhythmic feel.
  {{ "sequencer": {{ "drum_lengths": {{ "kick_a": 16, "hihat_a": 12 }} }} }}
  Also: "bass_len", "hoover_len", "an1x_len" integers control those lane lengths.
  Classic: kick=16 + hihat=12 gives 4-against-3 over 48 steps. kick=16 + clave=12 = afro-cuban.
  LLM trigger: "polyrhythm", "make it swing against itself", "kick 16 hihat 12"

RATCHET / NOTE REPEAT (per-step sub-triggers for drums):
  Add "drum_ratchets" object in the sequencer block to set ratchet count per voice per step.
  Each entry is {{ "voice_name": [r0, r1, ..., r15] }} — one integer per step, value 1–4.
  1 = single hit (default), 2 = two hits per step, 3 = three, 4 = four (machine-gun).
  {{ "sequencer": {{ "drum_ratchets": {{ "hihat_a": [1,1,1,1, 1,1,2,1, 1,1,1,1, 1,1,4,1] }} }} }}
  LLM trigger: "ratchet hi-hat on step 7", "machine-gun snare fill", "add note repeat"

AN1X VOICE (warm VA pads / leads — Boards of Canada aesthetic):
  an1x.enabled          — true/false
  an1x.volume           — 0–1
  an1x.osc1_level / osc2_level — oscillator mix levels 0–1
  an1x.osc2_detune      — 0.5=unison, 0.52=subtle chorus, 0.6=wide detune, 1.0=+24st
  an1x.sub_level        — sub-oscillator level (−1 octave square wave) 0–1
  an1x.filter_cutoff    — 0–1 (0.3=dark, 0.6=open, 1.0=bright)
  an1x.filter_resonance — 0–1
  an1x.filter_env_amount — 0.5=none, >0.5=positive mod (filter opens on note), <0.5=negative
  an1x.filter_attack/decay/sustain/release — filter ADSR, 0–1 (attack→10s, decay→8s, release→30s)
  an1x.amp_attack       — 0–1; 0=instant, 0.3=~300ms pad attack, 0.8+=glacial pad swell (up to 10s)
  an1x.amp_decay/sustain/release — amplitude ADSR (release up to 30s for ambient tails)
  an1x.hard_sync        — OSC2 phase resets each OSC1 cycle: harsh harmonic sweep when detuned
  an1x.lfo_bpm_sync     — snap LFO rate to a musical division of current BPM
  an1x.lfo_sync_beats   — division: 4.0=bar, 2.0=half, 1.0=quarter, 0.5=8th, 0.25=16th
  an1x.lfo_rate         — free rate (ignored when lfo_bpm_sync=true)
  an1x.lfo_depth        — 0–1 LFO depth
  an1x.lfo_delay        — 0–1 → 0–4s LFO fade-in (vibrato deepens as note is held)
  an1x.glide_legato     — true=glide only on legato; false=always glide
  an1x.pitch_env_attack — 0–1 AD pitch envelope attack
  an1x.pitch_env_decay  — 0–1 AD pitch envelope decay
  an1x.pitch_env_amount — 0–1 (0.5=none, >0.5=up, <0.5=down, ±24 st max)
  an1x.drift            — 0–1 pitch instability; 0.12=subtle analogue feel
  an1x.glide_time       — 0–1 → 0–500ms pitch glide between notes
  an1x.an1x_steps       — 16-element bool array: which steps trigger the AN1X
  an1x.an1x_notes       — 16-element int array: MIDI note per step
  LLM triggers: "add a pad", "warm lead", "BoC", "ambient melody", "detuned synth", "slow attack"

HOOVER LEAD (supersaw + resonant LP filter sweep — dominator/rave character):
  hoover.enabled        — true/false — MUST BE true for the hoover to sound at all
  hoover.filter_start   — LP cutoff openness at note trigger (0=100Hz dark, 0.88=12kHz wide open)
                          Sweeps DOWN from this value over sweep_time. Higher = brighter start.
                          Classic dominator: 0.85–0.92
  hoover.sweep_time     — filter sweep duration 0–1 (maps to 0.1–4 s; 0.2=~850ms)
  hoover.resonance      — resonance 0–1 (0.82 = moving resonant peak = hoover character)
  hoover.detune         — supersaw spread semitones (0=mono, 0.45=lush shimmer)
  hoover.voices         — unison count 2–7 (5 is standard)
  hoover.volume         — 0–1
  hoover.hoover_steps   — 16-element bool array: which steps trigger the Hoover
  hoover.hoover_notes   — 16-element int array: MIDI note per step
  AUTHENTIC DOMINATOR: enabled=true, filter_start=0.88, resonance=0.82, sweep_time=0.2, detune=0.45, voices=5
  LLM triggers: "add a hoover", "rave lead", "dominator", "hardcore lead", "early rave"

═══ RHYTHM BASICS ═══

Minimal 4/4 foundation — use index list format (compact, preferred):
  kick_a_steps 4-on-floor:   [0,4,8,12]
  hihat_a_steps offbeat 8ths:[2,6,10,14]
  snare_a_steps on 2 and 4:  [4,12]
  clap_b_steps on 2 and 4:   [4,12]
Build from there — add syncopation and gaps. Never fill every step with the same drum.

IMPORTANT — drum_ratchets takes INTEGERS 1–4 only, never booleans:
  CORRECT: {{"drum_ratchets": {{"hihat_a": [1,1,2,1,1,1,4,1,1,1,2,1,1,1,1,1]}}}}
  WRONG:   {{"drum_ratchets": {{"hihat_a": [true,false,true,…]}}}}  ← booleans are invalid here

BASS MELODY BASICS:
  Acid range C2–C3: C2=36, D2=38, Eb2=39, F2=41, G2=43, A2=45, Bb2=46, B2=47, C3=48
  Minor pentatonic (C): 36, 39, 41, 43, 46 (and 48 for octave)
  Keep to 3–5 distinct pitches per loop. Use false in bass_steps for rhythmic rests.

═══ MUSIC THEORY REFERENCE ═══

CHROMATIC: C=0, C#=1, D=2, D#=3, E=4, F=5, F#=6, G=7, G#=8, A=9, A#=10, B=11

SCALE INTERVALS (semitones from root):
  Major:        0 2 4 5 7 9 11   W-W-H-W-W-W-H — bright, resolved
  Minor:        0 2 3 5 7 8 10   W-H-W-W-H-W-W — dark, natural minor (acid default)
  Dorian:       0 2 3 5 7 9 10   like minor but raised 6th — warm, jazz-funk
  Phrygian:     0 1 3 5 7 8 10   like minor but b2 — flamenco, dark techno
  Pentatonic:   0 3 5 7 10       5-note minor — universal, always sounds good
  Blues:        0 3 5 6 7 10     pentatonic + tritone blue note

TRIADS (offsets from root):
  Major: 0 4 7  |  Minor: 0 3 7  |  Diminished: 0 3 6

APPLYING THE KEY — when setting bass_notes:
  Current key: root={root_note} ({root_name}), scale={scale_name}
  Scale notes in C2–C3 range: {scale_c2_c3}
  Prefer these MIDI notes for melodic coherence. Move between scale degrees for interest.
  Use the tonic (root) on strong beats (1, 9), 5th on medium beats for stability.

═══ HOW TO INTERPRET INSTRUCTIONS ═══

"change the melody" / "different pattern" / "new notes"
  → Set bass_steps to a new 16-step pattern, set bass_notes to MIDI pitches

"add claps" / "add snare"
  → Set clap_b_steps or snare_a_steps to a useful drum pattern

"add hihats" / "more hats"
  → Set hihat_a_steps or hihat_b_steps

"more acid" / "squelchier"
  → Raise bass.resonance (0.75–0.88), raise bass.env_mod, lower bass.cutoff

"darker" / "more weight"
  → Lower bass.cutoff, raise fx.reverb_mix slightly

"add space" / "more atmosphere"
  → Raise fx.reverb_mix (0.2–0.4), add fx.delay_mix (0.1–0.25)

"harder" / "more drive"
  → Raise fx.distortion_drive + fx.distortion_mix

"swing it" / "add shuffle" / "make it groove"
  → Set sequencer.swing to 0.25–0.4 for mild shuffle, 0.5 for strong triplet feel

"chorus" / "ensemble" / "wide" / "lush"
  → Set fx.chorus_mix to 0.3–0.6, fx.chorus_depth to 0.4–0.7

"highpass" / "thin it out" / "remove the lows"
  → Set bass.filter_mode to "Highpass" — removes sub lows, cutting and percussive

"bandpass" / "nasal" / "mid focus"
  → Set bass.filter_mode to "Bandpass"

"simpler" / "strip it back"
  → Reduce active bass_steps, remove some drum steps

CLEARING COMMANDS — use empty array [] to silence a voice:
"remove kick" / "no kick"   → {{"sequencer": {{"kick_a_steps": []}}}}
"no snare" / "remove snare" → {{"sequencer": {{"snare_a_steps": []}}}}
"no hats" / "remove hihat"  → {{"sequencer": {{"hihat_a_steps": []}}}}
"no claps" / "remove clap"  → {{"sequencer": {{"clap_b_steps": []}}}}
"no delay" / "remove delay" → {{"fx": {{"delay_mix": 0.0, "delay_feedback": 0.0}}}}
"no reverb"                  → {{"fx": {{"reverb_mix": 0.0}}}}
"clear all drums" / "no drums"
  → {{"sequencer": {{"kick_a_steps":[],"snare_a_steps":[],"hihat_a_steps":[],"clap_b_steps":[]}}}}

ACID JAM GUIDANCE — while jamming in acid styles, actively vary:
  bass.cutoff between 0.15 and 0.60 (keep it moving — static cutoff sounds dead)
  bass.resonance between 0.65 and 0.90 (higher = more squelch)
  bass.env_mod between 0.40 and 0.85 (controls sweep character)
  bass.decay between 0.20 and 0.55 (shorter = punchier acid stabs)

"wobble bass" / "slow filter sweep"
  → lfo[0].enabled=true, target="BassCutoff", rate=0.05–0.15, depth=0.2–0.4, waveform="Sine"

"tremolo" / "volume pulse"
  → lfo[0].enabled=true, target="BassVolume", rate=0.1–0.3, depth=0.3–0.5, waveform="Triangle"

"pitch vibrato"
  → lfo[0].enabled=true, target="BassPitch", rate=0.1–0.2, depth=0.1–0.2, waveform="Sine"

FX RESTRAINT — always start clean:
  Unless explicitly asked, keep FX minimal: reverb_mix ≤ 0.12, delay_mix ≤ 0.08, distortion at 0.0
  Never set heavy reverb + heavy delay + distortion simultaneously.

JAM HEAT: {heat_pct}% — {heat_desc}

RACK ROUTING — enable/disable modules and wire cables between them:
  {{"rack": {{
    "enable":     ["bitcrush"],                          ← turn a module on
    "disable":    ["reverb"],                            ← turn a module off
    "connect":    [{{"from": "bass",     "to": "bitcrush"}},
                   {{"from": "bitcrush", "to": "master"}}],   ← add patch cables
    "disconnect": [{{"from": "bitcrush", "to": "master"}}]    ← remove a cable
  }}}}
  Module names: "bass", "808", "909", "hoover", "an1x", "amen", "noise", "granular",
                "bitcrush", "reverb", "delay", "chorus", "phaser", "drive",
                "eq", "compressor", "tapesat", "waveshaper", "ringmod",
                "lfo", "master", "sequencer"

  "connect the bitcrush" / "wire it up" / "route bass through reverb"
    → add rack.connect entries from the source to the target module then to master if needed
  "disconnect reverb" / "remove that cable"
    → add rack.disconnect entry

SETTINGS — change only when explicitly asked:
  {{"settings": {{
    "heat": 0.3,                 ← jam mutation intensity 0–1 (0=subtle, 1=anything goes)
    "style": "acid_house",       ← switch active style (use style id from the style list)
    "jam_bars": 4,               ← bars between jam cycles (0=continuous, 1/2/4/8 common values)
    "persona": "PULSE",          ← AI name shown in UI
    "conversation_mode": "producer"  ← "off" | "producer" | "dj" | "mc"
    "spawn_agent": {{               ← spawn a new LLM agent module in the rack
      "persona": "BASS BRAIN",      ← name shown on the agent card
      "scope": ["bass", "fx"],      ← which modules this agent controls (empty = all)
      "model": null                  ← model override (null = use default model)
    }},
    "dismiss": true                  ← this agent removes itself from the rack
  }}}}
  "save_project": true           ← save current state to project-[timestamp].json
  Only output these when directly commanded. Always acknowledge in _comment what you did.
  When you set parameters that clearly match a known style, also set "style" to that id.
  spawn_agent: use when asked to add specialist agents (e.g. "add an MC", "spawn a bass agent").
  dismiss: use only when asked to remove yourself. Cannot dismiss the last remaining agent.

═══ OUTPUT FORMAT ═══

Always start your response with "_thinking": one or two sentences explaining what the user is asking for and what specific parameters you will change. This is your reasoning scratch-pad — write it before anything else.
{comment_instruction}
Only include fields you are actually changing.
In MC or DJ mode you may add an optional "mc_line" string — a short crowd shout spoken via TTS, separate from "_comment". Keep it under 12 words. Use it for big moments, drops, or energy peaks.
TOP-LEVEL SCHEMA — the only valid top-level keys are "_comment", "_thinking", "mc_line", "bass", "sequencer", "fx", "hoover", "an1x", "free_eg", "noise", "granular", "kit_a", "kit_b", "euclidean", "music_api", "ramp", "behaviour", "rack", "settings", "save_project".
  "bass" and "fx" are NEVER nested inside "sequencer".
  "fx" is NEVER nested inside "fx".
  Each key appears at most ONCE per object.

WRONG (do not do this):
  {{"sequencer": {{"bass_steps": [...], "bass": {{"cutoff": 0.3}}}}}}       ← bass inside sequencer
  {{"fx": {{"reverb_mix": 0.1, "fx": {{"delay_mix": 0.2}}}}}}              ← fx inside fx
  {{"sequencer": {{"bass_steps": [...], "fx": {{"reverb_mix": 0.1}}}}}}    ← fx inside sequencer

Example — "add claps on 2 and 4":
{{"_comment": "{clap_example}",
  "sequencer": {{"clap_b_steps": [4,12]}}}}

Example — "change the melody":
{{"_comment": "{melody_example}",
  "sequencer": {{"bass_steps": [0,2,5,7,10,13],
                 "bass_notes":  [36,36,36,41,43,38,36,36,36,38,36,36,36,36,40,36]}}}}

Example — "more acid":
{{"_comment": "{acid_example}",
  "bass": {{"resonance": 0.85, "env_mod": 0.80, "cutoff": 0.30, "decay": 0.25}}}}
"#,
        persona = persona,
        user_instructions_section = user_instructions_section,
        style_section = style_section,
        scope_section = scope_section,
        autonomy_section = autonomy_section,
        current_json = current_json,
        bass_info = bass_info,
        locked_str = locked_str,
        focused_str = focused_str,
        heat_pct = heat_pct,
        heat_desc = heat_desc,
        comment_instruction = match state.llm.conversation_mode {
            ConversationMode::Off =>
                "\"_comment\": one short technical label of what params changed. No personality.",
            ConversationMode::Producer =>
                "Always include \"_comment\" (one sentence) — what you changed and why it serves the music right now.",
            ConversationMode::Dj =>
                "Always include \"_comment\" in character as a hype DJ hyping up the crowd. \
                 Short, punchy, first-person, cheesy party energy. \
                 Examples: \"OKAY WE ARE DROPPING THE BASS RIGHT NOW!\", \
                 \"your boy just cranked the filter, you're WELCOME!\", \
                 \"DJ {persona} in the house, stepping up the BPM cos this crowd needs MORE!\"",
            ConversationMode::Mc =>
                "Always include \"_comment\" in character as a jungle/rave MC hyping the crowd. \
                 Short shoutouts, rave slang, aggressive energy. \
                 Use classic MC call-outs: SELECTOR, MASSIVE, REWIND, WHEEL IT, BIG UP, JUNGLIST. \
                 Examples: \"SELECTOR! junglist massive, big up!\", \
                 \"REWIND that ting, wheel it back selector!\", \
                 \"BIG UP the jungle massive, this is for you!\", \
                 \"WHEEL IT UP! darkness in the place tonight, massive massive!\", \
                 \"SELECTOR pull up that riddim, junglist in the house!\"",
        },
        clap_example = match state.llm.conversation_mode {
            ConversationMode::Off => "clap909_steps updated",
            ConversationMode::Producer =>
                "adding a 909 clap on beats 2 and 4 for a classic house feel",
            ConversationMode::Dj => "CLAP CLAP CLAP just dropped the backbeat FEEL THAT",
            ConversationMode::Mc => "SELECTOR! clap ting incoming, big up the backbeat massive!",
        },
        melody_example = match state.llm.conversation_mode {
            ConversationMode::Off => "bass_steps and bass_notes updated",
            ConversationMode::Producer =>
                "new bass line — stepping up a fifth and back with a chromatic passing note",
            ConversationMode::Dj =>
                "NEW BASSLINE JUST DROPPED who ordered the groove you're welcome",
            ConversationMode::Mc => "WHEEL IT UP! fresh line incoming, junglist riddim massive!",
        },
        acid_example = match state.llm.conversation_mode {
            ConversationMode::Off => "bass resonance and env_mod updated",
            ConversationMode::Producer =>
                "cranking the resonance and env_mod for full acid squelch",
            ConversationMode::Dj =>
                "ACID ACID ACID your boy just went full 303 mode YOU ARE WELCOME",
            ConversationMode::Mc => "REWIND! acid ting, selector pull up, junglist massive BWOY!",
        },
    )
}

/// JSON Schema for grammar-constrained generation (used by llama-cpp-2).
pub fn param_json_schema() -> serde_json::Value {
    let bool_array =
        serde_json::json!({ "type": "array", "items": { "type": "boolean" }, "maxItems": 16 });
    let note_array = serde_json::json!({ "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 127 }, "maxItems": 16 });
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema",
        "type": "object",
        "properties": {
            "_thinking": { "type": "string", "maxLength": 300 },
            "_comment": { "type": "string", "maxLength": 200 },
            "mc_line":  { "type": "string", "maxLength": 80, "description": "Short crowd shout for MC/DJ mode TTS (optional). Under 12 words." },
            "bass": {
                "type": "object",
                "properties": {
                    "cutoff":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "resonance":    { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "env_mod":      { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "decay":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "accent_level": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "waveform":          { "type": "string", "enum": ["Saw", "Square", "Supersaw"] },
                    "supersaw_detune":   { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "supersaw_voices":   { "type": "integer", "minimum": 2, "maximum": 7 },
                    "distortion":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "volume":            { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "sub_osc_level":     { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "sub-oscillator level: sine one octave below, 0=off 1=full" },
                    "portamento_time":   { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "slide/glide time: 0=10ms (snappy), 1=500ms (slow)" },
                    "noise_mix":         { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "white noise mixed before filter: 0=off, 0.3=gritty, 1=full noise" },
                    "osc_detune":        { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "oscillator pitch offset in semitones: -1=down 1st, 0=center, +1=up 1st" },
                    "fm_depth":          { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "2-op FM depth: 0=off (pure additive), 0.3=subtle metallic, 1=extreme bell/clang" },
                    "fm_ratio":          { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "FM modulator/carrier ratio 0->0.5x (sub-harmonic), 0.13->1x (unison FM), 0.2->2x (octave), 1.0->8x (bell/metallic)" }
                },
                "additionalProperties": false
            },
            "sequencer": {
                "type": "object",
                "properties": {
                    "bpm":           { "type": "number", "minimum": 40.0, "maximum": 250.0 },
                    "steps":         { "type": "integer", "minimum": 8, "maximum": 64, "multipleOf": 8 },
                    "swing":         { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "time_sig_num":  { "type": "integer", "minimum": 2, "maximum": 9 },
                    "root_note": { "type": "integer", "minimum": 0, "maximum": 11, "description": "tonic: 0=C 1=C# 2=D 3=D# 4=E 5=F 6=F# 7=G 8=G# 9=A 10=A# 11=B" },
                    "scale": { "type": "string", "enum": ["Major","Minor","Dorian","Phrygian","Lydian","Mixolydian","Locrian","Pentatonic","Blues","Chromatic"] },
                    "bass_steps":    bool_array.clone(),
                    "bass_notes":    note_array,
                    "kick_a_steps":  bool_array.clone(),
                    "snare_a_steps": bool_array.clone(),
                    "hihat_a_steps": bool_array.clone(),
                    "kick_b_steps":  bool_array.clone(),
                    "snare_b_steps": bool_array.clone(),
                    "clap_b_steps":  bool_array.clone(),
                    "hihat_b_steps": bool_array
                },
                "additionalProperties": false
            },
            "noise": {
                "type": "object",
                "properties": {
                    "enabled": { "type": "boolean" },
                    "volume":  { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "color":   { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "0=white, 0.5=pink, 1=brown" },
                    "cutoff":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "LP filter cutoff, 0=200Hz, 1=20kHz" },
                    "attack":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "amplitude attack 0-1 → 1ms-5s" },
                    "release": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "amplitude release 0-1 → 1ms-10s" },
                    "filter_lfo_rate":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "filter mod LFO rate 0.05-10Hz" },
                    "filter_lfo_depth": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "filter mod depth" },
                    "sh_rate":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "sample-and-hold rate 0.5-20Hz for rhythmic texture" },
                    "sh_depth": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "S&H modulation depth on filter" }
                },
                "additionalProperties": false
            },
            "granular": {
                "type": "object",
                "description": "Granular texture voice — overlapping micro-grains from a loaded WAV",
                "properties": {
                    "enabled":          { "type": "boolean" },
                    "volume":           { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "density":          { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "grain spawn rate: 0=sparse, 1=dense cloud" },
                    "grain_size":       { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "10-500ms per grain" },
                    "position":         { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "playback position in WAV" },
                    "position_jitter":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "random spread around position" },
                    "pitch_scatter":    { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "random pitch per grain, ±12st at max" },
                    "spray":            { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "stereo spread (0=mono, 1=full width)" }
                },
                "additionalProperties": false
            },
            "free_eg": {
                "type": "object",
                "properties": {
                    "enabled":   { "type": "boolean" },
                    "loop_mode": { "type": "boolean", "description": "true=loop, false=one-shot" },
                    "period":    { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "0=0.5s, 0.35≈2s, 0.5≈4s, 0.75≈11s, 1.0=32s" },
                    "depth":     { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "0.5=no mod, 1.0=full positive, 0.0=full negative" },
                    "target":    { "type": "string", "enum": ["None","BassCutoff","BassResonance","BassPitch","BassVolume","ReverbMix","DelayTime","DelayFeedback","ChorusMix","ChorusRate","Kick808Pitch"] },
                    "values":    { "type": "array", "items": { "type": "number", "minimum": 0.0, "maximum": 1.0 }, "minItems": 8, "maxItems": 8, "description": "8 envelope levels 0-1" }
                },
                "additionalProperties": false
            },
            "an1x": {
                "type": "object",
                "description": "AN1X-style VA voice — warm detuned pads/leads (Boards of Canada aesthetic). LLM triggers: 'add a pad', 'warm lead', 'BoC', 'ambient', 'detuned'.",
                "properties": {
                    "enabled":            { "type": "boolean" },
                    "volume":             { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "osc1_level":         { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "osc2_level":         { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "osc2_detune":        { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "OSC2 detune: 0.5=unison, 0=−24st, 1=+24st" },
                    "sub_level":          { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "sub-oscillator (−1 octave) level" },
                    "filter_cutoff":      { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "filter_resonance":   { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "filter_env_amount":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "filter env mod: 0.5=none, <0.5=negative, >0.5=positive" },
                    "filter_attack":      { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "filter ADSR attack 0-1 → 1ms-10s" },
                    "filter_decay":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "filter_sustain":     { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "filter_release":     { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "filter release 0-1 → 1ms-30s" },
                    "amp_attack":         { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "amp attack 0-1 → 1ms-10s. Use high values for glacial pad swells." },
                    "amp_decay":          { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "amp_sustain":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "amp_release":        { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "amp release 0-1 → 1ms-30s for ambient tails" },
                    "hard_sync":          { "type": "boolean", "description": "OSC2 hard sync to OSC1: harsh harmonic content when OSC2 is detuned above" },
                    "lfo_bpm_sync":       { "type": "boolean", "description": "snap LFO rate to musical division of current BPM" },
                    "lfo_sync_beats":     { "type": "number", "description": "LFO division in beats: 4=bar, 2=half, 1=quarter, 0.5=8th, 0.25=16th" },
                    "lfo_rate":           { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "free LFO rate 0-1 → 0.01-20 Hz (ignored when lfo_bpm_sync=true)" },
                    "lfo_depth":          { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "lfo_delay":          { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "LFO fade-in time: 0-1 → 0-4 s" },
                    "glide_legato":       { "type": "boolean", "description": "true=glide only when notes overlap; false=always glide" },
                    "pitch_env_attack":   { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "pitch envelope attack time" },
                    "pitch_env_decay":    { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "pitch envelope decay time" },
                    "pitch_env_amount":   { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "pitch env amount: 0.5=none, >0.5=up bend, <0.5=down bend (max ±24 st)" },
                    "drift":              { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "pitch instability depth — 0=stable, 1=max analogue wobble (±0.15 st)" },
                    "glide_time":         { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "pitch glide: 0=instant, 1=500ms exponential slide" },
                    "an1x_steps":         bool_array,
                    "an1x_notes":         { "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 127 }, "maxItems": 64 }
                },
                "additionalProperties": false
            },
            "hoover": {
                "type": "object",
                "description": "Hoover lead voice — supersaw + HP filter sweep. LLM triggers: 'add a hoover', 'rave lead', 'dominator'.",
                "properties": {
                    "enabled":          { "type": "boolean" },
                    "filter_start":     { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "HP filter starting cutoff (0=200Hz, 1=8kHz). High values = thin bright transient." },
                    "sweep_time":       { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Filter sweep duration 0-1 (maps to 0.1-4.0 s)" },
                    "resonance":        { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Filter resonance — high values create the Hoover character" },
                    "detune":           { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Supersaw detune spread (0=no shimmer, 1=wide)" },
                    "voices":           { "type": "integer", "minimum": 2, "maximum": 7, "description": "Supersaw unison voice count" },
                    "volume":           { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "pitch_lfo_rate":   { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Wail LFO rate 0-1 (maps to 0-8 Hz)" },
                    "pitch_lfo_depth":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Wail LFO depth 0-1 (maps to 0-2 semitones)" },
                    "hoover_steps":     bool_array,
                    "hoover_notes":     { "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 127 }, "maxItems": 64, "description": "MIDI note per hoover step" }
                },
                "additionalProperties": false
            },
            "fx": {
                "type": "object",
                "properties": {
                    "reverb_size":      { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "reverb_mix":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "reverb_gate_time": { "type": "number", "minimum": 0.0, "maximum": 2.0, "description": "gated reverb: 0=no gate, 0.1–2.0s = gate close time (80s snare effect)" },
                    "reverb_freeze":    { "type": "boolean", "description": "true = infinite reverb hold, tail loops forever (drone/ambient)" },
                    "master_pitch_st": { "type": "number", "minimum": -12.0, "maximum": 12.0, "description": "global semitone offset for melodic voices (vaporwave pitch drift)" },
                    "delay_time":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "delay_feedback":   { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "delay_mix":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "delay_wow_flutter": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "tape wow/flutter on delay (0=clean, 1=wobbly tape)" },
                    "delay_saturation": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "tape saturation on delay feedback (warm breakup)" },
                    "distortion_drive": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "distortion_mix":   { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "bitcrush_bits": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "bitcrush_rate": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "bitcrush_mix":  { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "chorus_rate":   { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "chorus_depth":  { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "chorus_mix":    { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "phaser_rate":        { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "phaser LFO rate: 0=0.05Hz (slow) 1=5Hz (fast)" },
                    "phaser_depth":       { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "phaser sweep width" },
                    "phaser_mix":         { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "phaser wet/dry" },
                    "waveshaper_drive":   { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "pre-FX soft clip drive (0=clean, 1=heavy saturation)" },
                    "waveshaper_mix":     { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "ring_mod_freq":      { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "ring mod carrier: 0=50Hz (growl), 1=500Hz (metallic)" },
                    "ring_mod_mix":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "eq_low_gain":           { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "low shelf 200Hz gain: -1=-12dB, 0=flat, +1=+12dB" },
                    "eq_mid_gain":           { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "mid peak 1kHz gain" },
                    "eq_hi_gain":            { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "high shelf 5kHz gain" },
                    "compressor_threshold":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "compressor threshold: 0=-40dB (heavy compression), 1=0dB (bypassed)" },
                    "compressor_ratio":      { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "compression ratio: 0=1:1 (off), 1=20:1 (limiting)" },
                    "compressor_mix":        { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "compressor parallel wet/dry; 0=off" },
                    "tape_drive":            { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "tape saturation drive — arctan soft clip, warm harmonics" },
                    "tape_mix":              { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "tape saturation wet/dry; 0=off" },
                    "tape_flutter":          { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "wow/flutter depth — ±4% AM at 2.5Hz; adds vintage instability" },
                    "autotune_amount":       { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "pitch shift amount: 0=bypass, 0.0833=+1 semitone, 0.25=+3st, 1.0=+12st (octave)" },
                    "autotune_mix":          { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "autotune wet/dry; 0=off" },
                    "xmod_bass_to_an1x_pitch": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "cross-mod: bass osc → AN1X pitch (FM for evolving textures)" },
                    "xmod_noise_to_filter":    { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "cross-mod: noise → bass filter cutoff (random filter movement)" },
                    "sidechain_amount":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "sidechain compression: kick ducks bass/pad (0=off, 0.5=pumping, 1=hard duck)" },
                    "sidechain_attack":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "sidechain attack 0.1-50ms" },
                    "sidechain_release": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "sidechain release 10-500ms (longer=more pumping)" },
                    "compressor_multiband": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "0=single-band, >0=3-band split (low/mid/high) compression" },
                    "stereo_width": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "stereo width: 0=mono, 0.5=normal, 1=wide" }
                },
                "additionalProperties": false
            },
            "kit_a": {
                "type": "object",
                "properties": {
                    "kick": {
                        "type": "object",
                        "properties": {
                            "pitch_env_depth": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "kick pitch drop height: 0=subtle 1=extreme" },
                            "pitch_env_time":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "kick pitch drop decay time: 0=10ms 1=200ms" },
                            "clip":            { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "gabber hard-clip drive: 0=clean 1=full distortion (flat-top sine)" }
                        },
                        "additionalProperties": false
                    }
                },
                "additionalProperties": false
            },
            "kit_b": {
                "type": "object",
                "properties": {
                    "kick": {
                        "type": "object",
                        "properties": {
                            "pitch_env_depth": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "kick pitch drop height: 0=subtle 1=extreme" },
                            "pitch_env_time":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "kick pitch drop decay time: 0=10ms 1=200ms" },
                            "clip":            { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "gabber hard-clip drive: 0=clean 1=full distortion (flat-top sine)" }
                        },
                        "additionalProperties": false
                    }
                },
                "additionalProperties": false
            }
        },
        "euclidean": {
            "type": "object",
            "description": "Apply a Euclidean (Bjorklund) rhythm to a drum voice. Distributes pulses as evenly as possible across steps.",
            "properties": {
                "voice":  { "type": "string", "enum": ["kick_a","snare_a","hihat_a","hihat_a_open","kick_b","snare_b","hihat_b","hihat_b_open","clap_b"], "description": "drum voice to pattern" },
                "pulses": { "type": "integer", "minimum": 0, "maximum": 64, "description": "number of active steps to place" },
                "steps":  { "type": "integer", "minimum": 1, "maximum": 64, "description": "total steps in the pattern (defaults to current sequencer step count)" }
            },
            "required": ["voice", "pulses"],
            "additionalProperties": false
        },
        "ramp": {
            "type": "object",
            "description": "Schedule a smooth parameter transition over N jam cycles. The value moves from 'from' (or current) to 'to' linearly.",
            "properties": {
                "param":  { "type": "string", "description": "Dot-path of the parameter, e.g. 'fx.reverb_mix', 'bass.cutoff', 'sequencer.bpm'" },
                "to":     { "type": "number", "description": "Target value to ramp toward" },
                "from":   { "type": "number", "description": "Starting value (optional, defaults to current param value)" },
                "cycles": { "type": "number", "minimum": 1, "description": "Number of jam cycles to spread the transition over (default 4; 'bars' is accepted as an alias)" }
            },
            "required": ["param", "to"],
            "additionalProperties": false
        },
        "behaviour": {
            "type": "string",
            "description": "Apply a pre-defined energy mood preset. Scales with current heat.",
            "enum": ["build", "buildup", "rise", "drop", "peak", "full_energy", "breakdown", "strip", "minimal", "tension", "dark", "euphoric", "bright"]
        },
        "music_api": {
            "type": "object",
            "description": "Internal music-theory helpers. Any combination of chord, amen_pattern, scale_run. Results are written directly into sequencer patterns.",
            "properties": {
                "seed": { "type": "integer", "description": "Optional fixed seed for deterministic output. Omit for random." },
                "chord": {
                    "type": "object",
                    "description": "Write a chord into bass steps 0, 4, 8, 12.",
                    "properties": {
                        "root":    { "type": "string", "description": "Root note: C, C#, D, D#, E, F, F#, G, G#, A, A#, B" },
                        "quality": { "type": "string", "enum": ["major","minor","dim","aug","sus2","sus4","dom7","maj7","min7","dim7"] }
                    },
                    "required": ["root", "quality"],
                    "additionalProperties": false
                },
                "amen_pattern": {
                    "type": "object",
                    "description": "Generate a mutated Amen break and write it into kick/snare/hihat_a (808) patterns.",
                    "properties": {
                        "heat": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "0=canonical Amen, 1=maximum variation" },
                        "seed": { "type": "integer", "description": "Override global seed for this call only." }
                    },
                    "additionalProperties": false
                },
                "scale_run": {
                    "type": "object",
                    "description": "Fill the bass pattern with a stepwise run through a scale.",
                    "properties": {
                        "root":      { "type": "string", "description": "Root note name" },
                        "scale":     { "type": "string", "description": "Scale name (Major, NaturalMinor, Dorian, Phrygian, Lydian, Mixolydian, Locrian, Pentatonic, Blues, Chromatic)" },
                        "direction": { "type": "string", "enum": ["up","down","updown","random"], "description": "up=ascending, down=descending, updown=bounce, random=shuffled" },
                        "seed": { "type": "integer", "description": "Override global seed for this call only." }
                    },
                    "required": ["root", "scale"],
                    "additionalProperties": false
                }
            },
            "additionalProperties": false
        },
        "additionalProperties": false
    })
}
