// ─── audio/dsp/rev_tap.rs ────────────────────────────────────────────────────
// Reverse-tap circular buffer shared by the Reverb and Delay FX arms.
// Extracted from mod.rs to stay under the 1000-line limit.

/// Length of the per-FX reverse-tap circular buffer in samples — 1 second
/// at the engine rate.  Trade-off: longer = richer reverse character but
/// more memory + a longer "rewind cycle" before the read tap loops.
pub(super) const REV_BUF_LEN: usize = crate::audio::SAMPLE_RATE_HZ as usize;

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

/// Convert a quant code (0=free 1 s, 1=1/4 bar, 2=1/2 bar, 3=1 bar,
/// 4=2 bars) into a reverse-tap loop length in samples.  Returns
/// `REV_BUF_LEN` (the heap-allocated cap) for the free setting and
/// for any quantised length that would exceed it at low BPMs.
pub(super) fn rev_tap_len_for_quant(quant: u8, sample_rate: f32, bpm: f32) -> usize {
    if quant == 0 {
        return REV_BUF_LEN;
    }
    let beats = match quant {
        1 => 1.0,
        2 => 2.0,
        3 => 4.0,
        _ => 8.0,
    };
    let samples = (sample_rate * 60.0 / bpm.max(1.0) * beats) as usize;
    samples.clamp(64, REV_BUF_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quant_zero_returns_full_buffer_length() {
        assert_eq!(rev_tap_len_for_quant(0, 44100.0, 120.0), REV_BUF_LEN);
    }

    #[test]
    fn quant_one_quarter_bar_at_120_bpm_is_one_beat() {
        // 120 BPM → 60 / 120 = 0.5 s/beat = 22050 samples @ 44.1 kHz.
        let len = rev_tap_len_for_quant(1, 44100.0, 120.0);
        assert!((len as i32 - 22050).abs() < 2);
    }

    #[test]
    fn quant_one_bar_at_120_bpm_is_two_seconds_clamped() {
        // 4 beats * 0.5 s = 2 s = 88200 samples > REV_BUF_LEN, so clamped.
        let len = rev_tap_len_for_quant(3, 44100.0, 120.0);
        assert_eq!(len, REV_BUF_LEN);
    }

    #[test]
    fn quant_one_quarter_bar_at_174_bpm_matches_jungle_tempo() {
        let len = rev_tap_len_for_quant(1, 44100.0, 174.0);
        let expected = (44100.0 * 60.0 / 174.0) as usize;
        assert!(len.abs_diff(expected) < 2);
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
