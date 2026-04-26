// ─── state/rack_wiring.rs ─────────────────────────────────────────────────────
// `wire_default_cables` — the canonical "patch a fresh rack so its modules
// actually make sound" routine.  Extracted from rack.rs so that file stays
// under the 1000-line cap; the function itself is ~110 lines and grows
// every time a new voice / sidechain FX joins the auto-wire set, so
// living in its own file gives it room to breathe.
//
// Symmetry with the rest of the rack code is intentional: this is an
// `impl RackState` block, not a free function, because the wiring
// reaches into `self.connect` / `self.connect_control` and needs the
// full mutable rack state.

use super::rack::{PortDir, PortKind, PortRef, RackModule, RackState};
use crate::state::ModuleKind;

impl RackState {
    /// Wire standard default cables for whichever modules are present:
    /// seq→voices (CV), voices→master (audio), FX serial chain, TTS→reverb,
    /// and agent→all controllable (control). Safe to call on any rack — only
    /// wires modules that actually exist.
    pub fn wire_default_cables(&mut self) {
        let find = |modules: &[RackModule], kind: ModuleKind| -> Option<u32> {
            modules.iter().find(|m| m.kind == kind).map(|m| m.id)
        };
        let master_id = find(&self.modules, ModuleKind::MasterOutput);
        let seq_id = find(&self.modules, ModuleKind::StepSequencer);
        // Voices that get a direct CV trigger from the sequencer and their
        // dry audio cabled to master.  V2 voices (PluckString /
        // WavetableVoice / SampleInstrument) must be in this list or
        // their cards appear silent on the rack.
        let voice_ids: Vec<u32> = [
            ModuleKind::AcidBass,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
            ModuleKind::HooverLead,
            ModuleKind::An1xVoice,
            ModuleKind::AmenSampler,
            ModuleKind::NoiseVoice,
            ModuleKind::GranularTexture,
            ModuleKind::GabberKick,
            ModuleKind::PluckString,
            ModuleKind::WavetableVoice,
            ModuleKind::SampleInstrument,
            ModuleKind::FmOpsVoice,
            ModuleKind::AdditiveVoice,
        ]
        .iter()
        .filter_map(|&k| find(&self.modules, k))
        .collect();
        let tts_id = find(&self.modules, ModuleKind::NeuTts);
        let reverb_id = find(&self.modules, ModuleKind::FxReverb);
        let delay_id = find(&self.modules, ModuleKind::FxDelay);

        let audio_cable = |from_id: u32, to_id: u32| {
            (
                PortRef {
                    module_id: from_id,
                    dir: PortDir::Out,
                    kind: PortKind::Audio,
                    index: 0,
                },
                PortRef {
                    module_id: to_id,
                    dir: PortDir::In,
                    kind: PortKind::Audio,
                    index: 0,
                },
            )
        };

        // Seq → voices (CV)
        if let Some(sid) = seq_id {
            for vid in &voice_ids {
                self.connect(
                    PortRef {
                        module_id: sid,
                        dir: PortDir::Out,
                        kind: PortKind::Cv,
                        index: 0,
                    },
                    PortRef {
                        module_id: *vid,
                        dir: PortDir::In,
                        kind: PortKind::Cv,
                        index: 0,
                    },
                );
            }
        }
        // Voices → master (dry direct path)
        if let Some(mid) = master_id {
            for vid in &voice_ids {
                let (a, b) = audio_cable(*vid, mid);
                self.connect(a, b);
            }
        }
        // Audio-only voices — they make sound but don't take a
        // sequencer-CV trigger (the user plays them directly via a
        // panel widget like the Theremin's XY pad).  Wire their
        // audio output to master so the card isn't silent the
        // moment it's added; skip the seq → CV cable since there's
        // no CV-in jack to land on.
        let audio_only_voices: Vec<u32> = [ModuleKind::Theremin, ModuleKind::Pendulum]
            .iter()
            .filter_map(|&k| find(&self.modules, k))
            .collect();
        if let Some(mid) = master_id {
            for vid in &audio_only_voices {
                let (a, b) = audio_cable(*vid, mid);
                self.connect(a, b);
            }
        }
        // TTS → Reverb (TTS gets ambience).  Reverb output is wired to
        // master below so this stays in the audio path.
        if let (Some(tid), Some(rid)) = (tts_id, reverb_id) {
            let (a, b) = audio_cable(tid, rid);
            self.connect(a, b);
        }
        // ── Wire the "important" FX to master so they have a complete
        // path.  Other FX live in the rack but stay unwired by default —
        // the user can patch them in as needed (transparent FX nodes).
        if let (Some(rid), Some(mid)) = (reverb_id, master_id) {
            let (a, b) = audio_cable(rid, mid);
            self.connect(a, b);
        }
        if let (Some(did), Some(mid)) = (delay_id, master_id) {
            let (a, b) = audio_cable(did, mid);
            self.connect(a, b);
        }
        // Agent → all controllable
        let agent_id = find(&self.modules, ModuleKind::LlmAgent);
        if let Some(aid) = agent_id {
            let targets: Vec<u32> = self
                .modules
                .iter()
                .filter(|m| {
                    !matches!(
                        m.kind,
                        ModuleKind::MasterOutput | ModuleKind::LlmAgent | ModuleKind::LlmConsole
                    )
                })
                .map(|m| m.id)
                .collect();
            for tid in &targets {
                self.connect_control(aid, *tid);
            }
        }
    }
}
