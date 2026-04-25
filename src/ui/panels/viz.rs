// ─── ui/panels/viz.rs ────────────────────────────────────────────────────────
// Tier-1 visualisation modules: stereo vectorscope (goniometer), LFO scope,
// pitch tracker / tuner, and chord / key display.  All four render directly
// from existing AppState (or from cached values on `ImpulseApp` like
// `scope_buf`, `stereo_buf`, `spectrum_mags`) — no audio-thread changes,
// no FX plumbing.
//
// Kept in one file rather than four 50-line siblings; together they're well
// under the 1000-line cap and share a few rendering conventions (canvas
// rect setup, Huth-coloured note labels, monospace overlay text).

use crate::audio::SAMPLE_RATE;
use crate::audio::analysis::{
    ChordKind, PITCH_CLASS_NAMES, chroma_from_spectrum, detect_chord, detect_pitch_hz,
};
use crate::audio::dsp::hz_to_midi;
use crate::audio::spectrum::compute_spectrum;
use crate::ui::{ImpulseApp, theme};

// ─── Stereo vectorscope (goniometer) ─────────────────────────────────────────

/// XY plot of L vs R from `app.stereo_buf` (interleaved).  A pure mono
/// signal traces a vertical line; uncorrelated stereo fills a circle;
/// out-of-phase pairs trace a horizontal line.
pub fn draw_vectorscope(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let avail_w = ui.available_width().max(48.0);
    let avail_h = ui.available_height().max(48.0);
    let side = avail_w.min(avail_h);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(side, side), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::Rounding::same(2.0), egui::Color32::from_gray(8));

    // Crosshair: vertical = mono, horizontal = anti-mono.  Drawn faint so
    // it doesn't compete with the lissajous trace.
    let cross = egui::Stroke::new(1.0, egui::Color32::from_gray(28));
    painter.line_segment(
        [
            egui::Pos2::new(rect.center().x, rect.top()),
            egui::Pos2::new(rect.center().x, rect.bottom()),
        ],
        cross,
    );
    painter.line_segment(
        [
            egui::Pos2::new(rect.left(), rect.center().y),
            egui::Pos2::new(rect.right(), rect.center().y),
        ],
        cross,
    );

    // Plot: rotate L/R by 45° so a mono signal (L=R) traces vertical, the
    // classic goniometer convention.  scale = side/2 less a 4-px margin.
    let scale = (side * 0.5) - 4.0;
    let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
    let frames = app.stereo_buf.len() / 2;
    if frames < 4 {
        return;
    }
    // Step over the buffer so we draw at most ~512 points; phosphor trail
    // looks better with a moderate density.
    let step = (frames / 512).max(1);
    let mut prev: Option<egui::Pos2> = None;
    for i in (0..frames).step_by(step) {
        let l = app.stereo_buf[i * 2];
        let r = app.stereo_buf[i * 2 + 1];
        // Rotate −45° so L=R lies on the +Y axis (downward in screen space).
        let x = (l - r) * inv_sqrt2;
        let y = (l + r) * inv_sqrt2;
        let px = rect.center().x + x * scale;
        let py = rect.center().y - y * scale;
        let p = egui::Pos2::new(px, py);
        if let Some(prv) = prev {
            painter.line_segment(
                [prv, p],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 200, 140)),
            );
        }
        prev = Some(p);
    }
}

// ─── LFO scope ───────────────────────────────────────────────────────────────

