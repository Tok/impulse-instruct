// ─── ui/panels/an1x.rs ───────────────────────────────────────────────────────
// AN1X-style VA voice panel — warm pads / leads.

use crate::state::{An1xWave, FilterMode, ParamMode, apply_boc_preset};
use crate::ui::{ImpulseApp, theme, widgets};
use egui::Ui;

pub fn draw_an1x(app: &mut ImpulseApp, ui: &mut Ui) {
    widgets::section_header(ui, "AN1X  VOICE  (Warm VA)");

    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    // ── ON / OFF ──────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        let enabled = app.state.read().an1x.enabled;
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
            app.state.write().an1x.enabled = !enabled;
            app.push_audio_params();
        }
        ui.label(
            egui::RichText::new("dual osc · SVF filter · drift · glide")
                .color(theme::PIT)
                .monospace()
                .size(7.5),
        );
    });
    ui.add_space(4.0);

    // ── OSCILLATORS ──────────────────────────────────────────────────────────
    widgets::section_header(ui, "OSCILLATORS");
    ui.horizontal_wrapped(|ui| {
        let (osc1w, osc2w) = {
            let s = app.state.read();
            (s.an1x.osc1_wave, s.an1x.osc2_wave)
        };
        for (label, wave) in &[
            ("SAW", An1xWave::Saw),
            ("SQR", An1xWave::Square),
            ("TRI", An1xWave::Triangle),
            ("SIN", An1xWave::Sine),
            ("NOI", An1xWave::Noise),
        ] {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(*label).color(theme::SMOKE).small());
                let a1 = osc1w == *wave;
                let a2 = osc2w == *wave;
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("O1").color(if a1 {
                            theme::CHALK
                        } else {
                            theme::IRON
                        }))
                        .fill(if a1 { theme::SLATE } else { theme::PIT })
                        .min_size(egui::vec2(22.0, 14.0)),
                    )
                    .clicked()
                {
                    app.state.write().an1x.osc1_wave = *wave;
                    app.push_audio_params();
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("O2").color(if a2 {
                            theme::CHALK
                        } else {
                            theme::IRON
                        }))
                        .fill(if a2 { theme::SLATE } else { theme::PIT })
                        .min_size(egui::vec2(22.0, 14.0)),
                    )
                    .clicked()
                {
                    app.state.write().an1x.osc2_wave = *wave;
                    app.push_audio_params();
                }
            });
        }
    });
    ui.horizontal_wrapped(|ui| {
        {
            let mut v = app.state.read().an1x.osc1_level;
            let (ch, _) = widgets::param_control(ui, "O1 LVL", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().an1x.osc1_level = v;
                app.push_audio_params();
            }
        }
        {
            let mut v = app.state.read().an1x.osc2_level;
            let (ch, _) = widgets::param_control(ui, "O2 LVL", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().an1x.osc2_level = v;
                app.push_audio_params();
            }
        }
        {
            let mut v = app.state.read().an1x.osc2_detune;
            let (ch, _) = widgets::param_control(ui, "O2 DET", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().an1x.osc2_detune = v;
                app.push_audio_params();
            }
        }
        {
            let oct = app.state.read().an1x.osc2_octave;
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("O2 OCT").color(theme::SMOKE).small());
                ui.horizontal(|ui| {
                    if ui.small_button("-").clicked() && oct > -2 {
                        app.state.write().an1x.osc2_octave = oct - 1;
                        app.push_audio_params();
                    }
                    ui.label(egui::RichText::new(format!("{:+}", oct)).color(theme::CHALK));
                    if ui.small_button("+").clicked() && oct < 2 {
                        app.state.write().an1x.osc2_octave = oct + 1;
                        app.push_audio_params();
                    }
                });
            });
        }
        {
            let mut v = app.state.read().an1x.sub_level;
            let (ch, _) = widgets::param_control(ui, "SUB", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().an1x.sub_level = v;
                app.push_audio_params();
            }
        }
        {
            let ring = app.state.read().an1x.ring_mod;
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("RING×").color(if ring {
                        theme::CHALK
                    } else {
                        theme::IRON
                    }))
                    .fill(if ring { theme::SLATE } else { theme::PIT }),
                )
                .clicked()
            {
                app.state.write().an1x.ring_mod = !ring;
                app.push_audio_params();
            }
        }
    });
    ui.add_space(4.0);

    // ── FILTER ───────────────────────────────────────────────────────────────
    widgets::section_header(ui, "FILTER");
    ui.horizontal_wrapped(|ui| {
        let fmode = app.state.read().an1x.filter_mode;
        for (label, mode) in &[
            ("LP", FilterMode::Lowpass),
            ("HP", FilterMode::Highpass),
            ("BP", FilterMode::Bandpass),
        ] {
            let active = fmode == *mode;
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(*label).color(if active {
                        theme::CHALK
                    } else {
                        theme::IRON
                    }))
                    .fill(if active { theme::SLATE } else { theme::PIT })
                    .min_size(egui::vec2(26.0, 16.0)),
                )
                .clicked()
            {
                app.state.write().an1x.filter_mode = *mode;
                app.push_audio_params();
            }
        }
        macro_rules! k {
            ($lbl:expr, $fld:ident) => {{
                let mut v = app.state.read().an1x.$fld;
                let (ch, _) = widgets::param_control(ui, $lbl, &mut v, ParamMode::Free, ctrl);
                if ch {
                    app.state.write().an1x.$fld = v;
                    app.push_audio_params();
                }
            }};
        }
        k!("CUTOFF", filter_cutoff);
        k!("RESO", filter_resonance);
        k!("ENV", filter_env_amount);
        k!("KEY", filter_key_track);
    });
    ui.label(
        egui::RichText::new("Filter ADSR")
            .color(theme::IRON)
            .small(),
    );
    ui.horizontal_wrapped(|ui| {
        macro_rules! k {
            ($lbl:expr, $fld:ident) => {{
                let mut v = app.state.read().an1x.$fld;
                let (ch, _) = widgets::param_control(ui, $lbl, &mut v, ParamMode::Free, ctrl);
                if ch {
                    app.state.write().an1x.$fld = v;
                    app.push_audio_params();
                }
            }};
        }
        k!("ATCK", filter_attack);
        k!("DCAY", filter_decay);
        k!("SUST", filter_sustain);
        k!("REL", filter_release);
    });
    ui.add_space(4.0);

    // ── AMP ADSR ─────────────────────────────────────────────────────────────
    widgets::section_header(ui, "AMP ADSR");
    ui.horizontal_wrapped(|ui| {
        macro_rules! k {
            ($lbl:expr, $fld:ident) => {{
                let mut v = app.state.read().an1x.$fld;
                let (ch, _) = widgets::param_control(ui, $lbl, &mut v, ParamMode::Free, ctrl);
                if ch {
                    app.state.write().an1x.$fld = v;
                    app.push_audio_params();
                }
            }};
        }
        k!("ATCK", amp_attack);
        k!("DCAY", amp_decay);
        k!("SUST", amp_sustain);
        k!("REL", amp_release);
        k!("VOL", volume);
    });
    ui.add_space(4.0);

    // ── PITCH ENVELOPE ───────────────────────────────────────────────────────
    widgets::section_header(ui, "PITCH  ENV  (AD)");
    ui.horizontal_wrapped(|ui| {
        macro_rules! k {
            ($lbl:expr, $fld:ident) => {{
                let mut v = app.state.read().an1x.$fld;
                let (ch, _) = widgets::param_control(ui, $lbl, &mut v, ParamMode::Free, ctrl);
                if ch {
                    app.state.write().an1x.$fld = v;
                    app.push_audio_params();
                }
            }};
        }
        k!("ATCK", pitch_env_attack);
        k!("DCAY", pitch_env_decay);
        k!("AMT", pitch_env_amount);
    });
    ui.add_space(4.0);

    // ── LFO + TEXTURE ────────────────────────────────────────────────────────
    widgets::section_header(ui, "LFO  /  TEXTURE");
    ui.horizontal_wrapped(|ui| {
        let target = app.state.read().an1x.lfo_target;
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(format!("-> {}", target.label())).color(theme::CHALK),
                )
                .fill(theme::SLATE),
            )
            .clicked()
        {
            app.state.write().an1x.lfo_target = target.next();
            app.push_audio_params();
        }
        macro_rules! k {
            ($lbl:expr, $fld:ident) => {{
                let mut v = app.state.read().an1x.$fld;
                let (ch, _) = widgets::param_control(ui, $lbl, &mut v, ParamMode::Free, ctrl);
                if ch {
                    app.state.write().an1x.$fld = v;
                    app.push_audio_params();
                }
            }};
        }
        k!("RATE", lfo_rate);
        k!("DEPTH", lfo_depth);
        k!("DELAY", lfo_delay);
        k!("DRIFT", drift);
        k!("GLIDE", glide_time);
    });
    ui.add_space(4.0);

    // ── WARM PRESET ──────────────────────────────────────────────────────────
    if ui
        .add(
            egui::Button::new(egui::RichText::new("WARM PRESET").color(theme::CHALK))
                .fill(theme::PIT)
                .stroke(egui::Stroke::new(1.0, theme::SLATE)),
        )
        .on_hover_text("Warm VA preset: slow attack, LP filter, detuned saws, subtle drift")
        .clicked()
    {
        let s = app.state.read().clone();
        *app.state.write() = apply_boc_preset(s);
        app.push_audio_params();
    }
}
