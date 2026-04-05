// ─── ui/widgets/mod.rs ───────────────────────────────────────────────────────
#![allow(dead_code)] // widget library grows alongside panels
// Custom widgets: rotary knob, step button, LED indicator.

pub mod emboss;
#[allow(unused_imports)]
pub use emboss::button_emboss;

pub mod step;
pub use step::{huth_note_cell, step_button};

use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::f32::consts::TAU;

use super::theme;
use crate::state::{KnobStyle, ParamMode, UiPrefs};

// ─── Control preferences ──────────────────────────────────────────────────────

/// Combined rendering mode derived from `UiPrefs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlStyle {
    KnobFlat,
    KnobChrome,
    Sliders,
}

/// Full control rendering preferences — style + size.
/// Pass this to `param_control`; derive it once per panel via `ControlPrefs::from_prefs`.
#[derive(Clone, Copy, Debug)]
pub struct ControlPrefs {
    pub style: ControlStyle,
    /// Knob body size in pixels (from `KnobSize::body_px()`).
    pub knob_size: f32,
}

impl ControlPrefs {
    pub fn from_prefs(prefs: &UiPrefs) -> Self {
        let style = if prefs.use_sliders {
            ControlStyle::Sliders
        } else if prefs.knob_style == KnobStyle::Chrome {
            ControlStyle::KnobChrome
        } else {
            ControlStyle::KnobFlat
        };
        Self {
            style,
            knob_size: prefs.knob_size.body_px(),
        }
    }

