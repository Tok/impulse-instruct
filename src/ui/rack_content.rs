// ─── ui/rack_content.rs ──────────────────────────────────────────────────────
// Content draw functions for each module kind, split from rack_canvas.rs to
// keep file sizes under the 1000-line limit.

use crate::state::ModuleKind;
use crate::ui::{ImpulseApp, module_card};

pub(super) fn draw_voice_content(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    kind: ModuleKind,
    module_id: u32,
) {
    match kind {
        ModuleKind::AcidBass => crate::ui::panels::draw_bass(app, ui),
        ModuleKind::DrumKit808 => crate::ui::panels::draw_kit_a(app, ui),
        ModuleKind::DrumKit909 => crate::ui::panels::draw_kit_b(app, ui),
        ModuleKind::HooverLead => crate::ui::panels::draw_hoover(app, ui),
        ModuleKind::An1xVoice => crate::ui::panels::draw_an1x(app, ui),
        ModuleKind::AmenSampler => crate::ui::panels::draw_amen(app, ui),
        ModuleKind::NoiseVoice => crate::ui::panels::draw_noise(app, ui),
        ModuleKind::GranularTexture => crate::ui::panels::draw_granular(app, ui),
        ModuleKind::NeuTts => crate::ui::panels::draw_tts(app, ui, module_id),
        _ => {}
    }
}

pub(super) fn draw_fx_content(app: &mut ImpulseApp, ui: &mut egui::Ui, kind: ModuleKind) {
    use crate::ui::widgets;

    let scale: f32 = ui
        .ctx()
        .data(|d| d.get_temp(egui::Id::new("module_scale")))
        .unwrap_or(1.0);
    let ctrl = widgets::ControlPrefs::from_prefs_scaled(&app.state.read().ui_prefs, scale);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);
    let mut changed = false;
    ui.spacing_mut().item_spacing.x = crate::ui::panels::KNOB_SPACING;

    // Helper: horizontal row of knobs
    macro_rules! hk {
        ($ui:expr, $( ($label:expr, $val:expr, $pm:expr) ),+ $(,)?) => {
            widgets::centered_row($ui, |ui| {
                $( if widgets::param_control(ui, $label, $val, $pm, ctrl).0 { changed = true; } )+
            });
        }
    }

    match kind {
        ModuleKind::FxReverb => {
            let (mut rs, mut rd, mut rm) = {
                let s = app.state.read();
                (s.fx.reverb_size, s.fx.reverb_damp, s.fx.reverb_mix)
            };
            hk!(
                ui,
                ("SIZE", &mut rs, pm("fx.reverb_size")),
                ("DAMPING", &mut rd, pm("fx.reverb_damp")),
                ("MIX", &mut rm, pm("fx.reverb_mix"))
            );
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
            hk!(
                ui,
                ("TIME", &mut dt, pm("fx.delay_time")),
                ("FEEDBACK", &mut df, pm("fx.delay_feedback")),
                ("MIX", &mut dm, pm("fx.delay_mix"))
            );
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
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.chorus_rate")),
                ("DEPTH", &mut d, pm("fx.chorus_depth")),
                ("MIX", &mut m, pm("fx.chorus_mix"))
            );
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
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.phaser_rate")),
                ("DEPTH", &mut d, pm("fx.phaser_depth")),
                ("MIX", &mut m, pm("fx.phaser_mix"))
            );
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
            hk!(
                ui,
                ("LOW", &mut lo, pm("fx.eq_low_gain")),
                ("MID", &mut mi, pm("fx.eq_mid_gain")),
                ("HIGH", &mut hi, pm("fx.eq_hi_gain"))
            );
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
            hk!(
                ui,
                ("THRESH", &mut th, pm("fx.compressor_threshold")),
                ("RATIO", &mut ra, pm("fx.compressor_ratio")),
                ("MIX", &mut mi, pm("fx.compressor_mix"))
            );
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
            hk!(
                ui,
                ("DRIVE", &mut dr, pm("fx.tape_drive")),
                ("FLUTTER", &mut fl, pm("fx.tape_flutter")),
                ("MIX", &mut mi, pm("fx.tape_mix"))
            );
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
            hk!(
                ui,
                ("DRIVE", &mut dr, pm("fx.distortion_drive")),
                ("MIX", &mut mi, pm("fx.distortion_mix"))
            );
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
            hk!(
                ui,
                ("AMOUNT", &mut amt, pm("fx.autotune_amount")),
                ("MIX", &mut mi, pm("fx.autotune_mix"))
            );
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
            hk!(
                ui,
                ("DRIVE", &mut dr, pm("fx.waveshaper_drive")),
                ("MIX", &mut mi, pm("fx.waveshaper_mix"))
            );
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
            hk!(
                ui,
                ("BITS", &mut bi, pm("fx.bitcrush_bits")),
                ("RATE", &mut ra, pm("fx.bitcrush_rate")),
                ("MIX", &mut mi, pm("fx.bitcrush_mix"))
            );
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
            hk!(
                ui,
                ("FREQ", &mut fr, pm("fx.ring_mod_freq")),
                ("MIX", &mut mi, pm("fx.ring_mod_mix"))
            );
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
        // Observe key FX params for style tracking
        let fx = &app.state.read().fx.clone();
        app.observe_edits(&[
            ("fx.reverb_mix", fx.reverb_mix),
            ("fx.delay_mix", fx.delay_mix),
            ("fx.delay_feedback", fx.delay_feedback),
            ("fx.chorus_mix", fx.chorus_mix),
            ("fx.compressor_threshold", fx.compressor_threshold),
            ("fx.distortion_drive", fx.distortion_drive),
            ("fx.master_volume", fx.master_volume),
            ("fx.stereo_width", fx.stereo_width),
        ]);
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

