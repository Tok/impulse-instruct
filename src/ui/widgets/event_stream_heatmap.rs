// ─── ui/widgets/event_stream_heatmap.rs ──────────────────────────────────────
// Per-voice activity heatmap overlay for the EventStream widget.  Lifted
// out of `event_stream.rs` so that file stays under the 1000-line cap.
//
// Reads the same `melodic_log` + `drum_log` queues the main widget uses
// for past-side dot rendering, but bins per sequencer step into rows
// (one per voice category).  Recent activity reads bright; older bins
// fade.  Toggled via `ui_prefs.stream_heatmap` — when off, the parent
// widget skips this draw entirely and the heatmap zone collapses to
// zero height.

use egui::{Color32, Pos2, Rect, Stroke};

use crate::ui::theme;
use crate::ui::{DrumLogEntry, MelodicLogEntry, MelodicVoice};

/// One heatmap row's category — used to filter `melodic_log` /
/// `drum_log` into per-voice bins.  Empty rows still draw their label
/// + an unlit strip so the layout stays stable when a voice falls
///   silent.
pub(super) enum HeatmapRow {
    Bass,
    An1x,
    Hoover,
    Kick,
    Snare,
    Hihat,
    Clap,
}

const HEATMAP_ROWS: &[(HeatmapRow, &str)] = &[
    (HeatmapRow::Bass, "BASS"),
    (HeatmapRow::An1x, "AN1X"),
    (HeatmapRow::Hoover, "HOOV"),
    (HeatmapRow::Kick, "KICK"),
    (HeatmapRow::Snare, "SN"),
    (HeatmapRow::Hihat, "HAT"),
    (HeatmapRow::Clap, "CLAP"),
];

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_heatmap(
    painter: &egui::Painter,
    rect: Rect,
    melodic_log: &std::collections::VecDeque<MelodicLogEntry>,
    drum_log: &std::collections::VecDeque<DrumLogEntry>,
    smooth_global_step: f64,
    step_w: f32,
    display_steps: f32,
    now_x: f32,
    bins_min_x: f32,
    bins_max_x: f32,
) {
    use crate::state::DrumVoice;

    // Backdrop — same recessed-screen treatment as the main widget.
    theme::draw_screen_bezel(painter, rect, egui::Rounding::same(2.0));

    let row_h = rect.height() / HEATMAP_ROWS.len() as f32;
    let label_w = 26.0_f32;
    let label_font = egui::FontId::monospace(6.0);
    let bin_min_x = (rect.min.x + label_w).max(bins_min_x);
    let bin_max_x = bins_max_x;

    // Number of bins on the past side (left of `now_x`).  Cap to keep
    // the alloc-free count array on the stack.
    const MAX_PAST_BINS: usize = 192;
    let past_bins = ((display_steps * 0.5).ceil() as usize).min(MAX_PAST_BINS);
    if past_bins == 0 || step_w <= 0.0 {
        return;
    }

    for (row_idx, (row, label)) in HEATMAP_ROWS.iter().enumerate() {
        let row_top = rect.min.y + row_idx as f32 * row_h;
        let row_rect = Rect::from_min_max(
            Pos2::new(rect.min.x, row_top),
            Pos2::new(rect.max.x, row_top + row_h),
        );
        // Faint horizontal separator between rows.
        if row_idx > 0 {
            painter.line_segment(
                [
                    Pos2::new(rect.min.x, row_top),
                    Pos2::new(rect.max.x, row_top),
                ],
                Stroke::new(0.3, Color32::from_gray(20)),
            );
        }
        painter.text(
            Pos2::new(rect.min.x + 2.0, row_rect.center().y),
            egui::Align2::LEFT_CENTER,
            *label,
            label_font.clone(),
            Color32::from_gray(70),
        );

        // Bin counts — index 0 = bin closest to `now_x` (most recent).
        let mut bin_counts = [0u32; MAX_PAST_BINS];
        let assign = |off: f64, counts: &mut [u32; MAX_PAST_BINS]| {
            // Negative off = past.  Convert to bin index: 0 = just fired.
            if off > 0.0 {
                return;
            }
            let bin = ((-off) as usize).min(past_bins.saturating_sub(1));
            if bin < counts.len() {
                counts[bin] = counts[bin].saturating_add(1);
            }
        };
        match row {
            HeatmapRow::Bass | HeatmapRow::An1x | HeatmapRow::Hoover => {
                for entry in melodic_log.iter() {
                    let matches_voice = matches!(
                        (row, entry.voice),
                        (HeatmapRow::Bass, MelodicVoice::Bass(_))
                            | (HeatmapRow::An1x, MelodicVoice::An1x)
                            | (HeatmapRow::Hoover, MelodicVoice::Hoover)
                    );
                    if !matches_voice {
                        continue;
                    }
                    let off = entry.fired_at as f64 - smooth_global_step;
                    assign(off, &mut bin_counts);
                }
            }
            HeatmapRow::Kick | HeatmapRow::Snare | HeatmapRow::Hihat | HeatmapRow::Clap => {
                for entry in drum_log.iter() {
                    let matches_voice = matches!(
                        (row, entry.voice),
                        (HeatmapRow::Kick, DrumVoice::Kick808 | DrumVoice::Kick909)
                            | (HeatmapRow::Snare, DrumVoice::Snare808 | DrumVoice::Snare909)
                            | (
                                HeatmapRow::Hihat,
                                DrumVoice::HihatClosed808
                                    | DrumVoice::HihatOpen808
                                    | DrumVoice::HihatClosed909
                                    | DrumVoice::HihatOpen909,
                            )
                            | (HeatmapRow::Clap, DrumVoice::Clap909)
                    );
                    if !matches_voice {
                        continue;
                    }
                    let off = entry.fired_at as f64 - smooth_global_step;
                    assign(off, &mut bin_counts);
                }
            }
        }

        // Render each bin as a grayscale-shaded rect.  Older bins fade
        // (brightness scaled by `1 - age_frac`) so the eye snaps to
        // recent activity without losing the long-tail context.
        let bin_top = row_top + 1.0;
        let bin_bot = row_top + row_h - 1.0;
        for (i, count) in bin_counts.iter().enumerate().take(past_bins) {
            if *count == 0 {
                continue;
            }
            let bin_x_right = now_x - i as f32 * step_w;
            let bin_x_left = bin_x_right - step_w;
            if bin_x_right < bin_min_x || bin_x_left > bin_max_x {
                continue;
            }
            // Clip to the row's bin area.
            let l = bin_x_left.max(bin_min_x);
            let r = bin_x_right.min(bin_max_x);
            if r <= l {
                continue;
            }
            let intensity = (*count as f32 / 3.0).clamp(0.0, 1.0);
            let age = (i as f32 / past_bins as f32).clamp(0.0, 1.0);
            let v_max = 70.0 + intensity * 160.0; // 70..230
            let v = (v_max * (1.0 - age * 0.6)) as u8;
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(l, bin_top), Pos2::new(r, bin_bot)),
                egui::Rounding::same(0.5),
                Color32::from_gray(v),
            );
        }
    }
}
