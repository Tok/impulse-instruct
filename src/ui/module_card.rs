// ─── ui/module_card.rs ────────────────────────────────────────────────────────
// Wraps a panel draw function in a labelled, bordered module card.
// Each card has a title bar with enable toggle + port indicators, a content
// area, and audio in/out port circles registered for cable hit-testing.

use egui::{Color32, Frame, Margin, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

use crate::state::{ModuleKind, PortDir, PortKind, PortRef};

// ─── Port registry (per-frame) ───────────────────────────────────────────────

/// Screen position of a module port — populated during card rendering,
/// consumed by the cable overlay draw pass.
#[derive(Clone, Debug)]
pub struct PortPos {
    pub port: PortRef,
    pub center: Pos2,
}

// ─── Card colours ─────────────────────────────────────────────────────────────

fn title_fill(kind: ModuleKind) -> Color32 {
    // All title bars are grayscale (R=G=B), differentiated by lightness only.
    let v: u8 = match kind {
        ModuleKind::StepSequencer => 22,
        ModuleKind::MasterOutput => 20,
        ModuleKind::AcidBass | ModuleKind::HooverLead => 26,
        ModuleKind::DrumKit808
        | ModuleKind::DrumKit909
        | ModuleKind::AmenSampler
        | ModuleKind::NoiseVoice
        | ModuleKind::GranularTexture => 24,
        ModuleKind::An1xVoice => 28,
        ModuleKind::NeuTts => 26,
        ModuleKind::FxReverb
        | ModuleKind::FxDelay
        | ModuleKind::FxChorus
        | ModuleKind::FxPhaser
        | ModuleKind::FxRingMod
        | ModuleKind::FxWaveshaper
        | ModuleKind::FxBitcrush
        | ModuleKind::FxEq
        | ModuleKind::FxCompressor
        | ModuleKind::FxTapeSat
        | ModuleKind::FxDrive
        | ModuleKind::FxAutotune
        | ModuleKind::SpectrumAnalyzer
        | ModuleKind::StereoMeter
        | ModuleKind::ActivityTimeline => 20,
        ModuleKind::LfoModule => 18,
        ModuleKind::LlmAgent => 30,
        ModuleKind::LlmConsole => 28,
    };
    Color32::from_gray(v)
}

/// Resolve the effective title background for a module, applying focus highlight
/// if the module kind matches the currently focused module.
fn focused_title_bg(ctx: &egui::Context, kind: ModuleKind) -> Color32 {
    let base = title_fill(kind);
    let focus_info: Option<(ModuleKind, f32)> = ctx
        .data(|d| d.get_temp(egui::Id::new("focused_module")))
        .flatten();
    match focus_info {
        Some((fk, elapsed)) if fk == kind => {
            let v = base.r();
            // Steady bright lift while focused
            let lift = 30u8;
            // Shine pulse: bright flash that decays over ~1.5s
            let shine = (80.0 * (-elapsed * 2.5).exp()) as u8;
            Color32::from_gray(v.saturating_add(lift).saturating_add(shine))
        }
        _ => base,
    }
}

/// Draw a sweeping shine overlay on a focused module's title bar.
/// The sweep moves left-to-right over ~0.8s then fades.
fn draw_focus_shine(
    painter: &egui::Painter,
    title_rect: Rect,
    kind: ModuleKind,
    ctx: &egui::Context,
) {
    let focus_info: Option<(ModuleKind, f32)> = ctx
        .data(|d| d.get_temp(egui::Id::new("focused_module")))
        .flatten();
    if let Some((fk, elapsed)) = focus_info
        && fk == kind
        && elapsed < 1.5
    {
        // Gradient wave expanding outward from the center of the title bar.
        let t = (elapsed / 0.8).min(1.0); // 0..1 over 0.8s
        let fade = (1.0 - elapsed / 1.5).max(0.0); // overall fade over 1.5s
        let half_w = title_rect.width() * 0.5;
        let spread = t * half_w; // how far the wave has spread from center
        let cx = title_rect.center().x;
        let top = title_rect.top();
        let bot = title_rect.bottom();

        // Draw gradient in slices — brighter at the wave front, fading behind it.
        let slices = 12;
        for i in 0..slices {
            let frac = i as f32 / slices as f32;
            let next_frac = (i + 1) as f32 / slices as f32;
            let dist = frac * spread;
            let next_dist = next_frac * spread;
            // Brightness peaks at the wave front (frac ≈ 1.0)
            let intensity = frac * frac; // quadratic ramp — brighter at edge
            let alpha = (intensity * fade * 50.0) as u8;
            if alpha == 0 {
                continue;
            }
            let col = Color32::from_white_alpha(alpha);
            // Right half
            let r = Rect::from_x_y_ranges(
                (cx + dist)..=(cx + next_dist).min(title_rect.right()),
                top..=bot,
            )
            .intersect(title_rect);
            if r.width() > 0.0 {
                painter.rect_filled(r, Rounding::ZERO, col);
            }
            // Left half (mirror)
            let l = Rect::from_x_y_ranges(
                (cx - next_dist).max(title_rect.left())..=(cx - dist),
                top..=bot,
            )
            .intersect(title_rect);
            if l.width() > 0.0 {
                painter.rect_filled(l, Rounding::ZERO, col);
            }
        }

        // Subtle border pulse on the entire card (same fade timing)
        let border_alpha = (fade * 25.0) as u8;
        if border_alpha > 0 {
            // Expand title_rect down to approximate the full card border
            let card_bottom = title_rect.bottom() + 200.0; // rough; clips to painter
            let card_rect = Rect::from_min_max(
                title_rect.left_top(),
                egui::Pos2::new(title_rect.right(), card_bottom),
            );
            painter.rect_stroke(
                card_rect,
                Rounding::same(8.0),
                Stroke::new(1.0, Color32::from_white_alpha(border_alpha)),
            );
        }
    }
}

// ─── Port circle helpers ──────────────────────────────────────────────────────

pub const PORT_RADIUS: f32 = 5.5;
const PORT_HOLE: f32 = 2.2;

/// Port radius for a given kind — Control and Mod ports are smaller.
pub fn port_radius(kind: PortKind) -> f32 {
    match kind {
        PortKind::Control => 3.5,
        PortKind::Mod => 3.8,
        _ => PORT_RADIUS,
    }
}

/// Draw a jack port circle and return its centre.
pub fn draw_port_circle(painter: &egui::Painter, center: Pos2, kind: PortKind, dir: PortDir) {
    let r = port_radius(kind);
    let small = matches!(kind, PortKind::Control | PortKind::Mod);
    let hole = if small { 1.3 } else { PORT_HOLE };
    let ring_color = Color32::from_gray(if small { 80 } else { 100 });
    painter.circle_filled(center, r + 1.5, Color32::from_gray(12));
    painter.circle_filled(center, r, ring_color);
    let inner = match dir {
        PortDir::Out => Color32::from_gray(if small { 45 } else { 60 }),
        PortDir::In => Color32::from_gray(if small { 25 } else { 30 }),
    };
    painter.circle_filled(center, r - 1.5, inner);
    painter.circle_filled(center, hole, Color32::from_gray(8));
    if kind == PortKind::Cv {
        painter.circle_filled(center, 0.8, Color32::from_gray(180));
    } else if kind == PortKind::Mod {
        let c = Color32::from_gray(170);
        painter.circle_filled(center + Vec2::new(-1.6, 0.0), 0.6, c);
        painter.circle_filled(center + Vec2::new(1.6, 0.0), 0.6, c);
    }
}

// ─── Card draw ────────────────────────────────────────────────────────────────

pub struct CardResponse {
    pub toggle_clicked: bool,
    pub remove_clicked: bool,
    pub title_dragged: bool,
    pub title_drag_released: bool,
    pub collapse_clicked: bool,
    pub card_rect: Rect,
}

/// Draw a module card around `content`, registering port positions into `ports`.
/// Returns a `CardResponse`.
///
/// `module_id` — used to build `PortRef` values.
/// `kind`      — determines colour, title, and port configuration.
/// `enabled`   — if false, card content is dimmed.
/// `min_width` — minimum card width in pixels; `None` → fill available.
/// `scale`     — per-module UI scale factor (1.0 = default).
pub fn module_card<R>(
    ui: &mut egui::Ui,
    module_id: u32,
    kind: ModuleKind,
    enabled: bool,
    min_width: Option<f32>,
    scale: f32,
    ports: &mut Vec<PortPos>,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> (CardResponse, Option<R>) {
    module_card_sized(
        ui, module_id, kind, enabled, min_width, None, scale, ports, content,
    )
}

/// Module card with explicit min_height for grid conformance.
/// Only sets `set_min_height` — content taller than this just grows naturally.
pub fn module_card_sized<R>(
    ui: &mut egui::Ui,
    _module_id: u32,
    kind: ModuleKind,
    enabled: bool,
    min_width: Option<f32>,
    min_height: Option<f32>,
    scale: f32,
    _ports: &mut Vec<PortPos>,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> (CardResponse, Option<R>) {
    // Per-module collapse: stored in egui persistent data keyed by module id
    let collapse_id = egui::Id::new("collapsed").with(_module_id);
    let collapsed = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(collapse_id))
        .unwrap_or(false);
    // Publish scale so content draw functions can read it without signature changes.
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("module_scale"), scale));

    let fill = Color32::from_gray(14);
    let title_bg = focused_title_bg(ui.ctx(), kind);

    // Set minimum width so the card doesn't shrink below the grid slot.
    // Max width is NOT set here — the caller constrains width via allocate_ui
    // or the frame's inner max_width handles overflow.
    if let Some(w) = min_width {
        ui.set_min_width(w);
    }

    let card_rounding = 8.0;
    let frame = Frame::none()
        .fill(fill)
        .inner_margin(Margin::ZERO)
        .rounding(Rounding::same(card_rounding))
        .stroke(Stroke::new(1.0, Color32::from_gray(38)));

    let mut toggle_clicked = false;
    let mut remove_clicked = false;
    let mut title_dragged = false;
    let mut title_drag_released = false;
    let mut collapse_clicked = false;

    let inner = frame.show(ui, |ui| {
        // Module cards are always vertical (title bar on top, content below),
        // regardless of the parent layout (e.g. horizontal_wrapped in the rack).
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            // Pin the card to exactly the requested width (or the slot width provided
            // by the enclosing scope).  Setting both min and max prevents content from
            // shrinking the card or overflowing its slot.
            let card_w = min_width.unwrap_or_else(|| ui.available_width());
            ui.set_min_width(card_w);
            ui.set_max_width(card_w);

            // ── Title bar — fixed height, not affected by per-module scale ──
            let title_h = 22.0_f32;
            // Use card_w explicitly — avoids inheriting stale available_width from
            // the horizontal_wrapped parent when the content hasn't settled yet.
            let (title_rect, _) =
                ui.allocate_exact_size(Vec2::new(card_w, title_h), Sense::hover());
            let painter = ui.painter_at(title_rect);

            // Title bar fill — rounding matches the outer frame so the fill
            // doesn't leak past the rounded corners.  The frame stroke (1px) is
            // painted on top by egui after all content, so it covers the edges.
            let title_rounding = Rounding {
                nw: card_rounding,
                ne: card_rounding,
                sw: 0.0,
                se: 0.0,
            };
            painter.rect_filled(title_rect, title_rounding, title_bg);
            // 1px shadow at bottom (flat — meets content area)
            painter.line_segment(
                [title_rect.left_bottom(), title_rect.right_bottom()],
                Stroke::new(1.0, Color32::from_gray(8)),
            );
            // Focus shine sweep overlay
            draw_focus_shine(&painter, title_rect, kind, ui.ctx());

            // Module kind label (embossed: shadow 1px below, then bright text)
            let label_font = 9.5;
            let label_pos = title_rect.left_center() + Vec2::new(10.0, 0.0);
            painter.text(
                label_pos + Vec2::new(0.0, 1.0),
                egui::Align2::LEFT_CENTER,
                kind.label(),
                egui::FontId::monospace(label_font),
                Color32::from_gray(8),
            );
            painter.text(
                label_pos,
                egui::Align2::LEFT_CENTER,
                kind.label(),
                egui::FontId::monospace(label_font),
                Color32::from_gray(if enabled { 200 } else { 80 }),
            );

            // ── Enable toggle (small square LED left of label) ────────────────
            let led_rect = Rect::from_center_size(
                Pos2::new(title_rect.left() + 4.0, title_rect.center().y),
                Vec2::splat(5.0),
            );
            let led_resp = ui.interact(
                led_rect,
                ui.id().with("led").with(_module_id),
                Sense::click(),
            );
            if led_resp.clicked() {
                toggle_clicked = true;
            }
            let led_color = if enabled {
                Color32::from_gray(220)
            } else {
                Color32::from_gray(32)
            };
            painter.rect_filled(led_rect, Rounding::same(1.0), led_color);

            // ── Title bar drag (for module reorder) ───────────────────────────
            // Use a wide drag zone in the centre of the title bar, clear of LED/buttons/ports.
            let drag_right = (title_rect.right() - 60.0).max(title_rect.left() + 40.0);
            let drag_rect = Rect::from_min_max(
                Pos2::new(title_rect.left() + 20.0, title_rect.min.y),
                Pos2::new(drag_right, title_rect.max.y),
            );
            let drag_resp = ui.interact(
                drag_rect,
                ui.id().with("title_drag").with(_module_id),
                Sense::click_and_drag(),
            );
            let is_fixed = matches!(
                kind,
                ModuleKind::StepSequencer | ModuleKind::MasterOutput | ModuleKind::LlmConsole
            );
            if !is_fixed {
                title_dragged = drag_resp.dragged();
                title_drag_released = drag_resp.drag_stopped();
            }
            if drag_resp.clicked() {
                collapse_clicked = true;
                ui.ctx()
                    .data_mut(|d| d.insert_temp(collapse_id, !collapsed));
            }
            if !is_fixed && (drag_resp.hovered() || drag_resp.dragged()) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if !is_fixed {
                let dot_col = if drag_resp.hovered() || drag_resp.dragged() {
                    Color32::from_gray(140)
                } else {
                    Color32::from_gray(55)
                };
                let handle_x = drag_rect.right() - 10.0;
                let cy = title_rect.center().y;
                for i in 0i32..3 {
                    let dx = (i - 1) as f32 * 5.0;
                    painter.circle_filled(Pos2::new(handle_x + dx, cy), 1.5, dot_col);
                }
            }

            // ── Remove button (×) — shown on all except core singletons
            let rm_rect = Rect::from_center_size(
                Pos2::new(title_rect.right() - 44.0, title_rect.center().y),
                Vec2::splat(10.0),
            );
            let is_core = matches!(
                kind,
                ModuleKind::StepSequencer | ModuleKind::MasterOutput | ModuleKind::LlmConsole
            );
            if !is_core {
                let rm_resp =
                    ui.interact(rm_rect, ui.id().with("rm").with(_module_id), Sense::click());
                if rm_resp.clicked() {
                    remove_clicked = true;
                }
                let rm_col = if rm_resp.hovered() {
                    Color32::from_gray(200)
                } else {
                    Color32::from_gray(60)
                };
                painter.text(
                    rm_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "×",
                    egui::FontId::monospace(10.0),
                    rm_col,
                );
            }

            // ── Content (hidden when collapsed) ──────────────────────────────
            if collapsed {
                None
            } else {
                // Compute content budget from grid height (if provided).
                let title_h = 22.0;
                let margin_y = 8.0 * scale * 2.0;
                let content_budget = min_height.map(|mh| (mh - title_h - margin_y).max(20.0));

                let content_frame = Frame::none()
                    .fill(fill)
                    .rounding(Rounding::same(card_rounding))
                    .inner_margin(Margin::symmetric(6.0 * scale, 8.0 * scale));
                let inner_resp = content_frame.show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(2.0 * scale, 2.0 * scale);
                    ui.set_max_width(card_w - 12.0);
                    let draw_content = |ui: &mut egui::Ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                            if enabled {
                                content(ui)
                            } else {
                                ui.add_enabled_ui(false, |ui| content(ui)).inner
                            }
                        })
                        .inner
                    };
                    if let Some(budget) = content_budget {
                        // Grid height set: scroll overflow, enforce exact height
                        egui::ScrollArea::vertical()
                            .max_height(budget)
                            .auto_shrink([false; 2])
                            .show(ui, draw_content)
                            .inner
                    } else {
                        draw_content(ui)
                    }
                });
                Some(inner_resp.inner)
            }
        })
        .inner
    });

    let card_rect = inner.response.rect;
    (
        CardResponse {
            toggle_clicked,
            remove_clicked,
            title_dragged,
            title_drag_released,
            collapse_clicked,
            card_rect,
        },
        inner.inner,
    )
}

