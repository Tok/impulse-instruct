// ─── ui/panels/amen.rs ────────────────────────────────────────────────────────
// AmenSampler panel — break-chopper UI.  Extracted from drums.rs to keep
// that file under the 1000-line limit.
//
// Layout top to bottom:
//   sample picker (combo + GET / RND / LD)  → sample metadata strip
//   waveform thumbnail + slice markers      → AUTO / RESET
//   slice wheel + SLICES / REV / LOOP       → volume/pitch sliders
//   start/end offset sliders                → gate/stutter sliders

use crate::audio::{AudioCommand, load_wav_to_44100};
use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

// ─── Amen / WAV sampler panel ─────────────────────────────────────────────────

/// Scan `samples/amen/` for .wav files and return their paths sorted by name.
/// Empty Vec means the directory is missing or contains no WAVs.
fn scan_amen_samples() -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = std::fs::read_dir("samples/amen")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        })
        .collect();
    out.sort();
    out
}

/// Internet Archive collection used by the "Get samples" button.
const AMEN_ARCHIVE_URL: &str = "https://archive.org/details/amen-breaks";

/// Pick a random WAV path from `samples/amen/`, or return None if empty.
/// Uses a simple time-based pseudo-random pick; good enough for
/// "surprise me" without an RNG dep.
pub fn pick_random_sample() -> Option<String> {
    let samples = scan_amen_samples();
    if samples.is_empty() {
        return None;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    let idx = nanos % samples.len();
    samples.get(idx).map(|p| p.to_string_lossy().to_string())
}

/// Load a WAV from disk, push its samples to the audio thread, cache
/// header metadata into AmenState.meta, and build a waveform thumbnail
/// for the panel display.
fn load_and_cache(app: &mut ImpulseApp, path: &str) {
    if let Some(data) = load_wav_to_44100(path) {
        // Rebuild the waveform thumbnail before handing the Arc over —
        // cheaper than reading the file again from the UI thread.
        app.amen_wave_cache = (path.to_string(), build_wave_thumb(&data, 256));
        let _ = app.audio_tx.push(AudioCommand::LoadSampler(data));
    }
    let meta = crate::audio::read_wav_meta(path);
    app.state.write().amen.meta = meta;
}

/// Downsample a mono sample buffer into `n_cols` min/max pairs for cheap
/// waveform rendering.  Returns empty if `samples` is empty.
fn build_wave_thumb(samples: &[f32], n_cols: usize) -> Vec<(f32, f32)> {
    if samples.is_empty() || n_cols == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(n_cols);
    let step = samples.len() as f32 / n_cols as f32;
    for i in 0..n_cols {
        let a = (i as f32 * step) as usize;
        let b = (((i + 1) as f32) * step) as usize;
        let end = b.min(samples.len()).max(a + 1);
        let slice = &samples[a..end];
        let mut mn = 0.0_f32;
        let mut mx = 0.0_f32;
        for &s in slice {
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

/// Paint the waveform thumbnail into `rect` with slice-boundary markers
/// and start/end offset shading.  `active_slice` (if any) highlights the
/// currently-playing slice wedge.
#[allow(clippy::too_many_arguments)]
fn draw_waveform(
    ui: &mut egui::Ui,
    thumb: &[(f32, f32)],
    slice_count: u8,
    start_offset: f32,
    end_offset: f32,
    slice_positions: &[f32],
    active_slice: Option<u8>,
    width: f32,
    height: f32,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        egui::Rounding::same(2.0),
        egui::Color32::from_gray(10),
    );
    painter.rect_stroke(
        rect,
        egui::Rounding::same(2.0),
        egui::Stroke::new(0.5, egui::Color32::from_gray(40)),
    );
    if thumb.is_empty() {
        return;
    }
    let mid = rect.center().y;
    let half_h = rect.height() * 0.45;
    let col_count = thumb.len();
    let col_w = rect.width() / col_count as f32;
    // Waveform columns.
    for (i, (mn, mx)) in thumb.iter().enumerate() {
        let x = rect.min.x + i as f32 * col_w + col_w * 0.5;
        let y_top = mid - mx * half_h;
        let y_bot = mid - mn * half_h;
        painter.line_segment(
            [egui::pos2(x, y_top), egui::pos2(x, y_bot)],
            egui::Stroke::new(col_w.max(1.0), egui::Color32::from_gray(160)),
        );
    }
    // Start/end offset shading — dim the parts of the sample outside
    // the usable region.
    let shade = egui::Color32::from_rgba_unmultiplied(8, 8, 8, 180);
    if start_offset > 0.0 {
        let x0 = rect.min.x;
        let x1 = rect.min.x + start_offset * rect.width();
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, rect.min.y), egui::pos2(x1, rect.max.y)),
            egui::Rounding::ZERO,
            shade,
        );
    }
    if end_offset < 1.0 {
        let x0 = rect.min.x + end_offset * rect.width();
        let x1 = rect.max.x;
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, rect.min.y), egui::pos2(x1, rect.max.y)),
            egui::Rounding::ZERO,
            shade,
        );
    }
    // Slice markers.  If slice_positions is populated, draw markers at
    // those exact times (custom / transient-detected).  Otherwise draw
    // equal divisions within [start_offset, end_offset].
    let slices = slice_count.max(1);
    let region_w = (end_offset - start_offset).max(0.001) * rect.width();
    let region_x0 = rect.min.x + start_offset * rect.width();
    let slice_w_equal = region_w / slices as f32;
    let use_custom = !slice_positions.is_empty();
    let marker_color = if use_custom {
        // Slightly brighter for user/detected markers so they're distinct.
        egui::Color32::from_gray(140)
    } else {
        egui::Color32::from_gray(80)
    };
    if use_custom {
        for &p in slice_positions {
            let x = rect.min.x + p.clamp(0.0, 1.0) * rect.width();
            painter.line_segment(
                [
                    egui::pos2(x, rect.min.y + 1.0),
                    egui::pos2(x, rect.max.y - 1.0),
                ],
                egui::Stroke::new(0.8, marker_color),
            );
        }
        // Also the trailing end-offset line so the last slice has a cap.
        let x_end = rect.min.x + end_offset * rect.width();
        painter.line_segment(
            [
                egui::pos2(x_end, rect.min.y + 1.0),
                egui::pos2(x_end, rect.max.y - 1.0),
            ],
            egui::Stroke::new(0.8, marker_color),
        );
    } else {
        for i in 0..=slices {
            let x = region_x0 + i as f32 * slice_w_equal;
            painter.line_segment(
                [
                    egui::pos2(x, rect.min.y + 1.0),
                    egui::pos2(x, rect.max.y - 1.0),
                ],
                egui::Stroke::new(0.6, marker_color),
            );
        }
    }
    // Highlight the active slice.
    if let Some(a) = active_slice
        && a < slices
    {
        let (x0, x1) = if use_custom {
            let a_x = slice_positions
                .get(a as usize)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let next = slice_positions
                .get(a as usize + 1)
                .copied()
                .unwrap_or(end_offset)
                .clamp(0.0, 1.0);
            (
                rect.min.x + a_x * rect.width(),
                rect.min.x + next * rect.width(),
            )
        } else {
            let x0 = region_x0 + a as f32 * slice_w_equal;
            (x0, x0 + slice_w_equal)
        };
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x0, rect.min.y + 1.0),
                egui::pos2(x1, rect.max.y - 1.0),
            ),
            egui::Rounding::ZERO,
            egui::Color32::from_rgba_unmultiplied(200, 200, 200, 30),
        );
    }
    // Center zero-line.
    painter.line_segment(
        [egui::pos2(rect.min.x, mid), egui::pos2(rect.max.x, mid)],
        egui::Stroke::new(0.3, egui::Color32::from_gray(45)),
    );
}

