// ─── llm/lanes.rs ────────────────────────────────────────────────────────────
// Lane-aware prompt + schema builders for the sequential inference pipeline.
//
// A "lane" is a single focused LLM call that owns a narrow slice of the synth
// (one bass voice, the 808 kit, the FX bus, …).  Splitting a user turn into
// a sequence of lane calls beats one giant monolithic inference because:
//   • each call's output is short → decode time shrinks proportionally
//   • the schema requires only the lane's output fields → no skipping accents
//   • per-lane failure doesn't kill the whole turn
//   • user sees audible progress as each lane lands
//
// This module supplies the LaneKind enum + the per-lane prompt/schema
// builders.  The pipeline executor (Phase 4) consumes them; the monolithic
// inference path stays untouched in this phase.

use crate::llm::prompt_summary::{
    bass_active_steps_summary, bass_groove_summary, bass_voices_summary,
    rack_voice_coverage_summary,
};
use crate::state::{AppState, ROOT_NAMES};

/// Which slice of the synth a given inference call is responsible for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneKind {
    /// Top-level settings: style, bpm, jam_bars, persona, conversation_mode,
    /// and the `music_api` / `behaviour` / `ramps` top-level stanzas.  The
    /// planner (Phase 3) typically emits this lane first to set the scene.
    Settings,
    /// A single bass voice.  Index 0 = bass #1 (legacy unnumbered keys),
    /// 1 = bass #2 (`bass2_*`), 2 = bass #3, 3 = bass #4.
    Bass(usize),
    /// Kit A (808): kick_a / snare_a / hihat_a steps + ratchets + per-voice velocities.
    KitA,
    /// Kit B (909): kick_b / snare_b / clap_b / hihat_b steps + ratchets.
    KitB,
    /// Amen break sampler — amen_steps, amen_slices, amen voice params.
    Amen,
    /// Hoover lead voice — hoover_steps, hoover_notes, hoover synth params.
    Hoover,
    /// AN1X voice — an1x_steps, an1x_notes, an1x synth params.
    An1x,
    /// FX bus: reverb / delay / distortion / chorus / bitcrush / master_pitch.
    Fx,
    /// Modulation layer: LFO slots + free_eg.
    Modulation,
    /// Rack routing: add / remove / enable / disable modules; cables; mod_cable.
    Rack,
}

