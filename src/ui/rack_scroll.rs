// ─── ui/rack_scroll.rs ── API-driven scroll + focus highlight for the rack ───
// Extracted from rack_canvas.rs to stay under the 1000-line limit.

use egui::scroll_area::ScrollAreaOutput;

use crate::state::ModuleKind;
use crate::ui::ImpulseApp;

/// Resolve a scroll target string to a focus-highlight module kind.
pub(crate) fn resolve_focus_kind(target: &str) -> Option<ModuleKind> {
    match target.to_ascii_lowercase().as_str() {
        "sequencer" => Some(ModuleKind::StepSequencer),
        "bass" | "acidbass" | "acid_bass" => Some(ModuleKind::AcidBass),
        "808" | "drumkit808" | "drums" => Some(ModuleKind::DrumKit808),
        "909" | "drumkit909" => Some(ModuleKind::DrumKit909),
        "hoover" => Some(ModuleKind::HooverLead),
        "an1x" => Some(ModuleKind::An1xVoice),
        "reverb" => Some(ModuleKind::FxReverb),
        "delay" => Some(ModuleKind::FxDelay),
        "chorus" => Some(ModuleKind::FxChorus),
        "lfo" => Some(ModuleKind::LfoModule),
        "console" | "llm" => Some(ModuleKind::LlmConsole),
        "agent" | "llm_agent" | "agents" => Some(ModuleKind::LlmAgent),
        "spectrum" => Some(ModuleKind::SpectrumAnalyzer),
        "master" => Some(ModuleKind::MasterOutput),
        _ => None,
    }
}

/// Handle keyboard scroll (arrows / WASD) and API scroll targets.
/// Called from `draw_rack` after the ScrollArea has rendered.
pub(crate) fn handle_scroll(
    app: &mut ImpulseApp,
    ctx: &egui::Context,
    scroll_target: &Option<String>,
    scroll_out: &ScrollAreaOutput<()>,
) {
    let scroll_state_id = scroll_out.id;
    let max_y = (scroll_out.content_size.y - scroll_out.inner_rect.height()).max(0.0);

    // ── Mouse wheel boost (3× default speed) ─────────────────────────────────
    {
        let wheel_y = ctx.input(|i| i.raw_scroll_delta.y);
        if wheel_y.abs() > 0.1
            && scroll_out
                .inner_rect
                .contains(ctx.pointer_latest_pos().unwrap_or_default())
        {
            let boost = wheel_y * 2.0; // 2× extra on top of egui's 1× = 3× total
            let mut state = scroll_out.state;
            state.offset.y = (state.offset.y - boost).clamp(0.0, max_y);
            state.store(ctx, scroll_state_id);
        }
    }

    // ── Keyboard scroll (arrows / WASD) ─────────────────────────────────────
    {
        let scroll_speed = 180.0;
        let wasd = ctx
            .data(|d| d.get_temp::<bool>(egui::Id::new("wasd_as_arrows")))
            .unwrap_or(false);
        let up =
            ctx.input(|i| i.key_down(egui::Key::ArrowUp) || (wasd && i.key_down(egui::Key::W)));
        let down =
            ctx.input(|i| i.key_down(egui::Key::ArrowDown) || (wasd && i.key_down(egui::Key::S)));
        if up || down {
            let mut state = scroll_out.state;
            if down {
                state.offset.y = (state.offset.y + scroll_speed).min(max_y);
            }
            if up {
                state.offset.y = (state.offset.y - scroll_speed).max(0.0);
            }
            state.store(ctx, scroll_state_id);
            ctx.request_repaint();
        }
    }

    // ── API-driven scroll ───────────────────────────────────────────────────
    // Supports zone names ("global", "voice", "fxmod") AND specific module names
    // ("sequencer", "bass", "808", "reverb", etc.) for precise scrolling.
    if let Some(t) = scroll_target {
        // First try module-level scroll using card_rects from this frame.
        let module_kind = resolve_focus_kind(t);

        // Read card_rects from this frame (stored by draw_rack_inner).
        let card_rects: Vec<(ModuleKind, egui::Rect)> = ctx
            .memory(|m| m.data.get_temp(egui::Id::new("module_card_rects")))
            .unwrap_or_default();

        // Try module-level first, fall back to zone-level.
        let target_y: Option<f32> = module_kind
            .and_then(|kind| {
                card_rects
                    .iter()
                    .find(|(k, _)| *k == kind)
                    .map(|(_, rect)| {
                        // Convert screen rect to scroll content offset.
                        // inner_rect.top() = screen position of scroll viewport top.
                        // rect.top() = screen position of the module card.
                        // The difference + current offset = absolute content Y.
                        let screen_offset = rect.top() - scroll_out.inner_rect.top();
                        scroll_out.state.offset.y + screen_offset
                    })
            })
            .or_else(|| match t.to_ascii_lowercase().as_str() {
                "global" => {
                    app.zone_global_collapsed = false;
                    Some(app.zone_y[0])
                }
                "voice" | "voices" | "drums" => {
                    app.zone_voice_collapsed = false;
                    Some(app.zone_y[1])
                }
                "fxmod" | "fx" | "effects" | "modulation" => {
                    app.zone_fxmod_collapsed = false;
                    Some(app.zone_y[2])
                }
                _ => None,
            });

        if let Some(y) = target_y {
            let mut state = scroll_out.state;
            let old_y = state.offset.y;
            state.offset.y = y.max(0.0).min(max_y);
            state.store(ctx, scroll_state_id);
            ctx.request_repaint();
            log::info!(
                "API scroll {:?}: offset {:.0} → {:.0}  (max={:.0})",
                t,
                old_y,
                state.offset.y,
                max_y
            );
        } else if module_kind.is_some() {
            // Module not in card_rects yet (just added this frame) — retry next frame
            app.state.write().scroll_target = Some(t.clone());
            ctx.request_repaint();
        }
    }
}

/// Publish the focused module + elapsed time to egui temp data for module_card rendering.
pub(crate) fn publish_focus(app: &mut ImpulseApp, ctx: &egui::Context) {
    let elapsed = app.focus_time.elapsed().as_secs_f32();
    // Auto-clear focus after the animation fades (3 seconds)
    if elapsed > 3.0 {
        app.focused_module = None;
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::new("focused_module"),
            app.focused_module.map(|k| (k, elapsed)),
        );
    });
    // Request repaints during the animation
    if app.focused_module.is_some() {
        ctx.request_repaint();
    }
}
