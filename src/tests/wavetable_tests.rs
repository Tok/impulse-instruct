// ─── tests/wavetable_tests.rs ────────────────────────────────────────────────
// Wavetable voice coverage — defaults, LLM apply path, frame split
// correctness on load, and the per-sample output contract (silent
// before trigger, periodic after, period matches the triggered note).

use std::sync::Arc;

use crate::audio::dsp::AudioParams;
use crate::audio::dsp::wavetable::{WT_FRAME_SIZE, WavetableVoice};
use crate::state::{AppState, ModuleKind, apply_llm_update};

// ─── Defaults ────────────────────────────────────────────────────────────────

#[test]
fn wavetable_defaults_are_silent_and_neutral() {
    let s = AppState::default();
    assert!(!s.wavetable.enabled);
    assert_eq!(s.wavetable.position, 0.0);
    assert_eq!(s.wavetable.phase_offset, 0.0);
    assert_eq!(s.wavetable.pan, 0.0);
    assert_eq!(s.wavetable.pitch_offset_semi, 0.0);
    assert!(s.wavetable.wave_path.is_empty());
}

#[test]
fn sequencer_starts_with_empty_wavetable_pattern_and_default_step_count() {
    let s = AppState::default();
    assert_eq!(s.sequencer.wavetable_steps, 32);
    assert!(!s.sequencer.wavetable_pattern.is_empty());
    assert!(s.sequencer.wavetable_pattern.iter().all(|st| !st.active));
}

// ─── ModuleKind wiring ───────────────────────────────────────────────────────

#[test]
fn wavetable_voice_is_voice_zone_with_expected_flags() {
    let k = ModuleKind::WavetableVoice;
    assert!(k.has_audio_output());
    assert_eq!(k.default_zone(), crate::state::Zone::Voice);
    assert_eq!(k.label(), "WAVETABLE");
}

// ─── LLM apply ───────────────────────────────────────────────────────────────

