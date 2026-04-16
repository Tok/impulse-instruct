// ─── state/llm_rack.rs ────────────────────────────────────────────────────────
// LLM "rack" JSON block handler — add / remove / enable / disable / connect /
// disconnect / mod_cable.  Extracted from llm_apply.rs to keep that file
// under the 1000-line cap.  All behaviour preserved verbatim.

use super::modulation::{apply_llm_mod_cable_entry, rack_out_port_kind};
use super::rack::{PortDir, PortKind, PortRef};
use super::rack_scope::{parse_module_kind, rack_kind_name_matches};
use super::{AppState, ModuleKind, TtsModuleState, Zone};

/// Apply a `"rack"` sub-object of an LLM update to the AppState's rack.
/// Mirrors the behaviour of the /api/rack/* endpoints: auto-wires added
/// voice/FX modules to MasterOutput, skips duplicate cables, respects
/// cycle detection.
pub(crate) fn apply_llm_rack_update(
    s: &mut AppState,
    rack_upd: &serde_json::Map<String, serde_json::Value>,
) {
    // ── rack.add — create new modules from a list of kind strings.
    // Auto-wires voice/FX modules to MasterOutput so they're audible
    // without a second "connect" action (matches /api/rack/add behavior).
    if let Some(arr) = rack_upd.get("add").and_then(|v| v.as_array()) {
        for v in arr {
            let Some(name) = v.as_str() else { continue };
            let Some(kind) = parse_module_kind(name) else {
                continue;
            };
            let id = s.rack.add_module(kind);
            if !matches!(kind.default_zone(), Zone::Global | Zone::Ai)
                && let Some(master_id) = s
                    .rack
                    .modules
                    .iter()
                    .find(|m| m.kind == ModuleKind::MasterOutput)
                    .map(|m| m.id)
            {
                s.rack.connect(
                    PortRef {
                        module_id: id,
                        dir: PortDir::Out,
                        kind: PortKind::Audio,
                        index: 0,
                    },
                    PortRef {
                        module_id: master_id,
                        dir: PortDir::In,
                        kind: PortKind::Audio,
                        index: 0,
                    },
                );
            }
            if kind == ModuleKind::NeuTts {
                s.tts_modules.push(TtsModuleState::new(id));
            }
            // Scroll the UI to the newly-added module so it's visible
            // in the rack (matches /api/rack/add behavior).
            s.scroll_target = Some(name.to_string());
        }
    }
    // ── rack.remove — delete the first module matching each kind string.
    if let Some(arr) = rack_upd.get("remove").and_then(|v| v.as_array()) {
        for v in arr {
            let Some(name) = v.as_str() else { continue };
            let target = s
                .rack
                .modules
                .iter()
                .find(|m| rack_kind_name_matches(m.kind, name))
                .map(|m| m.id);
            if let Some(id) = target {
                s.rack.remove_module(id);
                s.tts_modules.retain(|t| t.id != id);
            }
        }
    }
    if let Some(arr) = rack_upd.get("enable").and_then(|v| v.as_array()) {
        for v in arr {
            if let Some(name) = v.as_str() {
                for m in &mut s.rack.modules {
                    if rack_kind_name_matches(m.kind, name) {
                        m.enabled = true;
                    }
                }
            }
        }
    }
    if let Some(arr) = rack_upd.get("disable").and_then(|v| v.as_array()) {
        for v in arr {
            if let Some(name) = v.as_str() {
                for m in &mut s.rack.modules {
                    if rack_kind_name_matches(m.kind, name) {
                        m.enabled = false;
                    }
                }
            }
        }
    }
    if let Some(arr) = rack_upd.get("connect").and_then(|v| v.as_array()) {
        for v in arr {
            let from_name = v.get("from").and_then(|v| v.as_str());
            let to_name = v.get("to").and_then(|v| v.as_str());
            if let (Some(fn_), Some(tn)) = (from_name, to_name) {
                let from_mod = s
                    .rack
                    .modules
                    .iter()
                    .find(|m| rack_kind_name_matches(m.kind, fn_))
                    .map(|m| (m.id, m.kind));
                let to_mod = s
                    .rack
                    .modules
                    .iter()
                    .find(|m| rack_kind_name_matches(m.kind, tn))
                    .map(|m| (m.id, m.kind));
                if let (Some((fid, fkind)), Some((tid, _tkind))) = (from_mod, to_mod) {
                    // Don't duplicate an existing cable
                    let exists = s
                        .rack
                        .cables
                        .iter()
                        .any(|c| c.from.module_id == fid && c.to.module_id == tid);
                    if !exists {
                        let port_kind = rack_out_port_kind(fkind);
                        s.rack.connect(
                            PortRef {
                                module_id: fid,
                                dir: PortDir::Out,
                                kind: port_kind,
                                index: 0,
                            },
                            PortRef {
                                module_id: tid,
                                dir: PortDir::In,
                                kind: port_kind,
                                index: 0,
                            },
                        );
                    }
                }
            }
        }
    }
    if let Some(arr) = rack_upd.get("disconnect").and_then(|v| v.as_array()) {
        for v in arr {
            let from_name = v.get("from").and_then(|v| v.as_str());
            let to_name = v.get("to").and_then(|v| v.as_str());
            if let (Some(fn_), Some(tn)) = (from_name, to_name) {
                let from_mod = s
                    .rack
                    .modules
                    .iter()
                    .find(|m| rack_kind_name_matches(m.kind, fn_))
                    .map(|m| (m.id, m.kind));
                let to_id = s
                    .rack
                    .modules
                    .iter()
                    .find(|m| rack_kind_name_matches(m.kind, tn))
                    .map(|m| m.id);
                if let (Some((fid, fkind)), Some(tid)) = (from_mod, to_id) {
                    let port_kind = rack_out_port_kind(fkind);
                    let from_ref = PortRef {
                        module_id: fid,
                        dir: PortDir::Out,
                        kind: port_kind,
                        index: 0,
                    };
                    let to_ref = PortRef {
                        module_id: tid,
                        dir: PortDir::In,
                        kind: port_kind,
                        index: 0,
                    };
                    s.rack.disconnect(&from_ref, &to_ref);
                }
            }
        }
    }

    // rack.mod_cable — per-knob LFO modulation patching.
    if let Some(arr) = rack_upd.get("mod_cable").and_then(|v| v.as_array()) {
        for v in arr {
            apply_llm_mod_cable_entry(&mut s.rack, v);
        }
    }
}
