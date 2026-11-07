// ─── tests/tts_tests.rs ───────────────────────────────────────────────────────
// Tests for TTS module lifecycle, cable routing, and per-module state.

use crate::state::*;

#[test]
fn default_rack_creates_tts_module_state() {
    let s = AppState::default();
    let tts_modules: Vec<_> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::Tts)
        .collect();
    // Default rack has a Tts module
    assert!(
        !tts_modules.is_empty(),
        "default rack should have a TTS module"
    );
    // Each TTS rack module should have a corresponding TtsModuleState
    for m in &tts_modules {
        assert!(
            s.tts_modules.iter().any(|t| t.id == m.id),
            "TTS module {} should have a TtsModuleState",
            m.id
        );
    }
}

#[test]
fn tts_module_state_defaults() {
    let tts = TtsModuleState::new(42);
    assert_eq!(tts.id, 42);
    assert_eq!(tts.voice_ref, "default");
    assert!((tts.temperature - 0.8).abs() < 0.01);
    assert_eq!(tts.top_k, 30);
    assert!((tts.top_p - 0.9).abs() < 0.01);
    assert!(!tts.pitch_snap);
}

#[test]
fn tts_module_removed_with_rack_module() {
    let mut s = AppState::default();
    let id = s.rack.add_module(ModuleKind::Tts);
    s.tts_modules.push(TtsModuleState::new(id));
    assert!(s.tts_modules.iter().any(|t| t.id == id));
    // Simulate removal (as done in API)
    s.rack.remove_module(id);
    s.tts_modules.retain(|t| t.id != id);
    assert!(!s.tts_modules.iter().any(|t| t.id == id));
}

#[test]
fn tts_modules_cleared_on_rack_reset() {
    let mut s = AppState::default();
    let id = s.rack.add_module(ModuleKind::Tts);
    s.tts_modules.push(TtsModuleState::new(id));
    assert!(!s.tts_modules.is_empty());
    // Simulate rack reset (keep only core modules)
    let keep: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| {
            matches!(
                m.kind,
                ModuleKind::StepSequencer | ModuleKind::MasterOutput | ModuleKind::LlmConsole
            )
        })
        .map(|m| m.id)
        .collect();
    s.rack.modules.retain(|m| keep.contains(&m.id));
    s.rack
        .cables
        .retain(|c| keep.contains(&c.from.module_id) && keep.contains(&c.to.module_id));
    s.tts_modules.clear();
    assert!(s.tts_modules.is_empty());
}

