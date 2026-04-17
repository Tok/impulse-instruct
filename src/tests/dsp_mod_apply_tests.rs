// ─── tests/dsp_mod_apply_tests.rs ────────────────────────────────────────────
// Cover the LFO target dispatcher in audio/dsp/mod_apply.rs — 71 opcodes,
// each adding a (sometimes scaled) modulation value to a specific
// AudioParams field with a per-target clamp.  These tests pin down both
// the mapping and the clamp boundaries so future opcode additions don't
// silently shift any existing routing.

use crate::audio::dsp::AudioParams;
use crate::audio::dsp::mod_apply::apply_mod_target;
use crate::state::AppState;

/// Build a default AudioParams snapshot via the only public constructor.
fn fresh() -> AudioParams {
    AudioParams::from_app_state(&AppState::default())
}

#[cfg(test)]
mod no_op_targets {
    use super::*;

    #[test]
    fn opcode_zero_is_a_no_op() {
        let mut p = fresh();
        let before = p.cutoff;
        apply_mod_target(&mut p, 0, 1.0);
        assert_eq!(p.cutoff, before, "opcode 0 should not write any field");
    }

    #[test]
    fn unknown_opcode_above_table_is_a_no_op() {
        let mut p = fresh();
        let before = p.cutoff;
        apply_mod_target(&mut p, 200, 1.0);
        assert_eq!(p.cutoff, before);
    }

    #[test]
    fn zero_mod_value_leaves_target_unchanged() {
        let mut p = fresh();
        let before = p.cutoff;
        apply_mod_target(&mut p, 1, 0.0);
        assert_eq!(p.cutoff, before);
    }
}

#[cfg(test)]
mod unipolar_clamps {
    use super::*;

    #[test]
    fn cutoff_clamps_to_unit_range_above_one() {
        let mut p = fresh();
        p.cutoff = 0.5;
        apply_mod_target(&mut p, 1, 5.0);
        assert_eq!(p.cutoff, 1.0);
    }

    #[test]
    fn cutoff_clamps_to_unit_range_below_zero() {
        let mut p = fresh();
        p.cutoff = 0.5;
        apply_mod_target(&mut p, 1, -5.0);
        assert_eq!(p.cutoff, 0.0);
    }

    #[test]
    fn reverb_mix_clamps_at_unity() {
        let mut p = fresh();
        p.reverb_mix = 0.7;
        apply_mod_target(&mut p, 5, 1.0);
        assert_eq!(p.reverb_mix, 1.0);
    }

    #[test]
    fn delay_feedback_clamps_just_below_unity() {
        // 0.99 ceiling protects the feedback loop from runaway gain.
        let mut p = fresh();
        p.delay_feedback = 0.5;
        apply_mod_target(&mut p, 7, 5.0);
        assert!((p.delay_feedback - 0.99).abs() < 1e-6);
    }

    #[test]
    fn small_positive_mod_adds_to_existing_value() {
        let mut p = fresh();
        p.cutoff = 0.3;
        apply_mod_target(&mut p, 1, 0.2);
        assert!((p.cutoff - 0.5).abs() < 1e-6);
    }
}

#[cfg(test)]
mod scaled_mod_targets {
    use super::*;

    #[test]
    fn delay_time_uses_half_scaling() {
        let mut p = fresh();
        p.delay_time = 0.0;
        apply_mod_target(&mut p, 6, 1.0);
        // 0 + 1.0 * 0.5 = 0.5
        assert!((p.delay_time - 0.5).abs() < 1e-6);
    }

    #[test]
    fn kick808_pitch_uses_half_scaling() {
        let mut p = fresh();
        p.kick808_pitch = 0.2;
        apply_mod_target(&mut p, 10, 1.0);
        assert!((p.kick808_pitch - 0.7).abs() < 1e-6);
    }

    #[test]
    fn distortion_drive_uses_half_scaling() {
        let mut p = fresh();
        p.distortion_drive = 0.0;
        apply_mod_target(&mut p, 13, 1.0);
        assert!((p.distortion_drive - 0.5).abs() < 1e-6);
    }

    #[test]
    fn master_volume_uses_30_percent_scaling_with_1_5_ceiling() {
        let mut p = fresh();
        p.master_volume = 1.0;
        apply_mod_target(&mut p, 14, 5.0);
        // 1.0 + 5*0.3 = 2.5 → clamp to 1.5
        assert!((p.master_volume - 1.5).abs() < 1e-6);
    }
}

