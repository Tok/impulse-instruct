// ─── api/ui_prefs_api.rs ─────────────────────────────────────────────────────
// `POST /api/ui_prefs { partial fields }` — scenarios + external
// controllers pin UI preferences without having to load a full
// project.  Only fields present in the JSON body are updated; the rest
// stay untouched.  Used e.g. by `demo/scenarios/bach-italian-3rd.sh`
// to set its Huth-per-component toggles on a fresh session.
//
// Deliberately separate from `POST /api/params` — that endpoint is for
// LLM-shaped musical-param updates (bass, drums, fx).  UI preferences
// are user-scope, not LLM-scope, so they shouldn't ride the same
// write-path that `apply_llm_update` flows through.

use axum::{Json, extract::State as AxumState};
use serde_json::Value;

use super::{ApiState, OkResponse, api_log};

pub async fn post_ui_prefs(
    AxumState(api): AxumState<ApiState>,
    Json(body): Json<Value>,
) -> Json<OkResponse> {
    let Some(obj) = body.as_object() else {
        return Json(OkResponse {
            ok: false,
            message: Some("expected a JSON object".into()),
        });
    };

    let mut touched: Vec<String> = Vec::new();
    {
        let mut s = api.app_state.write();
        let p = &mut s.ui_prefs;

        // Helper: set a bool field if the JSON carries one.  Closing
        // over `touched` keeps the summary line cheap — no alloc when
        // the caller sends a no-op body.
        let mut set_bool = |key: &str, target: &mut bool| {
            if let Some(v) = obj.get(key).and_then(|v| v.as_bool()) {
                *target = v;
                touched.push(format!("{key}={v}"));
            }
        };
        set_bool("huth_piano", &mut p.huth_piano);
        set_bool("huth_bar_osc", &mut p.huth_bar_osc);
        set_bool("huth_ring_osc", &mut p.huth_ring_osc);
        set_bool("huth_spectrum", &mut p.huth_spectrum);
        set_bool("show_bar_oscilloscope", &mut p.show_bar_oscilloscope);
        set_bool("show_spectrum_bars", &mut p.show_spectrum_bars);
        set_bool("show_ring_oscilloscope", &mut p.show_ring_oscilloscope);
        set_bool("show_event_stream", &mut p.show_event_stream);
        set_bool("stream_bass_notes", &mut p.stream_bass_notes);
        set_bool("stream_drums", &mut p.stream_drums);
        set_bool("stream_hz_scale", &mut p.stream_hz_scale);
        set_bool("stream_ramps", &mut p.stream_ramps);
        set_bool("stream_stereo", &mut p.stream_stereo);
        set_bool("crt_effect", &mut p.crt_effect);
        set_bool("bloom_enabled", &mut p.bloom_enabled);
        set_bool("wasd_as_arrows", &mut p.wasd_as_arrows);
        set_bool("llm_auto_scroll", &mut p.llm_auto_scroll);

        // Numeric fields — clamped to sane ranges.
        if let Some(v) = obj.get("ui_scale").and_then(|v| v.as_f64()) {
            p.ui_scale = (v as f32).clamp(0.5, 3.0);
            touched.push(format!("ui_scale={:.2}", p.ui_scale));
        }
        if let Some(v) = obj.get("bloom_intensity").and_then(|v| v.as_f64()) {
            p.bloom_intensity = (v as f32).clamp(0.0, 1.0);
            touched.push(format!("bloom_intensity={:.2}", p.bloom_intensity));
        }
        if let Some(v) = obj.get("phosphor_intensity").and_then(|v| v.as_f64()) {
            p.phosphor_intensity = (v as f32).clamp(0.0, 1.0);
            touched.push(format!("phosphor_intensity={:.2}", p.phosphor_intensity));
        }
        if let Some(v) = obj.get("phosphor_frames").and_then(|v| v.as_u64()) {
            p.phosphor_frames = (v as usize).clamp(1, 32);
            touched.push(format!("phosphor_frames={}", p.phosphor_frames));
        }
        if let Some(v) = obj.get("rack_grid_cols").and_then(|v| v.as_u64()) {
            p.rack_grid_cols = (v as u8).clamp(3, 6);
            touched.push(format!("rack_grid_cols={}", p.rack_grid_cols));
        }

        // LLM-facing log prefs live on `state.llm` (not ui_prefs) but
        // they're user-scope preferences in practice — let the endpoint
        // set them too so scenarios don't need a second request to
        // toggle "show thinking / show model reasoning" before the LLM
        // starts firing.
        if let Some(v) = obj.get("show_thinking_in_log").and_then(|v| v.as_bool()) {
            s.llm.show_thinking_in_log = v;
            touched.push(format!("show_thinking_in_log={v}"));
        }
        if let Some(v) = obj.get("enable_thinking").and_then(|v| v.as_bool()) {
            s.llm.enable_thinking = v;
            // Propagate to every agent so jam cycles pick up the flag
            // immediately instead of waiting for the next explicit
            // per-agent toggle.
            for a in &mut s.llm_agents {
                a.enable_thinking = v;
            }
            touched.push(format!("enable_thinking={v}"));
        }
    }

    let msg = if touched.is_empty() {
        "no recognised ui_prefs fields in request".to_string()
    } else {
        format!("ui_prefs: {}", touched.join(", "))
    };
    api_log(&api, format!("[API] {msg}"));
    Json(OkResponse {
        ok: !touched.is_empty(),
        message: Some(msg),
    })
}
