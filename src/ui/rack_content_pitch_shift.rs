// ─── ui/rack_content_pitch_shift.rs ──────────────────────────────────────────
// PitchShift card — 3-knob header (SHIFT / MIX / FBK) + FINE cents
// knob on a second row, with the standard 3-knob XY pad for SHIFT /
// MIX / FBK.  Extracted from rack_content.rs so that file stays under
// the 1000-line cap.

use crate::state::{ModuleKind, ParamMode, param_mode};
use crate::ui::ImpulseApp;
use crate::ui::rack_content_pad::{PAD_SECTION_TOP_GAP, render_three_pad};
use crate::ui::widgets;

/// Render the PitchShift card.  The SHIFT and FINE knobs map their
/// bipolar stored values (±24 st, ±100 cents) onto 0..1 so they can
/// reuse the standard knob widget without a bipolar variant; 0.5 on
/// the knob is the zero-offset detent.
pub(super) fn draw_pitch_shift(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    module_id: u32,
    pad_expanded: bool,
    pad_pair: &mut u8,
) -> bool {
    let _ = ModuleKind::FxPitchShift;
    let scale: f32 = ui
        .ctx()
        .data(|d| d.get_temp(egui::Id::new("module_scale")))
        .unwrap_or(1.0);
    let ctrl = widgets::ControlPrefs::from_prefs_scaled(&app.state.read().ui_prefs, scale);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| param_mode(path, &locked, &focused);
    let user_owned = |path: &str| matches!(pm(path), ParamMode::UserOwned);

    let mut changed = false;
    let (mut semi, mut fine, mut mix, mut fbk) = {
        let s = app.state.read();
        (
            (s.fx.pitch_shift_semi / 48.0) + 0.5,
            (s.fx.pitch_shift_fine / 200.0) + 0.5,
            s.fx.pitch_shift_mix,
            s.fx.pitch_shift_fbk,
        )
    };
    widgets::centered_row(ui, |ui| {
        if widgets::param_control(ui, "SHIFT", &mut semi, pm("fx.pitch_shift_semi"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "MIX", &mut mix, pm("fx.pitch_shift_mix"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "FBK", &mut fbk, pm("fx.pitch_shift_fbk"), ctrl).0 {
            changed = true;
        }
    });
    widgets::centered_row(ui, |ui| {
        if widgets::param_control(ui, "FINE", &mut fine, pm("fx.pitch_shift_fine"), ctrl).0 {
            changed = true;
        }
    });

    if pad_expanded {
        ui.add_space(PAD_SECTION_TOP_GAP);
        let (vc, _) = render_three_pad(
            ui,
            &format!("pitch_shift_xy_{module_id}"),
            ["SHIFT", "MIX", "FBK"],
            pad_pair,
            (&mut semi, &mut mix, &mut fbk),
            [
                user_owned("fx.pitch_shift_semi"),
                user_owned("fx.pitch_shift_mix"),
                user_owned("fx.pitch_shift_fbk"),
            ],
        );
        if vc {
            changed = true;
        }
    }

    if changed {
        let mut s = app.state.write();
        s.fx.pitch_shift_semi = (semi - 0.5) * 48.0;
        s.fx.pitch_shift_fine = (fine - 0.5) * 200.0;
        s.fx.pitch_shift_mix = mix;
        s.fx.pitch_shift_fbk = fbk;
    }
    changed
}
