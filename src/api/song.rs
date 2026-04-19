// ─── api/song.rs ─────────────────────────────────────────────────────────────
// `POST /api/song` — bulk-set the chain + per-slot overrides + enabled flag.
// `GET  /api/song` — returns the current chain, overrides, and enabled flag.
// Companion to Song mode's per-chain-slot overrides (see state/song.rs).

use axum::{Json, extract::State as AxumState};
use serde::{Deserialize, Serialize};

use super::{ApiState, OkResponse, api_log};
use crate::state::ChainSlotOverride;

#[derive(Deserialize)]
pub struct SongRequest {
    /// Ordered chain of pattern-bank indices (0..=7, max 8 entries).
    pub chain: Vec<usize>,
    /// Parallel overrides; shorter than `chain` is fine — missing slots
    /// fall back to the pattern's intrinsic `pattern_style` /
    /// `pattern_bpm_apply` flags and `repeats = 1`.
    #[serde(default)]
    pub overrides: Vec<ChainSlotOverride>,
    /// Whether the chain is enabled for playback.
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct SongResponse {
    pub chain: Vec<usize>,
    pub overrides: Vec<ChainSlotOverride>,
    pub enabled: bool,
    /// Current position in the chain — where playback is right now.
    pub pos: usize,
    /// How many repeats of the current slot have played so far.
    pub repeat_count: u8,
}

pub async fn post_song(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<SongRequest>,
) -> Json<OkResponse> {
    let len = req.chain.len();
    {
        let mut s = api.app_state.write();
        let owned = std::mem::take(&mut *s);
        *s = crate::state::set_song(owned, req.chain, req.overrides, req.enabled);
    }
    api.params_dirty
        .store(true, std::sync::atomic::Ordering::Relaxed);
    api_log(&api, format!("[API] song: set {} slots", len).as_str());
    Json(OkResponse {
        ok: true,
        message: Some(format!("song set: {} slots", len)),
    })
}

pub async fn get_song(AxumState(api): AxumState<ApiState>) -> Json<SongResponse> {
    let s = api.app_state.read();
    Json(SongResponse {
        chain: s.chain.clone(),
        overrides: s.chain_overrides.clone(),
        enabled: s.chain_enabled,
        pos: s.chain_pos,
        repeat_count: s.chain_repeat_count,
    })
}
