// ─── tests/rec_chop_tests.rs ──────────────────────────────────────────────────
// REC → CHOP helper tests — `linearise_tap` ring-buffer reordering plus
// the onset detector's contract on a synthetic break-style buffer.
//
// The full `record_chop_into_amen` writeback path needs an `ImpulseApp`
// (rtrb sender, granular_tap, state mutations), so it's exercised
// indirectly: we lock the pure pieces here and trust the integration
// at the call site.

#[cfg(test)]
mod linearise_tap {
    use crate::ui::panels::amen::linearise_tap;

    #[test]
    fn empty_tap_yields_empty_vec() {
        let out = linearise_tap(&[], 0);
        assert!(out.is_empty());
    }

    #[test]
    fn head_zero_returns_buffer_in_place() {
        let tap = vec![1.0, 2.0, 3.0, 4.0];
        let out = linearise_tap(&tap, 0);
        assert_eq!(out, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn head_in_middle_rotates_so_oldest_first() {
        // head=2 means slot 2 is where the next write would go, so
        // slot 2 holds the OLDEST sample.  Output should be 3,4,1,2.
        let tap = vec![1.0, 2.0, 3.0, 4.0];
        let out = linearise_tap(&tap, 2);
        assert_eq!(out, vec![3.0, 4.0, 1.0, 2.0]);
    }

    #[test]
    fn head_at_len_wraps_via_modulo() {
        // head=4 on a 4-slot ring is equivalent to head=0.
        let tap = vec![1.0, 2.0, 3.0, 4.0];
        let out = linearise_tap(&tap, 4);
        assert_eq!(out, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn output_length_matches_input() {
        let tap = vec![0.5_f32; 1024];
        let out = linearise_tap(&tap, 555);
        assert_eq!(out.len(), 1024);
    }
}

#[cfg(test)]
mod onset_chop_contract {
    use crate::audio::onset::detect_onsets;

    /// Build a synthetic ~1 s buffer at 44.1 kHz with N evenly-spaced
    /// transients (Dirac-style impulses surrounded by silence).  Mimics
    /// the cleanest possible break loop so the onset detector's
    /// peak picker has unambiguous targets.
    fn pulse_train(seconds: f32, n_pulses: usize) -> Vec<f32> {
        let sr = 44100.0_f32;
        let n = (sr * seconds) as usize;
        let mut buf = vec![0.0_f32; n];
        if n_pulses == 0 || n == 0 {
            return buf;
        }
        let stride = n / n_pulses;
        for i in 0..n_pulses {
            let pos = i * stride;
            // 64-sample-wide envelope so RMS windows pick it up.
            for k in 0..64 {
                if pos + k < n {
                    buf[pos + k] = if k < 16 { 1.0 } else { 0.0 };
                }
            }
        }
        buf
    }

    #[test]
    fn detect_onsets_anchors_slice_zero_at_zero() {
        let buf = pulse_train(1.0, 8);
        let onsets = detect_onsets(&buf, 44100.0, 8);
        assert!(!onsets.is_empty());
        assert!((onsets[0] - 0.0).abs() < 1e-6, "got {}", onsets[0]);
    }

    #[test]
    fn detected_onsets_stay_in_unit_range_and_sorted() {
        let buf = pulse_train(1.0, 8);
        let onsets = detect_onsets(&buf, 44100.0, 8);
        for w in onsets.windows(2) {
            assert!(w[0] <= w[1], "not sorted: {w:?}");
        }
        for &v in &onsets {
            assert!((0.0..=1.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn silent_buffer_returns_safe_default() {
        // All-zero input is a valid-but-quiet buffer; the detector
        // must not panic and must give caller a usable slice 0.
        let buf = vec![0.0_f32; 44100];
        let onsets = detect_onsets(&buf, 44100.0, 8);
        assert!(!onsets.is_empty());
        assert!((onsets[0] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn very_short_buffer_returns_safe_default() {
        // < 512 samples → returns just [0.0] (documented contract).
        let buf = vec![0.5_f32; 100];
        let onsets = detect_onsets(&buf, 44100.0, 8);
        assert_eq!(onsets, vec![0.0]);
    }

    #[test]
    fn max_slices_caps_returned_count() {
        let buf = pulse_train(2.0, 32);
        let onsets = detect_onsets(&buf, 44100.0, 4);
        assert!(
            onsets.len() <= 4,
            "got {} onsets, expected ≤ 4",
            onsets.len()
        );
    }
}
