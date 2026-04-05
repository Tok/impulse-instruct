// ─── ui/module_card.rs ────────────────────────────────────────────────────────
// Wraps a panel draw function in a labelled, bordered module card.
//
// Each card:
//   • Title bar with module name, enable toggle, port indicators
//   • Content area that fills available width
//   • Port circles on the right edge of the title bar (audio out) and left edge
//     (audio in) — registered into the frame's port map for cable hit-testing.

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
        | ModuleKind::NoiseVoice => 24,
        ModuleKind::An1xVoice => 28,
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
        | ModuleKind::FxDrive => 20,
        ModuleKind::LfoModule => 18,
    };
    Color32::from_gray(v)
}

// ─── Port circle helpers ──────────────────────────────────────────────────────

pub const PORT_RADIUS: f32 = 5.5;
const PORT_HOLE: f32 = 2.2;

/// Draw a jack port circle and return its centre.
pub fn draw_port_circle(painter: &egui::Painter, center: Pos2, kind: PortKind, dir: PortDir) {
    // Outer ring — bright grey
    let ring_color = Color32::from_gray(100);
    painter.circle_filled(center, PORT_RADIUS + 1.5, Color32::from_gray(12));
    painter.circle_filled(center, PORT_RADIUS, ring_color);
    // Inner fill — direction-coded lightness
    let inner = match dir {
        PortDir::Out => Color32::from_gray(60),
        PortDir::In => Color32::from_gray(30),
    };
    painter.circle_filled(center, PORT_RADIUS - 1.5, inner);
    // Centre hole
    painter.circle_filled(center, PORT_HOLE, Color32::from_gray(8));
    // CV ports get a small bright dot in the hole
    if kind == PortKind::Cv {
        painter.circle_filled(center, 0.8, Color32::from_gray(180));
    }
}

// ─── Card draw ────────────────────────────────────────────────────────────────

pub struct CardResponse {
    /// Whether the enabled toggle was clicked.
    pub toggle_clicked: bool,
    /// Whether the remove button was clicked.
    pub remove_clicked: bool,
}

