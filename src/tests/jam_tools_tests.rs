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
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
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
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
    };
    let ramp2 = ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.9,
        step_per_cycle: 0.2,
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
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
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
    };
    let r2 = ParamRamp {
        param: "fx.delay_mix".into(),
        current: 0.0,
        target: 0.3,
        step_per_cycle: 0.05,
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
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
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
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
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
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
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
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
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
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
fn parse_ramp_bars_creates_bar_based_ramp() {
    let state = AppState::default();
    let obj = serde_json::from_str::<serde_json::Value>(
        r#"{ "param": "fx.delay_mix", "to": 0.4, "bars": 4 }"#,
    )
    .unwrap();
    let out = parse_and_schedule_ramp(state, obj.as_object().unwrap());
    assert_eq!(out.llm.active_ramps.len(), 1);
    let r = &out.llm.active_ramps[0];
    assert!(r.total_global_steps > 0, "should be bar-based");
    assert_eq!(r.total_global_steps, 4 * out.sequencer.steps as u64);
    assert!(
        (r.step_per_cycle).abs() < f32::EPSILON,
        "cycle step should be 0 for bar-based"
    );
}

#[test]
fn parse_ramp_noop_target_drops_ramp_and_pushes_error_feedback() {
    // A ramp from a param's current value back to itself (|delta| <
    // 0.001) is a no-op.  The apply layer drops it AND pushes an
    // ERROR-level feedback line onto `state.llm.recent_feedback` so
    // the LLM sees the mistake on its next prompt.
    let mut state = AppState::default();
    state.fx.reverb_mix = 0.40;
    let obj = serde_json::from_str::<serde_json::Value>(
        r#"{ "param": "fx.reverb_mix", "to": 0.40, "bars": 4 }"#,
    )
    .unwrap();
    let out = parse_and_schedule_ramp(state, obj.as_object().unwrap());
    assert!(
        out.llm.active_ramps.is_empty(),
        "no-op ramp must not be scheduled"
    );
    assert!(!out.llm.recent_feedback.is_empty(), "should emit feedback");
    let msg = out.llm.recent_feedback.back().unwrap();
    assert!(msg.starts_with("ramp ERROR"), "got {msg:?}");
    assert!(msg.contains("no-op"), "got {msg:?}");
}

#[test]
fn parse_ramp_tiny_delta_still_schedules_but_warns() {
    // 0.001 ≤ |delta| < 0.05 is the "barely audible" band — the
    // apply layer schedules the ramp (partial motion is still
    // motion) but pushes a WARN feedback line so the LLM can pick a
    // bigger target next turn.
    let mut state = AppState::default();
    state.fx.reverb_mix = 0.40;
    let obj = serde_json::from_str::<serde_json::Value>(
        r#"{ "param": "fx.reverb_mix", "to": 0.42, "bars": 4 }"#,
    )
    .unwrap();
    let out = parse_and_schedule_ramp(state, obj.as_object().unwrap());
    assert_eq!(out.llm.active_ramps.len(), 1, "tiny ramp still schedules");
    let msg = out.llm.recent_feedback.back().unwrap();
    assert!(msg.starts_with("ramp WARN"), "got {msg:?}");
    assert!(msg.contains("barely audible"), "got {msg:?}");
}

#[test]
fn parse_ramp_normal_delta_does_not_push_feedback() {
    // |delta| ≥ 0.05 — a musically meaningful move.  No feedback
    // pushed; the ramp schedules silently.
    let mut state = AppState::default();
    state.fx.reverb_mix = 0.40;
    let obj = serde_json::from_str::<serde_json::Value>(
        r#"{ "param": "fx.reverb_mix", "to": 0.65, "bars": 4 }"#,
    )
    .unwrap();
    let out = parse_and_schedule_ramp(state, obj.as_object().unwrap());
    assert_eq!(out.llm.active_ramps.len(), 1);
    assert!(
        out.llm.recent_feedback.is_empty(),
        "healthy ramps must not trip feedback"
    );
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

// ─── tick_bar_ramps ──────────────────────────────────────────────────────────

use crate::state::jam_tools::tick_bar_ramps;

#[test]
fn tick_bar_ramps_noop_when_empty() {
    let state = AppState::default();
    let out = tick_bar_ramps(state);
    assert!(out.llm.active_ramps.is_empty());
}

#[test]
fn tick_bar_ramps_interpolates_midway() {
    let mut state = AppState::default();
    state.global_step_count = 100;
    state.llm.active_ramps.push(ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 1.0,
        step_per_cycle: 0.0,
        from: 0.0,
        start_global_step: 100,
        total_global_steps: 16,
    });
    // Advance half the steps
    state.global_step_count = 108;
    let out = tick_bar_ramps(state);
    assert!((out.fx.reverb_mix - 0.5).abs() < 0.01, "should be ~50%");
    assert_eq!(out.llm.active_ramps.len(), 1, "ramp still active");
}

#[test]
fn tick_bar_ramps_completes_and_removes() {
    let mut state = AppState::default();
    state.global_step_count = 100;
    state.llm.active_ramps.push(ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.8,
        step_per_cycle: 0.0,
        from: 0.0,
        start_global_step: 100,
        total_global_steps: 16,
    });
    // Past the end
    state.global_step_count = 120;
    let out = tick_bar_ramps(state);
    assert!(
        (out.fx.reverb_mix - 0.8).abs() < 0.01,
        "should be at target"
    );
    assert!(out.llm.active_ramps.is_empty(), "completed ramp removed");
}