/// Draw the circular slice-wheel visualization — an actual loop made of
/// N wedges (one per slice).  The currently-playing slice is highlighted
/// based on the sequencer's last-fired amen step.
fn draw_slice_wheel(
    ui: &mut egui::Ui,
    slice_count: u8,
    active_slice: Option<u8>,
    reverse: bool,
    looping: bool,
    size: f32,
) {
    let (rect, _resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let r_outer = size * 0.46;
    let r_inner = size * 0.22;

    let n = slice_count.max(1) as usize;
    let tau = std::f32::consts::TAU;
    // Start at 12 o'clock, clockwise → angle0 = -π/2.
    let dir = if reverse { -1.0 } else { 1.0 };

    // Wedge fills
    for i in 0..n {
        let a0 = -std::f32::consts::FRAC_PI_2 + (i as f32 / n as f32) * tau * dir;
        let a1 = -std::f32::consts::FRAC_PI_2 + ((i + 1) as f32 / n as f32) * tau * dir;
        let active = active_slice.map(|s| s as usize) == Some(i);
        let fill = if active {
            egui::Color32::from_gray(180)
        } else {
            egui::Color32::from_gray(40)
        };
        // Tessellate the wedge as a triangle fan around the center.
        let steps = 12;
        let mut pts = vec![center];
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let a = a0 + (a1 - a0) * t;
            pts.push(center + egui::vec2(a.cos(), a.sin()) * r_outer);
        }
        painter.add(egui::Shape::convex_polygon(
            pts,
            fill,
            egui::Stroke::new(0.5, egui::Color32::from_gray(15)),
        ));
    }
    // Inner hole
    painter.circle_filled(center, r_inner, egui::Color32::from_gray(12));
    // Outer ring
    painter.circle_stroke(
        center,
        r_outer,
        egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
    );
    // Loop indicator: second ring just outside when loop_mode is on
    if looping {
        painter.circle_stroke(center, r_outer + 3.0, egui::Stroke::new(1.0, theme::CHALK));
    }
    // Direction arrow in the hub (black pointing glyphs render reliably
    // in egui's default font — the earlier open-pointer chars showed as
    // a fallback square on some systems).
    let arrow_col = theme::ASH;
    let hub_label = if reverse { "◄" } else { "►" };
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        hub_label,
        egui::FontId::monospace(size * 0.22),
        arrow_col,
    );
}