    /// Suggested max-width for a glass panel group containing controls of this style.
    /// Used by panels to constrain group widths so `horizontal_wrapped` can flow them.
    pub fn group_max_width(self) -> f32 {
        match self.style {
            ControlStyle::KnobFlat | ControlStyle::KnobChrome => {
                // Single column of knobs: body + label margins
                self.knob_size + 24.0
            }
            ControlStyle::Sliders => {
                // label (72) + slider track (130) + mode btn (18) + inner margin (12)
                232.0
            }
        }
    }
}

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
            "U User owned — LLM ignores this param, you still control it\nRight-click to set LLM focus (F)"
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
pub fn knob(ui: &mut Ui, label: &str, value: &mut f32, mode: ParamMode, size: f32) -> (bool, bool) {
    let label_h = (size * 0.28).max(14.0).round();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(size, size + label_h), Sense::click_and_drag());

    let knob_center = Pos2::new(rect.center().x, rect.min.y + size * 0.5);
    let knob_rect = Rect::from_center_size(knob_center, Vec2::splat(size));
    let label_rect = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.min.y + size + 1.0),
        Vec2::new(rect.width(), label_h),
    );
    let label_font_size = (size * 0.175).clamp(8.0, 13.0);

    let mut changed = false;

    if response.dragged() {
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
            egui::FontId::monospace(label_font_size),
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

    // Well shadow — dark drop shadow offset slightly down-right
    painter.circle_filled(
        center + Vec2::new(0.5, 1.0),
        radius + 1.5,
        Color32::from_gray(8),
    );

    // Body
    let bg = if hovered { theme::SLATE } else { theme::PIT };
    painter.circle_filled(center, radius, bg);

    // Catch-light: short bright line toward top-left, simulates overhead lamp
    let cl_angle = std::f32::consts::PI * 1.25; // ~10 o'clock
    let cl_a = center + Vec2::new(cl_angle.cos(), cl_angle.sin()) * (radius * 0.25);
    let cl_b = center + Vec2::new(cl_angle.cos(), cl_angle.sin()) * (radius * 0.60);
    painter.line_segment([cl_a, cl_b], Stroke::new(1.5, Color32::from_gray(100)));

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

    // Pointer line from near-centre to near-rim (replaces dot — clearer read)
    let end_angle = start_angle + filled_sweep;
    let ptr_a = center + Vec2::new(end_angle.cos(), end_angle.sin()) * (radius * 0.18);
    let ptr_b = center + Vec2::new(end_angle.cos(), end_angle.sin()) * (radius * 0.75);
    painter.line_segment(
        [ptr_a + Vec2::new(0.5, 0.5), ptr_b + Vec2::new(0.5, 0.5)],
        Stroke::new(1.0, Color32::from_gray(8)),
    );
    painter.line_segment([ptr_a, ptr_b], Stroke::new(1.5, arc_color));

    // Mode indicator — centre dot, colour signals state
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
    let steps = ((sweep.abs() * radius * 2.0) as usize).clamp(12, 96);
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = start + sweep * i as f32 / steps as f32;
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect();
    // Single polyline avoids seam artifacts at joints (vs individual line_segments)
    painter.add(egui::Shape::line(points, Stroke::new(width, color)));
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

/// Dispatch to the appropriate control widget based on `ControlPrefs`.
/// Returns `(value_changed, mode_cycled)`.
pub fn param_control(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    mode: ParamMode,
    prefs: ControlPrefs,
) -> (bool, bool) {
    match prefs.style {
        ControlStyle::Sliders => slider_glass(ui, label, value, mode),
        ControlStyle::KnobFlat => knob(ui, label, value, mode, prefs.knob_size),
        ControlStyle::KnobChrome => knob_chrome(ui, label, value, mode, prefs.knob_size),
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
    let resp = ui.label(
        egui::RichText::new(label)
            .monospace()
            .size(9.5)
            .color(theme::SMOKE),
    );
    // 1px rule line below — sub-panel separator cue
    let rule_y = resp.rect.max.y + 1.0;
    let rule_x0 = resp.rect.min.x;
    let rule_x1 = ui.max_rect().max.x;
    ui.painter().line_segment(
        [Pos2::new(rule_x0, rule_y), Pos2::new(rule_x1, rule_y)],
        Stroke::new(1.0, theme::SLATE),
    );
    ui.add_space(2.0);
}

// ─── XY Pad ──────────────────────────────────────────────────────────────────

/// An XY pad controlling two parameters simultaneously.
/// Returns true when a value changed.
/// `locked` follows the UserOwned convention — pad is read-only when true.
/// Return the currently selected pair index for an XY pad identified by `pad_id`.
/// Call this *before* `xy_pad` to dispatch the correct `&mut f32` values on every frame.
/// Return the currently selected pair index for an XY pad with the given `pad_id`.
/// Call this *before* `xy_pad` on each frame to dispatch the correct `&mut f32` values.
pub fn xy_pad_pair(ctx: &egui::Context, pad_id: &str) -> usize {
    ctx.data(|d| d.get_temp(egui::Id::new(pad_id)).unwrap_or(0usize))
}

/// XY pad with optional pair cycling.
///
/// - `num_pairs` — how many param pairs this pad cycles through (1 = fixed, no cycle indicator).
///   The active pair index is persisted in egui temp-memory keyed by the widget's allocated id.
///   Right-click cycles to the next pair.
///
/// Returns `(changed, pair_index)`.  When `changed` is true the caller should write back
/// the new `x`/`y` values.  The caller should use `pair_index` on every frame to decide
/// which param pair to pass in as `x`/`y`.
pub fn xy_pad(
    ui: &mut Ui,
    pad_id: &str,
    label_x: &str,
    label_y: &str,
    x: &mut f32,
    y: &mut f32,
    size: f32,
    locked: bool,
    num_pairs: usize,
) -> (bool, usize) {
    let label_h = 13.0_f32;
    let label_w = 12.0_f32;
    let total = Vec2::new(label_w + size + 2.0, size + label_h + 2.0);
    let (outer, response) = ui.allocate_exact_size(total, Sense::click_and_drag());

    let pad_rect = Rect::from_min_size(
        Pos2::new(outer.min.x + label_w, outer.min.y),
        Vec2::splat(size),
    );

    // ── Pair cycling (right-click) ────────────────────────────────────────────
    let mem_id = egui::Id::new(pad_id);
    let pair: usize = ui.ctx().data(|d| d.get_temp(mem_id).unwrap_or(0usize));
    if num_pairs > 1 && response.secondary_clicked() {
        let next = (pair + 1) % num_pairs;
        ui.ctx().data_mut(|d| d.insert_temp(mem_id, next));
    }

    let mut changed = false;

    if !locked
        && (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        *x = ((pos.x - pad_rect.min.x) / pad_rect.width()).clamp(0.0, 1.0);
        *y = (1.0 - (pos.y - pad_rect.min.y) / pad_rect.height()).clamp(0.0, 1.0);
        changed = true;
    }

    // ── Hover tooltip (shown inline after all drawing) ────────────────────────
    let show_tooltip = response.hovered();
    let tooltip_text = format!("{}: {:.3}   {}: {:.3}", label_x, *x, label_y, *y);

    if ui.is_rect_visible(outer) {
        let painter = ui.painter();

        // Glass panel background — recessed dark fill, bright top edge
        painter.rect_filled(pad_rect, egui::Rounding::same(3.0), Color32::from_gray(12));
        painter.rect_stroke(
            pad_rect,
            egui::Rounding::same(3.0),
            Stroke::new(1.0, Color32::from_gray(if locked { 28 } else { 40 })),
        );
        if response.hovered() && !locked {
            painter.rect_stroke(
                pad_rect,
                egui::Rounding::same(3.0),
                Stroke::new(1.0, Color32::from_gray(70)),
            );
        }
        // Bright top edge — glass surface sheen
        painter.line_segment(
            [
                pad_rect.left_top() + Vec2::new(2.0, 0.0),
                pad_rect.right_top() - Vec2::new(2.0, 0.0),
            ],
            Stroke::new(1.0, Color32::from_gray(55)),
        );

        // Grid crosshairs
        let grid_col = Color32::from_gray(22);
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
        // Guide lines (dim crosshair through cursor)
        painter.line_segment(
            [Pos2::new(cx, pad_rect.min.y), Pos2::new(cx, pad_rect.max.y)],
            Stroke::new(0.5, Color32::from_gray(35)),
        );
        painter.line_segment(
            [Pos2::new(pad_rect.min.x, cy), Pos2::new(pad_rect.max.x, cy)],
            Stroke::new(0.5, Color32::from_gray(35)),
        );

        // Chrome dome cursor — same language as the slider thumb
        let dot_r = (size * 0.065).max(4.0);
        let dot_pos = Pos2::new(cx, cy);
        if !locked {
            // Shadow
            painter.circle_filled(
                dot_pos + Vec2::new(0.5, 0.5),
                dot_r + 0.5,
                Color32::from_gray(6),
            );
            // Body
            painter.circle_filled(dot_pos, dot_r, Color32::from_gray(55));
            painter.circle_filled(dot_pos, dot_r * 0.6, Color32::from_gray(32));
            // Specular rim
            painter.circle_stroke(dot_pos, dot_r, Stroke::new(1.0, Color32::from_gray(140)));
        } else {
            painter.circle_filled(dot_pos, dot_r, Color32::from_gray(40));
            painter.circle_stroke(dot_pos, dot_r, Stroke::new(1.0, Color32::from_gray(55)));
        }

        if locked {
            painter.text(
                pad_rect.center(),
                egui::Align2::CENTER_CENTER,
                "U",
                egui::FontId::monospace(8.0),
                theme::IRON,
            );
        }

        // Pair-cycle indicator — small "⇄ n/N" in top-right corner when multiple pairs.
        if num_pairs > 1 {
            let ind_text = format!("{}/{}", pair + 1, num_pairs);
            painter.text(
                pad_rect.right_top() + Vec2::new(-2.0, 3.0),
                egui::Align2::RIGHT_TOP,
                ind_text,
                egui::FontId::monospace(6.5),
                if response.hovered() {
                    Color32::from_gray(110)
                } else {
                    Color32::from_gray(55)
                },
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

    if show_tooltip {
        response.on_hover_ui(|ui| {
            ui.label(egui::RichText::new(&tooltip_text).monospace().size(9.5));
        });
    }

    (changed, pair)
}

// ─── ADSR Envelope Visualiser ────────────────────────────────────────────────
mod adsr;
pub use adsr::{adsr_display, decay_display};

// ─── Glass Group ─────────────────────────────────────────────────────────────

/// Neumorphic smoked-glass group panel — dark fill, bright top edge, dark bottom edge.
///
/// `max_width` constrains the group so it sizes to content rather than expanding to
/// fill the row when used inside `horizontal_wrapped`.  Pass `ctrl.group_max_width()`
/// from the panel for the correct width based on the current control style.
pub fn glass_group<R>(
    ui: &mut Ui,
    max_width: f32,
    content: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let resp = egui::Frame::none()
        .fill(Color32::from_gray(15))
        .stroke(Stroke::new(1.0, Color32::from_gray(28)))
        .inner_margin(egui::Margin::same(6.0))
        .rounding(egui::Rounding::same(3.0))
        .show(ui, |ui| {
            // Force vertical stacking regardless of parent layout, and cap the width
            // so groups don't expand to fill the entire horizontal_wrapped row.
            ui.set_max_width(max_width);
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), content)
                .inner
        });

    // Overlay asymmetric edge highlights (smoked-glass edge illusion)
    let rect = resp.response.rect;
    let painter = ui.painter();
    painter.line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, Color32::from_gray(64)),
    );
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(8)),
    );
    resp
}

