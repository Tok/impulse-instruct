// ─── audio/dsp/gabber_kick.rs ─────────────────────────────────────────────────
// Dedicated gabber-kick voice.  A sine oscillator with an extreme pitch
// envelope, a hard clipper followed by a tanh saturator (for the screaming
// harmonics gabber is known for), and a separate transient "click" layer —
// a filtered noise burst on attack that makes the kick cut through the mix
// without being crushed by the distortion.

use super::AudioParams;
use super::voices::{Envelope, NoiseGen};

#[derive(Clone)]
pub(super) struct GabberKick {
    phase: f32,
    pitch_env: Envelope,
    amp_env: Envelope,
    transient_env: Envelope,
    noise_gen: NoiseGen,
    /// Simple 1-pole high-pass state for the transient click layer.
    click_hp_z: f32,
}

impl GabberKick {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            phase: 0.0,
            pitch_env: Envelope::default(),
            amp_env: Envelope::default(),
            transient_env: Envelope::default(),
            noise_gen: NoiseGen::new(seed),
            click_hp_z: 0.0,
        }
    }

    pub(super) fn trigger(&mut self) {
        self.pitch_env.trigger();
        self.amp_env.trigger();
        self.transient_env.trigger();
        self.phase = 0.0;
        self.click_hp_z = 0.0;
    }

    pub(super) fn process(&mut self, p: &AudioParams, sr: f32) -> f32 {
        // 50–110 Hz base — higher than 808 (40–80) for sharper gabber tone.
        let base_hz = 50.0 + p.gabber_pitch * 60.0;
        let amp_coeff = (-1.0 / (sr * (p.gabber_decay * 1.4 + 0.1))).exp();
        let pitch_decay_time = 0.005 + p.gabber_pitch_env_time * 0.055; // 5–60 ms
        let pitch_coeff = (-1.0 / (sr * pitch_decay_time)).exp();
        // Transient click: 8 ms decay, short filtered noise burst.
        let trans_coeff = (-1.0 / (sr * 0.008)).exp();

        let amp = self.amp_env.tick(amp_coeff);
        let pitch_mod = self.pitch_env.tick(pitch_coeff);
        let trans_amp = self.transient_env.tick(trans_coeff);

        // Pitch envelope: 1× to 13× base freq (steeper than 808's 10×).
        let freq = base_hz + pitch_mod * base_hz * (1.0 + p.gabber_pitch_env_depth * 12.0);
        self.phase += freq / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let sine = (self.phase * std::f32::consts::TAU).sin();
        // High-passed noise click so the transient sits on top of the sine
        // without masking the low-end.  One-pole HPF with ~0.15× cutoff coeff.
        let raw_noise = self.noise_gen.next();
        let hpf = raw_noise - self.click_hp_z;
        self.click_hp_z = self.click_hp_z * 0.85 + raw_noise * 0.15;
        let click = hpf * trans_amp * p.gabber_transient;

        let body = sine * amp + click * 0.5;

        // Gabber distortion chain: hard clip → tanh soft saturator.
        // Hard clip (1× clean → 10× at clip=1) clamped to ±1.
        // Tanh after clip adds screaming upper harmonics characteristic of
        // Rotterdam-style Alpha Juno "hoover into distortion" kicks.
        let distorted = if p.gabber_clip > 0.001 {
            let hard_drive = 1.0 + p.gabber_clip * 9.0;
            let clipped = (body * hard_drive).clamp(-1.0, 1.0);
            let tanh_drive = 1.0 + p.gabber_clip * 5.0;
            (clipped * tanh_drive).tanh()
        } else {
            body
        };

        distorted * p.gabber_volume
    }
}