// ─── Back-panel card ─────────────────────────────────────────────────────────
// Simplified card for the rack's back panel: title bar + port strip, no content.

/// Draw a back-panel module card showing only ports (inputs left, outputs right)
/// and a faint module name watermark. Used when the rack is flipped.
pub fn module_card_back(
    ui: &mut egui::Ui,
    module_id: u32,
    kind: ModuleKind,
    enabled: bool,
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

    let card_rounding = 8.0;
    let frame = Frame::none()
        .fill(fill)
        .inner_margin(Margin::ZERO)
        .rounding(Rounding::same(card_rounding))
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
            // Match the front panel's grid height
            if let Some(mh) = min_height {
                ui.set_min_height(mh);
            }

            // ── Title bar — fixed height, same as front ────────────────────
            let title_h = 22.0_f32;
            let (title_rect, _) =
                ui.allocate_exact_size(Vec2::new(card_w, title_h), Sense::hover());
            let painter = ui.painter_at(title_rect);
            // Round top corners to match the card frame; bottom stays flat
            let title_rounding = Rounding {
                nw: card_rounding,
                ne: card_rounding,
                sw: 0.0,
                se: 0.0,
            };
            painter.rect_filled(title_rect, title_rounding, title_bg);
            painter.line_segment(
                [title_rect.left_bottom(), title_rect.right_bottom()],
                Stroke::new(1.0, Color32::from_gray(8)),
            );
            draw_focus_shine(&painter, title_rect, kind, ui.ctx());
            let label_font = 9.5;
            let label_pos = title_rect.left_center() + Vec2::new(10.0, 0.0);
            // For LlmAgent modules, show persona name (e.g. "LLM AGENT · BASS")
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
            } else {
                kind.label().to_string()
            };
            painter.text(
                label_pos + Vec2::new(0.0, 1.0),
                egui::Align2::LEFT_CENTER,
                &label,
                egui::FontId::monospace(label_font),
                Color32::from_gray(8),
            );
            painter.text(
                label_pos,
                egui::Align2::LEFT_CENTER,
                &label,
                egui::FontId::monospace(label_font),
                Color32::from_gray(if enabled { 200 } else { 80 }),
            );
            // LED toggle
            let led_rect = Rect::from_center_size(
                Pos2::new(title_rect.left() + 4.0, title_rect.center().y),
                Vec2::splat(5.0),
            );
            if ui
                .interact(
                    led_rect,
                    ui.id().with("led").with(module_id),
                    Sense::click(),
                )
                .clicked()
            {
                toggle_clicked = true;
            }
            painter.rect_filled(
                led_rect,
                Rounding::same(1.0),
                Color32::from_gray(if enabled { 220 } else { 32 }),
            );
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
                    Color32::from_gray(60)
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
            let strip_h = 52.0;
            let (strip_rect, _) =
                ui.allocate_exact_size(Vec2::new(card_w, strip_h), Sense::hover());
            let sp = ui.painter_at(strip_rect);
            // Faint module name watermark
            sp.text(
                strip_rect.center(),
                egui::Align2::CENTER_CENTER,
                kind.label(),
                egui::FontId::monospace(11.0),
                Color32::from_gray(28),
            );

            let port_hit_r = PORT_RADIUS + 4.0;
            let port_size = Vec2::splat(port_hit_r * 2.0);
            let left_x = strip_rect.left() + 16.0;
            let right_x = strip_rect.right() - 16.0;
            let label_font = egui::FontId::monospace(7.0);
            let label_col = Color32::from_gray(60);

            // Determine which ports this module has
            let has_audio_in = matches!(
                kind,
                ModuleKind::FxReverb
                    | ModuleKind::FxDelay
                    | ModuleKind::FxChorus
                    | ModuleKind::FxPhaser
                    | ModuleKind::FxRingMod
                    | ModuleKind::FxWaveshaper
                    | ModuleKind::FxBitcrush
                    | ModuleKind::FxEq
                    | ModuleKind::FxCompressor
                    | ModuleKind::FxTapeSat
                    | ModuleKind::FxDrive
                    | ModuleKind::FxAutotune
                    | ModuleKind::MasterOutput
            );
            let has_cv_out = matches!(kind, ModuleKind::LfoModule | ModuleKind::StepSequencer);
            let has_control_out = matches!(kind, ModuleKind::LlmAgent);
            let has_control_in = !matches!(
                kind,
                ModuleKind::MasterOutput | ModuleKind::LlmAgent | ModuleKind::LlmConsole
            );
            let has_cv_in = matches!(
                kind,
                ModuleKind::AcidBass
                    | ModuleKind::DrumKit808
                    | ModuleKind::DrumKit909
                    | ModuleKind::HooverLead
                    | ModuleKind::An1xVoice
                    | ModuleKind::AmenSampler
                    | ModuleKind::NoiseVoice
                    | ModuleKind::NeuTts
            );

            // ── LEFT side: input ports ──────────────────────────────────────
            let mut in_y = strip_rect.center().y - 10.0;
            if has_audio_in {
                let pos = Pos2::new(left_x, in_y);
                draw_port_circle(&sp, pos, PortKind::Audio, PortDir::In);
                ports.push(PortPos {
                    port: PortRef {
                        module_id,
                        dir: PortDir::In,
                        kind: PortKind::Audio,
                        index: 0,
                    },
                    center: pos,
                });
                sp.text(
                    pos + Vec2::new(10.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    "AUD",
                    label_font.clone(),
                    label_col,
                );
                ui.interact(
                    Rect::from_center_size(pos, port_size),
                    ui.id().with("bp_ain"),
                    Sense::hover(),
                )
                .on_hover_text("Audio In");
                in_y += 20.0;
            }
            if has_cv_in {
                let pos = Pos2::new(left_x, in_y);
                draw_port_circle(&sp, pos, PortKind::Cv, PortDir::In);
                ports.push(PortPos {
                    port: PortRef {
                        module_id,
                        dir: PortDir::In,
                        kind: PortKind::Cv,
                        index: 0,
                    },
                    center: pos,
                });
                sp.text(
                    pos + Vec2::new(10.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    "CV",
                    label_font.clone(),
                    label_col,
                );
                ui.interact(
                    Rect::from_center_size(pos, port_size),
                    ui.id().with("bp_cin"),
                    Sense::hover(),
                )
                .on_hover_text("CV / Gate In");
                in_y += 20.0;
            }
            if has_control_in {
                let pos = Pos2::new(left_x, in_y);
                draw_port_circle(&sp, pos, PortKind::Control, PortDir::In);
                ports.push(PortPos {
                    port: PortRef {
                        module_id,
                        dir: PortDir::In,
                        kind: PortKind::Control,
                        index: 0,
                    },
                    center: pos,
                });
                sp.text(
                    pos + Vec2::new(10.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    "CTL",
                    label_font.clone(),
                    label_col,
                );
                in_y += 20.0;
            }
            super::module_card_mod::draw_mod_input_ports(
                ui,
                &sp,
                module_id,
                kind,
                left_x,
                in_y,
                &label_font,
                label_col,
                port_size,
                ports,
            );

            // ── RIGHT side: output ports ────────────────────────────────────
            let mut out_y = strip_rect.center().y - 10.0;
            {
                let pos = Pos2::new(right_x, out_y);
                draw_port_circle(&sp, pos, PortKind::Audio, PortDir::Out);
                ports.push(PortPos {
                    port: PortRef {
                        module_id,
                        dir: PortDir::Out,
                        kind: PortKind::Audio,
                        index: 0,
                    },
                    center: pos,
                });
                sp.text(
                    pos + Vec2::new(-10.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    "AUD",
                    label_font.clone(),
                    label_col,
                );
                ui.interact(
                    Rect::from_center_size(pos, port_size),
                    ui.id().with("bp_aout"),
                    Sense::hover(),
                )
                .on_hover_text("Audio Out");
                out_y += 20.0;
            }
            if has_cv_out {
                let pos = Pos2::new(right_x, out_y);
                draw_port_circle(&sp, pos, PortKind::Cv, PortDir::Out);
                ports.push(PortPos {
                    port: PortRef {
                        module_id,
                        dir: PortDir::Out,
                        kind: PortKind::Cv,
                        index: 0,
                    },
                    center: pos,
                });
                sp.text(
                    pos + Vec2::new(-10.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    "CV",
                    label_font.clone(),
                    label_col,
                );
                ui.interact(
                    Rect::from_center_size(pos, port_size),
                    ui.id().with("bp_cout"),
                    Sense::hover(),
                )
                .on_hover_text("CV Out");
                out_y += 20.0;
            }
            if has_control_out {
                let pos = Pos2::new(right_x, out_y);
                draw_port_circle(&sp, pos, PortKind::Control, PortDir::Out);
                ports.push(PortPos {
                    port: PortRef {
                        module_id,
                        dir: PortDir::Out,
                        kind: PortKind::Control,
                        index: 0,
                    },
                    center: pos,
                });
                sp.text(
                    pos + Vec2::new(-10.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    "CTL",
                    label_font,
                    label_col,
                );
            }
        });
    });

    CardResponse {
        toggle_clicked,
        remove_clicked,
        title_dragged,
        title_drag_released,
        collapse_clicked: false,
        card_rect: outer.response.rect,
    }
}

// ─── Zone rail ────────────────────────────────────────────────────────────────

/// Draw a horizontal zone rail separator with collapse toggle, label, and optional [+ Add] button.
/// `bg_gray` sets the zone rail background brightness (R=G=B — no tint).
/// `collapsed` reflects the current collapse state (affects the ▶/▼ arrow drawn).
/// `all_collapsed` is true when every zone is collapsed (affects the ▼▼/▶▶ button).
/// Returns `(add_clicked, collapse_toggle_clicked, collapse_all_clicked)`.
pub fn zone_rail(
    ui: &mut egui::Ui,
    label: &str,
    show_add: bool,
    bg_gray: u8,
    collapsed: bool,
    all_collapsed: bool,
) -> (bool, bool, bool) {
    let mut add_clicked = false;
    let mut collapse_clicked = false;
    let mut collapse_all_clicked = false;
    let (rail_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let painter = ui.painter_at(rail_rect);

    // Rail background — distinct per zone
    painter.rect_filled(rail_rect, Rounding::ZERO, Color32::from_gray(bg_gray));
    // Top highlight / bottom shadow
    painter.line_segment(
        [rail_rect.left_top(), rail_rect.right_top()],
        Stroke::new(1.0, Color32::from_gray(50)),
    );
    painter.line_segment(
        [rail_rect.left_bottom(), rail_rect.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(8)),
    );
    // Screw holes along the rail
    let screw_y = rail_rect.center().y;
    for x in (16..rail_rect.width() as i32).step_by(80) {
        let cx = rail_rect.left() + x as f32;
        painter.circle_filled(Pos2::new(cx, screw_y), 3.5, Color32::from_gray(12));
        painter.circle_stroke(
            Pos2::new(cx, screw_y),
            3.5,
            Stroke::new(0.5, Color32::from_gray(40)),
        );
        // Cross slots
        painter.line_segment(
            [Pos2::new(cx - 1.5, screw_y), Pos2::new(cx + 1.5, screw_y)],
            Stroke::new(0.5, Color32::from_gray(50)),
        );
    }
    // Collapse toggle (▶ collapsed / ▼ expanded) — left side before the label
    let arrow_rect = Rect::from_center_size(
        Pos2::new(rail_rect.left() + 8.0, screw_y),
        Vec2::splat(14.0),
    );
    let arrow_resp = ui.interact(
        arrow_rect,
        ui.id().with("collapse").with(label),
        Sense::click(),
    );
    if arrow_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let arrow_col = if arrow_resp.hovered() {
        Color32::from_gray(160)
    } else {
        Color32::from_gray(70)
    };
    let arrow = if collapsed { "▶" } else { "▼" };
    painter.text(
        arrow_rect.center(),
        egui::Align2::CENTER_CENTER,
        arrow,
        egui::FontId::monospace(8.0),
        arrow_col,
    );
    if arrow_resp.clicked() {
        collapse_clicked = true;
    }

    // Zone label — shifted right to make room for the arrow
    painter.text(
        Pos2::new(rail_rect.left() + 20.0, screw_y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::monospace(8.5),
        Color32::from_gray(70),
    );

    // ▼▼/▶▶ collapse/expand all — right-justified
    {
        let all_label = if all_collapsed { "▼▼" } else { "▶▶" };
        let all_rect = Rect::from_center_size(
            Pos2::new(rail_rect.right() - 16.0, screw_y),
            Vec2::new(20.0, 13.0),
        );
        let all_resp = ui.interact(all_rect, ui.id().with("all").with(label), Sense::click());
        let all_col = if all_resp.hovered() {
            Color32::from_gray(140)
        } else {
            Color32::from_gray(65)
        };
        painter.text(
            all_rect.center(),
            egui::Align2::CENTER_CENTER,
            all_label,
            egui::FontId::monospace(7.5),
            all_col,
        );
        if all_resp.clicked() {
            collapse_all_clicked = true;
        }
    }

    // [+ ADD] button — left of the ▼▼/▶▶ button
    if show_add {
        let btn_rect = Rect::from_center_size(
            Pos2::new(rail_rect.right() - 56.0, screw_y),
            Vec2::new(44.0, 13.0),
        );
        let btn_resp = ui.interact(btn_rect, ui.id().with("add").with(label), Sense::click());
        let btn_col = if btn_resp.hovered() {
            Color32::from_gray(120)
        } else {
            Color32::from_gray(55)
        };
        painter.rect_filled(btn_rect, Rounding::same(2.0), Color32::from_gray(24));
        painter.rect_stroke(btn_rect, Rounding::same(2.0), Stroke::new(0.5, btn_col));
        painter.text(
            btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            "+ ADD",
            egui::FontId::monospace(8.0),
            btn_col,
        );
        if btn_resp.clicked() {
            add_clicked = true;
        }
    }

    (add_clicked, collapse_clicked, collapse_all_clicked)
}
