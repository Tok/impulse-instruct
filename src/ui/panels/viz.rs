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

/// Render an LFO slot's output waveform across one cycle.
///
/// V2 (rack-cable aware): walk the rack cable graph for an incoming
/// CV cable from any `LfoModule` to *this* `LfoScope` instance and
/// render that module's slot.  When no cable is patched, fall back
/// to the V1 behaviour ("first enabled slot") so an unwired scope
/// still shows something useful — and so older sessions saved
/// before the cable wiring landed keep displaying their LFO.
///
/// Slot index for an `LfoModule` is its positional rank among
/// `LfoModule` modules in `rack.modules` order — same rule used by
/// `rack_canvas` when it publishes the `egui::Id::new("lfo_slot")`
/// temp data, so the back-panel label and the scope agree.
pub fn draw_lfo_scope(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    use crate::audio::dsp::fx_math::lfo_value_at;

    let (slots, cable_slot) = {
        let s = app.state.read();
        let slot = lfo_slot_from_cables(&s, module_id);
        (s.lfo, slot)
    };
    let active_slot = match cable_slot {
        Some(idx) if idx < slots.len() => Some((idx, &slots[idx])),
        _ => slots.iter().enumerate().find(|(_, s)| s.enabled),
    };

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

/// Walk `state.rack.cables` for an incoming CV cable from any
/// `LfoModule` to the `LfoScope` at `scope_id`, and return that
/// source module's slot index (its positional rank among `LfoModule`
/// instances in `rack.modules`).  Returns `None` when no cable is
/// patched, when the source isn't an `LfoModule`, or when the source
/// id doesn't resolve — caller then falls back to the V1
/// "first enabled slot" picker.
///
/// Pure read over `&AppState` — no borrows of egui temp data — so
/// the function is callable from anywhere with a state snapshot,
/// including future code paths outside the UI thread.
pub fn lfo_slot_from_cables(state: &crate::state::AppState, scope_id: u32) -> Option<usize> {
    use crate::state::{ModuleKind, PortKind};

    // Find the first incoming CV cable to this scope.  Multiple
    // cables would be ambiguous; V2 takes the first by cable
    // insertion order, which matches the rack canvas's draw order
    // and the user's mental model of "the cable I just patched in".
    let src_id = state
        .rack
        .cables
        .iter()
        .find(|c| {
            c.to.module_id == scope_id && c.to.kind == PortKind::Cv && c.from.kind == PortKind::Cv
        })
        .map(|c| c.from.module_id)?;

    // Resolve the source's slot index by counting LfoModule
    // instances up to (and including) it in rack order.  Same rule
    // as `rack_canvas.rs`'s `egui::Id::new("lfo_slot").with(m.id)`
    // publication so the back-panel "LFO 1/2/3" labels and the
    // scope output stay consistent.
    let mut slot_idx = 0usize;
    for m in &state.rack.modules {
        if m.kind != ModuleKind::LfoModule {
            continue;
        }
        if m.id == src_id {
            return Some(slot_idx);
        }
        slot_idx += 1;
    }
    None
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

// ─── Spectrogram waterfall ───────────────────────────────────────────────────

/// Render the rolling FFT history as a 2D heatmap: time on X (newest on the
/// right), log-frequency on Y (low at the bottom).  Each magnitude is mapped
/// from −96..0 dB to a brightness gradient.
pub fn draw_spectrogram(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let avail_w = ui.available_width().max(80.0);
    let avail_h = ui.available_height().max(40.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(avail_w, avail_h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::Rounding::same(2.0), egui::Color32::from_gray(8));

    let history = &app.spectrogram_history;
    if history.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "(no audio)",
            egui::FontId::monospace(8.0),
            egui::Color32::from_gray(60),
        );
        return;
    }

    // Determine bin_hz from the most recent frame's length — every frame is
    // FFT_SIZE/2 magnitudes wide, so bin_hz = sr/FFT_SIZE.  We don't store
    // bin_hz alongside each frame to keep the history struct small.
    let n_frames = history.len();
    let frame_len = history.back().map(|f| f.len()).unwrap_or(0).max(1);
    let bin_hz = crate::audio::SAMPLE_RATE / (2.0 * frame_len as f32).max(1.0);

    // Each pixel column corresponds to one history frame; pad with empty
    // columns if history is shorter than the canvas.
    let cols = (avail_w as usize).max(8);
    let rows = (avail_h as usize).max(8);

    // Build a fresh ColorImage each repaint — small enough that the
    // allocation cost is acceptable, and avoids a long-lived texture
    // that would have to be invalidated on resize.
    let mut img = egui::ColorImage::new([cols, rows], egui::Color32::from_gray(8));

    for px_x in 0..cols {
        // Map screen-x to history-frame index (newest column on the right).
        let frame_pos = px_x as f32 / cols as f32;
        let frame_idx = ((frame_pos * n_frames as f32) as usize).min(n_frames - 1);
        let frame = &history[frame_idx];
        if frame.is_empty() {
            continue;
        }

        for px_y in 0..rows {
            // Log-frequency axis: bottom = 30 Hz, top = 16 kHz.
            let v = 1.0 - (px_y as f32 / rows as f32);
            let f = 30.0 * (16_000.0_f32 / 30.0).powf(v);
            let bin = (f / bin_hz).round() as usize;
            if bin >= frame.len() {
                continue;
            }
            let db = frame[bin];
            // dBFS → 0..1 brightness (-90 dB = black, 0 dB = white-ish).
            let bright = ((db + 90.0) / 90.0).clamp(0.0, 1.0);
            // Slight green bias for readability.
            let g = (bright * 220.0) as u8;
            let r = (bright * bright * 200.0) as u8;
            let b = ((bright * 0.5).powf(1.5) * 200.0) as u8;
            img.pixels[px_y * cols + px_x] = egui::Color32::from_rgb(r, g, b);
        }
    }

    let tex = ui.ctx().load_texture(
        format!("spectrogram_{}", rect.left() as i32 + rect.top() as i32),
        img,
        egui::TextureOptions::NEAREST,
    );
    painter.image(
        tex.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::new(0.0, 0.0), egui::Pos2::new(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

// ─── Loudness / LUFS meter ───────────────────────────────────────────────────

/// Twin LUFS readouts — momentary (400 ms) on the left, short-term (3 s) on
/// the right — with a horizontal scale from −36..0 LUFS.  Numbers in
/// monospace; level bar widths driven by the same scale so the visual
/// meter and the readout stay in lockstep.
pub fn draw_loudness_meter(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let m = app.lufs_meter.momentary_lufs();
    let s = app.lufs_meter.short_term_lufs();

    let avail_w = ui.available_width().max(80.0);
    let avail_h = ui.available_height().max(40.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(avail_w, avail_h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::Rounding::same(2.0), egui::Color32::from_gray(8));

    // Helper: map LUFS in [-36, 0] → 0..1 normalised bar fill.
    let bar_norm = |lufs: f32| -> f32 { ((lufs + 36.0) / 36.0).clamp(0.0, 1.0) };

    let row_h = ((rect.height() - 12.0) / 2.0).max(8.0);
    for (i, (label, value, col)) in [
        ("M", m, egui::Color32::from_gray(180)),
        ("S", s, egui::Color32::from_gray(140)),
    ]
    .iter()
    .enumerate()
    {
        let row_top = rect.top() + 4.0 + (i as f32 * (row_h + 2.0));
        let bar_left = rect.left() + 18.0;
        let bar_right = rect.right() - 36.0;
        let bar_y0 = row_top + 2.0;
        let bar_y1 = row_top + row_h - 2.0;
        // Track.
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::Pos2::new(bar_left, bar_y0),
                egui::Pos2::new(bar_right, bar_y1),
            ),
            egui::Rounding::ZERO,
            egui::Color32::from_gray(20),
        );
        // Fill.
        let fill_x = bar_left + (bar_right - bar_left) * bar_norm(*value);
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::Pos2::new(bar_left, bar_y0),
                egui::Pos2::new(fill_x, bar_y1),
            ),
            egui::Rounding::ZERO,
            *col,
        );
        // M/S label, left.
        painter.text(
            egui::Pos2::new(rect.left() + 4.0, (bar_y0 + bar_y1) * 0.5),
            egui::Align2::LEFT_CENTER,
            *label,
            egui::FontId::monospace(9.0),
            theme::FOG,
        );
        // LUFS value, right.
        let txt = if *value > -110.0 {
            format!("{:>5.1}", *value)
        } else {
            "  -inf".to_string()
        };
        painter.text(
            egui::Pos2::new(rect.right() - 4.0, (bar_y0 + bar_y1) * 0.5),
            egui::Align2::RIGHT_CENTER,
            &txt,
            egui::FontId::monospace(9.0),
            theme::FOG,
        );
    }
}

// ─── Transport phase wheel ───────────────────────────────────────────────────

/// Circular bar/beat indicator.  The wheel divides the cycle into
/// `state.sequencer.steps` ticks, with longer ticks every 4 steps for
/// beat boundaries.  A radial pointer moves around the rim at the
/// current step + sub-step fraction.
pub fn draw_phase_wheel(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let s = app.state.read();
    let steps = s.sequencer.steps.max(1);
    let cur = s.sequencer.current_step.min(steps - 1);
    drop(s);

    let avail_w = ui.available_width().max(48.0);
    let avail_h = ui.available_height().max(48.0);
    let side = avail_w.min(avail_h);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(side, side), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::Rounding::same(2.0), egui::Color32::from_gray(8));

    let centre = rect.center();
    let radius = (side * 0.5) - 4.0;

    // Outer ring.
    painter.circle_stroke(
        centre,
        radius,
        egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
    );

    // Tick marks.  Beat ticks (every 4 steps) drawn longer + brighter.
    for i in 0..steps {
        let theta =
            -std::f32::consts::FRAC_PI_2 + (i as f32 / steps as f32) * std::f32::consts::TAU;
        let is_beat = i % 4 == 0;
        let r0 = radius - (if is_beat { 8.0 } else { 5.0 });
        let r1 = radius;
        let p0 = egui::Pos2::new(centre.x + theta.cos() * r0, centre.y + theta.sin() * r0);
        let p1 = egui::Pos2::new(centre.x + theta.cos() * r1, centre.y + theta.sin() * r1);
        let g = if is_beat { 160 } else { 70 };
        painter.line_segment(
            [p0, p1],
            egui::Stroke::new(if is_beat { 1.5 } else { 1.0 }, egui::Color32::from_gray(g)),
        );
    }

    // Pointer at the current step (no sub-step interpolation in V1 — the
    // step counter advances at audio-block granularity, which already feels
    // smooth at typical UI rates).
    let theta_cur =
        -std::f32::consts::FRAC_PI_2 + (cur as f32 / steps as f32) * std::f32::consts::TAU;
    let p_outer = egui::Pos2::new(
        centre.x + theta_cur.cos() * (radius - 2.0),
        centre.y + theta_cur.sin() * (radius - 2.0),
    );
    painter.line_segment(
        [centre, p_outer],
        egui::Stroke::new(2.0, egui::Color32::from_gray(220)),
    );
    painter.circle_filled(p_outer, 3.0, egui::Color32::from_gray(220));

    // Centre readout: current step number + total steps.
    painter.text(
        centre,
        egui::Align2::CENTER_CENTER,
        format!("{}/{}", cur + 1, steps),
        egui::FontId::monospace((radius * 0.32).min(13.0)),
        theme::FOG,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, ModuleKind, PortDir, PortKind, PortRef, RackModule, RackState};

    /// Build an empty rack with `kinds` added in order, IDs 1..=N.
    /// Cables are not added — caller patches them with `connect`.
    fn rack_with(kinds: &[ModuleKind]) -> RackState {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: (kinds.len() as u32) + 1,
            dyn_sequencer_rows: None,
        };
        for (i, &k) in kinds.iter().enumerate() {
            rack.modules.push(RackModule::new((i as u32) + 1, k));
        }
        rack
    }

    fn cv_cable(from_id: u32, to_id: u32) -> (PortRef, PortRef) {
        (
            PortRef {
                module_id: from_id,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            },
            PortRef {
                module_id: to_id,
                dir: PortDir::In,
                kind: PortKind::Cv,
                index: 0,
            },
        )
    }

    #[test]
    fn lfo_slot_from_cables_returns_none_when_no_cable() {
        // No cables patched → fall back to the V1 picker.
        let rack = rack_with(&[ModuleKind::LfoModule, ModuleKind::LfoScope]);
        let mut s = AppState::default();
        s.rack = rack;
        // Scope id is 2 (added second).
        assert_eq!(lfo_slot_from_cables(&s, 2), None);
    }

    #[test]
    fn lfo_slot_from_cables_picks_first_lfo_when_only_one_present() {
        // Single LFO → slot 0.  Cable: LfoModule(1) → LfoScope(2).
        let mut rack = rack_with(&[ModuleKind::LfoModule, ModuleKind::LfoScope]);
        let (a, b) = cv_cable(1, 2);
        assert!(rack.connect(a, b));
        let mut s = AppState::default();
        s.rack = rack;
        assert_eq!(lfo_slot_from_cables(&s, 2), Some(0));
    }

    #[test]
    fn lfo_slot_from_cables_uses_positional_rank_for_second_lfo() {
        // Two LfoModules; cable from the second → scope.  Slot
        // index counts LfoModules in rack order, so the second LFO
        // is slot 1.
        let mut rack = rack_with(&[
            ModuleKind::LfoModule, // id=1, slot 0
            ModuleKind::LfoModule, // id=2, slot 1
            ModuleKind::LfoScope,  // id=3
        ]);
        let (a, b) = cv_cable(2, 3);
        assert!(rack.connect(a, b));
        let mut s = AppState::default();
        s.rack = rack;
        assert_eq!(lfo_slot_from_cables(&s, 3), Some(1));
    }

    #[test]
    fn lfo_slot_from_cables_ignores_non_lfo_sources() {
        // Cable from StepSequencer → scope is also a CV cable but
        // the source isn't an LfoModule.  V2 ignores it; the V1
        // picker takes over.  Same for non-CV cables.
        let mut rack = rack_with(&[
            ModuleKind::StepSequencer,
            ModuleKind::LfoModule,
            ModuleKind::LfoScope,
        ]);
        let (a, b) = cv_cable(1, 3); // seq → scope; not an LfoModule source
        assert!(rack.connect(a, b));
        let mut s = AppState::default();
        s.rack = rack;
        assert_eq!(lfo_slot_from_cables(&s, 3), None);
    }

    #[test]
    fn lfo_slot_from_cables_takes_first_when_multiple_lfo_cables() {
        // Two cables land on the scope; V2 takes the first by
        // cable insertion order (matches the rack canvas's draw
        // order and the user's mental model of "the cable I
        // patched first wins").
        let mut rack = rack_with(&[
            ModuleKind::LfoModule, // id=1, slot 0
            ModuleKind::LfoModule, // id=2, slot 1
            ModuleKind::LfoScope,  // id=3
        ]);
        let (a1, b1) = cv_cable(2, 3); // patched first → slot 1 wins
        let (a2, b2) = cv_cable(1, 3);
        assert!(rack.connect(a1, b1));
        assert!(rack.connect(a2, b2));
        let mut s = AppState::default();
        s.rack = rack;
        assert_eq!(lfo_slot_from_cables(&s, 3), Some(1));
    }
}
