// ─── ui/panels/sample_instrument.rs ──────────────────────────────────────────
// Sample-Instrument voice panel.  Mirrors the WavetableVoice panel
// shape (ON/OFF + LOAD WAV + filename label, then a knob row) but the
// knobs are different: ROOT (source-recording note), VOL, PAN, PITCH
// trim in cents.
//
// V1 keeps it lean — a future iteration adds an "auto-detect root"
// button that runs `detect_pitch_hz` on the loaded buffer and writes
// the nearest MIDI note back into state.

use crate::audio::{AudioCommand, load_audio_to_engine};
use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

/// Synthetic path label written when the user captures the master
/// output into the SampleInstrument via the REC button — same idiom
/// the AmenSampler's REC→CHOP and the granular CAPTURE buttons use,
/// so the API-poll path doesn't try to reload from disk.
const REC_LABEL: &str = "«rec»";

/// Capture the master-output ring buffer (the same `granular_tap`
/// the AmenSampler's REC→CHOP and the granular CAPTURE buttons read)
/// and hand it to the SampleInstrument as the loaded source.  Mirrors
/// `record_chop_into_amen` in shape — extracted so a unit test can
/// drive the wave-thumbnail rebuild without rtrb plumbing.
pub(crate) fn record_into_sample_instrument(app: &mut ImpulseApp) {
    let buf = crate::ui::panels::amen::linearise_tap(&app.granular_tap, app.granular_tap_head);
    if buf.iter().all(|s| s.abs() < 1e-5) {
        log::warn!("[sample_instrument] REC: tap is silent, ignoring");
        return;
    }
    // Auto-detect root pitch on the captured material — same path
    // load_sample_instrument_path takes for fresh disk loads.  Only
    // commit the detected note when confidence clears the same 0.5
    // bar so a tap full of noise doesn't mis-tune the instrument.
    if let Some((hz, conf)) =
        crate::audio::analysis::detect_pitch_hz(&buf, crate::audio::SAMPLE_RATE)
        && conf >= 0.5
    {
        let midi = crate::audio::dsp::hz_to_midi(hz).round().clamp(0.0, 127.0) as u8;
        app.state.write().sample_instrument.root_note = midi;
    }
    // Captured-buffer mode replaces SFZ regions; clear the UI-side
    // cache + rebuild the waveform thumbnail for paint, just like
    // the disk-load single-WAV path.
    app.sample_sfz_regions.clear();
    app.sample_selected_region = None;
    let thumb = crate::ui::panels::sample_instrument_viz::build_thumbnail(&buf, 128);
    app.sample_wave_cache = (REC_LABEL.to_string(), thumb);
    let arc = std::sync::Arc::new(buf);
    let _ = app.audio_tx.push(AudioCommand::LoadSampleInstrument(arc));
    app.state.write().sample_instrument.sample_path = REC_LABEL.to_string();
    app.last_sample_instrument_path = REC_LABEL.to_string();
}

