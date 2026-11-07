//! Jungle-specific arrangement instructions replace the generic acid examples.
use super::lanes::LaneKind;
use crate::state::AppState;

pub fn task(state: &AppState, lane: LaneKind) -> Option<String> {
    if state.llm.active_style.as_deref() != Some("jungle") {
        return None;
    }
    let text = match lane {
        LaneKind::Bass(idx) => {
            let (root, _) = super::lane_composition::lane_key(state, lane);
            let tonic = 36 + root % 12 + if idx == 0 { 0 } else { 12 };
            return Some(format!(
                "Write ONLY bass voice #{} using its sequencer fields and optional synth params. JUNGLE SUB: use MIDI {tonic} as the anchor, {} as an occasional fifth, and {} as an octave answer. The synth's sine sub is one octave below the played pitch. Choose a full-loop rhythm from the schema: spread attacks through EVERY quarter of the playing loop, e.g. [0,10,16,26] at 32 steps. Never bunch them into [0,1,2,3,4,5]. Repeat the motif across both bars; silence is only for an explicit mute request. Keep the sub centred, low resonance, gentle filter envelope, no acid squelch or mandatory slides. Preserve the current bass volume and amp envelope when writing a pattern; the amp release supplies its sustained weight. Accents/slides may be []. Supporting bass voices answer sparsely in the higher register or remain silent; do not stack competing low melodies. Notes are step-indexed; use the tonic on REST steps too, never MIDI 0 as a rest marker. Keep the motif stable between jam cycles unless asked to rewrite it.",
                idx + 1,
                tonic + 7,
                tonic + 12
            ));
        }
        LaneKind::KitA | LaneKind::KitB => {
            "Write ONLY this drum kit's sequencer fields. JUNGLE LAYER: the sampled break is the main drummer. Use sparse reinforcement, not a second full beat. At the default 4 steps/beat and 32-step loop, kick example [0,10,16,26]; snare backbeat [4,12,20,28] only if the break needs reinforcement. No four-on-floor, no offbeat-house hats. Hats/claps can be [] when the break already supplies them. Preserve a recognisable backbeat; ghost hits are quieter, not displaced main snares. If no break is loaded, use a broken kick rhythm and rolling hats as a fallback, not house drums."
        }
        LaneKind::Amen => {
            "Write amen_steps and amen_slices plus optional amen playback params. JUNGLE BREAK: retain a recognisable sampled phrase and its backbeat; change a small turnaround near the phrase end, not every slice randomly. amen_slices is a STEP-INDEXED INTEGER ARRAY, never an object: 0=automatic, 1..slice_count selects a slice. Existing slice order is the starting motif. With a two-bar source and 16 equal slices, triggers every 2 steps reconstruct a 32-step phrase; adapt to the actual sample region and grid. Equal slice numbers are positions, NOT guaranteed kick/snare labels. Keep gate near 1, stutter normally 0, loop_mode false; enable bpm_stretch if source_bpm is known. Preserve sample path, source BPM and marker positions unless explicitly asked to change them. Preserve the current Amen volume and per-slice gains when editing rhythms; do not turn up the break or drown out the sub-bass. Do not pitch each slice randomly. Keep the first part recognisable and reserve a brief repeated chop for the ending."
        }
        LaneKind::Hoover | LaneKind::An1x => {
            "Write ONLY this lead/pad lane. JUNGLE SUPPORT: silence is valid. Otherwise use a sparse sustained tonic/fifth/octave above the bass; one recurring stab or pad motif, not an independent busy lead. Match the bass anchor and avoid simultaneous adjacent scale tones. Do not add detuned low drones or random melodic runs."
        }
        LaneKind::Settings => {
            "Keep Jungle at 160-175 BPM unless the user explicitly requests otherwise. Preserve the existing key, scale and loop length. Set scale_snap true for tonal writing unless the user requests chromatic freedom. Do not reset tempo to a generic 120 BPM."
        }
        _ => return None,
    };
    Some(text.into())
}

