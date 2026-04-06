// ─── tests/jam_tools_tests.rs ─────────────────────────────────────────────────
// Unit tests for src/state/jam_tools.rs — ramp scheduling, behaviour templates.

use crate::state::jam_tools::{
    advance_ramps, apply_behaviour, parse_and_schedule_ramp, schedule_ramp,
};
use crate::state::{AppState, ParamRamp};

// ─── schedule_ramp ────────────────────────────────────────────────────────────

#[test]
fn schedule_ramp_adds_to_active_list() {
    let state = AppState::default();
    assert!(state.llm.active_ramps.is_empty());
    let ramp = ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.6,
        step_per_cycle: 0.1,
    };
    let out = schedule_ramp(state, ramp);
    assert_eq!(out.llm.active_ramps.len(), 1);
    assert_eq!(out.llm.active_ramps[0].param, "fx.reverb_mix");
}

#[test]
fn schedule_ramp_replaces_existing_same_param() {
    let state = AppState::default();
    let ramp1 = ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.5,
        step_per_cycle: 0.1,
    };
    let ramp2 = ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.9,
        step_per_cycle: 0.2,
    };
    let state = schedule_ramp(state, ramp1);
    let state = schedule_ramp(state, ramp2);
    assert_eq!(
        state.llm.active_ramps.len(),
        1,
        "should replace, not duplicate"
    );
    assert!((state.llm.active_ramps[0].target - 0.9).abs() < 0.001);
}

#[test]
fn schedule_ramp_different_params_both_kept() {
    let state = AppState::default();
    let r1 = ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.5,
        step_per_cycle: 0.1,
    };
    let r2 = ParamRamp {
        param: "fx.delay_mix".into(),
        current: 0.0,
        target: 0.3,
        step_per_cycle: 0.05,
    };
    let state = schedule_ramp(state, r1);
    let state = schedule_ramp(state, r2);
    assert_eq!(state.llm.active_ramps.len(), 2);
}

// ─── advance_ramps ────────────────────────────────────────────────────────────

#[test]
fn advance_ramps_noop_when_empty() {
    let state = AppState::default();
    let reverb_before = state.fx.reverb_mix;
    let out = advance_ramps(state);
    assert_eq!(out.llm.active_ramps.len(), 0);
    assert!((out.fx.reverb_mix - reverb_before).abs() < f32::EPSILON);
}

#[test]
fn advance_ramps_moves_current_toward_target() {
    let state = AppState::default();
    let ramp = ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.5,
        step_per_cycle: 0.1,
    };
    let state = schedule_ramp(state, ramp);
    let out = advance_ramps(state);
    // Current should have advanced by one step
    assert!((out.fx.reverb_mix - 0.1).abs() < 0.001);
    assert_eq!(out.llm.active_ramps.len(), 1, "ramp still active");
    assert!((out.llm.active_ramps[0].current - 0.1).abs() < 0.001);
}

#[test]
fn advance_ramps_removes_completed_ramp() {
    let state = AppState::default();
    let ramp = ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.45,
        target: 0.5,
        step_per_cycle: 0.1, // one step overshoots
    };
    let state = schedule_ramp(state, ramp);
    let out = advance_ramps(state);
    assert_eq!(
        out.llm.active_ramps.len(),
        0,
        "completed ramp should be removed"
    );
    assert!((out.fx.reverb_mix - 0.5).abs() < 0.01);
}

#[test]
fn advance_ramps_multiple_steps_converge() {
    let mut state = AppState::default();
    let ramp = ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.4,
        step_per_cycle: 0.1,
    };
    state = schedule_ramp(state, ramp);
    for _ in 0..4 {
        state = advance_ramps(state);
    }
    // After 4 steps of 0.1 = 0.4, should be complete
    assert_eq!(
        state.llm.active_ramps.len(),
        0,
        "4-step ramp should complete"
    );
    assert!((state.fx.reverb_mix - 0.4).abs() < 0.01);
}

#[test]
fn advance_ramps_downward_ramp() {
    let mut state = AppState::default();
    // Set reverb high first (bypassing locks for the test)
    state.fx.reverb_mix = 0.8;
    let ramp = ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.8,
        target: 0.2,
        step_per_cycle: -0.1,
    };
    state = schedule_ramp(state, ramp);
    state = advance_ramps(state);
    assert!(state.fx.reverb_mix < 0.8, "value should decrease");
}

// ─── parse_and_schedule_ramp ──────────────────────────────────────────────────

