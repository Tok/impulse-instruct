// ─── ui/rack_content.rs ──────────────────────────────────────────────────────
// Content draw functions for each module kind, split from rack_canvas.rs to
// keep file sizes under the 1000-line limit.

use crate::state::ModuleKind;
use crate::ui::{ImpulseApp, module_card};

pub(super) fn draw_voice_content(app: &mut ImpulseApp, ui: &mut egui::Ui, kind: ModuleKind) {
    match kind {
        ModuleKind::AcidBass => crate::ui::panels::draw_bass(app, ui),
        ModuleKind::DrumKit808 => crate::ui::panels::draw_kit_a(app, ui),
        ModuleKind::DrumKit909 => crate::ui::panels::draw_kit_b(app, ui),
        ModuleKind::HooverLead => crate::ui::panels::draw_hoover(app, ui),
        ModuleKind::An1xVoice => crate::ui::panels::draw_an1x(app, ui),
        ModuleKind::AmenSampler => crate::ui::panels::draw_amen(app, ui),
        ModuleKind::NoiseVoice => draw_noise_stub(app, ui),
        ModuleKind::EspeakNgTts | ModuleKind::CoquiTts => crate::ui::panels::draw_tts(app, ui),
        _ => {}
    }
}

pub(super) fn draw_fx_content(app: &mut ImpulseApp, ui: &mut egui::Ui, kind: ModuleKind) {
    use crate::ui::widgets;

    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);
    let mut changed = false;

    match kind {
        ModuleKind::FxReverb => {
            let (mut rs, mut rd, mut rm) = {
                let s = app.state.read();
                (s.fx.reverb_size, s.fx.reverb_damp, s.fx.reverb_mix)
            };
            widgets::param_control(ui, "SIZE", &mut rs, pm("fx.reverb_size"), ctrl);
            if widgets::param_control(ui, "DAMP", &mut rd, pm("fx.reverb_damp"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut rm, pm("fx.reverb_mix"), ctrl).0 {
                changed = true;
            }
            if changed || rs != app.state.read().fx.reverb_size {
                let mut s = app.state.write();
                s.fx.reverb_size = rs;
                s.fx.reverb_damp = rd;
                s.fx.reverb_mix = rm;
            }
        }
        ModuleKind::FxDelay => {
            let (mut dt, mut df, mut dm) = {
                let s = app.state.read();
                (s.fx.delay_time, s.fx.delay_feedback, s.fx.delay_mix)
            };
            widgets::param_control(ui, "TIME", &mut dt, pm("fx.delay_time"), ctrl);
            widgets::param_control(ui, "FDBK", &mut df, pm("fx.delay_feedback"), ctrl);
            if widgets::param_control(ui, "MIX", &mut dm, pm("fx.delay_mix"), ctrl).0 {
                changed = true;
            }
            if changed || dt != app.state.read().fx.delay_time {
                let mut s = app.state.write();
                s.fx.delay_time = dt;
                s.fx.delay_feedback = df;
                s.fx.delay_mix = dm;
            }
        }
        ModuleKind::FxChorus => {
            let (mut r, mut d, mut m) = {
                let s = app.state.read();
                (s.fx.chorus_rate, s.fx.chorus_depth, s.fx.chorus_mix)
            };
            widgets::param_control(ui, "RATE", &mut r, pm("fx.chorus_rate"), ctrl);
            widgets::param_control(ui, "DEPTH", &mut d, pm("fx.chorus_depth"), ctrl);
            if widgets::param_control(ui, "MIX", &mut m, pm("fx.chorus_mix"), ctrl).0 {
                changed = true;
            }
            if changed || r != app.state.read().fx.chorus_rate {
                let mut s = app.state.write();
                s.fx.chorus_rate = r;
                s.fx.chorus_depth = d;
                s.fx.chorus_mix = m;
            }
        }
        ModuleKind::FxPhaser => {
            let (mut r, mut d, mut m) = {
                let s = app.state.read();
                (s.fx.phaser_rate, s.fx.phaser_depth, s.fx.phaser_mix)
            };
            widgets::param_control(ui, "RATE", &mut r, pm("fx.phaser_rate"), ctrl);
            widgets::param_control(ui, "DEPTH", &mut d, pm("fx.phaser_depth"), ctrl);
            if widgets::param_control(ui, "MIX", &mut m, pm("fx.phaser_mix"), ctrl).0 {
                changed = true;
            }
            if changed || r != app.state.read().fx.phaser_rate {
                let mut s = app.state.write();
                s.fx.phaser_rate = r;
                s.fx.phaser_depth = d;
                s.fx.phaser_mix = m;
            }
        }
        ModuleKind::FxEq => {
            let (mut lo, mut mi, mut hi) = {
                let s = app.state.read();
                (s.fx.eq_low_gain, s.fx.eq_mid_gain, s.fx.eq_hi_gain)
            };
            widgets::param_control(ui, "LOW", &mut lo, pm("fx.eq_low_gain"), ctrl);
            widgets::param_control(ui, "MID", &mut mi, pm("fx.eq_mid_gain"), ctrl);
            if widgets::param_control(ui, "HIGH", &mut hi, pm("fx.eq_hi_gain"), ctrl).0 {
                changed = true;
            }
            if changed || lo != app.state.read().fx.eq_low_gain {
                let mut s = app.state.write();
                s.fx.eq_low_gain = lo;
                s.fx.eq_mid_gain = mi;
                s.fx.eq_hi_gain = hi;
            }
        }
        ModuleKind::FxCompressor => {
            let (mut th, mut ra, mut mi) = {
                let s = app.state.read();
                (
                    s.fx.compressor_threshold,
                    s.fx.compressor_ratio,
                    s.fx.compressor_mix,
                )
            };
            widgets::param_control(ui, "THRESH", &mut th, pm("fx.compressor_threshold"), ctrl);
            widgets::param_control(ui, "RATIO", &mut ra, pm("fx.compressor_ratio"), ctrl);
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.compressor_mix"), ctrl).0 {
                changed = true;
            }
            if changed || th != app.state.read().fx.compressor_threshold {
                let mut s = app.state.write();
                s.fx.compressor_threshold = th;
                s.fx.compressor_ratio = ra;
                s.fx.compressor_mix = mi;
            }
        }
        ModuleKind::FxTapeSat => {
            let (mut dr, mut fl, mut mi) = {
                let s = app.state.read();
                (s.fx.tape_drive, s.fx.tape_flutter, s.fx.tape_mix)
            };
            widgets::param_control(ui, "DRIVE", &mut dr, pm("fx.tape_drive"), ctrl);
            widgets::param_control(ui, "FLUTTER", &mut fl, pm("fx.tape_flutter"), ctrl);
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.tape_mix"), ctrl).0 {
                changed = true;
            }
            if changed || dr != app.state.read().fx.tape_drive {
                let mut s = app.state.write();
                s.fx.tape_drive = dr;
                s.fx.tape_flutter = fl;
                s.fx.tape_mix = mi;
            }
        }
        ModuleKind::FxDrive => {
            let (mut dr, mut mi) = {
                let s = app.state.read();
                (s.fx.distortion_drive, s.fx.distortion_mix)
            };
            if widgets::param_control(ui, "DRIVE", &mut dr, pm("fx.distortion_drive"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.distortion_mix"), ctrl).0 {
                changed = true;
            }
            if changed {
                let mut s = app.state.write();
                s.fx.distortion_drive = dr;
                s.fx.distortion_mix = mi;
            }
        }
        ModuleKind::FxAutotune => {
            let (mut amt, mut mi) = {
                let s = app.state.read();
                (s.fx.autotune_amount, s.fx.autotune_mix)
            };
            if widgets::param_control(ui, "AMOUNT", &mut amt, pm("fx.autotune_amount"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.autotune_mix"), ctrl).0 {
                changed = true;
            }
            if changed {
                let mut s = app.state.write();
                s.fx.autotune_amount = amt;
                s.fx.autotune_mix = mi;
            }
        }
        ModuleKind::FxWaveshaper => {
            let (mut dr, mut mi) = {
                let s = app.state.read();
                (s.fx.waveshaper_drive, s.fx.waveshaper_mix)
            };
            if widgets::param_control(ui, "DRIVE", &mut dr, pm("fx.waveshaper_drive"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.waveshaper_mix"), ctrl).0 {
                changed = true;
            }
            if changed {
                let mut s = app.state.write();
                s.fx.waveshaper_drive = dr;
                s.fx.waveshaper_mix = mi;
            }
        }
        ModuleKind::FxBitcrush => {
            let (mut bi, mut ra, mut mi) = {
                let s = app.state.read();
                (s.fx.bitcrush_bits, s.fx.bitcrush_rate, s.fx.bitcrush_mix)
            };
            widgets::param_control(ui, "BITS", &mut bi, pm("fx.bitcrush_bits"), ctrl);
            widgets::param_control(ui, "RATE", &mut ra, pm("fx.bitcrush_rate"), ctrl);
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.bitcrush_mix"), ctrl).0 {
                changed = true;
            }
            if changed || bi != app.state.read().fx.bitcrush_bits {
                let mut s = app.state.write();
                s.fx.bitcrush_bits = bi;
                s.fx.bitcrush_rate = ra;
                s.fx.bitcrush_mix = mi;
            }
        }
        ModuleKind::FxRingMod => {
            let (mut fr, mut mi) = {
                let s = app.state.read();
                (s.fx.ring_mod_freq, s.fx.ring_mod_mix)
            };
            if widgets::param_control(ui, "FREQ", &mut fr, pm("fx.ring_mod_freq"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.ring_mod_mix"), ctrl).0 {
                changed = true;
            }
            if changed {
                let mut s = app.state.write();
                s.fx.ring_mod_freq = fr;
                s.fx.ring_mod_mix = mi;
            }
        }
        _ => {}
    }

    if changed {
        app.push_audio_params();
    }
}

pub(super) fn draw_lfo_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = {
        let rack = app.state.read();
        rack.rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::LfoModule)
            .enumerate()
            .find(|(_, m)| m.id == module_id)
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    crate::ui::panels::draw_lfo_slot(app, ui, slot);
}

