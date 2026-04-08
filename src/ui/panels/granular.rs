// ─── ui/panels/granular.rs ───────────────────────────────────────────────────
// Granular texture voice panel — density, grain size, position, jitter, pitch scatter.

use crate::audio::{AudioCommand, load_wav_to_44100};
use crate::ui::{ImpulseApp, widgets};

pub fn draw_granular(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // WAV file loader
    let mut path = app.state.read().granular.path.clone();
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut path)
                .hint_text("path/to/texture.wav")
                .desired_width(ui.available_width() - 60.0),
        );
        if resp.changed() {
            app.state.write().granular.path = path.clone();
        }
        if ui.button("Load").clicked() {
            match load_wav_to_44100(&path) {
                Some(data) => {
                    let _ = app.audio_tx.push(AudioCommand::LoadGranular(data));
                    log::info!("Granular: loaded '{}'", path);
                }
                None => {
                    log::warn!("Granular: could not load '{}'", path);
                }
            }
        }
    });
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);

    let (mut vol, mut density, mut grain_size, mut position, mut jitter, mut pitch, mut spray) = {
        let s = app.state.read();
        (
            s.granular.volume,
            s.granular.density,
            s.granular.grain_size,
            s.granular.position,
            s.granular.position_jitter,
            s.granular.pitch_scatter,
            s.granular.spray,
        )
    };
    let mut changed = false;
    if widgets::param_control(ui, "VOLUME", &mut vol, pm("granular.volume"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "DENSITY", &mut density, pm("granular.density"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "SIZE", &mut grain_size, pm("granular.grain_size"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "POSITION", &mut position, pm("granular.position"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(
        ui,
        "JITTER",
        &mut jitter,
        pm("granular.position_jitter"),
        ctrl,
    )
    .0
    {
        changed = true;
    }
    if widgets::param_control(ui, "PITCH", &mut pitch, pm("granular.pitch_scatter"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "SPRAY", &mut spray, pm("granular.spray"), ctrl).0 {
        changed = true;
    }
    if changed {
        {
            let mut s = app.state.write();
            s.granular.volume = vol;
            s.granular.density = density;
            s.granular.grain_size = grain_size;
            s.granular.position = position;
            s.granular.position_jitter = jitter;
            s.granular.pitch_scatter = pitch;
            s.granular.spray = spray;
        }
        app.push_audio_params();
    }
}