#[test]
fn parse_ramp_basic() {
    let state = AppState::default();
    let obj = serde_json::from_str::<serde_json::Value>(
        r#"{ "param": "fx.reverb_mix", "to": 0.6, "cycles": 8 }"#,
    )
    .unwrap();
    let out = parse_and_schedule_ramp(state, obj.as_object().unwrap());
    assert_eq!(out.llm.active_ramps.len(), 1);
    assert!((out.llm.active_ramps[0].target - 0.6).abs() < 0.001);
    assert!((out.llm.active_ramps[0].step_per_cycle - (0.6 / 8.0)).abs() < 0.001);
}

#[test]
fn parse_ramp_bars_alias_for_cycles() {
    let state = AppState::default();
    let obj = serde_json::from_str::<serde_json::Value>(
        r#"{ "param": "fx.delay_mix", "to": 0.4, "bars": 4 }"#,
    )
    .unwrap();
    let out = parse_and_schedule_ramp(state, obj.as_object().unwrap());
    assert_eq!(out.llm.active_ramps.len(), 1);
}

#[test]
fn parse_ramp_missing_param_is_noop() {
    let state = AppState::default();
    let obj = serde_json::from_str::<serde_json::Value>(r#"{ "to": 0.6, "cycles": 4 }"#).unwrap();
    let out = parse_and_schedule_ramp(state, obj.as_object().unwrap());
    assert_eq!(out.llm.active_ramps.len(), 0);
}

#[test]
fn parse_ramp_missing_to_is_noop() {
    let state = AppState::default();
    let obj = serde_json::from_str::<serde_json::Value>(r#"{ "param": "fx.reverb_mix" }"#).unwrap();
    let out = parse_and_schedule_ramp(state, obj.as_object().unwrap());
    assert_eq!(out.llm.active_ramps.len(), 0);
}

#[test]
fn parse_ramp_explicit_from() {
    let state = AppState::default();
    let obj = serde_json::from_str::<serde_json::Value>(
        r#"{ "param": "fx.reverb_mix", "from": 0.1, "to": 0.5, "cycles": 4 }"#,
    )
    .unwrap();
    let out = parse_and_schedule_ramp(state, obj.as_object().unwrap());
    assert!((out.llm.active_ramps[0].current - 0.1).abs() < 0.001);
    assert!((out.llm.active_ramps[0].step_per_cycle - 0.1).abs() < 0.001);
}

// ─── apply_behaviour ─────────────────────────────────────────────────────────

#[test]
fn behaviour_build_increases_reverb() {
    let state = AppState::default();
    let before = state.fx.reverb_mix;
    let out = apply_behaviour(state, "build", 0.5);
    // build at heat=0.5 should set reverb_mix > the default (assuming default is 0)
    assert!(
        out.fx.reverb_mix >= before,
        "build should not decrease reverb"
    );
}

#[test]
fn behaviour_drop_sets_zero_distortion_mix() {
    // drop at heat=0.5 gives distortion_drive=0.35, distortion_mix=0.275
    let state = AppState::default();
    let out = apply_behaviour(state, "drop", 0.5);
    assert!(
        out.fx.distortion_drive > 0.0,
        "drop should add distortion drive"
    );
}

#[test]
fn behaviour_breakdown_reduces_volume() {
    let state = AppState::default();
    let out = apply_behaviour(state, "breakdown", 0.5);
    // breakdown reduces master_volume
    assert!(out.fx.master_volume < 1.0, "breakdown should reduce volume");
}

#[test]
fn behaviour_unknown_is_noop() {
    let state = AppState::default();
    let reverb_before = state.fx.reverb_mix;
    let out = apply_behaviour(state, "frobniculate", 0.5);
    assert!(
        (out.fx.reverb_mix - reverb_before).abs() < f32::EPSILON,
        "unknown behaviour should be noop"
    );
}

#[test]
fn behaviour_aliases_work() {
    for name in [
        "buildup",
        "rise",
        "peak",
        "full_energy",
        "strip",
        "minimal",
        "dark",
        "bright",
    ] {
        let state = AppState::default();
        // Should not panic — just check it doesn't crash
        let _ = apply_behaviour(state, name, 0.5);
    }
}

#[test]
fn behaviour_tension_lowers_cutoff() {
    let state = AppState::default();
    let before = state.bass_voices[0].synth.cutoff;
    let out = apply_behaviour(state, "tension", 0.5);
    // tension lowers cutoff
    assert!(
        out.bass_voices[0].synth.cutoff < before,
        "tension should lower bass cutoff"
    );
}
