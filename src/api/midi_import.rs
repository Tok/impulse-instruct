// ─── api/midi_import.rs ──────────────────────────────────────────────────────
// HTTP handler for `POST /api/midi/import { path, step_division?, voices? }`.
// Reads a Standard MIDI File from disk and populates the pattern bank +
// song chain with up to two 303 voices (RH / LH).  Drives the Bach demo
// scenario (`demo/scenarios/bach-italian-3rd.sh`).

use axum::{Json, extract::State as AxumState};
use serde::{Deserialize, Serialize};

use super::{ApiState, api_log};
use crate::midi::{MidiImport, import_midi_file};

#[derive(Deserialize)]
pub struct MidiImportRequest {
    /// Path to a `.mid` file on disk.  Relative paths resolve against
    /// the server's working directory (typically the project root when
    /// launched by `start.sh` / scenario scripts).
    pub path: String,
    /// Optional override for the step-division grid (4/8/16).  Omit to
    /// auto-detect from the file's smallest inter-onset interval.
    #[serde(default)]
    pub step_division: Option<u8>,
    /// Optional (rh_track_index, lh_track_index) pair.  Omit to pick the
    /// two densest non-drum tracks and assign by mean pitch.
    #[serde(default)]
    pub voices: Option<(usize, usize)>,
    /// When true, blank bass voices 2/3 before writing; otherwise leave
    /// them alone so a subsequent import can layer onto an existing
    /// drum / FX setup.
    #[serde(default)]
    pub wipe_other_voices: bool,
}

/// What the importer wrote — useful to echo back to scenario scripts
/// that want to narrate truncation honestly.
#[derive(Serialize)]
pub struct MidiImportResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_division: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banks_used: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_voice_0: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_voice_1: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub was_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picked_tracks: Option<(usize, usize)>,
    /// Expected playback duration in seconds — lets demo scripts sleep
    /// exactly long enough for the piece to finish before sending the
    /// next command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f32>,
}

pub async fn post_midi_import(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<MidiImportRequest>,
) -> Json<MidiImportResponse> {
    let config = MidiImport {
        step_division: req.step_division,
        voice_tracks: req.voices,
        wipe_other_voices: req.wipe_other_voices,
    };
    let current = api.app_state.read().clone();
    match import_midi_file(current, std::path::Path::new(&req.path), &config) {
        Ok((new_state, summary)) => {
            *api.app_state.write() = new_state;
            api.params_dirty
                .store(true, std::sync::atomic::Ordering::Relaxed);
            let truncated = if summary.was_truncated {
                " (truncated)"
            } else {
                ""
            };
            api_log(
                &api,
                format!(
                    "[API] midi: imported {} — {} BPM, /{} grid, {} banks, v0={} v1={}{}",
                    req.path,
                    summary.bpm.round() as i32,
                    summary.step_division,
                    summary.banks_used,
                    summary.notes_voice_0,
                    summary.notes_voice_1,
                    truncated,
                ),
            );
            Json(MidiImportResponse {
                ok: true,
                message: None,
                bpm: Some(summary.bpm),
                step_division: Some(summary.step_division),
                banks_used: Some(summary.banks_used),
                notes_voice_0: Some(summary.notes_voice_0),
                notes_voice_1: Some(summary.notes_voice_1),
                was_truncated: Some(summary.was_truncated),
                picked_tracks: Some(summary.picked_tracks),
                duration_seconds: Some(summary.duration_seconds),
            })
        }
        Err(e) => {
            api_log(&api, format!("[API] midi: import failed — {}", e));
            Json(MidiImportResponse {
                ok: false,
                message: Some(e),
                bpm: None,
                step_division: None,
                banks_used: None,
                notes_voice_0: None,
                notes_voice_1: None,
                was_truncated: None,
                picked_tracks: None,
                duration_seconds: None,
            })
        }
    }
}
