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
        // Animated waveform previews for OSC1 and OSC2
        let wave_to_kind = |w: An1xWave| -> u8 {
            match w {
                An1xWave::Saw => 0,
                An1xWave::Square => 1,
                An1xWave::Triangle => 4,
                An1xWave::Sine => 3,
                An1xWave::Noise => 5, // reuse S&H-like animation
            }
        };
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("O1")
                    .color(theme::SMOKE)
                    .size(7.0)
                    .monospace(),
            );
            ui.add_space(2.0);
            widgets::waveform_icon(ui, wave_to_kind(osc1w), 100.0, 48.0);
            ui.add_space(2.0);
        });
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("O2")
                    .color(theme::SMOKE)
                    .size(7.0)
                    .monospace(),
            );
            ui.add_space(2.0);
            widgets::waveform_icon(ui, wave_to_kind(osc2w), 100.0, 48.0);
            ui.add_space(2.0);
        });
    });
    // ── Filter mode + ADSR vizzes (compact header row) ─────────────────────
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
                    egui::Button::new(
                        egui::RichText::new(*label)
                            .color(if active { theme::CHALK } else { theme::IRON })
                            .size(8.0),
                    )
                    .fill(if active { theme::SLATE } else { theme::PIT })
                    .min_size(egui::vec2(22.0, 14.0)),
                )
                .clicked()
            {
                app.state.write().an1x.filter_mode = *mode;
                app.push_audio_params();
            }
        }
        ui.separator();
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
            ui.label(
                egui::RichText::new("F.ENV")
                    .color(theme::IRON)
                    .monospace()
                    .size(7.0),
            );
            // Wrap the ADSR display in a padded frame so it has visible
            // breathing room instead of butting up against the F.ENV
            // label and the separator.
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    if widgets::adsr_display(ui, &mut a, &mut d, &mut s, &mut r, 280.0, 100.0) {
                        let mut st = app.state.write();
                        st.an1x.filter_attack = a;
                        st.an1x.filter_decay = d;
                        st.an1x.filter_sustain = s;
                        st.an1x.filter_release = r;
                        drop(st);
                        app.push_audio_params();
                    }
                });
        }
        ui.separator();
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
            ui.label(
                egui::RichText::new("A.ENV")
                    .color(theme::IRON)
                    .monospace()
                    .size(7.0),
            );
            // Matching padding to the F.ENV block above.
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    if widgets::adsr_display(ui, &mut a, &mut d, &mut s, &mut r, 280.0, 100.0) {
                        let mut st = app.state.write();
                        st.an1x.amp_attack = a;
                        st.an1x.amp_decay = d;
                        st.an1x.amp_sustain = s;
                        st.an1x.amp_release = r;
                        drop(st);
                        app.push_audio_params();
                    }
                });
        }
    });

    // AN1X knob macro
    macro_rules! ak {
        ($ui:expr, $lbl:expr, $fld:ident) => {{
            let mut v = app.state.read().an1x.$fld;
            if widgets::param_control($ui, $lbl, &mut v, ParamMode::Free, ctrl).0 {
                app.state.write().an1x.$fld = v;
                app.push_audio_params();
            }
        }};
    }

    // Consistent glass group height — row 1 has 2 knob rows, row 2 adds PAN
    let group_h = ctrl.knob_size * 2.0 + 50.0;
    let group_h2 = group_h + 24.0; // extra space for PAN slider in A.ADSR

    // ── Row 1 (3 cols): LEVELS | FILTER | F.ADSR ────────────────────────
    {
        let gw = widgets::even_group_width(ui, 3);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = super::GLASS_GAP;
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.set_min_height(group_h);
                ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
                ui.label(
                    egui::RichText::new("LEVELS")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    ak!(ui, "O1", osc1_level);
                    ak!(ui, "O2", osc2_level);
                    ak!(ui, "SUB", sub_level);
                });
            });
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.set_min_height(group_h);
                ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
                ui.label(
                    egui::RichText::new("FILTER")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    ak!(ui, "CUTOFF", filter_cutoff);
                    ak!(ui, "RESO", filter_resonance);
                });
                widgets::centered_row(ui, |ui| {
                    ak!(ui, "ENVAMT", filter_env_amount);
                    ak!(ui, "KEYTRK", filter_key_track);
                });
            });
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.set_min_height(group_h);
                ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
                ui.label(
                    egui::RichText::new("F.ADSR")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    ak!(ui, "ATTACK", filter_attack);
                    ak!(ui, "DECAY", filter_decay);
                });
                widgets::centered_row(ui, |ui| {
                    ak!(ui, "SUSTAIN", filter_sustain);
                    ak!(ui, "RELEASE", filter_release);
                });
            });
        });
    }

    ui.add_space(super::GLASS_GAP);

    // ── Row 2 (3 cols): TUNE | AMP ADSR | PITCH ENV ────────────────────
    {
        let gw = widgets::even_group_width(ui, 3);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = super::GLASS_GAP;
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.set_min_height(group_h2);
                ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
                ui.label(
                    egui::RichText::new("TUNE")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    ak!(ui, "DETUNE", osc2_detune);
                    let oct = app.state.read().an1x.osc2_octave;
                    ui.label(
                        egui::RichText::new("OCT")
                            .color(theme::SMOKE)
                            .size(7.5)
                            .monospace(),
                    );
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
                widgets::centered_row(ui, |ui| {
                    let ring = app.state.read().an1x.ring_mod;
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("RING×")
                                    .color(if ring { theme::CHALK } else { theme::IRON })
                                    .size(8.0),
                            )
                            .fill(if ring {
                                theme::SLATE
                            } else {
                                theme::PIT
                            }),
                        )
                        .clicked()
                    {
                        app.state.write().an1x.ring_mod = !ring;
                        app.push_audio_params();
                    }
                    let sync = app.state.read().an1x.hard_sync;
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("SYNC")
                                    .color(if sync { theme::CHALK } else { theme::IRON })
                                    .size(8.0),
                            )
                            .fill(if sync {
                                theme::SLATE
                            } else {
                                theme::PIT
                            }),
                        )
                        .clicked()
                    {
                        app.state.write().an1x.hard_sync = !sync;
                        app.push_audio_params();
                    }
                });
            });
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.set_min_height(group_h2);
                ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
                ui.label(
                    egui::RichText::new("AMP ADSR")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    ak!(ui, "ATTACK", amp_attack);
                    ak!(ui, "DECAY", amp_decay);
                    ak!(ui, "VOLUME", volume);
                });
                widgets::centered_row(ui, |ui| {
                    ak!(ui, "SUSTAIN", amp_sustain);
                    ak!(ui, "RELEASE", amp_release);
                });
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut pan = app.state.read().an1x.pan;
                        if widgets::pan_slider(ui, &mut pan, super::PAN_SLIDER_W) {
                            app.state.write().an1x.pan = pan;
                            app.push_audio_params();
                        }
                        ui.label(
                            egui::RichText::new("PAN")
                                .color(theme::SMOKE)
                                .monospace()
                                .size(7.5),
                        );
                    });
                });
            });
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.set_min_height(group_h2);
                ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
                ui.label(
                    egui::RichText::new("PITCH ENV")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    ak!(ui, "ATTACK", pitch_env_attack);
                    ak!(ui, "DECAY", pitch_env_decay);
                    ak!(ui, "AMOUNT", pitch_env_amount);
                });
            });
        });
    }
    ui.add_space(4.0);

    // ── LFO + TEXTURE ────────────────────────────────────────────────────────
    ui.scope(|ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            widgets::section_header(ui, "LFO  /  TEXTURE");
        });
    });
    ui.with_layout(
        egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true),
        |ui| {
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
                        .fill(if synced {
                            theme::SLATE
                        } else {
                            theme::PIT
                        }),
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
        },
    );
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
    // Observe edits for style tracking (snapshot key params)
    let (cut, reso, vol) = {
        let a = &app.state.read().an1x;
        (a.filter_cutoff, a.filter_resonance, a.volume)
    };
    app.observe_edits(&[
        ("an1x.filter_cutoff", cut),
        ("an1x.filter_resonance", reso),
        ("an1x.volume", vol),
    ]);
}
