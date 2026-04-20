// ─── ui/panels/amen.rs ────────────────────────────────────────────────────────
// AmenSampler panel — break-chopper UI.  Extracted from drums.rs to keep
// that file under the 1000-line limit.
//
// Layout top to bottom:
//   sample picker (combo + GET / RND / LD)  → sample metadata strip
//   waveform thumbnail + slice markers      → AUTO / RESET
//   slice wheel + SLICES / REV / LOOP       → volume/pitch sliders
//   start/end offset sliders                → gate/stutter sliders

use crate::audio::{AudioCommand, SAMPLE_RATE, load_wav_to_44100};
use crate::state::{BPM_MAX, BPM_MIN, ParamMode};
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
        app.amen_ui.wave_cache = (path.to_string(), build_wave_thumb(&data, 256));
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
            // Slice 0 means auto — mirror the sequencer's effective slice
            // resolution (apply amen_slice_order if set, else linear).
            let resolved = if step.slice == 0 {
                let order = &s.sequencer.amen_slice_order;
                let raw = if order.is_empty() {
                    idx as u8
                } else {
                    order[idx % order.len()]
                };
                raw % slices
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
        mut bpm_stretch_preserve,
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
            s.amen.bpm_stretch_preserve,
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
            // Track the previous path so we can detect a dropdown
            // selection AFTER the combobox closes — the earlier
            // "selectable_label.clicked() → load inline" pattern
            // sometimes lost the click when the combobox rebuilt its
            // popup on the same frame.  Assigning selectable_value
            // directly to `path` and syncing state once after show_ui
            // is the more reliable pattern.
            let path_before = path.clone();
            egui::ComboBox::from_id_source("amen_sample_picker")
                // Room for RANDOM + LOAD + PLAY (3 buttons, wider now).
                .width((ui.available_width() - 150.0).max(60.0))
                .selected_text(egui::RichText::new(current_name).monospace().size(8.0))
                .show_ui(ui, |ui| {
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
                                ui.selectable_value(
                                    &mut path,
                                    sp_str,
                                    egui::RichText::new(name).monospace().size(8.0),
                                );
                            }
                        });
                });
            if path != path_before {
                app.state.write().amen.path = path.clone();
                load_and_cache(app, &path);
            }
            if ui
                .small_button(egui::RichText::new("RANDOM").monospace().size(7.0))
                .on_hover_text("Load a random sample from samples/amen/")
                .clicked()
                && let Some(rand_path) = pick_random_sample()
            {
                path = rand_path.clone();
                app.state.write().amen.path = rand_path.clone();
                load_and_cache(app, &rand_path);
            }
            if ui
                .small_button(egui::RichText::new("LOAD").monospace().size(7.0))
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
        let sec = m.samples as f32 / SAMPLE_RATE;
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
    if !path.is_empty() && app.amen_ui.wave_cache.0 != path {
        load_and_cache(app, &path);
    }
    let active = current_amen_slice(app);
    // Update the slice-trail history once per frame, before any
    // animation-driven rendering.  Pushes a (slice, now) entry on each
    // active-slice change and prunes entries older than half a bar.
    let host_bpm_now = app.state.read().sequencer.bpm;
    let now_t = ui.ctx().input(|i| i.time);
    let trail_window = (120.0 / host_bpm_now.max(1.0)) as f64;
    let step_dur = (15.0 / host_bpm_now.max(1.0)) as f64; // 16 steps/bar
    if active != app.amen_ui.last_trail_slice {
        if let Some(s) = active {
            app.amen_ui.slice_trail.push((s, now_t));
        }
        app.amen_ui.last_trail_slice = active;
    }
    let cutoff = now_t - trail_window;
    app.amen_ui.slice_trail.retain(|(_, t)| *t >= cutoff);
    // Ease the wheel's pointer angle toward the active slice mid-angle.
    {
        let n = slice_count.max(1) as usize;
        let dir = if reverse { -1.0 } else { 1.0 };
        if let Some(a) = active {
            let target = -std::f32::consts::FRAC_PI_2
                + ((a as f32 + 0.5) / n as f32) * std::f32::consts::TAU * dir;
            let mut delta = target - app.amen_ui.wheel_angle;
            let tau = std::f32::consts::TAU;
            while delta > std::f32::consts::PI {
                delta -= tau;
            }
            while delta < -std::f32::consts::PI {
                delta += tau;
            }
            app.amen_ui.wheel_angle += delta * 0.25;
        }
    }
    // Reserve the waveform vertical slot whether or not a sample is
    // loaded — otherwise the whole lower half of the panel shifts up
    // when loading a WAV, which jitters the knob positions.  The rect
    // just stays empty until a thumbnail is available.
    let wave_h = 66.0;
    let wave_w = ui.available_width().min(260.0);
    if !app.amen_ui.wave_cache.1.is_empty() {
        let positions_snapshot: Vec<f32> = app.state.read().amen.slice_positions.clone();
        super::amen_viz::draw_waveform(
            ui,
            &app.amen_ui.wave_cache.1,
            slice_count,
            start_offset,
            end_offset,
            &positions_snapshot,
            active,
            wave_w,
            wave_h,
            &app.amen_ui.slice_trail,
            now_t,
            step_dur,
        );
    } else {
        // Reserve the same vertical slot so the panel layout doesn't
        // shift on load.  A thin dark rect gives a visual "no sample"
        // hint without drawing anything noisy.
        let (rect, _) = ui.allocate_exact_size(egui::vec2(wave_w, wave_h), egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            egui::Rounding::same(2.0),
            egui::Color32::from_gray(10),
        );
        ui.painter().rect_stroke(
            rect,
            egui::Rounding::same(2.0),
            egui::Stroke::new(0.5, egui::Color32::from_gray(30)),
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
                let positions = crate::audio::onset::detect_onsets(
                    &data,
                    SAMPLE_RATE,
                    slice_count.max(2) as usize,
                );
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
    // Fixed label column so SLICES / DIR / BPM align even though the
    // words differ in length.  "SLICES" is the longest at 6 chars.
    let label_col = 48.0_f32;
    let lbl = |ui: &mut egui::Ui, text: &str| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(label_col, 14.0), egui::Sense::hover());
        ui.painter().text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::monospace(7.5),
            theme::SMOKE,
        );
    };
    ui.horizontal(|ui| {
        // Small padding around the wheel so it doesn't hug other widgets.
        ui.add_space(6.0);
        super::amen_viz::draw_slice_wheel(
            ui,
            slice_count,
            active,
            reverse,
            loop_mode,
            144.0,
            &app.amen_ui.slice_trail,
            now_t,
            trail_window,
            app.amen_ui.wheel_angle,
        );
        ui.add_space(8.0);
        ui.vertical(|ui| {
            // SLICES selector
            ui.horizontal(|ui| {
                lbl(ui, "SLICES");
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
                lbl(ui, "DIR");
                if widgets::toggle_button(
                    ui,
                    if reverse { "REVERSE" } else { "FORWARD" },
                    &mut reverse,
                ) {
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
                lbl(ui, "BPM");
                let mut v = source_bpm;
                if ui
                    .add(
                        egui::DragValue::new(&mut v)
                            .range(BPM_MIN..=BPM_MAX)
                            .speed(0.5),
                    )
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
                // Pitch-preserving toggle — only meaningful while
                // STRETCH is on.  PITCH=preserve (granular path), TUNE=
                // classic resample (pitch shifts with tempo).  The
                // granular path has no crossfade yet, so splice clicks
                // at strong stretch ratios are a known v1 trade-off.
                let mut preserve_ui = bpm_stretch_preserve;
                ui.add_enabled_ui(bpm_stretch, |ui| {
                    if widgets::toggle_button(
                        ui,
                        if preserve_ui { "PITCH" } else { "TUNE" },
                        &mut preserve_ui,
                    ) {
                        bpm_stretch_preserve = preserve_ui;
                        changed = true;
                    }
                })
                .response
                .on_hover_text(if preserve_ui {
                    "Pitch-preserving granular stretch: timing follows the \
                     host BPM while pitch stays at the source's original.  \
                     Splice clicks at strong ratios (v1)."
                } else {
                    "Classic resample stretch: the whole sample is pitched \
                     along with the tempo change (move source BPM and hear \
                     it as pitch)."
                });
            });
        });
    });

    // ── Slice ORDER strip ────────────────────────────────────────────────
    // Maps step index to slice index — the heart of "rearrange the break".
    // Each cell shows the slice number that fires at that step position;
    // click to cycle through 1..slice_count.  Active step is highlighted
    // so the user can visually track the live playhead through the order.
    let order_changed = super::amen_strips::draw_slice_order_strip(
        ui,
        &mut app.state.write().sequencer.amen_slice_order,
        slice_count,
        active,
    );
    if order_changed {
        app.push_audio_params();
    }

    // ── Per-slice DIR strip ───────────────────────────────────────────────
    // Mirrors the order strip's layout but toggles direction per slice so
    // specific slices glitch backwards while others play forward — the
    // edit-era chop move.  When the vec is empty, every slice inherits
    // the global REV/FWD; first click populates and promotes out of
    // inherit mode.
    let reverse_changed = super::amen_strips::draw_slice_reverse_strip(
        ui,
        &mut app.state.write().amen.slice_reverses,
        slice_count,
        active,
        reverse,
    );
    if reverse_changed {
        app.push_audio_params();
    }

    // Earlier attempt to "glue" this row to the bottom via
    // add_space(available - knob_h) half-overflowed the panel because
    // glass_group_fill's own inner margin (+16 px) pushed the panes
    // past the module edge.

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
        s.amen.source_bpm = source_bpm.clamp(BPM_MIN, BPM_MAX);
        s.amen.bpm_stretch = bpm_stretch;
        s.amen.bpm_stretch_preserve = bpm_stretch_preserve;
        drop(s);
        app.push_audio_params();
    }
}
