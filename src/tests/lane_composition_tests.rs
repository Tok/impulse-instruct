use crate::llm::lane_composition::{
    composition_context, composition_schema, lane_key, pattern_span,
};
use crate::llm::lanes::{LaneKind, build_lane_prompt};
use crate::llm::pipeline_filter::{filter_lane_output, lane_apply_scope};
use crate::state::{AppState, DrumVoice, Scale, apply_llm_update};
use serde_json::json;

#[test]
fn composition_schema_covers_full_polymetric_note_arrays() {
    let mut s = AppState::default();
    s.sequencer.bass_voice_steps[1] = 64;
    s.sequencer.hoover_steps = 16;
    s.sequencer.an1x_steps = 8;
    s.sequencer.drum_steps.insert(DrumVoice::Kick808, 12);
    for (lane, prefix, span) in [
        (LaneKind::Bass(0), "bass", 32),
        (LaneKind::Bass(1), "bass2", 64),
        (LaneKind::Hoover, "hoover", 16),
        (LaneKind::An1x, "an1x", 8),
    ] {
        let schema = composition_schema(&s, lane);
        let props = &schema["properties"]["sequencer"]["properties"];
        assert_eq!(props[format!("{prefix}_notes")]["minItems"], span);
        assert_eq!(props[format!("{prefix}_notes")]["maxItems"], span);
        assert_eq!(
            props[format!("{prefix}_steps")]["items"]["maximum"],
            span - 1
        );
        assert_eq!(props[format!("{prefix}_steps")]["minItems"], 0);
    }
    for lane in [
        LaneKind::Hoover,
        LaneKind::An1x,
        LaneKind::Amen,
        LaneKind::KitB,
    ] {
        let schema = composition_schema(&s, lane);
        assert_eq!(schema["required"], json!(["sequencer"]));
        assert!(
            !schema["properties"]["sequencer"]["required"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(pattern_span(&s, "kick_a_steps"), 12);
    assert_eq!(
        composition_schema(&s, LaneKind::KitA)["properties"]["sequencer"]["properties"]["kick_a_steps"]
            ["items"]["maximum"],
        11
    );
    s.sequencer.hoover_steps = 0;
    assert_eq!(pattern_span(&s, "hoover_steps"), 1);
}

#[test]
fn composition_shares_pitches_and_respects_voice_key() {
    let mut s = AppState::default();
    s.bass_voices[0].enabled = true;
    s.sequencer.bass_pattern[0].active = true;
    s.sequencer.bass_pattern[0].note = 45;
    let context = composition_context(&s, LaneKind::An1x);
    assert!(context.contains("0:45"));
    assert!(context.contains("one per step including rests"));
    assert!(context.contains("recognisable motif"));
    s.bass_voices[1].lock_key = false;
    s.bass_voices[1].root_note = 2;
    s.bass_voices[1].scale = Scale::Major;
    assert_eq!(lane_key(&s, LaneKind::Bass(1)), (2, Scale::Major));
    assert!(build_lane_prompt(&s, LaneKind::Bass(1)).contains("HARMONY: key D Major"));
    assert_eq!(
        lane_key(&s, LaneKind::An1x),
        (s.sequencer.root_note, s.sequencer.scale)
    );
}

#[test]
fn lead_patterns_apply_from_sequencer_and_obey_locks_and_scope() {
    for (lane, prefix) in [(LaneKind::Hoover, "hoover"), (LaneKind::An1x, "an1x")] {
        let mut s = AppState::default();
        let steps_key = format!("{prefix}_steps");
        let notes_key = format!("{prefix}_notes");
        let raw =
            json!({"sequencer": {steps_key.clone(): [0, 20, 31], notes_key.clone(): vec![67; 32]}});
        let update = filter_lane_output(lane, raw);
        let applied = apply_llm_update(s.clone(), &update, &lane_apply_scope(lane));
        let pattern = if prefix == "hoover" {
            &applied.sequencer.hoover_pattern
        } else {
            &applied.sequencer.an1x_pattern
        };
        assert!(pattern[31].active);
        assert!(!pattern[30].active);
        assert_eq!(pattern[31].note, 67);
        let blocked = apply_llm_update(s.clone(), &update, &["bass".into()]);
        assert_eq!(
            serde_json::to_value(&blocked.sequencer).unwrap(),
            serde_json::to_value(&s.sequencer).unwrap()
        );
        s.llm.locked_params.insert(format!("sequencer.{steps_key}"));
        s.llm.locked_params.insert(format!("sequencer.{notes_key}"));
        let blocked = apply_llm_update(s.clone(), &update, &lane_apply_scope(lane));
        assert_eq!(
            serde_json::to_value(&blocked.sequencer).unwrap(),
            serde_json::to_value(&s.sequencer).unwrap()
        );
        let silence = filter_lane_output(lane, json!({"sequencer": {steps_key: []}}));
        let cleared = apply_llm_update(applied, &silence, &lane_apply_scope(lane));
        let pattern = if prefix == "hoover" {
            cleared.sequencer.hoover_pattern
        } else {
            cleared.sequencer.an1x_pattern
        };
        assert!(pattern.iter().all(|s| !s.active));
    }
}

#[test]
fn dense_index_lists_and_short_boolean_patterns_are_not_misread() {
    let mut gates = [false; 64];
    let indices: Vec<_> = (0..64).step_by(2).map(|i| json!(i)).collect();
    crate::state::apply_llm_step_array(&indices, &mut gates, 64, |s, a| *s = a);
    for (i, gate) in gates.iter().enumerate() {
        assert_eq!(*gate, i % 2 == 0);
    }
    crate::state::apply_llm_step_array(&[json!(false), json!(true)], &mut gates, 64, |s, a| *s = a);
    assert!(!gates[0]);
    assert!(gates[1]);
}

#[test]
fn harmonic_scoring_prefers_tonic_over_out_of_key_novelty() {
    let mut s = AppState::default();
    s.sequencer.root_note = 0;
    s.sequencer.scale = Scale::Major;
    for step in &mut s.sequencer.bass_pattern {
        step.active = false;
    }
    for step in s.sequencer.bass_pattern.iter_mut().take(6) {
        step.active = true;
        step.note = 36;
    }
    let tonic = crate::llm::lane_eval::evaluate_lane(&s, LaneKind::Bass(0));
    for (i, step) in s.sequencer.bass_pattern.iter_mut().take(6).enumerate() {
        step.note = [37, 39, 42, 44, 46, 49][i];
    }
    assert!(crate::llm::lane_eval::evaluate_lane(&s, LaneKind::Bass(0)) < tonic);
    s.sequencer.scale = Scale::Chromatic;
    assert_eq!(
        crate::llm::lane_eval::evaluate_lane(&s, LaneKind::Bass(0)),
        tonic
    );
}

#[test]
fn every_style_has_composition_guidance_and_corrected_seeds() {
    let styles: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../styles.json")).unwrap();
    for style in &styles {
        assert!(
            style["brief"].as_str().unwrap().contains("Composition:"),
            "{}",
            style["id"]
        );
        assert!(
            style["description"]
                .as_str()
                .unwrap()
                .contains("Composition:")
        );
        let mut s = AppState::default();
        s.llm.active_style = Some(style["id"].as_str().unwrap().into());
        assert!(build_lane_prompt(&s, LaneKind::Bass(0)).contains("Composition:"));
    }
    let dark = styles.iter().find(|s| s["id"] == "dark_ambient").unwrap();
    assert!(
        dark["seed_patterns"]["kick"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| *v == 0)
    );
    let dnb = styles.iter().find(|s| s["id"] == "drum_and_bass").unwrap();
    let snare: Vec<_> = dnb["seed_patterns"]["snare"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, v)| **v == 1)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(snare, [4, 12, 20, 28]);
}
