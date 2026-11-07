//! State-aware generation constraints and musical context for lane writers.
use super::lanes::{LaneKind, lane_schema};
use crate::state::{AppState, DrumVoice, MAX_STEPS};
use serde_json::{Value, json};

/// Resolve the playing span, including polymetric voice lengths.
pub fn pattern_span(state: &AppState, key: &str) -> usize {
    let seq = &state.sequencer;
    let prefix = key.trim_end_matches("_notes").trim_end_matches("_steps");
    let drums = [
        ("kick_a", DrumVoice::Kick808),
        ("snare_a", DrumVoice::Snare808),
        ("hihat_a", DrumVoice::HihatClosed808),
        ("hihat_a_open", DrumVoice::HihatOpen808),
        ("kick_b", DrumVoice::Kick909),
        ("snare_b", DrumVoice::Snare909),
        ("clap_b", DrumVoice::Clap909),
        ("hihat_b", DrumVoice::HihatClosed909),
        ("hihat_b_open", DrumVoice::HihatOpen909),
        ("amen", DrumVoice::Amen),
    ];
    let span = match prefix {
        "bass" => seq.bass_steps,
        "bass2" | "bass3" | "bass4" => seq
            .bass_voice_steps
            .get(prefix.as_bytes()[4] as usize - b'1' as usize)
            .copied()
            .unwrap_or(seq.bass_steps),
        "hoover" => seq.hoover_steps,
        "an1x" => seq.an1x_steps,
        _ => drums
            .iter()
            .find(|(name, _)| *name == prefix)
            .and_then(|(_, voice)| seq.drum_steps.get(voice))
            .copied()
            .unwrap_or(seq.steps),
    };
    span.clamp(1, MAX_STEPS)
}

/// Notes are step-indexed, not a list of pitches for active gates only.
/// Requiring the full span prevents old/default pitches leaking into a new phrase.
pub fn composition_schema(state: &AppState, lane: LaneKind) -> Value {
    let mut schema = lane_schema(lane);
    if lane == LaneKind::Settings {
        schema["properties"]["sequencer"]["properties"]["scale_snap"] = json!({"type":"boolean"});
    }
    if lane == LaneKind::Amen {
        schema["properties"]["amen"] = json!({
            "type": "object", "additionalProperties": false,
            "properties": {
                "slice_count": {"type": "integer", "enum": [state.amen.slice_count.clamp(1,16)]},
                "pitch": {"type": "number", "minimum": -24, "maximum": 24},
                "volume": {"type": "number", "minimum": 0, "maximum": 1},
                "gate": {"type": "number", "minimum": 0.05, "maximum": 1},
                "stutter": {"type": "integer", "minimum": 0, "maximum": 4},
                "loop_mode": {"type": "boolean"},
                "reverse": {"type": "boolean"},
                "bpm_stretch": {"type": "boolean"},
            }
        });
        schema["properties"]["sequencer"]["properties"]["amen_slices"] = json!({
            "type": "array", "items": {"type": "integer", "minimum": 0, "maximum": state.amen.slice_count.clamp(1,16)},
            "minItems": pattern_span(state, "amen_steps"),
            "maxItems": pattern_span(state, "amen_steps")
        });
        schema["properties"]["sequencer"]["additionalProperties"] = json!(false);
    }
    if let Some(props) = schema["properties"]["sequencer"]["properties"].as_object_mut() {
        for (key, field) in props {
            if field["type"] != "array" {
                continue;
            }
            let span = pattern_span(state, key);
            if key.ends_with("_notes") {
                field["minItems"] = json!(span);
                field["maxItems"] = json!(span);
                if state.sequencer.scale_snap {
                    let (root, scale) = lane_key(state, lane);
                    let notes = if state.llm.active_style.as_deref() == Some("jungle") {
                        super::jungle::pitch_palette(state, lane)
                    } else {
                        crate::state::scale_notes(root, scale)
                    };
                    field["items"]["enum"] = json!(notes);
                }
            } else if key.ends_with("_steps") {
                field["items"] = json!({"type": "integer", "minimum": 0});
                field["items"]["maximum"] = json!(span - 1);
                // Silence and one-hit drones are valid musical choices.
                field["minItems"] = json!(0);
                field["maxItems"] = json!(span);
                field["uniqueItems"] = json!(true);
                if state.llm.active_style.as_deref() == Some("jungle") {
                    let per_32 = match lane {
                        LaneKind::Bass(_) => 6,
                        LaneKind::KitA | LaneKind::KitB if key.contains("hihat") => 8,
                        LaneKind::KitA | LaneKind::KitB => 6,
                        LaneKind::Hoover | LaneKind::An1x => 4,
                        _ => 32,
                    };
                    field["maxItems"] = json!((span * per_32).div_ceil(32));
                }
            }
        }
    }
    if state.llm.active_style.as_deref() == Some("jungle") && lane == LaneKind::Bass(0) {
        let mut rhythms = super::jungle::bass_rhythm_options(pattern_span(state, "bass_steps"));
        rhythms.push(Vec::new()); // Explicit mute requests remain representable.
        schema["properties"]["sequencer"]["properties"]["bass_steps"] = json!({
            "type": "array", "items": {"type":"integer"}, "enum": rhythms,
            "description": "Complete bass phrases covering the loop; [] only for intentional silence."
        });
    }
    // These lanes previously allowed an empty object to count as success.
    // Require their pattern payload, while synth tweaks remain optional.
    let required = match lane {
        LaneKind::Hoover => vec!["hoover_steps", "hoover_notes"],
        LaneKind::An1x => vec!["an1x_steps", "an1x_notes"],
        LaneKind::Amen => vec!["amen_steps", "amen_slices"],
        LaneKind::KitB => vec![
            "kick_b_steps",
            "snare_b_steps",
            "clap_b_steps",
            "hihat_b_steps",
        ],
        _ => Vec::new(),
    };
    if !required.is_empty() {
        schema["required"] = json!(["sequencer"]);
        schema["properties"]["sequencer"]["required"] = json!(required);
    }
    schema
}

