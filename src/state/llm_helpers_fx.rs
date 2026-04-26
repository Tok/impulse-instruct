// ─── state/llm_helpers_fx.rs ─────────────────────────────────────────────────
// FX-specific apply helpers — extracted from `llm_helpers.rs` to keep
// that file under the 1000-line cap.  Both items are pure-ish in the
// sense the coding guide cares about: they take owned `&mut AppState`
// references via the apply path's "build a new state" pattern,
// don't lock anything, don't talk to other threads.  The `fx_field_mut`
// helper hands out a mutable ref to a single `f32` knob keyed by name —
// used by the XY-pad apply loop too.
//
// Visibility is `pub(super)` so only sibling files in `state/` can
// reach in; the public face stays the existing
// `crate::state::apply_llm_update` entrypoint.

use std::collections::HashSet;

use super::AppState;
use super::llm_helpers::unlocked_f32;

/// Apply FX fields from an LLM JSON update object.
pub(super) fn apply_fx_update(
    s: &mut AppState,
    fx: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    macro_rules! u {
        ($field:expr, $key:literal, $path:literal) => {
            $field = unlocked_f32($field, fx, $key, $path, locked);
        };
    }
    u!(s.fx.reverb_size, "reverb_size", "fx.reverb_size");
    u!(s.fx.reverb_damp, "reverb_damp", "fx.reverb_damp");
    u!(s.fx.reverb_mix, "reverb_mix", "fx.reverb_mix");
    u!(
        s.fx.reverb_gate_time,
        "reverb_gate_time",
        "fx.reverb_gate_time"
    );
    if !locked.contains("fx.reverb_freeze")
        && let Some(v) = fx.get("reverb_freeze").and_then(|v| v.as_bool())
    {
        s.fx.reverb_freeze = v;
    }
    // master_pitch_st: -12..+12 stored raw; unlocked_f32 clamp is applied via min/max
    if let Some(v) = fx.get("master_pitch_st").and_then(|v| v.as_f64()) {
        let path = "fx.master_pitch_st";
        if !locked.contains(path) {
            s.fx.master_pitch_st = (v as f32).clamp(-12.0, 12.0);
        }
    }
    u!(s.fx.delay_time, "delay_time", "fx.delay_time");
    u!(s.fx.delay_feedback, "delay_feedback", "fx.delay_feedback");
    u!(s.fx.delay_mix, "delay_mix", "fx.delay_mix");
    u!(
        s.fx.delay_wow_flutter,
        "delay_wow_flutter",
        "fx.delay_wow_flutter"
    );
    u!(
        s.fx.delay_saturation,
        "delay_saturation",
        "fx.delay_saturation"
    );
    if !locked.contains("fx.delay_freeze")
        && let Some(v) = fx.get("delay_freeze").and_then(|v| v.as_bool())
    {
        s.fx.delay_freeze = v;
    }
    u!(s.fx.delay_hpf, "delay_hpf", "fx.delay_hpf");
    u!(s.fx.delay_lpf, "delay_lpf", "fx.delay_lpf");
    u!(
        s.fx.distortion_drive,
        "distortion_drive",
        "fx.distortion_drive"
    );
    u!(s.fx.distortion_mix, "distortion_mix", "fx.distortion_mix");
    u!(s.fx.bitcrush_bits, "bitcrush_bits", "fx.bitcrush_bits");
    u!(s.fx.bitcrush_rate, "bitcrush_rate", "fx.bitcrush_rate");
    u!(s.fx.bitcrush_mix, "bitcrush_mix", "fx.bitcrush_mix");
    u!(s.fx.chorus_rate, "chorus_rate", "fx.chorus_rate");
    u!(s.fx.chorus_depth, "chorus_depth", "fx.chorus_depth");
    u!(s.fx.chorus_mix, "chorus_mix", "fx.chorus_mix");
    u!(s.fx.phaser_rate, "phaser_rate", "fx.phaser_rate");
    u!(s.fx.phaser_depth, "phaser_depth", "fx.phaser_depth");
    u!(s.fx.phaser_mix, "phaser_mix", "fx.phaser_mix");
    u!(s.fx.flanger_rate, "flanger_rate", "fx.flanger_rate");
    u!(s.fx.flanger_depth, "flanger_depth", "fx.flanger_depth");
    u!(
        s.fx.flanger_feedback,
        "flanger_feedback",
        "fx.flanger_feedback"
    );
    u!(s.fx.flanger_mix, "flanger_mix", "fx.flanger_mix");
    u!(
        s.fx.limiter_threshold,
        "limiter_threshold",
        "fx.limiter_threshold"
    );
    u!(
        s.fx.limiter_ceiling,
        "limiter_ceiling",
        "fx.limiter_ceiling"
    );
    u!(
        s.fx.limiter_release,
        "limiter_release",
        "fx.limiter_release"
    );
    u!(
        s.fx.limiter_lookahead,
        "limiter_lookahead",
        "fx.limiter_lookahead"
    );
    u!(s.fx.svf_cutoff, "svf_cutoff", "fx.svf_cutoff");
    u!(s.fx.svf_resonance, "svf_resonance", "fx.svf_resonance");
    u!(s.fx.svf_drive, "svf_drive", "fx.svf_drive");
    u!(s.fx.svf_mix, "svf_mix", "fx.svf_mix");
    u!(s.fx.comb_pitch, "comb_pitch", "fx.comb_pitch");
    u!(s.fx.comb_feedback, "comb_feedback", "fx.comb_feedback");
    u!(s.fx.comb_damp, "comb_damp", "fx.comb_damp");
    u!(s.fx.comb_mix, "comb_mix", "fx.comb_mix");
    u!(s.fx.tilt_tilt, "tilt_tilt", "fx.tilt_tilt");
    u!(s.fx.tilt_pivot, "tilt_pivot", "fx.tilt_pivot");
    u!(s.fx.tilt_mix, "tilt_mix", "fx.tilt_mix");
    u!(
        s.fx.transient_attack,
        "transient_attack",
        "fx.transient_attack"
    );
    u!(
        s.fx.transient_sustain,
        "transient_sustain",
        "fx.transient_sustain"
    );
    u!(s.fx.transient_mix, "transient_mix", "fx.transient_mix");
    u!(s.fx.exciter_amount, "exciter_amount", "fx.exciter_amount");
    u!(s.fx.exciter_freq, "exciter_freq", "fx.exciter_freq");
    u!(s.fx.exciter_mix, "exciter_mix", "fx.exciter_mix");
    u!(s.fx.multitap_time, "multitap_time", "fx.multitap_time");
    u!(
        s.fx.multitap_spread,
        "multitap_spread",
        "fx.multitap_spread"
    );
    u!(
        s.fx.multitap_feedback,
        "multitap_feedback",
        "fx.multitap_feedback"
    );
    u!(s.fx.multitap_mix, "multitap_mix", "fx.multitap_mix");
    u!(s.fx.revdelay_time, "revdelay_time", "fx.revdelay_time");
    u!(
        s.fx.revdelay_feedback,
        "revdelay_feedback",
        "fx.revdelay_feedback"
    );
    u!(s.fx.revdelay_mix, "revdelay_mix", "fx.revdelay_mix");
    u!(s.fx.tapestop_mix, "tapestop_mix", "fx.tapestop_mix");
    u!(s.fx.tapestop_time, "tapestop_time", "fx.tapestop_time");
    u!(s.fx.stutter_rate, "stutter_rate", "fx.stutter_rate");
    u!(s.fx.stutter_slice, "stutter_slice", "fx.stutter_slice");
    u!(s.fx.stutter_mix, "stutter_mix", "fx.stutter_mix");
    u!(s.fx.freeze_mix, "freeze_mix", "fx.freeze_mix");
    if !locked.contains("fx.conv_reverb_cabinet")
        && let Some(v) = fx.get("conv_reverb_cabinet").and_then(|v| v.as_bool())
    {
        s.fx.conv_reverb_cabinet = v;
    }
    u!(
        s.fx.waveshaper_drive,
        "waveshaper_drive",
        "fx.waveshaper_drive"
    );
    u!(s.fx.waveshaper_mix, "waveshaper_mix", "fx.waveshaper_mix");
    u!(s.fx.ring_mod_freq, "ring_mod_freq", "fx.ring_mod_freq");
    u!(s.fx.ring_mod_mix, "ring_mod_mix", "fx.ring_mod_mix");
    u!(s.fx.eq_low_gain, "eq_low_gain", "fx.eq_low_gain");
    u!(s.fx.eq_mid_gain, "eq_mid_gain", "fx.eq_mid_gain");
    u!(s.fx.eq_hi_gain, "eq_hi_gain", "fx.eq_hi_gain");
    u!(
        s.fx.compressor_threshold,
        "compressor_threshold",
        "fx.compressor_threshold"
    );
    u!(
        s.fx.compressor_ratio,
        "compressor_ratio",
        "fx.compressor_ratio"
    );
    u!(s.fx.compressor_mix, "compressor_mix", "fx.compressor_mix");
    u!(
        s.fx.compressor_multiband,
        "compressor_multiband",
        "fx.compressor_multiband"
    );
    if !locked.contains("fx.compressor_reverse")
        && let Some(v) = fx.get("compressor_reverse").and_then(|v| v.as_bool())
    {
        s.fx.compressor_reverse = v;
    }
    if !locked.contains("fx.compressor_sidechain")
        && let Some(v) = fx.get("compressor_sidechain").and_then(|v| v.as_bool())
    {
        s.fx.compressor_sidechain = v;
    }
    u!(s.fx.gate_threshold, "gate_threshold", "fx.gate_threshold");
    u!(s.fx.gate_attack, "gate_attack", "fx.gate_attack");
    u!(s.fx.gate_release, "gate_release", "fx.gate_release");
    u!(s.fx.gate_depth, "gate_depth", "fx.gate_depth");
    u!(s.fx.gate_mix, "gate_mix", "fx.gate_mix");
    u!(s.fx.vocoder_bands, "vocoder_bands", "fx.vocoder_bands");
    u!(
        s.fx.vocoder_carrier_mix,
        "vocoder_carrier_mix",
        "fx.vocoder_carrier_mix"
    );
    u!(s.fx.vocoder_sense, "vocoder_sense", "fx.vocoder_sense");
    u!(s.fx.vocoder_mix, "vocoder_mix", "fx.vocoder_mix");
    u!(s.fx.stereo_width, "stereo_width", "fx.stereo_width");
    if !locked.contains("fx.tuning")
        && let Some(v) = fx.get("tuning").and_then(|v| v.as_u64())
    {
        s.fx.tuning = (v as u8).min(3);
    }
    u!(s.fx.tape_drive, "tape_drive", "fx.tape_drive");
    u!(s.fx.tape_mix, "tape_mix", "fx.tape_mix");
    u!(s.fx.tape_flutter, "tape_flutter", "fx.tape_flutter");
    u!(
        s.fx.autotune_amount,
        "autotune_amount",
        "fx.autotune_amount"
    );
    u!(s.fx.autotune_mix, "autotune_mix", "fx.autotune_mix");
    u!(s.fx.fx_pan_pos, "fx_pan_pos", "fx.fx_pan_pos");
    u!(s.fx.fx_pan_width, "fx_pan_width", "fx.fx_pan_width");
    u!(s.fx.fx_pan_rate, "fx_pan_rate", "fx.fx_pan_rate");
    u!(s.fx.widen_haas, "widen_haas", "fx.widen_haas");
    u!(s.fx.widen_side, "widen_side", "fx.widen_side");
    u!(s.fx.widen_mix, "widen_mix", "fx.widen_mix");
    u!(
        s.fx.freq_shift_amount,
        "freq_shift_amount",
        "fx.freq_shift_amount"
    );
    u!(
        s.fx.freq_shift_feedback,
        "freq_shift_feedback",
        "fx.freq_shift_feedback"
    );
    u!(s.fx.freq_shift_mix, "freq_shift_mix", "fx.freq_shift_mix");
    u!(
        s.fx.dj_filter_morph,
        "dj_filter_morph",
        "fx.dj_filter_morph"
    );
    u!(
        s.fx.dj_filter_resonance,
        "dj_filter_resonance",
        "fx.dj_filter_resonance"
    );
    u!(s.fx.dj_filter_mix, "dj_filter_mix", "fx.dj_filter_mix");
    u!(s.fx.vinyl_noise, "vinyl_noise", "fx.vinyl_noise");
    u!(s.fx.vinyl_wear, "vinyl_wear", "fx.vinyl_wear");
    u!(s.fx.vinyl_mix, "vinyl_mix", "fx.vinyl_mix");
    u!(s.fx.tremolo_rate, "tremolo_rate", "fx.tremolo_rate");
    u!(s.fx.tremolo_depth, "tremolo_depth", "fx.tremolo_depth");
    u!(s.fx.tremolo_shape, "tremolo_shape", "fx.tremolo_shape");
    u!(s.fx.tremolo_mix, "tremolo_mix", "fx.tremolo_mix");
    if !locked.contains("fx.param_eq_ms_mode")
        && let Some(v) = fx.get("param_eq_ms_mode").and_then(|v| v.as_bool())
    {
        s.fx.param_eq_ms_mode = v;
    }
    u!(
        s.fx.conv_reverb_mix,
        "conv_reverb_mix",
        "fx.conv_reverb_mix"
    );
    u!(
        s.fx.conv_reverb_size,
        "conv_reverb_size",
        "fx.conv_reverb_size"
    );
    u!(
        s.fx.conv_reverb_predelay,
        "conv_reverb_predelay",
        "fx.conv_reverb_predelay"
    );
    u!(
        s.fx.conv_reverb_damp,
        "conv_reverb_damp",
        "fx.conv_reverb_damp"
    );
    u!(
        s.fx.conv_reverb_lowcut,
        "conv_reverb_lowcut",
        "fx.conv_reverb_lowcut"
    );
    u!(
        s.fx.conv_reverb_width,
        "conv_reverb_width",
        "fx.conv_reverb_width"
    );
    if !locked.contains("fx.conv_reverb_reverse")
        && let Some(v) = fx.get("conv_reverb_reverse").and_then(|v| v.as_bool())
    {
        s.fx.conv_reverb_reverse = v;
    }

    // ── Parametric EQ bands ──────────────────────────────────────────────
    // `fx.param_eq_bands` is a positional sparse array — entries may be
    // null to skip that band, so the LLM can edit a single band without
    // re-emitting the whole 8-band set.  Each per-band field respects an
    // `fx.param_eq_bands.N.<field>` lock path, mirroring how
    // `bass_voices.N` edits gate per-field locks.
    if !locked.contains("fx.param_eq_bands")
        && let Some(arr) = fx.get("param_eq_bands").and_then(|v| v.as_array())
    {
        for (i, entry) in arr.iter().enumerate().take(s.fx.param_eq_bands.len()) {
            let Some(obj) = entry.as_object() else {
                continue;
            };
            let band = &mut s.fx.param_eq_bands[i];
            let lock_kind = format!("fx.param_eq_bands.{}.kind", i);
            let lock_freq = format!("fx.param_eq_bands.{}.freq", i);
            let lock_gain = format!("fx.param_eq_bands.{}.gain", i);
            let lock_q = format!("fx.param_eq_bands.{}.q", i);
            let lock_enabled = format!("fx.param_eq_bands.{}.enabled", i);
            if !locked.contains(&lock_kind)
                && let Some(k) = obj.get("kind").and_then(|v| v.as_u64())
            {
                band.kind = super::fx::ParamEqBandKind::from_u8(k as u8);
            }
            if !locked.contains(&lock_freq)
                && let Some(f) = obj.get("freq").and_then(|v| v.as_f64())
            {
                band.freq_hz = (f as f32).clamp(20.0, 20_000.0);
            }
            if !locked.contains(&lock_gain)
                && let Some(g) = obj.get("gain").and_then(|v| v.as_f64())
            {
                band.gain_db = (g as f32).clamp(-18.0, 18.0);
            }
            if !locked.contains(&lock_q)
                && let Some(q) = obj.get("q").and_then(|v| v.as_f64())
            {
                band.q = (q as f32).clamp(0.1, 10.0);
            }
            if !locked.contains(&lock_enabled)
                && let Some(e) = obj.get("enabled").and_then(|v| v.as_bool())
            {
                band.enabled = e;
            }
        }
    }

    // ── Pitch shifter (standalone bidirectional shifter, distinct
    //     from Autotune which is upward-only) ─────────────────────────
    if !locked.contains("fx.pitch_shift_semi")
        && let Some(v) = fx.get("pitch_shift_semi").and_then(|v| v.as_f64())
    {
        s.fx.pitch_shift_semi = (v as f32).clamp(-24.0, 24.0);
    }
    if !locked.contains("fx.pitch_shift_fine")
        && let Some(v) = fx.get("pitch_shift_fine").and_then(|v| v.as_f64())
    {
        s.fx.pitch_shift_fine = (v as f32).clamp(-100.0, 100.0);
    }
    u!(
        s.fx.pitch_shift_mix,
        "pitch_shift_mix",
        "fx.pitch_shift_mix"
    );
    u!(
        s.fx.pitch_shift_fbk,
        "pitch_shift_fbk",
        "fx.pitch_shift_fbk"
    );

    // ── Mid/side master knobs ───────────────────────────────────────────
    u!(s.fx.ms_mid_gain, "ms_mid_gain", "fx.ms_mid_gain");
    u!(s.fx.ms_mid_tilt, "ms_mid_tilt", "fx.ms_mid_tilt");
    u!(s.fx.ms_mid_sat, "ms_mid_sat", "fx.ms_mid_sat");
    u!(s.fx.ms_side_gain, "ms_side_gain", "fx.ms_side_gain");
    u!(s.fx.ms_side_tilt, "ms_side_tilt", "fx.ms_side_tilt");
    u!(s.fx.ms_side_sat, "ms_side_sat", "fx.ms_side_sat");

    u!(s.fx.master_volume, "master_volume", "fx.master_volume");
    u!(
        s.fx.xmod_bass_to_an1x_pitch,
        "xmod_bass_to_an1x_pitch",
        "fx.xmod_bass_to_an1x_pitch"
    );
    u!(
        s.fx.xmod_noise_to_filter,
        "xmod_noise_to_filter",
        "fx.xmod_noise_to_filter"
    );
    u!(
        s.fx.sidechain_amount,
        "sidechain_amount",
        "fx.sidechain_amount"
    );
    u!(
        s.fx.sidechain_attack,
        "sidechain_attack",
        "fx.sidechain_attack"
    );
    u!(
        s.fx.sidechain_release,
        "sidechain_release",
        "fx.sidechain_release"
    );

    // ── XY pad first-class paths ─────────────────────────────────────────
    // Each entry is `(xy_key, field_a, field_b, min, max)` — writing
    // `fx.<xy_key>: [x, y]` sets `field_a` to x and `field_b` to y,
    // respecting per-field locks *and* the `fx.<xy_key>` lock path.
    // Maps the canonical Pair-0 of each FX pad; Pair 1 / Pair 2 stay
    // reachable via the individual knob paths.
    type XyMap = (&'static str, &'static str, &'static str, f32, f32);
    const XY_PAIRS: &[XyMap] = &[
        ("reverb_xy", "reverb_size", "reverb_damp", 0.0, 1.0),
        ("delay_xy", "delay_time", "delay_feedback", 0.0, 1.0),
        ("chorus_xy", "chorus_rate", "chorus_depth", 0.0, 1.0),
        ("phaser_xy", "phaser_rate", "phaser_depth", 0.0, 1.0),
        ("flanger_xy", "flanger_rate", "flanger_depth", 0.0, 1.0),
        (
            "limiter_xy",
            "limiter_threshold",
            "limiter_ceiling",
            0.0,
            1.0,
        ),
        ("svf_xy", "svf_cutoff", "svf_resonance", 0.0, 1.0),
        ("comb_xy", "comb_pitch", "comb_feedback", 0.0, 1.0),
        ("tilt_xy", "tilt_tilt", "tilt_pivot", 0.0, 1.0),
        (
            "transient_xy",
            "transient_attack",
            "transient_sustain",
            0.0,
            1.0,
        ),
        ("exciter_xy", "exciter_amount", "exciter_freq", 0.0, 1.0),
        ("multitap_xy", "multitap_time", "multitap_spread", 0.0, 1.0),
        (
            "revdelay_xy",
            "revdelay_time",
            "revdelay_feedback",
            0.0,
            1.0,
        ),
        ("stutter_xy", "stutter_rate", "stutter_slice", 0.0, 1.0),
        ("ring_mod_xy", "ring_mod_freq", "ring_mod_mix", 0.0, 1.0),
        (
            "waveshaper_xy",
            "waveshaper_drive",
            "waveshaper_mix",
            0.0,
            1.0,
        ),
        ("bitcrush_xy", "bitcrush_bits", "bitcrush_rate", 0.0, 1.0),
        ("eq_xy", "eq_low_gain", "eq_mid_gain", -1.0, 1.0),
        (
            "compressor_xy",
            "compressor_threshold",
            "compressor_ratio",
            0.0,
            1.0,
        ),
        ("tape_xy", "tape_drive", "tape_flutter", 0.0, 1.0),
        (
            "distortion_xy",
            "distortion_drive",
            "distortion_mix",
            0.0,
            1.0,
        ),
        ("autotune_xy", "autotune_amount", "autotune_mix", 0.0, 1.0),
        ("fx_pan_xy", "fx_pan_pos", "fx_pan_width", 0.0, 1.0),
        ("gate_xy", "gate_threshold", "gate_depth", 0.0, 1.0),
        (
            "vocoder_xy",
            "vocoder_bands",
            "vocoder_carrier_mix",
            0.0,
            1.0,
        ),
        ("widen_xy", "widen_haas", "widen_side", 0.0, 1.0),
        (
            "freq_shift_xy",
            "freq_shift_amount",
            "freq_shift_feedback",
            0.0,
            1.0,
        ),
    ];
    for (xy_key, field_a, field_b, min, max) in XY_PAIRS {
        let Some(arr) = fx.get(*xy_key).and_then(|v| v.as_array()) else {
            continue;
        };
        if arr.len() != 2 {
            continue;
        }
        let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) else {
            continue;
        };
        let xy_path = format!("fx.{}", xy_key);
        if locked.contains(&xy_path) {
            continue;
        }
        let path_a = format!("fx.{}", field_a);
        let path_b = format!("fx.{}", field_b);
        let x = (x as f32).clamp(*min, *max);
        let y = (y as f32).clamp(*min, *max);
        if !locked.contains(&path_a)
            && let Some(dst) = fx_field_mut(&mut s.fx, field_a)
        {
            *dst = x;
        }
        if !locked.contains(&path_b)
            && let Some(dst) = fx_field_mut(&mut s.fx, field_b)
        {
            *dst = y;
        }
    }
}

