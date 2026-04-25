// ─── tests/bass_lfo_pan_tests.rs ─────────────────────────────────────────────
// Bass-voice LFO Pan target + per-voice phase offset.  The Bach demo
// previously simulated this with a 10 Hz Python loop hammering
// /api/params; these tests lock the native DSP path so the scenario
// can configure two voices once and let the audio thread animate the
// stereo motion.

use crate::state::{AppState, BassLfoTarget, apply_llm_update};

// ─── Enum + state defaults ───────────────────────────────────────────────────

#[test]
fn bass_lfo_target_pan_label_and_cycle() {
    // Pan is part of the LFO-target rotation now: cycling forward
    // from Amplitude must land on Pan, and Pan's `next()` must
    // wrap back to Off so the cycle button is consistent.
    assert_eq!(BassLfoTarget::Amplitude.next(), BassLfoTarget::Pan);
    assert_eq!(BassLfoTarget::Pan.next(), BassLfoTarget::Off);
    assert_eq!(BassLfoTarget::Pan.label(), "PAN");
}

#[test]
fn bass_state_lfo_phase_default_is_zero() {
    let s = AppState::default();
    assert_eq!(s.bass_voices[0].synth.lfo_phase, 0.0);
    assert_eq!(s.bass_voices[1].synth.lfo_phase, 0.0);
}

// ─── LLM apply ───────────────────────────────────────────────────────────────