#[cfg(test)]
mod pitch_accumulators {
    use super::*;

    #[test]
    fn lfo_pitch_mod_st_accumulates_without_reset() {
        // Pitch mod is += rather than clamp-set, so two calls add.
        let mut p = fresh();
        p.lfo_pitch_mod_st = 0.0;
        apply_mod_target(&mut p, 3, 0.5); // +6 st
        apply_mod_target(&mut p, 3, 0.5); // +12 st total
        assert!((p.lfo_pitch_mod_st - 12.0).abs() < 1e-4);
    }

    #[test]
    fn lfo_pitch_mod_st_uses_12st_scaling() {
        let mut p = fresh();
        p.lfo_pitch_mod_st = 0.0;
        apply_mod_target(&mut p, 3, 1.0);
        assert!((p.lfo_pitch_mod_st - 12.0).abs() < 1e-4);
    }

    #[test]
    fn an1x_pitch_mod_st_uses_12st_scaling_and_accumulates() {
        let mut p = fresh();
        p.an1x_pitch_mod_st = 0.0;
        apply_mod_target(&mut p, 16, -1.0);
        apply_mod_target(&mut p, 16, 0.5);
        assert!((p.an1x_pitch_mod_st - -6.0).abs() < 1e-4);
    }
}

#[cfg(test)]
mod bipolar_pan_clamps {
    use super::*;

    #[test]
    fn pan_303_clamps_at_minus_one() {
        let mut p = fresh();
        p.pan_303 = 0.0;
        apply_mod_target(&mut p, 17, -5.0);
        assert_eq!(p.pan_303, -1.0);
    }

    #[test]
    fn pan_303_clamps_at_plus_one() {
        let mut p = fresh();
        p.pan_303 = 0.0;
        apply_mod_target(&mut p, 17, 5.0);
        assert_eq!(p.pan_303, 1.0);
    }

    #[test]
    fn each_drum_pan_target_routes_to_its_own_field() {
        // Sweep opcodes 20..=26 — each should write a distinct pan field
        // (kick808 / snare808 / hihat808 / kick909 / snare909 / hihat909 /
        // clap909) and only that field.
        let mut p = fresh();
        apply_mod_target(&mut p, 20, 0.5);
        apply_mod_target(&mut p, 21, -0.5);
        apply_mod_target(&mut p, 22, 0.3);
        apply_mod_target(&mut p, 23, -0.3);
        apply_mod_target(&mut p, 24, 0.2);
        apply_mod_target(&mut p, 25, -0.2);
        apply_mod_target(&mut p, 26, 0.1);
        assert!((p.pan_kick808 - 0.5).abs() < 1e-6);
        assert!((p.pan_snare808 - -0.5).abs() < 1e-6);
        assert!((p.pan_hihat808 - 0.3).abs() < 1e-6);
        assert!((p.pan_kick909 - -0.3).abs() < 1e-6);
        assert!((p.pan_snare909 - 0.2).abs() < 1e-6);
        assert!((p.pan_hihat909 - -0.2).abs() < 1e-6);
        assert!((p.pan_clap909 - 0.1).abs() < 1e-6);
    }
}

#[cfg(test)]
mod bipolar_eq_clamps {
    use super::*;

    #[test]
    fn eq_low_gain_clamps_in_signed_unit_range() {
        let mut p = fresh();
        p.eq_low_gain = 0.0;
        apply_mod_target(&mut p, 41, -2.0);
        assert_eq!(p.eq_low_gain, -1.0);
        apply_mod_target(&mut p, 41, 5.0);
        assert_eq!(p.eq_low_gain, 1.0);
    }

    #[test]
    fn eq_mid_and_hi_share_the_same_clamp_shape() {
        let mut p = fresh();
        apply_mod_target(&mut p, 42, 5.0);
        apply_mod_target(&mut p, 43, -5.0);
        assert_eq!(p.eq_mid_gain, 1.0);
        assert_eq!(p.eq_hi_gain, -1.0);
    }
}

#[cfg(test)]
mod special_range_clamps {
    use super::*;

