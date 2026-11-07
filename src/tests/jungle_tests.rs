use crate::llm::{
    jungle,
    lane_composition::{composition_schema, lane_key},
    lanes::{LaneKind, build_lane_prompt},
};
use crate::state::{AppState, Scale, apply_llm_update};
use serde_json::json;

fn jungle_state() -> AppState {
    let mut s = AppState::default();
    s.llm.active_style = Some("jungle".into());
    let style = crate::llm::styles::StyleCatalog::get()
        .find_by_id("jungle")
        .unwrap();
    apply_llm_update(s, style.baseline_params.as_ref().unwrap(), &[])
}

#[test]
fn jungle_replaces_acid_and_house_tasks() {
    let s = jungle_state();
    let bass = build_lane_prompt(&s, LaneKind::Bass(0));
    assert!(bass.contains("JUNGLE SUB"));
    assert!(!bass.contains("8-14 active steps"));
    let kit = build_lane_prompt(&s, LaneKind::KitA);
    assert!(kit.contains("JUNGLE LAYER"));
    assert!(!kit.contains("808 is LOOSE"));
    let mut generic = s;
    generic.llm.active_style = None;
    assert!(jungle::task(&generic, LaneKind::Bass(0)).is_none());
}

#[test]
fn jungle_schema_restricts_harmony_but_snap_can_be_disabled() {
    let mut s = jungle_state();
    for lane in [LaneKind::Bass(0), LaneKind::Bass(1), LaneKind::An1x] {
        let palette = jungle::pitch_palette(&s, lane);
        let (root, scale) = lane_key(&s, lane);
        assert!(!palette.is_empty());
        assert!(palette.iter().all(
            |&n| matches!((n as i16 - root as i16).rem_euclid(12), 0 | 7)
                && crate::state::note_in_scale(n, root, scale)
        ));
    }
    let schema = composition_schema(&s, LaneKind::Bass(0));
    let items = &schema["properties"]["sequencer"]["properties"]["bass_notes"]["items"];
    assert_eq!(
        items["enum"],
        json!(jungle::pitch_palette(&s, LaneKind::Bass(0)))
    );
    assert!(!items["enum"].as_array().unwrap().contains(&json!(0)));
    s.sequencer.scale_snap = false;
    assert!(composition_schema(&s, LaneKind::Bass(0))["properties"]["sequencer"]["properties"]["bass_notes"]["items"].get("enum").is_none());
}

#[test]
fn scale_snap_applies_to_leads_and_respects_voice_keys_locks_and_opt_out() {
    let mut s = AppState::default();
    s.sequencer.root_note = 0;
    s.sequencer.scale = Scale::Major;
    s.bass_voices[1].lock_key = false;
    s.bass_voices[1].root_note = 1;
    s.bass_voices[1].scale = Scale::Major;
    let update = json!({"sequencer":{"scale_snap":true,"bass2_notes":[61],"hoover_notes":[61],"an1x_notes":[61]}});
    let snapped = apply_llm_update(s.clone(), &update, &[]);
    assert!(snapped.sequencer.scale_snap);
    assert_eq!(snapped.sequencer.bass_patterns[1][0].note, 61);
    assert_eq!(snapped.sequencer.hoover_pattern[0].note, 60);
    assert_eq!(snapped.sequencer.an1x_pattern[0].note, 60);
    s.llm.locked_params.insert("sequencer.scale_snap".into());
    let locked = apply_llm_update(s, &update, &[]);
    assert!(!locked.sequencer.scale_snap);
    assert_eq!(locked.sequencer.an1x_pattern[0].note, 61);
}

#[test]
fn amen_schema_requires_typed_per_step_slices_and_sampler_settings() {
    let schema = composition_schema(&jungle_state(), LaneKind::Amen);
    assert_eq!(schema["properties"]["amen"]["type"], "object");
    assert_eq!(
        schema["properties"]["amen"]["properties"]["slice_count"]["enum"],
        json!([16])
    );
    assert!(
        schema["properties"]["amen"]["properties"]
            .get("source_bpm")
            .is_none()
    );
    let seq = &schema["properties"]["sequencer"];
    assert_eq!(seq["properties"]["amen_slices"]["items"]["type"], "integer");
    assert_eq!(seq["properties"]["amen_slices"]["minItems"], 32);
    assert_eq!(seq["additionalProperties"], false);
    assert!(
        seq["required"]
            .as_array()
            .unwrap()
            .contains(&json!("amen_slices"))
    );
}

#[test]
fn jungle_jams_do_not_rewrite_settings_even_from_retry_queue() {
    let mut s = jungle_state();
    assert!(!crate::llm::planner_jam::live_lane_candidates(&s).contains(&LaneKind::Settings));
    s.llm.retry_queue.push_back("settings".into());
    let mut rng = crate::llm::lane_scheduler::Xorshift32::new(42);
    assert!(
        !crate::llm::planner_jam::jam_plan(&s, &mut rng)
            .lanes
            .contains(&LaneKind::Settings)
    );
}

