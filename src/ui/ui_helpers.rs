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
                let mut all_alerts: Vec<String> =
                    analysis.alerts().iter().map(|s| s.to_string()).collect();
                // Pattern/mix alerts from sequencer state
                let pat_alerts = crate::audio::analysis::pattern_alerts(&self.state.read());
                all_alerts.extend(pat_alerts);
                // Stereo alert — only warn if stereo FX are active but output is still mono
                {
                    let s = self.state.read();
                    let has_stereo_fx =
                        s.fx.chorus_mix > 0.05 || s.fx.reverb_mix > 0.1 || s.fx.delay_mix > 0.05;
                    if self.stereo_corr > 0.98 && analysis.peak_db > -30.0 && has_stereo_fx {
                        all_alerts.push("mono mix".into());
                    }
                }
                // Build snapshot with stereo info
                let stereo_str = format!(
                    " st:{:+.0}% w:{:.0}%",
                    self.stereo_balance * 100.0,
                    (1.0 - self.stereo_corr.abs()) * 100.0
                );
                let base = format!("{}{}", analysis.one_line_summary(), stereo_str);
                let snap = if all_alerts.is_empty() {
                    base
                } else {
                    format!("{} !! {}", base, all_alerts.join(", "))
                };
                self.state.write().audio_snapshot = snap;
                self.audio_analysis = Some(analysis);
            }
        }
    }
}