/// Render the connected LFO module's output waveform across one cycle.
/// V1: each `LfoScope` rack module shares the *first enabled* LFO slot —
/// CV-cable wiring (where module_id ↔ slot index) lands in a follow-up
/// once the rack-side mod-cable types stabilise.  Useful as-is for
/// confirming an LFO's shape / rate without tabbing back to the LFO panel.
pub fn draw_lfo_scope(app: &mut ImpulseApp, ui: &mut egui::Ui, _module_id: u32) {
    use crate::audio::dsp::fx_math::lfo_value_at;

    let slots = app.state.read().lfo;
    let active_slot = slots.iter().enumerate().find(|(_, s)| s.enabled);

    let avail_w = ui.available_width().max(64.0);
    let avail_h = ui.available_height().max(40.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(avail_w, avail_h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::Rounding::same(2.0), egui::Color32::from_gray(8));

    // Zero-crossing line.
    painter.line_segment(
        [
            egui::Pos2::new(rect.left(), rect.center().y),
            egui::Pos2::new(rect.right(), rect.center().y),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_gray(30)),
    );

    let Some((slot_idx, slot)) = active_slot else {
        // No enabled LFO — print a hint.
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "(no enabled LFO)",
            egui::FontId::monospace(8.0),
            egui::Color32::from_gray(60),
        );
        return;
    };

    // Trace one full cycle across the X axis.  We use S/H = 0.0 because
    // the panel doesn't need the live audio-thread random source — the
    // shape of S/H draws as a stair-step at run time anyway.
    let n = (avail_w as usize).max(64);
    let mut prev: Option<egui::Pos2> = None;
    for i in 0..n {
        let phase = i as f32 / n as f32;
        let v = lfo_value_at(phase, slot.waveform, 0.0);
        // Apply depth so user can see the actual amplitude they'd hear.
        let scaled = v * slot.depth.clamp(0.0, 1.0);
        let px = rect.left() + i as f32 * avail_w / n as f32;
        let py = rect.center().y - scaled * (rect.height() * 0.45);
        let p = egui::Pos2::new(px, py);
        if let Some(prv) = prev {
            painter.line_segment(
                [prv, p],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(140, 180, 220)),
            );
        }
        prev = Some(p);
    }

    // Slot index + waveform name in the top-left.
    painter.text(
        egui::Pos2::new(rect.left() + 4.0, rect.top() + 2.0),
        egui::Align2::LEFT_TOP,
        format!("LFO {} · {:?}", slot_idx + 1, slot.waveform),
        egui::FontId::monospace(7.5),
        egui::Color32::from_gray(140),
    );
}

// ─── Pitch tracker / tuner ───────────────────────────────────────────────────

