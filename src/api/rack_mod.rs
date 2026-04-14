// ─── api/rack_mod.rs ─────────────────────────────────────────────────────────
// HTTP handlers for the per-knob modulation system: patch a Mod cable from
// an LFO to a target jack, set its multi-select target list, and adjust the
// per-jack depth.  All routes mirror the structures stored on `RackModule`.

use axum::{Json, extract::State as AxumState};
use serde::Deserialize;

use super::{ApiState, OkResponse, api_log};

#[derive(Deserialize)]
pub struct RackModCableRequest {
    pub from: u32,
    pub to: u32,
    pub slot: u8,
    #[serde(default)]
    pub depth: Option<f32>,
}

#[derive(Deserialize)]
pub struct RackModTargetRequest {
    pub module: u32,
    pub slot: u8,
    pub targets: Vec<String>,
}

#[derive(Deserialize)]
pub struct RackModDepthRequest {
    pub module: u32,
    pub slot: u8,
    pub depth: f32,
}

pub async fn post_rack_mod_cable(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackModCableRequest>,
) -> Json<OkResponse> {
    use crate::state::{PortDir, PortKind, PortRef};
    let mut s = api.app_state.write();
    s.rack.connect(
        PortRef {
            module_id: req.from,
            dir: PortDir::Out,
            kind: PortKind::Cv,
            index: 0,
        },
        PortRef {
            module_id: req.to,
            dir: PortDir::In,
            kind: PortKind::Mod,
            index: req.slot,
        },
    );
    if let Some(d) = req.depth
        && let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == req.to)
    {
        let idx = req.slot as usize;
        if m.mod_input_depths.len() <= idx {
            m.mod_input_depths.resize(idx + 1, 1.0);
        }
        m.mod_input_depths[idx] = d.clamp(0.0, 1.0);
    }
    drop(s);
    api_log(
        &api,
        format!(
            "[API] rack: mod cable LFO#{} → module {} slot {} (depth={:?})",
            req.from, req.to, req.slot, req.depth
        ),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!("mod {} → {} slot {}", req.from, req.to, req.slot)),
    })
}

pub async fn post_rack_mod_target(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackModTargetRequest>,
) -> Json<OkResponse> {
    use crate::state::LfoTarget;
    let parsed: Vec<LfoTarget> = req
        .targets
        .iter()
        .filter_map(|name| crate::state::parse_lfo_target(name))
        .collect();
    let mut s = api.app_state.write();
    if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == req.module) {
        let idx = req.slot as usize;
        if m.mod_selectors.len() <= idx {
            m.mod_selectors.resize(idx + 1, Vec::new());
        }
        m.mod_selectors[idx] = parsed;
    }
    drop(s);
    api_log(
        &api,
        format!(
            "[API] rack: mod target module {} slot {} -> {:?}",
            req.module, req.slot, req.targets
        ),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!(
            "mod target module {} slot {}",
            req.module, req.slot
        )),
    })
}

pub async fn post_rack_mod_depth(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackModDepthRequest>,
) -> Json<OkResponse> {
    let mut s = api.app_state.write();
    if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == req.module) {
        let idx = req.slot as usize;
        if m.mod_input_depths.len() <= idx {
            m.mod_input_depths.resize(idx + 1, 1.0);
        }
        m.mod_input_depths[idx] = req.depth.clamp(0.0, 1.0);
    }
    drop(s);
    api_log(
        &api,
        format!(
            "[API] rack: mod depth module {} slot {} = {:.2}",
            req.module, req.slot, req.depth
        ),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!(
            "mod depth module {} slot {} = {:.2}",
            req.module, req.slot, req.depth
        )),
    })
}