// ─── LLM agent card content ──────────────────────────────────────────────────

pub(super) fn draw_llm_agent_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    // Width is constrained by the grid; height auto-sizes to content.
    draw_llm_agent_inner(app, ui, module_id);
}

fn draw_llm_agent_inner(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    use crate::ui::theme;

    let agent_idx = {
        let s = app.state.read();
        s.llm_agents.iter().position(|a| a.id == module_id)
    };
    let Some(idx) = agent_idx else { return };

    // Snapshot mutable fields for editing
    let (
        mut persona,
        mut temp,
        mut jam_bars,
        last_resp,
        tps,
        cycles,
        inferring,
        agent_model,
        conv_mode,
        active_style,
        enable_thinking,
        mut user_instructions,
        mut prompt_override,
        role,
        can_spawn,
        can_dismiss,
    ) = {
        let s = app.state.read();
        let a = &s.llm_agents[idx];
        (
            a.persona_name.clone(),
            a.temperature,
            a.jam_bars,
            a.last_response.clone(),
            a.tokens_per_sec,
            a.jam_cycle_count,
            a.is_inferring,
            a.model_path.clone(),
            a.conversation_mode.clone(),
            a.active_style.clone(),
            a.enable_thinking,
            a.user_instructions.clone(),
            a.system_prompt_override.clone(),
            a.role,
            a.can_spawn,
            a.can_dismiss,
        )
    };

    // ── Persona + model + status (single line) ────────────────────────────
    ui.horizontal(|ui| {
        if inferring {
            let t = ui.ctx().input(|i| i.time) as f32;
            let pulse = (t * 4.0 * std::f32::consts::TAU).sin() * 0.3 + 0.7;
            let g = (220.0 * pulse) as u8;
            ui.label(
                egui::RichText::new("●")
                    .color(egui::Color32::from_gray(g))
                    .size(10.0),
            );
            ui.ctx().request_repaint();
        } else {
            ui.label(egui::RichText::new("●").color(theme::ASH).size(10.0));
        }
        let resp = ui.add(
            egui::TextEdit::singleline(&mut persona)
                .desired_width(80.0)
                .font(egui::FontId::monospace(9.5))
                .text_color(theme::FOG),
        );
        if resp.changed() {
            app.state.write().llm_agents[idx].persona_name = persona;
        }
        // Model dropdown (inline)
        {
            if app.available_models.is_empty() {
                app.available_models = super::scan_models();
            }
            let display_name = match &agent_model {
                Some(p) => std::path::Path::new(p)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(p)
                    .to_string(),
                None => "(Def)".to_string(),
            };
            let combo_id = ui.id().with("agent_model").with(module_id);
            egui::ComboBox::from_id_source(combo_id)
                .selected_text(
                    egui::RichText::new(&display_name)
                        .color(theme::SMOKE)
                        .size(7.5)
                        .monospace(),
                )
                .width(ui.available_width().min(100.0))
                .show_ui(ui, |ui| {
                    let is_default = agent_model.is_none();
                    if ui
                        .selectable_label(
                            is_default,
                            egui::RichText::new("(Default)")
                                .monospace()
                                .size(8.0)
                                .color(if is_default {
                                    theme::CHALK
                                } else {
                                    theme::SMOKE
                                }),
                        )
                        .clicked()
                    {
                        app.state.write().llm_agents[idx].model_path = None;
                    }
                    for path in &app.available_models.clone() {
                        let short = std::path::Path::new(path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(path)
                            .to_string();
                        let selected = agent_model.as_deref() == Some(path.as_str());
                        if ui
                            .selectable_label(
                                selected,
                                egui::RichText::new(&short)
                                    .monospace()
                                    .size(8.0)
                                    .color(if selected { theme::CHALK } else { theme::SMOKE }),
                            )
                            .clicked()
                        {
                            app.state.write().llm_agents[idx].model_path = Some(path.clone());
                        }
                    }
                });
        }
        // Status: tok/s or cycle count
        if inferring {
            ui.label(
                egui::RichText::new(format!("{:.0}t/s", tps))
                    .color(theme::FOG)
                    .monospace()
                    .size(7.5),
            );
        } else if cycles > 0 {
            ui.label(
                egui::RichText::new(format!("#{}", cycles))
                    .color(theme::IRON)
                    .monospace()
                    .size(7.5),
            );
        }
    });

    // ── Temp / Bars controls ───────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(
            egui::RichText::new("T")
                .color(theme::ASH)
                .monospace()
                .size(8.0),
        );
        if ui
            .add(
                egui::DragValue::new(&mut temp)
                    .range(0.0..=2.0)
                    .speed(0.01)
                    .fixed_decimals(2),
            )
            .changed()
        {
            app.state.write().llm_agents[idx].temperature = temp;
        }
        ui.label(
            egui::RichText::new("B")
                .color(theme::ASH)
                .monospace()
                .size(8.0),
        );
        if ui
            .add(
                egui::DragValue::new(&mut jam_bars)
                    .range(0.0..=16.0)
                    .speed(0.5)
                    .fixed_decimals(0),
            )
            .on_hover_text("Bars between jam cycles (0 = continuous)")
            .changed()
        {
            app.state.write().llm_agents[idx].jam_bars = jam_bars;
        }
    });

    // ── Scope (derived from control cables) ────────────────────────────
    {
        let cable_scope =
            crate::state::scope_from_control_cables(&app.state.read().rack, module_id);
        let label = if cable_scope.is_empty() {
            "SCOPE: ALL".to_string()
        } else {
            format!("SCOPE: {}", cable_scope.join(" "))
        };
        ui.label(
            egui::RichText::new(label)
                .monospace()
                .size(7.5)
                .color(theme::IRON),
        );
    }
    // ── Conversation mode + thinking ───────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        use crate::state::ConversationMode;
        for (label, mode) in &[
            ("OFF", ConversationMode::Off),
            ("PRD", ConversationMode::Producer),
            ("DJ", ConversationMode::Dj),
            ("MC", ConversationMode::Mc),
        ] {
            let active = conv_mode == *mode;
            let col = if active { theme::FOG } else { theme::SMOKE };
            let fill = if active {
                egui::Color32::from_gray(40)
            } else {
                egui::Color32::from_gray(18)
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(*label).monospace().size(7.5).color(col))
                        .fill(fill)
                        .min_size(egui::vec2(22.0, 14.0)),
                )
                .on_hover_text(match mode {
                    ConversationMode::Off => "No commentary",
                    ConversationMode::Producer => "Technical descriptions",
                    ConversationMode::Dj => "Hype DJ energy",
                    ConversationMode::Mc => "Jungle/rave MC",
                })
                .clicked()
            {
                app.state.write().llm_agents[idx].conversation_mode = mode.clone();
            }
        }
        ui.separator();
        // Thinking toggle
        let think_col = if enable_thinking {
            theme::FOG
        } else {
            theme::SMOKE
        };
        let think_fill = if enable_thinking {
            egui::Color32::from_gray(40)
        } else {
            egui::Color32::from_gray(18)
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("THK")
                        .monospace()
                        .size(7.5)
                        .color(think_col),
                )
                .fill(think_fill)
                .min_size(egui::vec2(22.0, 14.0)),
            )
            .on_hover_text(if enable_thinking {
                "Reasoning ON (/think)"
            } else {
                "Reasoning OFF (/no_think)"
            })
            .clicked()
        {
            app.state.write().llm_agents[idx].enable_thinking = !enable_thinking;
        }
    });
    // ── Role + autonomy permissions ─────────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        use crate::state::AgentRole;
        let role_btn = |ui: &mut egui::Ui, label, r: &AgentRole| -> bool {
            let active = role == *r;
            let col = if active { theme::FOG } else { theme::SMOKE };
            let fill = egui::Color32::from_gray(if active { 40 } else { 18 });
            ui.add(
                egui::Button::new(egui::RichText::new(label).monospace().size(7.5).color(col))
                    .fill(fill)
                    .min_size(egui::vec2(22.0, 14.0)),
            )
            .clicked()
        };
        if role_btn(ui, "PRD", &AgentRole::Producer) {
            app.state.write().llm_agents[idx].role = AgentRole::Producer;
        }
        if role_btn(ui, "MC", &AgentRole::Mc) {
            app.state.write().llm_agents[idx].role = AgentRole::Mc;
        }
        if role_btn(ui, "DJ", &AgentRole::Dj) {
            app.state.write().llm_agents[idx].role = AgentRole::Dj;
        }
        if role_btn(ui, "SPC", &AgentRole::Specialist) {
            app.state.write().llm_agents[idx].role = AgentRole::Specialist;
        }
        ui.separator();
        let perm_btn = |ui: &mut egui::Ui, label, on: bool, tip_on: &str, tip_off: &str| -> bool {
            let col = if on { theme::FOG } else { theme::IRON };
            let fill = egui::Color32::from_gray(if on { 35 } else { 18 });
            ui.add(
                egui::Button::new(egui::RichText::new(label).monospace().size(7.0).color(col))
                    .fill(fill)
                    .min_size(egui::vec2(24.0, 14.0)),
            )
            .on_hover_text(if on { tip_on } else { tip_off })
            .clicked()
        };
        if perm_btn(
            ui,
            "+AG",
            can_spawn,
            "Can spawn agents",
            "Cannot spawn agents",
        ) {
            app.state.write().llm_agents[idx].can_spawn = !can_spawn;
        }
        if perm_btn(ui, "BYE", can_dismiss, "Can sign off", "Cannot sign off") {
            app.state.write().llm_agents[idx].can_dismiss = !can_dismiss;
        }
    });
    // ── Style selector ──────────────────────────────────────────────────
    {
        use crate::llm::styles::StyleCatalog;
        let cat = StyleCatalog::get();
        let display = match &active_style {
            None => "No style".to_string(),
            Some(s) if s == "__free__" => "Free".to_string(),
            Some(s) if s == "__custom__" => "Custom".to_string(),
            Some(s) => cat
                .find_by_id(s)
                .map(|e| e.name.to_string())
                .unwrap_or_else(|| s.clone()),
        };
        let combo_id = ui.id().with("agent_style").with(module_id);
        egui::ComboBox::from_id_source(combo_id)
            .selected_text(
                egui::RichText::new(&display)
                    .color(theme::SMOKE)
                    .size(7.5)
                    .monospace(),
            )
            .width(ui.available_width().min(140.0))
            .show_ui(ui, |ui| {
                // No style
                if ui
                    .selectable_label(
                        active_style.is_none(),
                        egui::RichText::new("No style")
                            .monospace()
                            .size(8.0)
                            .color(theme::SMOKE),
                    )
                    .clicked()
                {
                    app.state.write().llm_agents[idx].active_style = None;
                }
                // Free
                if ui
                    .selectable_label(
                        active_style.as_deref() == Some("__free__"),
                        egui::RichText::new("Free")
                            .monospace()
                            .size(8.0)
                            .color(theme::SMOKE),
                    )
                    .clicked()
                {
                    app.state.write().llm_agents[idx].active_style = Some("__free__".to_string());
                }
                // Catalog entries
                for entry in cat.styles() {
                    let selected = active_style.as_deref() == Some(entry.id.as_str());
                    if ui
                        .selectable_label(
                            selected,
                            egui::RichText::new(&entry.name)
                                .monospace()
                                .size(8.0)
                                .color(if selected { theme::CHALK } else { theme::SMOKE }),
                        )
                        .clicked()
                    {
                        app.state.write().llm_agents[idx].active_style = Some(entry.id.to_string());
                    }
                }
            });
    }
    // ── User instructions ───────────────────────────────────────────────
    let instr_resp = ui.add(
        egui::TextEdit::multiline(&mut user_instructions)
            .desired_rows(2)
            .desired_width(ui.available_width())
            .font(egui::FontId::monospace(7.5))
            .text_color(theme::FOG)
            .hint_text("Instructions…"),
    );
    if instr_resp.changed() {
        app.state.write().llm_agents[idx].user_instructions = user_instructions;
    }
    // ── System prompt override ──────────────────────────────────────────
    let has_override = !prompt_override.is_empty();
    let ovr_header = if has_override {
        "▸ Prompt override (active)"
    } else {
        "▸ Prompt override"
    };
    let header_col = if has_override {
        theme::FOG
    } else {
        theme::IRON
    };
    let collapse_id = ui.id().with("prompt_ovr").with(module_id);
    egui::CollapsingHeader::new(
        egui::RichText::new(ovr_header)
            .monospace()
            .size(7.5)
            .color(header_col),
    )
    .id_source(collapse_id)
    .default_open(false)
    .show(ui, |ui| {
        let ovr_resp = ui.add(
            egui::TextEdit::multiline(&mut prompt_override)
                .desired_rows(3)
                .desired_width(ui.available_width())
                .font(egui::FontId::monospace(7.5))
                .text_color(theme::FOG)
                .hint_text("Leave empty for auto-generated prompt…"),
        );
        if ovr_resp.changed() {
            app.state.write().llm_agents[idx].system_prompt_override = prompt_override;
        }
    });
    super::rack_ai::draw_last_response_preview(ui, &last_resp);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{:.1} t/s  #{}", tps, cycles))
                .color(theme::ASH)
                .monospace()
                .size(7.5),
        );
    });
}

