// ─── llm/prompt.rs ───────────────────────────────────────────────────────────
// Builds the system prompt that grounds the LLM in current synth state.

use crate::llm::styles::StyleCatalog;
use crate::state::{AppState, ConversationMode, StyleVerbosity};

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
    let heat = state.llm.heat;
    let heat_pct = (heat * 100.0) as u32;
    let heat_desc = match heat {
        h if h < 0.25 => "cold — subtle incremental changes only, no pattern mutations",
        h if h < 0.5  => "warm — moderate evolution, occasional step changes",
        h if h < 0.75 => "hot — bold sweeps, pattern mutations, noticeable style shifts",
        _              => "fire — anything goes, dramatic mutations, surprise",
    };

    // Summarise active bass steps so the LLM can see what's playing
    let active_bass: Vec<usize> = state.sequencer.bass_pattern.iter().enumerate()
        .filter(|(_, s)| s.active).map(|(i, _)| i).collect();
    let bass_summary = if active_bass.is_empty() {
        "none (silent)".to_string()
    } else {
        active_bass.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
    };

    let current_json = serde_json::to_string_pretty(&serde_json::json!({
        "bass": {
            "cutoff": state.bass.cutoff,
            "resonance": state.bass.resonance,
            "env_mod": state.bass.env_mod,
            "decay": state.bass.decay,
            "accent_level": state.bass.accent_level,
            "waveform": format!("{:?}", state.bass.waveform),
            "distortion": state.bass.distortion,
            "volume": state.bass.volume
        },
        "sequencer": {
            "bpm": state.sequencer.bpm
        },
        "fx": {
            "reverb_mix": state.fx.reverb_mix,
            "delay_mix": state.fx.delay_mix,
            "distortion_drive": state.fx.distortion_drive,
            "distortion_mix": state.fx.distortion_mix
        }
    })).unwrap_or_default();
    // Bass pattern summary shown separately so the model doesn't treat it as an output field
    let bass_info = format!("Active bass steps (for reference only, not a JSON field): {}", bass_summary);

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
                format!(
                    "\n═══ ACTIVE STYLE ═══\n\n{}{}\nUse this as your creative brief. \
                     Evolve the current sound toward this aesthetic — don't reset everything at once.\n",
                    text, seed
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

    format!(
        r#"You are {persona} — the AI intelligence inside Impulse Instruct, a hardware-style synthesizer.
Output ONLY valid JSON. No prose, no markdown, no explanation outside the "_comment" field.
{style_section}{user_instructions_section}
CURRENT STATE:
{current_json}
{bass_info}

LOCKED (user-owned, never include in output): {locked_str}

═══ WHAT YOU CAN CONTROL ═══

BASS SYNTHESIZER (all 0.0–1.0):
  bass.cutoff       — filter frequency (0=very dark/closed, 0.5=mid, 1=fully open)
  bass.resonance    — filter resonance / squelch (0.7–0.9 = classic acid character)
  bass.env_mod      — how much the envelope opens the filter (high = dramatic sweep)
  bass.decay        — filter envelope decay time (low=punchy, high=slow sweep)
  bass.accent_level — accent intensity boost
  bass.waveform          — "Saw" (smooth, warm), "Square" (hollow, buzzy), or "Supersaw" (thick unison)
  bass.supersaw_detune   — 0–1 spread between supersaw voices (0=tight unison, 1=wide chorus)
  bass.supersaw_voices   — 2–7 unison voices (Supersaw mode only)
  bass.distortion        — internal overdrive (keep low; 0.0–0.15 is enough)
  bass.volume            — bass synth level in mix

STEP SEQUENCER (16 steps = one 4/4 bar of 16th notes):
  sequencer.steps         — total loop length in steps (8/16/32/64, default 16)
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

═══ RHYTHM BASICS ═══

Minimal 4/4 foundation (indices 0–15):
  kick_a_steps 4-on-the-floor: [true,false,false,false,true,false,false,false,true,false,false,false,true,false,false,false]
  hihat_a_steps offbeat 8ths:  [false,false,true,false,false,false,true,false,false,false,true,false,false,false,true,false]
Build from there — add syncopation and gaps. Never fill every step with the same drum.

BASS MELODY BASICS:
  Acid range C2–C3: C2=36, D2=38, Eb2=39, F2=41, G2=43, A2=45, Bb2=46, B2=47, C3=48
  Minor pentatonic (C): 36, 39, 41, 43, 46 (and 48 for octave)
  Keep to 3–5 distinct pitches per loop. Use false in bass_steps for rhythmic rests.

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

FX RESTRAINT — always start clean:
  Unless explicitly asked, keep FX minimal: reverb_mix ≤ 0.12, delay_mix ≤ 0.08, distortion at 0.0
  Never set heavy reverb + heavy delay + distortion simultaneously.

JAM HEAT: {heat_pct}% — {heat_desc}

═══ OUTPUT FORMAT ═══

Always start your response with "_thinking": one or two sentences explaining what the user is asking for and what specific parameters you will change. This is your reasoning scratch-pad — write it before anything else.
{comment_instruction}
Only include fields you are actually changing.
TOP-LEVEL SCHEMA — the only valid top-level keys are "_comment", "bass", "sequencer", "fx".
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
                 \"DJ Bonsai in the house, stepping up the BPM cos this crowd needs MORE!\"",
            ConversationMode::Mc =>
                "Always include \"_comment\" in character as a jungle/rave MC hyping the crowd. \
                 Short shoutouts, rave slang, aggressive energy. \
                 Examples: \"SELECTOR! junglist massive!\", \"REWIND that ting!\", \
                 \"BIG UP the bassline, massive massive!\", \"wheel it selector, wheel it up!\"",
        },
        clap_example = match state.llm.conversation_mode {
            ConversationMode::Off      => "clap909_steps updated",
            ConversationMode::Producer => "adding a 909 clap on beats 2 and 4 for a classic house feel",
            ConversationMode::Dj       => "CLAP CLAP CLAP just dropped the backbeat FEEL THAT",
            ConversationMode::Mc       => "SELECTOR! clap ting incoming, big up the backbeat massive!",
        },
        melody_example = match state.llm.conversation_mode {
            ConversationMode::Off      => "bass_steps and bass_notes updated",
            ConversationMode::Producer => "new bass line — stepping up a fifth and back with a chromatic passing note",
            ConversationMode::Dj       => "NEW BASSLINE JUST DROPPED who ordered the groove you're welcome",
            ConversationMode::Mc       => "WHEEL IT UP! fresh line incoming, junglist riddim massive!",
        },
        acid_example = match state.llm.conversation_mode {
            ConversationMode::Off      => "bass resonance and env_mod updated",
            ConversationMode::Producer => "cranking the resonance and env_mod for full acid squelch",
            ConversationMode::Dj       => "ACID ACID ACID your boy just went full 303 mode YOU ARE WELCOME",
            ConversationMode::Mc       => "REWIND! acid ting, selector pull up, junglist massive BWOY!",
        },
    )
}

/// JSON Schema for grammar-constrained generation (used by llama-cpp-2).
pub fn param_json_schema() -> serde_json::Value {
    let bool_array = serde_json::json!({ "type": "array", "items": { "type": "boolean" }, "maxItems": 16 });
    let note_array = serde_json::json!({ "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 127 }, "maxItems": 16 });
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema",
        "type": "object",
        "properties": {
            "_thinking": { "type": "string", "maxLength": 300 },
            "_comment": { "type": "string", "maxLength": 200 },
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
                    "volume":            { "type": "number", "minimum": 0.0, "maximum": 1.0 }
                },
                "additionalProperties": false
            },
            "sequencer": {
                "type": "object",
                "properties": {
                    "bpm":           { "type": "number", "minimum": 40.0, "maximum": 250.0 },
                    "steps":         { "type": "integer", "minimum": 8, "maximum": 64, "multipleOf": 8 },
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
                    "bitcrush_mix":  { "type": "number", "minimum": 0.0, "maximum": 1.0 }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}
