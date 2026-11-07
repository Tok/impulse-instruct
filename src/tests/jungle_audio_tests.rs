//! Regression checks for audible Jungle bass, using the actual Bass303 DSP.
use crate::audio::dsp::{AudioParams, TuningSystem, bass303::Bass303};
use crate::llm::{jungle, lanes::LaneKind};
use crate::state::{AppState, apply_llm_update};
use serde_json::json;

fn patch() -> AppState {
    let mut state = AppState::default();
    state.llm.active_style = Some("jungle".into());
    let style = crate::llm::styles::StyleCatalog::get()
        .find_by_id("jungle")
        .unwrap();
    apply_llm_update(state, style.baseline_params.as_ref().unwrap(), &[])
}

fn render(state: &AppState, steps: &[usize]) -> Vec<f32> {
    let mut params = AudioParams::from_app_state(state);
    params.sample_rate = 48_000.0;
    let step_samples =
        (params.sample_rate * 60.0 / state.sequencer.bpm / state.sequencer.step_division as f32)
            .round() as usize;
    let mut voice = Bass303::default();
    (0..step_samples * 32)
        .map(|i| {
            let step = i / step_samples;
            let position = i % step_samples;
            if steps.contains(&step) {
                if position == 0 {
                    voice.trigger(45, 0.0, 0.0, TuningSystem::TwelveTet);
                }
                // Same default gate ratio used by the sequencer, not a held test tone.
                if position
                    == (step_samples as f32 * state.sequencer.bass_pattern[step].gate) as usize
                {
                    voice.gate_off();
                }
            }
            voice.process(&params, &params.bass_voice_params[0])
        })
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
}

#[test]
fn jungle_bass_phrases_cover_the_playing_span() {
    for span in [4, 8, 16, 24, 32, 48, 64] {
        for steps in jungle::bass_rhythm_options(span) {
            assert!(steps.iter().all(|&step| step < span));
            assert!(steps.windows(2).all(|p| p[0] < p[1]));
            for quarter in 0..4 {
                assert!(steps.iter().any(|&step| (quarter*span/4..(quarter+1)*span/4).contains(&step)), "{span}: {steps:?}");
            }
        }
    }
    assert!(
        jungle::validate_rhythms(&json!({"sequencer":{
            "bass_steps":[0,1,2,3,4,5], "bass_notes":vec![45;32]
        }}))
        .is_err()
    );
    assert!(
        jungle::validate_rhythms(&json!({"sequencer":{
            "bass_steps":[0,10,16,26], "bass_notes":vec![45;32]
        }}))
        .is_ok()
    );
}

#[test]
fn jungle_bass_is_not_ducked_when_a_lane_applies() {
    let state = patch();
    let target = state.bass_voices[0].synth.volume;
    let next = crate::state::jam_tools::schedule_lane_fade_in(state, LaneKind::Bass(0));
    assert_eq!(next.bass_voices[0].synth.volume, target);
    assert!(next.llm.active_ramps.is_empty());
}

#[test]
fn jungle_envelope_applies_immediately_and_respects_locks() {
    let style = crate::llm::styles::StyleCatalog::get()
        .find_by_id("jungle")
        .unwrap();
    let (state, remaining) = crate::state::jam_tools::schedule_baseline_ramps(
        AppState::default(),
        style.baseline_params.as_ref().unwrap(),
        8.0,
    );
    let state = apply_llm_update(state, &remaining, &[]);
    assert_eq!(state.bass_voices[0].synth.amp_release, 0.32);
    assert!(
        !state
            .llm
            .active_ramps
            .iter()
            .any(|r| r.param == "bass.amp_release")
    );
    let mut locked = AppState::default();
    locked.llm.locked_params.insert("bass.amp_release".into());
    let locked = apply_llm_update(locked, &remaining, &[]);
    assert_eq!(locked.bass_voices[0].synth.amp_release, 0.05);
}