impl LaneKind {
    /// Short stable label for logs / telemetry / UI progress updates.
    pub fn label(self) -> &'static str {
        match self {
            LaneKind::Settings => "settings",
            LaneKind::Bass(0) => "bass1",
            LaneKind::Bass(1) => "bass2",
            LaneKind::Bass(2) => "bass3",
            LaneKind::Bass(3) => "bass4",
            LaneKind::Bass(_) => "bassN",
            LaneKind::KitA => "kit_a",
            LaneKind::KitB => "kit_b",
            LaneKind::Amen => "amen",
            LaneKind::Hoover => "hoover",
            LaneKind::An1x => "an1x",
            LaneKind::Fx => "fx",
            LaneKind::Modulation => "mod",
            LaneKind::Rack => "rack",
        }
    }

    /// Top-level JSON keys this lane is allowed to emit.  Used both to build
    /// the per-lane schema (required fields) and to filter LLM output so an
    /// off-topic emission doesn't leak into the wrong lane's state write.
    pub fn output_keys(self) -> &'static [&'static str] {
        match self {
            LaneKind::Settings => &["settings", "music_api", "behaviour", "ramp", "ramps"],
            LaneKind::Bass(_) => &["bass", "bass_voices", "sequencer"],
            LaneKind::KitA | LaneKind::KitB => &["sequencer"],
            LaneKind::Amen => &["amen", "sequencer"],
            LaneKind::Hoover => &["hoover", "sequencer"],
            LaneKind::An1x => &["an1x", "sequencer"],
            LaneKind::Fx => &["fx"],
            LaneKind::Modulation => &["lfo", "free_eg"],
            LaneKind::Rack => &["rack"],
        }
    }

    /// Sequencer subkeys this lane owns.  Used to scope `sequencer.*` updates
    /// — e.g. the Bass(1) lane owns `bass2_steps/notes/accents/slides/pans`
    /// but must not touch `kick_a_steps` even if the model tries.
    pub fn sequencer_subkeys(self) -> Vec<String> {
        match self {
            LaneKind::Bass(idx) => {
                let prefix = if idx == 0 {
                    "bass".to_string()
                } else {
                    format!("bass{}", idx + 1)
                };
                vec![
                    format!("{}_steps", prefix),
                    format!("{}_notes", prefix),
                    format!("{}_accents", prefix),
                    format!("{}_slides", prefix),
                    format!("{}_pans", prefix),
                    format!("{}_len", prefix),
                ]
            }
            LaneKind::KitA => vec![
                "kick_a_steps".into(),
                "snare_a_steps".into(),
                "hihat_a_steps".into(),
                "hihat_a_open_steps".into(),
                "drum_lengths".into(),
                "drum_ratchets".into(),
                "preecho".into(),
            ],
            LaneKind::KitB => vec![
                "kick_b_steps".into(),
                "snare_b_steps".into(),
                "clap_b_steps".into(),
                "hihat_b_steps".into(),
                "hihat_b_open_steps".into(),
                "drum_lengths".into(),
                "drum_ratchets".into(),
                "preecho".into(),
            ],
            LaneKind::Amen => vec![
                "amen_steps".into(),
                "amen_slices".into(),
                "amen_len".into(),
                "preecho".into(),
            ],
            LaneKind::Hoover => vec![
                "hoover_steps".into(),
                "hoover_notes".into(),
                "hoover_len".into(),
                "preecho".into(),
            ],
            LaneKind::An1x => vec![
                "an1x_steps".into(),
                "an1x_notes".into(),
                "an1x_len".into(),
                "preecho".into(),
            ],
            LaneKind::Settings => vec![
                "bpm".into(),
                "steps".into(),
                "swing".into(),
                "time_sig_num".into(),
                "root_note".into(),
                "scale".into(),
                "scale_snap".into(),
            ],
            LaneKind::Fx | LaneKind::Modulation | LaneKind::Rack => Vec::new(),
        }
    }

    /// Human-readable one-sentence task description used as the lane prompt
    /// footer.  Kept deliberately short — the focused-task framing is what
    /// keeps the model from wandering into other lanes.
    pub fn task_description(self) -> String {
        match self {
            LaneKind::Settings => "Set the global settings for this jam: style id, bpm, swing, \
                root_note, scale. Emit only the `settings` object and the `sequencer` \
                globals (bpm/steps/swing/time_sig_num/root_note/scale). \
                Do not write any voice patterns in this call."
                .into(),
            LaneKind::Bass(idx) => {
                let prefix = if idx == 0 {
                    "bass".to_string()
                } else {
                    format!("bass{}", idx + 1)
                };
                let counterpoint = if idx == 0 {
                    "Establish the main groove — later bass voices will counterpoint this."
                        .to_string()
                } else {
                    format!(
                        "Bass #{} is the SECONDARY voice — counterpoint bass #1 rather than \
                         doubling it. Different rhythm (fills where #1 rests), different \
                         register (up a 5th or octave is a safe default), but the SAME key.",
                        idx + 1,
                    )
                };
                format!(
                    "Write ONLY the pattern for bass voice #{voice}: `{prefix}_steps`, \
                     `{prefix}_notes`, `{prefix}_accents`, `{prefix}_slides`, `{prefix}_pans`. \
                     Also tune the synth via {synth_path}. Emit all four required arrays — \
                     accents/slides indices must be a subset of steps, and slides only \
                     between different-note steps. {counterpoint} Do not write drums, FX, \
                     or other bass voices in this call.",
                    voice = idx + 1,
                    synth_path = if idx == 0 {
                        "`bass`".to_string()
                    } else {
                        format!("`bass_voices[{idx}]`")
                    },
                )
            }
            LaneKind::KitA => "Write ONLY the 808 kit patterns: `sequencer.kick_a_steps`, \
                `sequencer.snare_a_steps`, `sequencer.hihat_a_steps` (and open if needed). \
                Match the active style's groove. Do not write bass, FX, or kit B."
                .into(),
            LaneKind::KitB => "Write ONLY the 909 kit patterns: `sequencer.kick_b_steps`, \
                `sequencer.snare_b_steps`, `sequencer.clap_b_steps`, `sequencer.hihat_b_steps`. \
                Do not write bass, FX, or kit A."
                .into(),
            LaneKind::Amen => "Write ONLY the amen sampler pattern (`sequencer.amen_steps`, \
                `amen_slices`) and amen voice params. Do not write other voices."
                .into(),
            LaneKind::Hoover => "Write ONLY the hoover lead: `sequencer.hoover_steps`, \
                `sequencer.hoover_notes`, and hoover voice params. Keep notes in the key \
                above — a 5th or octave above the bass register usually sits well as a \
                rave lead. Do not write other voices."
                .into(),
            LaneKind::An1x => "Write ONLY the AN1X pad/lead: `sequencer.an1x_steps`, \
                `sequencer.an1x_notes`, and an1x voice params. Harmonise with the bass — \
                third, fifth, or root-octave of the bass's current note on each shared \
                step. Do not write other voices."
                .into(),
            LaneKind::Fx => "Write ONLY the `fx` object — reverb / delay / distortion / \
                chorus / bitcrush / master_pitch. Keep values modest unless the style or \
                user asks for extremes. Do not write any patterns."
                .into(),
            LaneKind::Modulation => "Write ONLY the `lfo` array and/or `free_eg` object for \
                modulation. Pick one or two targets that suit the current groove. Do not \
                write patterns or synth params."
                .into(),
            LaneKind::Rack => "Write ONLY a `rack` object with add/remove/enable/disable/\
                connect/disconnect/mod_cable entries as needed. Do not write anything else."
                .into(),
        }
    }
}

