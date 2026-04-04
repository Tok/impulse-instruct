// ─── llm/prompt.rs ───────────────────────────────────────────────────────────
// Builds the system prompt that grounds the LLM in current synth state.

use crate::llm::styles::StyleCatalog;
use crate::state::{AppState, ConversationMode, ROOT_NAMES, StyleVerbosity};

/// Returns the system prompt. If the user has set a non-empty `system_prompt_override`,
/// that is returned verbatim — giving full control over the model's grounding.
pub fn build_system_prompt(state: &AppState) -> String {
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
        _ => "fire — anything goes, dramatic mutations, surprise",
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
            "cutoff": state.bass.cutoff,
            "resonance": state.bass.resonance,
            "env_mod": state.bass.env_mod,
            "decay": state.bass.decay,
            "accent_level": state.bass.accent_level,
            "waveform": format!("{:?}", state.bass.waveform),
            "filter_mode": format!("{:?}", state.bass.filter_mode),
            "distortion": state.bass.distortion,
            "volume": state.bass.volume
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
                    "\n═══ ACTIVE STYLE ═══\n\n{}{}{}\nUse this as your creative brief. \
                     Evolve the current sound toward this aesthetic — don't reset everything at once.\n",
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
{style_section}{user_instructions_section}
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
  bass.decay        — filter envelope decay time (low=punchy, high=slow sweep)
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
  sequencer.root_note     — tonic of the current key: 0=C, 1=C#, 2=D, 3=D#, 4=E, 5=F, 6=F#, 7=G, 8=G#, 9=A, 10=A#, 11=B
  sequencer.scale         — active scale: "Major" | "Minor" | "Dorian" | "Phrygian" | "Lydian" | "Mixolydian" | "Locrian" | "Pentatonic" | "Blues" | "Chromatic"
  sequencer.bass_steps    — 16-element bool array: which steps trigger the 303
  sequencer.bass_notes    — 16-element int array: MIDI note per step
                            (24=C1, 36=C2, 48=C3; typical range 33–48 for acid)
  sequencer.kick_a_steps  — 16-element bool: Kit A kick
  sequencer.snare_a_steps — 16-element bool: Kit A snare
  sequencer.hihat_a_steps — 16-element bool: Kit A closed hihat
  sequencer.kick_b_steps  — 16-element bool: Kit B kick
  sequencer.snare_b_steps — 16-element bool: Kit B snare
  sequencer.clap_b_steps  — 16-element bool: Kit B clap
  sequencer.hihat_b_steps — 16-element bool: Kit B closed hihat

FX (all 0.0–1.0):  ← ONLY valid inside "fx": {{…}}, never inside "sequencer"
  fx.reverb_mix       — reverb wet amount (0=off, 0.3=noticeable)
  fx.reverb_size      — reverb room size
  fx.delay_time       — delay time (0.375 = dotted 8th at ~130 BPM)
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

AN1X VOICE (warm VA pads / leads — Boards of Canada aesthetic):
  an1x.enabled          — true/false
  an1x.volume           — 0–1
  an1x.osc1_level / osc2_level — oscillator mix levels 0–1
  an1x.osc2_detune      — 0.5=unison, 0.52=subtle chorus, 0.6=wide detune, 1.0=+24st
  an1x.sub_level        — sub-oscillator level (−1 octave square wave) 0–1
  an1x.filter_cutoff    — 0–1 (0.3=dark, 0.6=open, 1.0=bright)
  an1x.filter_resonance — 0–1
  an1x.filter_env_amount — 0.5=none, >0.5=positive mod (filter opens on note), <0.5=negative
  an1x.filter_attack/decay/sustain/release — filter ADSR, 0–1 → 1ms–8s
  an1x.amp_attack       — 0–1; 0=instant, 0.3=~300ms pad attack, 0.6=slow swell
  an1x.amp_decay/sustain/release — amplitude ADSR
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

HOOVER LEAD (supersaw + HP filter sweep):
  hoover.enabled        — true/false
  hoover.filter_start   — HP cutoff start position (0=200Hz, 1=8kHz; 0.8 is classic)
  hoover.sweep_time     — filter sweep duration 0–1 (maps to 0.1–4 s; 0.13=~550ms)
  hoover.resonance      — resonance 0–1 (0.76 = canonical Hoover character)
  hoover.detune         — supersaw spread (0=mono, 0.42=lush shimmer)
  hoover.voices         — unison count 2–7 (5 is standard)
  hoover.volume         — 0–1
  hoover.hoover_steps   — 16-element bool array: which steps trigger the Hoover
  hoover.hoover_notes   — 16-element int array: MIDI note per step
  LLM triggers: "add a hoover", "rave lead", "hardcore lead", "early rave"

═══ RHYTHM BASICS ═══

Minimal 4/4 foundation (indices 0–15):
  kick_a_steps 4-on-the-floor: [true,false,false,false,true,false,false,false,true,false,false,false,true,false,false,false]
  hihat_a_steps offbeat 8ths:  [false,false,true,false,false,false,true,false,false,false,true,false,false,false,true,false]
Build from there — add syncopation and gaps. Never fill every step with the same drum.

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

CLEARING COMMANDS — these must use all-false 16-element arrays:
"remove kick" / "no kick" / "kick off"
  → {{"sequencer": {{"kick_a_steps": [false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false]}}}}

"no snare" / "remove snare"
  → {{"sequencer": {{"snare_a_steps": [false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false]}}}}

"no hats" / "no hihat" / "remove hihat"
  → {{"sequencer": {{"hihat_a_steps": [false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false]}}}}

"no claps" / "remove clap"
  → {{"sequencer": {{"clap_b_steps": [false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false]}}}}

"no delay" / "remove delay"
  → {{"fx": {{"delay_mix": 0.0, "delay_feedback": 0.0}}}}

"no reverb" / "remove reverb"
  → {{"fx": {{"reverb_mix": 0.0}}}}

"clear all drums" / "no drums"
  → {{"sequencer": {{"kick_a_steps": [false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false],
                     "snare_a_steps": [false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false],
                     "hihat_a_steps": [false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false],
                     "clap_b_steps":  [false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false]}}}}

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

═══ OUTPUT FORMAT ═══

Always start your response with "_thinking": one or two sentences explaining what the user is asking for and what specific parameters you will change. This is your reasoning scratch-pad — write it before anything else.
{comment_instruction}
Only include fields you are actually changing.
In MC or DJ mode you may add an optional "mc_line" string — a short crowd shout spoken via TTS, separate from "_comment". Keep it under 12 words. Use it for big moments, drops, or energy peaks.
TOP-LEVEL SCHEMA — the only valid top-level keys are "_comment", "mc_line", "bass", "sequencer", "fx".
  "bass" and "fx" are NEVER nested inside "sequencer".
  "fx" is NEVER nested inside "fx".
  Each key appears at most ONCE per object.

WRONG (do not do this):
  {{"sequencer": {{"bass_steps": [...], "bass": {{"cutoff": 0.3}}}}}}       ← bass inside sequencer
  {{"fx": {{"reverb_mix": 0.1, "fx": {{"delay_mix": 0.2}}}}}}              ← fx inside fx
  {{"sequencer": {{"bass_steps": [...], "fx": {{"reverb_mix": 0.1}}}}}}    ← fx inside sequencer

Example — "add claps on 2 and 4":
{{"_comment": "{clap_example}",
  "sequencer": {{"clap_b_steps": [false,false,false,false,true,false,false,false,false,false,false,false,true,false,false,false]}}}}

Example — "change the melody":
{{"_comment": "{melody_example}",
  "sequencer": {{"bass_steps": [true,false,true,false,false,true,false,true,false,false,true,false,false,true,false,false],
                 "bass_notes":  [36,36,36,36,36,41,36,43,36,36,38,36,36,36,40,36]}}}}

Example — "more acid":
{{"_comment": "{acid_example}",
  "bass": {{"resonance": 0.85, "env_mod": 0.80, "cutoff": 0.30, "decay": 0.25}}}}
"#,
        persona = persona,
        user_instructions_section = user_instructions_section,
        style_section = style_section,
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
                    "cutoff":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "LP filter cutoff, 0=200Hz, 1=20kHz" }
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
                    "filter_attack":      { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "filter ADSR attack 0-1 → 1ms-8s" },
                    "filter_decay":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "filter_sustain":     { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "filter_release":     { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "amp_attack":         { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "amp ADSR attack 0-1 → 1ms-8s. Use high values for slow pad attacks." },
                    "amp_decay":          { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "amp_sustain":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "amp_release":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
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
                    "delay_time":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "delay_feedback":   { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "delay_mix":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
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
                    "tape_flutter":          { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "wow/flutter depth — ±4% AM at 2.5Hz; adds vintage instability" }
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
                            "pitch_env_time":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "kick pitch drop decay time: 0=10ms 1=200ms" }
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
                            "pitch_env_time":  { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "kick pitch drop decay time: 0=10ms 1=200ms" }
                        },
                        "additionalProperties": false
                    }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}
