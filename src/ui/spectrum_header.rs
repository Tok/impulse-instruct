// ─── ui/spectrum_header.rs ───────────────────────────────────────────────────
// Header spectrum-bars widget.  Shares the recessed-screen styling of
// the oscilloscopes in `scope_footer.rs` so the header reads as one
// coherent instrument panel: same bezel, same grayscale language,
// same phosphor idea for peak markers.
//
// Input: pre-computed FFT magnitudes (`ImpulseApp.spectrum_magnitudes`,
// already populated every frame by `update_spectrum` on the scope ring
// buffer).  We just do the display math here.
//
// Grayscale (default): bar brightness tracks amplitude, 40 → 200.
// Huth mode (toggled via `ui_prefs.huth_oscilloscope`): every bar
// is tinted by the pitch class of its centre frequency, so a chord's
// tonal make-up reads visually — C-blue cold on the low end, F-orange
// warm in the middle, Rose / Pensée on the upper partials.

use crate::audio::SAMPLE_RATE;
use crate::ui::theme;
use egui::Color32;

/// Number of logarithmic display bands (20 Hz – 20 kHz).  Matches the
/// rack-module spectrum analyser so a user comparing the two sees
/// identical resolution.
const NUM_BANDS: usize = 64;
/// dBFS floor / ceiling for the vertical scale.
const DB_FLOOR: f32 = -96.0;
const DB_CEIL: f32 = 0.0;

/// Draw the header spectrum-bars widget.  Signature mirrors
/// `draw_scope_colored` so the header layout can swap scopes in and
/// out of the same rect without special-casing sizes.
///
/// `peaks` are the exponential-max peak-hold values maintained by
/// `update_spectrum` — same length as `magnitudes`.  When `huth_color`
/// carries a value it overrides the per-band tint (chosen to match
/// the dominant note of the current scope frame, matching the
/// scope widgets' global-tint behaviour).
pub fn draw_spectrum_bars(
    ui: &mut egui::Ui,
    magnitudes: &[f32],
    peaks: &[f32],
    w: f32,
    h: f32,
    huth_color: Option<Color32>,
    huth_per_band: bool,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);

    // Recessed-screen bezel — same treatment as the oscilloscopes so
    // the header's center column reads as one panel regardless of
    // which viz the user has selected.
    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));

    let bin_hz = SAMPLE_RATE / crate::audio::spectrum::FFT_SIZE as f32;
    let bands = if magnitudes.is_empty() {
        vec![DB_FLOOR; NUM_BANDS]
    } else {
        crate::audio::spectrum::log_bands(magnitudes, bin_hz, NUM_BANDS)
    };
    let peak_bands = if peaks.is_empty() {
        vec![DB_FLOOR; NUM_BANDS]
    } else {
        crate::audio::spectrum::log_bands(peaks, bin_hz, NUM_BANDS)
    };

    let bar_w = rect.width() / NUM_BANDS as f32;
    let db_range = DB_CEIL - DB_FLOOR;

    // Pre-compute each band's centre frequency and pitch class — used
    // for the Huth-per-band tint path.  Cheap enough to do unconditionally;
    // keeps the draw loop straightforward.
    let lo_hz = 20.0_f32;
    let hi_hz = 20_000.0_f32;
    let log_lo = lo_hz.ln();
    let log_hi = hi_hz.ln();

    for (i, &db) in bands.iter().enumerate() {
        let norm = ((db - DB_FLOOR) / db_range).clamp(0.0, 1.0);
        let bar_h = norm * rect.height();
        if bar_h < 0.5 {
            continue;
        }
        let x = rect.left() + i as f32 * bar_w;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(x, rect.bottom() - bar_h),
            egui::vec2((bar_w - 1.0).max(1.0), bar_h),
        );

        let color = if huth_per_band {
            // Centre freq of band i on the log scale → MIDI note → Huth color.
            let f =
                ((log_lo + (log_hi - log_lo) * (i as f32 + 0.5) / NUM_BANDS as f32).exp()).max(1.0);
            let midi = (69.0 + 12.0 * (f / 440.0).log2()).round().clamp(0.0, 127.0) as u8;
            // Fade dim bars toward grayscale so low-energy bins don't
            // paint a saturated rainbow over silence.  `norm` rides
            // 0..1 with amplitude; at norm=0 the bar is the same gray
            // as the default path, at norm=1 it's full Huth saturation.
            let h = theme::note_color(midi);
            let base = Color32::from_gray((40.0 + norm * 160.0) as u8);
            lerp_color(base, h, (norm * 0.85).clamp(0.0, 1.0))
        } else if let Some(c) = huth_color {
            // Global Huth tint (waveform-wide pitch detection — same
            // source as the scope widgets).  Blend toward grayscale at
            // low amplitudes so silence still reads neutral.
            let base = Color32::from_gray((40.0 + norm * 160.0) as u8);
            lerp_color(base, c, (norm * 0.80).clamp(0.0, 1.0))
        } else {
            // Default grayscale: brightness ramps 40..200 with amplitude,
            // matching the sparkline treatment so the whole header
            // shares one visual language.
            Color32::from_gray((40.0 + norm * 160.0) as u8)
        };
        painter.rect_filled(bar_rect, egui::Rounding::ZERO, color);
    }

    // Peak-hold markers — thin horizontal strokes.  Grayscale always
    // (keeps the peaks readable against Huth-tinted bars without
    // fighting their colour).
    for (i, &db) in peak_bands.iter().enumerate().take(NUM_BANDS) {
        let norm = ((db - DB_FLOOR) / db_range).clamp(0.0, 1.0);
        if norm < 0.02 {
            continue;
        }
        let x = rect.left() + i as f32 * bar_w;
        let y = rect.bottom() - norm * rect.height();
        painter.line_segment(
            [egui::pos2(x, y), egui::pos2(x + (bar_w - 1.0).max(1.0), y)],
            egui::Stroke::new(1.0, Color32::from_gray(220)),
        );
    }
}

/// Linear-blend two Color32s by t ∈ [0, 1].  Lives here rather than
/// in theme.rs because this is the only site that needs it; moving
/// it is a trivial refactor if a second caller appears.
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| -> u8 { ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8 };
    Color32::from_rgba_unmultiplied(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
        lerp(a.a(), b.a()),
    )
}