#[test]
fn tick_bar_ramps_cancels_locked_param() {
    let mut state = AppState::default();
    state.global_step_count = 100;
    state.llm.active_ramps.push(ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.8,
        step_per_cycle: 0.0,
        from: 0.0,
        start_global_step: 100,
        total_global_steps: 16,
    });
    // Lock the param mid-ramp
    state.llm.locked_params.insert("fx.reverb_mix".into());
    state.global_step_count = 108;
    let out = tick_bar_ramps(state);
    assert!(
        out.llm.active_ramps.is_empty(),
        "locked ramp should be cancelled"
    );
}

#[test]
fn tick_bar_ramps_skips_cycle_based() {
    let mut state = AppState::default();
    // Add a cycle-based ramp (total_global_steps == 0)
    state.llm.active_ramps.push(ParamRamp {
        param: "fx.delay_mix".into(),
        current: 0.0,
        target: 0.5,
        step_per_cycle: 0.1,
        from: 0.0,
        start_global_step: 0,
        total_global_steps: 0,
    });
    let out = tick_bar_ramps(state);
    assert_eq!(out.llm.active_ramps.len(), 1, "cycle-based ramp untouched");
    assert!((out.fx.delay_mix).abs() < f32::EPSILON, "value not changed");
}

#[test]
fn advance_ramps_skips_bar_based() {
    let mut state = AppState::default();
    state.global_step_count = 100;
    // Add a bar-based ramp
    state.llm.active_ramps.push(ParamRamp {
        param: "fx.reverb_mix".into(),
        current: 0.0,
        target: 0.8,
        step_per_cycle: 0.0,
        from: 0.0,
        start_global_step: 100,
        total_global_steps: 16,
    });
    let out = advance_ramps(state);
    assert_eq!(
        out.llm.active_ramps.len(),
        1,
        "bar-based ramp untouched by advance_ramps"
    );
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

// ─── schedule_baseline_ramps tests ──────────────────────────────────────────

use crate::state::jam_tools::schedule_baseline_ramps;

#[test]
fn baseline_ramps_schedule_f32_params() {
    let state = AppState::default();
    let baseline = serde_json::json!({
        "fx": { "reverb_mix": 0.8, "delay_mix": 0.5 }
    });
    let (out, remainder) = schedule_baseline_ramps(state, &baseline, 4.0);
    assert_eq!(out.llm.active_ramps.len(), 2);
    assert!(remainder.as_object().unwrap().is_empty());
}

#[test]
fn baseline_ramps_pass_through_non_f32() {
    let state = AppState::default();
    let baseline = serde_json::json!({
        "fx": { "reverb_mix": 0.8 },
        "sequencer": { "bass_steps": [0, 4, 8] }
    });
    let (out, remainder) = schedule_baseline_ramps(state, &baseline, 4.0);
    assert_eq!(out.llm.active_ramps.len(), 1); // only reverb_mix
    assert!(remainder["sequencer"]["bass_steps"].is_array()); // passed through
}

#[test]
fn baseline_ramps_skip_unchanged_params() {
    let mut state = AppState::default();
    state.fx.reverb_mix = 0.3;
    let baseline = serde_json::json!({"fx": {"reverb_mix": 0.3}});
    let (out, _) = schedule_baseline_ramps(state, &baseline, 4.0);
    assert!(out.llm.active_ramps.is_empty()); // no change needed
}

// ─── schedule_lane_fade_in (Phase 2/3 companion) ─────────────────────────────

use crate::llm::lanes::LaneKind;
use crate::state::jam_tools::{LANE_FADE_FLOOR, LANE_FADE_STEPS, schedule_lane_fade_in};
use crate::state::lock_params;

#[test]
fn lane_fade_in_schedules_bass0_ramp() {
    let state = AppState::default();
    let v0 = state.bass_voices[0].synth.volume;
    assert!(v0 > 0.02, "default bass volume should be audible");
    let out = schedule_lane_fade_in(state, LaneKind::Bass(0));
    assert_eq!(out.llm.active_ramps.len(), 1);
    let r = &out.llm.active_ramps[0];
    assert_eq!(r.param, "bass.volume");
    assert!((r.from - v0 * LANE_FADE_FLOOR).abs() < 1e-4);
    assert!((r.target - v0).abs() < 1e-4);
    assert_eq!(r.total_global_steps, LANE_FADE_STEPS);
}

#[test]
fn lane_fade_in_uses_nested_path_for_bass_voice_n() {
    let state = AppState::default();
    let out = schedule_lane_fade_in(state, LaneKind::Bass(2));
    assert_eq!(out.llm.active_ramps.len(), 1);
    assert_eq!(out.llm.active_ramps[0].param, "bass_voices.2.volume");
}

#[test]
fn lane_fade_in_schedules_for_hoover_an1x_amen() {
    for (lane, expected_path) in [
        (LaneKind::Hoover, "hoover.volume"),
        (LaneKind::An1x, "an1x.volume"),
        (LaneKind::Amen, "amen.volume"),
    ] {
        let out = schedule_lane_fade_in(AppState::default(), lane);
        assert_eq!(out.llm.active_ramps.len(), 1, "missing ramp for {:?}", lane);
        assert_eq!(out.llm.active_ramps[0].param, expected_path);
    }
}

#[test]
fn lane_fade_in_noops_for_kit_fx_settings_mod_rack() {
    // Kits have per-drum volumes (no master), fx/settings/mod/rack aren't
    // voices — all should no-op rather than schedule an invalid ramp.
    for lane in [
        LaneKind::KitA,
        LaneKind::KitB,
        LaneKind::Fx,
        LaneKind::Settings,
        LaneKind::Modulation,
        LaneKind::Rack,
    ] {
        let out = schedule_lane_fade_in(AppState::default(), lane);
        assert!(
            out.llm.active_ramps.is_empty(),
            "{:?} should not schedule a fade-in",
            lane
        );
    }
}

#[test]
fn lane_fade_in_noops_for_silent_voice() {
    // Voice sitting at near-zero volume — fading would just bring it back
    // to near-zero, wasting a ramp slot.
    let mut state = AppState::default();
    state.bass_voices[0].synth.volume = 0.01;
    let out = schedule_lane_fade_in(state, LaneKind::Bass(0));
    assert!(out.llm.active_ramps.is_empty());
}

#[test]
fn lane_fade_in_respects_lock() {
    // User pinned bass.volume → ramp must not schedule (would dip a
    // locked value and immediately fight apply_llm_update's lock guard).
    let state = lock_params(AppState::default(), &["bass.volume".into()]);
    let out = schedule_lane_fade_in(state, LaneKind::Bass(0));
    assert!(out.llm.active_ramps.is_empty());
}

#[test]
fn lane_fade_in_replaces_ramp_on_repeat_apply() {
    // Two applies of the same lane must leave only one active ramp (the
    // schedule_ramp dedup applies).  Otherwise stacking a ramp per cycle
    // would leak ramps in long-running jams.
    let mut state = schedule_lane_fade_in(AppState::default(), LaneKind::Bass(0));
    state.global_step_count = 32;
    state = schedule_lane_fade_in(state, LaneKind::Bass(0));
    assert_eq!(state.llm.active_ramps.len(), 1);
    assert_eq!(state.llm.active_ramps[0].start_global_step, 32);
}

#[test]
fn lane_fade_in_nested_path_reaches_apply_layer() {
    // End-to-end: running tick_bar_ramps on a bass_voices.N.volume ramp
    // actually hits the voice slot through the extended
    // apply_param_by_path branch.
    let mut state = AppState::default();
    state.bass_voices[2].synth.volume = 0.8;
    state = schedule_lane_fade_in(state, LaneKind::Bass(2));
    state.global_step_count = 8; // halfway through the 16-step fade
    let out = tick_bar_ramps(state);
    // Volume should now sit between floor and target.
    let v = out.bass_voices[2].synth.volume;
    let floor = 0.8 * LANE_FADE_FLOOR;
    assert!(
        v > floor && v < 0.8,
        "mid-ramp voice-2 volume should be between floor {:.3} and 0.8, got {:.3}",
        floor,
        v
    );
}
