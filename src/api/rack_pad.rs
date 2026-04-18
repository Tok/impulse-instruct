// ─── api/rack_pad.rs ─────────────────────────────────────────────────────────
// HTTP handler for the XY-pad expand / pair state on a single module.
// Mirrors the `"rack": { "pad": [...] }` LLM JSON block; extracted from
// api/mod.rs to stay under the 1000-line file cap.

use axum::{Json, extract::State as AxumState};
use serde::Deserialize;

use super::{ApiState, OkResponse, api_log};

#[derive(Deserialize)]
pub struct RackPadRequest {
    /// Module ID whose XY-pad state to update.
    pub id: u32,
    /// Expand/collapse the pad section.  Omit to leave unchanged.
    pub expanded: Option<bool>,
    /// Pair index (0 = A/B, 1 = A/C, 2 = B/C) for 3-knob FX.  Omit to
    /// leave unchanged.  Ignored by 2-knob FX.  Values >= 3 are clamped.
    pub pair: Option<u8>,
}

/// Update a module's XY-pad state (expand/collapse + active pair).
/// Either field is optional so callers can change just one.  When
/// `expanded` changes, `arrange_grid()` is called so neighbours reflow.
pub async fn post_rack_pad(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackPadRequest>,
) -> Json<OkResponse> {
    let mut s = api.app_state.write();
    let (applied_expanded, applied_pair, found) = {
        let m = s.rack.modules.iter_mut().find(|m| m.id == req.id);
        match m {
            Some(m) => {
                let mut expanded_changed = false;
                if let Some(e) = req.expanded
                    && m.pad_expanded != e
                {
                    m.pad_expanded = e;
                    expanded_changed = true;
                }
                if let Some(p) = req.pair {
                    m.pad_pair = p.min(2);
                }
                (expanded_changed, req.pair.is_some(), true)
            }
            None => (false, false, false),
        }
    };
    if !found {
        drop(s);
        api_log(
            &api,
            format!("[API] rack/pad: unknown module id {}", req.id),
        );
        return Json(OkResponse {
            ok: false,
            message: Some(format!("unknown module id {}", req.id)),
        });
    }
    if applied_expanded {
        s.rack.arrange_grid();
    }
    drop(s);
    api_log(
        &api,
        format!(
            "[API] rack/pad: id={} expanded={:?} pair={:?}",
            req.id, req.expanded, req.pair
        ),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!(
            "pad id={} expanded={} pair={}",
            req.id, applied_expanded, applied_pair
        )),
    })
}
