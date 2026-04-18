// ─── llm/prompt_summary.rs ───────────────────────────────────────────────────
// Small pure helpers that summarise AppState into short status lines for the
// system prompt.  Extracted from `prompt.rs` so the main prompt builder stays
// under the 1000-line file cap.

use crate::state::{AppState, ModuleKind};

/// Comma-separated list of active bass step indices, or "none (silent)".
pub fn bass_active_steps_summary(state: &AppState) -> String {
    let hits: Vec<usize> = state
        .sequencer
        .bass_pattern
        .iter()
        .enumerate()
        .filter(|(_, s)| s.active)
        .map(|(i, _)| i)
        .collect();
    if hits.is_empty() {
        "none (silent)".to_string()
    } else {
        hits.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// "Active bass voices (N of M): [#1, #2, …]" so the agent knows how many
/// voices are live and which indices to target.  When more than one voice
/// is active the line is followed by an explicit directive — the default
/// failure mode is writing only `bass_*` and ignoring the other voices.
pub fn bass_voices_summary(state: &AppState) -> String {
    let active: Vec<usize> = state
        .bass_voices
        .iter()
        .enumerate()
        .filter(|(_, v)| v.enabled)
        .map(|(i, _)| i)
        .collect();
    let header = format!(
        "Active bass voices ({} of {}): [{}]",
        active.len(),
        state.bass_voices.len(),
        active
            .iter()
            .map(|i| format!("#{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if active.len() < 2 {
        return header;
    }
    // Multi-voice hard rule — listed directly alongside the count so it
    // can't be skimmed past.
    let per_voice_keys = active
        .iter()
        .map(|&i| {
            if i == 0 {
                "bass_steps+bass_notes".to_string()
            } else {
                format!("bass{i}_steps+bass{i}_notes", i = i + 1)
            }
        })
        .collect::<Vec<_>>()
        .join(" AND ");
    format!(
        "{}\n\
         MULTI-VOICE: write a DISTINCT pattern for each active voice — \
         {}. Different rhythms/contours, not clones.",
        header, per_voice_keys
    )
}

/// Short list of voice modules the agent is actually expected to drive —
/// any `ModuleKind` that is enabled AND wired to MASTER.  Used to tell the
/// agent "these are the voices you must populate on initial generation"
/// so it doesn't silently skip the 808/909 when a drum kit is patched in.
pub fn rack_voice_coverage_summary(state: &AppState) -> String {
    let has = |kind: ModuleKind| -> bool {
        state
            .rack
            .modules
            .iter()
            .any(|m| m.kind == kind && m.enabled && state.rack.reaches_master(m.id))
    };
    let mut present: Vec<&'static str> = Vec::new();
    // Voice / sequencer-driven modules, in prompt-friendly order.
    let checks: &[(ModuleKind, &str)] = &[
        (ModuleKind::AcidBass, "bass"),
        (ModuleKind::HooverLead, "hoover"),
        (ModuleKind::An1xVoice, "an1x"),
        (ModuleKind::DrumKit808, "kit_a (808)"),
        (ModuleKind::DrumKit909, "kit_b (909)"),
        (ModuleKind::AmenSampler, "amen"),
        (ModuleKind::GabberKick, "gabber_kick"),
        (ModuleKind::NoiseVoice, "noise"),
        (ModuleKind::GranularTexture, "granular"),
        (ModuleKind::NeuTts, "tts"),
    ];
    for (kind, label) in checks {
        if has(*kind) {
            present.push(label);
        }
    }
    if present.is_empty() {
        "Active voices (wired to MASTER): none".to_string()
    } else {
        format!(
            "Active voices (wired to MASTER): [{}]\n\
             COVERAGE: on a beat-driven style (acid/house/techno/D&B/\
             dubstep/jungle/gabber/etc) every drum voice above MUST have \
             a pattern — not just bass. Ambient/drone/meditation/dub \
             styles may keep drums sparse or silent.",
            present.join(", ")
        )
    }
}

/// "Accent steps: ... | Slide steps: ..." — shows current groove density so
/// the agent can extend/vary rather than restart from scratch.
pub fn bass_groove_summary(state: &AppState) -> String {
    let accents: Vec<usize> = state
        .sequencer
        .bass_pattern
        .iter()
        .enumerate()
        .filter(|(_, s)| s.active && s.accent > 0.0)
        .map(|(i, _)| i)
        .collect();
    let slides: Vec<usize> = state
        .sequencer
        .bass_pattern
        .iter()
        .enumerate()
        .filter(|(_, s)| s.active && s.slide > 0.0)
        .map(|(i, _)| i)
        .collect();
    let fmt = |v: &[usize]| -> String {
        if v.is_empty() {
            "none".to_string()
        } else {
            v.iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    format!(
        "Accent steps: {} | Slide steps: {}",
        fmt(&accents),
        fmt(&slides)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voices_summary_counts_enabled() {
        let mut s = AppState::default();
        s.bass_voices[0].enabled = true;
        s.bass_voices[1].enabled = true;
        let out = bass_voices_summary(&s);
        assert!(out.starts_with("Active bass voices (2 of"));
        assert!(out.contains("#1"));
        assert!(out.contains("#2"));
    }

    #[test]
    fn voices_summary_emits_multi_voice_rule() {
        let mut s = AppState::default();
        s.bass_voices[0].enabled = true;
        s.bass_voices[1].enabled = true;
        let out = bass_voices_summary(&s);
        assert!(out.contains("MULTI-VOICE"));
        assert!(out.contains("bass_steps+bass_notes"));
        assert!(out.contains("bass2_steps+bass2_notes"));
    }

    #[test]
    fn voices_summary_single_voice_no_multi_rule() {
        let mut s = AppState::default();
        s.bass_voices[0].enabled = true;
        // voice 1 disabled — only voice 0 active, no multi-voice directive
        let out = bass_voices_summary(&s);
        assert!(!out.contains("MULTI-VOICE"));
    }

    #[test]
    fn groove_summary_reports_accents_and_slides() {
        let mut s = AppState::default();
        s.sequencer.bass_pattern[0].active = true;
        s.sequencer.bass_pattern[0].accent = 1.0;
        s.sequencer.bass_pattern[3].active = true;
        s.sequencer.bass_pattern[3].slide = 1.0;
        let out = bass_groove_summary(&s);
        assert!(out.contains("Accent steps: 0"));
        assert!(out.contains("Slide steps: 3"));
    }

    #[test]
    fn groove_summary_none_when_empty() {
        let s = AppState::default();
        let out = bass_groove_summary(&s);
        assert!(out.contains("Accent steps: none"));
        assert!(out.contains("Slide steps: none"));
    }

    #[test]
    fn rack_coverage_empty_when_no_voices() {
        // Empty rack — remove all voice modules so the default state has
        // nothing wired to MASTER.
        use crate::state::RackState;
        let mut s = AppState::default();
        s.rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 100,
            dyn_sequencer_rows: None,
        };
        let out = rack_voice_coverage_summary(&s);
        assert!(out.contains("none"));
        assert!(!out.contains("COVERAGE"));
    }

    #[test]
    fn rack_coverage_lists_wired_voices_and_rule() {
        use crate::state::{RACK_PRESETS, RackState};
        // "Full" preset includes bass + 808 + 909 + amen wired to MASTER.
        let full_preset = RACK_PRESETS
            .iter()
            .find(|p| p.name == "Full")
            .expect("Full preset present");
        let rack = RackState::from_preset(full_preset);
        let mut s = AppState::default();
        s.rack = rack;
        let out = rack_voice_coverage_summary(&s);
        assert!(out.contains("COVERAGE"));
        // We don't lock the exact preset contents; just assert we see a
        // drum module (the reason the rule exists) and the bass.
        assert!(out.contains("bass"));
        // At least one of the kit labels must appear so the rule has teeth.
        assert!(
            out.contains("kit_a") || out.contains("kit_b") || out.contains("amen"),
            "rack coverage should list at least one drum voice; got: {out}"
        );
        // Style-precedence caveat must be present so ambient styles don't
        // accidentally get forced drums.
        assert!(out.to_lowercase().contains("ambient"));
    }

    #[test]
    fn rack_coverage_matches_module_kind_list() {
        // Bass-only rack: only "bass" should appear in the coverage line.
        use crate::state::{ModuleKind, PortDir, PortKind, PortRef, RackModule, RackState};
        let mut rack = RackState::default();
        rack.modules.clear();
        rack.modules.push(RackModule::new(1, ModuleKind::AcidBass));
        rack.modules
            .push(RackModule::new(2, ModuleKind::MasterOutput));
        let _ = rack.connect(
            PortRef {
                module_id: 1,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: 2,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        );
        let mut s = AppState::default();
        s.rack = rack;
        let out = rack_voice_coverage_summary(&s);
        assert!(out.contains("bass"));
        assert!(!out.contains("kit_a"));
        assert!(!out.contains("kit_b"));
    }
}
