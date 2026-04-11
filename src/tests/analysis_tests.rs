// ─── tests/analysis_tests.rs ──────────────────────────────────────────────────
// Tests for audio analysis alerts and pattern observations.

use crate::audio::analysis::{AudioAnalysis, pattern_alerts};
use crate::state::{AppState, DrumVoice};

// ── Audio-level alerts ──────────────────────────────────────────────────────

#[test]
fn clipping_detected() {
    let a = AudioAnalysis {
        peak_db: -0.5,
        ..Default::default()
    };
    assert!(a.alerts().contains(&"CLIPPING"));
}

#[test]
fn near_clip_detected() {
    let a = AudioAnalysis {
        peak_db: -2.0,
        ..Default::default()
    };
    assert!(a.alerts().contains(&"near clip"));
}

#[test]
fn silence_detected() {
    let a = AudioAnalysis {
        peak_db: -50.0,
        ..Default::default()
    };
    assert!(a.alerts().contains(&"near silence"));
}

#[test]
fn normal_levels_no_alerts() {
    let a = AudioAnalysis {
        sub_rms_db: -20.0,
        low_rms_db: -15.0,
        mid_rms_db: -12.0,
        high_rms_db: -18.0,
        peak_db: -6.0,
        crest_db: 8.0,
        transients_per_bar: 4.0,
        duration_secs: 2.0,
    };
    assert!(a.alerts().is_empty());
}

#[test]
fn harsh_highs_detected() {
    let a = AudioAnalysis {
        high_rms_db: -4.0,
        peak_db: -6.0,
        crest_db: 8.0,
        ..Default::default()
    };
    assert!(a.alerts().contains(&"harsh highs"));
}

#[test]
fn over_compressed_detected() {
    let a = AudioAnalysis {
        peak_db: -8.0,
        crest_db: 2.0,
        ..Default::default()
    };
    assert!(a.alerts().contains(&"over-compressed"));
}

// ── Pattern alerts ──────────────────────────────────────────────────────────

#[test]
fn dense_bass_detected() {
    let mut s = AppState::default();
    s.sequencer.running = true;
    // Activate >80% of steps to trigger dense detection
    let threshold = (s.sequencer.bass_steps as f32 * 0.85) as usize;
    for step in s.sequencer.bass_pattern.iter_mut().take(threshold) {
        step.active = true;
        step.note = 45;
    }
    let alerts = pattern_alerts(&s);
    assert!(
        alerts.iter().any(|a| a.contains("dense")),
        "should detect dense bass: {:?}",
        alerts
    );
}

#[test]
fn sparse_bass_detected() {
    let mut s = AppState::default();
    s.sequencer.running = true;
    s.sequencer.bass_pattern[0].active = true;
    s.sequencer.bass_pattern[0].note = 45;
    let alerts = pattern_alerts(&s);
    assert!(
        alerts.iter().any(|a| a.contains("sparse")),
        "should detect sparse bass: {:?}",
        alerts
    );
}

#[test]
fn monotone_bass_detected() {
    let mut s = AppState::default();
    s.sequencer.running = true;
    for i in [0, 3, 6, 9, 12] {
        s.sequencer.bass_pattern[i].active = true;
        s.sequencer.bass_pattern[i].note = 45; // all same note
    }
    let alerts = pattern_alerts(&s);
    assert!(
        alerts.iter().any(|a| a.contains("monotone")),
        "should detect monotone bass: {:?}",
        alerts
    );
}

#[test]
fn varied_bass_no_monotone() {
    let mut s = AppState::default();
    s.sequencer.running = true;
    for (i, note) in [(0, 45), (3, 48), (6, 50), (9, 53), (12, 45)] {
        s.sequencer.bass_pattern[i].active = true;
        s.sequencer.bass_pattern[i].note = note;
    }
    let alerts = pattern_alerts(&s);
    assert!(
        !alerts.iter().any(|a| a.contains("monotone")),
        "varied notes should not be monotone: {:?}",
        alerts
    );
}

#[test]
fn no_kick_detected() {
    let mut s = AppState::default();
    s.sequencer.running = true;
    // All kick steps inactive (default)
    let alerts = pattern_alerts(&s);
    assert!(
        alerts.iter().any(|a| a.contains("no kick")),
        "should detect missing kick: {:?}",
        alerts
    );
}

#[test]
fn kick_present_no_alert() {
    let mut s = AppState::default();
    s.sequencer.running = true;
    if let Some(kick_pat) = s.sequencer.drum_patterns.get_mut(&DrumVoice::Kick808) {
        kick_pat[0].active = true;
        kick_pat[4].active = true;
    }
    let alerts = pattern_alerts(&s);
    assert!(
        !alerts.iter().any(|a| a.contains("no kick")),
        "kick present should not alert: {:?}",
        alerts
    );
}

#[test]
fn high_reverb_detected() {
    let mut s = AppState::default();
    s.fx.reverb_mix = 0.7;
    let alerts = pattern_alerts(&s);
    assert!(
        alerts.iter().any(|a| a.contains("reverb high")),
        "should detect high reverb: {:?}",
        alerts
    );
}

#[test]
fn normal_fx_no_alert() {
    let mut s = AppState::default();
    s.fx.reverb_mix = 0.1;
    s.fx.delay_feedback = 0.3;
    let alerts = pattern_alerts(&s);
    let fx_alerts: Vec<_> = alerts
        .iter()
        .filter(|a| a.contains("reverb") || a.contains("delay") || a.contains("distortion"))
        .collect();
    assert!(
        fx_alerts.is_empty(),
        "normal FX should not alert: {:?}",
        alerts
    );
}