/// A small shared harmonic palette prevents independent in-scale lines from
/// rubbing against one another. Used only when scale snapping is enabled.
pub fn pitch_palette(state: &AppState, lane: LaneKind) -> Vec<u8> {
    let (root, scale) = super::lane_composition::lane_key(state, lane);
    let (low, high) = match lane {
        LaneKind::Bass(0) => (36, 59),
        LaneKind::Bass(_) => (48, 71),
        _ => (60, 83),
    };
    (low..=high)
        .filter(|&n| {
            let interval = (n as i16 - root as i16).rem_euclid(12) as u8;
            matches!(interval, 0 | 7) && crate::state::note_in_scale(n, root, scale)
        })
        .collect()
}

/// Some grammar backends ignore uniqueItems. Reject gate masks masquerading
/// as index lists rather than silently applying a different rhythm.
pub fn validate_rhythms(raw: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(seq) = raw.get("sequencer").and_then(serde_json::Value::as_object) {
        for (key, value) in seq {
            if !key.ends_with("_steps") {
                continue;
            }
            let values = value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("{key} must be an index list"))?;
            if key == "bass_steps"
                && !values.is_empty()
                && let Some(notes) = seq.get("bass_notes").and_then(serde_json::Value::as_array)
            {
                let span = notes.len();
                for quarter in 0..4 {
                    let start = quarter * span / 4;
                    let end = (quarter + 1) * span / 4;
                    anyhow::ensure!(
                        start == end
                            || values.iter().any(|v| v
                                .as_u64()
                                .is_some_and(|i| (start..end).contains(&(i as usize)))),
                        "bass_steps must cover every quarter of the {span}-step loop; use a full phrase such as [0,10,16,26] at 32 steps, not a cluster at the beginning"
                    );
                }
            }
            let mut previous = None;
            for value in values {
                let index = value
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("{key} must contain integer step indices"))?;
                anyhow::ensure!(
                    previous.is_none_or(|p| index > p),
                    "{key} must be a strictly increasing list of active step indices, e.g. [0,2,4,8], NOT a 0/1 gate mask or duplicate list"
                );
                previous = Some(index);
            }
        }
    }
    Ok(())
}

/// Short motifs with attacks in every quarter of the playing loop. Long
/// banks repeat the two-bar motif; short banks scale it to their own length.
pub fn bass_rhythm_options(span: usize) -> Vec<Vec<usize>> {
    let span = span.clamp(1, crate::state::MAX_STEPS);
    let motifs: [&[usize]; 4] = [
        &[0, 10, 16, 26],
        &[0, 6, 10, 16, 22, 26],
        &[2, 8, 18, 24],
        &[0, 8, 14, 18, 24, 30],
    ];
    motifs
        .iter()
        .map(|motif| {
            let mut steps: Vec<_> = if span <= 32 {
                motif.iter().map(|step| step * span / 32).collect()
            } else {
                (0..span)
                    .filter(|step| motif.contains(&(step % 32)))
                    .collect()
            };
            steps.dedup();
            steps
        })
        .collect()
}

/// A newly requested Jungle rack starts with the break, not restored bass.
/// Explicitly locked patterns remain user-owned.
pub fn prepare_intro(mut state: AppState) -> AppState {
    if state.llm.active_style.as_deref() != Some("jungle") {
        return state;
    }
    state.llm.lane_scores.remove("amen");
    crate::state::apply_llm_update(
        state,
        &serde_json::json!({"sequencer": {
            "bass_steps": [], "bass2_steps": [], "bass3_steps": [], "bass4_steps": [],
            "amen_steps": []
        }}),
        &[],
    )
}

/// Keep explicit single-lane edits intact; when composing several lanes,
/// establish tempo first, then the break, then the supporting instruments.
pub fn order_intro(lanes: &mut [LaneKind]) {
    lanes.sort_by_key(|lane| match lane {
        LaneKind::Settings => 0,
        LaneKind::Amen => 1,
        _ => 2,
    });
}
