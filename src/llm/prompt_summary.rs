// ─── llm/prompt_summary.rs ───────────────────────────────────────────────────
// Small pure helpers that summarise AppState into short status lines for the
// system prompt.  Extracted from `prompt.rs` so the main prompt builder stays
// under the 1000-line file cap.

use crate::state::AppState;

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
/// voices are live and which indices to target.
pub fn bass_voices_summary(state: &AppState) -> String {
    let active: Vec<usize> = state
        .bass_voices
        .iter()
        .enumerate()
        .filter(|(_, v)| v.enabled)
        .map(|(i, _)| i)
        .collect();
    format!(
        "Active bass voices ({} of {}): [{}]",
        active.len(),
        state.bass_voices.len(),
        active
            .iter()
            .map(|i| format!("#{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// "Accent steps: ... | Slide steps: ..." — shows current groove density so
/// the agent can extend/vary rather than restart from scratch.
pub fn bass_groove_summary(state: &AppState) -> String {
    let accents: Vec<usize> = state
        .sequencer
        .bass_pattern
        .iter()
        .enumerate()
        .filter(|(_, s)| s.active && s.accent)
        .map(|(i, _)| i)
        .collect();
    let slides: Vec<usize> = state
        .sequencer
        .bass_pattern
        .iter()
        .enumerate()
        .filter(|(_, s)| s.active && s.slide)
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
    fn groove_summary_reports_accents_and_slides() {
        let mut s = AppState::default();
        s.sequencer.bass_pattern[0].active = true;
        s.sequencer.bass_pattern[0].accent = true;
        s.sequencer.bass_pattern[3].active = true;
        s.sequencer.bass_pattern[3].slide = true;
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
}
