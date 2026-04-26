// ─── ui/rack_content_conv_reverb.rs ──────────────────────────────────────────
// ConvReverb card — 6 knobs (MIX / SIZE / PREDELAY / DAMP / LOWCUT /
// WIDTH) in a glass-grouped 2-row bank, plus an IR-picker row (REV
// toggle + LOAD IR + × clear + filename).  Extracted from
// rack_content.rs so that file stays under the 1000-line cap.

use crate::state::{ModuleKind, ParamMode, param_mode};
use crate::ui::ImpulseApp;
use crate::ui::rack_content_pad::{PAD_SECTION_TOP_GAP, render_three_pad};
use crate::ui::{theme, widgets};

/// Render the ConvReverb card.  Returns `true` when any knob, toggle,
/// or button click changed state — the caller uses that to push audio
/// params + run any side observers.
pub(super) fn draw_conv_reverb(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    module_id: u32,
    pad_expanded: bool,
    pad_pair: &mut u8,
) -> bool {
    let _ = ModuleKind::FxConvReverb;
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
    let (mut mix, mut size, mut pre, mut damp, mut lc, mut wd, mut shim, mut rev, ir_path) = {
        let s = app.state.read();
        (
            s.fx.conv_reverb_mix,
            s.fx.conv_reverb_size,
            s.fx.conv_reverb_predelay,
            s.fx.conv_reverb_damp,
            s.fx.conv_reverb_lowcut,
            s.fx.conv_reverb_width,
            s.fx.conv_reverb_shimmer,
            s.fx.conv_reverb_reverse,
            s.fx.conv_reverb_ir_path.clone(),
        )
    };

    // 6 knobs in a single glass group, two rows of 3 — keeps the
    // card at 3 cols wide while the IR picker gets its own row
    // beneath.
    let avail = ui.available_width();
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        widgets::centered_row(ui, |ui| {
            if widgets::param_control(ui, "MIX", &mut mix, pm("fx.conv_reverb_mix"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "SIZE", &mut size, pm("fx.conv_reverb_size"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(
                ui,
                "PREDELAY",
                &mut pre,
                pm("fx.conv_reverb_predelay"),
                ctrl,
            )
            .0
            {
                changed = true;
            }
        });
        widgets::centered_row(ui, |ui| {
            if widgets::param_control(ui, "DAMP", &mut damp, pm("fx.conv_reverb_damp"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "LOWCUT", &mut lc, pm("fx.conv_reverb_lowcut"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "WIDTH", &mut wd, pm("fx.conv_reverb_width"), ctrl).0 {
                changed = true;
            }
        });
        widgets::centered_row(ui, |ui| {
            if widgets::param_control(ui, "SHIMMER", &mut shim, pm("fx.conv_reverb_shimmer"), ctrl)
                .0
            {
                changed = true;
            }
        });
    });

    // IR picker row — LOAD IR is the primary action so it gets a
    // larger button (wider + taller); REV toggle and × clear sit
    // on either side, with the filename label trailing.  Buttons
    // grouped in their own glass pane so they read as a unit
    // rather than free-floating chrome.
    let (rev_changed, load_clicked, clear_clicked) = {
        let avail = ui.available_width();
        widgets::glass_group_fill(ui, avail, avail, |ui| {
            ui.horizontal(|ui| {
                let prev_rev = rev;
                widgets::toggle_button(ui, if rev { "REV" } else { "rev" }, &mut rev);
                let load = ui
                    .add_sized(
                        [96.0, 24.0],
                        egui::Button::new(egui::RichText::new("LOAD IR").monospace().size(11.0)),
                    )
                    .clicked();
                let clear = ui
                    .add_enabled(
                        !ir_path.is_empty(),
                        egui::Button::new("×").min_size(egui::Vec2::new(20.0, 20.0)),
                    )
                    .clicked();
                let name = if ir_path.is_empty() {
                    "(no ir)".to_string()
                } else {
                    std::path::Path::new(&ir_path)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default()
                };
                ui.label(
                    egui::RichText::new(name)
                        .monospace()
                        .size(8.0)
                        .color(theme::ASH),
                );
                (rev != prev_rev, load, clear)
            })
            .inner
        })
        .inner
    };

    if load_clicked
        && let Some(p) = crate::ui::header_menu::pick_file_via_portal("WAV", &["wav", "WAV"])
    {
        let ps = p.to_string_lossy().to_string();
        if let Some((data, ch)) = crate::audio::load_wav_stereo_to_engine(&ps) {
            let _ = app
                .audio_tx
                .push(crate::audio::AudioCommand::LoadImpulseResponse {
                    data,
                    channels: ch,
                    reversed: rev,
                });
            app.state.write().fx.conv_reverb_ir_path = ps.clone();
            app.last_conv_reverb_ir_path = ps;
        }
    }
    if clear_clicked {
        let _ = app
            .audio_tx
            .push(crate::audio::AudioCommand::ClearImpulseResponse);
        app.state.write().fx.conv_reverb_ir_path = String::new();
        app.last_conv_reverb_ir_path = String::new();
    }
    // API/LLM path: when state.fx.conv_reverb_ir_path changes out
    // from under us (e.g. /api/conv_reverb), reload the WAV + push
    // the DSP command.  Mirrors the amen panel's wave_cache poll.
    if !ir_path.is_empty() && app.last_conv_reverb_ir_path != ir_path {
        if let Some((data, ch)) = crate::audio::load_wav_stereo_to_engine(&ir_path) {
            let _ = app
                .audio_tx
                .push(crate::audio::AudioCommand::LoadImpulseResponse {
                    data,
                    channels: ch,
                    reversed: rev,
                });
        }
        app.last_conv_reverb_ir_path = ir_path.clone();
    }

    if pad_expanded {
        ui.add_space(PAD_SECTION_TOP_GAP);
        let (vc, _) = render_three_pad(
            ui,
            &format!("conv_reverb_xy_{module_id}"),
            ["MIX", "SIZE", "DAMP"],
            pad_pair,
            (&mut mix, &mut size, &mut damp),
            [
                user_owned("fx.conv_reverb_mix"),
                user_owned("fx.conv_reverb_damp"),
                user_owned("fx.conv_reverb_size"),
            ],
        );
        if vc {
            changed = true;
        }
    }
    if changed || rev_changed {
        let mut s = app.state.write();
        s.fx.conv_reverb_mix = mix;
        s.fx.conv_reverb_size = size;
        s.fx.conv_reverb_predelay = pre;
        s.fx.conv_reverb_damp = damp;
        s.fx.conv_reverb_lowcut = lc;
        s.fx.conv_reverb_width = wd;
        s.fx.conv_reverb_shimmer = shim;
        s.fx.conv_reverb_reverse = rev;
    }
    changed || rev_changed
}
