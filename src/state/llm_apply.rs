// ─── state/llm_apply.rs ── LLM update application (extracted from transitions).
use super::llm_helpers::{
    apply_an1x_update, apply_bass_update, apply_fx_update, apply_hoover_update, unlocked_f32,
};
use super::modulation::rack_out_port_kind;
use super::rack::{PortDir, PortKind, PortRef};
use super::rack_scope::{parse_module_kind, rack_kind_name_matches};
use super::transitions::{
    expand_sequencer_steps, set_drum_step_ratchet, set_drum_voice_steps, set_lane_steps,
};
use super::{AppState, DrumVoice, LfoTarget, LfoWaveform, MAX_STEPS, Scale, snap_to_scale};
use crate::sequencer::euclidean_rhythm;

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
    let seq_scope = in_scope("sequencer");
    let bass_ok = seq_scope || in_scope("bass");
    let hoover_ok = seq_scope || in_scope("hoover");
    let an1x_ok = seq_scope || in_scope("an1x");
    let kit_a_ok = seq_scope || in_scope("kit_a");
    let kit_b_ok = seq_scope || in_scope("kit_b");
    let amen_ok = seq_scope || in_scope("amen");
    let any_seq_scope =
        seq_scope || bass_ok || hoover_ok || an1x_ok || kit_a_ok || kit_b_ok || amen_ok;
    if any_seq_scope && let Some(seq) = update.get("sequencer").and_then(|v| v.as_object()) {
        // ── Global fields — require explicit "sequencer" scope ───────
        if seq_scope {
            if !locked.contains("sequencer.bpm")
                && let Some(bpm) = seq.get("bpm").and_then(|v| v.as_f64())
            {
                s.sequencer.bpm = (bpm as f32).clamp(40.0, 250.0);
            }
            if !locked.contains("sequencer.swing")
                && let Some(v) = seq.get("swing").and_then(|v| v.as_f64())
            {
                s.sequencer.swing = (v as f32).clamp(0.0, 1.0);
            }
            if !locked.contains("sequencer.steps")
                && let Some(steps) = seq.get("steps").and_then(|v| v.as_u64())
            {
                s = expand_sequencer_steps(s, steps as usize);
            }
            if !locked.contains("sequencer.time_sig_num")
                && let Some(v) = seq.get("time_sig_num").and_then(|v| v.as_u64())
            {
                s.sequencer.time_sig_num = (v as u8).clamp(2, 9);
            }
            if !locked.contains("sequencer.root_note")
                && let Some(v) = seq.get("root_note").and_then(|v| v.as_u64())
            {
                s.sequencer.root_note = (v as u8).clamp(0, 11);
            }
            if !locked.contains("sequencer.scale")
                && let Some(v) = seq.get("scale").and_then(|v| v.as_str())
                && let Some(sc) = Scale::from_str(v)
            {
                s.sequencer.scale = sc;
            }
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
            if kit_a_ok {
                voices_to_apply.extend_from_slice(kit_a_voices);
            }
            if kit_b_ok {
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
            if kit_a_ok {
                voices_to_apply.extend_from_slice(kit_a_voices);
            }
            if kit_b_ok {
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

        // ── Per-melodic-voice fields ─────────────────────────────────
        if bass_ok {
            if !locked.contains("sequencer.bass_len")
                && let Some(n) = seq.get("bass_len").and_then(|v| v.as_u64())
            {
                s = set_lane_steps(s, "bass", n as usize);
            }
            if !locked.contains("sequencer.bass_steps")
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
        }
        if hoover_ok
            && !locked.contains("sequencer.hoover_len")
            && let Some(n) = seq.get("hoover_len").and_then(|v| v.as_u64())
        {
            s = set_lane_steps(s, "hoover", n as usize);
        }
        if an1x_ok
            && !locked.contains("sequencer.an1x_len")
            && let Some(n) = seq.get("an1x_len").and_then(|v| v.as_u64())
        {
            s = set_lane_steps(s, "an1x", n as usize);
        }
        if bass_ok
            && !locked.contains("sequencer.bass_notes")
            && let Some(arr) = seq.get("bass_notes").and_then(|v| v.as_array())
        {
            let snap = s.sequencer.scale_snap;
            let root = s.sequencer.root_note;
            let scale = s.sequencer.scale;
            for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                if let Some(note) = val.as_u64() {
                    let note = note.clamp(0, 127) as u8;
                    let snapped = if snap {
                        snap_to_scale(note, root, scale)
                    } else {
                        note
                    };
                    s.sequencer.bass_pattern[i].note = snapped;
                    // Keep voice 0 pattern in sync
                    if let Some(pat) = s.sequencer.bass_patterns.get_mut(0) {
                        pat[i].note = snapped;
                    }
                }
            }
        }
        // Per-step pan parallel to bass_notes (-1..1; 0 = use voice static).
        if bass_ok
            && !locked.contains("sequencer.bass_pans")
            && let Some(arr) = seq.get("bass_pans").and_then(|v| v.as_array())
        {
            for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                if let Some(p) = val.as_f64() {
                    let p = (p as f32).clamp(-1.0, 1.0);
                    s.sequencer.bass_pattern[i].pan = p;
                    if let Some(pat) = s.sequencer.bass_patterns.get_mut(0) {
                        pat[i].pan = p;
                    }
                }
            }
        }
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
            let kit_ok = if is_kit_a { kit_a_ok } else { kit_b_ok };
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

        // Pre-echo (anchor lead-ins).  JSON shape:
        //   "sequencer": { "preecho": { "kit_a": { "anchors": [0,16],
        //     "length": 4, "velocity_ramp": true, "ratchet_ramp": true }}}.
        // null clears that voice; voice keys match scope naming.
        if any_seq_scope && let Some(obj) = seq.get("preecho").and_then(|v| v.as_object()) {
            for (voice_key, val) in obj {
                let lock_key = format!("sequencer.preecho.{}", voice_key);
                if locked.contains(&lock_key) {
                    continue;
                }
                let scope_ok = match voice_key.as_str() {
                    "bass" => bass_ok,
                    "hoover" => hoover_ok,
                    "an1x" => an1x_ok,
                    "kit_a" => kit_a_ok,
                    "kit_b" => kit_b_ok,
                    "amen" => amen_ok,
                    _ => seq_scope,
                };
                if !scope_ok {
                    continue;
                }
                if val.is_null() {
                    s.sequencer.preecho.remove(voice_key);
                    continue;
                }
                let Some(cfg_obj) = val.as_object() else {
                    continue;
                };
                let mut cfg = s
                    .sequencer
                    .preecho
                    .get(voice_key)
                    .cloned()
                    .unwrap_or_default();
                if let Some(v) = cfg_obj.get("enabled").and_then(|v| v.as_bool()) {
                    cfg.enabled = v;
                }
                if let Some(arr) = cfg_obj.get("anchors").and_then(|v| v.as_array()) {
                    cfg.anchors = arr
                        .iter()
                        .filter_map(|x| x.as_u64())
                        .map(|n| (n as u8).min(63))
                        .collect();
                }
                if let Some(v) = cfg_obj.get("length").and_then(|v| v.as_u64()) {
                    cfg.length = (v as u8).min(16);
                }
                if let Some(v) = cfg_obj.get("velocity_ramp").and_then(|v| v.as_bool()) {
                    cfg.velocity_ramp = v;
                }
                if let Some(v) = cfg_obj.get("ratchet_ramp").and_then(|v| v.as_bool()) {
                    cfg.ratchet_ramp = v;
                }
                s.sequencer.preecho.insert(voice_key.clone(), cfg);
            }
        }

        // amen_steps + amen_slices — standard step array + optional per-step
        // slice indices (0 = auto-advance, 1..=slice_count = explicit).
        if amen_ok
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
        if amen_ok
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

    // ── Amen sampler params (pitch, volume, loop, slice settings) ───────────
    // JSON: { "amen": { "slice_count": 8, "gate": 0.85, "reverse": false,
    //                   "stutter": 0, "start_offset": 0.0, "end_offset": 1.0,
    //                   "loop_mode": true, "pitch": 0, "volume": 0.75 } }
    if in_scope("amen")
        && let Some(a) = update.get("amen").and_then(|v| v.as_object())
    {
        s.amen.pitch =
            unlocked_f32(s.amen.pitch, a, "pitch", "amen.pitch", locked).clamp(-24.0, 24.0);
        s.amen.volume = unlocked_f32(s.amen.volume, a, "volume", "amen.volume", locked);
        if let Some(v) = a.get("loop_mode").and_then(|v| v.as_bool())
            && !locked.contains("amen.loop_mode")
        {
            s.amen.loop_mode = v;
        }
        if let Some(v) = a.get("slice_count").and_then(|v| v.as_u64())
            && !locked.contains("amen.slice_count")
        {
            s.amen.slice_count = (v as u8).clamp(1, 16);
        }
        s.amen.start_offset = unlocked_f32(
            s.amen.start_offset,
            a,
            "start_offset",
            "amen.start_offset",
            locked,
        )
        .clamp(0.0, 1.0);
        s.amen.end_offset = unlocked_f32(
            s.amen.end_offset,
            a,
            "end_offset",
            "amen.end_offset",
            locked,
        )
        .clamp(0.0, 1.0);
        if s.amen.end_offset <= s.amen.start_offset {
            s.amen.end_offset = (s.amen.start_offset + 0.01).min(1.0);
        }
        if let Some(v) = a.get("reverse").and_then(|v| v.as_bool())
            && !locked.contains("amen.reverse")
        {
            s.amen.reverse = v;
        }
        s.amen.gate = unlocked_f32(s.amen.gate, a, "gate", "amen.gate", locked).clamp(0.05, 1.0);
        if let Some(v) = a.get("stutter").and_then(|v| v.as_u64())
            && !locked.contains("amen.stutter")
        {
            s.amen.stutter = (v as u8).min(4);
        }
        s.amen.source_bpm = unlocked_f32(
            s.amen.source_bpm,
            a,
            "source_bpm",
            "amen.source_bpm",
            locked,
        )
        .clamp(40.0, 300.0);
        if let Some(v) = a.get("bpm_stretch").and_then(|v| v.as_bool())
            && !locked.contains("amen.bpm_stretch")
        {
            s.amen.bpm_stretch = v;
        }
        // Per-slice pitch array: either an array of semitone offsets or
        // clears via empty array / null.
        if !locked.contains("amen.slice_pitches")
            && let Some(v) = a.get("slice_pitches")
        {
            if let Some(arr) = v.as_array() {
                s.amen.slice_pitches = arr
                    .iter()
                    .filter_map(|x| x.as_f64())
                    .map(|x| (x as f32).clamp(-24.0, 24.0))
                    .take(16)
                    .collect();
            } else if v.is_null() {
                s.amen.slice_pitches.clear();
            }
        }
        if !locked.contains("amen.slice_volumes")
            && let Some(v) = a.get("slice_volumes")
        {
            if let Some(arr) = v.as_array() {
                s.amen.slice_volumes = arr
                    .iter()
                    .filter_map(|x| x.as_f64())
                    .map(|x| (x as f32).clamp(0.0, 2.0))
                    .take(16)
                    .collect();
            } else if v.is_null() {
                s.amen.slice_volumes.clear();
            }
        }
    }

    // ── Euclidean rhythm ──────────────────────────────────────────────────────
    // JSON: { "euclidean": { "voice": "kick_a", "pulses": 5, "steps": 16 } }
    if in_scope("euclidean")
        && let Some(e) = update.get("euclidean").and_then(|v| v.as_object())
    {
        let voice_str = e.get("voice").and_then(|v| v.as_str()).unwrap_or("");
        let pulses = e.get("pulses").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
        let n_steps = e
            .get("steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(s.sequencer.steps as u64) as usize;
        let drum_voice = match voice_str {
            "kick_a" => Some(DrumVoice::Kick808),
            "snare_a" => Some(DrumVoice::Snare808),
            "hihat_a" | "closed_hat_a" => Some(DrumVoice::HihatClosed808),
            "hihat_a_open" | "open_hat_a" => Some(DrumVoice::HihatOpen808),
            "kick_b" => Some(DrumVoice::Kick909),
            "snare_b" => Some(DrumVoice::Snare909),
            "hihat_b" | "closed_hat_b" => Some(DrumVoice::HihatClosed909),
            "hihat_b_open" | "open_hat_b" => Some(DrumVoice::HihatOpen909),
            "clap_b" => Some(DrumVoice::Clap909),
            _ => None,
        };
        if let Some(voice) = drum_voice {
            let lock_path = format!("sequencer.{}_steps", voice_str);
            if !locked.contains(&lock_path) {
                let pattern = euclidean_rhythm(pulses, n_steps);
                if let Some(row) = s.sequencer.drum_patterns.get_mut(&voice) {
                    for (i, &active) in pattern.iter().enumerate().take(row.len()) {
                        row[i].active = active;
                    }
                }
            }
        }
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
    if let Some(rack_upd) = update.get("rack").and_then(|v| v.as_object()) {
        // ── rack.add — create new modules from a list of kind strings.
        // Auto-wires voice/FX modules to MasterOutput so they're audible
        // without a second "connect" action (matches /api/rack/add behavior).
        if let Some(arr) = rack_upd.get("add").and_then(|v| v.as_array()) {
            for v in arr {
                let Some(name) = v.as_str() else { continue };
                let Some(kind) = parse_module_kind(name) else {
                    continue;
                };
                let id = s.rack.add_module(kind);
                if !matches!(kind.default_zone(), super::Zone::Global | super::Zone::Ai)
                    && let Some(master_id) = s
                        .rack
                        .modules
                        .iter()
                        .find(|m| m.kind == super::ModuleKind::MasterOutput)
                        .map(|m| m.id)
                {
                    s.rack.connect(
                        PortRef {
                            module_id: id,
                            dir: PortDir::Out,
                            kind: PortKind::Audio,
                            index: 0,
                        },
                        PortRef {
                            module_id: master_id,
                            dir: PortDir::In,
                            kind: PortKind::Audio,
                            index: 0,
                        },
                    );
                }
                if kind == super::ModuleKind::NeuTts {
                    s.tts_modules.push(super::TtsModuleState::new(id));
                }
                // Scroll the UI to the newly-added module so it's visible
                // in the rack (matches /api/rack/add behavior).
                s.scroll_target = Some(name.to_string());
            }
        }
        // ── rack.remove — delete the first module matching each kind string.
        if let Some(arr) = rack_upd.get("remove").and_then(|v| v.as_array()) {
            for v in arr {
                let Some(name) = v.as_str() else { continue };
                let target = s
                    .rack
                    .modules
                    .iter()
                    .find(|m| rack_kind_name_matches(m.kind, name))
                    .map(|m| m.id);
                if let Some(id) = target {
                    s.rack.remove_module(id);
                    s.tts_modules.retain(|t| t.id != id);
                }
            }
        }
        if let Some(arr) = rack_upd.get("enable").and_then(|v| v.as_array()) {
            for v in arr {
                if let Some(name) = v.as_str() {
                    for m in &mut s.rack.modules {
                        if rack_kind_name_matches(m.kind, name) {
                            m.enabled = true;
                        }
                    }
                }
            }
        }
        if let Some(arr) = rack_upd.get("disable").and_then(|v| v.as_array()) {
            for v in arr {
                if let Some(name) = v.as_str() {
                    for m in &mut s.rack.modules {
                        if rack_kind_name_matches(m.kind, name) {
                            m.enabled = false;
                        }
                    }
                }
            }
        }
        if let Some(arr) = rack_upd.get("connect").and_then(|v| v.as_array()) {
            for v in arr {
                let from_name = v.get("from").and_then(|v| v.as_str());
                let to_name = v.get("to").and_then(|v| v.as_str());
                if let (Some(fn_), Some(tn)) = (from_name, to_name) {
                    let from_mod = s
                        .rack
                        .modules
                        .iter()
                        .find(|m| rack_kind_name_matches(m.kind, fn_))
                        .map(|m| (m.id, m.kind));
                    let to_mod = s
                        .rack
                        .modules
                        .iter()
                        .find(|m| rack_kind_name_matches(m.kind, tn))
                        .map(|m| (m.id, m.kind));
                    if let (Some((fid, fkind)), Some((tid, _tkind))) = (from_mod, to_mod) {
                        // Don't duplicate an existing cable
                        let exists = s
                            .rack
                            .cables
                            .iter()
                            .any(|c| c.from.module_id == fid && c.to.module_id == tid);
                        if !exists {
                            let port_kind = rack_out_port_kind(fkind);
                            s.rack.connect(
                                PortRef {
                                    module_id: fid,
                                    dir: PortDir::Out,
                                    kind: port_kind,
                                    index: 0,
                                },
                                PortRef {
                                    module_id: tid,
                                    dir: PortDir::In,
                                    kind: port_kind,
                                    index: 0,
                                },
                            );
                        }
                    }
                }
            }
        }
        if let Some(arr) = rack_upd.get("disconnect").and_then(|v| v.as_array()) {
            for v in arr {
                let from_name = v.get("from").and_then(|v| v.as_str());
                let to_name = v.get("to").and_then(|v| v.as_str());
                if let (Some(fn_), Some(tn)) = (from_name, to_name) {
                    let from_mod = s
                        .rack
                        .modules
                        .iter()
                        .find(|m| rack_kind_name_matches(m.kind, fn_))
                        .map(|m| (m.id, m.kind));
                    let to_id = s
                        .rack
                        .modules
                        .iter()
                        .find(|m| rack_kind_name_matches(m.kind, tn))
                        .map(|m| m.id);
                    if let (Some((fid, fkind)), Some(tid)) = (from_mod, to_id) {
                        let port_kind = rack_out_port_kind(fkind);
                        let from_ref = PortRef {
                            module_id: fid,
                            dir: PortDir::Out,
                            kind: port_kind,
                            index: 0,
                        };
                        let to_ref = PortRef {
                            module_id: tid,
                            dir: PortDir::In,
                            kind: port_kind,
                            index: 0,
                        };
                        s.rack.disconnect(&from_ref, &to_ref);
                    }
                }
            }
        }

        // rack.mod_cable — per-knob LFO modulation patching.
        if let Some(arr) = rack_upd.get("mod_cable").and_then(|v| v.as_array()) {
            for v in arr {
                crate::state::modulation::apply_llm_mod_cable_entry(&mut s.rack, v);
            }
        }
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
