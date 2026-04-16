// ─── audio/dsp/mod_apply.rs ──────────────────────────────────────────────────
// Apply a modulation value to a target param, dispatched by u8 opcode.
// Used by both the LFO-slot loop and Free-EG path inside `process_block`,
// plus the new cable-routed mod system.
//
// The opcode → param table here MUST stay in lockstep with `lfo_target_to_u8`
// in dsp/params.rs.  Adding a new LfoTarget variant requires a new code there
// AND a new arm here.

use super::AudioParams;

/// Add `mod_val` (already scaled by depth) to the target param identified by
/// `target` (u8 opcode).  Each arm picks a sensible scaling / clamp range.
/// No-ops on opcode 0 (None) or any unknown code.
pub fn apply_mod_target(p: &mut AudioParams, target: u8, mod_val: f32) {
    match target {
        // ── Bass voice (legacy) ───────────────────────────────────────────
        1 => p.cutoff = (p.cutoff + mod_val).clamp(0.0, 1.0),
        2 => p.resonance = (p.resonance + mod_val).clamp(0.0, 1.0),
        3 => p.lfo_pitch_mod_st += mod_val * 12.0,
        4 => p.volume_303 = (p.volume_303 + mod_val).clamp(0.0, 1.5),
        // ── FX legacy ─────────────────────────────────────────────────────
        5 => p.reverb_mix = (p.reverb_mix + mod_val).clamp(0.0, 1.0),
        6 => p.delay_time = (p.delay_time + mod_val * 0.5).clamp(0.0, 1.0),
        7 => p.delay_feedback = (p.delay_feedback + mod_val * 0.5).clamp(0.0, 0.99),
        8 => p.chorus_mix = (p.chorus_mix + mod_val).clamp(0.0, 1.0),
        9 => p.chorus_rate = (p.chorus_rate + mod_val).clamp(0.0, 1.0),
        10 => p.kick808_pitch = (p.kick808_pitch + mod_val * 0.5).clamp(0.0, 1.0),
        11 => p.phaser_rate = (p.phaser_rate + mod_val).clamp(0.0, 1.0),
        12 => p.phaser_depth = (p.phaser_depth + mod_val).clamp(0.0, 1.0),
        13 => p.distortion_drive = (p.distortion_drive + mod_val * 0.5).clamp(0.0, 1.0),
        14 => p.master_volume = (p.master_volume + mod_val * 0.3).clamp(0.0, 1.5),
        15 => p.an1x_filter_cutoff = (p.an1x_filter_cutoff + mod_val).clamp(0.0, 1.0),
        16 => p.an1x_pitch_mod_st += mod_val * 12.0, // ±12 st at full depth
        // ── Pan family — bipolar -1..+1 ───────────────────────────────────
        17 => p.pan_303 = (p.pan_303 + mod_val).clamp(-1.0, 1.0),
        18 => p.pan_hoover = (p.pan_hoover + mod_val).clamp(-1.0, 1.0),
        19 => p.pan_noise = (p.pan_noise + mod_val).clamp(-1.0, 1.0),
        20 => p.pan_kick808 = (p.pan_kick808 + mod_val).clamp(-1.0, 1.0),
        21 => p.pan_snare808 = (p.pan_snare808 + mod_val).clamp(-1.0, 1.0),
        22 => p.pan_hihat808 = (p.pan_hihat808 + mod_val).clamp(-1.0, 1.0),
        23 => p.pan_kick909 = (p.pan_kick909 + mod_val).clamp(-1.0, 1.0),
        24 => p.pan_snare909 = (p.pan_snare909 + mod_val).clamp(-1.0, 1.0),
        25 => p.pan_hihat909 = (p.pan_hihat909 + mod_val).clamp(-1.0, 1.0),
        26 => p.pan_clap909 = (p.pan_clap909 + mod_val).clamp(-1.0, 1.0),
        27 => p.pan_an1x = (p.pan_an1x + mod_val).clamp(-1.0, 1.0),
        // ── FX expansion ──────────────────────────────────────────────────
        28 => p.reverb_size = (p.reverb_size + mod_val).clamp(0.0, 1.0),
        29 => p.reverb_damp = (p.reverb_damp + mod_val).clamp(0.0, 1.0),
        30 => p.delay_mix = (p.delay_mix + mod_val).clamp(0.0, 1.0),
        31 => p.chorus_depth = (p.chorus_depth + mod_val).clamp(0.0, 1.0),
        32 => p.phaser_mix = (p.phaser_mix + mod_val).clamp(0.0, 1.0),
        33 => p.waveshaper_drive = (p.waveshaper_drive + mod_val).clamp(0.0, 1.0),
        34 => p.waveshaper_mix = (p.waveshaper_mix + mod_val).clamp(0.0, 1.0),
        35 => p.distortion_mix = (p.distortion_mix + mod_val).clamp(0.0, 1.0),
        36 => p.bitcrush_bits = (p.bitcrush_bits + mod_val).clamp(0.0, 1.0),
        37 => p.bitcrush_rate = (p.bitcrush_rate + mod_val).clamp(0.0, 1.0),
        38 => p.bitcrush_mix = (p.bitcrush_mix + mod_val).clamp(0.0, 1.0),
        39 => p.ring_mod_freq = (p.ring_mod_freq + mod_val).clamp(0.0, 1.0),
        40 => p.ring_mod_mix = (p.ring_mod_mix + mod_val).clamp(0.0, 1.0),
        41 => p.eq_low_gain = (p.eq_low_gain + mod_val).clamp(-1.0, 1.0),
        42 => p.eq_mid_gain = (p.eq_mid_gain + mod_val).clamp(-1.0, 1.0),
        43 => p.eq_hi_gain = (p.eq_hi_gain + mod_val).clamp(-1.0, 1.0),
        44 => p.compressor_threshold = (p.compressor_threshold + mod_val).clamp(0.0, 1.0),
        45 => p.compressor_ratio = (p.compressor_ratio + mod_val).clamp(0.0, 1.0),
        46 => p.compressor_mix = (p.compressor_mix + mod_val).clamp(0.0, 1.0),
        47 => p.tape_drive = (p.tape_drive + mod_val).clamp(0.0, 1.0),
        48 => p.tape_mix = (p.tape_mix + mod_val).clamp(0.0, 1.0),
        49 => p.tape_flutter = (p.tape_flutter + mod_val).clamp(0.0, 1.0),
        50 => p.autotune_amount = (p.autotune_amount + mod_val).clamp(0.0, 1.0),
        51 => p.autotune_mix = (p.autotune_mix + mod_val).clamp(0.0, 1.0),
        // Drum extras + sampler/granular.
        52 => p.kick808_decay = (p.kick808_decay + mod_val * 0.5).clamp(0.0, 1.0),
        53 => p.snare808_tone = (p.snare808_tone + mod_val).clamp(0.0, 1.0),
        54 => p.snare808_decay = (p.snare808_decay + mod_val * 0.5).clamp(0.0, 1.0),
        55 => p.kick909_pitch = (p.kick909_pitch + mod_val * 0.5).clamp(0.0, 1.0),
        56 => p.kick909_decay = (p.kick909_decay + mod_val * 0.5).clamp(0.0, 1.0),
        57 => p.snare909_tone = (p.snare909_tone + mod_val).clamp(0.0, 1.0),
        58 => p.snare909_decay = (p.snare909_decay + mod_val * 0.5).clamp(0.0, 1.0),
        59 => p.clap909_decay = (p.clap909_decay + mod_val * 0.5).clamp(0.0, 1.0),
        60 => p.amen_volume = (p.amen_volume + mod_val).clamp(0.0, 1.5),
        61 => p.amen_start_offset = (p.amen_start_offset + mod_val * 0.5).clamp(0.0, 1.0),
        62 => p.amen_gate = (p.amen_gate + mod_val).clamp(0.05, 1.0),
        63 => p.granular_volume = (p.granular_volume + mod_val).clamp(0.0, 1.5),
        64 => p.granular_density = (p.granular_density + mod_val).clamp(0.0, 1.0),
        65 => p.granular_grain_size = (p.granular_grain_size + mod_val).clamp(0.0, 1.0),
        66 => p.granular_position = (p.granular_position + mod_val).clamp(0.0, 1.0),
        // Stereo width — 0=mono, 0.5=normal, 1=wide.  Full-depth LFO sweeps
        // the whole range; callers pick centre with the voice's own width knob.
        67 => p.stereo_width = (p.stereo_width + mod_val * 0.5).clamp(0.0, 1.0),
        _ => {}
    }
}