    #[test]
    fn amen_gate_minimum_is_005_not_zero() {
        // amen.gate has a minimum of 0.05 (avoids zero-length envelopes).
        let mut p = fresh();
        p.amen_gate = 0.5;
        apply_mod_target(&mut p, 62, -5.0);
        assert!((p.amen_gate - 0.05).abs() < 1e-6);
    }

    #[test]
    fn amen_volume_clamps_above_unity_at_1_5() {
        let mut p = fresh();
        p.amen_volume = 1.0;
        apply_mod_target(&mut p, 60, 5.0);
        assert!((p.amen_volume - 1.5).abs() < 1e-6);
    }

    #[test]
    fn granular_volume_clamps_at_1_5() {
        let mut p = fresh();
        p.granular_volume = 1.0;
        apply_mod_target(&mut p, 63, 5.0);
        assert!((p.granular_volume - 1.5).abs() < 1e-6);
    }

    #[test]
    fn volume_303_clamps_at_1_5() {
        let mut p = fresh();
        p.volume_303 = 1.0;
        apply_mod_target(&mut p, 4, 5.0);
        assert!((p.volume_303 - 1.5).abs() < 1e-6);
    }
}

#[cfg(test)]
mod stereo_width_target {
    use super::*;

    #[test]
    fn stereo_width_uses_half_scaling_so_full_lfo_sweeps_unit_range() {
        // Centre the width then drive it to ±1 LFO; with 0.5 scaling the
        // result lands at the clamp boundaries.
        let mut p = fresh();
        p.stereo_width = 0.5;
        apply_mod_target(&mut p, 67, 1.0);
        assert!((p.stereo_width - 1.0).abs() < 1e-6);
        p.stereo_width = 0.5;
        apply_mod_target(&mut p, 67, -1.0);
        assert_eq!(p.stereo_width, 0.0);
    }
}

#[cfg(test)]
mod gabber_kick_targets {
    use super::*;

    #[test]
    fn gabber_pitch_decay_use_half_scaling() {
        let mut p = fresh();
        p.gabber_pitch = 0.0;
        p.gabber_decay = 0.0;
        apply_mod_target(&mut p, 68, 1.0);
        apply_mod_target(&mut p, 69, 1.0);
        assert!((p.gabber_pitch - 0.5).abs() < 1e-6);
        assert!((p.gabber_decay - 0.5).abs() < 1e-6);
    }

    #[test]
    fn gabber_pan_clamps_in_signed_unit_range() {
        let mut p = fresh();
        p.gabber_pan = 0.0;
        apply_mod_target(&mut p, 71, -5.0);
        assert_eq!(p.gabber_pan, -1.0);
        p.gabber_pan = 0.0;
        apply_mod_target(&mut p, 71, 5.0);
        assert_eq!(p.gabber_pan, 1.0);
    }

    #[test]
    fn gabber_clip_clamps_at_unity() {
        let mut p = fresh();
        p.gabber_clip = 0.5;
        apply_mod_target(&mut p, 70, 5.0);
        assert_eq!(p.gabber_clip, 1.0);
    }
}

#[cfg(test)]
mod target_isolation {
    use super::*;

    /// Exercise every opcode in the table; verify no opcode silently
    /// scribbles outside its declared field.  We do this by snapshotting
    /// the cutoff (opcode 1) field, then firing every OTHER opcode and
    /// asserting cutoff stayed put.
    #[test]
    fn no_other_opcode_touches_cutoff() {
        let baseline = fresh();
        let baseline_cutoff = baseline.cutoff;
        for op in 2..=71 {
            let mut p = fresh();
            apply_mod_target(&mut p, op, 0.7);
            assert_eq!(
                p.cutoff, baseline_cutoff,
                "opcode {op} unexpectedly changed cutoff"
            );
        }
    }

    /// Same idea for amen_gate — a special-clamp field whose neighbour
    /// opcodes shouldn't reach into it.
    #[test]
    fn no_other_opcode_touches_amen_gate() {
        let baseline = fresh();
        let baseline_gate = baseline.amen_gate;
        for op in 1..=71 {
            if op == 62 {
                continue;
            }
            let mut p = fresh();
            apply_mod_target(&mut p, op, 0.7);
            assert!(
                (p.amen_gate - baseline_gate).abs() < 1e-6,
                "opcode {op} unexpectedly changed amen_gate"
            );
        }
    }
}
