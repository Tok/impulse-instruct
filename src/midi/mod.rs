// ─── midi/mod.rs ─────────────────────────────────────────────────────────────
#![allow(dead_code)] // MIDI wiring grows with hardware support
// MIDI input/output via midir, plus SMF export for the sequencer pattern.
// Maps incoming MIDI to AppState mutations sent back to the UI thread.

pub mod export;
pub mod import;
pub use export::{
    drum_voice_to_gm_note, export_sequencer_smf, save_midi_export, save_midi_export_to,
};
pub use import::{ImportSummary, MidiImport, import_midi_file, import_midi_into};

use crossbeam_channel::Sender;
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

/// MIDI timing-clock resolution: 24 pulses per quarter note, as specified by
/// the MIDI 1.0 standard.  Used to compute the clock-out tick interval from
/// BPM and to recover BPM from incoming clock pulses.
pub const MIDI_CLOCK_PPQN: f64 = 24.0;

/// Samples between consecutive MIDI clock pulses (0xF8) at the given
/// BPM and sample rate.  24 PPQN means a 120-BPM clock fires every
/// 60s ÷ (120 × 24) ≈ 20.8 ms — at 48 kHz that's exactly 1000 samples.
/// Pure helper so the audio thread's clock-out accumulator can be
/// unit-tested without an audio engine.  Returns f64 to keep the
/// accumulator math precise across millions of pulses per session.
#[inline]
pub fn midi_clock_tick_interval_samples(bpm: f32, sample_rate: f32) -> f64 {
    let bpm = (bpm as f64).max(1.0);
    (sample_rate as f64 * 60.0) / (bpm * MIDI_CLOCK_PPQN)
}

/// True when an inter-pulse interval (seconds between two clock
/// pulses) is plausible enough to feed into the BPM averager.
/// Caps at 10 ms = 300 BPM (the upper bound of the BPM slider) and
/// 300 ms ≈ 7.5 BPM (slower than any plausible source clock).
/// Outside the window we discard the sample as a glitch — a short
/// interval is usually a doubled-up pulse, a long one usually a
/// dropped pulse or a paused source.
#[inline]
pub fn is_valid_clock_interval(secs: f64) -> bool {
    secs > 0.01 && secs < 0.30
}

/// Convert an averaged inter-pulse interval (seconds) into BPM.
/// 24 PPQN: BPM = 60 / (avg × 24).  Returns 0.0 for non-positive
/// inputs so a buggy averager (empty window, divide-by-zero) can't
/// poison the BPM display.
#[inline]
pub fn clock_interval_to_bpm(avg_secs: f64) -> f32 {
    if avg_secs <= 0.0 {
        return 0.0;
    }
    (60.0 / (avg_secs * MIDI_CLOCK_PPQN)) as f32
}

// ─── MIDI events we emit to the UI thread ─────────────────────────────────────

#[derive(Clone, Debug)]
pub enum MidiEvent {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    ControlChange {
        channel: u8,
        cc: u8,
        value: u8,
    },
    PitchBend {
        channel: u8,
        value: f32,
    }, // -1.0–+1.0
    /// MIDI Channel Pressure (aftertouch), 1 data byte (0..127).  MPE
    /// uses this on per-note channels (typically chs 2-N) for the
    /// "Z" expression axis (pressure / volume).
    ChannelPressure {
        channel: u8,
        value: u8,
    },
    Clock,
    Start,
    Stop,
}

/// True when `channel` is part of an MPE per-note zone — any channel
/// other than 0 (i.e. master ch 1 in 1-indexed MIDI numbering).
/// Treated as a heuristic rather than a strict zone-manager parse;
/// most MPE controllers default to chs 2-N for note channels and ch
/// 1 for master, which matches "channel != 0" perfectly.  Pure helper
/// so the dispatch path can be unit-tested without midir.
#[inline]
pub fn is_mpe_note_channel(channel: u8) -> bool {
    channel != 0
}

/// Map a 7-bit pressure value to a unit-interval expression value.
#[inline]
pub fn pressure_to_unit(value: u8) -> f32 {
    (value & 0x7F) as f32 / 127.0
}

// ─── CC → param mapping ───────────────────────────────────────────────────────

