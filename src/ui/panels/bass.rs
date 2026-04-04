// ─── ui/panels/bass.rs ────────────────────────────────────────────────────────
// Bass synthesizer panel.

use crate::state::{FilterMode, ParamMode, Waveform, cycle_param_mode, param_mode};
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_bass(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    widgets::section_header(ui, "BASS SYNTHESIZER");

    // Snapshot everything needed for rendering — lock released before any widget call
    let (
        mut cutoff,
        mut resonance,
        mut env_mod,
        mut decay,
        mut accent,
        mut dist,
        mut vol,
        mut sub_osc_level,
        waveform,
        filter_mode,
        mut supersaw_detune,
        supersaw_voices,
        locked,
        focused,
        auto_lock,
    ) = {
        let s = app.state.read();
        (
            s.bass.cutoff,
            s.bass.resonance,
            s.bass.env_mod,
            s.bass.decay,
            s.bass.accent_level,
            s.bass.distortion,
            s.bass.volume,
            s.bass.sub_osc_level,
            s.bass.waveform.clone(),
            s.bass.filter_mode.clone(),
            s.bass.supersaw_detune,
            s.bass.supersaw_voices,
            s.llm.locked_params.clone(),
            s.llm.focused_params.clone(),
            s.llm.auto_lock_on_touch,
        )
    };

    let mut cycle_paths: Vec<&str> = Vec::new();
    let mut changed = false;

    let use_sliders = app.use_sliders;
    let draw_bass_controls = |ui: &mut egui::Ui| {
        let (ch, cy) = widgets::param_control(
            ui,
            "CUTOFF",
            &mut cutoff,
            param_mode("bass.cutoff", &locked, &focused),
            use_sliders,
        );
        if ch {
            changed = true;
        }
        if cy {
            cycle_paths.push("bass.cutoff");
        }
        let (ch, cy) = widgets::param_control(
            ui,
            "RESONANCE",
            &mut resonance,
            param_mode("bass.resonance", &locked, &focused),
            use_sliders,
        );
        if ch {
            changed = true;
        }
        if cy {
            cycle_paths.push("bass.resonance");
        }
        let (ch, cy) = widgets::param_control(
            ui,
            "ENV MOD",
            &mut env_mod,
            param_mode("bass.env_mod", &locked, &focused),
            use_sliders,
        );
        if ch {
            changed = true;
        }
        if cy {
            cycle_paths.push("bass.env_mod");
        }
        let (ch, cy) = widgets::param_control(
            ui,
            "DECAY",
            &mut decay,
            param_mode("bass.decay", &locked, &focused),
            use_sliders,
        );
        if ch {
            changed = true;
        }
        if cy {
            cycle_paths.push("bass.decay");
        }
        let (ch, cy) = widgets::param_control(
            ui,
            "ACCENT",
            &mut accent,
            param_mode("bass.accent_level", &locked, &focused),
            use_sliders,
        );
        if ch {
            changed = true;
        }
        if cy {
            cycle_paths.push("bass.accent_level");
        }
        let (ch, cy) = widgets::param_control(
            ui,
            "DRIVE",
            &mut dist,
            param_mode("bass.distortion", &locked, &focused),
            use_sliders,
        );
        if ch {
            changed = true;
        }
        if cy {
            cycle_paths.push("bass.distortion");
        }
        let (ch, cy) = widgets::param_control(
            ui,
            "VOLUME",
            &mut vol,
            param_mode("bass.volume", &locked, &focused),
            use_sliders,
        );
        if ch {
            changed = true;
        }
        if cy {
            cycle_paths.push("bass.volume");
        }
        let (ch, cy) = widgets::param_control(
            ui,
            "SUB OSC",
            &mut sub_osc_level,
            param_mode("bass.sub_osc_level", &locked, &focused),
            use_sliders,
        );
        if ch {
            changed = true;
        }
        if cy {
            cycle_paths.push("bass.sub_osc_level");
        }
    };
    if use_sliders {
        ui.vertical(draw_bass_controls);
    } else {
        ui.horizontal_wrapped(draw_bass_controls);
    }

    // Apply all changes in a single brief write, using pure state transitions
    let needs_write = changed || !cycle_paths.is_empty();
    if needs_write {
        let mut snap = app.state.read().clone();
        if changed {
            snap.bass.cutoff = cutoff;
            snap.bass.resonance = resonance;
            snap.bass.env_mod = env_mod;
            snap.bass.decay = decay;
            snap.bass.accent_level = accent;
            snap.bass.distortion = dist;
            snap.bass.volume = vol;
            snap.bass.sub_osc_level = sub_osc_level;
            // auto_lock: touching a free param immediately makes it UserOwned
            if auto_lock {
                for p in [
                    "bass.cutoff",
                    "bass.resonance",
                    "bass.env_mod",
                    "bass.decay",
                    "bass.accent_level",
                    "bass.distortion",
                    "bass.volume",
                    "bass.sub_osc_level",
                ] {
                    if snap.llm.locked_params.contains(p) {
                        continue;
                    }
                    if snap.llm.focused_params.contains(p) {
                        continue;
                    }
                    snap.llm.locked_params.insert(p.to_string());
                }
            }
        }
        for path in &cycle_paths {
            snap = cycle_param_mode(snap, path);
        }
        *app.state.write() = snap;
        if changed {
            app.push_audio_params();
        }
    }

    ui.add_space(6.0);

    // XY Control Squares — two 2D pads for the core acid parameters
    let xy1_locked = param_mode("bass.cutoff", &locked, &focused) == ParamMode::UserOwned
        || param_mode("bass.resonance", &locked, &focused) == ParamMode::UserOwned;
    let xy2_locked = param_mode("bass.env_mod", &locked, &focused) == ParamMode::UserOwned
        || param_mode("bass.decay", &locked, &focused) == ParamMode::UserOwned;

    ui.horizontal(|ui| {
        // Pad 1: Cutoff (X) × Resonance (Y)
        if widgets::xy_pad(
            ui,
            "CUT",
            "RES",
            &mut cutoff,
            &mut resonance,
            88.0,
            xy1_locked,
        ) {
            let mut snap = app.state.read().clone();
            snap.bass.cutoff = cutoff;
            snap.bass.resonance = resonance;
            *app.state.write() = snap;
            app.push_audio_params();
        }
        ui.add_space(6.0);
        // Pad 2: Env Mod (X) × Decay (Y)
        if widgets::xy_pad(ui, "ENV", "DEC", &mut env_mod, &mut decay, 88.0, xy2_locked) {
            let mut snap = app.state.read().clone();
            snap.bass.env_mod = env_mod;
            snap.bass.decay = decay;
            *app.state.write() = snap;
            app.push_audio_params();
        }
    });

    ui.add_space(8.0);

    // Waveform toggle
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("WAVE")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        let mut saw = waveform == Waveform::Saw;
        if widgets::toggle_button(ui, "SAW", &mut saw) {
            app.state.write().bass.waveform = Waveform::Saw;
            app.push_audio_params();
        }
        let mut sq = waveform == Waveform::Square;
        if widgets::toggle_button(ui, "SQR", &mut sq) {
            app.state.write().bass.waveform = Waveform::Square;
            app.push_audio_params();
        }
        let mut ss = waveform == Waveform::Supersaw;
        if widgets::toggle_button(ui, "SUPER", &mut ss) {
            app.state.write().bass.waveform = Waveform::Supersaw;
            app.push_audio_params();
        }
    });

    // Filter mode toggle
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("FILT")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        let mut lp = filter_mode == FilterMode::Lowpass;
        if widgets::toggle_button(ui, "LP", &mut lp) {
            app.state.write().bass.filter_mode = FilterMode::Lowpass;
            app.push_audio_params();
        }
        let mut hp = filter_mode == FilterMode::Highpass;
        if widgets::toggle_button(ui, "HP", &mut hp) {
            app.state.write().bass.filter_mode = FilterMode::Highpass;
            app.push_audio_params();
        }
        let mut bp = filter_mode == FilterMode::Bandpass;
        if widgets::toggle_button(ui, "BP", &mut bp) {
            app.state.write().bass.filter_mode = FilterMode::Bandpass;
            app.push_audio_params();
        }
    });

    // Supersaw controls (only shown when Supersaw is active)
    if waveform == Waveform::Supersaw {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("DETUNE")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            if widgets::param_control(ui, "", &mut supersaw_detune, ParamMode::Free, use_sliders).0
            {
                let mut s = app.state.write();
                s.bass.supersaw_detune = supersaw_detune;
                drop(s);
                app.push_audio_params();
            }
            let voices_label = format!("{}V", supersaw_voices);
            ui.label(
                egui::RichText::new("VOICES")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            if ui.small_button("-").clicked() && supersaw_voices > 2 {
                app.state.write().bass.supersaw_voices = supersaw_voices - 1;
                app.push_audio_params();
            }
            ui.label(
                egui::RichText::new(&voices_label)
                    .color(theme::CHALK)
                    .monospace()
                    .size(9.0),
            );
            if ui.small_button("+").clicked() && supersaw_voices < 7 {
                app.state.write().bass.supersaw_voices = supersaw_voices + 1;
                app.push_audio_params();
            }
        });
    }

    ui.add_space(12.0);

    // Locked params management
    let locked_bass: Vec<String> = locked
        .iter()
        .filter(|p| p.starts_with("bass"))
        .cloned()
        .collect();

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("LOCKED:")
                .color(theme::SMOKE)
                .monospace()
                .size(8.5),
        );
        if locked_bass.is_empty() {
            ui.label(
                egui::RichText::new("none (LLM controls all)")
                    .color(theme::IRON)
                    .monospace()
                    .size(8.5),
            );
        } else {
            let mut to_remove: Option<String> = None;
            for p in &locked_bass {
                let short = p.replace("bass.", "");
                if ui
                    .small_button(
                        egui::RichText::new(format!("× {}", short))
                            .monospace()
                            .size(8.0),
                    )
                    .clicked()
                {
                    to_remove = Some(p.clone());
                }
            }
            if let Some(p) = to_remove {
                let next = crate::state::unlock_param(app.state.read().clone(), &p);
                *app.state.write() = next;
            }
        }
        if ui
            .small_button(egui::RichText::new("UNLOCK ALL").monospace().size(8.0))
            .clicked()
        {
            let mut next = app.state.read().clone();
            next.llm.locked_params.retain(|p| !p.starts_with("bass"));
            *app.state.write() = next;
        }
    });
}
