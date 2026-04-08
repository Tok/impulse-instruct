// ─── llm/json_repair.rs ──────────────────────────────────────────────────────
// JSON repair and sanitisation for LLM responses.
//
// Models occasionally emit:
//   • Truncated JSON (max_tokens reached mid-object)
//   • Valid JSON with structural errors (wrong nesting, dot-notation fields)
//   • Hallucinated top-level keys not in the schema
//
// `repair_json` is the entry point — it tries clean parse first, then
// bracket-closing repair, then sanitisation.

/// Parse `s` as JSON; if that fails, attempt bracket-closing repair.
/// In both cases run through `sanitize_json_structure` before returning.
pub fn repair_json(s: &str) -> Option<serde_json::Value> {
    // Fast path — valid JSON
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        return Some(sanitize_json_structure(v));
    }

    log::warn!("JSON parse failed — attempting repair (truncated/malformed output)");

    // Build a repaired candidate by closing unclosed brackets.
    let mut attempt = s.trim_end_matches([',', ' ', '\n', '\r', ':']).to_string();

    // Count unclosed nesting levels (ignore chars inside strings).
    let (mut obj_depth, mut arr_depth) = (0i32, 0i32);
    let mut in_str = false;
    let mut esc = false;
    for c in attempt.chars() {
        if esc {
            esc = false;
            continue;
        }
        if c == '\\' && in_str {
            esc = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if !in_str {
            match c {
                '{' => obj_depth += 1,
                '}' => obj_depth -= 1,
                '[' => arr_depth += 1,
                ']' => arr_depth -= 1,
                _ => {}
            }
        }
    }
    // Close in reverse order: arrays first, then objects
    for _ in 0..arr_depth.max(0) {
        attempt.push(']');
    }
    for _ in 0..obj_depth.max(0) {
        attempt.push('}');
    }

    serde_json::from_str::<serde_json::Value>(&attempt)
        .ok()
        .map(sanitize_json_structure)
}

/// Promote `bass` and `fx` keys that the model incorrectly nests inside
/// `sequencer` back to the top level, strip `"fx"` keys nested inside `"fx"`,
/// normalise LFO format variants to an indexed array, and strip hallucinated
/// top-level keys not in the schema.
pub fn sanitize_json_structure(v: serde_json::Value) -> serde_json::Value {
    let mut obj = match v {
        serde_json::Value::Object(m) => m,
        other => return other,
    };

    // Extract misplaced keys from inside "sequencer"
    let (bass_lift, fx_lift) =
        if let Some(seq) = obj.get_mut("sequencer").and_then(|s| s.as_object_mut()) {
            (seq.remove("bass"), seq.remove("fx"))
        } else {
            (None, None)
        };
    if let Some(b) = bass_lift {
        obj.entry("bass").or_insert(b);
    }
    if let Some(f) = fx_lift {
        obj.entry("fx").or_insert(f);
    }

    // Remove nested "fx" inside "fx" (the recursive loop the model falls into)
    if let Some(fx) = obj.get_mut("fx").and_then(|f| f.as_object_mut()) {
        fx.remove("fx");
    }

    // Fix LFO format variants — model emits one of three formats:
    //   A) dot-notation: {"lfo[0].enabled": true, "lfo[0].rate": 0.1}
    //   B) named-slot:   {"lfo_0": {"enabled": true}, "lfo_1": {...}}   ← Bonsai
    //   C) correct array: [{"enabled": true}, ...]
    // Normalise A and B → C (array indexed 0–3).
    if let Some(lfo_val) = obj.get("lfo")
        && let Some(lfo_obj) = lfo_val.as_object()
    {
        let has_dot_notation = lfo_obj
            .keys()
            .any(|k| k.starts_with("lfo[") && k.contains("]."));
        let has_named_slots = lfo_obj
            .keys()
            .any(|k| k.starts_with("lfo_") && k[4..].parse::<usize>().is_ok());

        if has_dot_notation {
            // Format A
            let mut slots: [serde_json::Map<String, serde_json::Value>; 4] = Default::default();
            for (key, val) in lfo_obj {
                if let Some(rest) = key.strip_prefix("lfo[")
                    && let Some(bracket) = rest.find("].")
                    && let Ok(idx) = rest[..bracket].parse::<usize>()
                    && idx < 4
                {
                    let field = &rest[bracket + 2..];
                    slots[idx].insert(field.to_string(), val.clone());
                }
            }
            obj.insert(
                "lfo".to_string(),
                serde_json::Value::Array(
                    slots.into_iter().map(serde_json::Value::Object).collect(),
                ),
            );
        } else if has_named_slots {
            // Format B: {"lfo_0": {...}, "lfo_1": {...}}
            let mut slots: [serde_json::Map<String, serde_json::Value>; 4] = Default::default();
            for (key, val) in lfo_obj {
                if let Some(idx_str) = key.strip_prefix("lfo_")
                    && let Ok(idx) = idx_str.parse::<usize>()
                    && idx < 4
                    && let Some(inner) = val.as_object()
                {
                    slots[idx] = inner.clone();
                }
            }
            obj.insert(
                "lfo".to_string(),
                serde_json::Value::Array(
                    slots.into_iter().map(serde_json::Value::Object).collect(),
                ),
            );
        }
    }

    // Strip hallucinated top-level keys that are not in the schema.
    // These cause no harm to apply_llm_update (it ignores unknown keys) but they
    // trip the no_unknown_top_level_keys llm_suite test and pollute logs.
    for key in ["drum_ratchets", "drums", "patterns", "ratchets"] {
        obj.remove(key);
    }

    serde_json::Value::Object(obj)
}

