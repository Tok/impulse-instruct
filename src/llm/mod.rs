// ─── llm/mod.rs ──────────────────────────────────────────────────────────────
// LLM inference thread.
// Loads a GGUF model and runs continuous inference to control synth params.
// Communicates with UI via crossbeam channels.

pub mod instructions;
pub mod json_repair;
pub mod lane_eval;
pub mod lane_scheduler;
pub mod lanes;
pub mod mock;
pub mod pipeline;
pub mod pipeline_events;
pub mod planner;
pub mod planner_heuristic;
pub mod planner_jam;
pub mod prompt;
pub mod prompt_summary;
pub mod schema;
pub mod styles;
pub mod vram;
pub use mock::mock_response;
use mock::run_mock_loop;
pub use prompt::build_system_prompt;
pub use prompt::param_json_schema;

use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;

use crate::state::{AppState, ConversationMode, apply_llm_update};

pub mod types;
pub use types::{LlmAction, LlmBackend, LlmInput, LlmOutput, SamplingParams};

pub use json_repair::extract_llm_actions;

pub mod server_pool;
pub use server_pool::{LLAMA_BASE_PORT, LlamaServerBackend, LlamaServerPool};

pub fn run_llm_loop(
    state: Arc<RwLock<AppState>>,
    input_rx: Receiver<LlmInput>,
    output_tx: Sender<LlmOutput>,
    mock: bool,
    tts_tx: crate::audio::TtsSink,
) {
    if mock {
        {
            let mut s = state.write();
            s.llm.is_mock = true;
            s.llm.llm_initializing = false;
        }
        log::warn!("Mock mode enabled via --mock flag. No real inference.");
        let _ = output_tx.try_send(LlmOutput {
            text: "[ Mock mode — pass --mock to confirm, or add model + server ]".to_string(),
            param_update: None,
            tokens_per_sec: 0.0,
            prompt_tokens: 0,
            completion_tokens: 0,
            context_used: 0,
            is_jam: false,
            thinking: None,
            mc_line: None,
            before_state: None,
            actions: vec![],
            agent_id: None,
        });
        run_mock_loop(state, input_rx, output_tx);
        return;
    }

    let model_path = state.read().llm.model_path.clone();
    let ctx_size = state.read().llm.context_max;
    let mut pool = LlamaServerPool::new(LLAMA_BASE_PORT, ctx_size);
    let _ = pool.acquire(&model_path);

    if !pool.is_any_live() {
        {
            let mut s = state.write();
            s.llm.is_mock = true;
            s.llm.model_missing = true;
            s.llm.llm_initializing = false;
        }
        log::error!(
            "No model found at '{}'. \
             Download one with: ./scripts/download-models.sh \
             Then select it in Prefs and press Restart.",
            model_path
        );
        let _ = output_tx.try_send(LlmOutput {
            text: "[ No model found — download one with ./scripts/download-models.sh \
                 then select it in Prefs → Model and press Restart.\n\
                 Running without LLM — synth works, prompts ignored. ]"
                .to_string(),
            param_update: None,
            tokens_per_sec: 0.0,
            prompt_tokens: 0,
            completion_tokens: 0,
            context_used: 0,
            is_jam: false,
            thinking: None,
            mc_line: None,
            before_state: None,
            actions: vec![],
            agent_id: None,
        });
        run_mock_loop(state, input_rx, output_tx);
        return;
    }

    {
        let mut s = state.write();
        s.llm.is_mock = false;
        s.llm.llm_initializing = false;
    }
    log::info!("LLM thread started — server live: {}", model_path);
    let _ = output_tx.try_send(LlmOutput {
        text: format!("[ Model loaded: {} ]", model_path),
        param_update: None,
        tokens_per_sec: 0.0,
        prompt_tokens: 0,
        completion_tokens: 0,
        context_used: 0,
        is_jam: false,
        thinking: None,
        mc_line: None,
        before_state: None,
        actions: vec![],
        agent_id: None,
    });

    while let Ok(input) = input_rx.recv() {
        match &input {
            LlmInput::SwitchModel(new_path) => {
                let new_path = new_path.clone();
                let old_path = state.read().llm.model_path.clone();
                log::info!("LLM: switching global model {} -> {}", old_path, new_path);

                // Console acts as master switch: reset every agent override
                // to None so they all inherit the new global on next infer,
                // and unconditionally GC every server in the pool that
                // isn't the new global.  Using shutdown_all_except (instead
                // of per-agent release) makes this robust against the UI's
                // optimistic agent reset — the pool ends up in a clean
                // state regardless of when agent.model_path flipped to None.
                {
                    let mut s = state.write();
                    for a in s.llm_agents.iter_mut() {
                        a.model_path = None;
                    }
                }
                pool.shutdown_all_except(&new_path);

                state.write().llm.model_path = new_path.clone();
                state.write().llm.context_used = 0;
                let live = pool.acquire(&new_path).is_ok() && pool.is_any_live();
                {
                    let mut s = state.write();
                    s.llm.is_mock = !live;
                    s.llm.llm_initializing = false;
                }
                if live {
                    crate::state::save_model_setting(&new_path);
                }
                let status = if live {
                    format!("[ Model loaded: {} ]", new_path)
                } else {
                    format!("[ Model not found: {} — check path and restart ]", new_path)
                };
                state.write().llm.last_response = status.clone();
                let _ = output_tx.try_send(LlmOutput {
                    text: status,
                    param_update: None,
                    tokens_per_sec: 0.0,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    context_used: 0,
                    is_jam: false,
                    thinking: None,
                    mc_line: None,
                    before_state: None,
                    actions: vec![],
                    agent_id: None,
                });
                continue;
            }
            LlmInput::SwitchAgentModel {
                agent_id,
                old_path,
                new_path,
            } => {
                let agent_id = *agent_id;
                let old_path = old_path.clone();
                let new_path = new_path.clone();
                // UI already optimistically wrote `new_path` to state; we
                // just need to keep the pool ref counts in sync with the
                // diff carried by the message.  Defensively make sure
                // state actually reflects `new_path` in case the LLM
                // thread is processing this turn before any UI frame.
                {
                    let mut s = state.write();
                    if let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == agent_id) {
                        a.model_path = new_path.clone();
                    } else {
                        log::warn!("LLM: SwitchAgentModel for unknown agent_id={}", agent_id);
                        continue;
                    }
                }
                if old_path == new_path {
                    continue;
                }
                log::info!(
                    "LLM: agent {} model {:?} -> {:?}",
                    agent_id,
                    old_path,
                    new_path
                );
                if let Some(old) = old_path {
                    pool.release(&old);
                }
                if let Some(new) = new_path
                    && let Err(e) = pool.acquire(&new)
                {
                    log::error!("LLM: agent {} acquire({}) failed: {}", agent_id, new, e);
                }
                continue;
            }
            LlmInput::ResetContext => {
                let model_path = state.read().llm.model_path.clone();
                log::info!("LLM: resetting context (restart with same model)");
                pool.shutdown_model(&model_path);
                state.write().llm.context_used = 0;
                let live = pool.acquire(&model_path).is_ok() && pool.is_any_live();
                {
                    let mut s = state.write();
                    s.llm.is_mock = !live;
                    s.llm.llm_initializing = false;
                }
                let _ = output_tx.try_send(LlmOutput {
                    text: "[ Context reset ]".to_string(),
                    param_update: None,
                    tokens_per_sec: 0.0,
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    context_used: 0,
                    is_jam: false,
                    thinking: None,
                    mc_line: None,
                    before_state: None,
                    actions: vec![],
                    agent_id: None,
                });
                continue;
            }
            LlmInput::Infer { .. } => {}
        }

        let LlmInput::Infer {
            ref prompt,
            one_shot,
            agent_id,
        } = input
        else {
            continue;
        };

        if one_shot {
            log::info!("YOU -> {}", prompt);
        } else {
            log::debug!("YOU (jam) -> {}", prompt);
        }

        // Look up agent state; fall back to singleton for agent_id=None or not found.
        // Also capture per-agent overrides for the system prompt.
        #[allow(clippy::type_complexity)]
        let (
            agent_heat,
            agent_temp,
            agent_scope,
            agent_enable_thinking,
            agent_model,
            agent_conv_mode,
            agent_style,
            agent_custom_style,
            agent_instructions,
            agent_persona,
            agent_prompt_override,
        ) = {
            let s = state.read();
            if let Some(aid) = agent_id {
                if let Some(a) = s.llm_agents.iter().find(|a| a.id == aid) {
                    let model = a
                        .model_path
                        .clone()
                        .unwrap_or_else(|| s.llm.model_path.clone());
                    (
                        a.heat,
                        a.temperature,
                        crate::state::scope_from_control_cables(&s.rack, aid),
                        a.enable_thinking,
                        model,
                        Some(a.conversation_mode.clone()),
                        Some(a.active_style.clone()),
                        Some(a.custom_style_text.clone()),
                        Some(a.user_instructions.clone()),
                        Some(a.persona_name.clone()),
                        Some(a.system_prompt_override.clone()),
                    )
                } else {
                    (
                        s.llm.heat,
                        s.llm.temperature,
                        vec![],
                        s.llm.enable_thinking,
                        s.llm.model_path.clone(),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                }
            } else {
                (
                    s.llm.heat,
                    s.llm.temperature,
                    vec![],
                    s.llm.enable_thinking,
                    s.llm.model_path.clone(),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
            }
        };

        // Ensure the agent's model has a server in the pool.
        let infer_port = match pool.acquire(&agent_model) {
            Ok(port) => port,
            Err(e) => {
                log::error!("Pool: cannot acquire server for {}: {}", agent_model, e);
                // Clear inferring state and send error to UI so the user sees feedback.
                let mut s = state.write();
                s.llm.is_inferring = false;
                if let Some(aid) = agent_id
                    && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
                {
                    a.is_inferring = false;
                }
                let _ = output_tx.try_send(LlmOutput {
                    text: format!("[server error: {}]", e),
                    agent_id,
                    ..LlmOutput::default()
                });
                continue;
            }
        };

        // Set is_inferring early so the UI shows activity immediately
        {
            let mut s = state.write();
            s.llm.is_inferring = true;
            if let Some(aid) = agent_id
                && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
            {
                a.is_inferring = true;
            }
        }

        let agent_label = agent_persona.clone().unwrap_or_else(|| "default".into());
        log::info!(
            "LLM: infer start (agent={}, one_shot={})",
            agent_label,
            one_shot
        );

        // Build sampling params up front — the pipeline branch needs them,
        // and the monolithic path would build identical params anyway.
        let sampling = {
            let s = state.read();
            let agent_seed = agent_id
                .and_then(|aid| s.llm_agents.iter().find(|a| a.id == aid))
                .map(|a| a.seed)
                .unwrap_or(s.llm.seed);
            SamplingParams {
                heat: agent_heat,
                temperature: agent_temp,
                top_k: s.llm.top_k,
                top_p: s.llm.top_p,
                min_p: s.llm.min_p,
                repeat_penalty: s.llm.repeat_penalty,
                frequency_penalty: s.llm.frequency_penalty,
                seed: agent_seed,
            }
        };

        // ── Pipeline branch (default) ───────────────────────────────────────
        // When `use_pipeline` is on, user turns go through planner +
        // per-lane inferences instead of one monolithic call.  Each
        // lane has its own focused prompt + required-fields schema so
        // outputs are short and the model can't skip required arrays.
        // The pipeline drives its own state apply + UI logging; on
        // success we `continue` past the monolithic path below.
        let use_pipeline_this_turn = state.read().llm.use_pipeline;
        if use_pipeline_this_turn {
            log::info!(
                "LLM: starting lane pipeline on port {} (prompt {} chars)",
                infer_port,
                prompt.len()
            );
            let t0 = Instant::now();
            let before_state = Box::new(state.read().clone());

            // Apply agent overrides to the snapshot used as pipeline
            // input — these only affect prompt-building; final state
            // still lands in the real app state.
            let mut snap = state.read().clone();
            if let Some(m) = agent_conv_mode.clone() {
                snap.llm.conversation_mode = m;
            }
            if let Some(s) = agent_style.clone() {
                snap.llm.active_style = s;
            }
            if let Some(c) = agent_custom_style.clone() {
                snap.llm.custom_style_text = c;
            }
            if let Some(p) = agent_persona.clone() {
                snap.llm.persona_name = p;
            }
            snap.llm.heat = agent_heat;

            let mut lanes_ran = 0usize;
            // Per-lane write-back: as soon as a lane applies, copy its
            // sequencer/synth/fx state into the shared app state so the
            // audio thread hears the change immediately instead of
            // waiting for every lane to finish.  Without this the
            // whole turn's worth of patterns switches on simultaneously
            // at the end, which feels blocky.
            let state_for_writeback = state.clone();
            let write_lane_back = move |snapshot: &AppState| {
                let mut s = state_for_writeback.write();
                let step = s.sequencer.current_step;
                s.bass_voices = snapshot.bass_voices.clone();
                s.kit_a = snapshot.kit_a.clone();
                s.kit_b = snapshot.kit_b.clone();
                s.sequencer = snapshot.sequencer.clone();
                s.sequencer.current_step = step;
                s.fx = snapshot.fx.clone();
                s.hoover = snapshot.hoover.clone();
                s.an1x = snapshot.an1x.clone();
                s.noise_voice = snapshot.noise_voice.clone();
                s.lfo = snapshot.lfo;
                // NOTE: do NOT write back `s.rack` from the pipeline's
                // snapshot.  Rack composition is user-owned (style picker,
                // wizard, manual edits) — overwriting it from a stale
                // snapshot here was reverting style-driven rack changes
                // mid-pipeline, so a freshly-applied "Classic Acid"
                // would silently get its full module set restored after
                // the next lane apply.  Lanes that legitimately need to
                // mutate the rack (LaneKind::Rack) write through a
                // different path; routine voice/FX lanes don't.
            };
            let state_for_progress = state.clone();
            let tx_for_cb = output_tx.clone();
            let label_for_cb = agent_label.clone();
            let new_state = pipeline::run_pipeline_via_pool(
                snap,
                prompt,
                &mut pool,
                infer_port,
                &sampling,
                !one_shot, // is_jam — jam cycles use Phase 2 single-lane picker
                |event| {
                    pipeline_events::handle_pipeline_event(
                        event,
                        &state_for_progress,
                        &tx_for_cb,
                        agent_id,
                        &label_for_cb,
                        &mut lanes_ran,
                    )
                },
                write_lane_back,
            );
            let _ = new_state; // per-lane write-back already committed everything
            // Clear inferring + progress flags now that the pipeline is done.
            // Belt-and-braces: PipelineDone clears `pipeline_progress`, but if
            // the pipeline ever returns early without emitting it the bars
            // would otherwise stick.
            {
                let mut s = state.write();
                s.llm.is_inferring = false;
                s.llm.pipeline_progress = None;
                if let Some(aid) = agent_id
                    && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
                {
                    a.is_inferring = false;
                    a.pipeline_progress = None;
                }
            }
            let elapsed = t0.elapsed().as_secs_f32();
            let _ = output_tx.try_send(LlmOutput {
                text: format!("[pipeline: {} lanes applied in {:.1}s]", lanes_ran, elapsed),
                agent_id,
                before_state: Some(before_state),
                ..LlmOutput::default()
            });
            // Jam loop hand-off: jam cycles (one_shot=false) need the
            // `[jam_cycle_done]` signal so `drain_llm_outputs` re-fires
            // the next turn on the round-robin agent.  The monolithic
            // path sends this at the bottom of the loop; our `continue`
            // above skips that path, so we mirror it here.  One-shot
            // user prompts never need this (they're not in a jam).
            if !one_shot {
                let _ = output_tx.try_send(LlmOutput {
                    text: "[jam_cycle_done]".to_string(),
                    is_jam: true,
                    ..LlmOutput::default()
                });
            }
            // Release the per-inference acquire (paired with the acquire at
            // `pool.acquire(&agent_model)` above).  Without this, the pool's
            // ref_count grows on every inference and servers never unload —
            // so model switches never reclaim VRAM.
            pool.release(&agent_model);
            continue;
        }

        // ── Monolithic path (legacy, `use_pipeline=false`) ──────────────────
        // Build system prompt with per-agent overrides patched in.
        log::debug!("LLM: building system prompt...");
        let system = {
            let mut snap = state.read().clone();
            log::debug!("LLM: state snapshot done");
            if let Some(mode) = agent_conv_mode {
                snap.llm.conversation_mode = mode;
            }
            if let Some(style) = agent_style {
                snap.llm.active_style = style;
            }
            if let Some(custom) = agent_custom_style {
                snap.llm.custom_style_text = custom;
            }
            if let Some(instr) = agent_instructions {
                snap.llm.user_instructions = instr;
            }
            if let Some(persona) = agent_persona.clone() {
                snap.llm.persona_name = persona;
            }
            if let Some(override_text) = agent_prompt_override
                && !override_text.is_empty()
            {
                snap.llm.system_prompt_override = override_text;
            }
            snap.llm.heat = agent_heat;
            let (agent_memory, style_obs, hints) = agent_id
                .and_then(|aid| snap.llm_agents.iter().find(|a| a.id == aid))
                .map(|a| {
                    (
                        a.memory.clone(),
                        a.style_observations.clone(),
                        a.pending_hints.clone(),
                    )
                })
                .unwrap_or_default();
            // Clear pending hints after reading
            if let Some(aid) = agent_id {
                let mut s = state.write();
                if let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid) {
                    a.pending_hints.clear();
                }
            }
            // Combine memory with hints for the prompt
            let mut full_memory = agent_memory;
            for h in &hints {
                full_memory.push(format!("[hint from another agent] {}", h));
            }
            log::debug!("LLM: calling build_system_prompt_full...");
            let result =
                prompt::build_system_prompt_full(&snap, &agent_scope, &full_memory, &style_obs);
            log::debug!("LLM: system prompt built ({} chars)", result.len());
            result
        };
        {
            state.write().llm.last_prompt = prompt.clone();
        }

        let enable_thinking = agent_enable_thinking;
        let think_prompt = format!(
            "{} {}",
            prompt,
            if enable_thinking {
                "/think"
            } else {
                "/no_think"
            }
        );
        log::info!(
            "LLM: sending inference to port {} ({} system chars, {} prompt chars)",
            infer_port,
            system.len(),
            think_prompt.len()
        );
        let t0 = Instant::now();
        let result = pool.infer(infer_port, &system, &think_prompt, &sampling);
        let elapsed = t0.elapsed().as_secs_f32();
        log::info!(
            "LLM: inference returned after {:.1}s (ok={})",
            elapsed,
            result.is_ok()
        );

        match result {
            Ok(output) => {
                let tps = output.tokens_per_sec;
                let ptok = output.prompt_tokens;
                let ctok = output.completion_tokens;
                let ctx = output.context_used;
                let tthink = output.thinking.as_ref().map(|t| t.len() / 4).unwrap_or(0);
                log::info!(
                    "LLM: infer done (agent={}, {:.1}t/s, prompt={}, completion={}, ctx={}, think~{})",
                    agent_label,
                    tps,
                    ptok,
                    ctok,
                    ctx,
                    tthink
                );

                log::trace!("LLM JSON: {}", output.text);

                // Apply LLM update: snapshot OUTSIDE the lock, apply, then
                // selectively write back under a short lock.
                let before_state = if let Some(ref update) = output.param_update {
                    let t0 = Instant::now();
                    // Snapshot outside the write lock to minimize hold time.
                    let current = state.read().clone();
                    let before = Box::new(current.clone());
                    log::info!(
                        "LLM apply: snapshot took {:.1}ms",
                        t0.elapsed().as_secs_f64() * 1000.0
                    );

                    let t1 = Instant::now();
                    let next = apply_llm_update(current, update, &agent_scope);
                    log::info!(
                        "LLM apply: apply_llm_update took {:.1}ms",
                        t1.elapsed().as_secs_f64() * 1000.0
                    );

                    // Short write lock: only copy synth-parameter fields.
                    let t2 = Instant::now();
                    {
                        let mut s = state.write();
                        let step = s.sequencer.current_step;
                        s.bass_voices = next.bass_voices;
                        s.kit_a = next.kit_a;
                        s.kit_b = next.kit_b;
                        s.sequencer = next.sequencer;
                        s.sequencer.current_step = step;
                        s.fx = next.fx;
                        s.hoover = next.hoover;
                        s.an1x = next.an1x;
                        s.noise_voice = next.noise_voice;
                        s.lfo = next.lfo;
                    } // write lock released here
                    log::info!(
                        "LLM apply: write-back took {:.1}ms",
                        t2.elapsed().as_secs_f64() * 1000.0
                    );

                    Some(before)
                } else {
                    None
                };

                log::info!(
                    "LLM apply: update done, has_param_update={}",
                    output.param_update.is_some()
                );
                {
                    let mut s = state.write();
                    log::debug!("LLM: setting is_inferring=false");
                    s.llm.is_inferring = false;
                    s.llm.tokens_per_sec = tps;
                    s.llm.prompt_tokens = ptok;
                    s.llm.completion_tokens = ctok;
                    s.llm.context_used = ctx;
                    s.llm.thinking_tokens = tthink;
                    s.llm.last_response = output.text.clone();
                }

                // Auto-compact: if context is > 85% full and the setting is on,
                // restart the server now so the *next* inference starts fresh.
                {
                    let s = state.read();
                    let pct = if s.llm.context_max > 0 {
                        ctx as f32 / s.llm.context_max as f32
                    } else {
                        0.0
                    };
                    if s.llm.auto_compact && pct >= 0.85 {
                        drop(s);
                        log::info!(
                            "LLM: context {:.0}% full — auto-compact: restarting server for {}",
                            pct * 100.0,
                            agent_model,
                        );
                        pool.shutdown_model(&agent_model);
                        state.write().llm.context_used = 0;
                        let live = pool.acquire(&agent_model).is_ok() && pool.is_any_live();
                        state.write().llm.is_mock = !live;
                        let _ = output_tx.try_send(LlmOutput {
                            text: "[ Context auto-compacted ]".to_string(),
                            param_update: None,
                            tokens_per_sec: 0.0,
                            prompt_tokens: 0,
                            completion_tokens: 0,
                            context_used: 0,
                            is_jam: false,
                            thinking: None,
                            mc_line: None,
                            before_state: None,
                            actions: vec![],
                            agent_id: None,
                        });
                    }
                }

                if let Some(ref update) = output.param_update {
                    let comment = update
                        .get("_comment")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&output.text);
                    let persona = agent_persona
                        .clone()
                        .unwrap_or_else(|| state.read().llm.persona_name.clone());
                    if !one_shot {
                        log::debug!("{} (jam): {}", persona, comment);
                    }
                }

                // ── TTS via rack cable ────────────────────────────────────
                // Trigger whenever the agent produced a spoken line — this
                // must NOT be gated on param_update, since an MC/DJ agent
                // commonly emits mc_line without changing any params.
                if output.mc_line.is_some() {
                    let mode = if let Some(aid) = agent_id {
                        state
                            .read()
                            .llm_agents
                            .iter()
                            .find(|a| a.id == aid)
                            .map(|a| a.conversation_mode.clone())
                            .unwrap_or_default()
                    } else {
                        state.read().llm.conversation_mode.clone()
                    };
                    let tts_mode = matches!(mode, ConversationMode::Mc | ConversationMode::Dj);
                    if tts_mode {
                        let s = state.read();
                        let src_id = agent_id.unwrap_or(0);
                        let tts_mod = s.rack.cables.iter().find_map(|c| {
                            if c.from.module_id == src_id
                                && c.from.kind == crate::state::PortKind::Control
                            {
                                let target = c.to.module_id;
                                if s.rack.modules.iter().any(|m| {
                                    m.id == target
                                        && m.kind == crate::state::ModuleKind::NeuTts
                                        && m.enabled
                                }) {
                                    s.tts_modules.iter().find(|t| t.id == target)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        });
                        if let Some(tts_mod) = tts_mod {
                            let tts_text = output.mc_line.as_deref().unwrap_or_default();
                            log::info!("[TTS] module={}: {}", tts_mod.id, tts_text);
                            speak_neutts(tts_text, tts_mod, &tts_tx);
                        } else {
                            log::warn!(
                                "[TTS] agent {} emitted mc_line but no NeuTts module is cable-wired from it",
                                src_id
                            );
                        }
                    }
                }
                let mut output = output;
                output.is_jam = !one_shot;
                output.before_state = before_state;
                output.agent_id = agent_id;
                // Write stats back to agent
                if let Some(aid) = agent_id {
                    let mut s = state.write();
                    if let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid) {
                        a.is_inferring = false;
                        a.tokens_per_sec = output.tokens_per_sec;
                        a.last_response = output.text.clone();
                        a.jam_cycle_count = a.jam_cycle_count.saturating_add(1);
                        // Persist a memory snippet from this response
                        if let Some(ref update) = output.param_update
                            && let Some(comment) = update.get("_comment").and_then(|v| v.as_str())
                        {
                            a.memory.push(comment.to_string());
                            if a.memory.len() > crate::state::AGENT_MEMORY_MAX {
                                a.memory
                                    .drain(..a.memory.len() - crate::state::AGENT_MEMORY_MAX);
                            }
                        }
                    }
                }
                let _ = output_tx.try_send(output);
                log::debug!("inference complete in {:.2}s", elapsed);
            }
            Err(e) => {
                log::error!("LLM inference error: {}", e);
                let mut s = state.write();
                s.llm.is_inferring = false;
                s.llm.last_response = format!("Error: {}", e);
                if let Some(aid) = agent_id
                    && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
                {
                    a.is_inferring = false;
                }
            }
        }

        // Release the per-inference acquire — paired with the acquire above.
        // Without this the pool ref_count leaks every inference.
        pool.release(&agent_model);

        if one_shot {
            // wait for next prompt
        } else {
            // NOTE: an old `let _ = input_rx.try_recv();` lived here as
            // (apparently) jam-message dedup, but it would also discard
            // the next user prompt or control message that happened to
            // be queued — silent command loss.  The jam loop drives
            // itself via the `[jam_cycle_done]` output below.
            let _ = output_tx.send(LlmOutput {
                text: "[jam_cycle_done]".to_string(),
                param_update: None,
                tokens_per_sec: 0.0,
                prompt_tokens: 0,
                completion_tokens: 0,
                context_used: 0,
                is_jam: true,
                thinking: None,
                mc_line: None,
                before_state: None,
                actions: vec![],
                agent_id: None,
            });
        }
    }

    log::info!("LLM thread exiting");
}

pub mod tts;
pub use tts::speak_neutts;
