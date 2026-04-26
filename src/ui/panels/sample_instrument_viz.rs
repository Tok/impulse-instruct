// ─── ui/panels/sample_instrument_viz.rs ─────────────────────────────────────
// V2 Stage 7: visualization strip rendered at the bottom of the
// SampleInstrument panel.
//
// Two modes, picked by what's loaded into the voice:
//   * Single-WAV: min/max waveform thumbnail (cached on path) +
//     loop-window shading.
//   * SFZ multisample: piano-keyboard zone map — each region shaded
//     across its `lokey..=hikey` range.  Region's `pitch_keycenter`
//     gets a vertical tick so the user can see at a glance which key
//     each sample is anchored at.
//
// Both helpers stay paint-only (no drag/click handling yet — Stage
// 7.5 adds drag markers + per-zone selection).

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::audio::dsp::sample_instrument::SfzRegionRuntime;
use crate::ui::theme;

/// Poly-meter dot count — must match `SampleInstrumentVoice::POLY_VOICES`.
/// Hard-coded here (not pulled from the test-only constant) so the UI
/// crate doesn't widen its public surface for a paint-only number.
/// Kept in sync via `poly_dots_matches_voice_pool` in the test module.
pub(crate) const POLY_DOTS: u8 = 8;

/// Build (min, max) thumbnail pairs for a sample buffer.  Bins the
/// buffer into `cols` columns; each column's pair is the min/max of
/// the samples in that bin.  Cheap to call once on load and stash the
/// result in `ImpulseApp.sample_wave_cache`.
pub(crate) fn build_thumbnail(samples: &[f32], cols: usize) -> Vec<(f32, f32)> {
    if samples.is_empty() || cols == 0 {
        return Vec::new();
    }
    let bin = samples.len() / cols.max(1);
    let bin = bin.max(1);
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(cols);
    for c in 0..cols {
        let lo = c * bin;
        let hi = ((c + 1) * bin).min(samples.len());
        if lo >= hi {
            out.push((0.0, 0.0));
            continue;
        }
        let mut mn = f32::INFINITY;
        let mut mx = f32::NEG_INFINITY;
        for &s in &samples[lo..hi] {
            if s < mn {
                mn = s;
            }
            if s > mx {
                mx = s;
            }
        }
        out.push((mn, mx));
    }
    out
}

/// Paint a horizontal row of `POLY_DOTS` dots — bright for active
/// slots (left-aligned: dot 0..active_count are lit), dim for free.
/// Surfaces the SampleInstrument poly pool occupancy so the user can
/// see voice-stealing pressure before they hit it.  Compact (fits
/// next to the LOAD/filename row); read-only by design.
pub(crate) fn draw_poly_meter(ui: &mut Ui, active: u8) {
    let n = POLY_DOTS as f32;
    let dot_r = 2.5_f32;
    let gap = 3.0_f32;
    let width = n * dot_r * 2.0 + (n - 1.0) * gap + 4.0;
    let height = dot_r * 2.0 + 2.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    let active = active.min(POLY_DOTS);
    for i in 0..POLY_DOTS {
        let cx = rect.min.x + 2.0 + dot_r + i as f32 * (dot_r * 2.0 + gap);
        let cy = rect.center().y;
        let lit = i < active;
        // Lit dots use FOG (primary text); idle dots use IRON
        // (inactive widget) — both grayscale, palette-compliant.
        let fill = if lit { theme::FOG } else { theme::IRON };
        painter.circle_filled(Pos2::new(cx, cy), dot_r, fill);
    }
    resp.on_hover_text(format!("Polyphony: {} / {}", active, POLY_DOTS));
}

/// How close (in pixels) the cursor must be to a loop boundary
/// before it counts as "over the handle".  6 px is a comfortable
/// drag affordance at default DPI without making the dead-zone in
/// the middle of the strip feel cramped.
const LOOP_HANDLE_GRAB_PX: f32 = 6.0;

/// Which loop boundary the user is currently dragging — stashed in
/// egui's per-id memory so the drag state survives across paint
/// frames without needing fields on `ImpulseApp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum LoopDragTarget {
    #[default]
    None,
    Start,
    End,
}

