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
                // Stereo alerts
                if self.stereo_corr > 0.95 && analysis.peak_db > -30.0 {
                    all_alerts.push("narrow stereo".into());
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

    pub(crate) fn update_spectrum(&mut self) {
        if self.scope_buf.len() < 256 {
            return;
        }
        let raw = crate::audio::spectrum::compute_spectrum(&self.scope_buf, 44100.0);
        let alpha = self.state.read().spectrum.smoothing;
        if self.spectrum_magnitudes.len() != raw.magnitudes.len() {
            self.spectrum_magnitudes = raw.magnitudes.clone();
            self.spectrum_peaks = raw.magnitudes;
        } else {
            for (i, &r) in raw.magnitudes.iter().enumerate() {
                self.spectrum_magnitudes[i] =
                    self.spectrum_magnitudes[i] * alpha + r * (1.0 - alpha);
                if r > self.spectrum_peaks[i] {
                    self.spectrum_peaks[i] = r;
                } else {
                    self.spectrum_peaks[i] -= 0.3;
                }
            }
        }
    }
}

/// Map LLM param update JSON keys to the most relevant module for focus highlighting.
pub(super) fn llm_update_focus_kind(
    update: Option<&serde_json::Value>,
) -> Option<crate::state::ModuleKind> {
    use crate::state::ModuleKind;
    let obj = update?.as_object()?;
    // Priority order: bass > sequencer > drums > fx > hoover > an1x
    if obj.contains_key("bass") {
        Some(ModuleKind::AcidBass)
    } else if obj.contains_key("sequencer") {
        if let Some(seq) = obj.get("sequencer").and_then(|v| v.as_object())
            && seq.keys().any(|k| {
                k.contains("kick")
                    || k.contains("snare")
                    || k.contains("hihat")
                    || k.contains("clap")
            })
        {
            return Some(ModuleKind::DrumKit808);
        }
        Some(ModuleKind::StepSequencer)
    } else if obj.contains_key("kit_a") || obj.contains_key("kit_b") {
        Some(ModuleKind::DrumKit808)
    } else if obj.contains_key("fx") {
        Some(ModuleKind::FxReverb)
    } else if obj.contains_key("hoover") {
        Some(ModuleKind::HooverLead)
    } else if obj.contains_key("an1x") {
        Some(ModuleKind::An1xVoice)
    } else {
        None
    }
}
