// ─── ui/midi_handler.rs ───────────────────────────────────────────────────────
// MIDI event drain — processes incoming MIDI events into state changes and DSP triggers.

use super::ImpulseApp;
use crate::audio::AudioCommand;
use crate::midi::MidiEvent;
use crate::sequencer::TriggerEvent;

impl ImpulseApp {
    pub(super) fn drain_midi_events(&mut self) {
        use crate::midi::cc_to_param_path;
        use crate::state::{apply_llm_update, toggle_sequencer_running};
        while let Ok(event) = self.midi_rx.try_recv() {
            match event {
                MidiEvent::NoteOn {
                    note, velocity: 0, ..
                } => {
                    self.pressed_notes.remove(&note);
                    let _ = self
                        .audio_tx
                        .push(AudioCommand::Trigger(TriggerEvent::BassGateOff {
                            voice_idx: 0,
                        }));
                }
                MidiEvent::NoteOn { note, velocity, .. } => {
                    self.pressed_notes.insert(note);
                    let vel = velocity as f32 / 127.0;
                    let _ = self
                        .audio_tx
                        .push(AudioCommand::Trigger(TriggerEvent::BassTrigger {
                            voice_idx: 0,
                            note,
                            accent: vel > 0.8,
                            slide: false,
                            gate_samples: 22050,
                            pan: 0.0,
                        }));
                    let step = self.state.read().sequencer.current_step;
                    let s = self.state.read().clone();
                    let was_active = s
                        .sequencer
                        .bass_pattern
                        .get(step)
                        .map(|b| b.active)
                        .unwrap_or(false);
                    *self.state.write() = crate::state::set_bass_step(s, step, note, was_active);
                }
                MidiEvent::NoteOff { note, .. } => {
                    self.pressed_notes.remove(&note);
                    let _ = self
                        .audio_tx
                        .push(AudioCommand::Trigger(TriggerEvent::BassGateOff {
                            voice_idx: 0,
                        }));
                }
                MidiEvent::ControlChange { cc, value, .. } => {
                    if let Some((path, scale)) = cc_to_param_path(cc) {
                        let scaled = scale(value);
                        let update = super::dot_path_to_json(path, scaled);
                        let next = apply_llm_update(self.state.read().clone(), &update, &[]);
                        *self.state.write() = next;
                        self.push_audio_params();
                    }
                }
                MidiEvent::Start => {
                    self.midi_clock_tracker.reset();
                    let s = self.state.read().clone();
                    if !s.sequencer.running {
                        *self.state.write() = toggle_sequencer_running(s);
                    }
                }
                MidiEvent::Stop => {
                    self.midi_clock_tracker.reset();
                    let s = self.state.read().clone();
                    if s.sequencer.running {
                        *self.state.write() = toggle_sequencer_running(s);
                    }
                }
                MidiEvent::Clock => {
                    let sync_on = self.state.read().sequencer.midi_clock_sync;
                    if sync_on && let Some(bpm) = self.midi_clock_tracker.on_clock() {
                        self.state.write().sequencer.bpm = bpm.clamp(20.0, 300.0);
                        self.push_audio_params();
                    }
                }
                _ => {}
            }
        }
    }
}