pub(super) fn draw_master_content(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    use crate::ui::widgets;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let mut master_vol = app.state.read().fx.master_volume;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("MASTER VOL")
                .monospace()
                .size(9.0)
                .color(egui::Color32::from_gray(90)),
        );
        if widgets::param_control(ui, "", &mut master_vol, crate::state::ParamMode::Free, ctrl).0 {
            app.state.write().fx.master_volume = master_vol;
            app.push_audio_params();
        }

        ui.separator();
        let rack = app.state.read();
        let voice_kinds = [
            (crate::state::ModuleKind::AcidBass, "BASS"),
            (crate::state::ModuleKind::DrumKit808, "808"),
            (crate::state::ModuleKind::DrumKit909, "909"),
            (crate::state::ModuleKind::HooverLead, "HVVR"),
            (crate::state::ModuleKind::An1xVoice, "AN1X"),
            (crate::state::ModuleKind::AmenSampler, "AMEN"),
        ];
        for (kind, label) in &voice_kinds {
            let present = rack
                .rack
                .modules
                .iter()
                .any(|m| m.kind == *kind && m.enabled);
            let col = if present {
                egui::Color32::from_gray(160)
            } else {
                egui::Color32::from_gray(28)
            };
            ui.label(egui::RichText::new(*label).monospace().size(8.0).color(col));
        }
    });
}

