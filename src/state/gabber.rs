// ─── state/gabber.rs ──────────────────────────────────────────────────────────
// Gabber-kick synth parameters.  A dedicated voice distinct from the 808/909
// kicks — tuned for hardcore, with extreme pitch-sweep, hard clipper, and a
// transient "click" layer built directly into the voice.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GabberKickParams {
    /// 0–1 → 50–110 Hz base — gabber sits higher than 808/909 by default.
    pub pitch: f32,
    /// 0–1 → 0.1–1.5 s amp decay.
    pub decay: f32,
    /// 0–1 → 1×–12× pitch-env depth.  Default is already in the "extreme" zone.
    pub pitch_env_depth: f32,
    /// 0–1 → 5–60 ms pitch-env time.  Fast by design.
    pub pitch_env_time: f32,
    /// 0–1 → 0–15× drive into a tanh saturator (on top of the 10× hard clip).
    /// 0 is clean, 0.4 is Rotterdam, 1 is meltdown.
    pub clip: f32,
    /// 0–1 → volume of the transient click layer (filtered noise burst on
    /// attack).  The click is a separate layer with its own envelope and
    /// is the signature tok that distinguishes gabber from distorted 808.
    pub transient: f32,
    /// 0–1 final voice level.
    pub volume: f32,
    /// Stereo pan: -1 = hard left, 0 = centre, +1 = hard right.
    #[serde(default)]
    pub pan: f32,
}

impl Default for GabberKickParams {
    fn default() -> Self {
        Self {
            pitch: 0.4,            // ~74 Hz
            decay: 0.35,           // ~0.6 s
            pitch_env_depth: 0.85, // very steep sweep
            pitch_env_time: 0.25,  // ~18 ms
            clip: 0.55,            // aggressive by default
            transient: 0.6,        // prominent click
            volume: 0.8,
            pan: 0.0,
        }
    }
}