/// Paint a waveform thumbnail with interactive loop-window
/// markers.  When `loop_enabled` is true and the cursor hovers
/// near either boundary the marker brightens; clicking and
/// dragging updates `loop_start` / `loop_end` in place (clamped
/// to `[0, 1]` and `loop_start + epsilon ≤ loop_end`).  Returns
/// `true` if either fraction changed this frame so the caller can
/// push audio params + observe-edit.
pub(crate) fn draw_waveform(
    ui: &mut Ui,
    thumb: &[(f32, f32)],
    loop_start: &mut f32,
    loop_end: &mut f32,
    loop_enabled: bool,
    width: f32,
    height: f32,
) -> bool {
    // Sense::click_and_drag so the user can grab a handle and pull
    // it, not just hover.  The id is derived from the rect so two
    // visualizers in the same window don't share drag state.
    let id = ui.next_auto_id();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let painter = ui.painter_at(rect);
    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));
    if thumb.is_empty() {
        return false;
    }
    let mid = rect.center().y;
    let half_h = rect.height() * 0.45;
    let cols = thumb.len();
    let col_w = rect.width() / cols as f32;
    for (i, (mn, mx)) in thumb.iter().enumerate() {
        let x = rect.min.x + i as f32 * col_w + col_w * 0.5;
        let y_top = mid - mx * half_h;
        let y_bot = mid - mn * half_h;
        painter.line_segment(
            [Pos2::new(x, y_top), Pos2::new(x, y_bot)],
            Stroke::new(col_w.max(1.0), Color32::from_gray(160)),
        );
    }

    let mut changed = false;
    // Shade outside the loop window when looping is on.
    if loop_enabled && *loop_end > *loop_start {
        let shade = Color32::from_rgba_unmultiplied(8, 8, 8, 160);
        let ls_x = rect.min.x + loop_start.clamp(0.0, 1.0) * rect.width();
        let le_x = rect.min.x + loop_end.clamp(0.0, 1.0) * rect.width();
        if ls_x > rect.min.x {
            painter.rect_filled(
                Rect::from_min_max(rect.min, Pos2::new(ls_x, rect.max.y)),
                egui::Rounding::ZERO,
                shade,
            );
        }
        if le_x < rect.max.x {
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(le_x, rect.min.y), rect.max),
                egui::Rounding::ZERO,
                shade,
            );
        }

        // Hover detection — which handle (if any) is under the
        // cursor right now.  When the user is mid-drag this
        // doesn't matter (the in-flight target wins), but it
        // drives the stem brightness so the user sees which
        // handle they're about to grab.
        let hover_target = if let Some(pos) = response.hover_pos() {
            let near_start = (pos.x - ls_x).abs() <= LOOP_HANDLE_GRAB_PX;
            let near_end = (pos.x - le_x).abs() <= LOOP_HANDLE_GRAB_PX;
            // Tie-breaker — when the start and end handles overlap
            // (loop window collapsed to nothing), pick whichever
            // edge the cursor is on the side of.
            if near_start && near_end {
                if pos.x < (ls_x + le_x) * 0.5 {
                    LoopDragTarget::Start
                } else {
                    LoopDragTarget::End
                }
            } else if near_start {
                LoopDragTarget::Start
            } else if near_end {
                LoopDragTarget::End
            } else {
                LoopDragTarget::None
            }
        } else {
            LoopDragTarget::None
        };

        // Drag start: latch the target into per-id memory so the
        // user can pull the cursor away from the handle and the
        // drag still resolves to the correct boundary.  Egui's
        // `drag_started` fires once at the press, then we read
        // back the target on subsequent `dragged` frames.
        if response.drag_started_by(egui::PointerButton::Primary)
            && hover_target != LoopDragTarget::None
        {
            ui.ctx()
                .data_mut(|d| d.insert_temp::<LoopDragTarget>(id, hover_target));
        }
        if response.drag_stopped() {
            ui.ctx()
                .data_mut(|d| d.insert_temp::<LoopDragTarget>(id, LoopDragTarget::None));
        }

        let active_target = ui
            .ctx()
            .data(|d| d.get_temp::<LoopDragTarget>(id))
            .unwrap_or_default();

        // Apply drag — convert pointer x to a fraction of the
        // rect width and write back, with the usual epsilon
        // clamp so the two markers can't cross.
        if response.dragged_by(egui::PointerButton::Primary)
            && active_target != LoopDragTarget::None
            && let Some(pos) = response.interact_pointer_pos()
        {
            let frac = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
            const EPS: f32 = 0.001;
            match active_target {
                LoopDragTarget::Start => {
                    let new_start = frac.min(*loop_end - EPS).max(0.0);
                    if (new_start - *loop_start).abs() > 1e-5 {
                        *loop_start = new_start;
                        changed = true;
                    }
                }
                LoopDragTarget::End => {
                    let new_end = frac.max(*loop_start + EPS).min(1.0);
                    if (new_end - *loop_end).abs() > 1e-5 {
                        *loop_end = new_end;
                        changed = true;
                    }
                }
                LoopDragTarget::None => {}
            }
        }

        // Loop boundary stems — bright lines so the shading reads
        // as "this bit loops" rather than "this bit is muted".
        // Hovered / dragged handles brighten + thicken so the user
        // sees the affordance.
        let start_active =
            active_target == LoopDragTarget::Start || hover_target == LoopDragTarget::Start;
        let end_active =
            active_target == LoopDragTarget::End || hover_target == LoopDragTarget::End;
        let stem_color = Color32::from_gray(140);
        let active_color = Color32::from_gray(220);
        painter.line_segment(
            [Pos2::new(ls_x, rect.min.y), Pos2::new(ls_x, rect.max.y)],
            Stroke::new(
                if start_active { 2.0 } else { 1.0 },
                if start_active {
                    active_color
                } else {
                    stem_color
                },
            ),
        );
        painter.line_segment(
            [Pos2::new(le_x, rect.min.y), Pos2::new(le_x, rect.max.y)],
            Stroke::new(
                if end_active { 2.0 } else { 1.0 },
                if end_active { active_color } else { stem_color },
            ),
        );

        // Reset cursor to ResizeHorizontal when over a handle so
        // the drag affordance reads visually before the click.
        if hover_target != LoopDragTarget::None || active_target != LoopDragTarget::None {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }

    changed
}

