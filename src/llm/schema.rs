// ─── llm/schema.rs ────────────────────────────────────────────────────────────
// JSON Schema for grammar-constrained LLM generation.
// Extracted from prompt.rs to stay under the line limit.

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
                    "fm_ratio":          { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "FM modulator/carrier ratio 0->0.5x (sub-harmonic), 0.13->1x (unison FM), 0.2->2x (octave), 1.0->8x (bell/metallic)" },
                    "pan":               { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "stereo pan: -1=left, 0=center, +1=right" }
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
                    "sh_depth": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "S&H modulation depth on filter" },
                    "pan":      { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "stereo pan: -1=left, 0=center, +1=right" }
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
                    "target":    { "type": "string", "enum": ["None","BassCutoff","BassResonance","BassPitch","BassVolume","ReverbMix","DelayTime","DelayFeedback","ChorusMix","ChorusRate","Kick808Pitch","PhaserRate","PhaserDepth","DistortionDrive","MasterVolume","An1xCutoff","An1xPitch"] },
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
                    "pan":                { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "stereo pan: -1=left, 0=center, +1=right" },
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
                    "pan":              { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "stereo pan: -1=left, 0=center, +1=right" },
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
                    "stereo_width": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "stereo width: 0=mono, 0.5=normal, 1=wide" },
                    "tuning": { "type": "integer", "minimum": 0, "maximum": 3, "description": "tuning system: 0=12-TET (default), 1=just intonation, 2=slendro (gamelan), 3=pelog (gamelan)" }
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
                            "clip":            { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "gabber hard-clip drive: 0=clean 1=full distortion (flat-top sine)" },
                            "pan":             { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "stereo pan: -1=left, 0=center, +1=right" }
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
                            "clip":            { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "gabber hard-clip drive: 0=clean 1=full distortion (flat-top sine)" },
                            "pan":             { "type": "number", "minimum": -1.0, "maximum": 1.0, "description": "stereo pan: -1=left, 0=center, +1=right" }
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
            "description": "Schedule a smooth parameter transition. Use 'bars' for real-time bar-synced interpolation, or 'cycles' for jam-cycle pacing.",
            "properties": {
                "param":  { "type": "string", "description": "Dot-path of the parameter, e.g. 'fx.reverb_mix', 'bass.cutoff', 'sequencer.bpm'" },
                "to":     { "type": "number", "description": "Target value to ramp toward" },
                "from":   { "type": "number", "description": "Starting value (optional, defaults to current param value)" },
                "bars":   { "type": "number", "minimum": 1, "description": "Duration in musical bars — smoothly interpolated at frame rate (preferred)" },
                "cycles": { "type": "number", "minimum": 1, "description": "Duration in jam cycles (legacy; use 'bars' for smoother results)" }
            },
            "required": ["param", "to"],
            "additionalProperties": false
        },
        "ramps": {
            "type": "array",
            "description": "Schedule multiple smooth parameter transitions at once. Each element has the same format as 'ramp'.",
            "items": {
                "type": "object",
                "properties": {
                    "param":  { "type": "string" },
                    "to":     { "type": "number" },
                    "from":   { "type": "number" },
                    "bars":   { "type": "number", "minimum": 1 },
                    "cycles": { "type": "number", "minimum": 1 }
                },
                "required": ["param", "to"]
            }
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
