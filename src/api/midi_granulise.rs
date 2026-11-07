// ─── api/midi_granulise.rs ────────────────────────────────────────────────────
// HTTP handler for `POST /api/midi/granulise` — applies the
// trigger-granuliser to a named voice's sequencer pattern in place.
// Absurd-queue feature #8.
//
// Body shape:
//   {
//     "voice": "bass" | "hoover" | "an1x" | "pluck" | "wavetable" | "sample",
//     "voice_index": 0,        // optional — 0..MAX_BASS_VOICES for bass
//     "density": 0.7,          // 0..1, default 1.0
//     "repeat_chance": 0.0,    // 0..1, default 0
//     "pitch_jitter_st": 0,    // 0..12, default 0
//     "seed": 42               // u64, default = nanos since epoch
//   }
//
// The granuliser itself lives in `sequencer::granuliser`; this
// handler just resolves the named voice to a pattern reference and
// hands it over.

use axum::{Json, extract::State as AxumState};

use crate::sequencer::granuliser::{GranuliseOpts, granulise_smf_bytes, granulise_tb303};

use super::{ApiState, OkResponse, api_log};

pub async fn post_midi_granulise(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<serde_json::Value>,
) -> Json<OkResponse> {
    // File-to-file mode (V2 follow-up).  When `in_path` is provided,
    // the handler reads SMF bytes from disk, runs the granuliser
    // through `granulise_smf_bytes`, and writes the result to
    // `out_path` (defaulting to `in_path`, i.e. in-place file edit).
    // No live state is touched, so this path is safe to call from
    // a script that doesn't want to disturb the current session.
    if let Some(in_path) = req.get("in_path").and_then(|v| v.as_str()) {
        return file_to_file_granulise(&api, in_path, &req).await;
    }

    // Parse + clamp inputs.  Defaults err on the side of pass-through
    // (density 1.0, no jitter, no repeat) so a body with just
    // `{"voice": "bass"}` is a no-op rather than a surprise.
    let voice = req
        .get("voice")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    if voice.is_empty() {
        return Json(OkResponse {
            ok: false,
            message: Some("granulise requires either a 'voice' or an 'in_path' field".into()),
        });
    }
    let voice_index = req
        .get("voice_index")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(0);
    let density = req
        .get("density")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(1.0);
    let repeat_chance = req
        .get("repeat_chance")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(0.0);
    let pitch_jitter_st = req
        .get("pitch_jitter_st")
        .and_then(|v| v.as_u64())
        .map(|v| v as u8)
        .unwrap_or(0);
    let seed = req.get("seed").and_then(|v| v.as_u64()).unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    });

    let opts = GranuliseOpts {
        density,
        repeat_chance,
        pitch_jitter_st,
        seed,
    };

    // Resolve the voice name to a pattern reference + apply.  All
    // melodic voices use `Vec<TB303Step>`, so the dispatch is a flat
    // match.  Bass takes a per-voice index because we have multiple
    // bass voices; the others are singletons.
    let mut s = api.app_state.write();
    let label: String = match voice.as_str() {
        "bass" | "303" | "acid" => {
            let idx = voice_index.min(s.sequencer.bass_patterns.len().saturating_sub(1));
            granulise_tb303(&mut s.sequencer.bass_patterns[idx], opts);
            format!("bass[{idx}]")
        }
        "hoover" | "lead" => {
            granulise_tb303(&mut s.sequencer.hoover_pattern, opts);
            "hoover".into()
        }
        "an1x" | "pad" => {
            granulise_tb303(&mut s.sequencer.an1x_pattern, opts);
            "an1x".into()
        }
        "pluck" => {
            granulise_tb303(&mut s.sequencer.pluck_pattern, opts);
            "pluck".into()
        }
        "wavetable" | "wt" => {
            granulise_tb303(&mut s.sequencer.wavetable_pattern, opts);
            "wavetable".into()
        }
        "sample" | "sampler" => {
            granulise_tb303(&mut s.sequencer.sample_pattern, opts);
            "sample".into()
        }
        other => {
            return Json(OkResponse {
                ok: false,
                message: Some(format!(
                    "unknown voice '{other}' — use one of: bass, hoover, an1x, pluck, wavetable, sample"
                )),
            });
        }
    };
    drop(s);

    api_log(
        &api,
        format!(
            "[API] midi/granulise: voice={label} density={density:.2} repeat={repeat_chance:.2} pj={pitch_jitter_st} seed={seed}"
        ),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!(
            "granulised {label} (density={density:.2}, repeat={repeat_chance:.2}, pitch±{pitch_jitter_st}st, seed={seed})"
        )),
    })
}

/// File-to-file granulise — the V2 follow-up shape.  Reads
/// `in_path`, runs `granulise_smf_bytes`, writes to `out_path` (or
/// back to `in_path` if absent).  Pulls the same density / repeat /
/// jitter / seed knobs from the request body.
async fn file_to_file_granulise(
    api: &ApiState,
    in_path: &str,
    req: &serde_json::Value,
) -> Json<OkResponse> {
    let density = req
        .get("density")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(1.0);
    let repeat_chance = req
        .get("repeat_chance")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(0.0);
    let pitch_jitter_st = req
        .get("pitch_jitter_st")
        .and_then(|v| v.as_u64())
        .map(|v| v as u8)
        .unwrap_or(0);
    let seed = req.get("seed").and_then(|v| v.as_u64()).unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    });
    let out_path = req
        .get("out_path")
        .and_then(|v| v.as_str())
        .unwrap_or(in_path)
        .to_string();

    let bytes = match std::fs::read(in_path) {
        Ok(b) => b,
        Err(e) => {
            return Json(OkResponse {
                ok: false,
                message: Some(format!("read {in_path}: {e}")),
            });
        }
    };
    let opts = GranuliseOpts {
        density,
        repeat_chance,
        pitch_jitter_st,
        seed,
    };
    let out_bytes = match granulise_smf_bytes(&bytes, opts) {
        Ok(b) => b,
        Err(e) => {
            return Json(OkResponse {
                ok: false,
                message: Some(format!("granulise: {e}")),
            });
        }
    };
    if let Err(e) = std::fs::write(&out_path, &out_bytes) {
        return Json(OkResponse {
            ok: false,
            message: Some(format!("write {out_path}: {e}")),
        });
    }

    api_log(
        api,
        format!(
            "[API] midi/granulise (file): {in_path} → {out_path} density={density:.2} repeat={repeat_chance:.2} pj={pitch_jitter_st} seed={seed}"
        ),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!(
            "granulised {in_path} → {out_path} (density={density:.2}, repeat={repeat_chance:.2}, pitch±{pitch_jitter_st}st, seed={seed})"
        )),
    })
}