/// Draw a module card around `content`, registering port positions into `ports`.
/// Returns a `CardResponse`.
///
/// `module_id` — used to build `PortRef` values.
/// `kind`      — determines colour, title, and port configuration.
/// `enabled`   — if false, card content is dimmed.
/// `min_width` — minimum card width in pixels; `None` → fill available.
pub fn module_card<R>(
    ui: &mut egui::Ui,
    module_id: u32,
    kind: ModuleKind,
    enabled: bool,
    min_width: Option<f32>,
    ports: &mut Vec<PortPos>,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> (CardResponse, R) {
    let fill = Color32::from_gray(14);
    let title_bg = title_fill(kind);

    // If min_width requested, ensure the card is at least that wide.
    if let Some(min_w) = min_width {
        ui.set_min_width(min_w);
    }

    let available_w = ui.available_width();

    let frame = Frame::none()
        .fill(fill)
        .inner_margin(Margin::ZERO)
        .rounding(Rounding::same(4.0))
        .stroke(Stroke::new(1.0, Color32::from_gray(38)));

    let mut toggle_clicked = false;
    let mut remove_clicked = false;

    let inner = frame.show(ui, |ui| {
        ui.set_min_width(available_w.max(min_width.unwrap_or(0.0)));

        // ── Title bar ─────────────────────────────────────────────────────
        let title_h = 22.0;
        let (title_rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), title_h), Sense::hover());
        let painter = ui.painter_at(title_rect);

        // Background gradient: slightly lighter at top (simulated overhead light)
        painter.rect_filled(title_rect, Rounding::same(0.0), title_bg);
        // 1px specular line at very top
        painter.line_segment(
            [title_rect.left_top(), title_rect.right_top()],
            Stroke::new(1.0, Color32::from_gray(60)),
        );
        // 1px shadow at bottom
        painter.line_segment(
            [title_rect.left_bottom(), title_rect.right_bottom()],
            Stroke::new(1.0, Color32::from_gray(8)),
        );

        // Module kind label (embossed: shadow 1px below, then bright text)
        let label_pos = title_rect.left_center() + Vec2::new(10.0, 0.0);
        painter.text(
            label_pos + Vec2::new(0.0, 1.0),
            egui::Align2::LEFT_CENTER,
            kind.label(),
            egui::FontId::monospace(9.5),
            Color32::from_gray(8),
        );
        painter.text(
            label_pos,
            egui::Align2::LEFT_CENTER,
            kind.label(),
            egui::FontId::monospace(9.5),
            Color32::from_gray(if enabled { 200 } else { 80 }),
        );

        // ── Port circles on right side of title bar ───────────────────────
        // Audio out: rightmost
        let audio_out_x = title_rect.right() - 12.0;
        let audio_out_y = title_rect.center().y;
        let audio_out_pos = Pos2::new(audio_out_x, audio_out_y);
        draw_port_circle(&painter, audio_out_pos, PortKind::Audio, PortDir::Out);
        ports.push(PortPos {
            port: PortRef {
                module_id,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            center: audio_out_pos,
        });

        // Audio in: just left of audio out (visible only if this is an FX module)
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
                | ModuleKind::MasterOutput
        );
        if has_audio_in {
            let audio_in_x = audio_out_x - 18.0;
            let audio_in_pos = Pos2::new(audio_in_x, audio_out_y);
            draw_port_circle(&painter, audio_in_pos, PortKind::Audio, PortDir::In);
            ports.push(PortPos {
                port: PortRef {
                    module_id,
                    dir: PortDir::In,
                    kind: PortKind::Audio,
                    index: 0,
                },
                center: audio_in_pos,
            });
        }

        // CV out for LFO
        if kind == ModuleKind::LfoModule {
            let cv_out_x = audio_out_x - 18.0;
            let cv_out_pos = Pos2::new(cv_out_x, audio_out_y);
            draw_port_circle(&painter, cv_out_pos, PortKind::Cv, PortDir::Out);
            ports.push(PortPos {
                port: PortRef {
                    module_id,
                    dir: PortDir::Out,
                    kind: PortKind::Cv,
                    index: 0,
                },
                center: cv_out_pos,
            });
        }

        // ── Enable toggle (small square LED left of label) ────────────────
        let led_rect = Rect::from_center_size(
            Pos2::new(title_rect.left() + 4.0, title_rect.center().y),
            Vec2::splat(5.0),
        );
        let led_resp = ui.interact(led_rect, ui.id().with("led"), Sense::click());
        if led_resp.clicked() {
            toggle_clicked = true;
        }
        let led_color = if enabled {
            Color32::from_gray(220)
        } else {
            Color32::from_gray(32)
        };
        painter.rect_filled(led_rect, Rounding::same(1.0), led_color);

        // ── Remove button (×) on far right, before ports ──────────────────
        let rm_rect = Rect::from_center_size(
            Pos2::new(title_rect.right() - 44.0, title_rect.center().y),
            Vec2::splat(10.0),
        );
        // Only show for removable modules
        let is_core = matches!(kind, ModuleKind::StepSequencer | ModuleKind::MasterOutput);
        if !is_core {
            let rm_resp = ui.interact(rm_rect, ui.id().with("rm"), Sense::click());
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

        // ── Content ───────────────────────────────────────────────────────
        let content_frame = Frame::none()
            .fill(fill)
            .inner_margin(Margin::symmetric(4.0, 4.0));
        let inner_resp = content_frame.show(ui, |ui| {
            if enabled {
                content(ui)
            } else {
                ui.add_enabled_ui(false, |ui| content(ui)).inner
            }
        });
        inner_resp.inner
    });

    (
        CardResponse {
            toggle_clicked,
            remove_clicked,
        },
        inner.inner,
    )
}

// ─── Zone rail ────────────────────────────────────────────────────────────────

/// Draw a horizontal zone rail separator with a label and optional [+ Add] button.
/// Returns true if the add button was clicked.
pub fn zone_rail(ui: &mut egui::Ui, label: &str, show_add: bool) -> bool {
    let mut add_clicked = false;
    let (rail_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let painter = ui.painter_at(rail_rect);

    // Rail background
    painter.rect_filled(rail_rect, Rounding::ZERO, Color32::from_gray(18));
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
    // Zone label
    painter.text(
        Pos2::new(rail_rect.left() + 24.0, screw_y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::monospace(8.5),
        Color32::from_gray(70),
    );

    // [+ ADD] button on the right
    if show_add {
        let btn_rect = Rect::from_center_size(
            Pos2::new(rail_rect.right() - 30.0, screw_y),
            Vec2::new(44.0, 13.0),
        );
        let btn_resp = ui.interact(btn_rect, ui.id().with("add"), Sense::click());
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

    add_clicked
}