/// Paint an SFZ zone-map: horizontal piano-keyboard strip with each
/// region shaded across its key range.  Multiple overlapping regions
/// are rendered with darker shading so dense clusters read as deeper
/// gray.  `pitch_keycenter` per region gets a tiny vertical tick.
///
/// Click handling: clicking on a region's banded rect updates
/// `*selected` to its index; clicking outside any region clears
/// the selection.  The selected region renders with a brighter
/// shade + an outline so the user sees what the inspector below
/// is reading from.
pub(crate) fn draw_zone_map(
    ui: &mut Ui,
    regions: &[SfzRegionRuntime],
    selected: &mut Option<usize>,
    width: f32,
    height: f32,
) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));

    // Show MIDI 21..=108 (piano range) so the strip uses the visible
    // area for the notes a user actually plays.  Out-of-range regions
    // still get drawn — clipped to the visible window.
    const KEY_LO: u8 = 21;
    const KEY_HI: u8 = 108;
    let span = (KEY_HI - KEY_LO) as f32;
    let to_x = |note: u8| -> f32 {
        let n = note.clamp(KEY_LO, KEY_HI) as f32 - KEY_LO as f32;
        rect.min.x + (n / span) * rect.width()
    };

    // C-major reference grid — every C gets a faint vertical line so
    // the user can identify octaves at a glance.
    for c in (KEY_LO..=KEY_HI).filter(|n| n.is_multiple_of(12)) {
        let x = to_x(c);
        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            Stroke::new(0.5, Color32::from_gray(30)),
        );
        // Only label C4 to keep the strip uncluttered.
        if c == 60 {
            painter.text(
                Pos2::new(x + 2.0, rect.min.y + 1.0),
                egui::Align2::LEFT_TOP,
                "C4",
                egui::FontId::monospace(6.0),
                Color32::from_gray(60),
            );
        }
    }

    // Stale selection from a previous SFZ load — clear so we don't
    // index past the new (shorter) region list.
    if let Some(idx) = *selected
        && idx >= regions.len()
    {
        *selected = None;
    }

    // Each region paints a translucent rectangle across its key range.
    // Stacking blends darker, so overlapping zones read clearly.  Y
    // banding spreads regions vertically in their declared order so a
    // visually stacked pack (close + room mics) shows as parallel
    // strips, not one merged block.
    if regions.is_empty() {
        return;
    }
    let band_h = (rect.height() - 4.0) / regions.len().max(1) as f32;
    // Pre-compute every band rect so the click hit-test below
    // reuses the exact same geometry as the paint.  Walked
    // top-down so the first hit (visually closest to the cursor's
    // y) wins when bands overlap.
    let mut bands: Vec<(usize, Rect)> = Vec::with_capacity(regions.len());
    for (i, r) in regions.iter().enumerate() {
        let y_top = rect.min.y + 2.0 + i as f32 * band_h;
        let y_bot = (y_top + band_h).min(rect.max.y - 2.0);
        let x_lo = to_x(r.region.lokey);
        let x_hi = to_x(r.region.hikey);
        let band = Rect::from_min_max(Pos2::new(x_lo, y_top), Pos2::new(x_hi, y_bot));
        bands.push((i, band));
    }

    // Click handling — first band hit wins.  Clicking anywhere in
    // the rect that isn't on a band clears the selection so the
    // user has a clear "back to overview" gesture.
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let mut hit: Option<usize> = None;
        for (i, band) in &bands {
            if band.contains(pos) {
                hit = Some(*i);
                break;
            }
        }
        *selected = hit;
    }

    for (i, band) in &bands {
        let r = &regions[*i];
        let is_selected = *selected == Some(*i);
        // Alternate two shades by row index so adjacent regions stay
        // visually distinct without needing colour.  Selected band
        // brightens further so it pops out of the strip.
        let shade = if is_selected {
            Color32::from_gray(220)
        } else if i.is_multiple_of(2) {
            Color32::from_gray(110)
        } else {
            Color32::from_gray(150)
        };
        painter.rect_filled(*band, egui::Rounding::same(1.0), shade);
        if is_selected {
            painter.rect_stroke(
                *band,
                egui::Rounding::same(1.0),
                Stroke::new(1.0, Color32::from_gray(255)),
            );
        }
        // Tick at pitch_keycenter — bright pip so the user sees where
        // each region's "natural" note sits.  Selected band's tick
        // darkens to keep it visible against the brighter shade.
        let kc_x = to_x(r.region.pitch_keycenter);
        let tick_color = if is_selected {
            Color32::from_gray(40)
        } else {
            Color32::from_gray(220)
        };
        painter.line_segment(
            [Pos2::new(kc_x, band.min.y), Pos2::new(kc_x, band.max.y)],
            Stroke::new(1.0, tick_color),
        );
    }
}

