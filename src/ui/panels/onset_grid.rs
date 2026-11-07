// ─── ui/panels/onset_grid.rs ──────────────────────────────────────────────────
// Onset / beat-grid overlay — rolling envelope strip of the
// master audio for the last bar overlaid with vertical step
// ticks.  Glanceable groove drift indicator: peaks aligned with
// ticks = on grid; peaks drifting before / after = early / late
// strikes.
//
// V1 deliberately scoped:
//   * Reads `granular_tap` (already maintained for the CAPTURE
//     path) so no new DSP plumbing — the master 3 s ring buffer
//     is grabbed under the read-lock-free constraint that the UI
//     already enforces for that field.
//   * Computes a mono RMS envelope at fixed window size (~4 ms
//     at 48 kHz) over the last bar's worth of samples.  Bar
//     length derives from the live BPM (60 / bpm × 4 beats).
//   * Renders the envelope as a filled grey rect strip;
//     16 vertical step-tick marks split the bar.
//   * Optional onset markers (energy-rise peaks) drawn as small
//     bright dots above the envelope, so the user can compare
//     onset positions to the grid.
//
// Pure UI: no DSP, no audio I/O, no state writes.  Reads are
// short and bounded by the bar-window size, not the full ring.

use crate::audio::SAMPLE_RATE;
use crate::ui::{ImpulseApp, theme};

/// Envelope analysis window in seconds.  ~4 ms at 48 kHz gives
/// 192 samples — small enough to resolve fast hi-hat strikes,
/// large enough that the RMS isn't dominated by single sample
/// transients.
const ENV_WIN_SEC: f32 = 0.004;
/// Minimum bar length in seconds to safely fit inside the ring
/// buffer (which holds 3 s).  Bars longer than this fall back to
/// 3 s of audio so we never read past the buffer's tail.
const MAX_BAR_SEC: f32 = 3.0;
/// How aggressively a peak in the envelope counts as an "onset".
/// The peak must be at least this fraction of the global maximum
/// (in the displayed window) to be marked.  Cheap energy-based
/// heuristic; a dedicated onset detector lives in `audio/onset.rs`
/// for the slicer use case.
const ONSET_THRESHOLD_FRAC: f32 = 0.4;

pub fn draw_onset_grid(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // Snapshot the bits we need from app state under one read.
    let (bpm, playhead_frac) = {
        let s = app.state.read();
        let bpm = s.sequencer.bpm.clamp(40.0, 240.0);
        // Playhead position 0..1 of the bar.  16 steps per bar;
        // current_step is 0..15 nominally but clamp defensively.
        let step = (s.sequencer.current_step % 16) as f32;
        (bpm, step / 16.0)
    };

    let bar_seconds = (60.0 / bpm) * 4.0;
    let bar_seconds = bar_seconds.min(MAX_BAR_SEC);
    let bar_samples = (bar_seconds * SAMPLE_RATE) as usize;

    // Pull the most recent `bar_samples` from `granular_tap`
    // (a circular buffer where `granular_tap_head` is the next
    // write position).  We read `bar_samples` samples ending at
    // head - 1, wrapping around the start.
    let envelope = compute_envelope(&app.granular_tap, app.granular_tap_head, bar_samples);
    let onsets = detect_onsets(&envelope);

    // ── Layout ──────────────────────────────────────────────
    let avail = ui.available_width();
    let strip_h = 56.0_f32;
    let onset_marker_h = 6.0_f32;
    let total_h = strip_h + onset_marker_h + 4.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail, total_h), egui::Sense::hover());

    let painter = ui.painter();

    // Background.
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(18));

    // Strip rect (envelope below, onsets above).
    let onset_y_top = rect.top() + 1.0;
    let onset_y_bot = onset_y_top + onset_marker_h;
    let strip_top = onset_y_bot + 2.0;
    let strip_bot = rect.bottom() - 1.0;
    let strip_h_real = strip_bot - strip_top;

    // Beat ticks — every 1/16.  Beats (1/4, 2/4, 3/4) get a
    // brighter line so the user reads bar structure easily.
    let cols = 16;
    let col_w = rect.width() / cols as f32;
    for i in 0..=cols {
        let x = rect.left() + col_w * i as f32;
        let is_beat = i % 4 == 0;
        let col = if is_beat {
            theme::ASH
        } else {
            egui::Color32::from_gray(40)
        };
        let stroke = egui::Stroke::new(if is_beat { 1.0 } else { 0.5 }, col);
        painter.line_segment([egui::pos2(x, strip_top), egui::pos2(x, strip_bot)], stroke);
    }

    // Envelope — render as a vertical bar per envelope bucket.
    // The envelope vec is already sized for a comfortable visual
    // density (bar_samples / win_size buckets).
    if !envelope.is_empty() {
        let env_max = envelope.iter().copied().fold(0.0_f32, f32::max).max(1e-3);
        let bucket_w = rect.width() / envelope.len() as f32;
        for (i, e) in envelope.iter().enumerate() {
            let h = (e / env_max).clamp(0.0, 1.0) * strip_h_real;
            if h < 0.5 {
                continue;
            }
            let x = rect.left() + bucket_w * i as f32;
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(x, strip_bot - h),
                egui::vec2(bucket_w.max(0.5), h),
            );
            painter.rect_filled(bar_rect, 0.0, egui::Color32::from_gray(140));
        }
        // Onset markers — small bright dots above the strip.
        for &onset_idx in &onsets {
            let frac = onset_idx as f32 / envelope.len() as f32;
            let x = rect.left() + frac * rect.width();
            let cy = (onset_y_top + onset_y_bot) * 0.5;
            painter.circle_filled(egui::pos2(x, cy), 2.0, theme::CHALK);
        }
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "no audio yet",
            egui::FontId::monospace(8.5),
            theme::IRON,
        );
    }

    // Playhead — vertical line against the strip.
    let ph_x = rect.left() + playhead_frac * rect.width();
    painter.line_segment(
        [
            egui::pos2(ph_x, rect.top()),
            egui::pos2(ph_x, rect.bottom()),
        ],
        egui::Stroke::new(1.5, theme::CHALK),
    );

    // BPM readout in the corner.
    painter.text(
        rect.right_top() + egui::vec2(-4.0, 4.0),
        egui::Align2::RIGHT_TOP,
        format!("{bpm:.0} BPM"),
        egui::FontId::monospace(8.0),
        theme::IRON,
    );
}

