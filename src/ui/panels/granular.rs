// ─── ui/panels/granular.rs ───────────────────────────────────────────────────
// Granular texture voice panel — density, grain size, position, jitter, pitch scatter.
//
// Same sample-picker workflow as the AmenSampler panel: scans a dedicated
// directory, offers a combo box + GET / RND / LD buttons, shows an empty-
// state prompt linking to archive.org / freesound when no samples are
// present.

use crate::audio::{AudioCommand, load_wav_to_44100};
use crate::ui::{ImpulseApp, theme, widgets};

/// Directory scanned for user-dropped texture WAVs.
const TEXTURES_DIR: &str = "samples/textures";

/// Link the empty-state GET button opens.  Archive.org's opensource_audio
/// collection isn't texture-specific but has a lot of grain-friendly
/// material; we also point the hover tooltip at Freesound (which doesn't
/// open well from a browser-launch button because it needs JS).
const TEXTURES_ARCHIVE_URL: &str = "https://archive.org/details/opensource_audio";

/// Scan `samples/textures/` for .wav files, sorted by name.  Empty Vec
/// means the dir is missing or contains no WAVs.
pub fn scan_texture_samples() -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = std::fs::read_dir(TEXTURES_DIR)
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

/// Pick a random WAV from samples/textures/.  Returns None if empty.
/// Uses the same time-nanos modulo pick as the amen panel.
pub fn pick_random_texture() -> Option<String> {
    let samples = scan_texture_samples();
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

/// Load a WAV from disk and push its samples to the audio thread's
/// granular voice buffer.  Also writes the path into GranularState.
fn load_and_push(app: &mut ImpulseApp, path: &str) {
    if let Some(data) = load_wav_to_44100(path) {
        let _ = app.audio_tx.push(AudioCommand::LoadGranular(data));
        log::info!("Granular: loaded '{}'", path);
    } else {
        log::warn!("Granular: could not load '{}'", path);
    }
    app.state.write().granular.path = path.to_string();
}

pub fn draw_granular(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let mut path = app.state.read().granular.path.clone();

    // ── Sample picker ────────────────────────────────────────────────────────
    // Same three-state pattern as the AmenSampler panel: empty dir → hint +
    // GET button; populated dir → combo + RND + LD.  LD reloads the current
    // selection so you can iterate on a sample in an external editor.
    let samples = scan_texture_samples();
    if samples.is_empty() {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("No samples in samples/textures/")
                    .monospace()
                    .size(8.0)
                    .color(theme::ASH),
            );
            if ui
                .small_button(egui::RichText::new("GET").monospace().size(7.0))
                .on_hover_text(
                    "Open archive.org/details/opensource_audio.\n\
                     For more texture-friendly material try freesound.org —\n\
                     search drone / pad / texture / field / reverb tail.\n\
                     Drop .wav files into samples/textures/.",
                )
                .clicked()
            {
                let _ = crate::ui::util::webbrowser_open(TEXTURES_ARCHIVE_URL);
            }
        });
    } else {
        ui.horizontal(|ui| {
            let current_name = std::path::Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("(pick sample)")
                .to_string();
            egui::ComboBox::from_id_source("granular_sample_picker")
                .width((ui.available_width() - 70.0).max(60.0))
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
                            load_and_push(app, &path);
                        }
                    }
                });
            if ui
                .small_button(egui::RichText::new("RANDOM").monospace().size(7.0))
                .on_hover_text("Load a random sample from samples/textures/")
                .clicked()
                && let Some(rand_path) = pick_random_texture()
            {
                path = rand_path.clone();
                load_and_push(app, &rand_path);
            }
            if ui
                .small_button(egui::RichText::new("LOAD").monospace().size(7.0))
                .on_hover_text("Reload the selected sample from disk")
                .clicked()
            {
                let p = path.clone();
                load_and_push(app, &p);
            }
        });
    }

    // Auto-reload if the path changed from outside (API, LLM, style).  A
    // simple equality check against a ctx-cached "last loaded" avoids
    // hammering the disk on every frame.
    let mem_id = egui::Id::new("granular_last_loaded_path");
    let last_loaded: String = ui
        .ctx()
        .data(|d| d.get_temp::<String>(mem_id))
        .unwrap_or_default();
    if !path.is_empty() && last_loaded != path {
        load_and_push(app, &path);
        ui.ctx().data_mut(|d| d.insert_temp(mem_id, path.clone()));
    }

    // ── Live master-output ring buffer + CAPTURE button ─────────────────────
    // Drain any samples the audio thread has produced since the last frame
    // into our local ring (granular_tap / granular_tap_head).  Then render
    // a compact min/max strip with a moving "head" cursor so the user can
    // see what's currently in the buffer before clicking CAPTURE.
    let tap_len = app.granular_tap.len();
    while let Ok(s) = app.granular_capture_rx.pop() {
        let h = app.granular_tap_head;
        app.granular_tap[h] = s;
        app.granular_tap_head = (h + 1) % tap_len;
    }
    // Bigger waveform strip (66 px — matches the amen panel) so the
    // ring buffer is actually legible.  Same dark-rect styling +
    // 0.5 stroke so the look is consistent with the amen display.
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(260.0), 66.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        egui::Rounding::same(2.0),
        egui::Color32::from_gray(10),
    );
    painter.rect_stroke(
        rect,
        egui::Rounding::same(2.0),
        egui::Stroke::new(0.5, egui::Color32::from_gray(30)),
    );
    // Column-wise min/max over the ring buffer, starting at the oldest
    // sample (head) so the display scrolls left→right in chronological
    // order.  200 columns across ~3 s of audio = ~15 ms per column.
    let cols = (rect.width().max(1.0) as usize).min(260);
    if cols > 0 && tap_len > 0 {
        let samples_per_col = tap_len / cols;
        let mid = rect.center().y;
        let half_h = rect.height() * 0.45;
        for c in 0..cols {
            let (mut mn, mut mx) = (0.0_f32, 0.0_f32);
            let col_start = c * samples_per_col;
            for k in 0..samples_per_col {
                // Read ring-relative: (head + col_start + k) % len
                let idx = (app.granular_tap_head + col_start + k) % tap_len;
                let s = app.granular_tap[idx];
                if s < mn {
                    mn = s;
                }
                if s > mx {
                    mx = s;
                }
            }
            let x = rect.min.x + c as f32 + 0.5;
            painter.line_segment(
                [
                    egui::pos2(x, mid - mx * half_h),
                    egui::pos2(x, mid - mn * half_h),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_gray(140)),
            );
        }
    }
    // Head cursor — always at the right edge (freshest sample arrives here).
    painter.line_segment(
        [
            egui::pos2(rect.max.x - 0.5, rect.min.y + 1.0),
            egui::pos2(rect.max.x - 0.5, rect.max.y - 1.0),
        ],
        egui::Stroke::new(1.0, theme::CHALK),
    );
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(33));

    // CAPTURE row
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("LIVE {:.1}s", tap_len as f32 / 44100.0))
                .monospace()
                .size(7.5)
                .color(theme::SMOKE),
        );
        if ui
            .small_button(egui::RichText::new("CAPTURE").monospace().size(7.5))
            .on_hover_text(
                "Freeze the ring buffer (the last few seconds of master\n\
                 output) as the granular source.  In-memory only — no\n\
                 file written.  Click again to re-capture.",
            )
            .clicked()
        {
            // Re-order so slot 0 = oldest sample, then push to the audio thread.
            let mut out: Vec<f32> = Vec::with_capacity(tap_len);
            let h = app.granular_tap_head;
            out.extend_from_slice(&app.granular_tap[h..]);
            out.extend_from_slice(&app.granular_tap[..h]);
            let arc = std::sync::Arc::new(out);
            let _ = app.audio_tx.push(AudioCommand::LoadGranular(arc));
            // Set a synthetic path so the panel doesn't auto-reload from disk.
            let label = "«captured»".to_string();
            app.state.write().granular.path = label.clone();
            ui.ctx().data_mut(|d| d.insert_temp(mem_id, label));
        }
    });

    // ── Knobs ────────────────────────────────────────────────────────────────
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);

    let (mut vol, mut density, mut grain_size, mut position, mut jitter, mut pitch, mut spray) = {
        let s = app.state.read();
        (
            s.granular.volume,
            s.granular.density,
            s.granular.grain_size,
            s.granular.position,
            s.granular.position_jitter,
            s.granular.pitch_scatter,
            s.granular.spray,
        )
    };
    let mut changed = false;
    ui.horizontal(|ui| {
        if widgets::param_control(ui, "VOLUME", &mut vol, pm("granular.volume"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "DENSITY", &mut density, pm("granular.density"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "SIZE", &mut grain_size, pm("granular.grain_size"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "POSITION", &mut position, pm("granular.position"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(
            ui,
            "JITTER",
            &mut jitter,
            pm("granular.position_jitter"),
            ctrl,
        )
        .0
        {
            changed = true;
        }
        if widgets::param_control(ui, "PITCH", &mut pitch, pm("granular.pitch_scatter"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "SPRAY", &mut spray, pm("granular.spray"), ctrl).0 {
            changed = true;
        }
    });
    if changed {
        {
            let mut s = app.state.write();
            s.granular.volume = vol;
            s.granular.density = density;
            s.granular.grain_size = grain_size;
            s.granular.position = position;
            s.granular.position_jitter = jitter;
            s.granular.pitch_scatter = pitch;
            s.granular.spray = spray;
        }
        app.push_audio_params();
        app.observe_edits(&[
            ("granular.volume", vol),
            ("granular.density", density),
            ("granular.grain_size", grain_size),
            ("granular.position", position),
            ("granular.pitch_scatter", pitch),
            ("granular.spray", spray),
        ]);
    }
}