#[test]
fn apply_llm_update_writes_wavetable_voice_fields() {
    let s0 = AppState::default();
    let update = serde_json::json!({
        "wavetable": {
            "enabled":  true,
            "position": 0.6,
            "phase_offset": 0.25,
            "volume":   0.8,
            "pan":      0.3,
            "pitch_offset_semi": -7.0,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(s1.wavetable.enabled);
    assert!((s1.wavetable.position - 0.6).abs() < 1e-6);
    assert!((s1.wavetable.phase_offset - 0.25).abs() < 1e-6);
    assert!((s1.wavetable.volume - 0.8).abs() < 1e-6);
    assert!((s1.wavetable.pan - 0.3).abs() < 1e-6);
    assert!((s1.wavetable.pitch_offset_semi - -7.0).abs() < 1e-6);
}

#[test]
fn apply_llm_update_writes_wavetable_sequencer_pattern() {
    let s0 = AppState::default();
    let update = serde_json::json!({
        "wavetable": {
            "wavetable_steps": [true, false, true, false],
            "wavetable_notes": [60, 64, 67, 72],
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(s1.sequencer.wavetable_pattern[0].active);
    assert!(!s1.sequencer.wavetable_pattern[1].active);
    assert!(s1.sequencer.wavetable_pattern[2].active);
    assert_eq!(s1.sequencer.wavetable_pattern[0].note, 60);
    assert_eq!(s1.sequencer.wavetable_pattern[3].note, 72);
}

#[test]
fn apply_llm_update_respects_wavetable_field_locks() {
    let mut s0 = AppState::default();
    s0.wavetable.position = 0.4;
    s0.llm
        .locked_params
        .insert("wavetable.position".to_string());
    let update = serde_json::json!({
        "wavetable": {
            "position":     0.9,
            "phase_offset": 0.1,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(
        (s1.wavetable.position - 0.4).abs() < 1e-6,
        "locked position kept"
    );
    assert!(
        (s1.wavetable.phase_offset - 0.1).abs() < 1e-6,
        "unlocked phase_offset applied"
    );
}

// ─── DSP ─────────────────────────────────────────────────────────────────────

fn make_params(volume: f32, position: f32) -> AudioParams {
    let mut s = AppState::default();
    s.wavetable.enabled = true;
    s.wavetable.position = position;
    s.wavetable.volume = volume;
    s.rack.add_module(ModuleKind::WavetableVoice);
    AudioParams::from_app_state(&s)
}

/// Build a 4-frame sine-table buffer: each frame holds one period of
/// a unit-amplitude sine.  Lets the period-match test compare the
/// voice's output against a known clean source.
fn sine_wavetable(num_frames: usize) -> Arc<Vec<f32>> {
    let mut buf = Vec::with_capacity(num_frames * WT_FRAME_SIZE);
    for _ in 0..num_frames {
        for i in 0..WT_FRAME_SIZE {
            let phase = i as f32 / WT_FRAME_SIZE as f32;
            buf.push((phase * std::f32::consts::TAU).sin());
        }
    }
    Arc::new(buf)
}

#[test]
fn wavetable_silent_before_trigger() {
    let mut v = WavetableVoice::new();
    v.load(sine_wavetable(4));
    let p = make_params(0.8, 0.0);
    let mut nonzero = 0;
    for _ in 0..1024 {
        let y = v.process(48_000.0, &p);
        if y.abs() > 1e-6 {
            nonzero += 1;
        }
    }
    assert_eq!(nonzero, 0, "un-triggered wavetable should be silent");
}

#[test]
fn wavetable_silent_when_no_table_is_loaded() {
    // Triggering with no table should not produce any sound — the
    // voice's frame_count is 0, the early return in `process` keeps
    // the audio bus clean.
    let mut v = WavetableVoice::new();
    let p = make_params(1.0, 0.0);
    v.trigger(
        69,
        crate::audio::dsp::TuningSystem::TwelveTet,
        0.0,
        0.0,
        0.0,
    );
    let mut nonzero = 0;
    for _ in 0..1024 {
        let y = v.process(48_000.0, &p);
        if y.abs() > 1e-6 {
            nonzero += 1;
        }
    }
    assert_eq!(nonzero, 0, "wavetable with no table loaded must be silent");
}

#[test]
fn wavetable_triggered_audio_is_finite_and_audible() {
    let mut v = WavetableVoice::new();
    v.load(sine_wavetable(4));
    v.trigger(
        69,
        crate::audio::dsp::TuningSystem::TwelveTet,
        0.0,
        0.0,
        0.0,
    );
    let p = make_params(1.0, 0.0);
    let mut peak = 0.0_f32;
    for _ in 0..2048 {
        let y = v.process(48_000.0, &p);
        assert!(y.is_finite());
        if y.abs() > peak {
            peak = y.abs();
        }
    }
    assert!(
        peak > 0.1,
        "triggered wavetable should produce audible output, got peak {peak}"
    );
}

#[test]
fn wavetable_period_matches_triggered_note_via_autocorrelation() {
    // A4 (440 Hz) sine wavetable should yield a 109-sample period at
    // 48 kHz.  Auto-correlation at the expected lag should be much
    // higher than at half that lag.
    let mut v = WavetableVoice::new();
    v.load(sine_wavetable(4));
    let p = make_params(1.0, 0.0);
    let sr = 48_000.0_f32;
    let freq = 440.0_f32;
    v.trigger(
        69,
        crate::audio::dsp::TuningSystem::TwelveTet,
        0.0,
        0.0,
        0.0,
    );

    let settle = 1024;
    let measure = 4096;
    let mut samples = Vec::with_capacity(measure);
    for i in 0..(settle + measure) {
        let y = v.process(sr, &p);
        if i >= settle {
            samples.push(y);
        }
    }
    let period = (sr / freq).round() as usize;
    let auto_corr = |lag: usize| -> f32 {
        let mut num = 0.0_f32;
        let mut den_a = 0.0_f32;
        let mut den_b = 0.0_f32;
        for i in 0..(samples.len() - lag) {
            let a = samples[i];
            let b = samples[i + lag];
            num += a * b;
            den_a += a * a;
            den_b += b * b;
        }
        num / (den_a * den_b).sqrt().max(1e-9)
    };
    let on_period = auto_corr(period);
    let half_period = auto_corr(period / 2);
    assert!(
        on_period > 0.8,
        "on-period autocorrelation should be near 1 for a sine table, got {on_period}",
    );
    assert!(
        on_period > half_period + 0.3,
        "on-period {on_period} should clearly exceed half-period {half_period}",
    );
}

#[test]
fn wavetable_load_splits_buffer_into_frames() {
    // Internal contract: feeding a buffer of N×WT_FRAME_SIZE samples
    // produces a voice that plays N frames.  The position-knob test
    // probes this indirectly: at position=0 the output should match
    // the first frame's samples; at position=1 it matches the last
    // frame's samples.
    let n_frames = 3;
    // Construct distinct-DC frames so we can tell them apart at
    // phase 0.  Frame k holds the constant value (k+1)/4 for every
    // sample.
    let mut buf = Vec::with_capacity(n_frames * WT_FRAME_SIZE);
    for k in 0..n_frames {
        let level = (k as f32 + 1.0) / 4.0;
        for _ in 0..WT_FRAME_SIZE {
            buf.push(level);
        }
    }
    let arc_buf = Arc::new(buf);
    let mut v = WavetableVoice::new();
    v.load(arc_buf);
    // Use a low note so the phase-advance per sample is small and
    // the read stays close to phase=0 for the whole settle window;
    // that lets us inspect the frame DC level directly.
    v.trigger(
        24,
        crate::audio::dsp::TuningSystem::TwelveTet,
        0.0,
        0.0,
        0.0,
    );

    let p_low = make_params(1.0, 0.0);
    let p_high = make_params(1.0, 1.0);
    // Skip the attack ramp; sample within ~10 ms of trigger.
    let skip = 480;
    let probe = 4;
    let mut sum_low = 0.0_f32;
    let mut sum_high = 0.0_f32;
    // Two probe runs — first frame, then last frame.
    for i in 0..(skip + probe) {
        let y = v.process(48_000.0, &p_low);
        if i >= skip {
            sum_low += y.abs();
        }
    }
    let mut v2 = WavetableVoice::new();
    let mut buf2 = Vec::with_capacity(n_frames * WT_FRAME_SIZE);
    for k in 0..n_frames {
        let level = (k as f32 + 1.0) / 4.0;
        for _ in 0..WT_FRAME_SIZE {
            buf2.push(level);
        }
    }
    v2.load(Arc::new(buf2));
    v2.trigger(
        24,
        crate::audio::dsp::TuningSystem::TwelveTet,
        0.0,
        0.0,
        0.0,
    );
    for i in 0..(skip + probe) {
        let y = v2.process(48_000.0, &p_high);
        if i >= skip {
            sum_high += y.abs();
        }
    }
    // Frame at pos=1 should have higher DC level (level = n_frames/4)
    // than frame at pos=0 (level = 1/4), so the absolute-sum ratio
    // approximates the level ratio.
    assert!(
        sum_high > sum_low * 1.5,
        "position=1 level {sum_high} should clearly exceed position=0 level {sum_low}",
    );
}
