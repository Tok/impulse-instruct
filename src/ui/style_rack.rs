// ─── ui/style_rack.rs ──────────────────────────────────────────────────────
// Style → rack auto-sync.  When the user picks a style with a `rack_modules`
// declaration in styles.json, the rack is reconfigured to match: voices and
// FX not in the spec are removed, missing ones are added, and bass voice
// count is set per the `bassN` notation.  Always-keep chrome (sequencer,
// master, console, agents, TTS) is never touched, and the LFO floor is
// enforced (≥3 LFO modules so per-knob modulation always has slots).
//
// Entry-name notation supports a trailing digit count:
//   "bass"  → 1 bass voice enabled (single AcidBass module)
//   "bass2" → 2 bass voices enabled
//   "lfo"   → 1 LFO module (then floored to ≥3)
//   "lfo3"  → 3 LFO modules
//   "808", "reverb", "delay", … → singleton modules

use crate::state::{MAX_BASS_VOICES, ModuleKind, parse_module_kind};
use crate::ui::ImpulseApp;
use std::collections::HashMap;

/// Always-keep chrome — never removed regardless of style spec.
fn is_always_keep(kind: ModuleKind) -> bool {
    matches!(
        kind,
        ModuleKind::StepSequencer
            | ModuleKind::MasterOutput
            | ModuleKind::LlmConsole
            | ModuleKind::LlmAgent
            | ModuleKind::NeuTts
    )
}

/// Minimum number of LFO modules to keep in any rack — modulation
/// infrastructure that's always available even if the style doesn't
/// ask for it.  Styles can request MORE via `lfoN`.
const MIN_LFO_COUNT: usize = 3;

/// Split a `"bass2"` / `"lfo3"` / `"reverb"` entry into (base_name, count).
/// Trailing digits are parsed as the count; absence means count = 1.
/// Aliases that ARE digits (`"808"`, `"909"`) keep their full name as the
/// base — the digits are part of the alias, not a count suffix.
fn split_count(s: &str) -> (String, u8) {
    let trimmed = s.trim_end_matches(|c: char| c.is_ascii_digit());
    // Whole string is digits, or no digit suffix at all → no count to peel.
    if trimmed.is_empty() || trimmed.len() == s.len() {
        return (s.to_string(), 1);
    }
    let count_str = &s[trimmed.len()..];
    let count = count_str.parse::<u8>().unwrap_or(1).max(1);
    (trimmed.to_string(), count)
}

/// Sync the rack to match `names` — destructive: removes voices/FX not in
/// the spec, adds missing ones, applies bass-voice count, enforces the LFO
/// floor, re-wires default cables, recompiles the FX plan.
pub fn apply(app: &mut ImpulseApp, names: &[String]) {
    if names.is_empty() {
        return;
    }

    // Parse spec into kind → max(count) map.  Repeated entries (or both
    // forms — `bass` and `bass2`) collapse via max so the more demanding
    // count wins.
    let mut spec: HashMap<ModuleKind, u8> = HashMap::new();
    for entry in names {
        let (base, count) = split_count(entry);
        if let Some(kind) = parse_module_kind(&base) {
            let cur = spec.get(&kind).copied().unwrap_or(0);
            spec.insert(kind, cur.max(count));
        }
    }
    if spec.is_empty() {
        return;
    }

    let mut removed: Vec<ModuleKind> = Vec::new();
    let mut added: Vec<ModuleKind> = Vec::new();
    {
        let mut s = app.state.write();

        // Step 1: remove modules not in spec (always-keep chrome stays).
        let to_remove: Vec<u32> = s
            .rack
            .modules
            .iter()
            .filter(|m| !is_always_keep(m.kind) && !spec.contains_key(&m.kind))
            .map(|m| m.id)
            .collect();
        for id in &to_remove {
            if let Some(m) = s.rack.modules.iter().find(|x| x.id == *id) {
                removed.push(m.kind);
            }
            s.rack.remove_module(*id);
        }

        // Step 2: add missing modules; for LfoModule honour the requested
        // count (extras kept, deficit added).  Other kinds are singletons.
        for (&kind, &count) in &spec {
            if kind == ModuleKind::LfoModule {
                let need = count as usize;
                while s
                    .rack
                    .modules
                    .iter()
                    .filter(|m| m.kind == ModuleKind::LfoModule)
                    .count()
                    < need
                {
                    s.rack.add_module(ModuleKind::LfoModule);
                    added.push(ModuleKind::LfoModule);
                }
            } else if !s.rack.modules.iter().any(|m| m.kind == kind) {
                s.rack.add_module(kind);
                added.push(kind);
            }
        }

        // Step 3: bass voice count — `bassN` enables N bass voice slots.
        if let Some(&bass_count) = spec.get(&ModuleKind::AcidBass) {
            let target = (bass_count as usize).clamp(1, MAX_BASS_VOICES);
            for i in 0..MAX_BASS_VOICES {
                let want = i < target;
                s.sequencer.bass_voice_enabled[i] = want;
                if let Some(v) = s.bass_voices.get_mut(i) {
                    v.enabled = want;
                }
            }
        }

        // Step 4: LFO floor — always at least MIN_LFO_COUNT LFO modules.
        while s
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::LfoModule)
            .count()
            < MIN_LFO_COUNT
        {
            s.rack.add_module(ModuleKind::LfoModule);
            added.push(ModuleKind::LfoModule);
        }

        // Step 5: re-wire default cables (any new modules get connected),
        // then run the canonical arrange (matches the ARRANGE toolbar
        // button) so the rack stays compact after add/remove churn.
        s.rack.wire_default_cables();
        s.rack.arrange_canonical();
    }

    if removed.is_empty() && added.is_empty() {
        return;
    }
    let mut parts: Vec<String> = Vec::new();
    if !removed.is_empty() {
        let names: Vec<String> = removed.iter().map(|k| format!("{:?}", k)).collect();
        parts.push(format!("- {}", names.join(", ")));
    }
    if !added.is_empty() {
        let names: Vec<String> = added.iter().map(|k| format!("{:?}", k)).collect();
        parts.push(format!("+ {}", names.join(", ")));
    }
    app.log_text
        .push_str(&format!("Style rack: {}\n", parts.join(" | ")));
    app.push_fx_plan();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_count_no_digit() {
        assert_eq!(split_count("bass"), ("bass".into(), 1));
        assert_eq!(split_count("reverb"), ("reverb".into(), 1));
    }

    #[test]
    fn split_count_with_digit() {
        assert_eq!(split_count("bass2"), ("bass".into(), 2));
        assert_eq!(split_count("lfo3"), ("lfo".into(), 3));
        assert_eq!(split_count("bass4"), ("bass".into(), 4));
    }

    #[test]
    fn split_count_zero_clamped_to_one() {
        assert_eq!(split_count("bass0"), ("bass".into(), 1));
    }

    #[test]
    fn split_count_all_digit_alias_preserved() {
        // "808" / "909" are their own aliases, not "80" + "8".  Stripping
        // would empty the base, so we keep the full string and count = 1.
        assert_eq!(split_count("808"), ("808".into(), 1));
        assert_eq!(split_count("909"), ("909".into(), 1));
    }
}
