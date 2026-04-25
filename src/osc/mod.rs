// ─── osc/mod.rs ───────────────────────────────────────────────────────────────
// OSC (Open Sound Control) UDP listener.
//
// Listens on a UDP port and translates incoming OSC messages into synth
// parameter changes, transport control, and LLM prompts.
//
// Address scheme:
//   /impulse/<section>/<param>  <value>     → apply param (same path as HTTP API)
//   /impulse/sequencer/play                 → start transport
//   /impulse/sequencer/stop                 → stop transport
//   /impulse/prompt             <string>    → send prompt to LLM (jam mode)
//   /impulse/lock               <path>      → lock a param dot-path
//   /impulse/unlock             <path>      → unlock a param dot-path
//   /impulse/scroll             <target>    → scroll UI to target zone / module
//   /impulse/preset             <name>      → apply rack preset by name
//   /impulse/style              <id|"">     → set global style (empty string clears)
//
// Examples (oscsend / TouchOSC / Max):
//   /impulse/bass/cutoff       0.7
//   /impulse/bass/resonance    0.85
//   /impulse/fx/reverb_mix     0.5
//   /impulse/sequencer/bpm     130
//   /impulse/sequencer/play
//   /impulse/sequencer/stop
//   /impulse/prompt            "make it darker"
//   /impulse/lock              "bass.cutoff"
//   /impulse/style             "drum_and_bass"
//   /impulse/preset            "Crew"
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

/// Translated OSC dispatch.  Made `pub(crate)` so the test module can
/// drive `parse_osc_addr` end-to-end without poking at private types.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OscAction {
    ParamUpdate(serde_json::Value),
    Play,
    Stop,
    Prompt(String),
    Lock(String),
    Unlock(String),
    Scroll(String),
    /// `Some(id)` sets the active style, `None` clears it (equivalent
    /// to the HTTP `POST /api/style` with `null`).  An OSC string of
    /// length 0 maps to `None` so TouchOSC users can clear the style
    /// from a single text widget without a separate command.
    Style(Option<String>),
    Preset(String),
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

pub(crate) fn parse_osc_addr(addr: &str, args: &[OscType]) -> Option<OscAction> {
    // Strip leading slash and split into up to 3 segments
    let parts: Vec<&str> = addr.trim_start_matches('/').splitn(3, '/').collect();

    // All addresses must start with "impulse"
    if parts.first() != Some(&"impulse") {
        return None;
    }

    let arg_string = || -> Option<String> {
        args.first().and_then(|a| match a {
            OscType::String(s) => Some(s.clone()),
            _ => None,
        })
    };

    match parts.as_slice() {
        ["impulse", "sequencer", "play"] => Some(OscAction::Play),
        ["impulse", "sequencer", "stop"] => Some(OscAction::Stop),
        ["impulse", "prompt"] => Some(OscAction::Prompt(arg_string()?)),
        ["impulse", "lock"] => Some(OscAction::Lock(arg_string()?)),
        ["impulse", "unlock"] => Some(OscAction::Unlock(arg_string()?)),
        ["impulse", "scroll"] => Some(OscAction::Scroll(arg_string()?)),
        ["impulse", "preset"] => Some(OscAction::Preset(arg_string()?)),
        ["impulse", "style"] => {
            // Empty string clears the style (matches POST /api/style with null).
            let s = arg_string()?;
            Some(OscAction::Style(if s.is_empty() { None } else { Some(s) }))
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
        OscAction::Lock(path) => {
            let snapshot = state.read().clone();
            let next = crate::state::lock_params(snapshot, &[path.as_str()]);
            *state.write() = next;
            log::info!("OSC: locked {path}");
        }
        OscAction::Unlock(path) => {
            let snapshot = state.read().clone();
            let next = crate::state::unlock_params(snapshot, &[path.as_str()]);
            *state.write() = next;
            log::info!("OSC: unlocked {path}");
        }
        OscAction::Scroll(target) => {
            // Mirror POST /api/scroll's primary effect — set the
            // scroll target so the UI's per-frame poll picks it up
            // and animates to the right zone / module.
            state.write().scroll_target = Some(target.clone());
            log::info!("OSC: scroll → {target}");
        }
        OscAction::Style(maybe_id) => {
            let snapshot = state.read().clone();
            let next = match maybe_id {
                Some(id) => {
                    let mut s = snapshot;
                    s.llm.active_style = Some(id.clone());
                    let after = crate::state::propagate_style(s, &id);
                    log::info!("OSC: style → {id}");
                    after
                }
                None => {
                    let mut s = snapshot;
                    s.llm.active_style = None;
                    log::info!("OSC: style cleared");
                    s
                }
            };
            *state.write() = next;
        }
        OscAction::Preset(name) => {
            // Preset application is non-trivial (rack mutation +
            // agent roster swap + model pinning) — mirroring the
            // full HTTP handler in sync code is out of V1 scope.
            // For now, just log; users wanting preset switching
            // via OSC can route through the HTTP API.
            log::warn!(
                "OSC: preset '{name}' ignored — preset application not yet mirrored in OSC; \
                 use POST /api/preset"
            );
        }
    }
}
