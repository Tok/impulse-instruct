// ─── api/midi_export.rs ──────────────────────────────────────────────────────
// HTTP handler for `POST /api/midi/export { path?: String }`.  Writes the
// current sequencer pattern to a Standard MIDI File.  Living in its own
// submodule to keep api/mod.rs under the 1000-line cap.

use axum::{Json, extract::State as AxumState};
use serde::Deserialize;

use super::{ApiState, OkResponse, api_log};

#[derive(Deserialize)]
pub struct MidiExportRequest {
    /// Optional output path.  When omitted, the file lands in the cwd
    /// as `pattern-<unix_seconds>.mid` (mirrors save_project).
    #[serde(default)]
    pub path: Option<String>,
}

/// Export the sequencer's active pattern bank slot as an SMF Type 1 file.
pub async fn post_midi_export(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<MidiExportRequest>,
) -> Json<OkResponse> {
    let seq = {
        let s = api.app_state.read();
        s.sequencer.clone()
    };
    let path = req.path.as_deref().map(std::path::Path::new);
    match crate::midi::save_midi_export_to(&seq, path) {
        Ok(p) => {
            let shown = p.display().to_string();
            api_log(&api, format!("[API] midi: exported to {}", shown));
            Json(OkResponse {
                ok: true,
                message: Some(shown),
            })
        }
        Err(e) => {
            api_log(&api, format!("[API] midi: export failed — {}", e));
            Json(OkResponse {
                ok: false,
                message: Some(e),
            })
        }
    }
}
