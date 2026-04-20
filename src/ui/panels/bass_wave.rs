// ─── ui/panels/bass_wave.rs ──────────────────────────────────────────────────
// Waveform / filter-mode / preset row + supersaw controls.
// Extracted from bass.rs to stay under the line limit.

use crate::state::{FilterMode, ParamMode, Waveform};
use crate::ui::{ImpulseApp, theme, widgets};

pub(super) fn draw_wave_preset_section(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (waveform, filter_mode, mut supersaw_detune, supersaw_voices) = {
        let s = app.state.read();
        let av = s.active_voice.min(s.bass_voices.len().saturating_sub(1));
        let b = &s.bass_voices[av].synth;
        (
            b.waveform.clone(),
            b.filter_mode,
            b.supersaw_detune,
            b.supersaw_voices,
        )
    };
    let env_h = app.state.read().ui_prefs.effective_env_h();
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    // WAVE / FILT / PRESET rows left, waveform display right
    let label_w = 50.0;
    let wave_kind = match waveform {
        Waveform::Saw => 0,
        Waveform::Square => 1,
        Waveform::Supersaw => 2,
    };
    let viz_w = (ui.available_width() - label_w - 160.0).max(80.0);
    let viz_h = env_h.max(60.0);

    ui.horizontal(|ui| {
        // Left column: WAVE + FILT + PRESET stacked
        ui.vertical(|ui| {
            // WAVE row
            ui.horizontal(|ui| {
                ui.add_sized(
                    [label_w, 18.0],
                    egui::Label::new(
                        egui::RichText::new("WAVE")
                            .color(theme::SMOKE)
                            .monospace()
                            .size(9.0),
                    ),
                );
                let mut saw = waveform == Waveform::Saw;
                if widgets::toggle_button(ui, "SAW", &mut saw) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.waveform = Waveform::Saw;
                    app.push_audio_params();
                }
                let mut sq = waveform == Waveform::Square;
                if widgets::toggle_button(ui, "SQR", &mut sq) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.waveform = Waveform::Square;
                    app.push_audio_params();
                }
                let mut ss = waveform == Waveform::Supersaw;
                if widgets::toggle_button(ui, "SUPER", &mut ss) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.waveform = Waveform::Supersaw;
                    app.push_audio_params();
                }
            });
            // FILT row
            ui.horizontal(|ui| {
                ui.add_sized(
                    [label_w, 18.0],
                    egui::Label::new(
                        egui::RichText::new("FILT")
                            .color(theme::SMOKE)
                            .monospace()
                            .size(9.0),
                    ),
                );
                let mut lp = filter_mode == FilterMode::Lowpass;
                if widgets::toggle_button(ui, "LP", &mut lp) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.filter_mode = FilterMode::Lowpass;
                    app.push_audio_params();
                }
                let mut hp = filter_mode == FilterMode::Highpass;
                if widgets::toggle_button(ui, "HP", &mut hp) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.filter_mode = FilterMode::Highpass;
                    app.push_audio_params();
                }
                let mut bp = filter_mode == FilterMode::Bandpass;
                if widgets::toggle_button(ui, "BP", &mut bp) {
                    let av = app.state.read().active_voice;
                    app.state.write().bass_voices[av].synth.filter_mode = FilterMode::Bandpass;
                    app.push_audio_params();
                }
            });
            // PRESET row
            ui.horizontal(|ui| {
                ui.add_sized(
                    [label_w, 18.0],
                    egui::Label::new(
                        egui::RichText::new("PRESET")
                            .color(theme::SMOKE)
                            .monospace()
                            .size(9.0),
                    ),
                );
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("REESE")
                            .monospace()
                            .size(9.0)
                            .color(theme::FOG),
                    ))
                    .on_hover_text("Detuned dual saws + sub + highpass — classic DnB/jungle bass")
                    .clicked()
                {
                    let s = app.state.read().clone();
                    *app.state.write() = crate::state::apply_reese_preset(s);
                    app.push_audio_params();
                }
            });
        });
        // Right column: waveform display (with padding)
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.add_space(4.0);
            widgets::waveform_icon(ui, wave_kind, viz_w, viz_h);
            ui.add_space(4.0);
        });
    });

    // Supersaw controls (only shown when Supersaw is active)
    if waveform == Waveform::Supersaw {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("DETUNE")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            if widgets::param_control(ui, "", &mut supersaw_detune, ParamMode::Free, ctrl).0 {
                let av = app.state.read().active_voice;
                let mut s = app.state.write();
                s.bass_voices[av].synth.supersaw_detune = supersaw_detune;
                drop(s);
                app.push_audio_params();
            }
            let voices_label = format!("{}V", supersaw_voices);
            ui.label(
                egui::RichText::new("VOICES")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            if ui.small_button("-").clicked() && supersaw_voices > 2 {
                let av = app.state.read().active_voice;
                app.state.write().bass_voices[av].synth.supersaw_voices = supersaw_voices - 1;
                app.push_audio_params();
            }
            ui.label(
                egui::RichText::new(&voices_label)
                    .color(theme::CHALK)
                    .monospace()
                    .size(9.0),
            );
            if ui.small_button("+").clicked() && supersaw_voices < 7 {
                let av = app.state.read().active_voice;
                app.state.write().bass_voices[av].synth.supersaw_voices = supersaw_voices + 1;
                app.push_audio_params();
            }
        });
    }
}
