// ─── state/jam_tools.rs ───────────────────────────────────────────────────────
// LLM jam helpers: ramp scheduling + behaviour templates.
//
// Ramp scheduling — LLM requests a smooth param transition:
//   { "ramp": { "param": "fx.reverb_mix", "from": 0.0, "to": 0.6, "cycles": 8 } }
// On every jam cycle, `advance_ramps` moves each ramp one step toward its target
// and writes the current value into AppState via a synthetic apply_llm_update call.
//
// Behaviour templates — LLM requests a named mood preset:
//   { "behaviour": "build" }  →  pre-defined set of param changes that shape the energy.

use super::{AppState, ParamRamp};

// ─── Ramp scheduling ─────────────────────────────────────────────────────────

/// Advance all active **cycle-based** ramps by one jam cycle.
/// Each ramp's `current` moves toward `target` by `step_per_cycle`.
/// Completed ramps (within ε of target) are removed.
/// The interpolated value is written to `state` via `apply_llm_update`.
/// Bar-based ramps (total_global_steps > 0) are left untouched — they are
/// ticked per-frame by `tick_bar_ramps`.
pub fn advance_ramps(state: AppState) -> AppState {
    if state.llm.active_ramps.is_empty() {
        return state;
    }

    let mut s = state;
    let mut next_ramps: Vec<ParamRamp> = Vec::with_capacity(s.llm.active_ramps.len());

    for mut ramp in std::mem::take(&mut s.llm.active_ramps) {
        // Skip bar-based ramps — they're handled by tick_bar_ramps.
        if ramp.total_global_steps > 0 {
            next_ramps.push(ramp);
            continue;
        }

        // Advance cycle-based ramp
        let remaining = ramp.target - ramp.current;
        if remaining.abs() <= ramp.step_per_cycle.abs() + f32::EPSILON {
            ramp.current = ramp.target;
        } else {
            ramp.current += ramp.step_per_cycle;
        }

        // Write value into state
        s = apply_param_by_path(s, &ramp.param, ramp.current);

        // Keep if not yet complete
        if (ramp.current - ramp.target).abs() > f32::EPSILON {
            next_ramps.push(ramp);
        }
    }

    s.llm.active_ramps = next_ramps;
    s
}

