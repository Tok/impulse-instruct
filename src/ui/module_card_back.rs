// ─── ui/module_card_back.rs ──────────────────────────────────────────────────
// Back-panel card rendering — extracted from module_card.rs to keep that file
// under the 1000-line limit.

use egui::{Color32, Frame, Margin, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

use super::module_card::{
    CARD_ROUNDING, CardResponse, PORT_RADIUS, PortPos, TITLE_BAR_H, draw_focus_shine,
    draw_port_circle, focused_title_bg,
};
use super::theme;
use crate::state::{ModuleKind, PortDir, PortKind, PortRef};

// ─── Back-panel card ─────────────────────────────────────────────────────────
// Simplified card for the rack's back panel: title bar + port strip, no content.

/// Draw a back-panel module card showing only ports (inputs left, outputs right)
/// and a faint module name watermark. Used when the rack is flipped.
pub fn module_card_back(
    ui: &mut egui::Ui,
    module_id: u32,
    kind: ModuleKind,
    enabled: bool,
    reaches_master: bool,
    min_width: Option<f32>,
    min_height: Option<f32>,
    _scale: f32,
    ports: &mut Vec<PortPos>,
) -> CardResponse {
    let fill = Color32::from_gray(14);
    let title_bg = focused_title_bg(ui.ctx(), kind);

    if let Some(min_w) = min_width {
        ui.set_min_width(min_w);
    }

    let frame = Frame::none()
        .fill(fill)
        .inner_margin(Margin::ZERO)
        .rounding(Rounding::same(CARD_ROUNDING))
        .stroke(Stroke::new(1.0, Color32::from_gray(38)));

    let mut toggle_clicked = false;
    let mut remove_clicked = false;
    let mut title_dragged = false;
    let mut title_drag_released = false;

    let outer = frame.show(ui, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            let card_w = min_width.unwrap_or_else(|| ui.available_width());
            ui.set_min_width(card_w);
            ui.set_max_width(card_w);
            // Publish the card width so the floating mod-overlay can size
            // its wrap width to the actual card it belongs to.
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("back_card_w").with(module_id), card_w));
            // Match the front panel's grid height
            if let Some(mh) = min_height {
                ui.set_min_height(mh);
            }

            // ── Title bar — fixed height, same as front ────────────────────
            let (title_rect, _) =
                ui.allocate_exact_size(Vec2::new(card_w, TITLE_BAR_H), Sense::hover());
            let painter = ui.painter_at(title_rect);
            // Round top corners to match the card frame; bottom stays flat
            let title_rounding = Rounding {
                nw: CARD_ROUNDING,
                ne: CARD_ROUNDING,
                sw: 0.0,
                se: 0.0,
            };
            painter.rect_filled(title_rect, title_rounding, title_bg);
            painter.line_segment(
                [title_rect.left_bottom(), title_rect.right_bottom()],
                Stroke::new(1.0, theme::VOID),
            );
            draw_focus_shine(&painter, title_rect, kind, ui.ctx());
            let label_font = 9.5;
            // Title shifts right past the LED (when present) so wide names
            // like "BASS SYNTH" don't lose their leading character.
            let label_x_off = if kind.has_audio_output() { 18.0 } else { 10.0 };
            let label_pos = title_rect.left_center() + Vec2::new(label_x_off, 0.0);
            // For LlmAgent modules, show persona name (e.g. "LLM AGENT · BASS").
            // For LfoModule, append a #N slot index so each instance is
            // individually identifiable on the back panel.
            let label = if kind == ModuleKind::LlmAgent {
                let persona: String = ui
                    .ctx()
                    .data(|d| d.get_temp(egui::Id::new("agent_persona").with(module_id)))
                    .unwrap_or_default();
                if persona.is_empty() {
                    kind.label().to_string()
                } else {
                    format!("{} · {}", kind.label(), persona)
                }
            } else if kind == ModuleKind::LfoModule {
                let slot: usize = ui
                    .ctx()
                    .data(|d| d.get_temp(egui::Id::new("lfo_slot").with(module_id)))
                    .unwrap_or(0);
                format!("{} #{}", kind.label(), slot + 1)
            } else {
                kind.label().to_string()
            };
            painter.text(
                label_pos + Vec2::new(0.0, 1.0),
                egui::Align2::LEFT_CENTER,
                &label,
                egui::FontId::monospace(label_font),
                theme::VOID,
            );
            painter.text(
                label_pos,
                egui::Align2::LEFT_CENTER,
                &label,
                egui::FontId::monospace(label_font),
                Color32::from_gray(if enabled { 200 } else { 80 }),
            );
            // Back-panel "reaches MASTER" LED — only meaningful for modules
            // that emit audio (voices and FX).  Sequencer / LFO / agents /
            // meters always pass, so we skip the LED entirely there to keep
            // the title bar clean.
            if kind.has_audio_output() {
                let led_center = Pos2::new(title_rect.left() + 9.0, title_rect.center().y);
                let led_r = 2.6_f32;
                let led_hit = Rect::from_center_size(led_center, Vec2::splat(10.0));
                let led_resp =
                    ui.interact(led_hit, ui.id().with("led").with(module_id), Sense::click());
                if led_resp.clicked() {
                    toggle_clicked = true;
                }
                let lit = enabled && reaches_master;
                if lit {
                    // Asymmetric clip — same reasoning as the front-card
                    // LED: bleed into the inter-module gap (sides + down)
                    // but cap upward expansion so the halo doesn't run
                    // into the panel above.
                    let halo_pad = led_r * 6.0;
                    let upward_pad = 0.0;
                    let cr = painter.clip_rect();
                    let bounded = egui::Rect::from_min_max(
                        egui::pos2(cr.min.x - halo_pad, cr.min.y - upward_pad),
                        egui::pos2(cr.max.x + halo_pad, cr.max.y + halo_pad),
                    );
                    let extended = painter.clone().with_clip_rect(bounded);
                    theme::led(&extended, led_center, led_r, Color32::from_gray(220), 1.0);
                } else {
                    // Off / unreachable state — dark socket with a faint glint
                    // so the click target stays visible.
                    painter.circle_filled(led_center, led_r, Color32::from_gray(28));
                    painter.circle_stroke(
                        led_center,
                        led_r,
                        Stroke::new(0.7, Color32::from_gray(8)),
                    );
                }
                let state_line = if !enabled {
                    "Disabled — click to enable."
                } else if !reaches_master {
                    "No audio path to MASTER — patch a cable."
                } else {
                    "Enabled and reaching MASTER."
                };
                led_resp.on_hover_text(format!(
                    "Audio path indicator\n\
                     Lit when this module is enabled and its audio reaches MASTER.\n\
                     Click to toggle the module on / off.\n\
                     \n\
                     {state_line}"
                ));
            }
            // Drag zone
            let drag_rect = Rect::from_min_max(
                Pos2::new(title_rect.left() + 20.0, title_rect.min.y),
                Pos2::new(title_rect.right() - 30.0, title_rect.max.y),
            );
            let drag_resp = ui.interact(drag_rect, ui.id().with("title_drag"), Sense::drag());
            title_dragged = drag_resp.dragged();
            title_drag_released = drag_resp.drag_stopped();
            if drag_resp.hovered() || drag_resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            // Remove button — shown on all except core singletons
            let is_core = matches!(
                kind,
                ModuleKind::StepSequencer | ModuleKind::MasterOutput | ModuleKind::LlmConsole
            );
            if !is_core {
                let rm_rect = Rect::from_center_size(
                    Pos2::new(title_rect.right() - 12.0, title_rect.center().y),
                    Vec2::splat(10.0),
                );
                let rm_resp =
                    ui.interact(rm_rect, ui.id().with("rm").with(module_id), Sense::click());
                if rm_resp.clicked() {
                    remove_clicked = true;
                }
                let rm_col = if rm_resp.hovered() {
                    Color32::from_gray(200)
                } else {
                    theme::IRON
                };
                painter.text(
                    rm_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "×",
                    egui::FontId::monospace(10.0),
                    rm_col,
                );
            }

            // ── Port strip (inputs left, outputs right) ─────────────────────
            // Strip height grows with port count so mod jacks don't get
            // clipped on small modules.
            let strip_h = super::module_card_mod::back_strip_height(kind);
            let (strip_rect, _) =
                ui.allocate_exact_size(Vec2::new(card_w, strip_h), Sense::hover());
            let sp = ui.painter_at(strip_rect);
            // Faint module name watermark — anchored bottom-right so the
            // mod-overlay chips/sliders growing rightward from the left jacks
            // never collide with it.  (Was centred, which clipped the label
            // for modules with many mod inputs like the 909 kit.)
            sp.text(
                strip_rect.right_bottom() - Vec2::new(8.0, 6.0),
                egui::Align2::RIGHT_BOTTOM,
                kind.label(),
                egui::FontId::monospace(11.0),
                Color32::from_gray(32),
            );

            let port_hit_r = PORT_RADIUS + 4.0;
            let port_size = Vec2::splat(port_hit_r * 2.0);
            let left_x = strip_rect.left() + 16.0;
            let right_x = strip_rect.right() - 16.0;
            let label_font = egui::FontId::monospace(7.0);
            let label_col = theme::IRON;

            // Port presence — deferred to module_card_mod helpers.
            use super::module_card_mod as mcm;
            let has_audio_in = mcm::has_audio_in(kind);
            let has_cv_in = mcm::has_cv_in(kind);
            let has_control_in = mcm::has_control_in(kind);
            let has_cv_out = matches!(kind, ModuleKind::LfoModule | ModuleKind::StepSequencer);
            let has_control_out = matches!(kind, ModuleKind::LlmAgent);

            // ── Top row: AUD/CV/CTL ports laid out HORIZONTALLY ─────────────
            // Inputs grow left → right from left_x; labels sit below the
            // port circle so each port pair fits in a narrow column.  Mod
            // jacks then stack vertically below.
            let row_y = strip_rect.top() + 10.0;
            // Push the first mod jack down a little so the chip overlays
            // (which sit to the right of the labels) don't crowd the top
            // AUD/CV/CTL port row.  Bumped 28 → 32 so the slider row has
            // a few extra px of clearance from the AUD label text above.
            let mod_start_y = strip_rect.top() + 32.0;
            let port_step_x = 24.0;
            let mut in_x = left_x;
            let mut place_in = |kind_p: PortKind, label: &str, hover: &str, hash: &str| {
                let pos = Pos2::new(in_x, row_y);
                draw_port_circle(&sp, pos, kind_p, PortDir::In);
                ports.push(PortPos {
                    port: PortRef {
                        module_id,
                        dir: PortDir::In,
                        kind: kind_p,
                        index: 0,
                    },
                    center: pos,
                });
                sp.text(
                    pos + Vec2::new(0.0, 8.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    label_font.clone(),
                    label_col,
                );
                ui.interact(
                    Rect::from_center_size(pos, port_size),
                    ui.id().with(hash),
                    Sense::hover(),
                )
                .on_hover_text(hover);
                in_x += port_step_x;
            };
            if has_audio_in {
                place_in(PortKind::Audio, "AUD", "Audio In", "bp_ain");
            }
            if has_cv_in {
                place_in(PortKind::Cv, "CV", "CV / Gate In", "bp_cin");
            }
            if has_control_in {
                place_in(PortKind::Control, "CTL", "Control In (LLM)", "bp_ctlin");
            }
            super::module_card_mod::draw_mod_input_ports(
                ui,
                &sp,
                module_id,
                kind,
                left_x,
                mod_start_y,
                &label_font,
                label_col,
                port_size,
                ports,
            );

            // ── RIGHT side: output ports — also horizontal, right→left ──────
            let mut out_x = right_x;
            let mut place_out = |kind_p: PortKind, label: String, hover: &str, hash: &str| {
                let pos = Pos2::new(out_x, row_y);
                draw_port_circle(&sp, pos, kind_p, PortDir::Out);
                ports.push(PortPos {
                    port: PortRef {
                        module_id,
                        dir: PortDir::Out,
                        kind: kind_p,
                        index: 0,
                    },
                    center: pos,
                });
                sp.text(
                    pos + Vec2::new(0.0, 8.0),
                    egui::Align2::CENTER_TOP,
                    &label,
                    label_font.clone(),
                    label_col,
                );
                ui.interact(
                    Rect::from_center_size(pos, port_size),
                    ui.id().with(hash),
                    Sense::hover(),
                )
                .on_hover_text(hover);
                out_x -= port_step_x;
            };
            place_out(PortKind::Audio, "AUD".into(), "Audio Out", "bp_aout");
            if has_cv_out {
                let cv_label = if kind == ModuleKind::LfoModule {
                    let slot: usize = ui
                        .ctx()
                        .data(|d| d.get_temp(egui::Id::new("lfo_slot").with(module_id)))
                        .unwrap_or(0);
                    format!("#{}", slot + 1)
                } else {
                    "CV".into()
                };
                place_out(PortKind::Cv, cv_label, "CV Out", "bp_cout");
            }
            if has_control_out {
                place_out(
                    PortKind::Control,
                    "CTL".into(),
                    "Control Out (LLM)",
                    "bp_ctlout",
                );
            }

            // ── LLM CONSOLE: keyboard hint banner ──────────────────────
            // The console has no audio bus and very few jacks, so its
            // back panel sits mostly empty.  Use the space to surface
            // the keyboard shortcuts for hiding cables and scrolling.
            if kind == ModuleKind::LlmConsole {
                let avail_h = ui.available_height();
                if avail_h > 18.0 {
                    let wasd = ui
                        .ctx()
                        .data(|d| d.get_temp::<bool>(egui::Id::new("wasd_as_arrows")))
                        .unwrap_or(false);
                    let scroll_line = if wasd {
                        "WASD to scroll."
                    } else {
                        "Cursors to scroll."
                    };
                    let (banner_rect, _) =
                        ui.allocate_exact_size(Vec2::new(card_w, avail_h), Sense::hover());
                    let bp = ui.painter_at(banner_rect);
                    let font = egui::FontId::monospace(13.0);
                    let col = Color32::from_gray(165);
                    let line_h = 16.0;
                    let cy = banner_rect.center().y;
                    bp.text(
                        egui::pos2(banner_rect.center().x, cy - line_h * 0.5),
                        egui::Align2::CENTER_CENTER,
                        "Hold Alt to hide cables.",
                        font.clone(),
                        col,
                    );
                    bp.text(
                        egui::pos2(banner_rect.center().x, cy + line_h * 0.5),
                        egui::Align2::CENTER_CENTER,
                        scroll_line,
                        font,
                        col,
                    );
                }
            }
        });
    });

    CardResponse {
        toggle_clicked,
        remove_clicked,
        title_dragged,
        title_drag_released,
        collapse_clicked: false,
        xy_pad_toggle_clicked: false,
        card_rect: outer.response.rect,
    }
}
