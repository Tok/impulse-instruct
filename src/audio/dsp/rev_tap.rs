// ─── audio/dsp/rev_tap.rs ────────────────────────────────────────────────────
// Reverse-tap circular buffer shared by the Reverb and Delay FX arms.
// Extracted from mod.rs to stay under the 1000-line limit.

/// Length of the per-FX reverse-tap circular buffer in samples — 1 second
/// at the engine rate.  Trade-off: longer = richer reverse character but
/// more memory + a longer "rewind cycle" before the read tap loops.
pub(super) const REV_BUF_LEN: usize = crate::audio::SAMPLE_RATE_HZ as usize;

/// Reverb / delay playback direction.  Forward is the default; Reverse
/// plays the reverse-tap as the wet signal (preverb / anti-echo);
/// Mirror sums forward + reverse so the effect builds into and trails
/// off after the dry hit.  Discriminants match the persisted `u8` in
/// `FxState::*_dir` so old sessions load via `FxDirection::from_u8`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum FxDirection {
    #[default]
    Forward = 0,
    Reverse = 1,
    Mirror = 2,
}

impl FxDirection {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Reverse,
            2 => Self::Mirror,
            _ => Self::Forward,
        }
    }
}

/// Beat quantisation for the reverse-tap loop length — snaps the
/// rewind cycle to a musical subdivision of the active BPM.  `Free`
/// uses the fixed 1 s buffer.  Discriminants match the persisted `u8`
/// in `FxState::*_rev_quant` so old sessions load unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum FxRevQuant {
    #[default]
    Free = 0,
    QuarterBar = 1,
    HalfBar = 2,
    Bar = 3,
    TwoBars = 4,
}

impl FxRevQuant {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::QuarterBar,
            2 => Self::HalfBar,
            3 => Self::Bar,
            4 => Self::TwoBars,
            _ => Self::Free,
        }
    }

    /// Beat count corresponding to this quantisation — `None` for `Free`
    /// which means "no beat-sync, use the raw buffer length".
    fn beats(self) -> Option<f32> {
        match self {
            Self::Free => None,
            Self::QuarterBar => Some(1.0),
            Self::HalfBar => Some(2.0),
            Self::Bar => Some(4.0),
            Self::TwoBars => Some(8.0),
        }
    }
}

/// Write `sig` to the FX-specific circular buffer at `head`, advance head,
/// then read+return the current reversed-tap sample at `play` and decrement
/// it.  `active_len` is the loop length actually used — clamped to the
/// underlying buffer.  Lets the rewind cycle snap to a beat division
/// (computed from BPM at the call site) instead of always wrapping at the
/// fixed 1 s buffer length.
pub(super) fn step_rev_tap(
    buf: &mut [f32],
    head: &mut usize,
    play: &mut usize,
    sig: f32,
    active_len: usize,
) -> f32 {
    let n = active_len.min(buf.len()).max(2);
    if *head >= n {
        *head %= n;
    }
    if *play >= n {
        *play %= n;
    }
    buf[*head] = sig;
    *head = (*head + 1) % n;
    let out = buf[*play];
    *play = if *play == 0 { n - 1 } else { *play - 1 };
    out
}

/// Convert a quantisation choice into a reverse-tap loop length in
/// samples.  Returns `REV_BUF_LEN` (the heap-allocated cap) for `Free`
/// and for any quantised length that would exceed it at low BPMs.
pub(super) fn rev_tap_len_for_quant(quant: FxRevQuant, sample_rate: f32, bpm: f32) -> usize {
    let Some(beats) = quant.beats() else {
        return REV_BUF_LEN;
    };
    let samples = (sample_rate * 60.0 / bpm.max(1.0) * beats) as usize;
    samples.clamp(64, REV_BUF_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_quant_returns_full_buffer_length() {
        assert_eq!(
            rev_tap_len_for_quant(FxRevQuant::Free, 44100.0, 120.0),
            REV_BUF_LEN,
        );
    }

    #[test]
    fn quarter_bar_at_120_bpm_is_one_beat() {
        // 120 BPM → 60 / 120 = 0.5 s/beat = 22050 samples @ 44.1 kHz.
        let len = rev_tap_len_for_quant(FxRevQuant::QuarterBar, 44100.0, 120.0);
        assert!((len as i32 - 22050).abs() < 2);
    }

    #[test]
    fn one_bar_at_120_bpm_clamps_to_buffer_length() {
        // 4 beats * 0.5 s = 2 s = 88200 samples > REV_BUF_LEN, so clamped.
        let len = rev_tap_len_for_quant(FxRevQuant::Bar, 44100.0, 120.0);
        assert_eq!(len, REV_BUF_LEN);
    }

    #[test]
    fn quarter_bar_at_174_bpm_matches_jungle_tempo() {
        let len = rev_tap_len_for_quant(FxRevQuant::QuarterBar, 44100.0, 174.0);
        let expected = (44100.0 * 60.0 / 174.0) as usize;
        assert!(len.abs_diff(expected) < 2);
    }

    #[test]
    fn from_u8_unknown_values_fall_back_to_safe_defaults() {
        // Out-of-range integers from a corrupt session.json must land
        // on the "safe" variant for each enum.
        assert_eq!(FxDirection::from_u8(99), FxDirection::Forward);
        assert_eq!(FxRevQuant::from_u8(99), FxRevQuant::Free);
    }

    #[test]
    fn step_rev_tap_wraps_at_active_len_not_buffer_len() {
        let mut buf = vec![0.0f32; 16];
        let mut head = 0usize;
        let mut play = 0usize;
        for i in 0..6 {
            step_rev_tap(&mut buf, &mut head, &mut play, i as f32, 4);
        }
        assert_eq!(head, 2);
    }
}
