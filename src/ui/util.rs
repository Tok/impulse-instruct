// ─── ui/util.rs ──────────────────────────────────────────────────────────────
// Small UI utilities: model scanning, browser open.

/// Scan for .gguf model files. Checks:
///   1. models/ relative to CWD (dev: cargo run from repo root)
///   2. models/ next to the binary (dist: user unpacked the release)
///   3. The repo root itself (for convenience when a .gguf is dropped there)
pub(crate) fn scan_models() -> Vec<String> {
    let mut dirs: Vec<std::path::PathBuf> = vec![std::path::PathBuf::from("models")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        let sibling = parent.join("models");
        if sibling != std::path::Path::new("models") {
            dirs.push(sibling);
        }
    }
    let mut found: Vec<String> = dirs
        .into_iter()
        .flat_map(|dir| {
            std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "gguf").unwrap_or(false))
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

use crate::state::ModuleKind;

/// Result of context-sensitive Ctrl+MW zoom detection.
pub(crate) enum ZoomTarget {
    /// Per-kind zoom: (kind, new_scale).  All modules of this kind scale together.
    Kind(ModuleKind, f32),
    /// Global UI zoom.
    Global(f32),
}

/// Convert a raw scroll-wheel delta to a unit-scale zoom step.  One
/// notch on a typical mouse wheel reports ±120 in `raw_scroll_delta`;
/// we map that to ±0.1 of zoom-scale and clamp the per-tick magnitude
/// at 0.05 (= half a notch) so a runaway / boosted wheel can't flip
/// the scale by half a unit in one frame.  Pure helper so the
/// damping curve is unit-testable.
pub(crate) fn zoom_step_from_delta(delta: f32) -> f32 {
    (delta / 120.0).clamp(-0.5, 0.5) * 0.1
}

/// Apply `step` to a per-module zoom scale and clamp to the
/// allowed range (0.5×..2.0×).  Per-module zoom is a finer band
/// than the global zoom because it stacks on top of the global
/// scale at render time.
pub(crate) fn next_module_scale(current: f32, step: f32) -> f32 {
    (current + step).clamp(0.5, 2.0)
}

/// Apply `step` to the global zoom scale and clamp to the allowed
/// range (0.5×..3.0×).  Global zoom can go higher than per-module
/// because it's the only knob that affects chrome (text, log, etc.).
pub(crate) fn next_global_scale(current: f32, step: f32) -> f32 {
    (current + step).clamp(0.5, 3.0)
}

/// Unsmoothed mouse-wheel delta for this frame, in points — what egui
/// ≤ 0.28 exposed as `InputState::raw_scroll_delta` (same unit
/// conversion and shift→horizontal folding).  Newer egui only exposes
/// `smooth_scroll_delta`, which also swallows Ctrl+wheel into
/// `zoom_delta`, so the Ctrl-zoom and wheel-boost paths need this.
pub(crate) fn raw_wheel_delta(ctx: &egui::Context) -> egui::Vec2 {
    let line_speed = ctx.options(|o| o.input_options.line_scroll_speed);
    ctx.input(|i| {
        let page_h = i.viewport_rect().height();
        i.events.iter().fold(egui::Vec2::ZERO, |acc, e| match e {
            egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } => acc + wheel_event_points(*unit, *delta, modifiers.shift, line_speed, page_h),
            _ => acc,
        })
    })
}

/// Convert one wheel event to points.  Pure; split out of
/// `raw_wheel_delta` so the unit math is testable without a Context.
pub(crate) fn wheel_event_points(
    unit: egui::MouseWheelUnit,
    delta: egui::Vec2,
    shift: bool,
    line_speed: f32,
    page_h: f32,
) -> egui::Vec2 {
    let d = match unit {
        egui::MouseWheelUnit::Point => delta,
        egui::MouseWheelUnit::Line => delta * line_speed,
        egui::MouseWheelUnit::Page => delta * page_h,
    };
    if shift { egui::vec2(d.x + d.y, 0.0) } else { d }
}

/// Temp-memory key for the rack CentralPanel's rect (published each
/// frame by `App::ui`).  Replaces egui ≤ 0.28's `Context::available_rect`,
/// which reported the space left after top/bottom panels.
fn central_rect_id() -> egui::Id {
    egui::Id::unique("central_panel_rect")
}

pub(crate) fn publish_central_rect(ctx: &egui::Context, rect: egui::Rect) {
    ctx.data_mut(|d| d.insert_temp(central_rect_id(), rect));
}

/// The rack CentralPanel's rect from this frame (or the full content
/// rect before the panel has been laid out).
pub(crate) fn central_rect(ctx: &egui::Context) -> egui::Rect {
    ctx.data(|d| d.get_temp(central_rect_id()))
        .unwrap_or_else(|| ctx.content_rect())
}

/// Detect Ctrl+MW zoom and return the appropriate target.
/// Card rects (kind + rect) are read from egui temp memory (set during rack render).
pub(crate) fn detect_ctrl_zoom(
    ctx: &egui::Context,
    kind_scales: &std::collections::HashMap<ModuleKind, f32>,
    current_global: f32,
) -> Option<ZoomTarget> {
    let ctrl_locked = ctx
        .data(|d| d.get_temp::<bool>(egui::Id::unique("ctrl_locked")))
        .unwrap_or(false);
    let ctrl = ctx.input(|i| i.modifiers.ctrl);
    let delta = if ctrl || ctrl_locked {
        raw_wheel_delta(ctx).y
    } else {
        0.0
    };
    if delta.abs() <= 0.1 {
        return None;
    }
    let step = zoom_step_from_delta(delta);
    let card_rects: Vec<(ModuleKind, egui::Rect)> = ctx
        .memory(|m| m.data.get_temp(egui::Id::unique("module_card_rects")))
        .unwrap_or_default();
    let hovered = ctx.pointer_latest_pos().and_then(|p| {
        card_rects
            .iter()
            .find(|(_, r)| r.contains(p))
            .map(|&(k, _)| k)
    });
    if let Some(kind) = hovered {
        let cur = kind_scales.get(&kind).copied().unwrap_or(1.0);
        Some(ZoomTarget::Kind(kind, next_module_scale(cur, step)))
    } else {
        Some(ZoomTarget::Global(next_global_scale(current_global, step)))
    }
}

/// Open a URL in the system browser (cross-platform, no extra dep).
pub(crate) fn webbrowser_open(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(url).spawn()?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn()?;
    Ok(())
}

/// Automatic generation waits for the rack choice and startup initialization.
/// Model loading itself may proceed while the setup window is open.
pub(crate) fn background_inference_ready(
    show_wizard: bool,
    startup_done: bool,
    llm_initializing: bool,
) -> bool {
    !show_wizard && startup_done && !llm_initializing
}
