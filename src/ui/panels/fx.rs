// ─── ui/panels/fx.rs ──────────────────────────────────────────────────────────
// FX chain panel.

use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_fx(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    widgets::section_header(ui, "FX CHAIN");

    // Snapshot all FX values + locked set before any widget call
    let (
        mut rs,
        mut rd,
        mut rm,
        mut dt,
        mut df,
        mut dm,
        mut dd,
        mut dx,
        mut mv,
        mut ch_rate,
        mut ch_depth,
        mut ch_mix,
        mut ph_rate,
        mut ph_depth,
        mut ph_mix,
        mut ws_drive,
        mut ws_mix,
        mut rm_freq,
        mut rm_mix,
        mut eq_low,
        mut eq_mid,
        mut eq_hi,
        locked,
        auto_lock,
    ) = {
        let s = app.state.read();
        (
            s.fx.reverb_size,
            s.fx.reverb_damp,
            s.fx.reverb_mix,
            s.fx.delay_time,
            s.fx.delay_feedback,
            s.fx.delay_mix,
            s.fx.distortion_drive,
            s.fx.distortion_mix,
            s.fx.master_volume,
            s.fx.chorus_rate,
            s.fx.chorus_depth,
            s.fx.chorus_mix,
            s.fx.phaser_rate,
            s.fx.phaser_depth,
            s.fx.phaser_mix,
            s.fx.waveshaper_drive,
            s.fx.waveshaper_mix,
            s.fx.ring_mod_freq,
            s.fx.ring_mod_mix,
            s.fx.eq_low_gain,
            s.fx.eq_mid_gain,
            s.fx.eq_hi_gain,
            s.llm.locked_params.clone(),
            s.llm.auto_lock_on_touch,
        )
    };

    let mut changed = false;

    let l_rs = locked.contains("fx.reverb_size");
    let l_rm = locked.contains("fx.reverb_mix");
    let l_df = locked.contains("fx.delay_feedback");
    let l_dm = locked.contains("fx.delay_mix");

    let use_sliders = app.use_sliders;
    ui.horizontal_wrapped(|ui| {
        ui.group(|ui| {
            ui.label(
                egui::RichText::new("REVERB")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            // MIX (X) × SIZE (Y) pad — the two most-used reverb params
            if widgets::xy_pad(ui, "MIX", "SIZE", &mut rm, &mut rs, 88.0, l_rm || l_rs) {
                let mut s = app.state.write();
                if !l_rm {
                    s.fx.reverb_mix = rm;
                    if auto_lock {
                        s.llm.locked_params.insert("fx.reverb_mix".to_string());
                    }
                }
                if !l_rs {
                    s.fx.reverb_size = rs;
                    if auto_lock {
                        s.llm.locked_params.insert("fx.reverb_size".to_string());
                    }
                }
                drop(s);
                app.push_audio_params();
            }
            if widgets::param_control(ui, "DAMP", &mut rd, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.label(
                egui::RichText::new("DELAY")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            // MIX (X) × FDBK (Y) pad — feedback × mix is the key delay texture dial
            if widgets::xy_pad(ui, "MIX", "FDBK", &mut dm, &mut df, 88.0, l_dm || l_df) {
                let mut s = app.state.write();
                if !l_dm {
                    s.fx.delay_mix = dm;
                    if auto_lock {
                        s.llm.locked_params.insert("fx.delay_mix".to_string());
                    }
                }
                if !l_df {
                    s.fx.delay_feedback = df;
                    if auto_lock {
                        s.llm.locked_params.insert("fx.delay_feedback".to_string());
                    }
                }
                drop(s);
                app.push_audio_params();
            }
            if widgets::param_control(ui, "TIME", &mut dt, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.label(
                egui::RichText::new("CHORUS")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "MIX", &mut ch_mix, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "RATE", &mut ch_rate, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "DEPTH", &mut ch_depth, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.label(
                egui::RichText::new("PHASER")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "MIX", &mut ph_mix, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "RATE", &mut ph_rate, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "DEPTH", &mut ph_depth, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.label(
                egui::RichText::new("WAVESHAPER")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "MIX", &mut ws_mix, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "DRIVE", &mut ws_drive, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.label(
                egui::RichText::new("RING MOD")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "MIX", &mut rm_mix, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "FREQ", &mut rm_freq, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.label(
                egui::RichText::new("EQ")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "LOW", &mut eq_low, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MID", &mut eq_mid, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "HI", &mut eq_hi, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
        });

        ui.add_space(4.0);

        ui.group(|ui| {
            ui.label(
                egui::RichText::new("DRIVE / MASTER")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "DRIVE", &mut dd, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut dx, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MASTER", &mut mv, ParamMode::Free, use_sliders).0 {
                changed = true;
            }
        });
    });

    // Write-back for standalone knobs (DAMP, TIME, DRIVE, CHORUS, etc.)
    if changed {
        let mut s = app.state.write();
        s.fx.reverb_damp = rd;
        s.fx.delay_time = dt;
        s.fx.distortion_drive = dd;
        s.fx.distortion_mix = dx;
        s.fx.master_volume = mv;
        s.fx.chorus_rate = ch_rate;
        s.fx.chorus_depth = ch_depth;
        s.fx.chorus_mix = ch_mix;
        s.fx.phaser_rate = ph_rate;
        s.fx.phaser_depth = ph_depth;
        s.fx.phaser_mix = ph_mix;
        s.fx.waveshaper_drive = ws_drive;
        s.fx.waveshaper_mix = ws_mix;
        s.fx.ring_mod_freq = rm_freq;
        s.fx.ring_mod_mix = rm_mix;
        s.fx.eq_low_gain = eq_low;
        s.fx.eq_mid_gain = eq_mid;
        s.fx.eq_hi_gain = eq_hi;
        drop(s);
        app.push_audio_params();
    }
}