// ─── Cable drag interaction ───────────────────────────────────────────────────

pub(super) fn handle_cable_drag(
    app: &mut ImpulseApp,
    ctx: &egui::Context,
    ports: &[module_card::PortPos],
) {
    let pointer = match ctx.pointer_latest_pos() {
        Some(p) => p,
        None => return,
    };
    let primary_down = ctx.input(|i| i.pointer.primary_down());
    let primary_released = ctx.input(|i| i.pointer.primary_released());

    let hovered_port = ports
        .iter()
        .find(|pp| pp.center.distance(pointer) <= module_card::PORT_RADIUS + 3.0);

    if primary_down
        && app.cable_drag.is_none()
        && let Some(pp) = hovered_port
    {
        app.cable_drag = Some(super::rack_canvas::CableDrag {
            from_port: pp.port.clone(),
            from_screen: pp.center,
        });
    }

    if primary_released
        && let Some(drag) = app.cable_drag.take()
        && let Some(target) = hovered_port
        && drag.from_port.dir != target.port.dir
        && drag.from_port.kind == target.port.kind
        && drag.from_port.module_id != target.port.module_id
    {
        let (from, to) = if drag.from_port.dir == crate::state::PortDir::Out {
            (drag.from_port, target.port.clone())
        } else {
            (target.port.clone(), drag.from_port)
        };
        app.state.write().rack.connect(from, to);
        app.push_fx_plan();
    }

    // Right-click a port to disconnect all cables attached to it.
    let secondary_released = ctx.input(|i| i.pointer.secondary_released());
    if secondary_released
        && app.cable_drag.is_none()
        && let Some(pp) = hovered_port
    {
        let prev_len = app.state.read().rack.cables.len();
        app.state
            .write()
            .rack
            .cables
            .retain(|c| c.from != pp.port && c.to != pp.port);
        if app.state.read().rack.cables.len() != prev_len {
            app.push_fx_plan();
        }
    }
}

fn draw_noise_stub(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    use crate::ui::widgets;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);

    let (mut vol, mut color, mut cutoff) = {
        let s = app.state.read();
        (
            s.noise_voice.volume,
            s.noise_voice.color,
            s.noise_voice.cutoff,
        )
    };
    let mut changed = false;
    if widgets::param_control(ui, "VOLUME", &mut vol, pm("noise_voice.volume"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "COLOR", &mut color, pm("noise_voice.color"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "CUTOFF", &mut cutoff, pm("noise_voice.cutoff"), ctrl).0 {
        changed = true;
    }
    if changed {
        {
            let mut s = app.state.write();
            s.noise_voice.volume = vol;
            s.noise_voice.color = color;
            s.noise_voice.cutoff = cutoff;
        }
        app.push_audio_params();
    }
}
