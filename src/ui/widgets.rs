// ─── ui/widgets.rs ───────────────────────────────────────────────────────────
#![allow(dead_code)] // widget library grows alongside panels
// Custom widgets: rotary knob, step button, LED indicator.

use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::f32::consts::TAU;

use super::theme;
use crate::state::ParamMode;

// ─── Mode indicator colours ───────────────────────────────────────────────────

/// Indicator brightness for each param mode — all values are pure grey (R=G=B).
///
/// Free      = near-invisible (28)  — no visual noise for unmanaged params
/// UserOwned = dim iron (60)        — "locked down", deliberately subdued
/// LlmFocus  = near-white (235)     — brightest state: LLM is actively driving this
fn mode_color(mode: ParamMode) -> Color32 {
    match mode {
        ParamMode::Free => Color32::from_gray(28),
        ParamMode::UserOwned => theme::IRON, // 60
        ParamMode::LlmFocus => theme::CHALK, // 235
    }
}

/// Short character label for the mode indicator button on sliders.
fn mode_char(mode: ParamMode) -> &'static str {
    match mode {
        ParamMode::Free => "·",
        ParamMode::UserOwned => "U",
        ParamMode::LlmFocus => "F",
    }
}

/// Hover tooltip text explaining the current mode and how to cycle.
/// Knobs: right-click cycles. Sliders: click the ·/U/F button.
fn mode_tooltip(mode: ParamMode) -> &'static str {
    match mode {
        ParamMode::Free => {
            "· Free — LLM and user both control this\nRight-click to lock to user only (U)"
        }
        ParamMode::UserOwned => {
            "U User owned — LLM ignores this param\nRight-click to set LLM focus (F)"
        }
        ParamMode::LlmFocus => {
            "F LLM focus — model will actively drive this param\nRight-click to release (·)"
        }
    }
}

// ─── Rotary Knob ─────────────────────────────────────────────────────────────

/// A rotary knob widget.
/// Returns `(value_changed, mode_cycled)`.
/// Left-drag changes the value; right-click cycles the param mode.
pub fn knob(ui: &mut Ui, label: &str, value: &mut f32, mode: ParamMode) -> (bool, bool) {
    let size = 44.0_f32;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(size + 14.0), Sense::click_and_drag());

    let knob_rect = Rect::from_center_size(rect.center() - Vec2::new(0.0, 6.0), Vec2::splat(size));
    let label_rect = Rect::from_min_size(
        Pos2::new(rect.min.x, knob_rect.max.y + 1.0),
        Vec2::new(rect.width(), 12.0),
    );

    let mut changed = false;

    // UserOwned: block dragging so the user explicitly has to unlock first
    if mode != ParamMode::UserOwned && response.dragged() {
        let delta = response.drag_delta();
        *value = (*value - delta.y * 0.005 + delta.x * 0.003).clamp(0.0, 1.0);
        changed = true;
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        draw_knob(painter, knob_rect, *value, mode, response.hovered());
        painter.text(
            label_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace(8.5),
            if mode == ParamMode::UserOwned {
                theme::ASH
            } else {
                theme::SMOKE
            },
        );
    }

    // Right-click = cycle mode; tooltip explains the states on hover.
    let response = response.on_hover_text(mode_tooltip(mode));
    let mode_cycled = response.secondary_clicked();
    (changed, mode_cycled)
}

fn draw_knob(painter: &Painter, rect: Rect, value: f32, mode: ParamMode, hovered: bool) {
    let center = rect.center();
    let radius = rect.width() * 0.45;

    // Background circle
    let bg = if hovered { theme::SLATE } else { theme::PIT };
    painter.circle_filled(center, radius, bg);
    let ring_col = match mode {
        ParamMode::UserOwned => theme::IRON,
        ParamMode::LlmFocus => mode_color(ParamMode::LlmFocus),
        ParamMode::Free => theme::ASH,
    };
    painter.circle_stroke(center, radius, Stroke::new(1.0, ring_col));

    // Arc track (270° sweep, starting bottom-left)
    let start_angle: f32 = std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4 * 3.0;
    let sweep = TAU * 0.75;
    let track_r = radius * 0.72;

    draw_arc(
        painter,
        center,
        track_r,
        start_angle,
        sweep,
        1.0,
        theme::SLATE,
    );

    let filled_sweep = sweep * value;
    let arc_color = match mode {
        ParamMode::UserOwned => theme::IRON,
        ParamMode::LlmFocus => mode_color(ParamMode::LlmFocus),
        ParamMode::Free => theme::FOG,
    };
    if filled_sweep > 0.01 {
        draw_arc(
            painter,
            center,
            track_r,
            start_angle,
            filled_sweep,
            2.0,
            arc_color,
        );
    }

    // Pointer dot
    let end_angle = start_angle + filled_sweep;
    let dot_pos = center + Vec2::new(end_angle.cos(), end_angle.sin()) * (radius * 0.58);
    painter.circle_filled(dot_pos, 2.5, arc_color);

    // Mode indicator — centre of knob, always rendered, colour signals state.
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        mode_char(mode),
        egui::FontId::monospace(8.0),
        mode_color(mode),
    );
}