/// Build the lane-specific system prompt.  Starts with a compact shared
/// context block (state snapshot, key, active voices, locked params) and
/// ends with the lane's task description.  Intentionally much shorter than
/// `build_system_prompt` — a lane prompt averages ~1-2 k tokens vs the
/// monolithic ~5 k.
pub fn build_lane_prompt(state: &AppState, lane: LaneKind) -> String {
    let locked: Vec<&str> = state.llm.locked_params.iter().map(|s| s.as_str()).collect();
    let locked_str = if locked.is_empty() {
        "none".to_string()
    } else {
        locked.join(", ")
    };
    let heat_pct = (state.llm.heat * 100.0) as u32;

    // Minimal state header — BPM, steps, key, active voice count.  The
    // monolithic prompt's current_json dump is too verbose for per-lane
    // calls; each lane only needs a few fields.
    let root_name = ROOT_NAMES
        .get(state.sequencer.root_note as usize)
        .copied()
        .unwrap_or("C");
    let state_header = format!(
        "bpm={:.0} steps={} swing={:.2} key={} {} heat={}%",
        state.sequencer.bpm,
        state.sequencer.steps,
        state.sequencer.swing,
        root_name,
        state.sequencer.scale.name(),
        heat_pct,
    );

    // Harmonic context — every melodic lane gets the scale notes in the
    // C2–C3 range as a concrete palette.  Without this, voice #2 can
    // wander into a different key than voice #1 or the hoover/an1x lead.
    // The rule is baked in as "stay in key unless the style explicitly
    // asks otherwise" so overrides are possible but never the default.
    let harmony_block = if matches!(lane, LaneKind::Bass(_) | LaneKind::Hoover | LaneKind::An1x) {
        let scale_notes = scale_notes_in_c2_c3(state);
        format!(
            "\nHARMONY: key {} {}. In-key MIDI notes (C2–C3): [{}]. Default: stay \
             in this scale so voices harmonise with each other. Break key only if \
             the style brief or the user prompt asks for it.\n",
            root_name,
            state.sequencer.scale.name(),
            scale_notes,
        )
    } else {
        String::new()
    };

    let style_hint = match state.llm.active_style.as_deref() {
        None => String::new(),
        Some("__free__") => "\nSTYLE: free — pick your own direction.\n".to_string(),
        Some("__custom__") => format!("\nSTYLE (custom): {}\n", state.llm.custom_style_text.trim()),
        Some(id) => match crate::llm::styles::StyleCatalog::get().find_by_id(id) {
            Some(s) => {
                // Use `brief` when available — shorter is better for lane calls.
                let text = if !s.brief.is_empty() {
                    s.brief.as_str()
                } else {
                    s.description.as_str()
                };
                format!("\nSTYLE: {} — {}\n", s.name, text)
            }
            None => format!("\nSTYLE: {}\n", id),
        },
    };

    // Bass-lane-specific context: show the bass voice summary so the model
    // knows how many voices are live.  Other lanes don't need it.
    let bass_context = if matches!(lane, LaneKind::Bass(_)) {
        format!(
            "\n{}\n{}\n{}\n",
            bass_voices_summary(state),
            bass_active_steps_summary(state),
            bass_groove_summary(state),
        )
    } else {
        String::new()
    };

    // Coverage hint is only useful to the settings / top-level planner
    // — individual lanes already know what they own.
    let coverage = if lane == LaneKind::Settings {
        format!("\n{}\n", rack_voice_coverage_summary(state))
    } else {
        String::new()
    };

    format!(
        "You are the {lane_label} writer inside Impulse Instruct. Output ONLY valid JSON \
         matching the schema. No prose outside `_comment`.\n\
         \n\
         STATE: {state_header}\n\
         LOCKED (never overwrite): {locked_str}\n\
         {style_hint}{coverage}{harmony_block}{bass_context}\n\
         TASK: {task}\n\
         \n\
         Always start with `_thinking` (one short sentence). Keep `_comment` concise. \
         Emit every required field — missing fields break the jam.",
        lane_label = lane.label(),
        state_header = state_header,
        locked_str = locked_str,
        style_hint = style_hint,
        coverage = coverage,
        harmony_block = harmony_block,
        bass_context = bass_context,
        task = lane.task_description(),
    )
}

