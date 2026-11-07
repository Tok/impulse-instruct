// ─── tests/pluck_tests.rs ────────────────────────────────────────────────────
// Karplus-Strong plucked-string voice coverage.  Locks the state
// defaults, LLM apply shape, rack-module flags, and the DSP's
// fundamental-frequency claim — a triggered delay-line should
// produce output whose zero-crossing count matches the triggered
// MIDI note.

use crate::state::{AppState, ModuleKind, apply_llm_update};

// ─── Defaults ────────────────────────────────────────────────────────────────

#[test]
fn pluck_defaults_are_silent_and_musical() {
    let s = AppState::default();
    assert!(!s.pluck.enabled);
    assert_eq!(s.pluck.pitch_offset_semi, 0.0);
    assert!(s.pluck.damping > 0.7 && s.pluck.damping < 0.95);
    assert!(s.pluck.brightness > 0.5 && s.pluck.brightness < 0.9);
    assert!(s.pluck.volume > 0.5 && s.pluck.volume < 1.0);
    assert_eq!(s.pluck.pan, 0.0);
}

#[test]
fn sequencer_starts_with_empty_pluck_pattern_and_default_step_count() {
    let s = AppState::default();
    assert_eq!(s.sequencer.pluck_steps, 32);
    assert!(!s.sequencer.pluck_pattern.is_empty());
    assert!(s.sequencer.pluck_pattern.iter().all(|st| !st.active));
}

// ─── ModuleKind wiring ───────────────────────────────────────────────────────

#[test]
fn pluck_string_is_voice_zone_with_expected_flags() {
    let k = ModuleKind::PluckString;
    assert!(k.has_audio_output());
    assert_eq!(k.default_zone(), crate::state::Zone::Voice);
    assert_eq!(k.label(), "PLUCK");
}

// ─── LLM apply ───────────────────────────────────────────────────────────────

#[test]
fn apply_llm_update_writes_pluck_voice_fields() {
    let s0 = AppState::default();
    let update = serde_json::json!({
        "pluck": {
            "enabled": true,
            "damping": 0.92,
            "brightness": 0.35,
            "volume": 0.6,
            "pan": -0.4,
            "pitch_offset_semi": 5.0,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(s1.pluck.enabled);
    assert!((s1.pluck.damping - 0.92).abs() < 1e-6);
    assert!((s1.pluck.brightness - 0.35).abs() < 1e-6);
    assert!((s1.pluck.volume - 0.6).abs() < 1e-6);
    assert!((s1.pluck.pan - -0.4).abs() < 1e-6);
    assert!((s1.pluck.pitch_offset_semi - 5.0).abs() < 1e-6);
}

#[test]
fn apply_llm_update_writes_pluck_sequencer_pattern() {
    let s0 = AppState::default();
    let update = serde_json::json!({
        "pluck": {
            "pluck_steps": [true, false, false, true, false, false, true, false],
            "pluck_notes": [60, 62, 64, 65, 67, 69, 71, 72],
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(s1.sequencer.pluck_pattern[0].active);
    assert!(!s1.sequencer.pluck_pattern[1].active);
    assert!(s1.sequencer.pluck_pattern[3].active);
    assert_eq!(s1.sequencer.pluck_pattern[0].note, 60);
    assert_eq!(s1.sequencer.pluck_pattern[7].note, 72);
}

#[test]
fn apply_llm_update_respects_pluck_field_locks() {
    let mut s0 = AppState::default();
    s0.pluck.damping = 0.5;
    s0.llm.locked_params.insert("pluck.damping".to_string());
    let update = serde_json::json!({
        "pluck": {
            "damping":    0.95,
            "brightness": 0.2,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!((s1.pluck.damping - 0.5).abs() < 1e-6, "locked damping kept");
    assert!(
        (s1.pluck.brightness - 0.2).abs() < 1e-6,
        "unlocked brightness applied"
    );
}

// ─── DSP ─────────────────────────────────────────────────────────────────────

use crate::audio::dsp::pluck::PluckVoice;
use crate::state::FxState;

/// Build an AudioParams snapshot with all pluck knobs set as requested
/// and everything else at a sensible flat default so the DSP test
/// doesn't leak unrelated voice / FX activity.
fn pluck_params(damping: f32, brightness: f32, volume: f32) -> crate::audio::dsp::AudioParams {
    // Synthesise a params struct from a fresh AppState, then override
    // just the pluck-related knobs we care about.
    let mut s = AppState::default();
    s.pluck.enabled = true;
    s.pluck.damping = damping;
    s.pluck.brightness = brightness;
    s.pluck.volume = volume;
    // Make sure the state thinks the pluck is racked so rack_pluck flips
    // true in params_from.  Add the module (enabled) via `add_module`.
    s.rack.add_module(ModuleKind::PluckString);
    crate::audio::dsp::AudioParams::from_app_state(&s)
}

#[test]
fn pluck_outputs_silence_before_trigger() {
    let mut v = PluckVoice::new();
    let p = pluck_params(0.85, 0.7, 0.7);
    let mut nonzero = 0;
    for _ in 0..1024 {
        let y = v.process(48_000.0, &p);
        if y.abs() > 1e-5 {
            nonzero += 1;
        }
    }
    assert_eq!(nonzero, 0, "un-triggered pluck should be silent");
}

#[test]
fn pluck_produces_audio_after_trigger() {
    // Trigger on A4 (MIDI 69 = 440 Hz) with high damping + brightness
    // so the tail doesn't decay before the measurement window.  At
    // 48 kHz we expect a fat delay-line of roughly 109 samples.
    let mut v = PluckVoice::new();
    let tuning = crate::audio::dsp::TuningSystem::TwelveTet;
    v.trigger(69, tuning, 0.0, 0.0, 48_000.0, 0.0);
    let p = pluck_params(0.95, 0.9, 1.0);
    let mut peak = 0.0_f32;
    for _ in 0..2048 {
        let y = v.process(48_000.0, &p);
        if y.abs() > peak {
            peak = y.abs();
        }
    }
    assert!(
        peak > 0.05,
        "triggered pluck should produce audible output, got peak {peak}"
    );
}

#[test]
fn pluck_period_matches_delay_line_length() {
    // A Karplus-Strong voice triggered at MIDI note N has period
    // round(sr / freq(N)) samples — verified here via auto-
    // correlation: cross-correlate the output with itself shifted by
    // that many samples; correlation at the correct lag should be
    // visibly higher than at nearby lags.  Zero-crossing counting
    // doesn't work for K-S because the noise burst leaves harmonic
    // colour that inflates the crossing count above 2·fundamental.
    let mut v = PluckVoice::new();
    let tuning = crate::audio::dsp::TuningSystem::TwelveTet;
    let freq = 440.0_f32;
    let sr = 48_000.0_f32;
    v.trigger(69, tuning, 0.0, 0.0, sr, 0.0);
    let p = pluck_params(0.98, 1.0, 1.0);

    let settle = 2048;
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
    let target = auto_corr(period);
    // Mis-aligned lag halfway between full-period peaks should be
    // lower than the on-period correlation by a healthy margin.
    let off = auto_corr(period / 2);
    assert!(
        target > 0.4,
        "auto-correlation at period {period} should be strong, got {target}",
    );
    assert!(
        target > off + 0.1,
        "on-period corr {target} should exceed half-period corr {off}",
    );
}

// Silence unused-import warning for FxState in future test expansions.
#[allow(dead_code)]
fn _types() -> FxState {
    FxState::default()
}