/// Big note name + cents-off needle, driven by `detect_pitch_hz` over the
/// recent scope buffer.  Renders an "—" if the signal is too quiet or
/// too inharmonic to lock on.
pub fn draw_pitch_tracker(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let detected = detect_pitch_hz(&app.scope_buf, SAMPLE_RATE);

    let avail_w = ui.available_width().max(60.0);
    let avail_h = ui.available_height().max(40.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(avail_w, avail_h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::Rounding::same(2.0), egui::Color32::from_gray(8));

    let (note_name, cents_off, conf) = match detected {
        None => ("—".to_string(), 0.0_f32, 0.0_f32),
        Some((hz, c)) => {
            let midi_f = hz_to_midi(hz);
            let nearest = midi_f.round() as i32;
            let cents = (midi_f - nearest as f32) * 100.0;
            let pc = nearest.rem_euclid(12) as usize;
            let octave = (nearest / 12) - 1;
            (format!("{}{}", PITCH_CLASS_NAMES[pc], octave), cents, c)
        }
    };

    // Big note label, centred vertically high in the card.
    painter.text(
        egui::Pos2::new(rect.center().x, rect.top() + rect.height() * 0.32),
        egui::Align2::CENTER_CENTER,
        &note_name,
        egui::FontId::monospace((rect.height() * 0.35).min(28.0)),
        theme::FOG,
    );

    // Cents needle: horizontal bar bottom of the card, centred at 0,
    // ±50 cents to either edge.  Confidence shapes the brightness.
    let needle_y = rect.bottom() - 14.0;
    painter.line_segment(
        [
            egui::Pos2::new(rect.left() + 6.0, needle_y),
            egui::Pos2::new(rect.right() - 6.0, needle_y),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
    );
    let cx = rect.center().x;
    painter.line_segment(
        [
            egui::Pos2::new(cx, needle_y - 4.0),
            egui::Pos2::new(cx, needle_y + 4.0),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
    );
    if detected.is_some() {
        let nx = cx + (cents_off.clamp(-50.0, 50.0) / 50.0) * (rect.width() * 0.45);
        let bright = (60.0 + 180.0 * conf) as u8;
        painter.circle_filled(
            egui::Pos2::new(nx, needle_y),
            3.0,
            egui::Color32::from_gray(bright),
        );
        painter.text(
            egui::Pos2::new(rect.right() - 4.0, rect.top() + 3.0),
            egui::Align2::RIGHT_TOP,
            format!("{:+.0}c", cents_off),
            egui::FontId::monospace(7.5),
            egui::Color32::from_gray(140),
        );
    }
}

// ─── Chord / key display ─────────────────────────────────────────────────────

/// Spectrum → chroma vector → triad-template match → rendered chord name.
/// Detects only major/minor triads; sevenths and extensions are out of
/// scope for V1 (the templates would need to grow + the false-positive
/// rate climbs sharply on a 4-bin template).
pub fn draw_chord_display(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let spectrum = if app.scope_buf.len() >= 256 {
        Some(compute_spectrum(&app.scope_buf, SAMPLE_RATE))
    } else {
        None
    };

    let avail_w = ui.available_width().max(60.0);
    let avail_h = ui.available_height().max(40.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(avail_w, avail_h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::Rounding::same(2.0), egui::Color32::from_gray(8));

    let mut chord_label = String::from("—");
    let mut conf_text = String::new();
    let mut chroma: Option<[f32; 12]> = None;

    if let Some(spec) = spectrum.as_ref() {
        let c = chroma_from_spectrum(&spec.magnitudes, spec.bin_hz);
        chroma = Some(c);
        if let Some((root, kind, conf)) = detect_chord(&c) {
            let suffix = match kind {
                ChordKind::Major => "",
                ChordKind::Minor => "m",
            };
            chord_label = format!("{}{}", PITCH_CLASS_NAMES[root as usize], suffix);
            conf_text = format!("{:.0}%", conf * 100.0);
        }
    }

    // Chord label, centred upper-third.
    painter.text(
        egui::Pos2::new(rect.center().x, rect.top() + rect.height() * 0.32),
        egui::Align2::CENTER_CENTER,
        &chord_label,
        egui::FontId::monospace((rect.height() * 0.32).min(24.0)),
        theme::FOG,
    );
    if !conf_text.is_empty() {
        painter.text(
            egui::Pos2::new(rect.right() - 4.0, rect.top() + 3.0),
            egui::Align2::RIGHT_TOP,
            &conf_text,
            egui::FontId::monospace(7.5),
            egui::Color32::from_gray(140),
        );
    }

    // Chroma bars across the bottom — 12 thin vertical bars showing the
    // current pitch-class distribution.  Lets the user see why a chord
    // was (or wasn't) picked.
    if let Some(c) = chroma {
        let strip_top = rect.top() + rect.height() * 0.62;
        let strip_bot = rect.bottom() - 4.0;
        let strip_h = (strip_bot - strip_top).max(6.0);
        let bar_w = (rect.width() - 12.0) / 12.0;
        for (i, &v) in c.iter().enumerate() {
            let bx = rect.left() + 6.0 + i as f32 * bar_w;
            let h = v.clamp(0.0, 1.0) * strip_h;
            let bar = egui::Rect::from_min_max(
                egui::Pos2::new(bx + 1.0, strip_bot - h),
                egui::Pos2::new(bx + bar_w - 1.0, strip_bot),
            );
            let g = (40.0 + 180.0 * v.clamp(0.0, 1.0)) as u8;
            painter.rect_filled(bar, egui::Rounding::ZERO, egui::Color32::from_gray(g));
        }
    }
}
