// ─── sequencer/preecho.rs ────────────────────────────────────────────────────
// Pre-echo pattern modulator — KeyKit-inspired reverse-echo lead-in.
//
// Semantics (per the expert's framing): the user declares a set of
// "anchor" step indices that form a pulse/groove signature.  The steps
// leading up to each anchor get shaped into a build-up — velocity ramp,
// ratchet build — so the pattern feels like it's "reaching for" the
// anchor.  Wrap-around is intentional: the tail of the bar leads into
// the downbeat at step 0.
//
// v1 scope:
//   - explicit anchor indices (user or LLM declared)
//   - fixed lead-in length in steps (0 = disabled)
//   - velocity ramp (0.3 → 1.0) toggleable
//   - ratchet build (+0 → +3) toggleable
//   - replace semantics: steps inside the lead-in get the ramped values
//
// Deferred to v2:
//   - note approach (chromatic / scale-step / arp)
//   - probability ramp
//   - accent / slide trailing
//   - auto-length (fill gap between anchors)
//   - curve shapes (exp, log) — v1 is linear

use serde::{Deserialize, Serialize};

/// Per-voice pre-echo configuration.  Empty `anchors` or `length == 0`
/// disables the effect entirely, which is the default.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PreechoConfig {
    /// Step indices that anchor the groove.  Out-of-range entries (>=
    /// pattern steps) are ignored at apply time.
    #[serde(default)]
    pub anchors: Vec<u8>,
    /// Number of lead-in steps before each anchor.  0 = disabled.
    /// 1..=16 is the useful range.
    #[serde(default)]
    pub length: u8,
    /// When true, scale velocity 0.3 → 1.0 across the lead-in (linear).
    #[serde(default)]
    pub velocity_ramp: bool,
    /// When true, add 0 → 3 to ratchet count across the lead-in
    /// (saturated at the u8-max in downstream code, but practically
    /// clamps to the sequencer's ratchet range).
    #[serde(default)]
    pub ratchet_ramp: bool,
}

impl PreechoConfig {
    pub fn is_active(&self) -> bool {
        self.length > 0 && !self.anchors.is_empty()
    }
}

/// Compute the pre-echo modulation to apply at a given step, given the
/// total pattern length.  Returns `(velocity_multiplier, ratchet_add)`.
///
/// Rules:
/// - If preecho is inactive (length=0 or anchors empty): (1.0, 0) —
///   no modulation.
/// - If `step` is on an anchor: (1.0, 0) — anchors play at user
///   intensity; they're the destination, not part of the ramp.
/// - If `step` is within `length` steps BEFORE any anchor (wrap-aware):
///   linear ramp based on distance.  Distance 1 = last lead-in step
///   (strongest); distance `length` = first lead-in step (weakest).
/// - Otherwise: (1.0, 0).
pub fn preecho_scale(step: usize, total_steps: usize, cfg: &PreechoConfig) -> (f32, u8) {
    if !cfg.is_active() || total_steps == 0 {
        return (1.0, 0);
    }
    let length = cfg.length as usize;
    // Find the smallest positive distance to any anchor (wrap-aware).
    let mut best_d: Option<usize> = None;
    for &a in &cfg.anchors {
        let a = a as usize;
        if a >= total_steps {
            continue;
        }
        let d = (a + total_steps - step) % total_steps;
        if d == 0 {
            return (1.0, 0); // on anchor
        }
        if d <= length && best_d.map(|x| d < x).unwrap_or(true) {
            best_d = Some(d);
        }
    }
    let Some(d) = best_d else {
        return (1.0, 0);
    };
    // d == 1 is the closest lead-in step (strongest), d == length is the
    // earliest (weakest).  Position 0..1 where 1 = at the anchor.
    let pos = 1.0 - ((d - 1) as f32 / length.max(1) as f32);
    let vel_mul = if cfg.velocity_ramp {
        0.3 + 0.7 * pos
    } else {
        1.0
    };
    let ratchet_add = if cfg.ratchet_ramp {
        (pos * 3.0).round() as u8
    } else {
        0
    };
    (vel_mul, ratchet_add)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(anchors: &[u8], length: u8, vr: bool, rr: bool) -> PreechoConfig {
        PreechoConfig {
            anchors: anchors.to_vec(),
            length,
            velocity_ramp: vr,
            ratchet_ramp: rr,
        }
    }

    #[test]
    fn inactive_returns_identity() {
        assert_eq!(preecho_scale(5, 16, &cfg(&[], 4, true, true)), (1.0, 0));
        assert_eq!(preecho_scale(5, 16, &cfg(&[0], 0, true, true)), (1.0, 0));
    }

    #[test]
    fn anchor_step_is_not_scaled() {
        let c = cfg(&[4], 3, true, true);
        assert_eq!(preecho_scale(4, 16, &c), (1.0, 0));
    }

    #[test]
    fn outside_leadin_is_identity() {
        let c = cfg(&[8], 3, true, true);
        // step 4 is 4 away from anchor 8 → outside length=3 window
        assert_eq!(preecho_scale(4, 16, &c), (1.0, 0));
    }

    #[test]
    fn velocity_ramp_last_step_near_full() {
        let c = cfg(&[8], 4, true, false);
        // step 7 is 1 away from anchor → pos = 1.0 → vel = 1.0
        let (v, r) = preecho_scale(7, 16, &c);
        assert!((v - 1.0).abs() < 0.001);
        assert_eq!(r, 0);
    }

    #[test]
    fn velocity_ramp_earliest_step_is_weakest() {
        let c = cfg(&[8], 4, true, false);
        // step 4 is 4 away (d=4) → pos = 1 - 3/4 = 0.25 → vel = 0.3 + 0.7*0.25 = 0.475
        let (v, _r) = preecho_scale(4, 16, &c);
        assert!((v - 0.475).abs() < 0.001, "expected ~0.475, got {}", v);
    }

    #[test]
    fn ratchet_ramp_builds() {
        let c = cfg(&[8], 4, false, true);
        // step 7 (d=1, pos=1.0) → +3
        assert_eq!(preecho_scale(7, 16, &c).1, 3);
        // step 4 (d=4, pos=0.25) → round(0.75) = 1
        assert_eq!(preecho_scale(4, 16, &c).1, 1);
    }

    #[test]
    fn wraps_around_pattern_end() {
        let c = cfg(&[0], 4, true, false);
        // step 14 is 2 away from anchor 0 (wrapping) → in window
        let (v, _r) = preecho_scale(14, 16, &c);
        assert!(v > 0.3 && v < 1.0, "expected ramp value, got {}", v);
        // step 12 is 4 away (at edge of window) → still in
        let (v, _r) = preecho_scale(12, 16, &c);
        assert!(v > 0.3 && v < 1.0);
    }

    #[test]
    fn nearest_anchor_wins_when_multiple() {
        let c = cfg(&[4, 12], 3, true, false);
        // step 10 is 2 from anchor 12 and 6 from anchor 4 → picks 12, in window
        let (v_10, _) = preecho_scale(10, 16, &c);
        assert!(v_10 > 0.5);
        // step 6 is 2 from anchor 4 (wrapping wouldn't matter here) — actually
        // d(6→12) = 6 and d(6→4) = (4+16-6)%16 = 14.  12 is further by forward-
        // distance in our convention.  Both outside window?  4 forward from 6
        // is d = (4+16-6)%16 = 14 (out); 12 forward from 6 is d=6 (out).  So
        // step 6 is NOT in any lead-in window of length 3.
        assert_eq!(preecho_scale(6, 16, &c), (1.0, 0));
    }
}
