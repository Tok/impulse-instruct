// ─── state/fx_defaults.rs ────────────────────────────────────────────────────
// Per-knob default-value functions used by `FxState`'s `#[serde(default =
// "...")]` attributes.  Extracted from `fx.rs` to keep that file under the
// 1000-line cap as the FX surface keeps growing.
//
// All functions are `pub(super)` so only `fx.rs` (and other state-module
// siblings) can reach them.  Mirrored 1:1 from the original definitions —
// no behaviour change.

pub(super) fn default_ms_unity() -> f32 {
    0.5
}

pub(super) fn default_fx_pan_rate() -> f32 {
    0.3
}

pub(super) fn default_flanger_rate() -> f32 {
    0.2
}

pub(super) fn default_flanger_depth() -> f32 {
    0.5
}

pub(super) fn default_flanger_feedback() -> f32 {
    0.5
}

pub(super) fn default_limiter_threshold() -> f32 {
    1.0 // 0 dB → no limiting until threshold knob is pulled down
}

pub(super) fn default_limiter_ceiling() -> f32 {
    1.0 // 0 dB ceiling
}

pub(super) fn default_limiter_release() -> f32 {
    0.3
}

pub(super) fn default_limiter_lookahead() -> f32 {
    0.4
}

pub(super) fn default_svf_cutoff() -> f32 {
    0.7 // ~3 kHz — open by default
}

pub(super) fn default_comb_pitch() -> f32 {
    0.4 // ~250 Hz
}

pub(super) fn default_tilt_pivot() -> f32 {
    0.5 // 1 kHz log-mapped
}

pub(super) fn default_exciter_freq() -> f32 {
    0.3 // ~2 kHz HP
}

pub(super) fn default_multitap_time() -> f32 {
    0.3 // ~300 ms
}

pub(super) fn default_multitap_spread() -> f32 {
    0.7 // mostly evenly distributed
}

pub(super) fn default_revdelay_time() -> f32 {
    0.25 // ~500 ms segment
}

pub(super) fn default_tapestop_time() -> f32 {
    0.3 // ~600 ms scratch tail
}

pub(super) fn default_stutter_rate() -> f32 {
    0.5 // 1/16 (third quartile)
}

pub(super) fn default_stutter_slice() -> f32 {
    0.5
}

pub(super) fn default_conv_reverb_size() -> f32 {
    1.0
}

pub(super) fn default_conv_reverb_width() -> f32 {
    1.0
}

pub(super) fn default_gate_threshold() -> f32 {
    0.5 // ~−30 dBFS — sits between the noise floor and a typical signal.
}

pub(super) fn default_gate_attack() -> f32 {
    0.05 // ~3 ms — fast enough to track a kick, soft enough to avoid clicks.
}

pub(super) fn default_gate_release() -> f32 {
    0.4 // ~200 ms — comfortable kick-ducks-pad release.
}

pub(super) fn default_gate_depth() -> f32 {
    0.7 // moderate ducking by default — full mute is harsh.
}

pub(super) fn default_vocoder_bands() -> f32 {
    1.0 // all 16 bands active.
}

pub(super) fn default_vocoder_sense() -> f32 {
    0.5 // mid-range detector gain.
}

pub(super) fn default_widen_haas() -> f32 {
    0.4 // ~12 ms — comfortable Haas window without flam at the kick.
}

pub(super) fn default_freq_shift_amount() -> f32 {
    0.5 // 0 Hz centre — engaging the FX with default knobs is no-op.
}

pub(super) fn default_dj_filter_morph() -> f32 {
    0.5 // BP at the crossover — neutral position before the user sweeps.
}

pub(super) fn default_dj_filter_resonance() -> f32 {
    0.4 // Audible peak at the BP crossover without screaming.
}

pub(super) fn default_tremolo_rate() -> f32 {
    0.4 // ~3 Hz — classic "slow tremolo" guitar-amp feel.
}

pub(super) fn default_tremolo_depth() -> f32 {
    0.6 // Audible swell on first engagement without going to silence.
}

pub(super) fn default_vibrato_rate() -> f32 {
    0.45 // ~5 Hz — natural-sounding vocal / string vibrato rate.
}

pub(super) fn default_vibrato_depth() -> f32 {
    0.5 // ~25 cents peak swing — audible without sounding seasick.
}

pub(super) fn default_iso_pass() -> f32 {
    1.0 // All bands pass at full level — engaging the ISO via mix=>0 is the only audible change until the user kills a band.
}

pub(super) fn default_deess_freq() -> f32 {
    0.5 // ~6 kHz — between most "S" and "SH" centres for vocals.
}

pub(super) fn default_deess_threshold() -> f32 {
    0.5 // Linear amplitude — moderately permissive starting point.
}

pub(super) fn default_deess_amount() -> f32 {
    0.7 // Audible ducking on first engagement — users dial back if too aggressive.
}

pub(super) fn default_resbank_root() -> f32 {
    0.5 // ~MIDI 60 (middle C) — neutral starting pitch.
}

pub(super) fn default_resbank_resonance() -> f32 {
    0.6 // Singing-but-not-screaming Q on first engagement.
}

pub(super) fn default_tape_echo_time() -> f32 {
    0.4 // ~250 ms — classic dub slap-back.
}

pub(super) fn default_tape_echo_feedback() -> f32 {
    0.4 // Three or four audible repeats — enough to read as an echo without piling up.
}

pub(super) fn default_tape_echo_age() -> f32 {
    0.5 // Audible analog character on first engagement.
}

pub(super) fn default_mb_thresh() -> f32 {
    1.0 // No compression until the user pulls a band threshold below the signal peak.
}