/// Find the slice index of the most recently-played amen step.
/// Returns None if no step or the drum isn't playing.
fn current_amen_slice(app: &ImpulseApp) -> Option<u8> {
    let s = app.state.read();
    if !s.sequencer.running {
        return None;
    }
    let steps = s.sequencer.steps.max(1);
    let cur = s.sequencer.current_step % steps;
    let pat = s
        .sequencer
        .drum_patterns
        .get(&crate::state::DrumVoice::Amen)?;
    // Walk backwards from current_step to find the last active step.
    for off in 0..steps {
        let idx = (cur + steps - off) % steps;
        if let Some(step) = pat.get(idx)
            && step.active
        {
            let slices = s.amen.slice_count.max(1);
            // Slice 0 means auto — we can't know the DSP's auto counter, but
            // approximate by using (loop_count * pattern hits + pos_in_loop)
            // ≈ current_step mod slice_count so the wheel still animates.
            let resolved = if step.slice == 0 {
                (idx as u8) % slices
            } else {
                (step.slice - 1) % slices
            };
            return Some(resolved);
        }
    }
    None
}

pub fn draw_amen(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let mut path = app.state.read().amen.path.clone();
    let (
        mut vol,
        mut pitch,
        mut loop_mode,
        mut slice_count,
        mut start_offset,
        mut end_offset,
        mut reverse,
        mut gate,
        mut stutter,
        mut source_bpm,
        mut bpm_stretch,
        meta,
    ) = {
        let s = app.state.read();
        (
            s.amen.volume,
            s.amen.pitch,
            s.amen.loop_mode,
            s.amen.slice_count,
            s.amen.start_offset,
            s.amen.end_offset,
            s.amen.reverse,
            s.amen.gate,
            s.amen.stutter,
            s.amen.source_bpm,
            s.amen.bpm_stretch,
            s.amen.meta.clone(),
        )
    };
    let mut changed = false;

    // ── Sample picker ────────────────────────────────────────────────────────
    let samples = scan_amen_samples();
    if samples.is_empty() {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("No samples in samples/amen/")
                    .monospace()
                    .size(8.0)
                    .color(theme::ASH),
            );
            if ui
                .small_button(egui::RichText::new("GET").monospace().size(7.0))
                .on_hover_text(format!(
                    "Open {}\nDownload a .zip of WAVs and extract into samples/amen/",
                    AMEN_ARCHIVE_URL
                ))
                .clicked()
            {
                let _ = crate::ui::util::webbrowser_open(AMEN_ARCHIVE_URL);
            }
        });
    } else {
        ui.horizontal(|ui| {
            let current_name = std::path::Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("(pick sample)")
                .to_string();
            // Leave room for RND + LD buttons (two small buttons ~48px total
            // plus item_spacing).  Using a fixed cap keeps the picker from
            // pushing outside the 3-cell module width on narrow rack layouts.
            egui::ComboBox::from_id_source("amen_sample_picker")
                // Room for RND + LD + PLAY (3 small buttons).
                .width((ui.available_width() - 96.0).max(60.0))
                .selected_text(egui::RichText::new(current_name).monospace().size(8.0))
                .show_ui(ui, |ui| {
                    // Cap the popup height so long sample-pack dirs don't
                    // overflow the rack/panel bounds (the user reported
                    // entries getting cut off).  200px ≈ 14 rows at 8pt.
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for sp in &samples {
                                let sp_str = sp.to_string_lossy().to_string();
                                let name = sp
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or(sp_str.as_str())
                                    .to_string();
                                if ui
                                    .selectable_label(
                                        path == sp_str,
                                        egui::RichText::new(name).monospace().size(8.0),
                                    )
                                    .clicked()
                                {
                                    path = sp_str;
                                    app.state.write().amen.path = path.clone();
                                    load_and_cache(app, &path);
                                }
                            }
                        });
                });
            if ui
                .small_button(egui::RichText::new("RND").monospace().size(7.0))
                .on_hover_text("Load a random sample from samples/amen/")
                .clicked()
                && let Some(rand_path) = pick_random_sample()
            {
                path = rand_path.clone();
                app.state.write().amen.path = rand_path.clone();
                load_and_cache(app, &rand_path);
            }
            if ui
                .small_button(egui::RichText::new("LD").monospace().size(7.0))
                .on_hover_text("Reload the selected sample from disk")
                .clicked()
            {
                let p = path.clone();
                load_and_cache(app, &p);
            }
            // PLAY — one-shot trigger of the amen voice through the
            // normal DSP trigger pipeline, using current slice/gate/
            // stutter settings.  Useful for auditioning without the
            // sequencer running.
            if ui
                .small_button(egui::RichText::new("▶").monospace().size(7.5))
                .on_hover_text(
                    "Trigger the sample once (uses current slice, gate,\n\
                     stutter, reverse, and BPM-stretch settings).\n\
                     Slice 0 = auto-advance.",
                )
                .clicked()
            {
                use crate::audio::AudioCommand;
                use crate::sequencer::TriggerEvent;
                use crate::state::DrumVoice;
                let _ = app
                    .audio_tx
                    .push(AudioCommand::Trigger(TriggerEvent::DrumTrigger {
                        voice: DrumVoice::Amen,
                        velocity: 1.0,
                        slice: 0,
                    }));
            }
        });
    }

    // ── Metadata strip ───────────────────────────────────────────────────────
    if let Some(m) = meta {
        let sec = m.samples as f32 / 44100.0;
        let size_kb = m.file_bytes as f32 / 1024.0;
        let info = format!(
            "{:.2}s  {}ch  {}-bit  {:.1}kHz  {:.0}KB",
            sec,
            m.channels,
            m.bits,
            m.src_rate as f32 / 1000.0,
            size_kb
        );
        ui.label(
            egui::RichText::new(info)
                .monospace()
                .size(7.0)
                .color(theme::ASH),
        );
    } else if !path.is_empty() {
        ui.label(
            egui::RichText::new("no metadata")
                .monospace()
                .size(7.0)
                .color(theme::PIT),
        );
    }

    // ── Waveform display + auto-slicing controls ────────────────────────────
    // If the path changed since we last synced (user picked a new file, or
    // the API / LLM wrote amen.path), fully reload: push samples to the
    // audio thread, refresh the metadata cache, rebuild the thumbnail.
    if !path.is_empty() && app.amen_wave_cache.0 != path {
        load_and_cache(app, &path);
    }
    let active = current_amen_slice(app);
    if !app.amen_wave_cache.1.is_empty() {
        let wave_w = ui.available_width().min(260.0);
        let positions_snapshot: Vec<f32> = app.state.read().amen.slice_positions.clone();
        draw_waveform(
            ui,
            &app.amen_wave_cache.1,
            slice_count,
            start_offset,
            end_offset,
            &positions_snapshot,
            active,
            wave_w,
            44.0,
        );
    }
    // AUTO detects transients and populates AmenState.slice_positions;
    // RESET clears them back to equal-division slices.  Both are only
    // meaningful with a sample loaded.
    let has_custom = !app.state.read().amen.slice_positions.is_empty();
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!path.is_empty(), |ui| {
            if ui
                .small_button(egui::RichText::new("AUTO").monospace().size(7.5))
                .on_hover_text(
                    "Energy-based transient detection.  Finds onset \
                     positions in the sample and sets them as slice \
                     boundaries.  Useful for breaks where the slices \
                     don't line up to equal divisions.",
                )
                .clicked()
                && let Some(data) = load_wav_to_44100(&path)
            {
                let positions =
                    crate::audio::onset::detect_onsets(&data, 44100.0, slice_count.max(2) as usize);
                app.state.write().amen.slice_positions = positions;
                changed = true;
            }
        });
        ui.add_enabled_ui(has_custom, |ui| {
            if ui
                .small_button(egui::RichText::new("RESET").monospace().size(7.5))
                .on_hover_text("Clear custom slice markers, back to equal divisions")
                .clicked()
            {
                app.state.write().amen.slice_positions.clear();
                changed = true;
            }
        });
        if has_custom {
            ui.label(
                egui::RichText::new("custom markers")
                    .monospace()
                    .size(7.0)
                    .color(theme::ASH),
            );
        }
    });

    // ── Slice wheel + slices/rev/loop + tempo stacked column ───────────────
    // The slice wheel sits on the left; the column to its right holds the
    // SLICES buttons, REV/LOOP toggles, and the tempo row (SRC BPM +
    // STRETCH).  Packing all three into that column frees the lower half
    // of the panel for a single knob row.
    let host_bpm = app.state.read().sequencer.bpm;
    ui.horizontal(|ui| {
        // Small padding around the wheel so it doesn't hug other widgets.
        ui.add_space(4.0);
        draw_slice_wheel(ui, slice_count, active, reverse, loop_mode, 96.0);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            // SLICES selector
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("SLICES")
                        .monospace()
                        .size(7.5)
                        .color(theme::SMOKE),
                );
                for &n in &[1u8, 2, 4, 8, 16] {
                    let col = if slice_count == n {
                        theme::CHALK
                    } else {
                        theme::IRON
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(n.to_string())
                                    .monospace()
                                    .size(7.5)
                                    .color(col),
                            )
                            .min_size(egui::vec2(14.0, 14.0)),
                        )
                        .clicked()
                    {
                        slice_count = n;
                        changed = true;
                    }
                }
            });
            // DIR (REV/FWD) + LOOP — label added to the direction toggle
            // so the state reads cleanly without needing the hub arrow.
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("DIR")
                        .monospace()
                        .size(7.5)
                        .color(theme::SMOKE),
                );
                if widgets::toggle_button(ui, if reverse { "REV" } else { "FWD" }, &mut reverse) {
                    changed = true;
                }
                if widgets::toggle_button(
                    ui,
                    if loop_mode { "LOOP" } else { "ONE" },
                    &mut loop_mode,
                ) {
                    changed = true;
                }
            });
            // SRC BPM + STRETCH — tempo-matching controls.  SYNC button
            // snaps source_bpm to the sequencer BPM (the "host" tempo)
            // so STRETCH effectively becomes a no-op until you change
            // sequencer.bpm, matching the "default synced" behavior the
            // user expects.
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("BPM")
                        .monospace()
                        .size(7.5)
                        .color(theme::SMOKE),
                );
                let mut v = source_bpm;
                if ui
                    .add(egui::DragValue::new(&mut v).range(40.0..=300.0).speed(0.5))
                    .changed()
                {
                    source_bpm = v;
                    changed = true;
                }
                if ui
                    .small_button(egui::RichText::new("=").monospace().size(7.0))
                    .on_hover_text(format!("Sync source BPM to host ({:.0})", host_bpm))
                    .clicked()
                {
                    source_bpm = host_bpm;
                    changed = true;
                }
                if widgets::toggle_button(
                    ui,
                    if bpm_stretch { "STRETCH" } else { "FREE" },
                    &mut bpm_stretch,
                ) {
                    changed = true;
                }
            });
        });
    });

    // ── Knob row — three glass-pane groups on one line ──────────────────────
    // LEVEL (vol/pitch) · REGION (start/end) · SHAPE (gate/stutter).
    // even_group_width evenly distributes the 3 panes across the panel's
    // available width, respecting the inter-group gap.
    let gw = widgets::even_group_width(ui, 3);
    ui.horizontal(|ui| {
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.horizontal(|ui| {
                if widgets::param_control(ui, "VOLUME", &mut vol, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                let mut pitch_norm = (pitch + 24.0) / 48.0;
                if widgets::param_control(ui, "PITCH", &mut pitch_norm, ParamMode::Free, ctrl).0 {
                    pitch = pitch_norm * 48.0 - 24.0;
                    changed = true;
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.horizontal(|ui| {
                if widgets::param_control(ui, "START", &mut start_offset, ParamMode::Free, ctrl).0 {
                    if start_offset >= end_offset {
                        start_offset = (end_offset - 0.01).max(0.0);
                    }
                    changed = true;
                }
                if widgets::param_control(ui, "END", &mut end_offset, ParamMode::Free, ctrl).0 {
                    if end_offset <= start_offset {
                        end_offset = (start_offset + 0.01).min(1.0);
                    }
                    changed = true;
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.horizontal(|ui| {
                if widgets::param_control(ui, "GATE", &mut gate, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                let mut stutter_norm = stutter as f32 / 4.0;
                if widgets::param_control(ui, "STUTTER", &mut stutter_norm, ParamMode::Free, ctrl).0
                {
                    stutter = (stutter_norm * 4.0).round().clamp(0.0, 4.0) as u8;
                    changed = true;
                }
            });
        });
    });

    if changed {
        let mut s = app.state.write();
        s.amen.volume = vol;
        s.amen.pitch = pitch;
        s.amen.loop_mode = loop_mode;
        s.amen.slice_count = slice_count.max(1);
        s.amen.start_offset = start_offset.clamp(0.0, 1.0);
        s.amen.end_offset = end_offset.clamp(0.0, 1.0);
        s.amen.reverse = reverse;
        s.amen.gate = gate.clamp(0.05, 1.0);
        s.amen.stutter = stutter.min(4);
        s.amen.source_bpm = source_bpm.clamp(40.0, 300.0);
        s.amen.bpm_stretch = bpm_stretch;
        drop(s);
        app.push_audio_params();
    }
}
