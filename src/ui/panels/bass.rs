// ─── ui/panels/bass.rs ────────────────────────────────────────────────────────
// Bass synthesizer panel.

use crate::state::{FilterMode, ParamMode, Waveform, cycle_param_mode, param_mode, set_param_mode};
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_bass(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // ── Voice selector ────────────────────────────────────────────────────────
    {
        let (active, _, voice_enabled) = {
            let s = app.state.read();
            let enabled: Vec<bool> = s.bass_voices.iter().map(|v| v.enabled).collect();
            (s.active_voice, s.bass_voices.len(), enabled)
        };
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("VOICE")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(8.5),
            );
            for (i, &enabled) in voice_enabled.iter().enumerate() {
                let is_active = i == active;
                let label = format!("V{}", i + 1);
                let color = if is_active {
                    theme::CHALK
                } else if enabled {
                    theme::FOG
                } else {
                    theme::IRON
                };
                let fill = if is_active {
                    egui::Color32::from_gray(55)
                } else {
                    egui::Color32::from_gray(22)
                };
                if ui
                    .add_sized(
                        [26.0, 18.0],
                        egui::Button::new(
                            egui::RichText::new(&label)
                                .monospace()
                                .size(8.5)
                                .color(color),
                        )
                        .fill(fill),
                    )
                    .clicked()
                {
                    app.state.write().active_voice = i;
                }
            }
            // Enable toggle for voices 1-3 (voice 0 is always on)
            if active > 0 {
                let enabled = voice_enabled[active];
                let btn_text = if enabled { "ON" } else { "OFF" };
                let btn_color = if enabled { theme::CHALK } else { theme::IRON };
                let btn_fill = if enabled {
                    egui::Color32::from_gray(50)
                } else {
                    egui::Color32::TRANSPARENT
                };
                if ui
                    .add_sized(
                        [32.0, 18.0],
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
                    let av = app.state.read().active_voice;
                    let cur = app.state.read().bass_voices[av].enabled;
                    app.state.write().bass_voices[av].enabled = !cur;
                }
            }
        });
        // Per-voice key: checkbox to lock to global, and selectors when unlocked
        let (lock_key, voice_root_note, voice_scale) = {
            let s = app.state.read();
            let av = s.active_voice.min(s.bass_voices.len().saturating_sub(1));
            let v = &s.bass_voices[av];
            (v.lock_key, v.root_note, v.scale)
        };
        ui.horizontal(|ui| {
            let mut lk = lock_key;
            if ui
                .checkbox(
                    &mut lk,
                    egui::RichText::new("GLOBAL KEY")
                        .color(theme::SMOKE)
                        .monospace()
                        .size(8.5),
                )
                .changed()
            {
                let av = app.state.read().active_voice;
                app.state.write().bass_voices[av].lock_key = lk;
            }
            if !lock_key {
                // Root note buttons (C through B)
                ui.separator();
                let note_names = [
                    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                ];
                for (n, name) in note_names.iter().enumerate() {
                    let active_note = n as u8 == voice_root_note;
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(*name).monospace().size(7.5).color(
                                    if active_note {
                                        theme::CHALK
                                    } else {
                                        theme::IRON
                                    },
                                ),
                            )
                            .fill(if active_note {
                                theme::SLATE
                            } else {
                                theme::PIT
                            })
                            .min_size(egui::vec2(18.0, 14.0)),
                        )
                        .clicked()
                    {
                        let av = app.state.read().active_voice;
                        app.state.write().bass_voices[av].root_note = n as u8;
                    }
                }
                let _ = voice_scale; // used for future scale selector UI
            }
        });
        ui.add_space(2.0);
    }

    // Snapshot everything needed for rendering — lock released before any widget call
    let active_voice = app.state.read().active_voice;
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
        let av = active_voice.min(s.bass_voices.len().saturating_sub(1));
        let b = &s.bass_voices[av].synth;
        (
            b.cutoff,
            b.resonance,
            b.env_mod,
            b.decay,
            b.accent_level,
            b.distortion,
            b.volume,
            b.sub_osc_level,
            b.portamento_time,
            b.noise_mix,
            b.osc_detune,
            b.fm_ratio,
            b.fm_depth,
            b.waveform.clone(),
            b.filter_mode,
            b.supersaw_detune,
            b.supersaw_voices,
            s.llm.locked_params.clone(),
            s.llm.focused_params.clone(),
            s.llm.auto_lock_on_touch,
        )
    };

    let mut cycle_paths: Vec<&str> = Vec::new();
    let mut changed = false;

    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let xy_size = app.state.read().ui_prefs.effective_xy_px();

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
        if widgets::param_control_bipolar(ui, "DETUNE", &mut osc_detune, ParamMode::Free, ctrl).0 {
            changed = true;
        }
    } else {
        // ── Knob mode: FILTER + CHARACTER wide, MOD compact ──────────────────
        let ctrl_big = ctrl.phi_bigger(); // primary: cutoff, resonance
        let ctrl_sm = ctrl.phi_smaller(); // secondary: glide, noise, FM
        let avail = ui.available_width();
        let spacing = ui.spacing().item_spacing.x;
        // FILTER and CHARACTER get 40% each, MOD gets 20%
        let gw_main = ((avail - spacing * 2.0) * 0.40).floor();
        let gw_mod = avail - gw_main * 2.0 - spacing * 2.0;
        // Fixed height so all three glass groups match
        // Height based on the largest knobs (FILTER uses ctrl_big)
        let group_h = ctrl_big.knob_size * 2.0 + 50.0;
        ui.horizontal(|ui| {
            // FILTER group: 2×2 grid (CUT/RES, ENV/DEC)
            widgets::glass_group_fill(ui, gw_main, gw_main, |ui| {
                ui.set_min_height(group_h);
                ui.label(
                    egui::RichText::new("FILTER")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "CUT",
                        &mut cutoff,
                        param_mode("bass.cutoff", &locked, &focused),
                        ctrl_big,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.cutoff");
                    }
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "RES",
                        &mut resonance,
                        param_mode("bass.resonance", &locked, &focused),
                        ctrl_big,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.resonance");
                    }
                });
                widgets::centered_row(ui, |ui| {
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "ENV",
                        &mut env_mod,
                        param_mode("bass.env_mod", &locked, &focused),
                        ctrl,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.env_mod");
                    }
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "DEC",
                        &mut decay,
                        param_mode("bass.decay", &locked, &focused),
                        ctrl,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.decay");
                    }
                });
            });
            // CHARACTER group: ACC, DRV, VOL, SUB
            widgets::glass_group_fill(ui, gw_main, gw_main, |ui| {
                ui.set_min_height(group_h);
                ui.label(
                    egui::RichText::new("CHARACTER")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "ACC",
                        &mut accent,
                        param_mode("bass.accent_level", &locked, &focused),
                        ctrl,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.accent_level");
                    }
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "DRV",
                        &mut dist,
                        param_mode("bass.distortion", &locked, &focused),
                        ctrl,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.distortion");
                    }
                });
                widgets::centered_row(ui, |ui| {
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "VOL",
                        &mut vol,
                        param_mode("bass.volume", &locked, &focused),
                        ctrl,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.volume");
                    }
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "SUB",
                        &mut sub_osc_level,
                        param_mode("bass.sub_osc_level", &locked, &focused),
                        ctrl,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.sub_osc_level");
                    }
                });
            });
            // MOD group: GLD, NSE, FMD, FMR (compact)
            widgets::glass_group_fill(ui, gw_mod, gw_mod, |ui| {
                ui.set_min_height(group_h);
                ui.label(
                    egui::RichText::new("MOD")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "GLD",
                        &mut portamento_time,
                        param_mode("bass.portamento_time", &locked, &focused),
                        ctrl_sm,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.portamento_time");
                    }
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "NSE",
                        &mut noise_mix,
                        param_mode("bass.noise_mix", &locked, &focused),
                        ctrl_sm,
                    );
                    if ch {
                        changed = true;
                    }
                    if cy {
                        cycle_paths.push("bass.noise_mix");
                    }
                });
                widgets::centered_row(ui, |ui| {
                    if widgets::param_control(ui, "FMD", &mut fm_depth, ParamMode::Free, ctrl_sm).0
                    {
                        changed = true;
                    }
                    if widgets::param_control(ui, "FMR", &mut fm_ratio, ParamMode::Free, ctrl_sm).0
                    {
                        changed = true;
                    }
                });
                if widgets::param_control_bipolar(
                    ui,
                    "DTN",
                    &mut osc_detune,
                    ParamMode::Free,
                    ctrl_sm,
                )
                .0
                {
                    changed = true;
                }
            });
        });
    }

    // Apply all changes in a single brief write, using pure state transitions
    let needs_write = changed || !cycle_paths.is_empty();
    if needs_write {
        let mut snap = app.state.read().clone();
        if changed {
            let av = snap
                .active_voice
                .min(snap.bass_voices.len().saturating_sub(1));
            snap.bass_voices[av].synth.cutoff = cutoff;
            snap.bass_voices[av].synth.resonance = resonance;
            snap.bass_voices[av].synth.env_mod = env_mod;
            snap.bass_voices[av].synth.decay = decay;
            snap.bass_voices[av].synth.accent_level = accent;
            snap.bass_voices[av].synth.distortion = dist;
            snap.bass_voices[av].synth.volume = vol;
            snap.bass_voices[av].synth.sub_osc_level = sub_osc_level;
            snap.bass_voices[av].synth.portamento_time = portamento_time;
            snap.bass_voices[av].synth.noise_mix = noise_mix;
            snap.bass_voices[av].synth.osc_detune = osc_detune;
            snap.bass_voices[av].synth.fm_ratio = fm_ratio;
            snap.bass_voices[av].synth.fm_depth = fm_depth;
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
            app.observe_edits(&[
                ("bass.cutoff", cutoff),
                ("bass.resonance", resonance),
                ("bass.env_mod", env_mod),
                ("bass.decay", decay),
                ("bass.accent_level", accent),
                ("bass.distortion", dist),
                ("bass.volume", vol),
            ]);
        }
    }

    // Pan slider
    {
        let mut pan = app.state.read().bass_voices
            [active_voice.min(app.state.read().bass_voices.len().saturating_sub(1))]
        .synth
        .pan;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("PAN")
                    .monospace()
                    .size(8.0)
                    .color(theme::SMOKE),
            );
            if widgets::pan_slider(ui, &mut pan, 200.0) {
                let av = active_voice.min(app.state.read().bass_voices.len().saturating_sub(1));
                app.state.write().bass_voices[av].synth.pan = pan;
                app.push_audio_params();
            }
        });
    }
    ui.add_space(2.0);

    // XY Control Squares — two 2D pads for the core acid parameters
    let xy1_locked = param_mode("bass.cutoff", &locked, &focused) == ParamMode::UserOwned
        || param_mode("bass.resonance", &locked, &focused) == ParamMode::UserOwned;
    let xy2_locked = param_mode("bass.env_mod", &locked, &focused) == ParamMode::UserOwned
        || param_mode("bass.decay", &locked, &focused) == ParamMode::UserOwned;

    ui.vertical_centered(|ui| {
        ui.horizontal(|ui| {
            // Center the two pads
            let pads_w = xy_size * 2.0 + 24.0; // two pads + spacing
            let pad_spacer = ((ui.available_width() - pads_w) / 2.0).max(0.0);
            ui.add_space(pad_spacer);
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
                        || param_mode("bass.sub_osc_level", &locked, &focused)
                            == ParamMode::UserOwned
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
                let av = snap
                    .active_voice
                    .min(snap.bass_voices.len().saturating_sub(1));
                match p1 {
                    1 => {
                        snap.bass_voices[av].synth.accent_level = vx1;
                        snap.bass_voices[av].synth.volume = vy1;
                    }
                    2 => {
                        snap.bass_voices[av].synth.distortion = vx1;
                        snap.bass_voices[av].synth.sub_osc_level = vy1;
                    }
                    _ => {
                        snap.bass_voices[av].synth.cutoff = vx1;
                        snap.bass_voices[av].synth.resonance = vy1;
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
                let av = snap
                    .active_voice
                    .min(snap.bass_voices.len().saturating_sub(1));
                match p2 {
                    1 => {
                        snap.bass_voices[av].synth.fm_depth = vx2;
                        snap.bass_voices[av].synth.fm_ratio = vy2;
                    }
                    2 => {
                        snap.bass_voices[av].synth.noise_mix = vx2;
                        snap.bass_voices[av].synth.portamento_time = vy2;
                    }
                    _ => {
                        snap.bass_voices[av].synth.env_mod = vx2;
                        snap.bass_voices[av].synth.decay = vy2;
                    }
                }
                *app.state.write() = snap;
                app.push_audio_params();
            }
        });
    }); // close horizontal + vertical_centered for XY pads

    ui.add_space(2.0);

    // Decay envelope visualiser — shows the 303's decay-only filter envelope shape.
    // Width spans both pads (2×xy_size + spacing); height scales with pad size.
    let env_w = (xy_size * 2.0 + 14.0).max(200.0);
    let env_h = app.state.read().ui_prefs.effective_env_h();
    ui.vertical_centered(|ui| {
        ui.horizontal(|ui| {
            // Center the env/filter displays
            let disp_w = env_w + 60.0; // display + label
            let disp_spacer = ((ui.available_width() - disp_w) / 2.0).max(0.0);
            ui.add_space(disp_spacer);
            ui.label(
                egui::RichText::new("ENV")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            if widgets::decay_display(ui, &mut decay, env_mod, env_w, env_h) {
                let mut snap = app.state.read().clone();
                let av = snap
                    .active_voice
                    .min(snap.bass_voices.len().saturating_sub(1));
                snap.bass_voices[av].synth.decay = decay;
                *app.state.write() = snap;
                app.push_audio_params();
            }
        });
        ui.add_space(10.0);
        // Filter response curve
        ui.horizontal(|ui| {
            let disp_spacer2 = ((ui.available_width() - env_w - 60.0) / 2.0).max(0.0);
            ui.add_space(disp_spacer2);
            ui.label(
                egui::RichText::new("FLT")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            let fm = match filter_mode {
                FilterMode::Highpass => 1u8,
                FilterMode::Bandpass => 2,
                _ => 0,
            };
            widgets::filter_response(ui, cutoff, resonance, fm, env_w, env_h);
        });
    }); // end vertical_centered

    ui.add_space(12.0);

    // WAVE / FILT / PRESET rows left, waveform display right
    let label_w = 50.0; // fixed label width for alignment
    let wave_kind = match waveform {
        Waveform::Saw => 0,
        Waveform::Square => 1,
        Waveform::Supersaw => 2,
    };
    let viz_w = (ui.available_width() - label_w - 160.0).max(80.0);
    let viz_h = env_h.max(60.0);
    let viz_total_h = viz_h; // waveform display spans all 3 rows

    ui.horizontal(|ui| {
        // Left column: WAVE + FILT + PRESET stacked
        ui.vertical(|ui| {
            // WAVE row
            ui.horizontal(|ui| {
                ui.add_sized(
                    [label_w, 18.0],
                    egui::Label::new(
                        egui::RichText::new("WAVE")
                            .color(theme::SMOKE)
                            .monospace()
                            .size(9.0),
                    ),
                );
                let mut saw = waveform == Waveform::Saw;
                if widgets::toggle_button(ui, "SAW", &mut saw) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.waveform = Waveform::Saw;
                    app.push_audio_params();
                }
                let mut sq = waveform == Waveform::Square;
                if widgets::toggle_button(ui, "SQR", &mut sq) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.waveform = Waveform::Square;
                    app.push_audio_params();
                }
                let mut ss = waveform == Waveform::Supersaw;
                if widgets::toggle_button(ui, "SUPER", &mut ss) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.waveform = Waveform::Supersaw;
                    app.push_audio_params();
                }
            });
            // FILT row
            ui.horizontal(|ui| {
                ui.add_sized(
                    [label_w, 18.0],
                    egui::Label::new(
                        egui::RichText::new("FILT")
                            .color(theme::SMOKE)
                            .monospace()
                            .size(9.0),
                    ),
                );
                let mut lp = filter_mode == FilterMode::Lowpass;
                if widgets::toggle_button(ui, "LP", &mut lp) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.filter_mode = FilterMode::Lowpass;
                    app.push_audio_params();
                }
                let mut hp = filter_mode == FilterMode::Highpass;
                if widgets::toggle_button(ui, "HP", &mut hp) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.filter_mode = FilterMode::Highpass;
                    app.push_audio_params();
                }
                let mut bp = filter_mode == FilterMode::Bandpass;
                if widgets::toggle_button(ui, "BP", &mut bp) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.filter_mode = FilterMode::Bandpass;
                    app.push_audio_params();
                }
            });
            // PRESET row
            ui.horizontal(|ui| {
                ui.add_sized(
                    [label_w, 18.0],
                    egui::Label::new(
                        egui::RichText::new("PRESET")
                            .color(theme::SMOKE)
                            .monospace()
                            .size(9.0),
                    ),
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
        });
        // Right column: waveform display (with left padding)
        ui.add_space(8.0);
        widgets::waveform_icon(ui, wave_kind, viz_w, viz_total_h);
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
                let av = app.state.read().active_voice;
                let mut s = app.state.write();
                s.bass_voices[av].synth.supersaw_detune = supersaw_detune;
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
                let av = app.state.read().active_voice;
                app.state.write().bass_voices[av].synth.supersaw_voices = supersaw_voices - 1;
                app.push_audio_params();
            }
            ui.label(
                egui::RichText::new(&voices_label)
                    .color(theme::CHALK)
                    .monospace()
                    .size(9.0),
            );
            if ui.small_button("+").clicked() && supersaw_voices < 7 {
                let av = app.state.read().active_voice;
                app.state.write().bass_voices[av].synth.supersaw_voices = supersaw_voices + 1;
                app.push_audio_params();
            }
        });
    }

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

    super::bass_noise::draw_noise_section(app, ui);
}
