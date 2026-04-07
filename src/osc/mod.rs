// ─── osc/mod.rs ───────────────────────────────────────────────────────────────
// OSC (Open Sound Control) UDP listener.
//
// Listens on a UDP port and translates incoming OSC messages into synth
// parameter changes, transport control, and LLM prompts.
//
// Address scheme:
//   /impulse/<section>/<param>  <value>   → apply param (same path as HTTP API)
//   /impulse/sequencer/play               → start transport
//   /impulse/sequencer/stop               → stop transport
//   /impulse/prompt             <string>  → send prompt to LLM
//
// Examples (oscsend / TouchOSC / Max):
//   /impulse/bass/cutoff       0.7
//   /impulse/bass/resonance    0.85
//   /impulse/fx/reverb_mix     0.5
//   /impulse/sequencer/bpm     130
//   /impulse/sequencer/play
//   /impulse/sequencer/stop
//   /impulse/prompt            "make it darker"
//
// Enable:  cargo run -- --osc            (port 57120, the SuperCollider default)
//          cargo run -- --osc-port 9000  (custom port)

use crate::llm::LlmInput;
use crate::state::{AppState, apply_llm_update};
use crossbeam_channel::Sender;
use parking_lot::RwLock;
use rosc::{OscPacket, OscType, decoder};
use std::sync::Arc;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Unit struct — spawns the OSC listener thread and immediately returns.
/// Drop has no effect; the thread runs until the process exits.
pub struct OscListener;

impl OscListener {
    pub fn start(port: u16, state: Arc<RwLock<AppState>>, llm_tx: Sender<LlmInput>) -> Self {
        std::thread::Builder::new()
            .name("osc".into())
            .spawn(move || run_osc_loop(port, state, llm_tx))
            .expect("failed to spawn OSC thread");
        Self
    }
}

// ─── Internal types ───────────────────────────────────────────────────────────

enum OscAction {
    ParamUpdate(serde_json::Value),
    Play,
    Stop,
    Prompt(String),
}

// ─── Receive loop ─────────────────────────────────────────────────────────────

fn run_osc_loop(port: u16, state: Arc<RwLock<AppState>>, llm_tx: Sender<LlmInput>) {
    let addr = format!("0.0.0.0:{port}");
    let socket = match std::net::UdpSocket::bind(&addr) {
        Ok(s) => {
            log::info!("OSC: listening on udp://{addr}");
            s
        }
        Err(e) => {
            log::error!("OSC: failed to bind {addr}: {e}");
            return;
        }
    };

    let mut buf = [0u8; 1500]; // one Ethernet MTU — enough for any OSC message
    loop {
        match socket.recv_from(&mut buf) {
            Ok((size, _from)) => match decoder::decode_udp(&buf[..size]) {
                Ok((_sender, packet)) => handle_packet(packet, &state, &llm_tx),
                Err(e) => log::debug!("OSC: decode error: {e}"),
            },
            Err(e) => log::debug!("OSC: recv error: {e}"),
        }
    }
}

fn handle_packet(packet: OscPacket, state: &Arc<RwLock<AppState>>, llm_tx: &Sender<LlmInput>) {
    match packet {
        OscPacket::Message(msg) => match parse_osc_addr(&msg.addr, &msg.args) {
            Some(action) => dispatch(action, state, llm_tx),
            None => log::debug!("OSC: unhandled address: {}", msg.addr),
        },
        OscPacket::Bundle(bundle) => {
            for p in bundle.content {
                handle_packet(p, state, llm_tx);
            }
        }
    }
}

// ─── Address parsing ──────────────────────────────────────────────────────────

fn parse_osc_addr(addr: &str, args: &[OscType]) -> Option<OscAction> {
    // Strip leading slash and split into up to 3 segments
    let parts: Vec<&str> = addr.trim_start_matches('/').splitn(3, '/').collect();

    // All addresses must start with "impulse"
    if parts.first() != Some(&"impulse") {
        return None;
    }

    match parts.as_slice() {
        ["impulse", "sequencer", "play"] => Some(OscAction::Play),
        ["impulse", "sequencer", "stop"] => Some(OscAction::Stop),
        ["impulse", "prompt"] => {
            let s = args.first().and_then(|a| match a {
                OscType::String(s) => Some(s.clone()),
                _ => None,
            })?;
            Some(OscAction::Prompt(s))
        }
        ["impulse", section, param] => {
            let val = args.first().and_then(osc_arg_to_json)?;
            let update = serde_json::json!({ *section: { *param: val } });
            Some(OscAction::ParamUpdate(update))
        }
        _ => None,
    }
}

/// Convert an OSC argument to a serde_json Value.
/// Numeric types all become JSON numbers; serde_json's as_f64() handles both
/// integer and float JSON numbers, so BPM sent as Int(130) works correctly.
fn osc_arg_to_json(arg: &OscType) -> Option<serde_json::Value> {
    match arg {
        OscType::Float(f) => Some(serde_json::json!(*f)),
        OscType::Double(d) => Some(serde_json::json!(*d)),
        OscType::Int(i) => Some(serde_json::json!(*i)),
        OscType::Long(l) => Some(serde_json::json!(*l)),
        OscType::Bool(b) => Some(serde_json::json!(*b)),
        OscType::String(s) => Some(serde_json::json!(s)),
        _ => None,
    }
}

// ─── Dispatch ─────────────────────────────────────────────────────────────────

fn dispatch(action: OscAction, state: &Arc<RwLock<AppState>>, llm_tx: &Sender<LlmInput>) {
    match action {
        OscAction::ParamUpdate(update) => {
            let snapshot = state.read().clone();
            let next = apply_llm_update(snapshot, &update, &[]);
            *state.write() = next;
        }
        OscAction::Play => {
            state.write().sequencer.running = true;
        }
        OscAction::Stop => {
            state.write().sequencer.running = false;
        }
        OscAction::Prompt(text) => {
            let _ = llm_tx.try_send(LlmInput::Infer {
                prompt: text,
                one_shot: false,
                agent_id: None,
            });
        }
    }
}