#[test]
fn sampler_baseline_is_applied_without_unsupported_ramps() {
    let s = jungle_state();
    assert!(s.sequencer.scale_snap);
    assert_eq!(s.amen.slice_count, 16);
    assert!(s.amen.bpm_stretch);
    let baseline = json!({"amen":{"slice_count":8,"gate":0.8,"volume":0.6}});
    let (s, remainder) = crate::state::jam_tools::schedule_baseline_ramps(s, &baseline, 8.0);
    assert_eq!(remainder, baseline);
    let applied = apply_llm_update(s, &remainder, &[]);
    assert_eq!(applied.amen.slice_count, 8);
    assert_eq!(applied.amen.gate, 0.8);
}

#[test]
#[ignore = "requires a live llama-server at LLAMA_SERVER_URL; no mock fallback"]
fn jungle_live_lane_smoke() {
    let url = std::env::var("LLAMA_SERVER_URL").expect("set LLAMA_SERVER_URL");
    let mut backend = crate::llm::LlamaServerBackend::connect(&url);
    assert!(backend.is_live(), "live server required");
    let mut s = jungle_state();
    if let Ok(path) = std::env::var("JUNGLE_TEST_SAMPLE") {
        s.amen.meta = Some(crate::audio::read_wav_meta(&path).expect("read test break"));
        s.amen.source_bpm =
            crate::ui::util::sample_bpm_hint(&path).expect("BPM-labelled test break");
        s.amen.path = path;
    }
    let sampling = crate::llm::SamplingParams {
        seed: 42,
        temperature: 0.6,
        ..Default::default()
    };
    for lane in [LaneKind::Amen, LaneKind::KitA, LaneKind::Bass(0)] {
        let result = crate::llm::pipeline::infer_lane_with_retry(
            &mut backend,
            &build_lane_prompt(&s, lane),
            "Create an authentic Jungle groove in the current key. /no_think",
            &composition_schema(&s, lane),
            &sampling,
            lane,
            true,
        )
        .unwrap();
        println!("{}: {}", lane.label(), result);
        let seq = &result["sequencer"];
        match lane {
            LaneKind::Amen => assert_eq!(seq["amen_slices"].as_array().unwrap().len(), 32),
            LaneKind::Bass(0) => {
                let notes = seq["bass_notes"].as_array().unwrap();
                assert_eq!(notes.len(), 32);
                let palette = jungle::pitch_palette(&s, lane);
                assert!(
                    notes
                        .iter()
                        .all(|n| palette.contains(&(n.as_u64().unwrap() as u8)))
                );
                let hits = seq["bass_steps"].as_array().unwrap().len();
                assert!(
                    (4..=6).contains(&hits),
                    "sub should span the loop, got {hits}"
                );
                jungle::validate_rhythms(&result).unwrap();
            }
            _ => (),
        }
        s = apply_llm_update(
            s,
            &crate::llm::pipeline::filter_lane_output(lane, result),
            &crate::llm::pipeline::lane_apply_scope(lane),
        );
    }
}

#[test]
fn sample_bpm_uses_explicit_filename_labels_only() {
    use crate::ui::util::sample_bpm_hint;
    assert_eq!(
        sample_bpm_hint("samples/amen/cw_amen11_145.wav"),
        Some(145.0)
    );
    assert_eq!(sample_bpm_hint("Creek Break 160.wav"), Some(160.0));
    assert_eq!(sample_bpm_hint("break_172.5bpm.wav"), Some(172.5));
    for name in ["amen.wav", "take_01.wav", "break_999.wav"] {
        assert_eq!(sample_bpm_hint(name), None);
    }
}

#[test]
fn jungle_rejects_masks_and_duplicates_in_rhythm_index_lists() {
    for steps in [
        json!([0, 0, 1, 0]),
        json!([0, 4, 4]),
        json!([8, 4]),
        json!([true, false]),
    ] {
        assert!(jungle::validate_rhythms(&json!({"sequencer":{"amen_steps":steps}})).is_err());
    }
    assert!(
        jungle::validate_rhythms(
            &json!({"sequencer":{"amen_steps":[0,2,4,16,30],"bass_steps":[]}})
        )
        .is_ok()
    );
}

#[test]
fn jungle_rhythm_retry_is_corrective_and_bounded() {
    struct Backend {
        calls: usize,
        always_invalid: bool,
    }
    impl crate::llm::pipeline::PipelineBackend for Backend {
        fn infer_lane_json(
            &mut self,
            _: &str,
            user: &str,
            _: &serde_json::Value,
            _: &crate::llm::SamplingParams,
        ) -> anyhow::Result<serde_json::Value> {
            self.calls += 1;
            if self.calls > 1 {
                assert!(user.contains("Correction required:"));
            }
            Ok(if self.calls == 1 || self.always_invalid {
                json!({"sequencer":{"amen_steps":[0,0,1,0]}})
            } else {
                json!({"sequencer":{"amen_steps":[0,2,4,16]}})
            })
        }
    }
    for always_invalid in [false, true] {
        let mut backend = Backend {
            calls: 0,
            always_invalid,
        };
        let result = crate::llm::pipeline::infer_lane_with_retry(
            &mut backend,
            "system",
            "jungle",
            &json!({}),
            &Default::default(),
            LaneKind::Amen,
            true,
        );
        assert_eq!(result.is_err(), always_invalid);
        assert_eq!(backend.calls, 2);
    }
}