pub fn draw_sample_instrument(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        let enabled = app.state.read().sample_instrument.enabled;
        let btn_text = if enabled { "ON" } else { "OFF" };
        let btn_color = if enabled { theme::CHALK } else { theme::IRON };
        let btn_fill = if enabled {
            egui::Color32::from_gray(55)
        } else {
            egui::Color32::from_gray(22)
        };
        if ui
            .add_sized(
                [36.0, 20.0],
                egui::Button::new(
                    egui::RichText::new(btn_text)
                        .monospace()
                        .size(8.5)
                        .color(btn_color),
                )
                .fill(btn_fill),
            )
            .clicked()
        {
            app.state.write().sample_instrument.enabled = !enabled;
            app.push_audio_params();
        }

        // LOAD WAV button + filename — same picker pattern as Wavetable.
        // V1.1: also runs auto-detect-root via `detect_pitch_hz` on the
        // loaded buffer; if confidence is decent (>= 0.5) we set the
        // root note to the detected pitch so users don't have to know
        // their sample's source pitch.  Manual root knob still wins —
        // the detect only fires when a fresh file is loaded.
        if ui
            .add_sized([60.0, 20.0], egui::Button::new("LOAD"))
            .clicked()
            && let Some(p) = crate::ui::header_menu::pick_file_via_portal(
                "Audio/SFZ",
                &[
                    "wav", "WAV", "sfz", "SFZ", "flac", "FLAC", "aif", "AIF", "aiff", "AIFF",
                    "aifc", "AIFC",
                ],
            )
        {
            let ps = p.to_string_lossy().to_string();
            load_sample_instrument_path(app, &ps);
        }
        // REC — capture the master-output ring buffer as the
        // SampleInstrument source.  Same shared `granular_tap` the
        // amen REC→CHOP + granular CAPTURE read; auto-detect-root
        // runs on the captured material so the instrument tunes
        // itself.  No file is written; in-memory only.
        if ui
            .add_sized([46.0, 20.0], egui::Button::new("REC"))
            .on_hover_text(
                "Freeze the master-output ring buffer (last few seconds)\n\
                 as the SampleInstrument source.  Auto-detects root pitch.\n\
                 In-memory only — no file written.",
            )
            .clicked()
        {
            record_into_sample_instrument(app);
        }
        let path = app.state.read().sample_instrument.sample_path.clone();
        let name = if path.is_empty() {
            "(no sample)".to_string()
        } else {
            std::path::Path::new(&path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        ui.label(
            egui::RichText::new(name)
                .monospace()
                .size(8.0)
                .color(theme::ASH),
        );

        // Poll for API-driven sample_path changes.
        if !path.is_empty() && app.last_sample_instrument_path != path {
            load_sample_instrument_path(app, &path);
        }

        // Poly-meter — right-aligned so it sits flush at the panel
        // edge regardless of how long the loaded filename is.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let active = app
                .sample_instrument_poly
                .load(std::sync::atomic::Ordering::Relaxed);
            crate::ui::panels::sample_instrument_viz::draw_poly_meter(ui, active);
        });
    });

    ui.add_space(2.0);

    let gw = widgets::even_group_width(ui, 2);
    let group_h = widgets::glass_group_height(ctrl, 60.0);
    ui.horizontal(|ui| {
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("ROOT")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            // ROOT NOTE is the single primary control of the group
            // (it anchors every played pitch against the source
            // recording's natural note) — promote it to φ-bigger.
            let big = ctrl.phi_bigger();
            widgets::centered_row(ui, |ui| {
                let raw = app.state.read().sample_instrument.root_note;
                let mut v = raw as f32 / 127.0;
                if widgets::param_control(ui, "NOTE", &mut v, ParamMode::Free, big).0 {
                    let n = (v * 127.0).round().clamp(0.0, 127.0) as u8;
                    app.state.write().sample_instrument.root_note = n;
                    app.push_audio_params();
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("MIX")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().sample_instrument.volume;
                    if widgets::param_control(ui, "VOL", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.volume = v;
                        app.push_audio_params();
                    }
                }
                {
                    let raw = app.state.read().sample_instrument.pan;
                    let mut v = (raw + 1.0) * 0.5;
                    if widgets::param_control(ui, "PAN", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.pan = (v * 2.0 - 1.0).clamp(-1.0, 1.0);
                        app.push_audio_params();
                    }
                }
                {
                    let raw = app.state.read().sample_instrument.pitch_offset_cents;
                    let mut v = (raw / 200.0) + 0.5;
                    if widgets::param_control(ui, "TRIM", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.pitch_offset_cents =
                            ((v - 0.5) * 200.0).clamp(-100.0, 100.0);
                        app.push_audio_params();
                    }
                }
            });
        });
    });

    ui.add_space(2.0);

    // Row 2: ADSR + loop window.  Two glass groups again.
    ui.horizontal(|ui| {
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("ADSR")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            // Two rows of two — A/D on top, S/R on bottom — so
            // the labels can spell out fully (ATTACK / DECAY /
            // SUSTAIN / RELEASE) instead of cryptic single
            // letters.  Same group height as before because the
            // previous one-row layout had `glass_group_height(ctrl, 60.0)`
            // already sized for two knob rows.
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().sample_instrument.attack;
                    if widgets::param_control(ui, "ATTACK", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.attack = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().sample_instrument.decay;
                    if widgets::param_control(ui, "DECAY", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.decay = v;
                        app.push_audio_params();
                    }
                }
            });
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().sample_instrument.sustain;
                    if widgets::param_control(ui, "SUSTAIN", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.sustain = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().sample_instrument.release;
                    if widgets::param_control(ui, "RELEASE", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.release = v;
                        app.push_audio_params();
                    }
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("LOOP")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            // Row 1: START / END loop-window knobs.  Row 2: the
            // LOOP / 1× toggle on its own line so the button
            // doesn't compete with the knobs for the user's
            // pointer when they're sweeping the window.  Labels
            // spelled out (START vs the previous STR).
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().sample_instrument.loop_start;
                    if widgets::param_control(ui, "START", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.loop_start = v.clamp(0.0, 1.0);
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().sample_instrument.loop_end;
                    if widgets::param_control(ui, "END", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.loop_end = v.clamp(0.0, 1.0);
                        app.push_audio_params();
                    }
                }
            });
            widgets::centered_row(ui, |ui| {
                let on = app.state.read().sample_instrument.loop_enabled;
                let label = if on { "LOOP" } else { "1×" };
                let col = if on { theme::CHALK } else { theme::IRON };
                if ui
                    .add_sized(
                        [54.0, 20.0],
                        egui::Button::new(
                            egui::RichText::new(label).monospace().size(9.0).color(col),
                        ),
                    )
                    .clicked()
                {
                    app.state.write().sample_instrument.loop_enabled = !on;
                    app.push_audio_params();
                }
            });
        });
    });

    ui.add_space(2.0);

    // Row 3: filter — cutoff / resonance / mix + LP/BP/HP mode toggle.
    // Always shown so the user can dial in a per-voice colour even
    // when no SFZ region carries cutoff/resonance opcodes.
    ui.horizontal(|ui| {
        widgets::glass_group_fill(ui, gw * 2.0 + crate::ui::panels::GLASS_GAP, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("FILTER")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().sample_instrument.filter_cutoff;
                    if widgets::param_control(ui, "CUT", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.filter_cutoff = v.clamp(0.0, 1.0);
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().sample_instrument.filter_resonance;
                    if widgets::param_control(ui, "RES", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.filter_resonance = v.clamp(0.0, 1.0);
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().sample_instrument.filter_mix;
                    if widgets::param_control(ui, "MIX", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.filter_mix = v.clamp(0.0, 1.0);
                        app.push_audio_params();
                    }
                }
                let mode = app.state.read().sample_instrument.filter_mode;
                let label = match mode {
                    1 => "BP",
                    2 => "HP",
                    _ => "LP",
                };
                if ui
                    .add_sized(
                        [28.0, 18.0],
                        egui::Button::new(
                            egui::RichText::new(label)
                                .monospace()
                                .size(8.0)
                                .color(theme::CHALK),
                        ),
                    )
                    .clicked()
                {
                    let next = (mode + 1) % 3;
                    app.state.write().sample_instrument.filter_mode = next;
                    app.push_audio_params();
                }
                // Formant-preserve opt-in (V2 Stage 8 — flag wired,
                // DSP lands in a follow-up).  Renders as `FRMT` /
                // `frmt` so the user can pre-set it to flip on once
                // the implementation ships without touching their
                // session.
                let fp_on = app.state.read().sample_instrument.formant_preserve;
                let fp_label = if fp_on { "FRMT" } else { "frmt" };
                let fp_col = if fp_on { theme::CHALK } else { theme::IRON };
                if ui
                    .add_sized(
                        [32.0, 18.0],
                        egui::Button::new(
                            egui::RichText::new(fp_label)
                                .monospace()
                                .size(8.0)
                                .color(fp_col),
                        ),
                    )
                    .clicked()
                {
                    app.state.write().sample_instrument.formant_preserve = !fp_on;
                    app.push_audio_params();
                }
                // Mellotron-mode toggle.  When on, the slot's
                // playback gains tape-loop character (per-note
                // pitch flutter + spin-up + tanh sat).  Cheap
                // path stays the V1.1 default — flutter modulates
                // the read rate directly without going through
                // the spectral processor.
                let mello_on = app.state.read().sample_instrument.mellotron_mode;
                let mello_label = if mello_on { "MELLO" } else { "mello" };
                let mello_col = if mello_on { theme::CHALK } else { theme::IRON };
                if ui
                    .add_sized(
                        [38.0, 18.0],
                        egui::Button::new(
                            egui::RichText::new(mello_label)
                                .monospace()
                                .size(8.0)
                                .color(mello_col),
                        ),
                    )
                    .on_hover_text(
                        "Mellotron mode: tape-loop character — per-note pitch flutter, \
                         spin-up transient on attack, gentle tanh saturation.",
                    )
                    .clicked()
                {
                    app.state.write().sample_instrument.mellotron_mode = !mello_on;
                    app.push_audio_params();
                }
                // Continuous time-stretch knob — bipolar so the
                // resting position (1.0×) sits at knob centre and
                // dragging right makes playback faster, left
                // slower.  Maps logarithmically: each octave of
                // bipolar travel doubles / halves the multiplier
                // (bipolar ±1 → 4.0× / 0.25×; bipolar 0 → 1.0×).
                // Auto-engages the spectral processor when off-rest;
                // the cheap path stays the V1.1 default at the
                // detent.  Hover shows the live multiplier.
                let ts = app.state.read().sample_instrument.time_stretch;
                let mut bipolar = time_stretch_to_bipolar(ts);
                let (changed, _) =
                    widgets::param_control_bipolar(ui, "TIME", &mut bipolar, ParamMode::Free, ctrl);
                if changed {
                    let next = bipolar_to_time_stretch(bipolar);
                    app.state.write().sample_instrument.time_stretch = next;
                    app.push_audio_params();
                }
            });
        });
    });

    ui.add_space(2.0);

    // V2 Stage 7: visualizer row.  In SFZ mode shows the zone map
    // (regions banded across a piano keyboard); in single-WAV mode
    // shows the waveform thumbnail with loop-window shading.  Both
    // modes are read-only for now — drag-to-edit lands in Stage 7.5.
    let viz_w = ui.available_width().max(120.0);
    let viz_h = 40.0_f32;
    if !app.sample_sfz_regions.is_empty() {
        crate::ui::panels::sample_instrument_viz::draw_zone_map(
            ui,
            &app.sample_sfz_regions,
            &mut app.sample_selected_region,
            viz_w,
            viz_h,
        );
        // Inspector for the currently selected region — V1 read-only.
        // Lives directly under the zone map so eye travel from the
        // band the user clicked to its details is short.
        if let Some(idx) = app.sample_selected_region
            && let Some(region) = app.sample_sfz_regions.get(idx)
        {
            crate::ui::panels::sample_instrument_viz::draw_zone_inspector(ui, region);
        }
    } else {
        let (mut loop_start, mut loop_end, loop_enabled) = {
            let s = app.state.read();
            (
                s.sample_instrument.loop_start,
                s.sample_instrument.loop_end,
                s.sample_instrument.loop_enabled,
            )
        };
        let changed = crate::ui::panels::sample_instrument_viz::draw_waveform(
            ui,
            &app.sample_wave_cache.1,
            &mut loop_start,
            &mut loop_end,
            loop_enabled,
            viz_w,
            viz_h,
        );
        if changed {
            let mut s = app.state.write();
            s.sample_instrument.loop_start = loop_start;
            s.sample_instrument.loop_end = loop_end;
            drop(s);
            app.push_audio_params();
        }
    }
}