#[test]
fn agent_finds_tts_via_control_cable() {
    let mut s = AppState::default();
    // Add an agent
    let agent_id = s.rack.add_module(ModuleKind::LlmAgent);
    // Add a TTS module
    let tts_id = s.rack.add_module(ModuleKind::Tts);
    let mut tts_state = TtsModuleState::new(tts_id);
    tts_state.temperature = 1.2;
    s.tts_modules.push(tts_state);
    // Wire agent -> TTS via control cable
    s.rack.connect_control(agent_id, tts_id);
    // Find TTS module via cable routing (same logic as llm/mod.rs)
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack
                .modules
                .iter()
                .any(|m| m.id == target && m.kind == ModuleKind::Tts && m.enabled)
            {
                s.tts_modules.iter().find(|t| t.id == target)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(found.is_some(), "should find TTS module via cable");
    assert!((found.unwrap().temperature - 1.2).abs() < 0.01);
}

#[test]
fn agent_without_tts_cable_finds_nothing() {
    let mut s = AppState::default();
    let agent_id = s.rack.add_module(ModuleKind::LlmAgent);
    let tts_id = s.rack.add_module(ModuleKind::Tts);
    s.tts_modules.push(TtsModuleState::new(tts_id));
    // NO cable wired — agent should not find TTS
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack
                .modules
                .iter()
                .any(|m| m.id == target && m.kind == ModuleKind::Tts)
            {
                s.tts_modules.iter().find(|t| t.id == target)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(found.is_none(), "no cable = no TTS");
}

#[test]
fn disabled_tts_module_not_found() {
    let mut s = AppState::default();
    let agent_id = s.rack.add_module(ModuleKind::LlmAgent);
    let tts_id = s.rack.add_module(ModuleKind::Tts);
    s.tts_modules.push(TtsModuleState::new(tts_id));
    s.rack.connect_control(agent_id, tts_id);
    // Disable the TTS module
    if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == tts_id) {
        m.enabled = false;
    }
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack
                .modules
                .iter()
                .any(|m| m.id == target && m.kind == ModuleKind::Tts && m.enabled)
            {
                s.tts_modules.iter().find(|t| t.id == target)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(found.is_none(), "disabled TTS module should not be found");
}

#[test]
fn multiple_tts_modules_agent_finds_wired_one() {
    let mut s = AppState::default();
    let agent_id = s.rack.add_module(ModuleKind::LlmAgent);
    let tts1 = s.rack.add_module(ModuleKind::Tts);
    let tts2 = s.rack.add_module(ModuleKind::Tts);
    let mut state1 = TtsModuleState::new(tts1);
    state1.temperature = 0.5;
    let mut state2 = TtsModuleState::new(tts2);
    state2.temperature = 1.5;
    s.tts_modules.push(state1);
    s.tts_modules.push(state2);
    // Wire agent to tts2 only
    s.rack.connect_control(agent_id, tts2);
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack
                .modules
                .iter()
                .any(|m| m.id == target && m.kind == ModuleKind::Tts && m.enabled)
            {
                s.tts_modules.iter().find(|t| t.id == target)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(found.is_some());
    let tts = found.unwrap();
    assert_eq!(tts.id, tts2);
    assert!((tts.temperature - 1.5).abs() < 0.01);
}

#[test]
fn tts_module_state_serializes() {
    let tts = TtsModuleState::new(7);
    let json = serde_json::to_string(&tts).unwrap();
    let back: TtsModuleState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, 7);
    assert_eq!(back.voice_ref, "default");
}

// ── Engine switch NeuTTS → Qwen3-TTS (2026-11) ─────────────────────────────
// Sessions and projects saved before the switch serialise the module kind as
// "NeuTts" and the LFO target as "NeuTtsVolume"; both must still load.

#[test]
fn legacy_neutts_module_kind_deserialises_to_tts() {
    let k: ModuleKind = serde_json::from_str("\"NeuTts\"").unwrap();
    assert_eq!(k, ModuleKind::Tts);
    // Older engines' aliases still hold too.
    let k: ModuleKind = serde_json::from_str("\"CoquiTts\"").unwrap();
    assert_eq!(k, ModuleKind::Tts);
    // New saves use the engine-neutral name.
    assert_eq!(serde_json::to_string(&ModuleKind::Tts).unwrap(), "\"Tts\"");
}

#[test]
fn legacy_neutts_volume_lfo_target_deserialises() {
    use crate::state::LfoTarget;
    let t: LfoTarget = serde_json::from_str("\"NeuTtsVolume\"").unwrap();
    assert_eq!(t, LfoTarget::TtsVolume);
}

#[test]
fn sanitize_tts_text_keeps_unicode_drops_control_and_caps_length() {
    use crate::llm::tts::sanitize_tts_text;
    // Non-ASCII survives (the old filter stripped umlauts to "Der Bass ist zurck").
    assert_eq!(
        sanitize_tts_text("Der Bass ist zurück!"),
        "Der Bass ist zurück!"
    );
    // Newlines/tabs become spaces; other control chars vanish; edges trimmed.
    assert_eq!(sanitize_tts_text("\n drop\tit\u{7}  "), "drop it");
    // 200-character cap counts chars, not bytes.
    let long: String = "ü".repeat(500);
    assert_eq!(sanitize_tts_text(&long).chars().count(), 200);
    assert_eq!(sanitize_tts_text(" \n\t "), "");
}

/// Live round-trip through `speak_tts` → scripts/tts-server.py → WAV decode →
/// ring buffer.  Needs the server running on TTS_PORT:
///   .tts-venv/bin/python scripts/tts-server.py &
///   cargo test tts_server_roundtrip -- --ignored
#[test]
#[ignore = "needs a running scripts/tts-server.py on TTS_PORT"]
fn tts_server_roundtrip_pushes_samples() {
    use crate::audio::TtsSink;
    use parking_lot::Mutex;
    use std::sync::Arc;
    let (tx, mut rx) = rtrb::RingBuffer::<f32>::new(48_000 * 30);
    let sink = TtsSink {
        tx: Arc::new(Mutex::new(tx)),
        target_sr: 48_000,
    };
    let mut tts = TtsModuleState::new(1);
    tts.voice_ref = "mc".to_string();
    // Non-ASCII on purpose: the pre-Qwen3 filter stripped it.
    crate::llm::tts::speak_tts("Der Bass ist zurück, Selecta!", &tts, &sink);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut got = 0usize;
    while std::time::Instant::now() < deadline {
        while rx.pop().is_ok() {
            got += 1;
        }
        if got > 48_000 {
            break; // > 1 s of audio at the device rate
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        got > 48_000,
        "only {got} samples arrived from the TTS server"
    );
}

#[test]
fn jungle_setup_reuses_tts_and_enables_one_mc() {
    let mut state = AppState::default();
    state.llm.active_style = Some("jungle".into());
    let state = crate::llm::jungle_mc::configure(state);
    let count = state.tts_modules.len();
    let cables = state.rack.cables.len();
    let state = crate::llm::jungle_mc::configure(state);
    assert_eq!(state.tts_modules.len(), count);
    assert_eq!(state.rack.cables.len(), cables);
    assert_eq!(state.llm_agents[0].conversation_mode, ConversationMode::Mc);
    assert_eq!(state.tts_modules[0].voice_ref, "mc");
}

#[test]
fn jungle_mc_respects_transport_cooldown_mutes_and_overrides() {
    use crate::llm::jungle_mc::{configure, next_call};
    let mut state = AppState::default();
    state.llm.active_style = Some("jungle".into());
    let mut state = configure(state);
    state.sequencer.running = true;
    let aid = Some(state.llm_agents[0].id);
    let first = next_call(&state, aid, None, 0).unwrap();
    assert_eq!(first.0, effective_mc_lines(&state, "jungle")[0]);
    assert!(next_call(&state, aid, Some(89.0), 1).is_none());
    assert_ne!(first.0, next_call(&state, aid, Some(90.0), 1).unwrap().0);
    state.sequencer.running = false;
    assert!(next_call(&state, aid, None, 0).is_none());
    state.sequencer.running = true;
    state.llm_agents[0].conversation_mode = ConversationMode::Off;
    assert!(next_call(&state, aid, None, 0).is_none());
    state.llm_agents[0].conversation_mode = ConversationMode::Mc;
    state
        .rack
        .modules
        .iter_mut()
        .find(|m| m.id == first.1.id)
        .unwrap()
        .enabled = false;
    assert!(next_call(&state, aid, None, 0).is_none());
    state
        .rack
        .modules
        .iter_mut()
        .find(|m| m.id == first.1.id)
        .unwrap()
        .enabled = true;
    state.style_overrides.insert(
        "jungle".into(),
        StyleOverride {
            mc_lines: Some(vec![]),
            themes: None,
        },
    );
    assert!(next_call(&state, aid, None, 0).is_none());
    state.style_overrides.get_mut("jungle").unwrap().mc_lines = Some(vec!["Selector!".into()]);
    assert_eq!(next_call(&state, aid, None, 77).unwrap().0, "Selector!");
    state.rack.cables.retain(|c| c.to.module_id != first.1.id);
    assert!(next_call(&state, aid, None, 0).is_none());
}

#[test]
#[ignore = "starts local Qwen3-TTS and synthesizes speech; requires installed TTS models"]
fn jungle_mc_live_synthesis_reaches_audio_sink() {
    let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(48_000 * 20);
    let sink = crate::audio::TtsSink {
        tx: std::sync::Arc::new(parking_lot::Mutex::new(producer)),
        target_sr: 48_000,
    };
    let mut voice = TtsModuleState::new(1);
    voice.voice_ref = "mc".into();
    crate::llm::speak_tts("Junglist massive! Selector!", &voice, &sink);
    let start = std::time::Instant::now();
    while consumer.slots() == 0 && start.elapsed().as_secs() < 125 {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _producer_guard = sink.tx.lock();
    let mut samples = Vec::new();
    while let Ok(sample) = consumer.pop() {
        samples.push(sample);
    }
    assert!(samples.len() > 4800, "no speech reached the audio sink");
    assert!(samples.iter().all(|s| s.is_finite()));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    assert!(rms > 0.001, "synthesized speech is silent");
    println!("MC speech: {} samples, RMS={rms:.4}", samples.len());
}

#[test]
fn mc_mic_effects_add_grit_and_a_tail_without_master_clipping() {
    let sr = 48_000;
    let input: Vec<f32> = (0..24_000)
        .map(|i| (i as f32 * std::f32::consts::TAU * 220.0 / sr as f32).sin() * 0.2)
        .collect();
    let bypass = crate::llm::tts_effects::process_mic(&input, sr, 0.0, 0.0);
    assert_eq!(bypass, input);
    let driven = crate::llm::tts_effects::process_mic(&input, sr, 0.85, 0.22);
    assert!(driven.len() > input.len());
    assert!(driven.iter().all(|s| s.is_finite() && s.abs() <= 0.9));
    assert!(driven[input.len()..].iter().any(|s| s.abs() > 0.005));
    // Clipping the mic should add a third harmonic, rather than merely
    // turning up the original clean sine wave.
    let harmonic = |samples: &[f32]| {
        let mut real = 0.0;
        let mut imaginary = 0.0;
        for (i, sample) in samples[4800..19200].iter().enumerate() {
            let phase = i as f32 * std::f32::consts::TAU * 660.0 / sr as f32;
            real += sample * phase.cos();
            imaginary += sample * phase.sin();
        }
        (real * real + imaginary * imaginary).sqrt() / 14400.0
    };
    assert!(harmonic(&driven) > harmonic(&input) + 0.005);
    if let Ok(path) = std::env::var("MC_PREVIEW_INPUT") {
        let mut reader = hound::WavReader::open(path).unwrap();
        let sr = reader.spec().sample_rate;
        let input: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect();
        let lower = crate::llm::tts::resample_pitch_shift(&input, -2.0);
        let output = crate::llm::tts_effects::process_mic(&lower, sr, 0.85, 0.22);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: sr,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer =
            hound::WavWriter::create("target/jungle-audio-check/mc-driven.wav", spec).unwrap();
        for sample in output {
            writer
                .write_sample((sample * 0.8 * 32767.0) as i16)
                .unwrap();
        }
        writer.finalize().unwrap();
    }
}

#[test]
fn old_tts_sessions_keep_clean_defaults_and_jungle_preserves_the_backing() {
    let old: TtsModuleState = serde_json::from_value(serde_json::json!({"id":1})).unwrap();
    assert_eq!(
        (old.pitch_st, old.mic_drive, old.room_mix, old.music_duck),
        (0.0, 0.0, 0.0, 0.35)
    );
    let mut state = AppState::default();
    state.llm.active_style = Some("jungle".into());
    let state = crate::llm::jungle_mc::configure(state);
    let params = crate::audio::dsp::AudioParams::from_app_state(&state);
    assert_eq!(params.tts_music_duck, 1.0);
    assert_eq!(state.tts_modules[0].pitch_st, -2.0);
    assert_eq!(state.tts_modules[0].volume, 0.25);
    assert_eq!(state.tts_modules[0].mic_drive, 1.0);
}

#[test]
fn speech_bus_ignores_orphaned_settings_and_disabled_modules() {
    let mut state = AppState::default();
    state.llm.active_style = Some("jungle".into());
    let mut state = crate::llm::jungle_mc::configure(state);
    let mut orphan = TtsModuleState::new(u32::MAX);
    orphan.volume = 1.5;
    orphan.music_duck = 0.1;
    state.tts_modules.insert(0, orphan);
    let params = crate::audio::dsp::AudioParams::from_app_state(&state);
    assert_eq!(params.tts_voice_volume, 0.25);
    assert_eq!(params.tts_music_duck, 1.0);
    for module in &mut state.rack.modules {
        if module.kind == ModuleKind::Tts {
            module.enabled = false;
        }
    }
    let muted = crate::audio::dsp::AudioParams::from_app_state(&state);
    assert_eq!(muted.tts_voice_volume, 0.0);
    assert_eq!(muted.tts_music_duck, 1.0);
}