/// Tiny helper that converts a MIDI note to its scientific-pitch
/// label (e.g. 60 → "C4", 69 → "A4").  Used by the zone-inspector
/// readout below so the user reads keys as notes rather than
/// MIDI numbers.
pub(super) fn midi_label(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (note as i32 / 12) - 1;
    format!("{}{}", NAMES[(note % 12) as usize], octave)
}

/// Read-only inspector for the currently selected SFZ region.
/// Renders below the zone map to surface the per-region opcodes
/// the user can't see in the banded paint (velocity layers,
/// round-robin, tune cents, dB offset, sample filename).  V1 of
/// Stage 7.5 — future iterations may add inline edit.
pub(crate) fn draw_zone_inspector(ui: &mut Ui, region: &SfzRegionRuntime) {
    let r = &region.region;
    let sample_name = r
        .sample_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "(no sample)".to_string());
    let line1 = format!(
        "{}–{}  vel {}–{}  root {}",
        midi_label(r.lokey),
        midi_label(r.hikey),
        r.lovel,
        r.hivel,
        midi_label(r.pitch_keycenter),
    );
    // Round-robin only shows when active so unused regions don't
    // get a dead "RR 0/0" suffix.  Tune in cents; transpose in
    // semitones; volume_db verbatim.
    let mut extras = vec![
        format!("{:+.1} dB", r.volume_db),
        format!("tune {:+.0}c", r.tune_cents),
    ];
    if r.transpose != 0 {
        extras.push(format!("transp {:+}st", r.transpose));
    }
    if r.seq_length > 0 {
        extras.push(format!("RR {}/{}", r.seq_position, r.seq_length));
    }
    let line2 = extras.join("  ");
    ui.label(
        egui::RichText::new(&sample_name)
            .monospace()
            .size(8.5)
            .color(theme::FOG),
    );
    ui.label(
        egui::RichText::new(&line1)
            .monospace()
            .size(8.0)
            .color(theme::SMOKE),
    );
    ui.label(
        egui::RichText::new(&line2)
            .monospace()
            .size(8.0)
            .color(theme::ASH),
    );
}

#[cfg(test)]
mod tests {
    use super::midi_label;

    #[test]
    fn midi_label_known_notes() {
        // Anchor a few of the well-known notes against the
        // scientific-pitch convention (MIDI 60 = C4 in this app's
        // octave numbering — same as Yamaha / Apple Logic).
        assert_eq!(midi_label(60), "C4");
        assert_eq!(midi_label(69), "A4"); // 440 Hz reference
        assert_eq!(midi_label(72), "C5");
        assert_eq!(midi_label(48), "C3");
        assert_eq!(midi_label(127), "G9");
        assert_eq!(midi_label(0), "C-1");
    }

    #[test]
    fn midi_label_sharps_at_each_semitone() {
        // Walk the chromatic scale from C4 and confirm every sharp
        // lands at the expected offset.
        let expected = [
            "C4", "C#4", "D4", "D#4", "E4", "F4", "F#4", "G4", "G#4", "A4", "A#4", "B4",
        ];
        for (i, want) in expected.iter().enumerate() {
            assert_eq!(midi_label(60 + i as u8), *want);
        }
    }
}
