// ─── ui/panels/bass.rs ────────────────────────────────────────────────────────
// Bass synthesizer panel.

use crate::state::{FilterMode, ParamMode, Waveform, cycle_param_mode, param_mode, set_param_mode};
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_bass(app: &mut ImpulseApp, ui: &mut egui::Ui) {
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
        mut portamento_time,
        mut noise_mix,
        mut osc_detune,
        mut fm_ratio,
        mut fm_depth,
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
            s.bass.portamento_time,
            s.bass.noise_mix,
            s.bass.osc_detune,
            s.bass.fm_ratio,
            s.bass.fm_depth,
            s.bass.waveform.clone(),
            s.bass.filter_mode,
            s.bass.supersaw_detune,
            s.bass.supersaw_voices,
            s.llm.locked_params.clone(),
            s.llm.focused_params.clone(),
            s.llm.auto_lock_on_touch,
        )
    };

    let mut cycle_paths: Vec<&str> = Vec::new();
    let mut changed = false;

    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let xy_size = app.state.read().ui_prefs.pad_size.px() * (88.0 / 26.0);

    // Helper macro — avoids repeating the changed/cycle boilerplate 13 times.
    // Not a real macro here; we use a local closure that borrows changed/cycle_paths.
    // (Rust closures can capture by &mut only if nothing else borrows them at the
    //  same time — fine here since both are unused after the scope.)
    if ctrl.style == widgets::ControlStyle::Sliders {
        // ── Slider mode: plain vertical list ─────────────────────────────────
        macro_rules! p {
            ($label:expr, $val:expr, $mode:expr, $path:expr) => {{
                let (ch, cy) = widgets::param_control(ui, $label, &mut $val, $mode, ctrl);
                if ch {
                    changed = true;
                }
                if cy {
                    cycle_paths.push($path);
                }
            }};
        }
        p!(
            "CUTOFF",
            cutoff,
            param_mode("bass.cutoff", &locked, &focused),
            "bass.cutoff"
        );
        p!(
            "RES",
            resonance,
            param_mode("bass.resonance", &locked, &focused),
            "bass.resonance"
        );
        p!(
            "ENV MOD",
            env_mod,
            param_mode("bass.env_mod", &locked, &focused),
            "bass.env_mod"
        );
        p!(
            "DECAY",
            decay,
            param_mode("bass.decay", &locked, &focused),
            "bass.decay"
        );
        p!(
            "ACCENT",
            accent,
            param_mode("bass.accent_level", &locked, &focused),
            "bass.accent_level"
        );
        p!(
            "DRIVE",
            dist,
            param_mode("bass.distortion", &locked, &focused),
            "bass.distortion"
        );
        p!(
            "VOLUME",
            vol,
            param_mode("bass.volume", &locked, &focused),
            "bass.volume"
        );
        p!(
            "SUB OSC",
            sub_osc_level,
            param_mode("bass.sub_osc_level", &locked, &focused),
            "bass.sub_osc_level"
        );
        p!(
            "GLIDE",
            portamento_time,
            param_mode("bass.portamento_time", &locked, &focused),
            "bass.portamento_time"
        );
        p!(
            "NOISE",
            noise_mix,
            param_mode("bass.noise_mix", &locked, &focused),
            "bass.noise_mix"
        );
        p!("FM DEPTH", fm_depth, ParamMode::Free, "bass.fm_depth");
        p!("FM RATIO", fm_ratio, ParamMode::Free, "bass.fm_ratio");
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("DETUNE")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            if ui
                .add(
                    egui::DragValue::new(&mut osc_detune)
                        .range(-1.0..=1.0)
                        .speed(0.01)
                        .fixed_decimals(2)
                        .suffix(" st"),
                )
                .changed()
            {
                changed = true;
            }
        });
    } else {
        // ── Knob mode: 4-column grid ──────────────────────────────────────────
        // Row layout:  CUT  | RES  | ENV  | DEC
        //              ACC  | DRV  | VOL  | SUB
        //              GLD  | NSE  | FMD  | FMR
        egui::Grid::new("bass_knobs")
            .num_columns(4)
            .spacing([4.0, 2.0])
            .show(ui, |ui| {
                macro_rules! k {
                    ($label:expr, $val:expr, $mode:expr, $path:expr) => {{
                        let (ch, cy) = widgets::param_control(ui, $label, &mut $val, $mode, ctrl);
                        if ch {
                            changed = true;
                        }
                        if cy {
                            cycle_paths.push($path);
                        }
                    }};
                    // FM params with no lock path
                    ($label:expr, $val:expr) => {{
                        let (ch, _) =
                            widgets::param_control(ui, $label, &mut $val, ParamMode::Free, ctrl);
                        if ch {
                            changed = true;
                        }
                    }};
                }
                // Row 1 — filter core
                k!(
                    "CUT",
                    cutoff,
                    param_mode("bass.cutoff", &locked, &focused),
                    "bass.cutoff"
                );
                k!(
                    "RES",
                    resonance,
                    param_mode("bass.resonance", &locked, &focused),
                    "bass.resonance"
                );
                k!(
                    "ENV",
                    env_mod,
                    param_mode("bass.env_mod", &locked, &focused),
                    "bass.env_mod"
                );
                k!(
                    "DEC",
                    decay,
                    param_mode("bass.decay", &locked, &focused),
                    "bass.decay"
                );
                ui.end_row();
                // Row 2 — character
                k!(
                    "ACC",
                    accent,
                    param_mode("bass.accent_level", &locked, &focused),
                    "bass.accent_level"
                );
                k!(
                    "DRV",
                    dist,
                    param_mode("bass.distortion", &locked, &focused),
                    "bass.distortion"
                );
                k!(
                    "VOL",
                    vol,
                    param_mode("bass.volume", &locked, &focused),
                    "bass.volume"
                );
                k!(
                    "SUB",
                    sub_osc_level,
                    param_mode("bass.sub_osc_level", &locked, &focused),
                    "bass.sub_osc_level"
                );
                ui.end_row();
                // Row 3 — modulation
                k!(
                    "GLD",
                    portamento_time,
                    param_mode("bass.portamento_time", &locked, &focused),
                    "bass.portamento_time"
                );
                k!(
                    "NSE",
                    noise_mix,
                    param_mode("bass.noise_mix", &locked, &focused),
                    "bass.noise_mix"
                );
                {
                    let (ch, cy) =
                        widgets::param_control(ui, "FMD", &mut fm_depth, ParamMode::Free, ctrl);
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.fm_depth");
                    }
                }
                {
                    let (ch, cy) =
                        widgets::param_control(ui, "FMR", &mut fm_ratio, ParamMode::Free, ctrl);
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.fm_ratio");
                    }
                }
                ui.end_row();
            });

        // Detune: bipolar DragValue — one compact row below the grid
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("DETUNE")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            if ui
                .add(
                    egui::DragValue::new(&mut osc_detune)
                        .range(-1.0..=1.0)
                        .speed(0.01)
                        .fixed_decimals(2)
                        .suffix(" st"),
                )
                .changed()
            {
                changed = true;
            }
        });
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
            snap.bass.portamento_time = portamento_time;
            snap.bass.noise_mix = noise_mix;
            snap.bass.osc_detune = osc_detune;
            snap.bass.fm_ratio = fm_ratio;
            snap.bass.fm_depth = fm_depth;
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
                    "bass.portamento_time",
                    "bass.noise_mix",
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
        // Touch-paint mode: set to the active mode; otherwise cycle Free→U→F→Free.
        let tmode = app.touch_mode;
        for path in &cycle_paths {
            snap = if let Some(m) = tmode {
                set_param_mode(snap, path, m)
            } else {
                cycle_param_mode(snap, path)
            };
        }
        *app.state.write() = snap;
        if changed {
            app.push_audio_params();
        }
    }

    ui.add_space(2.0);

    // XY Control Squares — two 2D pads for the core acid parameters
    let xy1_locked = param_mode("bass.cutoff", &locked, &focused) == ParamMode::UserOwned
        || param_mode("bass.resonance", &locked, &focused) == ParamMode::UserOwned;
    let xy2_locked = param_mode("bass.env_mod", &locked, &focused) == ParamMode::UserOwned
        || param_mode("bass.decay", &locked, &focused) == ParamMode::UserOwned;

    ui.horizontal(|ui| {
        // Pad 1 — cycles: CUT×RES | ACCENT×VOL | DIST×SUB
        let p1 = widgets::xy_pad_pair(ui.ctx(), "bass_xy1");
        let (lx1, ly1, mut vx1, mut vy1) = match p1 {
            1 => ("ACCENT", "VOL", accent, vol),
            2 => ("DIST", "SUB", dist, sub_osc_level),
            _ => ("CUT", "RES", cutoff, resonance),
        };
        let xy1_locked_cur = match p1 {
            1 => {
                param_mode("bass.accent_level", &locked, &focused) == ParamMode::UserOwned
                    || param_mode("bass.volume", &locked, &focused) == ParamMode::UserOwned
            }
            2 => {
                param_mode("bass.distortion", &locked, &focused) == ParamMode::UserOwned
                    || param_mode("bass.sub_osc_level", &locked, &focused) == ParamMode::UserOwned
            }
            _ => xy1_locked,
        };
        if let (true, _) = widgets::xy_pad(
            ui,
            "bass_xy1",
            lx1,
            ly1,
            &mut vx1,
            &mut vy1,
            xy_size,
            xy1_locked_cur,
            3,
        ) {
            let mut snap = app.state.read().clone();
            match p1 {
                1 => {
                    snap.bass.accent_level = vx1;
                    snap.bass.volume = vy1;
                }
                2 => {
                    snap.bass.distortion = vx1;
                    snap.bass.sub_osc_level = vy1;
                }
                _ => {
                    snap.bass.cutoff = vx1;
                    snap.bass.resonance = vy1;
                }
            }
            *app.state.write() = snap;
            app.push_audio_params();
        }

        ui.add_space(6.0);

        // Pad 2 — cycles: ENV×DEC | FM.D×FM.R | NOISE×GLIDE
        let p2 = widgets::xy_pad_pair(ui.ctx(), "bass_xy2");
        let (lx2, ly2, mut vx2, mut vy2) = match p2 {
            1 => ("FM.D", "FM.R", fm_depth, fm_ratio),
            2 => ("NOISE", "GLIDE", noise_mix, portamento_time),
            _ => ("ENV", "DEC", env_mod, decay),
        };
        let xy2_locked_cur = match p2 {
            1 => false,
            2 => false,
            _ => xy2_locked,
        };
        if let (true, _) = widgets::xy_pad(
            ui,
            "bass_xy2",
            lx2,
            ly2,
            &mut vx2,
            &mut vy2,
            xy_size,
            xy2_locked_cur,
            3,
        ) {
            let mut snap = app.state.read().clone();
            match p2 {
                1 => {
                    snap.bass.fm_depth = vx2;
                    snap.bass.fm_ratio = vy2;
                }
                2 => {
                    snap.bass.noise_mix = vx2;
                    snap.bass.portamento_time = vy2;
                }
                _ => {
                    snap.bass.env_mod = vx2;
                    snap.bass.decay = vy2;
                }
            }
            *app.state.write() = snap;
            app.push_audio_params();
        }
    });

    ui.add_space(2.0);

    // Decay envelope visualiser — shows the 303's decay-only filter envelope shape.
    // Width spans both pads (2×xy_size + spacing); height scales with pad size.
    let env_w = (xy_size * 2.0 + 14.0).max(200.0);
    let env_h = (xy_size * 0.30).max(28.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("ENV")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        if widgets::decay_display(ui, &mut decay, env_mod, env_w, env_h) {
            let mut snap = app.state.read().clone();
            snap.bass.decay = decay;
            *app.state.write() = snap;
            app.push_audio_params();
        }
    });

    ui.add_space(2.0);

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
            if widgets::param_control(ui, "", &mut supersaw_detune, ParamMode::Free, ctrl).0 {
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

    // Preset shortcuts
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("PRESET")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        if ui
            .add(egui::Button::new(
                egui::RichText::new("REESE")
                    .monospace()
                    .size(9.0)
                    .color(theme::FOG),
            ))
            .on_hover_text("Detuned dual saws + sub + highpass — classic DnB/jungle bass")
            .clicked()
        {
            let s = app.state.read().clone();
            *app.state.write() = crate::state::apply_reese_preset(s);
            app.push_audio_params();
        }
    });

    ui.add_space(4.0);

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

    ui.add_space(4.0);
    widgets::section_header(ui, "NOISE VOICE");

    let (noise_enabled, mut noise_volume, mut noise_color, mut noise_cutoff) = {
        let s = app.state.read();
        (
            s.noise_voice.enabled,
            s.noise_voice.volume,
            s.noise_voice.color,
            s.noise_voice.cutoff,
        )
    };

    ui.horizontal(|ui| {
        // ON toggle
        let on_color = if noise_enabled {
            theme::CHALK
        } else {
            theme::IRON
        };
        let on_fill = if noise_enabled {
            egui::Color32::from_gray(50)
        } else {
            egui::Color32::TRANSPARENT
        };
        if ui
            .add_sized(
                [28.0, 18.0],
                egui::Button::new(
                    egui::RichText::new("ON")
                        .monospace()
                        .size(9.0)
                        .color(on_color),
                )
                .fill(on_fill),
            )
            .clicked()
        {
            app.state.write().noise_voice.enabled = !noise_enabled;
            app.push_audio_params();
        }

        ui.label(
            egui::RichText::new("VOL")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        if ui
            .add(
                egui::Slider::new(&mut noise_volume, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .changed()
        {
            app.state.write().noise_voice.volume = noise_volume;
            app.push_audio_params();
        }
    });

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("COLOR (WHITE→BROWN)")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        if ui
            .add(
                egui::Slider::new(&mut noise_color, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .changed()
        {
            app.state.write().noise_voice.color = noise_color;
            app.push_audio_params();
        }
        let color_label = if noise_color < 0.25 {
            "WHITE"
        } else if noise_color < 0.75 {
            "PINK"
        } else {
            "BROWN"
        };
        ui.label(
            egui::RichText::new(color_label)
                .color(theme::FOG)
                .monospace()
                .size(9.0),
        );
    });

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("CUTOFF")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        if ui
            .add(
                egui::Slider::new(&mut noise_cutoff, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .changed()
        {
            app.state.write().noise_voice.cutoff = noise_cutoff;
            app.push_audio_params();
        }
    });
}
