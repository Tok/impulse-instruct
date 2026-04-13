// ─── ui/panels/drums.rs ───────────────────────────────────────────────────────
// Drum kit panels: Kit A (808-style), Kit B (909-style), and Amen sampler.

use super::PAN_SLIDER_W;
use crate::audio::{AudioCommand, load_wav_to_44100};
use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_kit_a(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // Snapshot all values before any widget rendering
    let (
        mut kp,
        mut kd,
        mut kpu,
        mut kv,
        mut kped,
        mut kpet,
        mut kclip,
        mut kpan_a,
        mut st,
        mut ssn,
        mut sd,
        mut sv,
        mut hcd,
        mut hod,
        mut hv,
    ) = {
        let s = app.state.read();
        (
            s.kit_a.kick.pitch,
            s.kit_a.kick.decay,
            s.kit_a.kick.punch,
            s.kit_a.kick.volume,
            s.kit_a.kick.pitch_env_depth,
            s.kit_a.kick.pitch_env_time,
            s.kit_a.kick.clip,
            s.kit_a.kick.pan,
            s.kit_a.snare.tone,
            s.kit_a.snare.snappy,
            s.kit_a.snare.decay,
            s.kit_a.snare.volume,
            s.kit_a.hihat_closed.decay,
            s.kit_a.hihat_open.decay,
            s.kit_a.hihat_closed.volume,
        )
    };
    let mut changed = false;

    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let xy_size = app.state.read().ui_prefs.effective_xy_px();
    let avail = ui.available_width();
    let gw = ((avail - super::GLASS_GAP) / 2.0).floor(); // 2-column layout
    let group_h = ctrl.knob_size * 2.0 + 50.0;

    // PAN slider — right-justified
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pan_slider(ui, &mut kpan_a, PAN_SLIDER_W) {
                changed = true;
            }
            ui.label(
                egui::RichText::new("PAN")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(8.0),
            );
        });
    });

    // Row 1: KICK (left) + KICK XY PAD (right)
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = super::GLASS_GAP;
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("KICK")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "PITCH", &mut kp, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "DECAY", &mut kd, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "PUNCH", &mut kpu, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "LEVEL", &mut kv, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "P.DPT", &mut kped, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "P.TIM", &mut kpet, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "CLIP", &mut kclip, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
        });
        // KICK XY PAD (right column, padded)
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.set_min_height(group_h);
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("KICK: PITCH × DECAY")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(8.0),
            );
            // Constrain pad to fit within the cell with padding
            let pad_size = (gw - 40.0).min(xy_size).min(group_h - 30.0);
            if widgets::xy_pad(
                ui,
                "drums_kick_xy",
                "PITCH",
                "DECAY",
                &mut kp,
                &mut kd,
                pad_size,
                false,
                1,
            )
            .0
            {
                let mut s = app.state.write();
                s.kit_a.kick.pitch = kp;
                s.kit_a.kick.decay = kd;
                drop(s);
                app.push_audio_params();
            }
        });
    });

    ui.add_space(super::GLASS_GAP);

    // Row 2: SNARE (left) + HIHAT (right)
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = super::GLASS_GAP;
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("SNARE")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "TONE", &mut st, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "SNAPPY", &mut ssn, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "DECAY", &mut sd, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "LEVEL", &mut sv, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("HIHAT")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "CLOSED", &mut hcd, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "OPEN", &mut hod, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "LEVEL", &mut hv, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
        });
    });

    // Single brief write with all changes
    if changed {
        let mut s = app.state.write();
        s.kit_a.kick.pitch = kp;
        s.kit_a.kick.decay = kd;
        s.kit_a.kick.punch = kpu;
        s.kit_a.kick.volume = kv;
        s.kit_a.kick.pitch_env_depth = kped;
        s.kit_a.kick.pitch_env_time = kpet;
        s.kit_a.kick.clip = kclip;
        s.kit_a.kick.pan = kpan_a;
        s.kit_a.snare.tone = st;
        s.kit_a.snare.snappy = ssn;
        s.kit_a.snare.decay = sd;
        s.kit_a.snare.volume = sv;
        s.kit_a.hihat_closed.decay = hcd;
        s.kit_a.hihat_open.decay = hod;
        s.kit_a.hihat_closed.volume = hv;
        s.kit_a.hihat_open.volume = hv;
        drop(s);
        app.push_audio_params();
        app.observe_edits(&[
            ("kit_a.kick.pitch", kp),
            ("kit_a.kick.decay", kd),
            ("kit_a.kick.punch", kpu),
            ("kit_a.kick.volume", kv),
            ("kit_a.snare.tone", st),
            ("kit_a.snare.snappy", ssn),
            ("kit_a.snare.decay", sd),
            ("kit_a.snare.volume", sv),
            ("kit_a.hihat_closed.decay", hcd),
            ("kit_a.hihat_closed.volume", hv),
        ]);
    }
}