/// Advance all active **bar-based** ramps using the global step counter.
/// Called every UI frame.  Computes progress from `global_step_count`,
/// writes the interpolated value, and removes completed ramps.
/// Cycle-based ramps (total_global_steps == 0) are left untouched.
pub fn tick_bar_ramps(state: AppState) -> AppState {
    if state.llm.active_ramps.is_empty() {
        return state;
    }

    let now = state.global_step_count;
    let locked = state.llm.locked_params.clone();

    let mut s = state;
    let mut next_ramps: Vec<ParamRamp> = Vec::with_capacity(s.llm.active_ramps.len());

    for ramp in std::mem::take(&mut s.llm.active_ramps) {
        // Skip cycle-based ramps.
        if ramp.total_global_steps == 0 {
            next_ramps.push(ramp);
            continue;
        }

        // Cancel ramp if param was locked mid-ramp.
        if locked.contains(&ramp.param) {
            continue;
        }

        let elapsed = now.saturating_sub(ramp.start_global_step);
        let t = if ramp.total_global_steps > 0 {
            (elapsed as f32 / ramp.total_global_steps as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let value = lerp(ramp.from, ramp.target, t);
        s = apply_param_by_path(s, &ramp.param, value);

        if t < 1.0 {
            next_ramps.push(ramp);
        }
    }

    s.llm.active_ramps = next_ramps;
    s
}

/// Add a ramp to the active list (or replace one with the same param path).
pub fn schedule_ramp(state: AppState, ramp: ParamRamp) -> AppState {
    let mut s = state;
    // Replace any existing ramp for the same param
    s.llm.active_ramps.retain(|r| r.param != ramp.param);
    s.llm.active_ramps.push(ramp);
    s
}

/// Schedule ramps for all f32 params in a style baseline JSON.
/// Non-f32 values (arrays, strings, objects) are collected into a remainder
/// JSON that the caller should apply immediately via `apply_llm_update`.
/// `cycles` controls how many jam cycles the transition takes (e.g. 8 ≈ 2 bars).
pub fn schedule_baseline_ramps(
    state: AppState,
    baseline: &serde_json::Value,
    cycles: f32,
) -> (AppState, serde_json::Value) {
    let mut s = state;
    let mut remainder = serde_json::Map::new();

    if let Some(obj) = baseline.as_object() {
        for (section, values) in obj {
            if let Some(inner) = values.as_object() {
                let mut section_remainder = serde_json::Map::new();
                for (key, val) in inner {
                    if let Some(target) = val.as_f64() {
                        let path = format!("{section}.{key}");
                        let current = current_param_value(&s, &path);
                        let target_f = target as f32;
                        if (current - target_f).abs() > 0.001 && cycles > 0.0 {
                            let step = (target_f - current) / cycles;
                            s = schedule_ramp(
                                s,
                                ParamRamp {
                                    param: path,
                                    current,
                                    target: target_f,
                                    step_per_cycle: step,
                                    from: 0.0,
                                    start_global_step: 0,
                                    total_global_steps: 0,
                                },
                            );
                        }
                    } else {
                        // Non-f32 (arrays, strings, etc.) → apply immediately
                        section_remainder.insert(key.clone(), val.clone());
                    }
                }
                if !section_remainder.is_empty() {
                    remainder.insert(
                        section.clone(),
                        serde_json::Value::Object(section_remainder),
                    );
                }
            } else if !values.is_f64() {
                // Top-level non-object, non-f32 (e.g. "sequencer": {...} with mixed content)
                remainder.insert(section.clone(), values.clone());
            } else {
                // Top-level f32 (e.g. BPM) — schedule ramp
                let path = section.clone();
                let current = current_param_value(&s, &path);
                let target_f = values.as_f64().unwrap() as f32;
                if (current - target_f).abs() > 0.001 && cycles > 0.0 {
                    let step = (target_f - current) / cycles;
                    s = schedule_ramp(
                        s,
                        ParamRamp {
                            param: path,
                            current,
                            target: target_f,
                            step_per_cycle: step,
                            from: 0.0,
                            start_global_step: 0,
                            total_global_steps: 0,
                        },
                    );
                }
            }
        }
    }

    (s, serde_json::Value::Object(remainder))
}

/// Parse a ramp JSON object and schedule it.
///
/// Cycle-based: `{ "param": "fx.reverb_mix", "to": 0.6, "cycles": 8 }`
/// Bar-based:   `{ "param": "fx.reverb_mix", "to": 0.6, "bars": 4 }`
///
/// When `"bars"` is present, creates a bar-based ramp that ticks per UI frame
/// using `global_step_count`.  Otherwise creates a cycle-based ramp (legacy).
/// `"cycles"` defaults to 4 when neither key is present.
pub fn parse_and_schedule_ramp(
    state: AppState,
    obj: &serde_json::Map<String, serde_json::Value>,
) -> AppState {
    let param = match obj.get("param").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return state,
    };
    let to = match obj.get("to").and_then(|v| v.as_f64()) {
        Some(v) => v as f32,
        None => return state,
    };
    // "from" defaults to the current value of the param
    let from = obj
        .get("from")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or_else(|| current_param_value(&state, &param));

    // No-op guard: if the ramp target is already (effectively) where we are,
    // don't schedule — it would waste a ramp slot and show a flat line in the
    // event stream. 0.002 ≈ 0.2 %, below visible knob movement.
    if (from - to).abs() < 0.002 {
        log::debug!(
            "ramp: skipping no-op ({} already at {:.3}, target {:.3})",
            param,
            from,
            to
        );
        return state;
    }

    // Bar-based ramp: { "bars": 4 }  →  smooth interpolation over N bars.
    if let Some(bars) = obj.get("bars").and_then(|v| v.as_f64()) {
        let bars = bars.max(1.0) as u64;
        let steps_per_bar = state.sequencer.steps as u64; // 1 pattern = 1 bar
        let total_global_steps = bars * steps_per_bar;
        let ramp = ParamRamp {
            param,
            current: from,
            target: to,
            step_per_cycle: 0.0,
            from,
            start_global_step: state.global_step_count,
            total_global_steps,
        };
        return schedule_ramp(state, ramp);
    }

    // Cycle-based ramp (legacy): { "cycles": 8 }
    let cycles = obj
        .get("cycles")
        .and_then(|v| v.as_f64())
        .unwrap_or(4.0)
        .max(1.0) as f32;

    let step_per_cycle = (to - from) / cycles;
    let ramp = ParamRamp {
        param,
        current: from,
        target: to,
        step_per_cycle,
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
    };
    schedule_ramp(state, ramp)
}

// ─── Behaviour templates ──────────────────────────────────────────────────────

/// Expand a behaviour template name into a JSON update and apply it.
/// These are curated moods — param changes that express a collective intent.
pub fn apply_behaviour(state: AppState, name: &str, heat: f32) -> AppState {
    let json = behaviour_json(name, heat);
    if json.is_null() {
        return state;
    }
    super::transitions::apply_llm_update(state, &json, &[])
}

/// Return the param-change JSON for a named behaviour at the given heat level.
/// Returns `serde_json::Value::Null` for unknown names.
fn behaviour_json(name: &str, heat: f32) -> serde_json::Value {
    let h = heat.clamp(0.0, 1.0);
    match name.to_lowercase().as_str() {
        "build" | "buildup" | "rise" => serde_json::json!({
            "fx": {
                "reverb_mix": lerp(0.2, 0.5, h),
                "delay_mix":  lerp(0.1, 0.4, h),
                "reverb_size": lerp(0.5, 0.9, h)
            },
            "sequencer": { "swing": lerp(0.0, 0.25, h) }
        }),
        "drop" | "peak" | "full_energy" => serde_json::json!({
            "fx": {
                "reverb_mix": lerp(0.3, 0.1, h),
                "delay_mix":  lerp(0.2, 0.0, h),
                "master_volume": lerp(0.85, 1.0, h),
                "distortion_drive": lerp(0.2, 0.5, h),
                "distortion_mix": lerp(0.15, 0.4, h)
            },
            "sequencer": { "swing": 0.0 }
        }),
        "breakdown" | "strip" | "minimal" => serde_json::json!({
            "fx": {
                "reverb_mix":  lerp(0.5, 0.8, h),
                "delay_mix":   lerp(0.4, 0.6, h),
                "reverb_size": lerp(0.7, 1.0, h),
                "distortion_mix": 0.0,
                "master_volume": lerp(0.7, 0.5, h)
            }
        }),
        "tension" | "dark" => serde_json::json!({
            "bass": { "cutoff": lerp(0.1, 0.25, h), "resonance": lerp(0.5, 0.8, h) },
            "fx": {
                "reverb_mix": lerp(0.4, 0.7, h),
                "reverb_size": 0.9,
                "distortion_mix": lerp(0.1, 0.3, h)
            }
        }),
        "euphoric" | "bright" => serde_json::json!({
            "bass": { "cutoff": lerp(0.6, 0.9, h), "resonance": lerp(0.3, 0.5, h) },
            "fx": {
                "reverb_mix": lerp(0.3, 0.5, h),
                "chorus_mix": lerp(0.2, 0.4, h),
                "master_volume": lerp(0.85, 1.0, h)
            }
        }),
        _ => serde_json::Value::Null,
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ─── Param path read/write helpers ───────────────────────────────────────────

/// Read the current float value of a param by dot-path.
/// Returns 0.0 for unknown paths (ramp will start from 0 in that case).
fn current_param_value(state: &AppState, path: &str) -> f32 {
    match path {
        "fx.reverb_mix" => state.fx.reverb_mix,
        "fx.reverb_size" => state.fx.reverb_size,
        "fx.reverb_damp" => state.fx.reverb_damp,
        "fx.reverb_gate_time" => state.fx.reverb_gate_time,
        "fx.delay_mix" => state.fx.delay_mix,
        "fx.delay_time" => state.fx.delay_time,
        "fx.delay_feedback" => state.fx.delay_feedback,
        "fx.distortion_drive" => state.fx.distortion_drive,
        "fx.distortion_mix" => state.fx.distortion_mix,
        "fx.chorus_mix" => state.fx.chorus_mix,
        "fx.chorus_rate" => state.fx.chorus_rate,
        "fx.chorus_depth" => state.fx.chorus_depth,
        "fx.phaser_mix" => state.fx.phaser_mix,
        "fx.phaser_rate" => state.fx.phaser_rate,
        "fx.phaser_depth" => state.fx.phaser_depth,
        "fx.waveshaper_drive" => state.fx.waveshaper_drive,
        "fx.waveshaper_mix" => state.fx.waveshaper_mix,
        "fx.ring_mod_freq" => state.fx.ring_mod_freq,
        "fx.ring_mod_mix" => state.fx.ring_mod_mix,
        "fx.eq_low_gain" => state.fx.eq_low_gain,
        "fx.eq_mid_gain" => state.fx.eq_mid_gain,
        "fx.eq_hi_gain" => state.fx.eq_hi_gain,
        "fx.compressor_threshold" => state.fx.compressor_threshold,
        "fx.compressor_ratio" => state.fx.compressor_ratio,
        "fx.compressor_mix" => state.fx.compressor_mix,
        "fx.tape_drive" => state.fx.tape_drive,
        "fx.tape_mix" => state.fx.tape_mix,
        "fx.tape_flutter" => state.fx.tape_flutter,
        "fx.master_volume" => state.fx.master_volume,
        "fx.master_pitch_st" => state.fx.master_pitch_st,
        "bass.cutoff" => state.bass_voices[0].synth.cutoff,
        "bass.resonance" => state.bass_voices[0].synth.resonance,
        "bass.volume" => state.bass_voices[0].synth.volume,
        "bass.env_mod" => state.bass_voices[0].synth.env_mod,
        "bass.decay" => state.bass_voices[0].synth.decay,
        "sequencer.bpm" => state.sequencer.bpm,
        "sequencer.swing" => state.sequencer.swing,
        _ => 0.0,
    }
}

/// Write a single float value to a param by dot-path.
/// Builds a minimal nested JSON and calls apply_llm_update so locked_params are respected.
fn apply_param_by_path(state: AppState, path: &str, value: f32) -> AppState {
    // Split "fx.reverb_mix" → {"fx": {"reverb_mix": value}}
    let parts: Vec<&str> = path.splitn(2, '.').collect();
    let json = match parts.as_slice() {
        [section, key] => serde_json::json!({ *section: { *key: value } }),
        [key] => serde_json::json!({ *key: value }),
        _ => return state,
    };
    super::transitions::apply_llm_update(state, &json, &[])
}
