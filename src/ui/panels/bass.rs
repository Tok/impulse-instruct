// ─── ui/panels/bass.rs ────────────────────────────────────────────────────────
// Bass synthesizer panel.

use crate::state::{ParamMode, cycle_param_mode, param_mode, set_param_mode};
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_bass(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // ── Voice selector + Global Key + Pan — single row ─────────────────────
    {
        let (active, _, voice_enabled) = {
            let s = app.state.read();
            let enabled: Vec<bool> = s.bass_voices.iter().map(|v| v.enabled).collect();
            (s.active_voice, s.bass_voices.len(), enabled)
        };
        let (lock_key, voice_root_note, voice_scale) = {
            let s = app.state.read();
            let av = s.active_voice.min(s.bass_voices.len().saturating_sub(1));
            let v = &s.bass_voices[av];
            (v.lock_key, v.root_note, v.scale)
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
            ui.separator();
            // GLOBAL KEY checkbox
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
            // PAN slider — right-justified
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut pan = app.state.read().bass_voices
                    [active.min(app.state.read().bass_voices.len().saturating_sub(1))]
                .synth
                .pan;
                if widgets::pan_slider(ui, &mut pan, super::PAN_SLIDER_W) {
                    let av = active.min(app.state.read().bass_voices.len().saturating_sub(1));
                    app.state.write().bass_voices[av].synth.pan = pan;
                    app.push_audio_params();
                }
                ui.label(
                    egui::RichText::new("PAN")
                        .monospace()
                        .size(8.0)
                        .color(theme::SMOKE),
                );
            });
        });
        // Note buttons when GLOBAL KEY is unchecked (own row)
        if !lock_key {
            ui.horizontal(|ui| {
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
                let _ = voice_scale;
            });
        }
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
        filter_mode,
        mut lfo_target,
        mut lfo_rate,
        mut lfo_depth,
        mut lfo_phase,
        mut lfo_waveform,
        mut lfo_bpm_sync,
        mut lfo_sync_beats,
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
            b.filter_mode,
            b.lfo_target,
            b.lfo_rate,
            b.lfo_depth,
            b.lfo_phase,
            b.lfo_waveform,
            b.lfo_bpm_sync,
            b.lfo_sync_beats,
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
    {
        // ── Knob layout: FILTER + CHARACTER wide, MOD compact ────────────────
        let ctrl_big = ctrl.phi_bigger(); // primary: cutoff, resonance
        let ctrl_sm = ctrl.phi_smaller(); // secondary: glide, noise, FM
        let avail = ui.available_width();
        let gap = super::GLASS_GAP;
        // FILTER and CHARACTER get 40% each, MOD gets 20%
        let gw_main = ((avail - gap * 2.0) * 0.40).floor();
        let gw_mod = avail - gw_main * 2.0 - gap * 2.0;
        // Fixed height so all three glass groups match
        // Height based on the largest knobs (FILTER uses ctrl_big)
        let group_h = widgets::glass_group_height(ctrl_big, 50.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = super::GLASS_GAP;
            // FILTER group: 2×2 grid (CUT/RES, ENV/DEC)
            widgets::glass_group_fill(ui, gw_main, gw_main, |ui| {
                ui.set_min_height(group_h);
                ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
                ui.label(
                    egui::RichText::new("FILTER")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "CUTOFF",
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
                        "RESONANCE",
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
                        "ENV. MOD",
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
                        "DECAY",
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
                ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
                ui.label(
                    egui::RichText::new("CHARACTER")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "ACCENT",
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
                        "DRIVE",
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
                        "VOLUME",
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
                ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
                ui.label(
                    egui::RichText::new("MOD")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                widgets::centered_row(ui, |ui| {
                    let (ch, cy) = widgets::param_control(
                        ui,
                        "GLIDE",
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
                        "NOISE",
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
                    if widgets::param_control(
                        ui,
                        "FM.DEPTH",
                        &mut fm_depth,
                        ParamMode::Free,
                        ctrl_sm,
                    )
                    .0
                    {
                        changed = true;
                    }
                    if widgets::param_control(
                        ui,
                        "FM.RATIO",
                        &mut fm_ratio,
                        ParamMode::Free,
                        ctrl_sm,
                    )
                    .0
                    {
                        changed = true;
                    }
                });
                if widgets::param_control_bipolar(
                    ui,
                    "DETUNE",
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

        // ── LFO row — per-voice SH-101-style modulator ──────────────────
        // Compact glass strip below the FILTER/CHARACTER/MOD groups so the
        // user can manually tune what the LLM can already write via
        // bass.lfo_target / lfo_rate / lfo_depth / lfo_waveform /
        // lfo_bpm_sync / lfo_sync_beats.
        ui.add_space(4.0);
        widgets::glass_group_fill(ui, ui.available_width(), ui.available_width(), |ui| {
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("LFO")
                        .color(theme::FOG)
                        .monospace()
                        .size(9.5),
                );
                // TARGET cycle button
                let target_label = lfo_target.label();
                if ui
                    .small_button(
                        egui::RichText::new(format!("→ {}", target_label))
                            .monospace()
                            .size(8.0),
                    )
                    .on_hover_text("LFO target — click to cycle (Off → Pitch → PWM → Cutoff → Amp)")
                    .clicked()
                {
                    lfo_target = lfo_target.next();
                    changed = true;
                }
                // WAVEFORM cycle button
                let wave_label = match lfo_waveform {
                    crate::state::LfoWaveform::Sine => "SIN",
                    crate::state::LfoWaveform::Triangle => "TRI",
                    crate::state::LfoWaveform::Saw => "SAW",
                    crate::state::LfoWaveform::InvSaw => "↓SW",
                    crate::state::LfoWaveform::Square => "SQR",
                    crate::state::LfoWaveform::SampleAndHold => "S&H",
                };
                if ui
                    .small_button(egui::RichText::new(wave_label).monospace().size(8.0))
                    .on_hover_text("LFO waveform — click to cycle")
                    .clicked()
                {
                    lfo_waveform = match lfo_waveform {
                        crate::state::LfoWaveform::Sine => crate::state::LfoWaveform::Triangle,
                        crate::state::LfoWaveform::Triangle => crate::state::LfoWaveform::Saw,
                        crate::state::LfoWaveform::Saw => crate::state::LfoWaveform::InvSaw,
                        crate::state::LfoWaveform::InvSaw => crate::state::LfoWaveform::Square,
                        crate::state::LfoWaveform::Square => {
                            crate::state::LfoWaveform::SampleAndHold
                        }
                        crate::state::LfoWaveform::SampleAndHold => crate::state::LfoWaveform::Sine,
                    };
                    changed = true;
                }
                // SYNC toggle
                let sync_text = if lfo_bpm_sync { "SYNC●" } else { "SYNC○" };
                if ui
                    .small_button(egui::RichText::new(sync_text).monospace().size(8.0))
                    .on_hover_text("BPM-sync vs free-run rate")
                    .clicked()
                {
                    lfo_bpm_sync = !lfo_bpm_sync;
                    changed = true;
                }
                // RATE / BEATS — show whichever is active
                if lfo_bpm_sync {
                    if widgets::param_control(
                        ui,
                        "BEATS",
                        &mut lfo_sync_beats,
                        ParamMode::Free,
                        ctrl_sm,
                    )
                    .0
                    {
                        changed = true;
                    }
                } else if widgets::param_control(
                    ui,
                    "RATE",
                    &mut lfo_rate,
                    ParamMode::Free,
                    ctrl_sm,
                )
                .0
                {
                    changed = true;
                }
                // DEPTH
                if widgets::param_control(ui, "DEPTH", &mut lfo_depth, ParamMode::Free, ctrl_sm).0 {
                    changed = true;
                }
                // PHASE — per-voice offset (0..1).  Set voice 0 to 0
                // and voice 1 to 0.5 for an anti-phase pan sweep when
                // both are targeting Pan.
                if widgets::param_control(ui, "PHASE", &mut lfo_phase, ParamMode::Free, ctrl_sm).0 {
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
            snap.bass_voices[av].synth.lfo_target = lfo_target;
            snap.bass_voices[av].synth.lfo_rate = lfo_rate;
            snap.bass_voices[av].synth.lfo_depth = lfo_depth;
            snap.bass_voices[av].synth.lfo_phase = lfo_phase;
            snap.bass_voices[av].synth.lfo_waveform = lfo_waveform;
            snap.bass_voices[av].synth.lfo_bpm_sync = lfo_bpm_sync;
            snap.bass_voices[av].synth.lfo_sync_beats = lfo_sync_beats;
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

    // PAN slider is now in the voice selector row above.

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
            // Pad 1 — cycles: CUTOFF×RESO | ACCENT×VOLUME | DRIVE×SUB
            let p1 = widgets::xy_pad_pair(ui.ctx(), "bass_xy1");
            let (lx1, ly1, mut vx1, mut vy1) = match p1 {
                1 => ("ACCENT", "VOLUME", accent, vol),
                2 => ("DRIVE", "SUB", dist, sub_osc_level),
                _ => ("CUTOFF", "RESONANCE", cutoff, resonance),
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

            // Pad 2 — cycles: ENVMOD×DECAY | FM.DEPTH×FM.RATIO | NOISE×GLIDE
            let p2 = widgets::xy_pad_pair(ui.ctx(), "bass_xy2");
            let (lx2, ly2, mut vx2, mut vy2) = match p2 {
                1 => ("FM.DEPTH", "FM.RATIO", fm_depth, fm_ratio),
                2 => ("NOISE", "GLIDE", noise_mix, portamento_time),
                _ => ("ENV. MOD", "DECAY", env_mod, decay),
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
            widgets::filter_response(ui, cutoff, resonance, filter_mode, env_w, env_h);
        });
    }); // end vertical_centered

    ui.add_space(12.0);
    super::bass_wave::draw_wave_preset_section(app, ui);

    ui.add_space(4.0);
    super::bass_locks::draw_locked_params(app, ui);

    super::bass_noise::draw_noise_section(app, ui);
}
