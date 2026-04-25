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

/// Detect Ctrl+MW zoom and return the appropriate target.
/// Card rects (kind + rect) are read from egui temp memory (set during rack render).
pub(crate) fn detect_ctrl_zoom(
    ctx: &egui::Context,
    kind_scales: &std::collections::HashMap<ModuleKind, f32>,
    current_global: f32,
) -> Option<ZoomTarget> {
    let ctrl_locked = ctx
        .data(|d| d.get_temp::<bool>(egui::Id::new("ctrl_locked")))
        .unwrap_or(false);
    let delta = ctx.input(|i| {
        if i.modifiers.ctrl || ctrl_locked {
            i.raw_scroll_delta.y
        } else {
            0.0
        }
    });
    if delta.abs() <= 0.1 {
        return None;
    }
    let step = zoom_step_from_delta(delta);
    let card_rects: Vec<(ModuleKind, egui::Rect)> = ctx
        .memory(|m| m.data.get_temp(egui::Id::new("module_card_rects")))
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
