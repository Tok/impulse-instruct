// ─── ui/rack_canvas_menus.rs ─────────────────────────────────────────────────
// Modal pop-ups layered over the rack canvas:
//   • `draw_remove_confirm` — "Remove this module?" confirm dialog.
//   • `draw_add_menu`       — Zone-scoped "Add module" picker.
//
// Plus the static module-kind lookup tables each zone's Add menu
// offers.  Lifted out of `rack_canvas.rs` so that file can stay
// focused on the scroll / grid / cables overlay work.

use egui::Color32;

use crate::state::ModuleKind;
use crate::ui::ImpulseApp;

pub(super) const VOICE_KINDS: &[ModuleKind] = &[
    ModuleKind::AcidBass,
    ModuleKind::DrumKit808,
    ModuleKind::DrumKit909,
    ModuleKind::GabberKick,
    ModuleKind::HooverLead,
    ModuleKind::An1xVoice,
    ModuleKind::AmenSampler,
    ModuleKind::NoiseVoice,
];
pub(super) const AI_KINDS: &[ModuleKind] = &[ModuleKind::LlmConsole, ModuleKind::LlmAgent];
pub(super) const GLOBAL_KINDS: &[ModuleKind] = &[];
pub(super) const FXMOD_KINDS: &[ModuleKind] = &[
    ModuleKind::FxReverb,
    ModuleKind::FxDelay,
    ModuleKind::FxChorus,
    ModuleKind::FxPhaser,
    ModuleKind::FxEq,
    ModuleKind::FxCompressor,
    ModuleKind::FxTapeSat,
    ModuleKind::FxDrive,
    ModuleKind::FxAutotune,
    ModuleKind::FxWaveshaper,
    ModuleKind::FxBitcrush,
    ModuleKind::FxRingMod,
    ModuleKind::SpectrumAnalyzer,
    ModuleKind::StereoMeter,
    ModuleKind::ActivityTimeline,
    ModuleKind::LfoModule,
];

pub(super) fn draw_remove_confirm(app: &mut ImpulseApp, ctx: &egui::Context) {
    let module_id = match app.confirm_remove_module {
        Some(id) => id,
        None => return,
    };
    let label = app
        .state
        .read()
        .rack
        .modules
        .iter()
        .find(|m| m.id == module_id)
        .map(|m| m.kind.label().to_string())
        .unwrap_or_else(|| format!("Module #{}", module_id));
    let mut open = true;
    egui::Window::new("confirm_remove")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Remove {}?", label))
                    .monospace()
                    .size(9.5)
                    .color(Color32::from_gray(200)),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Remove").clicked() {
                    let is_agent = app
                        .state
                        .read()
                        .rack
                        .modules
                        .iter()
                        .any(|m| m.id == module_id && m.kind == ModuleKind::LlmAgent);
                    app.state.write().rack.remove_module(module_id);
                    if is_agent {
                        app.state.write().llm_agents.retain(|a| a.id != module_id);
                    }
                    app.push_fx_plan();
                    app.confirm_remove_module = None;
                }
                if ui.button("Cancel").clicked() {
                    app.confirm_remove_module = None;
                }
            });
        });
    if !open {
        app.confirm_remove_module = None;
    }
}

pub(super) fn draw_add_menu(app: &mut ImpulseApp, ctx: &egui::Context) {
    let zone = match app.add_menu_zone {
        Some(z) => z,
        None => return,
    };
    let kinds: &[ModuleKind] = match zone {
        crate::state::Zone::Ai => AI_KINDS,
        crate::state::Zone::Voice => VOICE_KINDS,
        crate::state::Zone::FxMod => FXMOD_KINDS,
        crate::state::Zone::Global => GLOBAL_KINDS,
    };

    let mut open = true;
    egui::Window::new("add_module_popup")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(160.0);
            ui.label(
                egui::RichText::new("ADD MODULE")
                    .monospace()
                    .size(8.5)
                    .color(Color32::from_gray(100)),
            );
            ui.separator();
            let mut close = false;
            for kind in kinds {
                let already_exists = !kind.allows_multiple()
                    && app
                        .state
                        .read()
                        .rack
                        .modules
                        .iter()
                        .any(|m| m.kind == *kind);
                if already_exists {
                    ui.add_enabled(
                        false,
                        egui::Button::new(egui::RichText::new(kind.label()).monospace().size(9.5)),
                    );
                } else if ui
                    .button(egui::RichText::new(kind.label()).monospace().size(9.5))
                    .clicked()
                {
                    let id = app.state.write().rack.add_module(*kind);
                    // Place the new module on the grid
                    app.state.write().rack.arrange_grid();
                    if *kind == ModuleKind::LlmAgent {
                        let agent =
                            crate::state::LlmAgentState::from_singleton(id, &app.state.read().llm);
                        app.state.write().llm_agents.push(agent);
                    }
                    close = true;
                }
            }
            if close
                || ui
                    .button(egui::RichText::new("cancel").monospace().size(8.5))
                    .clicked()
            {
                app.add_menu_zone = None;
            }
        });
    if !open {
        app.add_menu_zone = None;
    }
}
