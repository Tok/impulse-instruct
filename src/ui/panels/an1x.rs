// ─── ui/panels/an1x.rs ───────────────────────────────────────────────────────
// AN1X-style VA voice panel — warm pads / leads.

use crate::state::{An1xWave, FilterMode, ParamMode, apply_boc_preset};
use crate::ui::{ImpulseApp, theme, widgets};
use egui::Ui;

pub fn draw_an1x(app: &mut ImpulseApp, ui: &mut Ui) {
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
    {
        let gw = widgets::even_group_width(ui, 2);
        ui.horizontal(|ui| {
            // LEVELS group: O1 LVL, O2 LVL, SUB
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(
                    egui::RichText::new("LEVELS")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                {
                    let mut v = app.state.read().an1x.osc1_level;
                    let (ch, _) =
                        widgets::param_control(ui, "O1 LVL", &mut v, ParamMode::Free, ctrl);
                    if ch {
                        app.state.write().an1x.osc1_level = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().an1x.osc2_level;
                    let (ch, _) =
                        widgets::param_control(ui, "O2 LVL", &mut v, ParamMode::Free, ctrl);
                    if ch {
                        app.state.write().an1x.osc2_level = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().an1x.sub_level;
                    let (ch, _) = widgets::param_control(ui, "SUB", &mut v, ParamMode::Free, ctrl);
                    if ch {
                        app.state.write().an1x.sub_level = v;
                        app.push_audio_params();
                    }
                }
            });
            // TUNE group: O2 DET, O2 OCT stepper, RING×, SYNC
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(
                    egui::RichText::new("TUNE")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                {
                    let mut v = app.state.read().an1x.osc2_detune;
                    let (ch, _) =
                        widgets::param_control(ui, "O2 DET", &mut v, ParamMode::Free, ctrl);
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
                    let ring = app.state.read().an1x.ring_mod;
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("RING×").color(if ring {
                                theme::CHALK
                            } else {
                                theme::IRON
                            }))
                            .fill(if ring {
                                theme::SLATE
                            } else {
                                theme::PIT
                            }),
                        )
                        .on_hover_text("Ring modulation: OSC1 × OSC2")
                        .clicked()
                    {
                        app.state.write().an1x.ring_mod = !ring;
                        app.push_audio_params();
                    }
                }
                {
                    let sync = app.state.read().an1x.hard_sync;
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("SYNC").color(if sync {
                                theme::CHALK
                            } else {
                                theme::IRON
                            }))
                            .fill(if sync {
                                theme::SLATE
                            } else {
                                theme::PIT
                            }),
                        )
                        .on_hover_text("Hard sync: OSC2 phase resets on each OSC1 cycle")
                        .clicked()
                    {
                        app.state.write().an1x.hard_sync = !sync;
                        app.push_audio_params();
                    }
                }
            });
        });
    }
    ui.add_space(4.0);

    // ── FILTER ───────────────────────────────────────────────────────────────
    widgets::section_header(ui, "FILTER");
    // Filter mode buttons — full width
    ui.horizontal(|ui| {
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
    });
    // Filter ADSR visualiser — interactive, drag to edit (full width)
    ui.label(
        egui::RichText::new("Filter ADSR")
            .color(theme::IRON)
            .small(),
    );
    {
        let (mut a, mut d, mut s, mut r) = {
            let st = app.state.read();
            (
                st.an1x.filter_attack,
                st.an1x.filter_decay,
                st.an1x.filter_sustain,
                st.an1x.filter_release,
            )
        };
        if widgets::adsr_display(ui, &mut a, &mut d, &mut s, &mut r, 280.0, 100.0) {
            let mut st = app.state.write();
            st.an1x.filter_attack = a;
            st.an1x.filter_decay = d;
            st.an1x.filter_sustain = s;
            st.an1x.filter_release = r;
            drop(st);
            app.push_audio_params();
        }
    }
    // Filter knobs in 2 glass groups: FILTER (cutoff/reso/env/key) and F.ADSR (attack/decay/sust/rel)
    {
        let gw = widgets::even_group_width(ui, 2);
        ui.horizontal(|ui| {
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(
                    egui::RichText::new("FILTER")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                macro_rules! k {
                    ($lbl:expr, $fld:ident) => {{
                        let mut v = app.state.read().an1x.$fld;
                        let (ch, _) =
                            widgets::param_control(ui, $lbl, &mut v, ParamMode::Free, ctrl);
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
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(
                    egui::RichText::new("F.ADSR")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                macro_rules! k {
                    ($lbl:expr, $fld:ident) => {{
                        let mut v = app.state.read().an1x.$fld;
                        let (ch, _) =
                            widgets::param_control(ui, $lbl, &mut v, ParamMode::Free, ctrl);
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
        });
    }
    ui.add_space(4.0);

    // ── AMP ADSR + PITCH ENV ─────────────────────────────────────────────────
    widgets::section_header(ui, "AMP ADSR");
    // Amp ADSR visualiser — full width
    {
        let (mut a, mut d, mut s, mut r) = {
            let st = app.state.read();
            (
                st.an1x.amp_attack,
                st.an1x.amp_decay,
                st.an1x.amp_sustain,
                st.an1x.amp_release,
            )
        };
        if widgets::adsr_display(ui, &mut a, &mut d, &mut s, &mut r, 280.0, 100.0) {
            let mut st = app.state.write();
            st.an1x.amp_attack = a;
            st.an1x.amp_decay = d;
            st.an1x.amp_sustain = s;
            st.an1x.amp_release = r;
            drop(st);
            app.push_audio_params();
        }
    }
    // Amp knobs + Pitch env knobs in 2 glass groups: A.ADSR and PITCH ENV
    {
        let gw = widgets::even_group_width(ui, 2);
        ui.horizontal(|ui| {
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(
                    egui::RichText::new("A.ADSR")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                macro_rules! k {
                    ($lbl:expr, $fld:ident) => {{
                        let mut v = app.state.read().an1x.$fld;
                        let (ch, _) =
                            widgets::param_control(ui, $lbl, &mut v, ParamMode::Free, ctrl);
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
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(
                    egui::RichText::new("PITCH ENV")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                macro_rules! k {
                    ($lbl:expr, $fld:ident) => {{
                        let mut v = app.state.read().an1x.$fld;
                        let (ch, _) =
                            widgets::param_control(ui, $lbl, &mut v, ParamMode::Free, ctrl);
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
        });
    }
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
        // BPM sync toggle
        {
            let synced = app.state.read().an1x.lfo_bpm_sync;
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("BPM").color(if synced {
                        theme::CHALK
                    } else {
                        theme::IRON
                    }))
                    .fill(if synced { theme::SLATE } else { theme::PIT }),
                )
                .on_hover_text("Snap LFO rate to a musical division of the current BPM")
                .clicked()
            {
                app.state.write().an1x.lfo_bpm_sync = !synced;
                app.push_audio_params();
            }
        }
        if app.state.read().an1x.lfo_bpm_sync {
            // Division selector: 4, 2, 1, 0.5, 0.25 beats
            let cur_beats = app.state.read().an1x.lfo_sync_beats;
            for (label, beats) in &[
                ("1/1", 4.0_f32),
                ("1/2", 2.0),
                ("1/4", 1.0),
                ("1/8", 0.5),
                ("1/16", 0.25),
            ] {
                let active = (cur_beats - beats).abs() < 0.01;
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(*label)
                                .monospace()
                                .size(8.5)
                                .color(if active { theme::CHALK } else { theme::IRON }),
                        )
                        .fill(if active { theme::SLATE } else { theme::PIT })
                        .min_size(egui::vec2(28.0, 16.0)),
                    )
                    .clicked()
                {
                    app.state.write().an1x.lfo_sync_beats = *beats;
                    app.push_audio_params();
                }
            }
        } else {
            k!("RATE", lfo_rate);
        }
        k!("DEPTH", lfo_depth);
        k!("DELAY", lfo_delay);
        k!("DRIFT", drift);
        // Glide controls
        k!("GLIDE", glide_time);
        {
            let legato = app.state.read().an1x.glide_legato;
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(if legato { "LEG" } else { "ALWYS" })
                            .monospace()
                            .size(8.5)
                            .color(theme::CHALK),
                    )
                    .fill(theme::SLATE),
                )
                .on_hover_text(if legato {
                    "Legato glide: only when notes overlap"
                } else {
                    "Always glide: even from silence"
                })
                .clicked()
            {
                app.state.write().an1x.glide_legato = !legato;
                app.push_audio_params();
            }
        }
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
