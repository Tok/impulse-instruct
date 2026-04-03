// ─── llm/prompt.rs ───────────────────────────────────────────────────────────
// Builds the system prompt that grounds the LLM in current synth state.

use crate::state::{AppState, ConversationMode};

pub fn build_system_prompt(state: &AppState) -> String {
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
            "bpm": state.sequencer.bpm,
            "active_bass_steps": bass_summary
        },
        "fx": {
            "reverb_mix": state.fx.reverb_mix,
            "delay_mix": state.fx.delay_mix,
            "distortion_drive": state.fx.distortion_drive,
            "distortion_mix": state.fx.distortion_mix
        }
    })).unwrap_or_default();

    format!(
        r#"You are Impulse Instruct — an AI that controls a hardware-style synthesizer.
Output ONLY valid JSON. No prose, no markdown, no explanation outside the "_comment" field.

CURRENT STATE:
{current_json}

LOCKED (user-owned, never include in output): {locked_str}

═══ WHAT YOU CAN CONTROL ═══

BASS SYNTHESIZER (all 0.0–1.0):
  bass.cutoff       — filter frequency (0=very dark/closed, 0.5=mid, 1=fully open)
  bass.resonance    — filter resonance / squelch (0.7–0.9 = classic acid character)
  bass.env_mod      — how much the envelope opens the filter (high = dramatic sweep)
  bass.decay        — filter envelope decay time (low=punchy, high=slow sweep)
  bass.accent_level — accent intensity boost
  bass.waveform     — "Saw" (smooth, warm) or "Square" (hollow, buzzy)
  bass.distortion   — internal overdrive (keep low; 0.0–0.15 is enough)
  bass.volume       — bass synth level in mix

STEP SEQUENCER (16 steps = one 4/4 bar of 16th notes):
  sequencer.bass_steps       — 16-element bool array: which steps trigger the 303
  sequencer.bass_notes       — 16-element int array: MIDI note per step
                               (24=C1, 36=C2, 48=C3; typical range 33–48 for acid)
  sequencer.kick808_steps    — 16-element bool: 808 kick pattern
  sequencer.snare808_steps   — 16-element bool: 808 snare pattern
  sequencer.hihat808_steps   — 16-element bool: 808 closed hihat
  sequencer.snare909_steps   — 16-element bool: 909 snare pattern
  sequencer.clap909_steps    — 16-element bool: 909 clap pattern
  sequencer.hihat909_steps   — 16-element bool: 909 closed hihat

FX (all 0.0–1.0):
  fx.reverb_mix      — reverb wet amount (0=off, 0.3=noticeable)
  fx.reverb_size     — reverb room size
  fx.delay_time      — delay time (0.375 = dotted 8th at ~130 BPM)
  fx.delay_feedback  — delay repeats
  fx.delay_mix       — delay wet amount
  fx.distortion_drive — master bus saturation drive
  fx.distortion_mix  — master bus distortion wet amount

═══ HOW TO INTERPRET INSTRUCTIONS ═══

"change the melody" / "different pattern" / "new notes"
  → Set bass_steps to a new 16-step pattern, set bass_notes to MIDI pitches

"add claps" / "add snare"
  → Set clap909_steps or snare808_steps to a useful drum pattern

"add hihats" / "more hats"
  → Set hihat808_steps or hihat909_steps

"more acid" / "squelchier"
  → Raise bass.resonance (0.75–0.88), raise bass.env_mod, lower bass.cutoff

"darker" / "more weight"
  → Lower bass.cutoff, raise reverb_mix slightly

"add space" / "more atmosphere"
  → Raise reverb_mix (0.2–0.4), add delay_mix (0.1–0.25)

"harder" / "more drive"
  → Raise distortion_drive + distortion_mix

"simpler" / "strip it back"
  → Reduce active bass_steps, remove some drum steps

JAM HEAT: {heat_pct}% — {heat_desc}

═══ OUTPUT FORMAT ═══

{comment_instruction}
Only include fields you are actually changing.

Example — "add claps on 2 and 4":
{{"_comment": "{clap_example}",
  "sequencer": {{"clap909_steps": [false,false,false,false,true,false,false,false,false,false,false,false,true,false,false,false]}}}}

Example — "change the melody":
{{"_comment": "{melody_example}",
  "sequencer": {{"bass_steps": [true,false,true,false,false,true,false,true,false,false,true,false,false,true,false,false],
                 "bass_notes":  [36,36,36,36,36,41,36,43,36,36,38,36,36,36,40,36]}}}}

Example — "more acid":
{{"_comment": "{acid_example}",
  "bass": {{"resonance": 0.85, "env_mod": 0.80, "cutoff": 0.35}}}}
"#,
        current_json = current_json,
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
            ConversationMode::Dj       => "CLAP CLAP CLAP DJ Bonsai just dropped the backbeat FEEL THAT",
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
            "_comment": { "type": "string", "maxLength": 200 },
            "bass": {
                "type": "object",
                "properties": {
                    "cutoff":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "resonance":    { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "env_mod":      { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "decay":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "accent_level": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "waveform":     { "type": "string", "enum": ["Saw", "Square"] },
                    "distortion":   { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "volume":       { "type": "number", "minimum": 0.0, "maximum": 1.0 }
                },
                "additionalProperties": false
            },
            "sequencer": {
                "type": "object",
                "properties": {
                    "bpm":              { "type": "number", "minimum": 40.0, "maximum": 250.0 },
                    "bass_steps":       bool_array.clone(),
                    "bass_notes":       note_array,
                    "kick808_steps":    bool_array.clone(),
                    "snare808_steps":   bool_array.clone(),
                    "hihat808_steps":   bool_array.clone(),
                    "snare909_steps":   bool_array.clone(),
                    "clap909_steps":    bool_array.clone(),
                    "hihat909_steps":   bool_array
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
                    "distortion_mix":   { "type": "number", "minimum": 0.0, "maximum": 1.0 }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}
