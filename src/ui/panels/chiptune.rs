// ─── ui/panels/chiptune.rs ────────────────────────────────────────────────────
// SID-flavoured chiptune voice panel.  Header (ON/OFF + VOLUME +
// PAN + RING / SYNC toggles) + 3 oscillator rows (WAVE cycle +
// LEVEL + ADSR) + 1 filter row (CUTOFF + RESONANCE + MODE cycle
// + PULSE WIDTH + MIX).

use crate::state::{CHIPTUNE_FILTER_MODES, CHIPTUNE_OSCS, CHIPTUNE_WAVEFORMS, ParamMode};
use crate::ui::{ImpulseApp, theme, widgets};

const WAVE_LABELS: [&str; CHIPTUNE_WAVEFORMS as usize] = ["SAW", "TRI", "PULSE", "NOISE"];
const FILTER_MODE_LABELS: [&str; CHIPTUNE_FILTER_MODES as usize] = ["LP", "BP", "HP"];

pub fn draw_chiptune(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    // ── Header row: ON/OFF + global volume / pan + RING / SYNC
    ui.horizontal(|ui| {
        let enabled = app.state.read().chiptune.enabled;
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
            app.state.write().chiptune.enabled = !enabled;
            app.push_audio_params();
        }

        let mut vol = app.state.read().chiptune.volume;
        if widgets::param_control(ui, "VOLUME", &mut vol, ParamMode::Free, ctrl).0 {
            app.state.write().chiptune.volume = vol.clamp(0.0, 1.5);
            app.push_audio_params();
        }
        let raw_pan = app.state.read().chiptune.pan;
        let mut pan = (raw_pan + 1.0) * 0.5;
        if widgets::param_control(ui, "PAN", &mut pan, ParamMode::Free, ctrl).0 {
            app.state.write().chiptune.pan = (pan * 2.0 - 1.0).clamp(-1.0, 1.0);
            app.push_audio_params();
        }

        // RING MOD + SYNC toggles — the SID-defining flags.  Single
        // toggle button each so the panel doesn't bury them behind
        // a menu.
        let mut ring = app.state.read().chiptune.ring_mod;
        if widgets::toggle_button(ui, if ring { "RING" } else { "ring" }, &mut ring) {
            app.state.write().chiptune.ring_mod = ring;
            app.push_audio_params();
        }
        let mut sync = app.state.read().chiptune.sync;
        if widgets::toggle_button(ui, if sync { "SYNC" } else { "sync" }, &mut sync) {
            app.state.write().chiptune.sync = sync;
            app.push_audio_params();
        }
    });

    ui.add_space(2.0);

    // ── Three oscillator rows.  Each: OP-N label + WAVE cycle +
    // LEVEL + ATTACK + DECAY + SUSTAIN + RELEASE.  Glass-grouped
    // per-row so the three oscs read as distinct units.
    let avail = ui.available_width();
    let group_h = widgets::glass_group_height(ctrl, 35.0);
    for osc_idx in 0..CHIPTUNE_OSCS {
        widgets::glass_group_fill(ui, avail, avail, |ui| {
            ui.set_min_height(group_h);
            ui.horizontal(|ui| {
                let label = match osc_idx {
                    0 => "OSC 1",
                    1 => "OSC 2",
                    _ => "OSC 3",
                };
                ui.label(
                    egui::RichText::new(label)
                        .color(theme::FOG)
                        .monospace()
                        .size(10.0),
                );

                // Waveform cycle button.  Click to rotate
                // through SAW → TRI → PULSE → NOISE.
                let cur_wave = {
                    let s = app.state.read();
                    let osc = match osc_idx {
                        0 => &s.chiptune.osc1,
                        1 => &s.chiptune.osc2,
                        _ => &s.chiptune.osc3,
                    };
                    osc.waveform.min(CHIPTUNE_WAVEFORMS - 1)
                };
                let label = WAVE_LABELS[cur_wave as usize];
                if ui
                    .add_sized(
                        [54.0, 20.0],
                        egui::Button::new(
                            egui::RichText::new(label)
                                .monospace()
                                .size(9.0)
                                .color(theme::CHALK),
                        )
                        .fill(egui::Color32::from_gray(55)),
                    )
                    .on_hover_text("Click to cycle waveform: SAW → TRI → PULSE → NOISE.")
                    .clicked()
                {
                    let next = (cur_wave + 1) % CHIPTUNE_WAVEFORMS;
                    let mut s = app.state.write();
                    let osc = match osc_idx {
                        0 => &mut s.chiptune.osc1,
                        1 => &mut s.chiptune.osc2,
                        _ => &mut s.chiptune.osc3,
                    };
                    osc.waveform = next;
                    drop(s);
                    app.push_audio_params();
                }

                // Per-osc level + ADSR.
                let (mut level, mut a, mut d, mut sus, mut r) = {
                    let s = app.state.read();
                    let osc = match osc_idx {
                        0 => &s.chiptune.osc1,
                        1 => &s.chiptune.osc2,
                        _ => &s.chiptune.osc3,
                    };
                    (osc.level, osc.attack, osc.decay, osc.sustain, osc.release)
                };
                let mut changed = false;
                if widgets::param_control(ui, "LEVEL", &mut level, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "ATTACK", &mut a, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "DECAY", &mut d, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "SUSTAIN", &mut sus, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "RELEASE", &mut r, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if changed {
                    let mut s = app.state.write();
                    let osc = match osc_idx {
                        0 => &mut s.chiptune.osc1,
                        1 => &mut s.chiptune.osc2,
                        _ => &mut s.chiptune.osc3,
                    };
                    osc.level = level.clamp(0.0, 1.0);
                    osc.attack = a.clamp(0.0, 1.0);
                    osc.decay = d.clamp(0.0, 1.0);
                    osc.sustain = sus.clamp(0.0, 1.0);
                    osc.release = r.clamp(0.0, 1.0);
                    drop(s);
                    app.push_audio_params();
                }
            });
        });
        ui.add_space(2.0);
    }

    // ── Filter row: CUTOFF + RESONANCE + MIX + PULSE WIDTH +
    // MODE cycle.  Glass-grouped on its own so the filter
    // section reads as a separate unit from the oscillators.
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("FILTER")
                    .color(theme::FOG)
                    .monospace()
                    .size(10.0),
            );

            let mut cut = app.state.read().chiptune.filter_cutoff;
            if widgets::param_control(ui, "CUTOFF", &mut cut, ParamMode::Free, ctrl).0 {
                app.state.write().chiptune.filter_cutoff = cut.clamp(0.0, 1.0);
                app.push_audio_params();
            }
            let mut res = app.state.read().chiptune.filter_resonance;
            if widgets::param_control(ui, "RES", &mut res, ParamMode::Free, ctrl).0 {
                app.state.write().chiptune.filter_resonance = res.clamp(0.0, 1.0);
                app.push_audio_params();
            }
            let mut mix = app.state.read().chiptune.filter_mix;
            if widgets::param_control(ui, "MIX", &mut mix, ParamMode::Free, ctrl).0 {
                app.state.write().chiptune.filter_mix = mix.clamp(0.0, 1.0);
                app.push_audio_params();
            }

            // Filter mode cycle button.
            let cur_mode = app
                .state
                .read()
                .chiptune
                .filter_mode
                .min(CHIPTUNE_FILTER_MODES - 1);
            let mode_label = FILTER_MODE_LABELS[cur_mode as usize];
            if ui
                .add_sized(
                    [40.0, 20.0],
                    egui::Button::new(
                        egui::RichText::new(mode_label)
                            .monospace()
                            .size(9.0)
                            .color(theme::CHALK),
                    )
                    .fill(egui::Color32::from_gray(55)),
                )
                .on_hover_text("Click to cycle filter mode: LP → BP → HP.")
                .clicked()
            {
                let next = (cur_mode + 1) % CHIPTUNE_FILTER_MODES;
                app.state.write().chiptune.filter_mode = next;
                app.push_audio_params();
            }

            let mut pw = app.state.read().chiptune.pulse_width;
            if widgets::param_control(ui, "PULSE WIDTH", &mut pw, ParamMode::Free, ctrl).0 {
                app.state.write().chiptune.pulse_width = pw.clamp(0.05, 0.95);
                app.push_audio_params();
            }
        });
    });
}