fn draw_arc(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    start: f32,
    sweep: f32,
    width: f32,
    color: Color32,
) {
    let steps = ((sweep.abs() * radius * 2.0) as usize).clamp(8, 64);
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = start + sweep * i as f32 / steps as f32;
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect();
    for i in 0..points.len().saturating_sub(1) {
        painter.line_segment([points[i], points[i + 1]], Stroke::new(width, color));
    }
}

// ─── Horizontal Slider ───────────────────────────────────────────────────────

/// A labeled horizontal slider.
/// Returns `(value_changed, mode_cycled)`.
/// The mode button at the end cycles Free → UserOwned → LlmFocus → Free.
pub fn slider(ui: &mut Ui, label: &str, value: &mut f32, mode: ParamMode) -> (bool, bool) {
    let label_w = 72.0_f32;
    let mode_btn_w = 18.0_f32;
    let mut changed = false;
    let mut mode_cycled = false;

    ui.horizontal(|ui| {
        let text_color = if mode == ParamMode::UserOwned {
            theme::ASH
        } else {
            theme::SMOKE
        };
        ui.add_sized(
            [label_w, 14.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .monospace()
                    .size(9.0)
                    .color(text_color),
            ),
        );

        let avail = (ui.available_width() - mode_btn_w).max(40.0);

        if mode == ParamMode::UserOwned {
            // Show the value but disable editing
            let mut v = *value;
            ui.add_enabled(
                false,
                egui::Slider::new(&mut v, 0.0..=1.0).show_value(false),
            );
        } else {
            let resp = ui.add_sized(
                [avail, 14.0],
                egui::Slider::new(value, 0.0..=1.0).show_value(false),
            );
            if resp.changed() {
                changed = true;
            }
        }

        // Mode cycle button — character and colour reflect current state
        let btn = ui.add_sized(
            [mode_btn_w, 14.0],
            egui::Button::new(
                egui::RichText::new(mode_char(mode))
                    .monospace()
                    .size(9.0)
                    .color(mode_color(mode)),
            )
            .fill(egui::Color32::TRANSPARENT)
            .frame(false),
        );
        if btn.on_hover_text(mode_tooltip(mode)).clicked() {
            mode_cycled = true;
        }
    });

    (changed, mode_cycled)
}

/// Dispatch to `knob` or `slider` based on `use_sliders`.
/// Returns `(value_changed, mode_cycled)`.
pub fn param_control(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    mode: ParamMode,
    use_sliders: bool,
) -> (bool, bool) {
    if use_sliders {
        slider(ui, label, value, mode)
    } else {
        knob(ui, label, value, mode)
    }
}

// ─── Toggle Button ───────────────────────────────────────────────────────────

/// A stateful on/off button.  Flips `active` and returns true on click.
pub fn toggle_button(ui: &mut Ui, label: &str, active: &mut bool) -> bool {
    let fill = if *active { theme::IRON } else { theme::PIT };
    let text_color = if *active { theme::CHALK } else { theme::ASH };

    let button = egui::Button::new(
        egui::RichText::new(label)
            .color(text_color)
            .size(9.5)
            .monospace(),
    )
    .fill(fill)
    .stroke(Stroke::new(
        1.0,
        if *active { theme::ASH } else { theme::SLATE },
    ))
    .min_size(Vec2::new(36.0, 16.0));

    let resp = ui.add(button);
    if resp.clicked() {
        *active = !*active;
        return true;
    }
    false
}

// ─── Step Button ─────────────────────────────────────────────────────────────

/// A 16-step sequencer button.  Returns true when clicked (toggle request).
/// `vel` tints the fill when active (0 = dim, 1 = full bright).
pub fn step_button(
    ui: &mut Ui,
    active: bool,
    current: bool,
    vel: f32,
    color_override: Option<Color32>,
) -> bool {
    let size = Vec2::new(26.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        let fill = if active {
            let base = color_override.unwrap_or(theme::IRON);
            let dim = 0.4_f32 + vel * 0.6;
            Color32::from_rgb(
                (base.r() as f32 * dim) as u8,
                (base.g() as f32 * dim) as u8,
                (base.b() as f32 * dim) as u8,
            )
        } else {
            theme::PIT
        };

        let border = if current { theme::CHALK } else { theme::SLATE };

        painter.rect_filled(rect.shrink(1.0), 2.0, fill);
        painter.rect_stroke(rect.shrink(1.0), 2.0, Stroke::new(1.0, border));
    }

    response.clicked()
}

// ─── LED Indicator ────────────────────────────────────────────────────────────