#[test]
fn jungle_bass_has_energy_in_every_quarter_and_an_audible_release() {
    let state = patch();
    let steps = &jungle::bass_rhythm_options(32)[0];
    let audio = render(&state, steps);
    let mut old = state.clone();
    old.bass_voices[0].synth.amp_attack = 0.0;
    old.bass_voices[0].synth.amp_release = 0.05;
    old.bass_voices[0].synth.volume = 0.7;
    let old_envelope = render(&old, steps);
    let old_cluster = render(&old, &[0, 1, 2, 3, 4, 5]);
    let new_rms = rms(&audio);
    let old_rms = rms(&old_envelope);
    let peak = audio.iter().copied().map(f32::abs).fold(0.0, f32::max);
    println!(
        "bass voice: old RMS={old_rms:.4}, new RMS={new_rms:.4}, gain={:.1} dB, peak={peak:.4}",
        20.0 * (new_rms / old_rms).log10()
    );
    assert!(audio.iter().all(|s| s.is_finite()));
    assert!(peak < 1.0, "isolated bass must retain headroom: {peak}");
    assert!(
        new_rms > old_rms * 1.8,
        "release must materially improve phrase energy"
    );
    for (quarter, samples) in audio.chunks(audio.len() / 4).enumerate() {
        println!("quarter {quarter}: RMS={:.4}", rms(samples));
        assert!(rms(samples) > 0.04, "quarter {quarter} is too quiet");
    }
    // 250–400 ms after the first trigger: well past the ~45 ms gate.
    let tail = rms(&audio[12_000..19_200]);
    let old_tail = rms(&old_envelope[12_000..19_200]);
    assert!(tail > old_tail * 5.0, "release tail should remain audible");
    if let Ok(dir) = std::env::var("JUNGLE_RENDER_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, samples) in [("before.wav", old_cluster), ("after.wav", audio)] {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer =
                hound::WavWriter::create(std::path::Path::new(&dir).join(name), spec).unwrap();
            for sample in samples {
                writer
                    .write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                    .unwrap();
            }
            writer.finalize().unwrap();
        }
    }
}

#[test]
fn jungle_break_edits_keep_their_level() {
    let state = patch();
    let level = state.amen.volume;
    let next = crate::state::jam_tools::schedule_lane_fade_in(state, LaneKind::Amen);
    assert_eq!(next.amen.volume, level);
    assert!(next.llm.active_ramps.is_empty());
}

#[test]
fn jungle_intro_clears_restored_bass_and_orders_break_first() {
    let state = apply_llm_update(
        patch(),
        &json!({"sequencer":{"bass_steps":[0,8],"amen_steps":[0,2]}}),
        &[],
    );
    assert!(state.sequencer.bass_pattern.iter().any(|s| s.active));
    let state = jungle::prepare_intro(state);
    assert!(state.sequencer.bass_pattern.iter().all(|s| !s.active));
    assert!(
        state
            .sequencer
            .bass_patterns
            .iter()
            .flatten()
            .all(|s| !s.active)
    );
    let mut lanes = vec![
        LaneKind::Bass(0),
        LaneKind::KitA,
        LaneKind::Amen,
        LaneKind::Settings,
    ];
    jungle::order_intro(&mut lanes);
    assert_eq!(
        lanes,
        vec![
            LaneKind::Settings,
            LaneKind::Amen,
            LaneKind::Bass(0),
            LaneKind::KitA
        ]
    );
    let mut bass_only = vec![LaneKind::Bass(0)];
    jungle::order_intro(&mut bass_only);
    assert_eq!(bass_only, vec![LaneKind::Bass(0)]);
}

#[test]
fn jungle_first_jam_prefers_break_over_a_bass_retry() {
    let mut state = patch();
    state.amen.path = "test.wav".into();
    if !state
        .rack
        .modules
        .iter()
        .any(|m| m.kind == crate::state::ModuleKind::AmenSampler)
    {
        state.rack.add_module(crate::state::ModuleKind::AmenSampler);
    }
    state.rack.wire_default_cables();
    state.llm.retry_queue.push_back("bass1".into());
    let mut rng = crate::llm::lane_scheduler::Xorshift32::new(123);
    assert_eq!(
        crate::llm::planner_jam::jam_plan(&state, &mut rng).lanes,
        vec![LaneKind::Amen]
    );
}