pub fn draw_kit_b(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (
        mut kp,
        mut kd,
        mut kpu,
        mut kv,
        mut kped,
        mut kpet,
        mut kclip,
        mut kpan_b,
        mut st,
        mut ssn,
        mut sd,
        mut sv,
        mut cd,
        mut cv,
    ) = {
        let s = app.state.read();
        (
            s.kit_b.kick.pitch,
            s.kit_b.kick.decay,
            s.kit_b.kick.punch,
            s.kit_b.kick.volume,
            s.kit_b.kick.pitch_env_depth,
            s.kit_b.kick.pitch_env_time,
            s.kit_b.kick.clip,
            s.kit_b.kick.pan,
            s.kit_b.snare.tone,
            s.kit_b.snare.snappy,
            s.kit_b.snare.decay,
            s.kit_b.snare.volume,
            s.kit_b.clap.decay,
            s.kit_b.clap.volume,
        )
    };
    let mut changed = false;

    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let ctrl_big = ctrl.phi_bigger(); // larger knobs for the important KICK params
    let avail = ui.available_width();
    let gw_half = ((avail - super::GLASS_GAP) / 2.0).floor();

    // PAN slider — right-justified
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pan_slider(ui, &mut kpan_b, PAN_SLIDER_W) {
                changed = true;
            }
            ui.label(
                egui::RichText::new("PAN")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(8.0),
            );
        });
    });

    // Row 1: KICK — full width, bigger knobs (most important for 909)
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
        ui.label(
            egui::RichText::new("KICK")
                .color(theme::FOG)
                .monospace()
                .size(9.5),
        );
        widgets::centered_row(ui, |ui| {
            if widgets::param_control(ui, "PITCH", &mut kp, ParamMode::Free, ctrl_big).0 {
                changed = true;
            }
            if widgets::param_control(ui, "DECAY", &mut kd, ParamMode::Free, ctrl_big).0 {
                changed = true;
            }
            if widgets::param_control(ui, "PUNCH", &mut kpu, ParamMode::Free, ctrl_big).0 {
                changed = true;
            }
            if widgets::param_control(ui, "LEVEL", &mut kv, ParamMode::Free, ctrl_big).0 {
                changed = true;
            }
        });
        widgets::centered_row(ui, |ui| {
            if widgets::param_control(ui, "P.DEPTH", &mut kped, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "P.TIME", &mut kpet, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "CLIP", &mut kclip, ParamMode::Free, ctrl).0 {
                changed = true;
            }
        });
    });

    ui.add_space(super::GLASS_GAP);

    // Row 2: SNARE (left) + CLAP/RIM (right) — all knobs single row each
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = super::GLASS_GAP;
        widgets::glass_group_fill(ui, gw_half, gw_half, |ui| {
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("SNARE")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "TONE", &mut st, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "SNAPPY", &mut ssn, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "DECAY", &mut sd, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "LEVEL", &mut sv, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
        });
        widgets::glass_group_fill(ui, gw_half, gw_half, |ui| {
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("CLAP / RIM")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "DECAY", &mut cd, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "LEVEL", &mut cv, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
        });
    });

    if changed {
        let mut s = app.state.write();
        s.kit_b.kick.pitch = kp;
        s.kit_b.kick.decay = kd;
        s.kit_b.kick.punch = kpu;
        s.kit_b.kick.volume = kv;
        s.kit_b.kick.pitch_env_depth = kped;
        s.kit_b.kick.pitch_env_time = kpet;
        s.kit_b.kick.clip = kclip;
        s.kit_b.kick.pan = kpan_b;
        s.kit_b.snare.tone = st;
        s.kit_b.snare.snappy = ssn;
        s.kit_b.snare.decay = sd;
        s.kit_b.snare.volume = sv;
        s.kit_b.clap.decay = cd;
        s.kit_b.clap.volume = cv;
        drop(s);
        app.push_audio_params();
        app.observe_edits(&[
            ("kit_b.kick.pitch", kp),
            ("kit_b.kick.decay", kd),
            ("kit_b.kick.punch", kpu),
            ("kit_b.kick.volume", kv),
            ("kit_b.snare.tone", st),
            ("kit_b.snare.snappy", ssn),
            ("kit_b.snare.decay", sd),
            ("kit_b.snare.volume", sv),
            ("kit_b.clap.decay", cd),
            ("kit_b.clap.volume", cv),
        ]);
    }
}

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
fn draw_waveform(
    ui: &mut egui::Ui,
    thumb: &[(f32, f32)],
    slice_count: u8,
    start_offset: f32,
    end_offset: f32,
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
    // Slice markers: vertical lines at equal divisions within [start, end].
    let slices = slice_count.max(1);
    let region_w = (end_offset - start_offset).max(0.001) * rect.width();
    let region_x0 = rect.min.x + start_offset * rect.width();
    let slice_w = region_w / slices as f32;
    for i in 0..=slices {
        let x = region_x0 + i as f32 * slice_w;
        painter.line_segment(
            [
                egui::pos2(x, rect.min.y + 1.0),
                egui::pos2(x, rect.max.y - 1.0),
            ],
            egui::Stroke::new(0.6, egui::Color32::from_gray(80)),
        );
    }
    // Highlight the active slice.
    if let Some(a) = active_slice
        && a < slices
    {
        let x0 = region_x0 + a as f32 * slice_w;
        let x1 = x0 + slice_w;
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
    // Reverse arrow indicator in the hub
    let arrow_col = theme::ASH;
    let hub_label = if reverse { "◁" } else { "▷" };
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        hub_label,
        egui::FontId::proportional(size * 0.28),
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
            egui::ComboBox::from_id_source("amen_sample_picker")
                .width(ui.available_width() - 30.0)
                .selected_text(egui::RichText::new(current_name).monospace().size(8.0))
                .show_ui(ui, |ui| {
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
            if ui
                .small_button(egui::RichText::new("LD").monospace().size(7.0))
                .on_hover_text("Reload the selected sample from disk")
                .clicked()
            {
                let p = path.clone();
                load_and_cache(app, &p);
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

    // ── Waveform display ─────────────────────────────────────────────────────
    // Rebuild the thumbnail if the path changed since our last cache.
    if !path.is_empty()
        && app.amen_wave_cache.0 != path
        && let Some(data) = load_wav_to_44100(&path)
    {
        app.amen_wave_cache = (path.clone(), build_wave_thumb(&data, 256));
    }
    let active = current_amen_slice(app);
    if !app.amen_wave_cache.1.is_empty() {
        let wave_w = ui.available_width().min(260.0);
        draw_waveform(
            ui,
            &app.amen_wave_cache.1,
            slice_count,
            start_offset,
            end_offset,
            active,
            wave_w,
            44.0,
        );
    }

    // ── Slice wheel + reverse indicator ──────────────────────────────────────
    ui.horizontal(|ui| {
        draw_slice_wheel(ui, slice_count, active, reverse, loop_mode, 56.0);
        ui.vertical(|ui| {
            // SLICES selector (cycles through 1/2/4/8/16)
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("SLICES")
                        .monospace()
                        .size(7.5)
                        .color(theme::SMOKE),
                );
                for &n in &[1u8, 2, 4, 8, 16] {
                    let active_sel = slice_count == n;
                    let col = if active_sel {
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
            // REV / LOOP toggles
            ui.horizontal(|ui| {
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
        });
    });

    // ── Sliders row 1: volume / pitch ────────────────────────────────────────
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

    // ── Sliders row 2: region offsets ────────────────────────────────────────
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

    // ── Sliders row 3: gate + stutter ────────────────────────────────────────
    ui.horizontal(|ui| {
        if widgets::param_control(ui, "GATE", &mut gate, ParamMode::Free, ctrl).0 {
            changed = true;
        }
        let mut stutter_norm = stutter as f32 / 4.0;
        if widgets::param_control(ui, "STUTTER", &mut stutter_norm, ParamMode::Free, ctrl).0 {
            stutter = (stutter_norm * 4.0).round().clamp(0.0, 4.0) as u8;
            changed = true;
        }
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
        drop(s);
        app.push_audio_params();
    }
}