/// Return a comma-separated list of MIDI notes in the C2–C3 range that
/// belong to the current scale.  Mirrors the `APPLYING THE KEY` block in
/// the monolithic prompt — each melodic lane uses this as its note
/// palette so voices stay in the same key by default.
fn scale_notes_in_c2_c3(state: &AppState) -> String {
    let root = state.sequencer.root_note;
    let scale = state.sequencer.scale;
    let intervals = scale.intervals();
    let mut out: Vec<String> = Vec::new();
    // MIDI 36 = C2, 48 = C3 — inclusive.
    for midi in 36u8..=48 {
        let semi_from_root = (midi as i32 - root as i32).rem_euclid(12);
        if intervals.contains(&(semi_from_root as u8)) {
            out.push(midi.to_string());
        }
    }
    out.join(", ")
}

/// Per-lane JSON schema.  The crucial property: fields the lane is meant
/// to emit are marked `required`, so grammar-constrained generation can't
/// skip them (which was the failure mode of the monolithic prompt for
/// `bass_accents` / `bass_slides`).  Fields outside the lane's scope are
/// not even listed in `properties`, making `additionalProperties: false`
/// a hard ceiling.
pub fn lane_schema(lane: LaneKind) -> serde_json::Value {
    let bool_array =
        serde_json::json!({ "type": "array", "items": { "type": "boolean" }, "maxItems": 64 });
    let note_array = serde_json::json!({ "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 127 }, "maxItems": 64, "minItems": 1 });
    let intensity_array = serde_json::json!({ "type": "array", "items": { "type": "number", "minimum": 0.0, "maximum": 1.0 }, "maxItems": 64 });
    let pan_array = serde_json::json!({ "type": "array", "items": { "type": "number", "minimum": -1.0, "maximum": 1.0 }, "maxItems": 64 });

    let common_top = |props: serde_json::Value, required: Vec<&str>| -> serde_json::Value {
        // Every lane also permits a `_thinking` preamble and a `_comment`
        // (required in chatty modes, optional-but-present in Off mode).
        let mut props_obj = props.as_object().cloned().unwrap_or_default();
        props_obj.insert(
            "_thinking".into(),
            serde_json::json!({ "type": "string", "maxLength": 200 }),
        );
        props_obj.insert(
            "_comment".into(),
            serde_json::json!({ "type": "string", "maxLength": 200 }),
        );
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema",
            "type": "object",
            "properties": serde_json::Value::Object(props_obj),
            "required": required,
            "additionalProperties": false,
        })
    };

    match lane {
        LaneKind::Bass(idx) => {
            let prefix = if idx == 0 {
                "bass".to_string()
            } else {
                format!("bass{}", idx + 1)
            };
            let steps_key = format!("{prefix}_steps");
            let notes_key = format!("{prefix}_notes");
            let accents_key = format!("{prefix}_accents");
            let slides_key = format!("{prefix}_slides");
            let pans_key = format!("{prefix}_pans");
            let mut seq_props = serde_json::Map::new();
            seq_props.insert(steps_key.clone(), bool_array.clone());
            seq_props.insert(notes_key.clone(), note_array.clone());
            seq_props.insert(accents_key.clone(), intensity_array.clone());
            seq_props.insert(slides_key.clone(), intensity_array.clone());
            seq_props.insert(pans_key.clone(), pan_array.clone());
            let sequencer = serde_json::json!({
                "type": "object",
                "properties": serde_json::Value::Object(seq_props),
                "required": [steps_key, notes_key, accents_key, slides_key],
                "additionalProperties": false,
            });
            // The voice's synth-param block — optional per-lane but when
            // present, matches the main `bass` schema.  We keep it loose
            // so the lane can skip synth tuning on a pure pattern update.
            let bass_synth = serde_json::json!({
                "type": "object",
                "properties": {
                    "cutoff":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "resonance":    { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "env_mod":      { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "decay":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "accent_level": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "waveform":     { "type": "string", "enum": ["Saw", "Square", "Supersaw"] },
                    "filter_mode":  { "type": "string", "enum": ["Lowpass", "Highpass", "Bandpass"] },
                    "volume":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "pan":          { "type": "number", "minimum": -1.0, "maximum": 1.0 },
                    "sub_osc_level":    { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "portamento_time":  { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                    "distortion":       { "type": "number", "minimum": 0.0, "maximum": 1.0 }
                },
                "additionalProperties": false,
            });
            if idx == 0 {
                common_top(
                    serde_json::json!({
                        "bass": bass_synth,
                        "sequencer": sequencer,
                    }),
                    vec!["sequencer"],
                )
            } else {
                // For voices 1..=3 the synth tweak goes via bass_voices[idx].
                let mut voices_items = vec![serde_json::Value::Null; idx + 1];
                voices_items[idx] = bass_synth;
                let _ = voices_items; // placeholder — JSON schema has no easy "exactly N nulls
                // then synth" form.  Use a looser array-of-objects shape
                // and rely on the apply layer to dispatch by index.
                let bass_voices = serde_json::json!({
                    "type": "array",
                    "items": {
                        "type": ["object", "null"],
                    },
                    "maxItems": 4,
                });
                common_top(
                    serde_json::json!({
                        "bass_voices": bass_voices,
                        "sequencer": sequencer,
                    }),
                    vec!["sequencer"],
                )
            }
        }
        LaneKind::KitA => {
            let sequencer = serde_json::json!({
                "type": "object",
                "properties": {
                    "kick_a_steps":  bool_array.clone(),
                    "snare_a_steps": bool_array.clone(),
                    "hihat_a_steps": bool_array.clone(),
                },
                "required": ["kick_a_steps"],
                "additionalProperties": false,
            });
            common_top(
                serde_json::json!({ "sequencer": sequencer }),
                vec!["sequencer"],
            )
        }
        LaneKind::KitB => {
            let sequencer = serde_json::json!({
                "type": "object",
                "properties": {
                    "kick_b_steps":  bool_array.clone(),
                    "snare_b_steps": bool_array.clone(),
                    "clap_b_steps":  bool_array.clone(),
                    "hihat_b_steps": bool_array.clone(),
                },
                "required": ["kick_b_steps"],
                "additionalProperties": false,
            });
            common_top(
                serde_json::json!({ "sequencer": sequencer }),
                vec!["sequencer"],
            )
        }
        LaneKind::Amen => common_top(
            serde_json::json!({
                "amen": { "type": "object" },
                "sequencer": {
                    "type": "object",
                    "properties": { "amen_steps": bool_array.clone() },
                    "additionalProperties": true,
                }
            }),
            Vec::new(),
        ),
        LaneKind::Hoover => common_top(
            serde_json::json!({
                "hoover": { "type": "object" },
                "sequencer": {
                    "type": "object",
                    "properties": {
                        "hoover_steps": bool_array.clone(),
                        "hoover_notes": note_array.clone(),
                    },
                    "additionalProperties": true,
                }
            }),
            Vec::new(),
        ),
        LaneKind::An1x => common_top(
            serde_json::json!({
                "an1x": { "type": "object" },
                "sequencer": {
                    "type": "object",
                    "properties": {
                        "an1x_steps": bool_array,
                        "an1x_notes": note_array,
                    },
                    "additionalProperties": true,
                }
            }),
            Vec::new(),
        ),
        LaneKind::Fx => common_top(
            serde_json::json!({
                "fx": {
                    "type": "object",
                    "properties": {
                        "reverb_mix":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "reverb_size":      { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "delay_time":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "delay_feedback":   { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "delay_mix":        { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "distortion_drive": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "distortion_mix":   { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "chorus_mix":       { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "chorus_rate":      { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "chorus_depth":     { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "bitcrush_bits":    { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "master_pitch_st":  { "type": "number", "minimum": -12.0, "maximum": 12.0 }
                    },
                    "additionalProperties": false,
                }
            }),
            vec!["fx"],
        ),
        LaneKind::Modulation => common_top(
            serde_json::json!({
                "lfo": { "type": "array", "maxItems": 4 },
                "free_eg": { "type": "object" },
            }),
            Vec::new(),
        ),
        LaneKind::Rack => common_top(
            serde_json::json!({ "rack": { "type": "object" } }),
            vec!["rack"],
        ),
        LaneKind::Settings => common_top(
            serde_json::json!({
                "settings": { "type": "object" },
                "sequencer": {
                    "type": "object",
                    "properties": {
                        "bpm": { "type": "number", "minimum": 40.0, "maximum": 250.0 },
                        "swing": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
                        "steps": { "type": "integer", "minimum": 1, "maximum": 64 },
                        "root_note": { "type": "integer", "minimum": 0, "maximum": 11 },
                        "scale": { "type": "string" },
                        "time_sig_num": { "type": "integer", "minimum": 2, "maximum": 9 }
                    },
                    "additionalProperties": false,
                }
            }),
            Vec::new(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    #[test]
    fn every_lane_has_a_label() {
        for lane in all_lanes_for_test() {
            assert!(!lane.label().is_empty(), "{lane:?} has empty label");
        }
    }

    #[test]
    fn every_lane_has_a_non_empty_task_description() {
        for lane in all_lanes_for_test() {
            let desc = lane.task_description();
            assert!(!desc.is_empty(), "{lane:?} has empty task_description");
            assert!(
                desc.len() < 600,
                "{lane:?} task_description too long ({} chars)",
                desc.len()
            );
        }
    }

    #[test]
    fn bass_lane_schema_requires_per_voice_arrays() {
        // Bass(0) — the voice-0 schema must require all four pattern arrays
        // so grammar-constrained generation can't skip accents/slides.
        let schema = lane_schema(LaneKind::Bass(0));
        let seq = schema
            .pointer("/properties/sequencer/required")
            .expect("sequencer required list");
        let arr = seq.as_array().expect("required is an array");
        let required: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        assert!(required.contains(&"bass_steps"));
        assert!(required.contains(&"bass_notes"));
        assert!(required.contains(&"bass_accents"));
        assert!(required.contains(&"bass_slides"));
    }

    #[test]
    fn bass2_lane_uses_numbered_keys() {
        let schema = lane_schema(LaneKind::Bass(1));
        let seq = schema
            .pointer("/properties/sequencer/required")
            .expect("sequencer required");
        let arr = seq.as_array().unwrap();
        let required: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        assert!(required.contains(&"bass2_steps"));
        assert!(required.contains(&"bass2_accents"));
        assert!(required.contains(&"bass2_slides"));
    }

    #[test]
    fn bass_lane_prompt_mentions_the_voice() {
        let s = AppState::default();
        let prompt = build_lane_prompt(&s, LaneKind::Bass(0));
        assert!(prompt.contains("bass1"));
        assert!(prompt.contains("bass_steps"));
        assert!(prompt.contains("bass_accents"));
        let prompt2 = build_lane_prompt(&s, LaneKind::Bass(1));
        assert!(prompt2.contains("bass2"));
        assert!(prompt2.contains("bass2_steps"));
    }

    #[test]
    fn lane_prompts_are_compact() {
        // Each lane prompt should be well under the monolithic prompt — we
        // specifically want these short so the pipeline's per-call prefill
        // is cheap.  Asserting < 4 k chars sanity-checks that we haven't
        // accidentally inherited the old bloat.
        let s = AppState::default();
        for lane in all_lanes_for_test() {
            let p = build_lane_prompt(&s, lane);
            assert!(
                p.len() < 4000,
                "{lane:?} prompt is {} chars, should be < 4000",
                p.len()
            );
        }
    }

    #[test]
    fn kit_lanes_require_the_anchor_step_array() {
        let a = lane_schema(LaneKind::KitA);
        let req = a
            .pointer("/properties/sequencer/required")
            .and_then(|v| v.as_array())
            .unwrap();
        assert!(req.iter().any(|v| v == "kick_a_steps"));

        let b = lane_schema(LaneKind::KitB);
        let req = b
            .pointer("/properties/sequencer/required")
            .and_then(|v| v.as_array())
            .unwrap();
        assert!(req.iter().any(|v| v == "kick_b_steps"));
    }

    #[test]
    fn fx_lane_requires_fx_object() {
        let s = lane_schema(LaneKind::Fx);
        let req = s.pointer("/required").and_then(|v| v.as_array()).unwrap();
        assert!(req.iter().any(|v| v == "fx"));
    }

    #[test]
    fn output_keys_nonempty_for_every_lane() {
        for lane in all_lanes_for_test() {
            assert!(
                !lane.output_keys().is_empty(),
                "{lane:?} has no output keys"
            );
        }
    }

    #[test]
    fn melodic_lanes_include_harmony_block() {
        // Bass / hoover / an1x must see the key and the in-key note palette
        // so they harmonise with each other by default.
        let s = AppState::default();
        for lane in [
            LaneKind::Bass(0),
            LaneKind::Bass(1),
            LaneKind::Hoover,
            LaneKind::An1x,
        ] {
            let p = build_lane_prompt(&s, lane);
            assert!(
                p.contains("HARMONY"),
                "{lane:?} prompt missing HARMONY block"
            );
            assert!(
                p.contains("stay in this scale"),
                "{lane:?} prompt missing the stay-in-key default"
            );
            // Default state is C minor (root 0, scale Minor).  C2=36 must be
            // in the palette.
            assert!(
                p.contains("36"),
                "{lane:?} prompt should list the tonic MIDI note 36 in C2–C3"
            );
        }
    }

    #[test]
    fn secondary_bass_voice_asks_for_counterpoint() {
        // Bass #2/#3/#4 should be told to counterpoint voice #1, not clone it.
        for idx in 1..=3 {
            let desc = LaneKind::Bass(idx).task_description();
            assert!(
                desc.to_lowercase().contains("counterpoint"),
                "Bass({idx}) task desc missing counterpoint guidance"
            );
        }
        // Voice 1 itself gets the "establish main groove" framing, not counterpoint.
        let first = LaneKind::Bass(0).task_description();
        assert!(first.to_lowercase().contains("establish"));
    }

    #[test]
    fn an1x_task_points_to_bass_for_harmony() {
        let desc = LaneKind::An1x.task_description();
        assert!(
            desc.to_lowercase().contains("harmonise") || desc.to_lowercase().contains("bass"),
            "AN1X task desc should reference bass harmony"
        );
    }

    #[test]
    fn non_melodic_lanes_skip_harmony_block() {
        // Drums / FX / modulation / rack lanes don't need the note palette.
        let s = AppState::default();
        for lane in [LaneKind::KitA, LaneKind::KitB, LaneKind::Fx, LaneKind::Rack] {
            let p = build_lane_prompt(&s, lane);
            assert!(!p.contains("HARMONY"), "{lane:?} shouldn't have HARMONY");
        }
    }

    fn all_lanes_for_test() -> Vec<LaneKind> {
        vec![
            LaneKind::Settings,
            LaneKind::Bass(0),
            LaneKind::Bass(1),
            LaneKind::Bass(2),
            LaneKind::Bass(3),
            LaneKind::KitA,
            LaneKind::KitB,
            LaneKind::Amen,
            LaneKind::Hoover,
            LaneKind::An1x,
            LaneKind::Fx,
            LaneKind::Modulation,
            LaneKind::Rack,
        ]
    }
}