// ─── Cable drag interaction ───────────────────────────────────────────────────
/// Two port kinds can be patched together iff they're identical OR one side
/// is CV (LFO/seq output) and the other is Mod (per-knob modulation input).
fn port_kinds_compatible(a: crate::state::PortKind, b: crate::state::PortKind) -> bool {
    use crate::state::PortKind::*;
    a == b || matches!((a, b), (Cv, Mod) | (Mod, Cv))
}

pub(super) fn handle_cable_drag(
    app: &mut ImpulseApp,
    ctx: &egui::Context,
    ports: &[module_card::PortPos],
) {
    // Cable patching only in back-panel view (allow in-progress drags to complete)
    if !app.rack_flipped && app.cable_drag.is_none() {
        return;
    }
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
        && port_kinds_compatible(drag.from_port.kind, target.port.kind)
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

// ─── Drag helpers (moved from rack_canvas.rs to stay under 1000-line limit) ──
use crate::ui::rack_cables::ModuleDrag;

/// Process drag start/stop from a card response and update app.module_drag.
/// Returns true if this card was just dropped (so caller can reorder slots).
pub(super) fn handle_title_drag(
    app: &mut ImpulseApp,
    ctx: &egui::Context,
    id: u32,
    resp: &module_card::CardResponse,
    zone: crate::state::Zone,
    zone_origin: egui::Pos2,
    step: f32,
    col_w: f32,
) -> bool {
    if resp.title_dragged {
        if app.module_drag.as_ref().map(|d| d.module_id) != Some(id) {
            let (cw, rh) = app
                .state
                .read()
                .rack
                .modules
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.kind.grid_size(crate::state::GRID_COLS))
                .unwrap_or((1, 1));
            app.module_drag = Some(ModuleDrag {
                module_id: id,
                pointer: ctx.pointer_latest_pos().unwrap_or_default(),
                zone,
                col_span: cw,
                row_span: rh,
                zone_origin,
                step,
                col_w,
            });
        } else if let Some(ref mut drag) = app.module_drag {
            drag.pointer = ctx.pointer_latest_pos().unwrap_or(drag.pointer);
        }
    }
    if resp.title_drag_released && app.module_drag.as_ref().map(|d| d.module_id) == Some(id) {
        app.module_drag = None;
        return true;
    }
    false
}