type CcMapping = Option<(&'static str, fn(u8) -> f32)>;

pub fn cc_to_param_path(cc: u8) -> CcMapping {
    let norm = |v: u8| v as f32 / 127.0;
    match cc {
        74 => Some(("bass.cutoff", norm)),
        71 => Some(("bass.resonance", norm)),
        72 => Some(("bass.decay", norm)),
        73 => Some(("bass.env_mod", norm)),
        75 => Some(("bass.accent_level", norm)),
        76 => Some(("bass.distortion", norm)),
        91 => Some(("fx.reverb_mix", norm)),
        93 => Some(("fx.delay_mix", norm)),
        94 => Some(("fx.delay_feedback", norm)),
        7 => Some(("fx.master_volume", norm)),
        _ => None,
    }
}

// ─── MIDI listener ────────────────────────────────────────────────────────────

pub struct MidiListener {
    _connection: Option<MidiInputConnection<()>>,
}

impl MidiListener {
    pub fn new(port_name: &str, event_tx: Sender<MidiEvent>) -> Self {
        let connection = try_open_midi(port_name, event_tx);
        Self {
            _connection: connection,
        }
    }

    pub fn list_ports() -> Vec<String> {
        let input = MidiInput::new("impulse-instruct-scan").ok();
        input
            .map(|m| {
                m.ports()
                    .iter()
                    .filter_map(|p| m.port_name(p).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Scan available MIDI ports and connect to the first non-"Midi Through" device.
    /// Prefers a port whose name contains "LPK25"; otherwise takes the first real port.
    /// Returns (listener, connected_port_name).
    pub fn auto_connect(event_tx: Sender<MidiEvent>) -> (Self, Option<String>) {
        let ports = Self::list_ports();
        log::debug!("MIDI ports available: {:?}", ports);

        let chosen = ports
            .iter()
            .find(|p| p.to_lowercase().contains("lpk25"))
            .or_else(|| {
                ports
                    .iter()
                    .find(|p| !p.to_lowercase().contains("midi through"))
            })
            .cloned();

        match chosen {
            Some(ref name) => {
                log::info!("Connecting MIDI input: {}", name);
                let listener = Self::new(name, event_tx);
                (listener, Some(name.clone()))
            }
            None => {
                log::info!("No MIDI input device found");
                (Self { _connection: None }, None)
            }
        }
    }
}

fn try_open_midi(port_name: &str, event_tx: Sender<MidiEvent>) -> Option<MidiInputConnection<()>> {
    let mut midi_in = MidiInput::new("impulse-instruct").ok()?;
    midi_in.ignore(midir::Ignore::None);

    let port = midi_in
        .ports()
        .into_iter()
        .find(|p| midi_in.port_name(p).ok().as_deref() == Some(port_name))?;

    let connection = midi_in
        .connect(
            &port,
            "impulse-instruct-in",
            move |_ts, msg, _| {
                if let Some(event) = parse_midi(msg) {
                    let _ = event_tx.try_send(event);
                }
            },
            (),
        )
        .ok()?;

    log::info!("MIDI input connected: {}", port_name);
    Some(connection)
}

pub(crate) fn parse_midi(msg: &[u8]) -> Option<MidiEvent> {
    if msg.is_empty() {
        return None;
    }
    let status = msg[0];
    let channel = status & 0x0F;
    match status & 0xF0 {
        0x90 if msg.len() >= 3 && msg[2] > 0 => Some(MidiEvent::NoteOn {
            channel,
            note: msg[1],
            velocity: msg[2],
        }),
        0x90 | 0x80 if msg.len() >= 2 => Some(MidiEvent::NoteOff {
            channel,
            note: msg[1],
        }),
        0xB0 if msg.len() >= 3 => Some(MidiEvent::ControlChange {
            channel,
            cc: msg[1],
            value: msg[2],
        }),
        0xE0 if msg.len() >= 3 => {
            let raw = (msg[1] as i16) | ((msg[2] as i16) << 7);
            let val = (raw - 8192) as f32 / 8192.0;
            Some(MidiEvent::PitchBend {
                channel,
                value: val,
            })
        }
        // Channel Pressure (aftertouch) — 1 data byte = pressure.
        0xD0 if msg.len() >= 2 => Some(MidiEvent::ChannelPressure {
            channel,
            value: msg[1],
        }),
        0xF8 => Some(MidiEvent::Clock),
        0xFA => Some(MidiEvent::Start),
        0xFC => Some(MidiEvent::Stop),
        _ => None,
    }
}

// ─── MIDI clock output ────────────────────────────────────────────────────────

/// Sends MIDI timing clock (0xF8 / 24 PPQN), Start (0xFA), and Stop (0xFC)
/// to a MIDI output port. Thread-safe to move; send methods are blocking but
/// complete in microseconds for single-byte messages.
pub struct MidiClockOutput {
    connection: MidiOutputConnection,
}

impl MidiClockOutput {
    pub fn list_output_ports() -> Vec<String> {
        let output = MidiOutput::new("impulse-instruct-scan").ok();
        output
            .map(|m| {
                m.ports()
                    .iter()
                    .filter_map(|p| m.port_name(p).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn connect(port_name: &str) -> Option<Self> {
        let output = MidiOutput::new("impulse-instruct-clock").ok()?;
        let port = output
            .ports()
            .into_iter()
            .find(|p| output.port_name(p).ok().as_deref() == Some(port_name))?;
        let connection = output.connect(&port, "impulse-clock-out").ok()?;
        log::info!("MIDI clock out: connected to '{}'", port_name);
        Some(Self { connection })
    }

    /// Auto-connect to the first non-"Midi Through" output port.
    pub fn auto_connect() -> (Option<Self>, Option<String>) {
        let ports = Self::list_output_ports();
        let chosen = ports
            .iter()
            .find(|p| !p.to_lowercase().contains("midi through"))
            .cloned();
        match chosen {
            Some(ref name) => (Self::connect(name), Some(name.clone())),
            None => {
                log::info!("MIDI clock out: no output port found");
                (None, None)
            }
        }
    }

    /// Send a single raw MIDI byte (0xF8 clock / 0xFA start / 0xFC stop).
    #[inline]
    pub fn send_byte(&mut self, byte: u8) {
        self.connection.send(&[byte]).ok();
    }
}