/// Load a path into the SampleInstrument voice.  Sniffs `.sfz` vs
/// `.wav` by extension; on `.sfz`, parses + loads every referenced WAV
/// off the audio thread and sends the runtime region list.  On `.wav`,
/// preserves the V1.1 single-sample path (load + auto-detect-root +
/// `LoadSampleInstrument` command).  Centralised so the LOAD button
/// and the API-poll path can share it.
fn load_sample_instrument_path(app: &mut ImpulseApp, path: &str) {
    let is_sfz = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("sfz"))
        .unwrap_or(false);
    if is_sfz {
        if let Some(regions) = crate::audio::sfz_loader::load_sfz_file(path) {
            // Stash a UI-side copy for the zone-map visualizer before
            // the Vec is moved into the audio command — the audio
            // thread owns the runtime list, but the UI needs to read
            // the metadata for paint.
            app.sample_sfz_regions = regions.clone();
            // Fresh SFZ — drop any stale region selection so the
            // inspector doesn't index the previous bank's regions.
            app.sample_selected_region = None;
            let _ = app
                .audio_tx
                .push(AudioCommand::LoadSampleInstrumentSfz(regions));
            app.state.write().sample_instrument.sample_path = path.to_string();
            app.last_sample_instrument_path = path.to_string();
        }
    } else if let Some(data) = load_audio_to_engine(path) {
        if let Some((hz, conf)) =
            crate::audio::analysis::detect_pitch_hz(&data, crate::audio::SAMPLE_RATE)
            && conf >= 0.5
        {
            let midi = crate::audio::dsp::hz_to_midi(hz).round().clamp(0.0, 127.0) as u8;
            app.state.write().sample_instrument.root_note = midi;
        }
        // Single-WAV mode replaces SFZ regions; clear the UI-side
        // cache + rebuild the waveform thumbnail for paint.
        app.sample_sfz_regions.clear();
        app.sample_selected_region = None;
        let thumb = crate::ui::panels::sample_instrument_viz::build_thumbnail(&data, 128);
        app.sample_wave_cache = (path.to_string(), thumb);
        let _ = app.audio_tx.push(AudioCommand::LoadSampleInstrument(data));
        app.state.write().sample_instrument.sample_path = path.to_string();
        app.last_sample_instrument_path = path.to_string();
    }
}