/// Split a `<think>…</think>` block from the front of a model response.
/// Returns `(thinking_text, remainder)`.  If no block is present, thinking is
/// None and the whole string is returned as remainder.
pub fn split_thinking(s: &str) -> (Option<String>, String) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("<think>")
        && let Some(end) = rest.find("</think>")
    {
        let thinking = rest[..end].trim().to_string();
        let after = rest[end + "</think>".len()..].trim().to_string();
        return (Some(thinking).filter(|t| !t.is_empty()), after);
    }
    (None, s.to_string())
}

/// Extract `LlmAction`s from the mutable JSON map, consuming "save_project"
/// and "settings" keys.  Pure function — no side effects.
pub fn extract_llm_actions(
    obj: &mut serde_json::Map<String, serde_json::Value>,
) -> Vec<super::LlmAction> {
    use super::LlmAction;
    let mut actions = Vec::new();
    if obj
        .remove("save_project")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        actions.push(LlmAction::SaveProject);
    }
    if let Some(s) = obj.get("settings").and_then(|v| v.as_object()).cloned() {
        if let Some(h) = s.get("heat").and_then(|v| v.as_f64()) {
            actions.push(LlmAction::SetHeat((h as f32).clamp(0.0, 1.0)));
        }
        if let Some(st) = s.get("style").and_then(|v| v.as_str()) {
            actions.push(LlmAction::SetStyle(st.to_string()));
        }
        if let Some(p) = s.get("persona").and_then(|v| v.as_str()) {
            actions.push(LlmAction::SetPersona(p.to_string()));
        }
        if let Some(m) = s.get("conversation_mode").and_then(|v| v.as_str()) {
            actions.push(LlmAction::SetConversationMode(m.to_string()));
        }
        if let Some(j) = s.get("jam_bars").and_then(|v| v.as_f64()) {
            actions.push(LlmAction::SetJamBars((j as f32).max(0.0)));
        }
        // spawn_agent: { persona, scope, model }
        if let Some(spawn) = s.get("spawn_agent").and_then(|v| v.as_object()) {
            let persona = spawn
                .get("persona")
                .and_then(|v| v.as_str())
                .unwrap_or("AGENT")
                .to_string();
            let scope = spawn
                .get("scope")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let model = spawn
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from);
            actions.push(LlmAction::SpawnAgent {
                persona,
                scope,
                model,
            });
        }
        // dismiss: true — the agent that produced this output dismisses itself
        if s.get("dismiss").and_then(|v| v.as_bool()).unwrap_or(false) {
            actions.push(LlmAction::DismissAgent);
        }
        // send_hint: { "to": "AgentName", "hint": "..." }
        if let Some(hint_obj) = s.get("send_hint").and_then(|v| v.as_object())
            && let (Some(to), Some(hint)) = (
                hint_obj.get("to").and_then(|v| v.as_str()),
                hint_obj.get("hint").and_then(|v| v.as_str()),
            )
        {
            actions.push(LlmAction::SendHint {
                to: to.to_string(),
                hint: hint.to_string(),
            });
        }
    }
    obj.remove("settings");
    actions
}