pub fn led(ui: &mut Ui, active: bool) {
    let size = Vec2::new(8.0, 8.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let color = if active { theme::CHALK } else { theme::SLATE };
        ui.painter().circle_filled(rect.center(), 3.5, color);
    }
}

// ─── Section Header ───────────────────────────────────────────────────────────

pub fn section_header(ui: &mut Ui, label: &str) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(label)
            .monospace()
            .size(9.5)
            .color(theme::SMOKE),
    );
    ui.add_space(2.0);
}

// ─── XY Pad ──────────────────────────────────────────────────────────────────

/// An XY pad controlling two parameters simultaneously.
/// Returns true when a value changed.
/// `locked` follows the UserOwned convention — pad is read-only when true.
pub fn xy_pad(
    ui: &mut Ui,
    label_x: &str,
    label_y: &str,
    x: &mut f32,
    y: &mut f32,
    size: f32,
    locked: bool,
) -> bool {
    let label_h = 13.0_f32;
    let label_w = 12.0_f32;
    let total = Vec2::new(label_w + size + 2.0, size + label_h + 2.0);
    let (outer, response) = ui.allocate_exact_size(total, Sense::click_and_drag());

    let pad_rect = Rect::from_min_size(
        Pos2::new(outer.min.x + label_w, outer.min.y),
        Vec2::splat(size),
    );

    let mut changed = false;

    if !locked
        && (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        *x = ((pos.x - pad_rect.min.x) / pad_rect.width()).clamp(0.0, 1.0);
        *y = (1.0 - (pos.y - pad_rect.min.y) / pad_rect.height()).clamp(0.0, 1.0);
        changed = true;
    }

    if ui.is_rect_visible(outer) {
        let painter = ui.painter();

        let bg_col = if response.hovered() && !locked {
            theme::SLATE
        } else {
            theme::PIT
        };
        painter.rect_filled(pad_rect, egui::Rounding::same(2.0), bg_col);
        painter.rect_stroke(
            pad_rect,
            egui::Rounding::same(2.0),
            Stroke::new(1.0, if locked { theme::IRON } else { theme::ASH }),
        );

        let grid_col = theme::SLATE;
        for t in [0.25_f32, 0.5, 0.75] {
            let gx = pad_rect.min.x + pad_rect.width() * t;
            let gy = pad_rect.min.y + pad_rect.height() * t;
            painter.line_segment(
                [Pos2::new(gx, pad_rect.min.y), Pos2::new(gx, pad_rect.max.y)],
                Stroke::new(0.5, grid_col),
            );
            painter.line_segment(
                [Pos2::new(pad_rect.min.x, gy), Pos2::new(pad_rect.max.x, gy)],
                Stroke::new(0.5, grid_col),
            );
        }

        let cx = pad_rect.min.x + pad_rect.width() * x.clamp(0.0, 1.0);
        let cy = pad_rect.min.y + pad_rect.height() * (1.0 - y.clamp(0.0, 1.0));
        let guide_col = theme::IRON;
        painter.line_segment(
            [Pos2::new(cx, pad_rect.min.y), Pos2::new(cx, pad_rect.max.y)],
            Stroke::new(0.5, guide_col),
        );
        painter.line_segment(
            [Pos2::new(pad_rect.min.x, cy), Pos2::new(pad_rect.max.x, cy)],
            Stroke::new(0.5, guide_col),
        );

        let dot_col = if locked { theme::ASH } else { theme::CHALK };
        painter.circle_filled(Pos2::new(cx, cy), 4.5, dot_col);
        painter.circle_stroke(
            Pos2::new(cx, cy),
            4.5,
            Stroke::new(1.0, if locked { theme::IRON } else { theme::FOG }),
        );

        if locked {
            painter.text(
                pad_rect.center(),
                egui::Align2::CENTER_CENTER,
                "U",
                egui::FontId::monospace(8.0),
                theme::IRON,
            );
        }

        let x_label_rect = Rect::from_min_size(
            Pos2::new(pad_rect.min.x, pad_rect.max.y + 1.0),
            Vec2::new(pad_rect.width(), label_h),
        );
        let col = if locked { theme::IRON } else { theme::SMOKE };
        painter.text(
            x_label_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{} {:.2}", label_x, x),
            egui::FontId::monospace(8.0),
            col,
        );

        let y_label_center = Pos2::new(outer.min.x + label_w * 0.5, pad_rect.center().y);
        painter.text(
            y_label_center,
            egui::Align2::CENTER_CENTER,
            format!("{:.2}", y),
            egui::FontId::monospace(7.5),
            col,
        );
        painter.text(
            Pos2::new(outer.min.x + label_w * 0.5, pad_rect.min.y + 5.0),
            egui::Align2::CENTER_CENTER,
            label_y,
            egui::FontId::monospace(7.0),
            col,
        );
    }

    changed
}
