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
                            accent: if vel > 0.8 { 1.0 } else { 0.0 },
                            slide: 0.0,
                            gate_samples: 22050,
                            pan: 0.0,
                        }));
                    // Pitch-mappable granular: live MIDI notes also drive
                    // the granular base-note transposition so a played
                    // bird-call corpus tracks the keyboard.
                    let granular_pitch_map = {
                        let s = self.state.read();
                        s.granular.enabled && s.granular.pitch_mappable
                    };
                    if granular_pitch_map {
                        let _ = self
                            .audio_tx
                            .push(AudioCommand::Trigger(TriggerEvent::GranularPitch { note }));
                    }
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
                MidiEvent::PitchBend { channel, value } => {
                    // MPE per-note pitch bend — store on AppState.mpe
                    // for downstream consumers (API / WS) and route
                    // pitch bend on the master channel through the
                    // existing pipeline (none of which exists yet, so
                    // V1 is just the snapshot).  Channel 0 = master in
                    // an MPE lower zone; we still capture it.
                    let mut s = self.state.write();
                    s.mpe.channel = channel;
                    s.mpe.pitch_bend = value.clamp(-1.0, 1.0);
                }
                MidiEvent::ChannelPressure { channel, value } => {
                    let mut s = self.state.write();
                    s.mpe.channel = channel;
                    s.mpe.pressure = crate::midi::pressure_to_unit(value);
                }
                MidiEvent::ControlChange { cc, value, channel }
                    if cc == 74 && crate::midi::is_mpe_note_channel(channel) =>
                {
                    // CC74 on a per-note channel = MPE timbre (Y axis).
                    // Snapshot to AppState.mpe instead of routing
                    // through the static cc_to_param_path table so
                    // MPE controllers don't accidentally wrench the
                    // bass cutoff knob on every per-note Y wiggle.
                    let mut s = self.state.write();
                    s.mpe.channel = channel;
                    s.mpe.timbre = value as f32 / 127.0;
                }
                MidiEvent::ControlChange { cc, value, .. } => {
                    // 1. If a learn-next-CC request is pending, this CC
                    //    becomes the binding instead of acting on a
                    //    parameter.  Save and clear; let the next CC
                    //    of the same number drive the param normally.
                    if let Some(target) = self.midi_learn_target.take() {
                        let mut s = self.state.write();
                        s.ui_prefs.midi_cc_bindings.insert(cc, target.clone());
                        log::info!("[midi] learned CC{} → {}", cc, target);
                        self.session_dirty = true;
                        continue;
                    }
                    // 2. User binding wins over the static table.
                    let user_binding = self
                        .state
                        .read()
                        .ui_prefs
                        .midi_cc_bindings
                        .get(&cc)
                        .cloned();
                    if let Some(path) = user_binding {
                        let scaled = value as f32 / 127.0;
                        let update = super::dot_path_to_json(&path, scaled);
                        let next = apply_llm_update(self.state.read().clone(), &update, &[]);
                        *self.state.write() = next;
                        self.push_audio_params();
                    } else if let Some((path, scale)) = cc_to_param_path(cc) {
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
            }
        }
    }
}
