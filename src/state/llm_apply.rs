// ─── state/llm_apply.rs ── LLM update application (extracted from transitions).
use super::llm_apply_seq::{
    SeqScope, apply_amen_update, apply_bass_notes, apply_bass_pans, apply_euclidean_update,
    apply_melodic_lane_lens, apply_preecho_voices, apply_sequencer_globals,
};
use super::llm_helpers::{
    apply_an1x_update, apply_bass_update, apply_fx_update, apply_hoover_update, unlocked_f32,
};
use super::transitions::{set_drum_step_ratchet, set_drum_voice_steps};
use super::{AppState, DrumVoice, LfoTarget, LfoWaveform, MAX_STEPS};

/// Apply an LLM-generated partial update, respecting locked params.
pub fn apply_llm_update(state: AppState, update: &serde_json::Value, scope: &[String]) -> AppState {
    let mut s = state;
    let locked = &s.llm.locked_params.clone();
    let in_scope = |key: &str| scope.is_empty() || scope.iter().any(|s| s == key);

    // Legacy "bass" key → voice 0
    if in_scope("bass")
        && let Some(b) = update.get("bass").and_then(|v| v.as_object())
    {
        apply_bass_update(&mut s, b, locked, 0);
    }
    // "bass_voices": [{...}, null, {...}, ...] — per-voice updates; null skips that slot
    if in_scope("bass")
        && let Some(arr) = update.get("bass_voices").and_then(|v| v.as_array())
    {
        for (idx, voice_update) in arr.iter().enumerate().take(crate::state::MAX_BASS_VOICES) {
            if voice_update.is_null() {
                continue;
            }
            if let Some(b) = voice_update.as_object() {
                apply_bass_update(&mut s, b, locked, idx);
            }
        }
    }

    // Per-voice sequencer fields (bass_steps, bass_notes, kick_a_steps, …)
    // live under `sequencer.*` in the JSON, but logically belong to their
    // voice's scope — a BASS agent with scope=["bass"] must still be able
    // to rewrite bass_steps/bass_notes. Gate each subsection by whichever
    // scope it actually belongs to. A "sequencer" scope grants everything.
    let seq_scope_flags = SeqScope::from_scope(scope);
    if seq_scope_flags.any()
        && let Some(seq) = update.get("sequencer").and_then(|v| v.as_object())
    {
        // Global fields — require explicit "sequencer" scope.
        if seq_scope_flags.seq {
            apply_sequencer_globals(&mut s, seq, locked);
        }

        // ── Per-drum-kit lengths + ratchets ──────────────────────────
        use DrumVoice::*;
        let kit_a_voices: &[(&str, DrumVoice)] = &[
            ("kick_a", Kick808),
            ("snare_a", Snare808),
            ("hihat_a", HihatClosed808),
            ("hihat_a_open", HihatOpen808),
        ];
        let kit_b_voices: &[(&str, DrumVoice)] = &[
            ("kick_b", Kick909),
            ("snare_b", Snare909),
            ("hihat_b", HihatClosed909),
            ("hihat_b_open", HihatOpen909),
            ("clap_b", Clap909),
        ];
        if !locked.contains("sequencer.drum_lengths")
            && let Some(obj) = seq.get("drum_lengths").and_then(|v| v.as_object())
        {
            let mut voices_to_apply: Vec<(&str, DrumVoice)> = Vec::new();
            if seq_scope_flags.kit_a {
                voices_to_apply.extend_from_slice(kit_a_voices);
            }
            if seq_scope_flags.kit_b {
                voices_to_apply.extend_from_slice(kit_b_voices);
            }
            for (key, voice) in &voices_to_apply {
                if let Some(n) = obj.get(*key).and_then(|v| v.as_u64()) {
                    s = set_drum_voice_steps(s, *voice, n as usize);
                }
            }
        }
        if !locked.contains("sequencer.drum_ratchets")
            && let Some(obj) = seq.get("drum_ratchets").and_then(|v| v.as_object())
        {
            let mut voices_to_apply: Vec<(&str, DrumVoice)> = Vec::new();
            if seq_scope_flags.kit_a {
                voices_to_apply.extend_from_slice(kit_a_voices);
            }
            if seq_scope_flags.kit_b {
                voices_to_apply.extend_from_slice(kit_b_voices);
            }
            for (key, voice) in &voices_to_apply {
                if let Some(arr) = obj.get(*key).and_then(|v| v.as_array()) {
                    for (step, val) in arr.iter().enumerate().take(MAX_STEPS) {
                        if let Some(r) = val.as_u64() {
                            s = set_drum_step_ratchet(s, *voice, step, r as u8);
                        }
                    }
                }
            }
        }

        // Per-melodic-voice lane lengths + bass step / note / pan arrays.
        // Each helper checks its own scope flag + lock path; cheap to call
        // unconditionally.
        apply_melodic_lane_lens(&mut s, seq, seq_scope_flags, locked);
        if seq_scope_flags.bass
            && !locked.contains("sequencer.bass_steps")
            && let Some(arr) = seq.get("bass_steps").and_then(|v| v.as_array())
        {
            apply_llm_step_array(arr, &mut s.sequencer.bass_pattern, MAX_STEPS, |step, a| {
                step.active = a;
            });
            let bass_pattern_clone = s.sequencer.bass_pattern.clone();
            if let Some(pat) = s.sequencer.bass_patterns.get_mut(0) {
                *pat = bass_pattern_clone;
            }
        }
        apply_bass_notes(&mut s, seq, seq_scope_flags, locked);
        apply_bass_pans(&mut s, seq, seq_scope_flags, locked);

        let drum_step_fields: &[(&str, DrumVoice, f32, bool)] = &[
            ("kick_a_steps", DrumVoice::Kick808, 1.0, true),
            ("hihat_a_steps", DrumVoice::HihatClosed808, 0.7, true),
            ("snare_a_steps", DrumVoice::Snare808, 1.0, true),
            ("kick_b_steps", DrumVoice::Kick909, 1.0, false),
            ("snare_b_steps", DrumVoice::Snare909, 1.0, false),
            ("clap_b_steps", DrumVoice::Clap909, 1.0, false),
            ("hihat_b_steps", DrumVoice::HihatClosed909, 0.7, false),
        ];
        for &(field, voice, default_vel, is_kit_a) in drum_step_fields {
            let kit_ok = if is_kit_a {
                seq_scope_flags.kit_a
            } else {
                seq_scope_flags.kit_b
            };
            if !kit_ok {
                continue;
            }
            let lock_key = format!("sequencer.{}", field);
            if !locked.contains(&lock_key)
                && let Some(arr) = seq.get(field).and_then(|v| v.as_array())
                && let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice)
            {
                let arr: Vec<_> = arr.clone();
                apply_llm_step_array(&arr, pattern, MAX_STEPS, |step, active| {
                    step.active = active;
                    if active && step.velocity == 0.0 {
                        step.velocity = default_vel;
                    }
                });
            }
        }

        apply_preecho_voices(&mut s, seq, seq_scope_flags, locked);

        // amen_steps + amen_slices — standard step array + optional per-step
        // slice indices (0 = auto-advance, 1..=slice_count = explicit).
        if seq_scope_flags.amen
            && !locked.contains("sequencer.amen_steps")
            && let Some(arr) = seq.get("amen_steps").and_then(|v| v.as_array())
        {
            let arr: Vec<_> = arr.clone();
            if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&DrumVoice::Amen) {
                apply_llm_step_array(&arr, pattern, MAX_STEPS, |step, active| {
                    step.active = active;
                    if active && step.velocity == 0.0 {
                        step.velocity = 1.0;
                    }
                });
            }
        }
        if seq_scope_flags.amen
            && !locked.contains("sequencer.amen_slices")
            && let Some(arr) = seq.get("amen_slices").and_then(|v| v.as_array())
            && let Some(pattern) = s.sequencer.drum_patterns.get_mut(&DrumVoice::Amen)
        {
            for (i, val) in arr.iter().enumerate().take(pattern.len()) {
                if let Some(n) = val.as_u64() {
                    pattern[i].slice = (n as u8).min(16);
                }
            }
        }
    }

    if in_scope("kit_a")
        && let Some(kit_a) = update.get("kit_a").and_then(|v| v.as_object())
        && let Some(kick) = kit_a.get("kick").and_then(|v| v.as_object())
    {
        s.kit_a.kick.pitch_env_depth = unlocked_f32(
            s.kit_a.kick.pitch_env_depth,
            kick,
            "pitch_env_depth",
            "kit_a.kick.pitch_env_depth",
            locked,
        );
        s.kit_a.kick.pitch_env_time = unlocked_f32(
            s.kit_a.kick.pitch_env_time,
            kick,
            "pitch_env_time",
            "kit_a.kick.pitch_env_time",
            locked,
        );
        s.kit_a.kick.clip =
            unlocked_f32(s.kit_a.kick.clip, kick, "clip", "kit_a.kick.clip", locked);
        if !locked.contains("kit_a.kick.pan")
            && let Some(v) = kick.get("pan").and_then(|v| v.as_f64())
        {
            s.kit_a.kick.pan = (v as f32).clamp(-1.0, 1.0);
        }
    }
    // kit_a snare/hihat pan
    if in_scope("kit_a")
        && let Some(kit_a) = update.get("kit_a").and_then(|v| v.as_object())
    {
        if let Some(snare) = kit_a.get("snare").and_then(|v| v.as_object())
            && let Some(v) = snare.get("pan").and_then(|v| v.as_f64())
            && !locked.contains("kit_a.snare.pan")
        {
            s.kit_a.snare.pan = (v as f32).clamp(-1.0, 1.0);
        }
        if let Some(hihat) = kit_a.get("hihat").and_then(|v| v.as_object())
            && let Some(v) = hihat.get("pan").and_then(|v| v.as_f64())
            && !locked.contains("kit_a.hihat.pan")
        {
            s.kit_a.hihat_closed.pan = (v as f32).clamp(-1.0, 1.0);
            s.kit_a.hihat_open.pan = (v as f32).clamp(-1.0, 1.0);
        }
    }

    if in_scope("kit_b")
        && let Some(kit_b) = update.get("kit_b").and_then(|v| v.as_object())
        && let Some(kick) = kit_b.get("kick").and_then(|v| v.as_object())
    {
        s.kit_b.kick.pitch_env_depth = unlocked_f32(
            s.kit_b.kick.pitch_env_depth,
            kick,
            "pitch_env_depth",
            "kit_b.kick.pitch_env_depth",
            locked,
        );
        s.kit_b.kick.pitch_env_time = unlocked_f32(
            s.kit_b.kick.pitch_env_time,
            kick,
            "pitch_env_time",
            "kit_b.kick.pitch_env_time",
            locked,
        );
        s.kit_b.kick.clip =
            unlocked_f32(s.kit_b.kick.clip, kick, "clip", "kit_b.kick.clip", locked);
        if !locked.contains("kit_b.kick.pan")
            && let Some(v) = kick.get("pan").and_then(|v| v.as_f64())
        {
            s.kit_b.kick.pan = (v as f32).clamp(-1.0, 1.0);
        }
    }
    // kit_b snare/hihat/clap pan
    if in_scope("kit_b")
        && let Some(kit_b) = update.get("kit_b").and_then(|v| v.as_object())
    {
        if let Some(snare) = kit_b.get("snare").and_then(|v| v.as_object())
            && let Some(v) = snare.get("pan").and_then(|v| v.as_f64())
            && !locked.contains("kit_b.snare.pan")
        {
            s.kit_b.snare.pan = (v as f32).clamp(-1.0, 1.0);
        }
        if let Some(hihat) = kit_b.get("hihat").and_then(|v| v.as_object())
            && let Some(v) = hihat.get("pan").and_then(|v| v.as_f64())
            && !locked.contains("kit_b.hihat.pan")
        {
            s.kit_b.hihat_closed.pan = (v as f32).clamp(-1.0, 1.0);
            s.kit_b.hihat_open.pan = (v as f32).clamp(-1.0, 1.0);
        }
        if let Some(clap) = kit_b.get("clap").and_then(|v| v.as_object())
            && let Some(v) = clap.get("pan").and_then(|v| v.as_f64())
            && !locked.contains("kit_b.clap.pan")
        {
            s.kit_b.clap.pan = (v as f32).clamp(-1.0, 1.0);
        }
    }

    if in_scope("gabber_kick")
        && let Some(g) = update.get("gabber_kick").and_then(|v| v.as_object())
    {
        let gk = &mut s.gabber_kick;
        gk.pitch = unlocked_f32(gk.pitch, g, "pitch", "gabber_kick.pitch", locked);
        gk.decay = unlocked_f32(gk.decay, g, "decay", "gabber_kick.decay", locked);
        gk.pitch_env_depth = unlocked_f32(
            gk.pitch_env_depth,
            g,
            "pitch_env_depth",
            "gabber_kick.pitch_env_depth",
            locked,
        );
        gk.pitch_env_time = unlocked_f32(
            gk.pitch_env_time,
            g,
            "pitch_env_time",
            "gabber_kick.pitch_env_time",
            locked,
        );
        gk.clip = unlocked_f32(gk.clip, g, "clip", "gabber_kick.clip", locked);
        gk.transient = unlocked_f32(
            gk.transient,
            g,
            "transient",
            "gabber_kick.transient",
            locked,
        );
        // Volume allows up to 1.5; unlocked_f32 clamps to 0..1 — inline version here.
        if !locked.contains("gabber_kick.volume")
            && let Some(v) = g.get("volume").and_then(|v| v.as_f64())
        {
            gk.volume = (v as f32).clamp(0.0, 1.5);
        }
        // Pan is bipolar -1..+1 — unlocked_f32 is unipolar, so inline again.
        if !locked.contains("gabber_kick.pan")
            && let Some(v) = g.get("pan").and_then(|v| v.as_f64())
        {
            gk.pan = (v as f32).clamp(-1.0, 1.0);
        }
    }

    if in_scope("fx")
        && let Some(fx) = update.get("fx").and_then(|v| v.as_object())
    {
        apply_fx_update(&mut s, fx, locked);
    }

    if in_scope("lfo")
        && let Some(lfo_arr) = update.get("lfo").and_then(|v| v.as_array())
    {
        for (i, slot_val) in lfo_arr.iter().enumerate().take(4) {
            let path_prefix = format!("lfo[{}]", i);
            if locked.contains(&path_prefix) {
                continue;
            }
            if let Some(obj) = slot_val.as_object() {
                if let Some(v) = obj.get("enabled").and_then(|v| v.as_bool()) {
                    s.lfo[i].enabled = v;
                }
                if let Some(v) = obj.get("rate").and_then(|v| v.as_f64()) {
                    s.lfo[i].rate = (v as f32).clamp(0.0, 1.0);
                }
                if let Some(v) = obj.get("depth").and_then(|v| v.as_f64()) {
                    s.lfo[i].depth = (v as f32).clamp(0.0, 1.0);
                }
                if let Some(v) = obj.get("phase_offset").and_then(|v| v.as_f64()) {
                    s.lfo[i].phase_offset = (v as f32).clamp(0.0, 1.0);
                }
                if let Some(v) = obj.get("waveform").and_then(|v| v.as_str()) {
                    s.lfo[i].waveform = match v {
                        "Triangle" => LfoWaveform::Triangle,
                        "Saw" => LfoWaveform::Saw,
                        "InvSaw" => LfoWaveform::InvSaw,
                        "Square" => LfoWaveform::Square,
                        "SampleAndHold" | "S&H" => LfoWaveform::SampleAndHold,
                        _ => LfoWaveform::Sine,
                    };
                }
                if let Some(v) = obj.get("target").and_then(|v| v.as_str()) {
                    s.lfo[i].target = match v {
                        "BassCutoff" => LfoTarget::BassCutoff,
                        "BassResonance" => LfoTarget::BassResonance,
                        "BassPitch" => LfoTarget::BassPitch,
                        "BassVolume" => LfoTarget::BassVolume,
                        "ReverbMix" => LfoTarget::ReverbMix,
                        "DelayTime" => LfoTarget::DelayTime,
                        "DelayFeedback" => LfoTarget::DelayFeedback,
                        "ChorusMix" => LfoTarget::ChorusMix,
                        "ChorusRate" => LfoTarget::ChorusRate,
                        "Kick808Pitch" => LfoTarget::Kick808Pitch,
                        _ => LfoTarget::None,
                    };
                }
            }
        }
    }

    if in_scope("free_eg")
        && let Some(eg) = update.get("free_eg").and_then(|v| v.as_object())
        && !locked.contains("free_eg")
    {
        if let Some(v) = eg.get("enabled").and_then(|v| v.as_bool()) {
            s.free_eg.enabled = v;
        }
        if let Some(v) = eg.get("period").and_then(|v| v.as_f64()) {
            s.free_eg.period = (v as f32).clamp(0.0, 1.0);
        }
        if let Some(v) = eg.get("depth").and_then(|v| v.as_f64()) {
            s.free_eg.depth = (v as f32).clamp(0.0, 1.0);
        }
        if let Some(v) = eg.get("loop_mode").and_then(|v| v.as_bool()) {
            s.free_eg.loop_mode = v;
        }
        if let Some(v) = eg.get("target").and_then(|v| v.as_str()) {
            s.free_eg.target = match v {
                "BassCutoff" => LfoTarget::BassCutoff,
                "BassResonance" => LfoTarget::BassResonance,
                "BassPitch" => LfoTarget::BassPitch,
                "BassVolume" => LfoTarget::BassVolume,
                "ReverbMix" => LfoTarget::ReverbMix,
                "DelayTime" => LfoTarget::DelayTime,
                "DelayFeedback" => LfoTarget::DelayFeedback,
                "ChorusMix" => LfoTarget::ChorusMix,
                "ChorusRate" => LfoTarget::ChorusRate,
                "Kick808Pitch" => LfoTarget::Kick808Pitch,
                _ => LfoTarget::None,
            };
        }
        if let Some(arr) = eg.get("values").and_then(|v| v.as_array()) {
            for (i, val) in arr.iter().enumerate().take(8) {
                if let Some(v) = val.as_f64() {
                    s.free_eg.values[i] = (v as f32).clamp(0.0, 1.0);
                }
            }
        }
    }

    if in_scope("noise")
        && let Some(n) = update.get("noise").and_then(|v| v.as_object())
    {
        if !locked.contains("noise.enabled")
            && let Some(v) = n.get("enabled").and_then(|v| v.as_bool())
        {
            s.noise_voice.enabled = v;
        }
        s.noise_voice.volume =
            unlocked_f32(s.noise_voice.volume, n, "volume", "noise.volume", locked);
        s.noise_voice.color = unlocked_f32(s.noise_voice.color, n, "color", "noise.color", locked);
        s.noise_voice.cutoff =
            unlocked_f32(s.noise_voice.cutoff, n, "cutoff", "noise.cutoff", locked);
        s.noise_voice.attack =
            unlocked_f32(s.noise_voice.attack, n, "attack", "noise.attack", locked);
        s.noise_voice.release =
            unlocked_f32(s.noise_voice.release, n, "release", "noise.release", locked);
        s.noise_voice.filter_lfo_rate = unlocked_f32(
            s.noise_voice.filter_lfo_rate,
            n,
            "filter_lfo_rate",
            "noise.filter_lfo_rate",
            locked,
        );
        s.noise_voice.filter_lfo_depth = unlocked_f32(
            s.noise_voice.filter_lfo_depth,
            n,
            "filter_lfo_depth",
            "noise.filter_lfo_depth",
            locked,
        );
        s.noise_voice.sh_rate =
            unlocked_f32(s.noise_voice.sh_rate, n, "sh_rate", "noise.sh_rate", locked);
        s.noise_voice.sh_depth = unlocked_f32(
            s.noise_voice.sh_depth,
            n,
            "sh_depth",
            "noise.sh_depth",
            locked,
        );
        if !locked.contains("noise.pan")
            && let Some(v) = n.get("pan").and_then(|v| v.as_f64())
        {
            s.noise_voice.pan = (v as f32).clamp(-1.0, 1.0);
        }
    }

    if in_scope("granular")
        && let Some(g) = update.get("granular").and_then(|v| v.as_object())
    {
        if !locked.contains("granular.enabled")
            && let Some(v) = g.get("enabled").and_then(|v| v.as_bool())
        {
            s.granular.enabled = v;
        }
        s.granular.volume = unlocked_f32(s.granular.volume, g, "volume", "granular.volume", locked);
        s.granular.density =
            unlocked_f32(s.granular.density, g, "density", "granular.density", locked);
        s.granular.grain_size = unlocked_f32(
            s.granular.grain_size,
            g,
            "grain_size",
            "granular.grain_size",
            locked,
        );
        s.granular.position = unlocked_f32(
            s.granular.position,
            g,
            "position",
            "granular.position",
            locked,
        );
        s.granular.position_jitter = unlocked_f32(
            s.granular.position_jitter,
            g,
            "position_jitter",
            "granular.position_jitter",
            locked,
        );
        s.granular.pitch_scatter = unlocked_f32(
            s.granular.pitch_scatter,
            g,
            "pitch_scatter",
            "granular.pitch_scatter",
            locked,
        );
        s.granular.spray = unlocked_f32(s.granular.spray, g, "spray", "granular.spray", locked);
    }

    if in_scope("hoover")
        && let Some(h) = update.get("hoover").and_then(|v| v.as_object())
    {
        apply_hoover_update(&mut s, h, locked);
    }

    if in_scope("an1x")
        && let Some(a) = update.get("an1x").and_then(|v| v.as_object())
    {
        apply_an1x_update(&mut s, a, locked);
    }

    // Amen sampler params (pitch, volume, loop, slice settings).
    // See `state::llm_apply_seq::apply_amen_update` for the per-field rules.
    if in_scope("amen")
        && let Some(a) = update.get("amen").and_then(|v| v.as_object())
    {
        apply_amen_update(&mut s, a, locked);
    }

    // Euclidean rhythm shortcut — see `apply_euclidean_update`.
    if in_scope("euclidean")
        && let Some(e) = update.get("euclidean").and_then(|v| v.as_object())
    {
        apply_euclidean_update(&mut s, e, locked);
    }

    // ── Internal music API ────────────────────────────────────────────────────
    // JSON: { "music_api": { "chord": {...}, "amen_pattern": {...}, "scale_run": {...} } }
    if let Some(api) = update.get("music_api").and_then(|v| v.as_object()) {
        s = crate::music_api::apply_music_api(s, api);
    }

    // ── Ramp scheduling ───────────────────────────────────────────────────────
    // Singular: { "ramp": { "param": "fx.reverb_mix", "to": 0.6, "bars": 4 } }
    if let Some(obj) = update.get("ramp").and_then(|v| v.as_object()) {
        s = crate::state::jam_tools::parse_and_schedule_ramp(s, obj);
    }
    // Plural: { "ramps": [{ "param": "bass.cutoff", "to": 0.8, "bars": 4 }, ...] }
    if let Some(arr) = update.get("ramps").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(obj) = item.as_object() {
                s = crate::state::jam_tools::parse_and_schedule_ramp(s, obj);
            }
        }
    }

    // ── Behaviour templates ───────────────────────────────────────────────────
    // JSON: { "behaviour": "build" }
    if let Some(name) = update.get("behaviour").and_then(|v| v.as_str()) {
        let heat = s.llm.heat;
        s = crate::state::jam_tools::apply_behaviour(s, name, heat);
    }

    // ── Rack routing (enable/disable modules, add/remove cables) ─────────────
    // JSON: { "rack": { "enable": ["bitcrush"], "disable": ["reverb"],
    //                   "connect": [{"from": "bitcrush", "to": "master"}],
    //                   "disconnect": [{"from": "bitcrush", "to": "master"}] } }
    // Implementation in state/llm_rack.rs.
    if let Some(rack_upd) = update.get("rack").and_then(|v| v.as_object()) {
        super::llm_rack::apply_llm_rack_update(&mut s, rack_upd);
    }

    s
}