/// Snap-to-grid drop: compute the target grid cell from the pointer position
/// and move the module there if the cell is free (or swap with occupant).
pub(super) fn reorder_module_by_drop(
    app: &mut ImpulseApp,
    dragged_id: u32,
    drop_pos: egui::Pos2,
    zone: crate::state::Zone,
    zone_origin: egui::Pos2,
    step: f32,
    col_w: f32,
) {
    let (col_span, row_span) = app
        .state
        .read()
        .rack
        .modules
        .iter()
        .find(|m| m.id == dragged_id)
        .map(|m| m.kind.grid_size(crate::state::GRID_COLS))
        .unwrap_or((1, 1));

    // Compute snap target from pointer position relative to zone origin.
    let rel_x = drop_pos.x - zone_origin.x;
    let rel_y = drop_pos.y - zone_origin.y;
    let snap_col = (rel_x / step).round().max(0.0) as u8;
    let snap_row = (rel_y / step).round().max(0.0) as u8;
    let snap_col = snap_col.min(crate::state::GRID_COLS.saturating_sub(col_span));

    // Check current position — no-op if unchanged.
    let current = app
        .state
        .read()
        .rack
        .modules
        .iter()
        .find(|m| m.id == dragged_id)
        .map(|m| (m.grid_col, m.grid_row));
    if current == Some((snap_col, snap_row)) {
        return;
    }

    // Overlap check: reject drop if any other module in the same zone occupies
    // any cell in the target (snap_col..+col_span, snap_row..+row_span) block.
    let blocked = {
        let s = app.state.read();
        s.rack
            .modules
            .iter()
            .filter(|m| m.id != dragged_id && m.zone == zone)
            .any(|m| {
                let (mw, mh) = m.kind.grid_size(crate::state::GRID_COLS);
                // AABB overlap test
                snap_col < m.grid_col + mw
                    && m.grid_col < snap_col + col_span
                    && snap_row < m.grid_row + mh
                    && m.grid_row < snap_row + row_span
            })
    };
    if blocked {
        return; // target occupied — keep original position
    }

    app.push_history();
    if let Some(m) = app
        .state
        .write()
        .rack
        .modules
        .iter_mut()
        .find(|m| m.id == dragged_id)
    {
        m.grid_col = snap_col;
        m.grid_row = snap_row;
    }
    let _ = col_w;
}