/// Concrete note/rhythm context plus phrasing guidance. No forced quantisation:
/// explicit chromatic styles and user requests retain their musical freedom.
pub fn composition_context(state: &AppState, lane: LaneKind) -> String {
    let mut out = String::from(
        "\nCOMPOSITION: The style brief and explicit user request override generic density, pitch-variety and rhythm examples. Silence is valid when requested or called for by the style; [] clears a rhythm. Otherwise develop the phrase across the whole playing loop, not only its first half. Repetition with a small ending variation is preferable to unrelated new material.\n",
    );
    out.push_str(&format!(
        "GRID: {} steps per beat, {} beats per bar. Scale example rhythms to this grid.\n",
        state.sequencer.step_division, state.sequencer.time_sig_num
    ));
    if lane == LaneKind::Amen {
        let span = pattern_span(state, "amen_steps");
        if let Some(meta) = &state.amen.meta {
            let beats = meta.samples as f32 / crate::audio::SAMPLE_RATE * state.amen.source_bpm
                / 60.0
                * (state.amen.end_offset - state.amen.start_offset).clamp(0.0, 1.0);
            out.push_str(&format!("SOURCE REGION: approximately {beats:.2} source beats; preserve this phrase before adding a turnaround.\n"));
        }
        let slices: Vec<_> = state
            .sequencer
            .drum_patterns
            .get(&DrumVoice::Amen)
            .into_iter()
            .flatten()
            .take(span)
            .map(|step| step.slice)
            .collect();
        out.push_str(&format!(
            "SAMPLER: path_configured={} slice_count={} source_bpm={} gate={} bpm_stretch={}; manual_markers={}; current step-indexed slices={slices:?}. If no sample is configured, these events need a loaded break to produce sampled drums.\n",
            !state.amen.path.is_empty(), state.amen.slice_count, state.amen.source_bpm,
            state.amen.gate, state.amen.bpm_stretch, state.amen.slice_positions.len()
        ));
    }
    for key in lane.sequencer_subkeys() {
        if key.ends_with("_steps") || key.ends_with("_notes") {
            let span = pattern_span(state, &key);
            out.push_str(&format!("{key}: loop length {span}; "));
            if key.ends_with("_notes") {
                out.push_str(&format!("exactly {span} MIDI pitches, one per step including rests; notes[i] belongs to step i, NOT the ith active hit.\n"));
            } else {
                out.push_str(&format!("strictly increasing UNIQUE active indices 0..{}; never an inline 0/1 gate array.\n", span - 1));
            }
        }
    }
    if matches!(lane, LaneKind::Bass(_) | LaneKind::Hoover | LaneKind::An1x) {
        out.push_str("PHRASING: Build a short recognisable motif; repeat its rhythm and contour. Anchor strong beats on tonic/chord tones, use mostly stepwise passing motion and resolve tension by the phrase end. One or two pitches are enough for drones/minimal styles. Avoid arbitrary leaps and parallel semitone clashes. Put supporting voices in a separate register with fewer attacks. Chromatic/atonal writing is allowed when explicitly requested by the user or style.\nEXISTING VOICES (step:MIDI; each repeats at its own loop length):\n");
        for (idx, voice) in state.bass_voices.iter().enumerate() {
            if !voice.enabled {
                continue;
            }
            let pattern = if idx == 0 {
                Some(&state.sequencer.bass_pattern)
            } else {
                state.sequencer.bass_patterns.get(idx)
            };
            if let Some(pattern) = pattern {
                let key = if idx == 0 {
                    "bass_steps".to_string()
                } else {
                    format!("bass{}_steps", idx + 1)
                };
                let span = pattern_span(state, &key);
                let notes: Vec<_> = pattern
                    .iter()
                    .take(span)
                    .enumerate()
                    .filter(|(_, s)| s.active)
                    .map(|(i, s)| format!("{i}:{}", s.note))
                    .collect();
                out.push_str(&format!(
                    "bass{} (length {span}): {}\n",
                    idx + 1,
                    notes.join(", ")
                ));
            }
        }
        for (name, enabled, pattern, span) in [
            (
                "hoover",
                state.hoover.enabled,
                &state.sequencer.hoover_pattern,
                state.sequencer.hoover_steps,
            ),
            (
                "an1x",
                state.an1x.enabled,
                &state.sequencer.an1x_pattern,
                state.sequencer.an1x_steps,
            ),
        ] {
            if enabled {
                let notes: Vec<_> = pattern
                    .iter()
                    .take(span)
                    .enumerate()
                    .filter(|(_, s)| s.active)
                    .map(|(i, s)| format!("{i}:{}", s.note))
                    .collect();
                out.push_str(&format!("{name} (length {span}): {}\n", notes.join(", ")));
            }
        }
    }
    out
}

/// Bass voices may opt out of the global key.
pub fn lane_key(state: &AppState, lane: LaneKind) -> (u8, crate::state::Scale) {
    if let LaneKind::Bass(idx) = lane
        && let Some(voice) = state.bass_voices.get(idx)
        && !voice.lock_key
    {
        return (voice.root_note, voice.scale);
    }
    (state.sequencer.root_note, state.sequencer.scale)
}
