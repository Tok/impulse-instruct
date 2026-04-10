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
}