#[test]
fn apply_llm_update_accepts_pan_target_and_phase_offset() {
    let s0 = AppState::default();
    let update = serde_json::json!({
        "bass": {
            "lfo_target": "pan",
            "lfo_rate":   0.4,
            "lfo_depth":  0.7,
            "lfo_phase":  0.5,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert_eq!(s1.bass_voices[0].synth.lfo_target, BassLfoTarget::Pan);
    assert!((s1.bass_voices[0].synth.lfo_rate - 0.4).abs() < 1e-6);
    assert!((s1.bass_voices[0].synth.lfo_depth - 0.7).abs() < 1e-6);
    assert!((s1.bass_voices[0].synth.lfo_phase - 0.5).abs() < 1e-6);
}

#[test]
fn apply_llm_update_can_set_voice1_to_anti_phase() {
    // The Bach scenario's anti-phase pattern: voice 0 at phase 0,
    // voice 1 at phase 0.5.  Updates use the per-voice
    // bass_voices[N] form for voices 1..=3.
    let s0 = AppState::default();
    let update = serde_json::json!({
        "bass": { "lfo_target": "pan", "lfo_phase": 0.0, "lfo_depth": 0.6 },
        "bass_voices": [
            null,
            { "lfo_target": "pan", "lfo_phase": 0.5, "lfo_depth": 0.6, "enabled": true },
            null,
            null
        ]
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert_eq!(s1.bass_voices[0].synth.lfo_target, BassLfoTarget::Pan);
    assert!((s1.bass_voices[0].synth.lfo_phase - 0.0).abs() < 1e-6);
    assert_eq!(s1.bass_voices[1].synth.lfo_target, BassLfoTarget::Pan);
    assert!((s1.bass_voices[1].synth.lfo_phase - 0.5).abs() < 1e-6);
    assert!(s1.bass_voices[1].enabled);
}

#[test]
fn apply_llm_update_respects_lock_on_lfo_phase() {
    let mut s0 = AppState::default();
    s0.bass_voices[0].synth.lfo_phase = 0.25;
    s0.llm.locked_params.insert("bass.lfo_phase".to_string());
    let update = serde_json::json!({
        "bass": { "lfo_phase": 0.9, "lfo_depth": 0.4 }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(
        (s1.bass_voices[0].synth.lfo_phase - 0.25).abs() < 1e-6,
        "locked phase should stay 0.25, got {}",
        s1.bass_voices[0].synth.lfo_phase,
    );
    assert!((s1.bass_voices[0].synth.lfo_depth - 0.4).abs() < 1e-6);
}

// ─── DSP — anti-phase scenario ───────────────────────────────────────────────
//
// Build an AudioParams snapshot with two Pan-target voices at
// phases 0 and 0.5, drive both Bass303 voices at the same note for a
// fixed number of samples, and confirm their `pan_side`
// contributions are time-mirrored — adding them sample-by-sample
// across a steady-state window should sum to ~0.

#[test]
fn two_voices_anti_phase_pan_sum_to_near_zero() {
    use crate::audio::dsp::AudioParams;
    use crate::audio::dsp::bass303::Bass303;

    let mut s = AppState::default();
    // Voice 0: Pan, phase 0.0.
    s.bass_voices[0].synth.lfo_target = BassLfoTarget::Pan;
    s.bass_voices[0].synth.lfo_rate = 0.3;
    s.bass_voices[0].synth.lfo_depth = 0.8;
    s.bass_voices[0].synth.lfo_delay = 0.0;
    s.bass_voices[0].synth.lfo_phase = 0.0;
    s.bass_voices[0].synth.distortion = 0.0;
    s.bass_voices[0].synth.volume = 0.8;
    // Voice 1: Pan, phase 0.5 — same other settings, same audio output
    // before pan modulation, so the side signals cancel pairwise.
    s.bass_voices[1].enabled = true;
    s.bass_voices[1].synth = s.bass_voices[0].synth.clone();
    s.bass_voices[1].synth.lfo_phase = 0.5;
    let p = AudioParams::from_app_state(&s);

    let mut v0 = Bass303::default();
    let mut v1 = Bass303::default();
    let tuning = crate::audio::dsp::TuningSystem::TwelveTet;
    v0.trigger(57, 0.0, 0.0, tuning); // A3
    v1.trigger(57, 0.0, 0.0, tuning);

    // Settle past the amp-attack ramp (a few ms is enough).
    let settle = 4096;
    let measure = 8192;
    let mut sum_abs = 0.0_f32;
    let mut max_abs_pair = 0.0_f32;
    for i in 0..(settle + measure) {
        v0.process(&p, &p.bass_voice_params[0]);
        v1.process(&p, &p.bass_voice_params[1]);
        if i >= settle {
            // Sum the two pan_side contributions; anti-phase means
            // they should cancel within float precision + the
            // tiny audio output difference between the voices.
            let pair_sum = v0.pan_side + v1.pan_side;
            sum_abs += pair_sum.abs();
            // Track the pair max so we know the test isn't trivially
            // passing because both contributions are zero.
            let pair_max = v0.pan_side.abs().max(v1.pan_side.abs());
            if pair_max > max_abs_pair {
                max_abs_pair = pair_max;
            }
        }
    }
    let avg_pair = sum_abs / measure as f32;
    assert!(
        max_abs_pair > 1e-3,
        "pan_side should be audibly non-zero before we test cancellation, peak {max_abs_pair}",
    );
    assert!(
        avg_pair < max_abs_pair * 0.05,
        "anti-phase pan_side should cancel — avg pair sum {avg_pair} vs peak {max_abs_pair}",
    );
}

#[test]
fn pan_target_off_keeps_pan_side_at_zero() {
    use crate::audio::dsp::AudioParams;
    use crate::audio::dsp::bass303::Bass303;

    // A voice with target=Cutoff (or any non-Pan) should leave
    // pan_side at zero — the master mixer's per-voice sum will
    // therefore be 0 for non-Pan patches and the existing pan_303
    // path stays the only contributor.
    let mut s = AppState::default();
    s.bass_voices[0].synth.lfo_target = BassLfoTarget::FilterCutoff;
    s.bass_voices[0].synth.lfo_rate = 0.3;
    s.bass_voices[0].synth.lfo_depth = 0.8;
    s.bass_voices[0].synth.lfo_delay = 0.0;
    let p = AudioParams::from_app_state(&s);

    let mut v = Bass303::default();
    let tuning = crate::audio::dsp::TuningSystem::TwelveTet;
    v.trigger(57, 0.0, 0.0, tuning);
    for _ in 0..2048 {
        v.process(&p, &p.bass_voice_params[0]);
        assert!(
            v.pan_side.abs() < 1e-9,
            "non-Pan target must leave pan_side at zero, got {}",
            v.pan_side,
        );
    }
}
