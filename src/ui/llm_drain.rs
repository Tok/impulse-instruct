// ─── ui/llm_drain.rs ── pull LlmOutput messages off the channel ──────────────
// Handles thinking-token echo, log/activity-line formatting, jam re-fire
// scheduling, and the LlmAction switch (style / persona / spawn / dismiss / …).

use super::{ActivityAction, ActivityEntry, ImpulseApp, ansi_colorize_notes, ui_helpers};
use crate::llm::LlmInput;

impl ImpulseApp {
    pub(super) fn drain_llm_outputs(&mut self) {
        while let Ok(out) = self.llm_rx.try_recv() {
            log::info!(
                "UI: drain_llm_output received (has_update={}, text_len={})",
                out.param_update.is_some(),
                out.text.len()
            );
            // Resolve persona: agent-specific name when available, else singleton.
            let persona_name = out
                .agent_id
                .and_then(|aid| {
                    let s = self.state.read();
                    s.llm_agents
                        .iter()
                        .find(|a| a.id == aid)
                        .map(|a| a.persona_name.clone())
                })
                .unwrap_or_else(|| self.state.read().llm.persona_name.clone());

            // Store thinking tokens for display; also echo to log if enabled.
            if let Some(ref thinking) = out.thinking
                && !thinking.is_empty()
            {
                self.last_thinking = Some(thinking.clone());
                let think_persona = persona_name.clone();
                log::info!(
                    "{} (thinking): {}",
                    think_persona,
                    ansi_colorize_notes(thinking)
                );
                if self.state.read().llm.show_thinking_in_log {
                    self.log_text
                        .push_str(&format!("{} (thinking): {}\n", think_persona, thinking));
                }
            }

            if !out.is_jam
                && (out.param_update.is_some()
                    || (!out.text.is_empty() && !out.text.starts_with('[')))
            {
                let conv_mode = self.state.read().llm.conversation_mode.clone();
                let display = crate::state::format_llm_display(
                    out.param_update.as_ref(),
                    &out.text,
                    &conv_mode,
                );
                // Append thinking indicator when present; tag audio-feedback responses
                let persona = if self.listen_pending {
                    self.listen_pending = false;
                    "LISTEN".to_string()
                } else {
                    persona_name.clone()
                };
                let line = if out.thinking.as_ref().is_some_and(|t| !t.is_empty()) {
                    format!("{}: {} [think]\n", persona, display)
                } else {
                    format!("{}: {}\n", persona, display)
                };
                log::info!("{}", ansi_colorize_notes(line.trim_end()));
                self.log_text.push_str(&line);
                let action = if out.param_update.is_some() {
                    ActivityAction::ParamUpdate
                } else {
                    ActivityAction::Response
                };
                self.activity_log.push(ActivityEntry {
                    timestamp: std::time::Instant::now(),
                    persona: persona.clone(),
                    action,
                    detail: display,
                });
                self.trim_activity_log();
                if let Some(ref mc) = out.mc_line {
                    self.log_text.push_str(&format!("► {}\n", mc));
                    log::info!("► {}", mc);
                }
            }
            // Jam re-triggers unless heat is at zero (model is parked).
            if out.text == "[jam_cycle_done]" && self.state.read().llm.heat > 0.0 {
                {
                    // Advance ramps selectively (don't full-replace state — would overwrite API/rack edits).
                    let cur = self.state.read().clone();
                    let next = crate::state::jam_tools::advance_ramps(cur);
                    let mut s = self.state.write();
                    let step = s.sequencer.current_step;
                    s.bass_voices = next.bass_voices;
                    s.kit_a = next.kit_a;
                    s.kit_b = next.kit_b;
                    s.sequencer = next.sequencer;
                    s.sequencer.current_step = step;
                    s.fx = next.fx;
                    s.lfo = next.lfo;
                }
                self.state.write().llm.jam_cycle_count =
                    self.state.read().llm.jam_cycle_count.saturating_add(1);
                if self.auto_listen {
                    self.auto_listen_counter += 1;
                    if self.auto_listen_counter >= 4 {
                        self.auto_listen_counter = 0;
                        self.trigger_listen();
                    }
                }
                // Round-robin: pick next enabled agent
                let (next_id, jam_bars, bpm) = {
                    let s = self.state.read();
                    let agents = &s.llm_agents;
                    let enabled: Vec<_> = agents
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| s.rack.modules.iter().any(|m| m.id == a.id && m.enabled))
                        .collect();
                    if enabled.is_empty() {
                        (None, s.llm.jam_bars, s.sequencer.bpm)
                    } else {
                        let idx = self.jam_next_agent % enabled.len();
                        self.jam_next_agent = idx + 1;
                        let agent = enabled[idx].1;
                        (Some(agent.id), agent.jam_bars, s.sequencer.bpm)
                    }
                };
                if jam_bars > 0.0 && bpm > 0.0 {
                    let delay_ms = (jam_bars * 240_000.0 / bpm) as u64;
                    self.jam_next_fire = Some((
                        std::time::Instant::now() + std::time::Duration::from_millis(delay_ms),
                        next_id,
                    ));
                } else if let Some(aid) = next_id {
                    let _ = self.llm_tx.try_send(LlmInput::Infer {
                        prompt: "continue jamming, evolve the pattern".to_string(),
                        one_shot: false,
                        agent_id: Some(aid),
                    });
                }
            }
            use crate::llm::LlmAction;
            for action in &out.actions {
                match action {
                    LlmAction::SaveProject => {
                        let msg = match crate::state::save_project(&self.state.read().clone()) {
                            Ok(p) => format!("Saved → {}\n", p.display()),
                            Err(e) => format!("Save failed: {e}\n"),
                        };
                        self.log_text.push_str(&msg);
                    }
                    LlmAction::SetStyle(sid) if !out.is_jam => {
                        // Respect style lock — agents can't override user-selected style
                        if !self.state.read().llm.style_lock {
                            use crate::llm::styles::StyleCatalog;
                            let cat = StyleCatalog::get();
                            let resolved = cat
                                .find_by_id(sid)
                                .or_else(|| {
                                    let lo = sid.to_lowercase();
                                    cat.styles().iter().find(|s| {
                                        s.id.to_lowercase() == lo || s.name.to_lowercase() == lo
                                    })
                                })
                                .map(|s| s.id.clone());
                            if let Some(id) = resolved {
                                self.state.write().llm.active_style = Some(id);
                            }
                            self.session_dirty = true;
                        }
                    }
                    LlmAction::SetHeat(_) => {} // heat is user-only; always ignore
                    LlmAction::SetStyle(_) => {} // jam: ignore
                    LlmAction::SetPersona(p) => {
                        self.state.write().llm.persona_name = p.clone();
                        self.session_dirty = true;
                    }
                    LlmAction::SetConversationMode(m) => {
                        use crate::state::ConversationMode;
                        let mode = match m.to_lowercase().as_str() {
                            "off" => ConversationMode::Off,
                            "dj" => ConversationMode::Dj,
                            "mc" => ConversationMode::Mc,
                            _ => ConversationMode::Producer,
                        };
                        self.state.write().llm.conversation_mode = mode;
                        self.session_dirty = true;
                    }
                    LlmAction::SetJamBars(b) => {
                        self.state.write().llm.jam_bars = *b;
                        self.session_dirty = true;
                    }
                    LlmAction::SpawnAgent {
                        persona,
                        scope,
                        model,
                        mode,
                        tts,
                    } => {
                        let s = self.state.read();
                        let ok = s.llm.agent_autonomy
                            && out
                                .agent_id
                                .and_then(|aid| s.llm_agents.iter().find(|a| a.id == aid))
                                .map(|a| a.can_spawn)
                                .unwrap_or(true);
                        // VRAM budget check
                        let vram_ok = !ok || {
                            let vram_total = self
                                .sys_info
                                .lock()
                                .ok()
                                .map(|si| si.vram_total_mb)
                                .unwrap_or(0);
                            !crate::llm::vram::would_exceed_vram(
                                &s.llm_agents,
                                &s.llm.model_path,
                                model.as_deref(),
                                vram_total,
                            )
                        };
                        if !vram_ok {
                            log::warn!(
                                "Rejected agent spawn '{}': would exceed VRAM budget",
                                persona
                            );
                        }
                        drop(s);
                        if ok && vram_ok {
                            self.push_history();
                            let snapshot = self.state.read().clone();
                            let (spawned, agent_id) = crate::state::spawn_agent(
                                snapshot,
                                persona,
                                scope,
                                crate::state::AgentRole::Producer,
                                model.clone(),
                            );
                            let new_state = crate::state::apply_agent_mode_and_tts(
                                spawned,
                                agent_id,
                                mode.as_deref(),
                                *tts,
                            );
                            *self.state.write() = new_state;
                            let tts_tag = if *tts { " + TTS" } else { "" };
                            self.log_text.push_str(&format!(
                                "Agent spawned: {persona} ({scope:?}){tts_tag}\n"
                            ));
                            self.session_dirty = true;
                        }
                    }
                    LlmAction::DismissAgent => {
                        if let Some(aid) = out.agent_id {
                            let s = self.state.read();
                            let ok = s.llm.agent_autonomy
                                && s.llm_agents
                                    .iter()
                                    .find(|a| a.id == aid)
                                    .map(|a| a.can_dismiss)
                                    .unwrap_or(false);
                            let count = s.llm_agents.len();
                            let name = s
                                .llm_agents
                                .iter()
                                .find(|a| a.id == aid)
                                .map(|a| a.persona_name.clone())
                                .unwrap_or_default();
                            drop(s);
                            if ok && count > 1 {
                                self.push_history();
                                self.state.write().rack.remove_module(aid);
                                self.state.write().llm_agents.retain(|a| a.id != aid);
                                self.push_fx_plan();
                                self.log_text.push_str(&format!("{} signed off\n", name));
                                if self.state.read().llm_agents.len() == 1 {
                                    self.state.write().llm_agents[0].scope.clear();
                                }
                                self.session_dirty = true;
                            }
                        }
                    }
                    LlmAction::SendHint { to, hint } => {
                        let mut s = self.state.write();
                        if let Some(target) = s
                            .llm_agents
                            .iter_mut()
                            .find(|a| a.persona_name.eq_ignore_ascii_case(to))
                        {
                            target.pending_hints.push(hint.clone());
                            // Cap at 5 pending hints
                            if target.pending_hints.len() > 5 {
                                target.pending_hints.drain(..target.pending_hints.len() - 5);
                            }
                        }
                    }
                }
            }
            // Push updated params after LLM changed state; record the pre-update
            // snapshot to the undo history so Ctrl+Z can revert an LLM response.
            if out.param_update.is_some() {
                if let Some(before) = out.before_state {
                    self.history.push(*before); // snapshot taken by LLM thread pre-update
                } else {
                    self.push_history(); // fallback: snapshot current state
                }
                self.push_audio_params();
                log::debug!("UI: LLM output processed, audio params pushed");

                // Highlight (and optionally scroll to) the module affected by this update.
                if let Some(kind) = ui_helpers::llm_update_focus_kind(out.param_update.as_ref()) {
                    self.focused_module = Some(kind);
                    self.focus_time = std::time::Instant::now();
                    if self.state.read().ui_prefs.llm_auto_scroll {
                        self.state.write().scroll_target =
                            Some(kind.default_zone().scroll_name().to_string());
                    }
                }
            }
        }
        log::trace!("UI: drain_llm_outputs complete");
    }
}
