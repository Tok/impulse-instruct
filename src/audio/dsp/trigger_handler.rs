// ─── audio/dsp/trigger_handler.rs ────────────────────────────────────────────
// `DspState::handle_trigger` — dispatch table for sequencer trigger
// events.  Extracted from `dsp/mod.rs` to keep that file under the
// 1000-line cap; every voice + drum trigger lives here.

use crate::sequencer::TriggerEvent;
use crate::state::DrumVoice;

use super::DspState;
use super::voices;

impl DspState {
    pub fn handle_trigger(&mut self, event: &TriggerEvent) {
        use TriggerEvent::*;
        match event {
            DrumTrigger {
                voice,
                velocity,
                slice,
            } => {
                let in_rack = match voice {
                    DrumVoice::Kick808
                    | DrumVoice::Snare808
                    | DrumVoice::HihatClosed808
                    | DrumVoice::HihatOpen808
                    | DrumVoice::TomHi808
                    | DrumVoice::TomMid808
                    | DrumVoice::TomLo808 => self.params.rack_drums808,
                    DrumVoice::Kick909
                    | DrumVoice::Snare909
                    | DrumVoice::HihatClosed909
                    | DrumVoice::HihatOpen909
                    | DrumVoice::Clap909
                    | DrumVoice::Rim909 => self.params.rack_drums909,
                    DrumVoice::Amen => self.params.rack_amen,
                    DrumVoice::GabberKick => self.params.rack_gabber_kick,
                };
                if !in_rack {
                    return;
                }
                self.drum_velocity[voices::drum_voice_idx(voice)] = velocity.clamp(0.0, 1.0);
                match voice {
                    DrumVoice::Kick808 => self.kick808.trigger(),
                    DrumVoice::Snare808 => self.snare808.trigger(),
                    DrumVoice::HihatClosed808 => self.hihat_closed808.trigger(),
                    DrumVoice::HihatOpen808 => self.hihat_open808.trigger(),
                    DrumVoice::TomHi808 => self.tom_hi808.trigger(),
                    DrumVoice::TomMid808 => self.tom_mid808.trigger(),
                    DrumVoice::TomLo808 => self.tom_lo808.trigger(),
                    DrumVoice::Kick909 => self.kick909.trigger(),
                    DrumVoice::Snare909 => self.snare909.trigger(),
                    DrumVoice::HihatClosed909 => self.hihat_closed909.trigger(),
                    DrumVoice::HihatOpen909 => self.hihat_open909.trigger(),
                    DrumVoice::Clap909 => self.clap909.trigger(),
                    DrumVoice::Rim909 => self.rim909.trigger(),
                    DrumVoice::Amen => self.amen.trigger(
                        *slice,
                        self.params.amen_slice_count,
                        self.params.amen_start_offset,
                        self.params.amen_end_offset,
                        self.params.amen_reverse,
                        self.params.amen_gate,
                        self.params.amen_stutter,
                        &self.params.amen_slice_positions,
                        &self.params.amen_slice_pitches,
                        &self.params.amen_slice_volumes,
                        &self.params.amen_slice_reverses,
                        self.params.amen_bpm_stretch,
                        self.params.amen_bpm_stretch_preserve,
                        self.params.amen_source_bpm,
                        self.params.sequencer_bpm,
                    ),
                    DrumVoice::GabberKick => self.gabber_kick.trigger(),
                }
            }
            BassTrigger {
                voice_idx,
                note,
                accent,
                slide,
                gate_samples: _,
                pan,
            } => {
                if self.params.rack_bass && *voice_idx < crate::state::MAX_BASS_VOICES {
                    self.bass[*voice_idx].trigger(*note, *accent, *slide, self.params.tuning);
                    self.bass_step_pan = pan.clamp(-1.0, 1.0);
                }
            }
            BassGateOff { voice_idx } => {
                if self.params.rack_bass && *voice_idx < crate::state::MAX_BASS_VOICES {
                    self.bass[*voice_idx].gate_off();
                }
            }
            HooverTrigger {
                note,
                accent,
                slide,
            } => {
                if self.params.rack_hoover {
                    self.hoover
                        .trigger(*note, self.params.tuning, *accent, *slide);
                }
            }
            HooverGateOff => {
                if self.params.rack_hoover {
                    self.hoover.gate_off();
                }
            }
            An1xTrigger {
                note,
                accent,
                slide,
            } => {
                if self.params.rack_an1x {
                    self.an1x
                        .trigger(*note, *accent, *slide, self.sample_rate, &self.params);
                }
            }
            An1xGateOff => {
                if self.params.rack_an1x {
                    self.an1x.gate_off();
                }
            }
            PluckTrigger {
                note,
                accent,
                slide,
            } => {
                if self.params.rack_pluck {
                    self.pluck.trigger(
                        *note,
                        self.params.tuning,
                        *accent,
                        *slide,
                        self.sample_rate,
                        self.params.pluck_pitch_offset_semi,
                    );
                }
            }
            PluckGateOff => {
                if self.params.rack_pluck {
                    self.pluck.gate_off();
                }
            }
            WavetableTrigger {
                note,
                accent,
                slide,
            } => {
                if self.params.rack_wavetable {
                    self.wavetable.trigger(
                        *note,
                        self.params.tuning,
                        *accent,
                        *slide,
                        self.params.wavetable_pitch_offset_semi,
                    );
                }
            }
            WavetableGateOff => {
                if self.params.rack_wavetable {
                    self.wavetable.gate_off();
                }
            }
            SampleTrigger {
                note,
                accent,
                slide,
            } => {
                if self.params.rack_sample {
                    // Refresh the source-recording reference each trigger
                    // so a UI-side root-note change takes effect on the
                    // very next note rather than on a load.
                    self.sample_instrument
                        .set_root_note(self.params.sample_root_note, self.params.tuning);
                    self.sample_instrument.trigger(
                        *note,
                        self.params.tuning,
                        *accent,
                        *slide,
                        self.params.sample_pitch_offset_cents,
                        self.params.sample_mic_blend,
                    );
                }
            }
            SampleGateOff => {
                if self.params.rack_sample {
                    self.sample_instrument.gate_off();
                }
            }
            FmOpsTrigger {
                note,
                accent,
                slide: _,
            } => {
                if self.params.rack_fm_ops {
                    let freq = super::midi_to_hz_tuned(*note, self.params.tuning);
                    self.fm_ops.trigger(freq, *accent);
                }
            }
            FmOpsGateOff => {
                if self.params.rack_fm_ops {
                    self.fm_ops.gate_off();
                }
            }
            AdditiveTrigger {
                note,
                accent,
                slide: _,
            } => {
                if self.params.rack_additive {
                    let freq = super::midi_to_hz_tuned(*note, self.params.tuning);
                    self.additive.trigger(freq, *accent);
                }
            }
            AdditiveGateOff => {
                if self.params.rack_additive {
                    self.additive.gate_off();
                }
            }
            ModalTrigger {
                note,
                accent,
                slide: _,
            } => {
                if self.params.rack_modal {
                    let freq = super::midi_to_hz_tuned(*note, self.params.tuning);
                    self.modal
                        .trigger(freq, *accent, &self.params, self.sample_rate);
                }
            }
            ModalGateOff => {
                if self.params.rack_modal {
                    self.modal.gate_off();
                }
            }
            ChiptuneTrigger {
                note,
                accent,
                slide: _,
            } => {
                if self.params.rack_chiptune {
                    let freq = super::midi_to_hz_tuned(*note, self.params.tuning);
                    self.chiptune.trigger(freq, *accent);
                }
            }
            ChiptuneGateOff => {
                if self.params.rack_chiptune {
                    self.chiptune.gate_off();
                }
            }
            VocalTrigger {
                note,
                accent,
                slide: _,
            } => {
                if self.params.rack_vocal {
                    let freq = super::midi_to_hz_tuned(*note, self.params.tuning);
                    self.vocal.trigger(freq, *accent);
                }
            }
            VocalGateOff => {
                if self.params.rack_vocal {
                    self.vocal.gate_off();
                }
            }
            GranularPitch { note } => {
                if self.params.granular_enabled && self.params.granular_pitch_mappable {
                    self.granular.set_base_note(*note);
                }
            }
        }
    }
}