/// Time-stretch ↔ bipolar-knob log mapping.  The knob's bipolar
/// `[-1, +1]` range maps to the time-stretch multiplier
/// `[0.25, 4.0]` so that:
///
///   * bipolar  0  → multiplier 1.0× (rest position)
///   * bipolar +1 → multiplier 4.0× (max speed-up)
///   * bipolar -1 → multiplier 0.25× (max slow-down)
///   * each ±0.5 of bipolar travel doubles or halves the multiplier
///
/// The doubling-per-half-knob symmetry matches musicians' ear for
/// "octave" relationships (half speed ≈ pitch down an octave's
/// worth of duration before the spectral shifter compensates) so
/// the control feels uniform across the whole range.
///
/// Pulled out as a free function so the math is testable without
/// spinning up egui or AppState.  Pure: same input → same output.
fn bipolar_to_time_stretch(bipolar: f32) -> f32 {
    let b = bipolar.clamp(-1.0, 1.0);
    2.0_f32.powf(b * 2.0).clamp(0.25, 4.0)
}

/// Inverse of `bipolar_to_time_stretch` — used by the panel to
/// initialise the knob position from `state.sample_instrument
/// .time_stretch`.  Clamps the input to the legal range first so a
/// pathological state (LLM / API set 100×, file load with zero)
/// doesn't underflow log2.
fn time_stretch_to_bipolar(stretch: f32) -> f32 {
    let s = stretch.clamp(0.25, 4.0);
    (s.log2() / 2.0).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bipolar_to_time_stretch_centre_is_unity() {
        assert!((bipolar_to_time_stretch(0.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn bipolar_to_time_stretch_endpoints_hit_clamp_range() {
        assert!((bipolar_to_time_stretch(1.0) - 4.0).abs() < 1e-4);
        assert!((bipolar_to_time_stretch(-1.0) - 0.25).abs() < 1e-4);
    }

    #[test]
    fn bipolar_to_time_stretch_octave_symmetry() {
        // Half-knob travel doubles or halves the multiplier — the
        // log mapping's defining property.  At ±0.5 the knob
        // should land on 2.0× and 0.5× respectively.
        assert!((bipolar_to_time_stretch(0.5) - 2.0).abs() < 1e-4);
        assert!((bipolar_to_time_stretch(-0.5) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn bipolar_to_time_stretch_clamps_out_of_range_input() {
        // Bipolar values outside ±1 (defensive) get clamped before
        // the exponential, so the multiplier never escapes
        // 0.25..=4.0 even if the caller mishandles the knob.
        assert!((bipolar_to_time_stretch(2.0) - 4.0).abs() < 1e-4);
        assert!((bipolar_to_time_stretch(-3.5) - 0.25).abs() < 1e-4);
    }

    #[test]
    fn time_stretch_to_bipolar_round_trip() {
        // The inverse mapping is exact for the canonical values
        // and round-trips within float-tolerance for arbitrary
        // ones.
        for &b in &[-1.0, -0.5, 0.0, 0.25, 0.5, 0.75, 1.0_f32] {
            let s = bipolar_to_time_stretch(b);
            let b2 = time_stretch_to_bipolar(s);
            assert!(
                (b - b2).abs() < 1e-4,
                "round-trip drift: bipolar {b} → stretch {s} → bipolar {b2}"
            );
        }
    }

    #[test]
    fn time_stretch_to_bipolar_clamps_pathological_input() {
        // Out-of-range stretch (e.g. zero from a bad file load)
        // shouldn't underflow log2 — clamps to 0.25 first, which
        // maps to bipolar -1.
        assert!((time_stretch_to_bipolar(0.0) + 1.0).abs() < 1e-4);
        assert!((time_stretch_to_bipolar(100.0) - 1.0).abs() < 1e-4);
    }
}