// ─── Step-array parser ────────────────────────────────────────────────────────

/// Apply a JSON step array to a mutable pattern slice.
///
/// Accepts: `[]` clear, `[0,4,8]` index list (< 16), `[1,0,…]` inline 0/1 (≥ 16).
/// `set_active` must update `active` plus any default fields.
pub fn apply_llm_step_array<T, F>(
    arr: &[serde_json::Value],
    items: &mut [T],
    max_write: usize,
    mut set_active: F,
) where
    F: FnMut(&mut T, bool),
{
    let n = items.len().min(max_write);
    if arr.is_empty() {
        // [] = clear all
        for item in items[..n].iter_mut() {
            set_active(item, false);
        }
        return;
    }
    if arr.len() < 16 {
        // Index list: clear everything, then activate listed positions
        for item in items[..n].iter_mut() {
            set_active(item, false);
        }
        for val in arr {
            if let Some(idx) = val.as_u64().map(|i| i as usize)
                && idx < n
            {
                set_active(&mut items[idx], true);
            }
        }
        return;
    }
    // Inline: element-by-element (0/1 integers accepted alongside true/false)
    for (i, val) in arr.iter().enumerate().take(n) {
        let active = match val {
            serde_json::Value::Bool(b) => *b,
            serde_json::Value::Number(num) => num.as_u64().unwrap_or(0) != 0,
            _ => continue,
        };
        set_active(&mut items[i], active);
    }
}