/// Resolve an `FxState` field name to a mutable reference to that field.
/// Returns `None` for fields that aren't scalar `f32` knobs (booleans,
/// enum-ish `u8` selectors).  Kept in one place so the XY-pad apply loop
/// doesn't need to duplicate the big match.
fn fx_field_mut<'a>(fx: &'a mut super::FxState, key: &str) -> Option<&'a mut f32> {
    Some(match key {
        "reverb_size" => &mut fx.reverb_size,
        "reverb_damp" => &mut fx.reverb_damp,
        "reverb_mix" => &mut fx.reverb_mix,
        "delay_time" => &mut fx.delay_time,
        "delay_feedback" => &mut fx.delay_feedback,
        "delay_mix" => &mut fx.delay_mix,
        "chorus_rate" => &mut fx.chorus_rate,
        "chorus_depth" => &mut fx.chorus_depth,
        "chorus_mix" => &mut fx.chorus_mix,
        "phaser_rate" => &mut fx.phaser_rate,
        "phaser_depth" => &mut fx.phaser_depth,
        "phaser_mix" => &mut fx.phaser_mix,
        "flanger_rate" => &mut fx.flanger_rate,
        "flanger_depth" => &mut fx.flanger_depth,
        "flanger_feedback" => &mut fx.flanger_feedback,
        "flanger_mix" => &mut fx.flanger_mix,
        "limiter_threshold" => &mut fx.limiter_threshold,
        "limiter_ceiling" => &mut fx.limiter_ceiling,
        "limiter_release" => &mut fx.limiter_release,
        "limiter_lookahead" => &mut fx.limiter_lookahead,
        "svf_cutoff" => &mut fx.svf_cutoff,
        "svf_resonance" => &mut fx.svf_resonance,
        "svf_drive" => &mut fx.svf_drive,
        "svf_mix" => &mut fx.svf_mix,
        "comb_pitch" => &mut fx.comb_pitch,
        "comb_feedback" => &mut fx.comb_feedback,
        "comb_damp" => &mut fx.comb_damp,
        "comb_mix" => &mut fx.comb_mix,
        "tilt_tilt" => &mut fx.tilt_tilt,
        "tilt_pivot" => &mut fx.tilt_pivot,
        "tilt_mix" => &mut fx.tilt_mix,
        "transient_attack" => &mut fx.transient_attack,
        "transient_sustain" => &mut fx.transient_sustain,
        "transient_mix" => &mut fx.transient_mix,
        "exciter_amount" => &mut fx.exciter_amount,
        "exciter_freq" => &mut fx.exciter_freq,
        "exciter_mix" => &mut fx.exciter_mix,
        "multitap_time" => &mut fx.multitap_time,
        "multitap_spread" => &mut fx.multitap_spread,
        "multitap_feedback" => &mut fx.multitap_feedback,
        "multitap_mix" => &mut fx.multitap_mix,
        "revdelay_time" => &mut fx.revdelay_time,
        "revdelay_feedback" => &mut fx.revdelay_feedback,
        "revdelay_mix" => &mut fx.revdelay_mix,
        "tapestop_mix" => &mut fx.tapestop_mix,
        "tapestop_time" => &mut fx.tapestop_time,
        "stutter_rate" => &mut fx.stutter_rate,
        "stutter_slice" => &mut fx.stutter_slice,
        "stutter_mix" => &mut fx.stutter_mix,
        "freeze_mix" => &mut fx.freeze_mix,
        "ring_mod_freq" => &mut fx.ring_mod_freq,
        "ring_mod_mix" => &mut fx.ring_mod_mix,
        "waveshaper_drive" => &mut fx.waveshaper_drive,
        "waveshaper_mix" => &mut fx.waveshaper_mix,
        "bitcrush_bits" => &mut fx.bitcrush_bits,
        "bitcrush_rate" => &mut fx.bitcrush_rate,
        "bitcrush_mix" => &mut fx.bitcrush_mix,
        "eq_low_gain" => &mut fx.eq_low_gain,
        "eq_mid_gain" => &mut fx.eq_mid_gain,
        "eq_hi_gain" => &mut fx.eq_hi_gain,
        "compressor_threshold" => &mut fx.compressor_threshold,
        "compressor_ratio" => &mut fx.compressor_ratio,
        "compressor_mix" => &mut fx.compressor_mix,
        "gate_threshold" => &mut fx.gate_threshold,
        "gate_attack" => &mut fx.gate_attack,
        "gate_release" => &mut fx.gate_release,
        "gate_depth" => &mut fx.gate_depth,
        "gate_mix" => &mut fx.gate_mix,
        "vocoder_bands" => &mut fx.vocoder_bands,
        "vocoder_carrier_mix" => &mut fx.vocoder_carrier_mix,
        "vocoder_sense" => &mut fx.vocoder_sense,
        "vocoder_mix" => &mut fx.vocoder_mix,
        "widen_haas" => &mut fx.widen_haas,
        "widen_side" => &mut fx.widen_side,
        "widen_mix" => &mut fx.widen_mix,
        "freq_shift_amount" => &mut fx.freq_shift_amount,
        "freq_shift_feedback" => &mut fx.freq_shift_feedback,
        "freq_shift_mix" => &mut fx.freq_shift_mix,
        "tape_drive" => &mut fx.tape_drive,
        "tape_mix" => &mut fx.tape_mix,
        "tape_flutter" => &mut fx.tape_flutter,
        "distortion_drive" => &mut fx.distortion_drive,
        "distortion_mix" => &mut fx.distortion_mix,
        "autotune_amount" => &mut fx.autotune_amount,
        "autotune_mix" => &mut fx.autotune_mix,
        "fx_pan_pos" => &mut fx.fx_pan_pos,
        "fx_pan_width" => &mut fx.fx_pan_width,
        "fx_pan_rate" => &mut fx.fx_pan_rate,
        "conv_reverb_mix" => &mut fx.conv_reverb_mix,
        "conv_reverb_size" => &mut fx.conv_reverb_size,
        "conv_reverb_predelay" => &mut fx.conv_reverb_predelay,
        "conv_reverb_damp" => &mut fx.conv_reverb_damp,
        "conv_reverb_lowcut" => &mut fx.conv_reverb_lowcut,
        "conv_reverb_width" => &mut fx.conv_reverb_width,
        "pitch_shift_semi" => &mut fx.pitch_shift_semi,
        "pitch_shift_fine" => &mut fx.pitch_shift_fine,
        "pitch_shift_mix" => &mut fx.pitch_shift_mix,
        "pitch_shift_fbk" => &mut fx.pitch_shift_fbk,
        "ms_mid_gain" => &mut fx.ms_mid_gain,
        "ms_mid_tilt" => &mut fx.ms_mid_tilt,
        "ms_mid_sat" => &mut fx.ms_mid_sat,
        "ms_side_gain" => &mut fx.ms_side_gain,
        "ms_side_tilt" => &mut fx.ms_side_tilt,
        "ms_side_sat" => &mut fx.ms_side_sat,
        _ => return None,
    })
}
