// ─── midi/mod.rs ─────────────────────────────────────────────────────────────
#![allow(dead_code)] // MIDI wiring grows with hardware support
// MIDI input/output via midir.
// Maps incoming MIDI to AppState mutations sent back to the UI thread.

use crossbeam_channel::Sender;
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

// ─── MIDI events we emit to the UI thread ─────────────────────────────────────

#[derive(Clone, Debug)]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    ControlChange { channel: u8, cc: u8, value: u8 },
    PitchBend { channel: u8, value: f32 }, // -1.0–+1.0
    Clock,
    Start,
    Stop,
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

fn parse_midi(msg: &[u8]) -> Option<MidiEvent> {
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