// ─── Glass Slider ────────────────────────────────────────────────────────────

/// Frosted-glass horizontal slider — recessed track, brighter filled segment,
/// 1px specular top edge, chrome-pill thumb.
/// Returns `(value_changed, mode_cycled)`.
pub fn slider_glass(ui: &mut Ui, label: &str, value: &mut f32, mode: ParamMode) -> (bool, bool) {
    let label_w = 72.0_f32;
    let mode_btn_w = 18.0_f32;
    let track_h = 10.0_f32;
    let _thumb_w = 8.0_f32;
    let mut changed = false;
    let mut mode_cycled = false;

    ui.horizontal(|ui| {
        let text_color = if mode == ParamMode::UserOwned {
            theme::ASH
        } else {
            theme::SMOKE
        };
        ui.add_sized(
            [label_w, track_h + 4.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .monospace()
                    .size(9.0)
                    .color(text_color),
            ),
        );

        let avail = (ui.available_width() - mode_btn_w).max(40.0);
        let (track_rect, response) =
            ui.allocate_exact_size(Vec2::new(avail, track_h + 4.0), Sense::click_and_drag());

        let track = Rect::from_min_size(
            Pos2::new(track_rect.min.x, track_rect.center().y - track_h * 0.5),
            Vec2::new(track_rect.width(), track_h),
        );

        if (response.dragged() || response.clicked())
            && let Some(pos) = response.interact_pointer_pos()
        {
            *value = ((pos.x - track.min.x) / track.width()).clamp(0.0, 1.0);
            changed = true;
        }

        if ui.is_rect_visible(track_rect) {
            let painter = ui.painter();
            let rounding = egui::Rounding::same(2.0);

            // Recessed track background
            painter.rect_filled(track, rounding, Color32::from_gray(12));
            painter.rect_stroke(track, rounding, Stroke::new(1.0, Color32::from_gray(8)));

            // Filled segment from left to thumb
            let fill_w = track.width() * value.clamp(0.0, 1.0);
            if fill_w > 1.0 {
                let fill_rect = Rect::from_min_size(track.min, Vec2::new(fill_w, track.height()));
                let fill_col = match mode {
                    ParamMode::UserOwned => Color32::from_gray(45),
                    ParamMode::LlmFocus => Color32::from_gray(180),
                    ParamMode::Free => Color32::from_gray(90),
                };
                painter.rect_filled(fill_rect, rounding, fill_col);
                // 1px specular top edge on fill
                painter.line_segment(
                    [fill_rect.left_top(), fill_rect.right_top()],
                    Stroke::new(1.0, Color32::from_gray(140)),
                );
            }

            // Round chrome thumb
            let thumb_cx = track.min.x + track.width() * value.clamp(0.0, 1.0);
            let thumb_r = track.height() * 0.70;
            let thumb_center = Pos2::new(thumb_cx, track.center().y);
            // Shadow
            painter.circle_filled(
                thumb_center + Vec2::new(0.5, 0.5),
                thumb_r + 0.5,
                Color32::from_gray(8),
            );
            // Body: concentric fills for a dome look
            painter.circle_filled(thumb_center, thumb_r, Color32::from_gray(55));
            painter.circle_filled(thumb_center, thumb_r * 0.70, Color32::from_gray(35));
            // Top specular
            painter.circle_stroke(
                thumb_center,
                thumb_r,
                Stroke::new(1.0, Color32::from_gray(140)),
            );
        }

        // Mode cycle button
        let btn = ui.add_sized(
            [mode_btn_w, track_h + 4.0],
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

// ─── Chrome Knob ─────────────────────────────────────────────────────────────

/// Neumorphic chrome rotary knob — concentric ring face, raised tick, value arc.
/// Same interaction contract as `knob`: left-drag changes value, right-click cycles mode.
pub fn knob_chrome(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    mode: ParamMode,
    size: f32,
) -> (bool, bool) {
    let label_h = (size * 0.28).max(14.0).round();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(size, size + label_h), Sense::click_and_drag());

    let knob_center = Pos2::new(rect.center().x, rect.min.y + size * 0.5);
    let knob_rect = Rect::from_center_size(knob_center, Vec2::splat(size));
    let label_rect = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.min.y + size + 1.0),
        Vec2::new(rect.width(), label_h),
    );
    let label_font_size = (size * 0.175).clamp(8.0, 13.0);

    let mut changed = false;
    if response.dragged() {
        let delta = response.drag_delta();
        *value = (*value - delta.y * 0.005 + delta.x * 0.003).clamp(0.0, 1.0);
        changed = true;
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        draw_knob_chrome(painter, knob_rect, *value, mode, response.hovered());
        let label_col = if mode == ParamMode::UserOwned {
            theme::ASH
        } else {
            theme::SMOKE
        };
        painter.text(
            label_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace(label_font_size),
            label_col,
        );
    }

    let response = response.on_hover_text(mode_tooltip(mode));
    let mode_cycled = response.secondary_clicked();
    (changed, mode_cycled)
}

fn draw_knob_chrome(painter: &Painter, rect: Rect, value: f32, mode: ParamMode, hovered: bool) {
    let center = rect.center();
    // Smaller body leaves room for value arc + scale marks outside
    let radius = rect.width() * 0.38;

    let start_angle: f32 = std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4 * 3.0;
    let sweep = TAU * 0.75;
    let filled_sweep = sweep * value;

    // ── Scale marks — drawn FIRST so body circle occludes their inner ends ────
    // 5 major (at 0%, 25%, 50%, 75%, 100%) + 8 minor (2 between each major pair)
    // Total 13 marks; minor marks are at every 1/12 step except the 5 major positions.
    for i in 0..=12usize {
        let t = i as f32 / 12.0;
        let is_major = i % 3 == 0; // 0, 3, 6, 9, 12 → the 5 major positions
        let angle = start_angle + sweep * t;
        let (inner_r, outer_r, col, width) = if is_major {
            (radius * 1.10, radius * 1.28, 95u8, 1.5_f32)
        } else {
            (radius * 1.12, radius * 1.21, 55u8, 1.0_f32)
        };
        let p_inner = center + Vec2::new(angle.cos(), angle.sin()) * inner_r;
        let p_outer = center + Vec2::new(angle.cos(), angle.sin()) * outer_r;
        if is_major {
            // Shadow pass for raised look
            let off = Vec2::new(0.4, 0.4);
            painter.line_segment(
                [p_inner + off, p_outer + off],
                Stroke::new(1.0, Color32::from_gray(8)),
            );
        }
        painter.line_segment(
            [p_inner, p_outer],
            Stroke::new(width, Color32::from_gray(col)),
        );
    }

    // ── Chrome body ───────────────────────────────────────────────────────────
    // Outer shadow ring
    painter.circle_stroke(
        center,
        radius + 1.0,
        Stroke::new(1.0, Color32::from_gray(8)),
    );

    // Concentric fills: dark centre → slightly lighter edge (machined dome illusion)
    let body_rings: &[(f32, u8)] = &[
        (1.00, 50), // outer body
        (0.84, 38),
        (0.68, 26),
        (0.50, 16), // dark centre well
    ];
    for &(frac, grey) in body_rings {
        painter.circle_filled(center, radius * frac, Color32::from_gray(grey));
    }

    // Micro bevel groove — 1px dark ring just inside the rim.
    // This is the single biggest "machined metal" cue: bright rim → dark groove → grey body.
    painter.circle_stroke(
        center,
        radius * 0.92,
        Stroke::new(1.0, Color32::from_gray(14)),
    );

    // Bright outer rim — the "polished edge" (brightens slightly on hover)
    let rim_col = if hovered { 180u8 } else { 120u8 };
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0, Color32::from_gray(rim_col)),
    );

    // Short glint arc on the rim at ~10 o'clock — surface sheen, clearly not a pointer
    {
        let glint_start = std::f32::consts::PI * 1.35; // ~10 o'clock
        let glint_sweep = std::f32::consts::FRAC_PI_4 * 0.55; // ~25°
        draw_arc(
            painter,
            center,
            radius,
            glint_start,
            glint_sweep,
            2.5,
            Color32::from_gray(210),
        );
    }

    // ── Value arc ring (outside body, inside scale marks) ─────────────────────
    let arc_r = radius * 1.055;
    draw_arc(
        painter,
        center,
        arc_r,
        start_angle,
        sweep,
        2.0,
        Color32::from_gray(16),
    );

    let arc_color = match mode {
        ParamMode::UserOwned => Color32::from_gray(70),
        ParamMode::LlmFocus => theme::CHALK,
        ParamMode::Free => Color32::from_gray(180),
    };
    if filled_sweep > 0.01 {
        draw_arc(
            painter,
            center,
            arc_r,
            start_angle,
            filled_sweep,
            2.0,
            arc_color,
        );
    }

    // ── Pointer line from near-centre to near-rim — unambiguous ───────────────
    let ptr_angle = start_angle + filled_sweep;
    let ptr_a = center + Vec2::new(ptr_angle.cos(), ptr_angle.sin()) * (radius * 0.15);
    let ptr_b = center + Vec2::new(ptr_angle.cos(), ptr_angle.sin()) * (radius * 0.80);
    // Dark shadow
    let shadow = Vec2::new(0.5, 0.5);
    painter.line_segment(
        [ptr_a + shadow, ptr_b + shadow],
        Stroke::new(1.0, Color32::from_gray(8)),
    );
    // Bright line
    painter.line_segment([ptr_a, ptr_b], Stroke::new(2.0, Color32::from_gray(220)));

    // ── Mode indicator ────────────────────────────────────────────────────────
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        mode_char(mode),
        egui::FontId::monospace(7.0),
        match mode {
            ParamMode::Free => Color32::from_gray(42),
            ParamMode::UserOwned => Color32::from_gray(72),
            ParamMode::LlmFocus => Color32::from_gray(185),
        },
    );
}
