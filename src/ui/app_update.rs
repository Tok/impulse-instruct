// ─── ui/app_update.rs ────────────────────────────────────────────────────────
// `impl eframe::App for ImpulseApp` — the per-frame `update` hook plus
// the session-save `save` hook.  Lifted out of ui/mod.rs to keep the
// entry module under the 1000-line cap; mod.rs now holds the struct
// definition + its inherent `impl ImpulseApp` block, while all frame-
// driven drawing / input / message-draining lives here.
//
// Keeps the full `update` body in one file so debugging a frame-loop
// issue doesn't need a hunt across sub-modules.

use egui::{CentralPanel, Frame, TopBottomPanel};

use super::{
    DRUM_LOG_CAP, DrumLogEntry, ImpulseApp, MELODIC_LOG_CAP, MelodicLogEntry, panels, rack_canvas,
    scope_footer, theme, util,
};

impl eframe::App for ImpulseApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let s = self.state.read().clone();
        if let Ok(json) = serde_json::to_string(&s) {
            storage.set_string("session", json);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Capture the system's native pixels_per_point on the first real frame,
        // after the window is shown and DPI is known.
        if self.native_ppp <= 0.0 {
            self.native_ppp = ctx.pixels_per_point();
            // Stop sequencer while wizard is visible so no sound plays before user decides.
            if self.show_wizard && self.state.read().sequencer.running {
                self.state.write().sequencer.running = false;
                self.push_audio_params();
            }
        }
        // Context-sensitive Ctrl+MW zoom: per-module over cards, global elsewhere.
        let cg = self.state.read().ui_prefs.ui_scale;
        match util::detect_ctrl_zoom(ctx, &self.module_scales, cg) {
            Some(util::ZoomTarget::Kind(kind, s)) => {
                self.module_scales.insert(kind, s);
                self.session_dirty = true;
            }
            Some(util::ZoomTarget::Global(s)) => {
                self.state.write().ui_prefs.ui_scale = s;
                self.session_dirty = true;
            }
            None => {}
        }
        ctx.set_pixels_per_point(self.native_ppp * self.state.read().ui_prefs.ui_scale);

        // Publish touch_mode + WASD flag so widgets can read them without signature changes.
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("touch_mode"), self.touch_mode);
            d.insert_temp(
                egui::Id::new("wasd_as_arrows"),
                self.state.read().ui_prefs.wasd_as_arrows,
            );
            d.insert_temp(egui::Id::new("ctrl_locked"), self.ctrl_locked);
        });

        self.drain_llm_outputs();
        self.drain_api_log();
        self.drain_midi_events();
        // Poll API params_dirty flag — push audio params when API changed state
        if self
            .api_params_dirty
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            self.push_audio_params();
        }

        // ── Auto-save: write session.json if the state changed ──────────────
        // (moved here to diagnose hang — see if save blocks)
        if let Some((fire_at, pending_agent)) = self.jam_next_fire {
            if std::time::Instant::now() >= fire_at {
                self.jam_next_fire = None;
                if self.state.read().llm.heat > 0.0 {
                    self.send_llm_infer(self.jam_prompt_for_active_style(), false, pending_agent);
                }
            } else {
                // Still waiting — request a repaint so we check again next frame
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }

        // ── Jam-loop heartbeat ──────────────────────────────────────────────
        // The jam loop self-perpetuates via [jam_cycle_done], but nothing
        // kicks off the *first* cycle when heat goes 0 → >0 (slider move
        // or app startup with a saved heat).  Detect "heat > 0 + dormant"
        // and fire one Infer to spark it.  Cooldown stops re-fires while
        // the LLM thread is still picking up the message.
        {
            let (heat, initializing, any_inferring) = {
                let s = self.state.read();
                (
                    s.llm.heat,
                    s.llm.llm_initializing,
                    s.llm.is_inferring || s.llm_agents.iter().any(|a| a.is_inferring),
                )
            };
            let dormant = !any_inferring && self.jam_next_fire.is_none();
            let cooldown = std::time::Duration::from_millis(500);
            let cooldown_left = cooldown.saturating_sub(self.last_jam_kickoff.elapsed());
            if heat > 0.0 && dormant && !initializing {
                if cooldown_left.is_zero() {
                    let next = {
                        let s = self.state.read();
                        // Sleeping agents are skipped — they save VRAM
                        // by parking until explicitly woken.  Disabled
                        // rack modules have always been excluded; sleep
                        // is the in-app analogue.
                        let enabled: Vec<u32> = s
                            .llm_agents
                            .iter()
                            .filter(|a| {
                                !a.sleeping
                                    && s.rack.modules.iter().any(|m| m.id == a.id && m.enabled)
                            })
                            .map(|a| a.id)
                            .collect();
                        if enabled.is_empty() {
                            None
                        } else {
                            let idx = self.jam_next_agent % enabled.len();
                            self.jam_next_agent = idx + 1;
                            Some(enabled[idx])
                        }
                    };
                    if let Some(aid) = next {
                        self.send_llm_infer(self.jam_prompt_for_active_style(), false, Some(aid));
                        self.last_jam_kickoff = std::time::Instant::now();
                    }
                } else {
                    // Wake up exactly when the cooldown expires so the
                    // heartbeat can fire without waiting for some other
                    // event to repaint us.
                    ctx.request_repaint_after(cooldown_left);
                }
            }
        }

        // ── Auto-save session when rack or key settings change ────────────────
        {
            let s = self.state.read();
            let rack = &s.rack;
            let sig = (
                rack.modules.len() + rack.cables.len() * 100,
                rack.modules
                    .iter()
                    .map(|m| m.slot as usize + m.enabled as usize * 1000)
                    .sum::<usize>(),
            );
            if sig != self.last_saved_rack_sig {
                self.last_saved_rack_sig = sig;
                self.session_dirty = true;
            }
            // Model selections (global + per-agent overrides) — the rack sig
            // doesn't catch these because they don't change module/cable count.
            let model_sig = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                s.llm.model_path.hash(&mut h);
                for a in &s.llm_agents {
                    a.model_path.hash(&mut h);
                }
                h.finish()
            };
            if model_sig != self.last_saved_model_sig {
                self.last_saved_model_sig = model_sig;
                self.session_dirty = true;
            }
        }
        if self.session_dirty {
            use crate::state::AutosaveInterval;
            let interval = self.state.read().ui_prefs.autosave_interval;
            let should_save = match interval {
                AutosaveInterval::Manual => false,
                AutosaveInterval::Immediate => true,
                _ => interval
                    .duration()
                    .map(|d| self.last_save_time.elapsed() >= d)
                    .unwrap_or(false),
            };
            if should_save {
                let state = self.state.read().clone();
                crate::state::save_session(&state, self.show_cables, self.rack_flipped);
                self.session_dirty = false;
                self.last_save_time = std::time::Instant::now();
            }
        }

        // ── Undo / redo (Ctrl+Z / Ctrl+Y or Ctrl+Shift+Z) ────────────────────
        let undo = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::Z) && !i.modifiers.shift
        });
        let redo = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::Y)
                || (i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z))
        });
        if undo {
            let current = self.state.read().clone();
            if let Some(prev) = self.history.undo(current) {
                *self.state.write() = prev;
                self.push_audio_params();
            }
        } else if redo {
            let current = self.state.read().clone();
            if let Some(next) = self.history.redo(current) {
                *self.state.write() = next;
                self.push_audio_params();
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
            self.toggle_rack_flip();
        }
        let api_flip = self.state.write().rack_flip_requested.take();
        if let Some(show_back) = api_flip
            && show_back != self.rack_flipped
        {
            self.toggle_rack_flip();
        }
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::SHIFT, egui::Key::Slash) // Shift+/ = ?
                || i.consume_key(egui::Modifiers::NONE, egui::Key::F1)
        }) {
            self.show_shortcuts = !self.show_shortcuts;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
            let mut s = self.state.write();
            s.ui_prefs.performance_mode = !s.ui_prefs.performance_mode;
            self.session_dirty = true;
        }
        // Pattern snapshot slots — Shift+1..=4 instantly load pattern
        // bank slots 0..=3 without saving the current edits, so a
        // live performer can flip A/B/C/D at any time.  Right-click
        // on the bank strip cells (or the bank-write API) is still
        // the way to capture into a slot.
        for (key, slot) in [
            (egui::Key::Num1, 0usize),
            (egui::Key::Num2, 1),
            (egui::Key::Num3, 2),
            (egui::Key::Num4, 3),
        ] {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, key)) {
                let s = self.state.read().clone();
                *self.state.write() = crate::state::bank_load(s, slot, true);
                self.push_audio_params();
            }
        }

        // ── Startup hook — auto-prompt after wizard closes ──────────────────
        // Once the wizard is dismissed and the LLM is ready, send a one-shot
        // prompt to get a basic pattern going so the track isn't silent.
        if !self.startup_done && !self.show_wizard && !self.state.read().llm.llm_initializing {
            self.startup_done = true;
            // Auto-sync rack to active style — opt-in preference.  When
            // on and a genre style is active, reshape the rack to match
            // the style's `rack_modules` so restarting in Classic Acid
            // never leaves a Hoover in the rack from a prior session.
            // Skip for `__free__` / `__custom__` (no rack_modules to
            // sync to) and when no style is set.
            let (autosync, style_modules) = {
                let s = self.state.read();
                let sync = s.ui_prefs.autosync_rack_on_start;
                let modules = match s.llm.active_style.as_deref() {
                    Some("__free__") | Some("__custom__") | None => None,
                    Some(id) => crate::llm::styles::StyleCatalog::get()
                        .find_by_id(id)
                        .map(|st| st.rack_modules.clone()),
                };
                (sync, modules)
            };
            if autosync
                && let Some(modules) = style_modules
                && !modules.is_empty()
            {
                crate::ui::style_rack::apply(self, &modules);
                log::info!(
                    "startup: auto-synced rack to active style ({} modules)",
                    modules.len()
                );
                self.log_text
                    .push_str("[ rack auto-synced to active style ]\n");
            }
            let has_agents = !self.state.read().llm_agents.is_empty();
            if has_agents {
                // Phrase the prompt in terms of the user's selected style
                // instead of asking the model to "pick a style".  When no
                // style is set we fall back to a neutral phrasing so the
                // sentence still reads cleanly.
                let style_label = {
                    let s = self.state.read();
                    match s.llm.active_style.as_deref() {
                        Some("__free__") | None => None,
                        Some("__custom__") => {
                            let t = s.llm.custom_style_text.trim();
                            if t.is_empty() {
                                None
                            } else {
                                Some(t.to_string())
                            }
                        }
                        Some(id) => crate::llm::styles::StyleCatalog::get()
                            .find_by_id(id)
                            .map(|s| s.name.clone()),
                    }
                };
                let head = match style_label {
                    Some(name) => format!("Create a pattern in the style of {}.", name),
                    None => "Create a starter pattern.".to_string(),
                };
                let prompt = format!(
                    "{head} Bass line should use 3-5 different notes but leave gaps — not \
                     every step needs a note. Use accent and slide on some steps. Add a \
                     kick pattern and hi-hats. Set the filter to something interesting. \
                     Set pan positions for stereo width and add subtle chorus."
                );
                self.send_llm_infer(prompt, true, None);
                self.log_text
                    .push_str("AUTO → startup prompt sent, generating initial pattern…\n");
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        // ── Drain scope + DSP load ring buffers ──────────────────────────────
        while let Ok(s) = self.scope_rx.pop() {
            self.scope_buf.push(s);
        }
        if self.scope_buf.len() > 512 {
            let drain = self.scope_buf.len() - 512;
            self.scope_buf.drain(..drain);
        }
        if self.scope_buf.len() >= 64 {
            // phosphor persistence — frame count from preferences
            self.scope_history.push_back(self.scope_buf.clone());
            let max_frames = self.state.read().ui_prefs.phosphor_frames.clamp(2, 20);
            if self.scope_history.len() > max_frames {
                self.scope_history.pop_front();
            }
        }
        self.update_spectrum();
        // Stereo correlation meter
        while let Ok(s) = self.stereo_rx.pop() {
            self.stereo_buf.push(s);
        }
        if self.stereo_buf.len() > 4096 {
            self.stereo_buf.drain(..self.stereo_buf.len() - 4096);
        }
        if self.stereo_buf.len() >= 200 {
            let (c, b) = crate::audio::analysis::stereo_correlation(&self.stereo_buf);
            self.stereo_corr = self.stereo_corr * 0.8 + c * 0.2;
            self.stereo_balance = self.stereo_balance * 0.8 + b * 0.2;
        }
        // Spectrogram history — push the latest FFT magnitudes (already
        // computed by `update_spectrum`) so the Spectrogram viz module
        // can render a scrolling waterfall.  Cap at the history length
        // so memory usage stays bounded across long sessions.
        if !self.spectrum_magnitudes.is_empty() {
            self.spectrogram_history
                .push_back(self.spectrum_magnitudes.clone());
            while self.spectrogram_history.len() > crate::audio::analysis::SPECTROGRAM_HISTORY_LEN {
                self.spectrogram_history.pop_front();
            }
        }
        // LUFS — push the same scope buffer through the K-weighting
        // filters.  The meter integrates internally; we just feed it
        // the latest block.
        self.lufs_meter.process_block(&self.scope_buf);
        while let Ok(load) = self.dsp_load_rx.pop() {
            self.dsp_load_buf.push(load);
        }
        if self.dsp_load_buf.len() > 64 {
            let drain = self.dsp_load_buf.len() - 64;
            self.dsp_load_buf.drain(..drain);
        }
        // Track step changes for smooth event stream interpolation, and
        // snapshot melodic notes that fired this tick into a frozen log
        // so the event-stream display retains the past even if the
        // pattern is mutated mid-cycle.
        {
            let s = self.state.read();
            let step = s.sequencer.current_step;
            if step != self.last_seq_step && s.sequencer.running {
                self.last_seq_step = step;
                self.last_step_time = ctx.input(|i| i.time);
                self.last_step_global = s.global_step_count;
                let fired_at = s.global_step_count;
                let seq = &s.sequencer;
                let mut push = |note: u8, gate: f32, accent: f32, slide: f32| {
                    self.melodic_log.push_back(MelodicLogEntry {
                        fired_at,
                        note,
                        gate,
                        accent,
                        slide,
                    });
                };
                // Bass voices (multi-voice) — voice 0 mirrors bass_pattern.
                for (vi, voice) in s.bass_voices.iter().enumerate() {
                    if !voice.enabled {
                        continue;
                    }
                    let pattern = if vi == 0 {
                        &seq.bass_pattern
                    } else if let Some(p) = seq.bass_patterns.get(vi) {
                        p
                    } else {
                        continue;
                    };
                    let voice_steps = seq
                        .bass_voice_steps
                        .get(vi)
                        .copied()
                        .unwrap_or(seq.steps)
                        .max(1);
                    if let Some(st) = pattern.get(step % voice_steps)
                        && st.active
                    {
                        push(st.note, st.gate, st.accent, st.slide);
                    }
                }
                // AN1X
                if s.an1x.enabled {
                    let n = seq.an1x_steps.max(1);
                    if let Some(st) = seq.an1x_pattern.get(step % n)
                        && st.active
                    {
                        push(st.note, st.gate, st.accent, st.slide);
                    }
                }
                // Hoover
                if s.hoover.enabled {
                    let n = seq.hoover_steps.max(1);
                    if let Some(st) = seq.hoover_pattern.get(step % n)
                        && st.active
                    {
                        push(st.note, st.gate, st.accent, st.slide);
                    }
                }
                while self.melodic_log.len() > MELODIC_LOG_CAP {
                    self.melodic_log.pop_front();
                }
                // Drum hits — snapshot every active drum voice at this step.
                let n = seq.steps.max(1);
                for (voice, pattern) in seq.drum_patterns.iter() {
                    if let Some(st) = pattern.get(step % n)
                        && st.active
                    {
                        self.drum_log.push_back(DrumLogEntry {
                            fired_at,
                            voice: *voice,
                        });
                    }
                }
                while self.drum_log.len() > DRUM_LOG_CAP {
                    self.drum_log.pop_front();
                }
            } else if step != self.last_seq_step {
                // Sequencer stopped; just update the cached cursor.
                self.last_seq_step = step;
                self.last_step_time = ctx.input(|i| i.time);
                self.last_step_global = s.global_step_count;
            }
        }
        self.update_audio_analysis(ctx);
        self.tick_ramps();
        // Drain the master-output capture ring buffer into the shared
        // tap so any panel (granular OR amen rec-chop) can read the
        // most recent few seconds without competing for the rtrb
        // consumer.  Was previously inline in the granular panel,
        // which meant the tap went stale whenever the user wasn't
        // looking at granular — so a "record into amen" button had no
        // recent audio to slice.
        let tap_len = self.granular_tap.len();
        if tap_len > 0 {
            while let Ok(s) = self.granular_capture_rx.pop() {
                let h = self.granular_tap_head;
                self.granular_tap[h] = s;
                self.granular_tap_head = (h + 1) % tap_len;
            }
        }
        self.draw_windows(ctx);
        self.draw_menu_and_header(ctx);
        TopBottomPanel::bottom("footer")
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0)),
            )
            .exact_height(18.0)
            .show(ctx, |ui| {
                let (nm, na, nc) = {
                    let s = self.state.read();
                    (
                        s.rack.modules.len(),
                        s.llm_agents.len(),
                        s.rack.cables.len(),
                    )
                };
                let was_flipped = self.rack_flipped;
                let mut perf = self.state.read().ui_prefs.performance_mode;
                let was_perf = perf;
                scope_footer::draw_footer_status(
                    ui,
                    &self.midi_port,
                    &self.dsp_load_buf,
                    &mut self.rack_flipped,
                    &mut self.ctrl_locked,
                    &mut perf,
                    scope_footer::FooterStats {
                        n_modules: nm,
                        n_agents: na,
                        n_cables: nc,
                        uptime_secs: self.session_start.elapsed().as_secs(),
                        api_port: self.api_port,
                    },
                );
                if was_flipped != self.rack_flipped {
                    self.rack_flipped = was_flipped;
                    self.toggle_rack_flip();
                }
                if perf != was_perf {
                    self.state.write().ui_prefs.performance_mode = perf;
                    self.session_dirty = true;
                }
            });

        // Performance mode hides the piano panel to free up vertical
        // space for the rack canvas.  Re-enabled when the user toggles
        // performance mode off in the header view menu.
        if !self.state.read().ui_prefs.performance_mode {
            TopBottomPanel::bottom("piano")
                .frame(
                    Frame::none()
                        .fill(theme::VOID)
                        .inner_margin(egui::Margin::symmetric(0.0, 0.0)),
                )
                .exact_height(80.0)
                .show(ctx, |ui| {
                    panels::draw_piano(self, ui, ctx);
                });
        }

        // ── Rack canvas (replaces tab panels) ────────────────────────────────
        // When the startup wizard is visible, show an empty central panel
        // so the rack doesn't load and nothing plays in the background.
        CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::same(4.0)),
            )
            .show(ctx, |ui| {
                if !self.show_wizard {
                    rack_canvas::draw_rack(self, ctx, ui);
                }
            });
        if self.show_shortcuts && scope_footer::draw_shortcuts_overlay(ctx) {
            self.show_shortcuts = false;
        }
        if self.state.read().ui_prefs.crt_effect {
            scope_footer::draw_crt_overlay(ctx);
        }
    }
}