/// Compute an RMS envelope from a circular buffer's most recent
/// `bar_samples` ending at `head - 1`.
fn compute_envelope(buf: &[f32], head: usize, bar_samples: usize) -> Vec<f32> {
    if buf.is_empty() || bar_samples == 0 {
        return Vec::new();
    }
    let len = buf.len();
    let bar = bar_samples.min(len);
    let win = (ENV_WIN_SEC * SAMPLE_RATE) as usize;
    let win = win.max(8);
    if bar < win {
        return Vec::new();
    }
    let buckets = bar / win;
    let mut out = Vec::with_capacity(buckets);
    // Walk the bar window.  Start at (head + len - bar) % len
    // (i.e. bar_samples ago), advance one win-size step per
    // bucket.
    let start = (head + len - bar) % len;
    for b in 0..buckets {
        let mut acc = 0.0_f32;
        for s in 0..win {
            let idx = (start + b * win + s) % len;
            let v = buf[idx];
            acc += v * v;
        }
        out.push((acc / win as f32).sqrt());
    }
    out
}

/// Find local maxima in the envelope that exceed `THRESHOLD_FRAC`
/// × the window peak.  Cheap heuristic — sufficient for visual
/// "did the strike land on the tick" purposes.
fn detect_onsets(env: &[f32]) -> Vec<usize> {
    if env.len() < 3 {
        return Vec::new();
    }
    let max = env.iter().copied().fold(0.0_f32, f32::max);
    if max < 1e-3 {
        return Vec::new();
    }
    let thr = max * ONSET_THRESHOLD_FRAC;
    let mut out = Vec::new();
    // Skip the first / last sample to keep peak-pick simple.
    for i in 1..env.len() - 1 {
        if env[i] >= thr && env[i] > env[i - 1] && env[i] >= env[i + 1] {
            out.push(i);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_tracks_a_simple_signal() {
        // Build a fake circular buffer with a single transient
        // burst near the end.  Envelope should peak near the end
        // bucket.
        let mut buf = vec![0.0_f32; 48_000]; // 1 s
        for i in 40_000..40_500 {
            buf[i] = 1.0; // 500-sample loud burst
        }
        let env = compute_envelope(&buf, 48_000, 48_000);
        assert!(!env.is_empty(), "envelope should have buckets");
        let peak_idx = env
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        // Burst is at samples 40_000..40_500 of a 48_000-sample
        // window starting at sample 0 → fractional position
        // ~0.83.  Allow a window of ±10 % for the bucket boundary.
        let frac = peak_idx as f32 / env.len() as f32;
        assert!(
            (frac - 0.833).abs() < 0.1,
            "peak bucket should be near 0.83 of the window (got {frac})"
        );
    }

    #[test]
    fn detect_onsets_picks_local_maxima_above_threshold() {
        let env = vec![0.1, 0.2, 0.9, 0.3, 0.1, 0.6, 0.8, 0.2, 0.0];
        // Peaks above 0.4 * 0.9 = 0.36 are at indices 2 (0.9) and
        // 6 (0.8).  Both are local maxima.
        let onsets = detect_onsets(&env);
        assert!(onsets.contains(&2));
        assert!(onsets.contains(&6));
    }

    #[test]
    fn empty_buf_yields_empty_envelope() {
        assert!(compute_envelope(&[], 0, 1024).is_empty());
        assert!(compute_envelope(&vec![0.0; 100], 0, 0).is_empty());
    }
}
