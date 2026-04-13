// ─── audio/onset.rs ──────────────────────────────────────────────────────────
// Simple energy-based onset detector for drum-loop slicing.
//
// Not as accurate as spectral flux or a trained model, but cheap and
// good enough for the breaks we care about (amen / think / funky drummer
// style samples).  The algorithm:
//
//   1. Split the mono buffer into short windows (~11.6ms @ 44.1k).
//   2. Compute RMS per window.
//   3. First-order difference gives an energy-rise envelope.
//   4. Peak-pick local maxima whose height exceeds the global mean
//      delta by a configurable factor.
//   5. Return the strongest `max_slices` peaks, sorted by time,
//      each normalised to 0..1 of the full buffer length.

/// Detect up to `max_slices` transient onsets in a mono sample buffer.
/// Positions returned are normalised 0..1 of the buffer length, in
/// ascending order, unique.  Always starts with 0.0 (the head of the
/// sample) so slice 0 is anchored.
pub fn detect_onsets(samples: &[f32], sample_rate: f32, max_slices: usize) -> Vec<f32> {
    if samples.len() < 512 || max_slices == 0 {
        return vec![0.0];
    }
    let target = max_slices.max(1);
    let win = (sample_rate * 0.0116).round() as usize; // ~11.6ms
    let hop = (win / 2).max(64);
    let mut rms: Vec<f32> = Vec::new();
    let mut i = 0;
    while i + win <= samples.len() {
        let mut acc = 0.0_f32;
        for s in &samples[i..i + win] {
            acc += s * s;
        }
        rms.push((acc / win as f32).sqrt());
        i += hop;
    }
    if rms.len() < 4 {
        return vec![0.0];
    }

    // First-order difference, clamped to positive (we want rises).
    let mut delta: Vec<f32> = Vec::with_capacity(rms.len());
    delta.push(0.0);
    for k in 1..rms.len() {
        delta.push((rms[k] - rms[k - 1]).max(0.0));
    }

    // Threshold = mean + stddev of positive deltas.
    let positives: Vec<f32> = delta.iter().copied().filter(|d| *d > 0.0).collect();
    let mean: f32 = if positives.is_empty() {
        0.0
    } else {
        positives.iter().sum::<f32>() / positives.len() as f32
    };
    let var: f32 = if positives.is_empty() {
        0.0
    } else {
        positives.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / positives.len() as f32
    };
    let std = var.sqrt();
    let threshold = mean + std;

    // Peak-pick: local maxima above threshold, min spacing 3 frames.
    let mut peaks: Vec<(usize, f32)> = Vec::new();
    for k in 1..delta.len() - 1 {
        if delta[k] >= threshold && delta[k] >= delta[k - 1] && delta[k] >= delta[k + 1] {
            if let Some((last_k, _)) = peaks.last()
                && k - *last_k < 3
            {
                continue;
            }
            peaks.push((k, delta[k]));
        }
    }

    // Always anchor slice 0 at the start of the sample.
    let mut anchored: Vec<(usize, f32)> = vec![(0, f32::INFINITY)];
    anchored.extend(peaks);

    // Keep the strongest `target` peaks by delta magnitude, then re-sort
    // by time so the caller gets an ascending slice-position list.
    anchored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    anchored.truncate(target);
    anchored.sort_by_key(|&(k, _)| k);

    // Convert frame index → normalised time.  Dedup near-duplicates.
    let mut out: Vec<f32> = Vec::new();
    for &(k, _) in &anchored {
        let t = (k as f32 * hop as f32 / samples.len() as f32).clamp(0.0, 1.0);
        if !out.iter().any(|&y| (y - t).abs() < 1e-4) {
            out.push(t);
        }
    }
    if out.is_empty() {
        out.push(0.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_short_returns_single_zero() {
        assert_eq!(detect_onsets(&[], 44100.0, 8), vec![0.0]);
        assert_eq!(detect_onsets(&[0.1; 100], 44100.0, 8), vec![0.0]);
    }

    #[test]
    fn constant_signal_has_at_least_one_slice() {
        let sig = vec![0.5_f32; 4096];
        let out = detect_onsets(&sig, 44100.0, 8);
        assert!(!out.is_empty());
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn obvious_transients_found() {
        // Build a signal with three clear bursts at 25%, 50%, 75%.
        let len = 44100; // 1s at 44.1k
        let mut sig = vec![0.0_f32; len];
        for &pos in &[0.25, 0.5, 0.75] {
            let start = (pos * len as f32) as usize;
            for k in 0..1000 {
                if start + k < len {
                    sig[start + k] = (k as f32 * 0.01).sin() * 0.9;
                }
            }
        }
        let out = detect_onsets(&sig, 44100.0, 8);
        // First entry always 0.0; then we expect at least 2 of the 3
        // transient positions detected.
        assert_eq!(out[0], 0.0);
        let found_near = |target: f32| out.iter().any(|&x| (x - target).abs() < 0.05);
        let hits = [found_near(0.25), found_near(0.5), found_near(0.75)]
            .iter()
            .filter(|&&b| b)
            .count();
        assert!(hits >= 2, "expected ≥2 transients, got {:?}", out);
    }
}
