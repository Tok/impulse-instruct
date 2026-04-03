// ─── llm/prompt.rs ───────────────────────────────────────────────────────────
// Builds the system prompt that grounds the LLM in current synth state.

use crate::state::AppState;

pub fn build_system_prompt(state: &AppState) -> String {
    let locked: Vec<&str> = state.llm.locked_params.iter().map(|s| s.as_str()).collect();
    let locked_str = if locked.is_empty() {
        "none".to_string()
    } else {
        locked.join(", ")
    };

    let current_json = serde_json::to_string_pretty(&serde_json::json!({
        "tb303": {
            "cutoff": state.tb303.cutoff,
            "resonance": state.tb303.resonance,
            "env_mod": state.tb303.env_mod,
            "decay": state.tb303.decay,
            "accent_level": state.tb303.accent_level,
            "waveform": format!("{:?}", state.tb303.waveform),
            "distortion": state.tb303.distortion,
            "volume": state.tb303.volume
        },
        "sequencer": {
            "bpm": state.sequencer.bpm
        },
        "fx": {
            "reverb_size": state.fx.reverb_size,
            "reverb_mix": state.fx.reverb_mix,
            "delay_time": state.fx.delay_time,
            "delay_feedback": state.fx.delay_feedback,
            "delay_mix": state.fx.delay_mix,
            "distortion_drive": state.fx.distortion_drive,
            "distortion_mix": state.fx.distortion_mix
        }
    })).unwrap_or_default();

    format!(
        r#"You are Impulse Instruct, an AI audio synthesizer controller.
Your ONLY output is valid JSON that modifies synthesizer parameters.
Do NOT write explanations, prose, or markdown. Output JSON only.

CURRENT PARAMETERS:
{current_json}

LOCKED PARAMETERS (user-controlled, do NOT include these in output):
{locked_str}

AVAILABLE PARAMETERS (all floats 0.0–1.0 unless noted):
  tb303.cutoff         — filter cutoff (0=dark, 1=bright)
  tb303.resonance      — filter resonance / squelch (high=acid)
  tb303.env_mod        — filter envelope depth
  tb303.decay          — filter envelope decay
  tb303.accent_level   — accent velocity boost
  tb303.waveform       — "Saw" or "Square"
  tb303.distortion     — internal overdrive
  tb303.volume         — bass level

  sequencer.bpm        — tempo in BPM (float 40–250)

  fx.reverb_size       — room size
  fx.reverb_mix        — reverb wet/dry
  fx.delay_time        — delay time (0=0ms, 1=1000ms)
  fx.delay_feedback    — delay repeats
  fx.delay_mix         — delay wet/dry
  fx.distortion_drive  — master bus drive
  fx.distortion_mix    — distortion wet/dry

STYLE VOCABULARY:
  "acid"    → cutoff 0.3-0.6, resonance 0.7-0.9, env_mod 0.6-0.9, fast decay
  "dark"    → cutoff < 0.3, reverb_mix 0.4+, bpm 110-125
  "minimal" → fewer active steps, low resonance, moderate cutoff
  "hard"    → bpm 145+, distortion 0.3+, punch
  "ambient" → slow bpm, high reverb, long delay, gentle filter
  "squeeze" → compressor_threshold low, ratio high
  "glitch"  → rapid param variation, short decay

OUTPUT FORMAT: Only include fields you want to change. Example:
{{"tb303": {{"cutoff": 0.45, "resonance": 0.78}}, "sequencer": {{"bpm": 132.0}}}}"#,
        current_json = current_json,
        locked_str = locked_str,
    )
}

/// JSON Schema for grammar-constrained generation (used by llama-cpp-2).
pub fn param_json_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema",
        "type": "object",
        "properties": {
            "tb303": {
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
                    "bpm": { "type": "number", "minimum": 40.0, "maximum": 250.0 }
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
