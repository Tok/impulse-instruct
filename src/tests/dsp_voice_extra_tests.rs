// ─── tests/dsp_voice_extra_tests.rs ──────────────────────────────────────────
// Extra DSP voice coverage: GranularVoice (silent / loaded behaviour) and
// GabberKick (silent before trigger, audible after, decays back to silence,
// clip parameter introduces distortion).  Both voices were previously only
// reachable through DspState::process_block which is too tied to hardware
// to test.

#[cfg(test)]
mod granular_voice_tests {
    use crate::audio::dsp::samplers::GranularVoice;
    use std::sync::Arc;

    #[test]
    fn no_sample_loaded_returns_silence() {
        let mut g = GranularVoice::new(7);
        let (l, r) = g.process(1.0, 1.0, 0.5, 0.5, 0.0, 0.0, 44100.0);
        assert_eq!(l, 0.0);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn empty_buffer_returns_silence() {
        let mut g = GranularVoice::new(7);
        g.load(Arc::new(Vec::new()));
        let (l, r) = g.process(1.0, 1.0, 0.5, 0.5, 0.0, 0.0, 44100.0);
        assert_eq!(l, 0.0);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn zero_volume_short_circuits_to_silence() {
        let mut g = GranularVoice::new(7);
        g.load(Arc::new(vec![0.5; 8000]));
        let (l, r) = g.process(0.0, 1.0, 0.5, 0.5, 0.0, 0.0, 44100.0);
        assert_eq!(l, 0.0);
        assert_eq!(r, 0.0);
    }

    #[test]
    fn audible_after_load_with_volume_and_density() {
        let mut g = GranularVoice::new(7);
        // 0.5 amplitude content over a couple of seconds of audio.
        g.load(Arc::new(vec![0.5_f32; 88_200]));
        let mut peak = 0.0_f32;
        for _ in 0..(44_100 / 4) {
            let (l, r) = g.process(1.0, 1.0, 0.5, 0.5, 0.0, 0.0, 44100.0);
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak > 0.0, "expected non-zero output once a grain spawns");
        assert!(peak.is_finite());
    }

    #[test]
    fn output_stays_finite_with_pitch_scatter_and_jitter() {
        let mut g = GranularVoice::new(7);
        g.load(Arc::new(vec![0.3_f32; 44_100]));
        for _ in 0..1000 {
            let (l, r) = g.process(0.7, 0.6, 0.4, 0.3, 0.5, 0.4, 44100.0);
            assert!(l.is_finite() && r.is_finite());
        }
    }

    #[test]
    fn silent_input_buffer_produces_silent_output() {
        let mut g = GranularVoice::new(7);
        g.load(Arc::new(vec![0.0_f32; 44_100]));
        for _ in 0..(44_100 / 4) {
            let (l, r) = g.process(1.0, 1.0, 0.5, 0.5, 0.0, 0.0, 44100.0);
            assert_eq!(l, 0.0);
            assert_eq!(r, 0.0);
        }
    }
}

#[cfg(test)]
mod gabber_kick_tests {
    use crate::audio::dsp::AudioParams;
    use crate::audio::dsp::gabber_kick::GabberKick;
    use crate::state::AppState;

    fn params() -> AudioParams {
        AudioParams::from_app_state(&AppState::default())
    }

    #[test]
    fn fresh_gabber_kick_is_silent_before_trigger() {
        let mut k = GabberKick::new(7);
        let p = params();
        for _ in 0..100 {
            let s = k.process(&p, 44100.0);
            assert_eq!(s, 0.0);
        }
    }

    #[test]
    fn audible_immediately_after_trigger() {
        let mut k = GabberKick::new(7);
        let mut p = params();
        p.gabber_volume = 1.0;
        k.trigger();
        let mut peak = 0.0_f32;
        for _ in 0..1000 {
            peak = peak.max(k.process(&p, 44100.0).abs());
        }
        assert!(peak > 0.0);
        assert!(peak.is_finite());
    }

    #[test]
    fn output_decays_back_to_silence_after_long_run() {
        let mut k = GabberKick::new(7);
        let mut p = params();
        p.gabber_volume = 1.0;
        p.gabber_decay = 0.0; // shortest decay
        k.trigger();
        // Burn through enough samples for the envelope to deactivate.
        for _ in 0..44_100 {
            let _ = k.process(&p, 44100.0);
        }
        // Sample the tail — should be at or near zero.
        let mut tail_peak = 0.0_f32;
        for _ in 0..1000 {
            tail_peak = tail_peak.max(k.process(&p, 44100.0).abs());
        }
        assert!(tail_peak < 0.01, "expected silent tail, got {tail_peak}");
    }

    #[test]
    fn clip_param_changes_the_signal_shape() {
        let mut k_clean = GabberKick::new(7);
        let mut k_dirty = GabberKick::new(7);
        let mut p_clean = params();
        let mut p_dirty = params();
        p_clean.gabber_volume = 1.0;
        p_dirty.gabber_volume = 1.0;
        p_clean.gabber_clip = 0.0;
        p_dirty.gabber_clip = 1.0;
        k_clean.trigger();
        k_dirty.trigger();
        let (mut clean_peak, mut dirty_peak) = (0.0_f32, 0.0_f32);
        for _ in 0..2000 {
            clean_peak = clean_peak.max(k_clean.process(&p_clean, 44100.0).abs());
            dirty_peak = dirty_peak.max(k_dirty.process(&p_dirty, 44100.0).abs());
        }
        // Both should be audible.
        assert!(clean_peak > 0.0);
        assert!(dirty_peak > 0.0);
        // Heavy clip pushes the signal toward unit amplitude.
        assert!(
            dirty_peak > 0.7,
            "clip=1 should push peak near unity, got {dirty_peak}"
        );
    }

    #[test]
    fn zero_volume_silences_everything_after_trigger() {
        let mut k = GabberKick::new(7);
        let mut p = params();
        p.gabber_volume = 0.0;
        k.trigger();
        for _ in 0..1000 {
            assert_eq!(k.process(&p, 44100.0), 0.0);
        }
    }
}

#[cfg(test)]
mod starts_with_persona_tests {
    use crate::log_fmt::starts_with_persona;

    #[test]
    fn matches_simple_uppercase_persona() {
        assert!(starts_with_persona("PULSE: hello"));
        assert!(starts_with_persona("BASS: dropping in"));
    }

    #[test]
    fn matches_persona_with_digits_and_underscore() {
        assert!(starts_with_persona("AGENT_42: ready"));
        assert!(starts_with_persona("X1: spinning"));
    }

    #[test]
    fn rejects_lowercase_persona() {
        assert!(!starts_with_persona("pulse: hello"));
    }

    #[test]
    fn rejects_when_no_colon_space() {
        assert!(!starts_with_persona("PULSE hello"));
        assert!(!starts_with_persona("PULSE:hello")); // missing space
    }

    #[test]
    fn rejects_empty_string() {
        assert!(!starts_with_persona(""));
    }

    #[test]
    fn rejects_single_uppercase_letter_with_no_colon() {
        assert!(!starts_with_persona("A"));
    }

    #[test]
    fn rejects_persona_at_end_of_line_with_no_following_text() {
        // Persona token + colon but nothing after — not a valid line either.
        assert!(!starts_with_persona("PULSE:"));
    }

    #[test]
    fn rejects_brackets_or_other_leading_punctuation() {
        assert!(!starts_with_persona("[INFO] PULSE: hi"));
        assert!(!starts_with_persona(" PULSE: hi")); // leading space
    }
}
