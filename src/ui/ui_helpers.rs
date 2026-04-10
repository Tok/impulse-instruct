// ─── ui/ui_helpers.rs ── small utility methods on ImpulseApp ─────────────────
// Extracted from mod.rs to stay under the 1000-line limit.

use crate::audio::{AudioCommand, AudioParams};
use crate::state::compile_fx_plan;

use super::ImpulseApp;

impl ImpulseApp {
    /// Apply user edits (e.g. knob turns) as lock-triggering param changes.
    pub(crate) fn observe_edits(&mut self, edits: &[(&str, f32)]) {
        for &(path, val) in edits {
            let s = self.state.read().clone();
            *self.state.write() = crate::state::observe_user_edit(s, path, val);
        }
    }

    /// Snapshot the current params and push them to the audio thread.
    pub(crate) fn push_audio_params(&mut self) {
        let params = {
            let s = self.state.read();
            let mut p = AudioParams::from_app_state(&s);
            p.sample_rate = 44100.0;
            p
        };
        let _ = self
            .audio_tx
            .push(AudioCommand::UpdateParams(Box::new(params)));
    }

    /// Recompile the FX routing plan from the current rack cable graph and
    /// send it to the audio thread.  Call whenever rack topology changes
    /// (cable connect/disconnect, module enable/disable, module remove).
    pub(crate) fn push_fx_plan(&mut self) {
        let plan = {
            let s = self.state.read();
            compile_fx_plan(&s.rack)
        };
        let _ = self.audio_tx.push(AudioCommand::SetFxPlan(plan));
    }

    /// Advance any active bar-based parameter ramps and push updated params.
    pub(crate) fn tick_ramps(&mut self) {
        let s = self.state.read().clone();
        if !s.llm.active_ramps.is_empty() {
            let next = crate::state::jam_tools::tick_bar_ramps(s);
            *self.state.write() = next;
            self.push_audio_params();
        }
    }

    /// Auto-analyse captured audio every ~2s for the header display.
    pub(crate) fn update_audio_analysis(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.last_analysis_time > 2.0 {
            self.last_analysis_time = now;
            let mut captured: Vec<f32> = Vec::with_capacity(88200);
            while let Ok(s) = self.capture_rx.pop() {
                captured.push(s);
            }
            if !captured.is_empty() {
                let analysis = crate::audio::analysis::analyse_audio(&captured, 44100.0);
                // Compact snapshot for LLM context (injected into every system prompt)
                self.state.write().audio_snapshot = analysis.one_line_summary();
                self.audio_analysis = Some(analysis);
            }
        }
    }
}
