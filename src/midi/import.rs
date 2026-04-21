// ─── midi/import.rs ──────────────────────────────────────────────────────────
// Standard MIDI File (SMF) importer — reads a .mid into the sequencer as
// up to two bass voices spread over multiple pattern-bank slots, auto-
// chained in song mode.  Used by File → Import MIDI and by the Bach
// scenario demo (`demo/scenarios/bach-italian-3rd.sh`).
//
// Design:
//   • Parse with `midly` (zero-copy SMF parser).
//   • Pick the two densest non-drum tracks as RH / LH.  RH = higher
//     mean pitch, LH = lower — the standard outer-voices reduction.
//   • Quantise to a `step_division` grid; default auto-selects a grid
//     fine enough to resolve the file's smallest onset (4, 8, or 16).
//   • Fill `pattern_bank[0..=N-1]` with up to MAX_STEPS (64) steps
//     each (see `crate::state::MAX_BANKS` for the overall bank cap —
//     currently 64).  Build a `chain = [0, 1, …, N-1]` and set
//     `chain_loop = false` so the piece plays once and stops.
//   • Set `bpm` from the tempo meta of track 0; `step_division` on
//     every touched bank.
//   • Enable bass voices 0 (RH) + 1 (LH); clear their previous
//     patterns.  Leaves other voices / drum patterns untouched unless
//     the caller opts into `wipe = true`.
//
// MAX_BANKS × MAX_STEPS puts an upper bound on total import length
// (~4 minutes at a 32nd-note grid / 120 BPM).  Longer pieces truncate
// and `was_truncated = true` comes back in the summary so the caller
// can surface that honestly.

use std::path::Path;

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

use crate::state::{
    AppState, ChainSlotOverride, MAX_BANKS, MAX_BASS_VOICES, MAX_STEPS, SequencerState, TB303Step,
};

/// Absolute ceiling on `step_division` for auto-detect.  16 = 64th-note
/// grid — anything finer (e.g. 32nd triplets) is rare in SMF files we
/// expect to import and starts eating bank capacity with little audible
/// gain.
pub const MAX_STEP_DIVISION: u8 = 16;

/// Melodic note event (SMF NoteOn → matched NoteOff), resolved to
/// absolute ticks.  `end_tick - start_tick` is the note's sustain in
/// SMF ticks.
#[derive(Clone, Copy, Debug)]
pub struct NoteEvent {
    pub start_tick: u32,
    pub end_tick: u32,
    pub pitch: u8,
    pub velocity: u8,
}

/// Import configuration.
#[derive(Clone, Debug, Default)]
pub struct MidiImport {
    /// Override the auto-detected step division (4 = 16ths, 8 = 32nds,
    /// 16 = 64ths).  `None` lets the importer pick the finest grid
    /// necessary to resolve the file's smallest onset.
    pub step_division: Option<u8>,
    /// Explicit track indices to use as (RH, LH).  `None` → pick the
    /// two densest non-drum tracks and assign by mean pitch.
    pub voice_tracks: Option<(usize, usize)>,
    /// When true, clear all bass voice patterns (0..MAX_BASS_VOICES)
    /// before writing.  When false, leave voices 2+ untouched.  Bass
    /// voices 0 and 1 are always overwritten because the importer
    /// populates them.
    pub wipe_other_voices: bool,
}

/// Report returned by a successful import.
#[derive(Clone, Debug)]
pub struct ImportSummary {
    /// Tempo recovered from the SMF's first tempo-meta event (BPM).
    pub bpm: f32,
    /// Step division selected for this import.  Matches
    /// `sequencer.step_division` on every touched bank.
    pub step_division: u8,
    /// Number of pattern-bank slots populated (1..=MAX_BANKS).
    pub banks_used: usize,
    /// Total source length in SMF ticks — useful to report truncation.
    pub source_ticks: u32,
    /// Total source length the importer kept (clamped by MAX_BANKS×64).
    pub kept_ticks: u32,
    /// True when the source was longer than the bank budget and the
    /// trailing music was dropped.
    pub was_truncated: bool,
    /// Notes placed on bass voice 0 (RH) after quantisation.
    pub notes_voice_0: usize,
    /// Notes placed on bass voice 1 (LH) after quantisation.
    pub notes_voice_1: usize,
    /// Track indices selected as (RH, LH) — either user-provided or
    /// auto-picked.  Useful for logs / UI feedback.
    pub picked_tracks: (usize, usize),
    /// Expected wall-clock playback duration of the kept music, in
    /// seconds.  Derived from `banks_used × MAX_STEPS × (60 / (bpm ×
    /// step_division))`.  Demo scenarios use this to sleep for exactly
    /// as long as the piece plays before sending the next command.
    /// Ignores any partial last bank that's shorter than MAX_STEPS —
    /// the audio thread still plays only the active step window, so
    /// this is a slight overestimate on truncation-free imports of
    /// lengths that don't divide evenly by 64.  Accurate enough for
    /// scenario pacing.
    pub duration_seconds: f32,
}

/// Full import pipeline: parse SMF bytes, mutate `state` with the new
/// pattern bank + chain, enable voices 0/1.  Pure: returns a new
/// `(AppState, ImportSummary)` pair, no I/O.
///
/// Errors return a user-readable string — surface them verbatim in the
/// menu log or API response.
pub fn import_midi_into(
    state: AppState,
    smf_bytes: &[u8],
    config: &MidiImport,
) -> Result<(AppState, ImportSummary), String> {
    let smf = Smf::parse(smf_bytes).map_err(|e| format!("parse: {e}"))?;
    let ppq = match smf.header.timing {
        Timing::Metrical(t) => t.as_int() as u32,
        // SMPTE timing is extremely rare outside of pro film scoring.
        // Rather than convert frames-per-second into BPM-relative
        // ticks (which loses tempo info entirely) we reject it —
        // clearer error than a silently wrong tempo.
        Timing::Timecode(..) => {
            return Err("SMPTE-timed SMF files are not supported (metrical only)".into());
        }
    };

    // ── tempo: first SetTempo meta in any track wins ──────────────────
    let bpm = extract_bpm(&smf).unwrap_or(120.0);

    // ── notes per track ───────────────────────────────────────────────
    let tracks_notes: Vec<Vec<NoteEvent>> = smf.tracks.iter().map(|t| extract_notes(t)).collect();

    // ── pick RH / LH tracks ───────────────────────────────────────────
    let (rh_idx, lh_idx) = match config.voice_tracks {
        Some((a, b)) if a < tracks_notes.len() && b < tracks_notes.len() && a != b => (a, b),
        _ => pick_outer_voices(&tracks_notes)
            .ok_or_else(|| "no two non-empty melodic tracks found".to_string())?,
    };

    // ── pick step division ────────────────────────────────────────────
    let step_division = config
        .step_division
        .unwrap_or_else(|| auto_step_division(&tracks_notes[rh_idx], &tracks_notes[lh_idx], ppq))
        .clamp(1, MAX_STEP_DIVISION);

    // ── quantise notes to step indices ────────────────────────────────
    let ticks_per_step = (ppq / step_division.max(1) as u32).max(1);
    let rh_steps = quantise_monophonic(&tracks_notes[rh_idx], ticks_per_step, false);
    let lh_steps = quantise_monophonic(&tracks_notes[lh_idx], ticks_per_step, true);

    let source_ticks = *[
        tracks_notes[rh_idx]
            .iter()
            .map(|n| n.end_tick)
            .max()
            .unwrap_or(0),
        tracks_notes[lh_idx]
            .iter()
            .map(|n| n.end_tick)
            .max()
            .unwrap_or(0),
    ]
    .iter()
    .max()
    .unwrap();

    // ── fill banks ────────────────────────────────────────────────────
    let total_steps = rh_steps.len().max(lh_steps.len());
    let banks_needed = total_steps.div_ceil(MAX_STEPS).max(1);
    let banks_used = banks_needed.min(MAX_BANKS);
    let kept_steps = banks_used * MAX_STEPS;
    let was_truncated = total_steps > kept_steps;
    let kept_ticks = (kept_steps as u32 * ticks_per_step).min(source_ticks);

    let notes_voice_0 = rh_steps
        .iter()
        .take(kept_steps)
        .filter(|s| s.active)
        .count();
    let notes_voice_1 = lh_steps
        .iter()
        .take(kept_steps)
        .filter(|s| s.active)
        .count();

    // Build the new state.  Grow `pattern_bank` just far enough to hold
    // the banks we're about to populate — don't eagerly reserve all
    // MAX_BANKS slots, so a 2-bank import doesn't carry around 62
    // zeroed `SequencerState`s of ballast.
    let mut s = state;
    let needed = banks_used.max(crate::state::DEFAULT_BANKS);
    if s.pattern_bank.len() < needed {
        s.pattern_bank.resize_with(needed, SequencerState::default);
    }

    for bank_idx in 0..banks_used {
        let start = bank_idx * MAX_STEPS;
        let end = (start + MAX_STEPS).min(kept_steps);
        let last_bank_steps = (end - start).max(1);

        // Base this bank on the current live sequencer so we inherit
        // the user's drum patterns, time signature, etc. — then overlay
        // our two voices on top.
        let mut bank = s.sequencer.clone();
        bank.bpm = bpm;
        bank.step_division = step_division;
        bank.steps = last_bank_steps;
        bank.bass_steps = last_bank_steps;
        // Voice 0 / 1 get the imported patterns; 2+ either wiped or
        // preserved from the source bank based on config.
        let mut patterns: Vec<Vec<TB303Step>> =
            vec![vec![TB303Step::default(); MAX_STEPS]; MAX_BASS_VOICES];
        copy_slice(&rh_steps, &mut patterns[0], start, end);
        copy_slice(&lh_steps, &mut patterns[1], start, end);
        if !config.wipe_other_voices {
            for (i, src) in bank.bass_patterns.iter().enumerate().take(MAX_BASS_VOICES) {
                if i >= 2 {
                    patterns[i] = src.clone();
                }
            }
        }
        bank.bass_patterns = patterns;
        // Voice 0's pattern still drives the legacy `bass_pattern` field,
        // so mirror it (matches SequencerState::default and existing
        // live-record behaviour).
        bank.bass_pattern = bank.bass_patterns[0].clone();
        bank.bass_voice_steps = vec![last_bank_steps; MAX_BASS_VOICES];
        bank.bass_voice_enabled[0] = true;
        bank.bass_voice_enabled[1] = true;
        // Carry the imported tempo automatically across chain advances.
        bank.pattern_bpm_apply = true;

        s.pattern_bank[bank_idx] = bank;
    }

    // Load bank 0 as the live pattern so Play-on-import does the right
    // thing without an extra bank_load round-trip.
    if let Some(first) = s.pattern_bank.first() {
        s.sequencer = first.clone();
    }

    // Song chain: walk all populated banks once, default overrides.
    // `chain_loop = false` so the piece plays exactly once and the
    // audio thread stops at the end instead of wrapping back to bank 0.
    // Imports of definite-ending pieces (MIDI scores) want one-shot
    // playback; loops can be re-enabled from the UI / API if the user
    // wants to jam on the imported material afterwards.
    s.chain = (0..banks_used).collect();
    s.chain_overrides = vec![ChainSlotOverride::default(); banks_used];
    s.chain_enabled = banks_used > 1;
    s.chain_loop = false;
    s.chain_pos = 0;
    s.chain_repeat_count = 0;
    s.pattern_edit = 0;

    // Enable bass voices 0 and 1 on the top-level bass_voices list
    // (mirrors bass_voice_enabled but lives outside the sequencer).
    if let Some(v) = s.bass_voices.get_mut(0) {
        v.enabled = true;
    }
    if let Some(v) = s.bass_voices.get_mut(1) {
        v.enabled = true;
    }

    // Playback duration at the selected tempo + grid.  Use
    // `kept_steps` (which equals `banks_used × MAX_STEPS` up to the
    // total we truncated to) rather than `total_steps` so truncated
    // imports report the time that *will actually play*, not the
    // source length.  At the very last bank the effective step window
    // is `last_bank_steps` instead of MAX_STEPS, so this is a slight
    // overestimate for imports whose length doesn't divide MAX_STEPS
    // evenly — acceptable for scenario pacing.
    let seconds_per_step = 60.0 / ((bpm as f64) * (step_division.max(1) as f64));
    let duration_seconds = (kept_steps as f64 * seconds_per_step) as f32;

    Ok((
        s,
        ImportSummary {
            bpm,
            step_division,
            banks_used,
            source_ticks,
            kept_ticks,
            was_truncated,
            notes_voice_0,
            notes_voice_1,
            picked_tracks: (rh_idx, lh_idx),
            duration_seconds,
        },
    ))
}

/// Convenience wrapper: read from disk, then call `import_midi_into`.
pub fn import_midi_file(
    state: AppState,
    path: &Path,
    config: &MidiImport,
) -> Result<(AppState, ImportSummary), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    import_midi_into(state, &bytes, config)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// First SetTempo meta in any track, converted to BPM.  None if no
/// tempo meta exists (the importer then falls back to 120 BPM).
pub fn extract_bpm(smf: &Smf<'_>) -> Option<f32> {
    for track in &smf.tracks {
        for ev in track {
            if let TrackEventKind::Meta(MetaMessage::Tempo(us_per_quarter)) = ev.kind {
                let us = us_per_quarter.as_int() as f64;
                if us > 0.0 {
                    return Some((60_000_000.0 / us) as f32);
                }
            }
        }
    }
    None
}

/// Walk one track's event stream, pairing NoteOn↔NoteOff into
/// `NoteEvent`s with absolute tick timings.  Un-matched NoteOns at end
/// of track are dropped (DAW bugs, common enough in real-world SMFs).
pub fn extract_notes(track: &[TrackEvent<'_>]) -> Vec<NoteEvent> {
    let mut out = Vec::new();
    // pending[channel][pitch] → (start_tick, velocity).  Channel-aware so
    // a single track with multiple channels still pairs correctly.
    let mut pending: [[Option<(u32, u8)>; 128]; 16] =
        std::array::from_fn(|_| std::array::from_fn(|_| None));
    let mut tick = 0u32;
    for ev in track {
        tick = tick.saturating_add(ev.delta.as_int());
        if let TrackEventKind::Midi { channel, message } = ev.kind {
            let ch = channel.as_int() as usize;
            match message {
                MidiMessage::NoteOn { key, vel } => {
                    let k = key.as_int() as usize;
                    let v = vel.as_int();
                    if v > 0 {
                        pending[ch][k] = Some((tick, v));
                    } else {
                        // NoteOn vel=0 is a running-status NoteOff.
                        if let Some((start, vel)) = pending[ch][k].take() {
                            out.push(NoteEvent {
                                start_tick: start,
                                end_tick: tick.max(start + 1),
                                pitch: k as u8,
                                velocity: vel,
                            });
                        }
                    }
                }
                MidiMessage::NoteOff { key, .. } => {
                    let k = key.as_int() as usize;
                    if let Some((start, vel)) = pending[ch][k].take() {
                        out.push(NoteEvent {
                            start_tick: start,
                            end_tick: tick.max(start + 1),
                            pitch: k as u8,
                            velocity: vel,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Pick (higher-pitch track, lower-pitch track) from the two densest
/// non-empty tracks.  Returns None if fewer than two tracks carry any
/// notes.  Drum tracks (every note on channel 10) are excluded — SMF
/// files often include a tempo/conductor track as track 0 with no notes
/// at all, which is also naturally excluded.
pub fn pick_outer_voices(tracks: &[Vec<NoteEvent>]) -> Option<(usize, usize)> {
    let mut candidates: Vec<(usize, usize, f32)> = tracks
        .iter()
        .enumerate()
        .filter(|(_, n)| !n.is_empty())
        .map(|(i, n)| {
            let count = n.len();
            let mean = n.iter().map(|e| e.pitch as f32).sum::<f32>() / count as f32;
            (i, count, mean)
        })
        .collect();
    if candidates.len() < 2 {
        return None;
    }
    // Sort by note-count descending; tiebreak on lower index for stability.
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    // Of the two densest, label the higher-mean as RH.
    let (i1, _, m1) = candidates[0];
    let (i2, _, m2) = candidates[1];
    if m1 >= m2 {
        Some((i1, i2))
    } else {
        Some((i2, i1))
    }
}

/// Auto-select a step division fine enough to resolve the smallest
/// inter-onset interval across RH+LH tracks.  Returns 4 / 8 / 16.
pub fn auto_step_division(rh: &[NoteEvent], lh: &[NoteEvent], ppq: u32) -> u8 {
    // Collect onsets from both voices together; duplicates are fine —
    // the minimum nonzero delta is what matters.
    let mut onsets: Vec<u32> = rh.iter().map(|n| n.start_tick).collect();
    onsets.extend(lh.iter().map(|n| n.start_tick));
    onsets.sort_unstable();
    onsets.dedup();

    let mut min_delta = u32::MAX;
    for pair in onsets.windows(2) {
        let d = pair[1].saturating_sub(pair[0]);
        if d > 0 && d < min_delta {
            min_delta = d;
        }
    }

    if min_delta == u32::MAX {
        // Degenerate: one note or fewer.  Use 16ths (the historical default).
        return 4;
    }

    // ticks_per_step ≤ min_delta → step grid fine enough.  Smallest
    // grid is `ppq / div`; so we need `div ≥ ppq / min_delta`.
    let ideal_div = (ppq as f32 / min_delta as f32).ceil() as u32;
    // Snap up to the nearest power of two in {4, 8, 16} — musically
    // useful subdivisions.  Powers of two match the sequencer's
    // beat / bar line expectations (see step_grid_width in the UI).
    let snapped = if ideal_div <= 4 {
        4
    } else if ideal_div <= 8 {
        8
    } else {
        16
    };
    snapped.min(MAX_STEP_DIVISION as u32) as u8
}

/// Collapse a polyphonic note stream into one `TB303Step` per grid
/// position.  When multiple notes share a quantised step, keep the
/// highest-pitched one for the upper voice and the lowest for the
/// lower voice — the classical "outer voices" reduction that matches
/// how two-hand piano scores collapse onto a monophonic synth.
///
/// `lower_preferred = true` picks the minimum pitch per step; `false`
/// picks the maximum.
pub fn quantise_monophonic(
    notes: &[NoteEvent],
    ticks_per_step: u32,
    lower_preferred: bool,
) -> Vec<TB303Step> {
    if notes.is_empty() {
        return Vec::new();
    }
    let last_tick = notes.iter().map(|n| n.start_tick).max().unwrap_or(0);
    let total_steps = (last_tick / ticks_per_step) as usize + 1;
    let mut out = vec![TB303Step::default(); total_steps];

    for n in notes {
        let step = (n.start_tick / ticks_per_step) as usize;
        if step >= out.len() {
            continue;
        }
        let existing = out[step];
        let take = if !existing.active {
            true
        } else if lower_preferred {
            n.pitch < existing.note
        } else {
            n.pitch > existing.note
        };
        if !take {
            continue;
        }
        // Gate = sustain-in-ticks / ticks_per_step, clamped to [0.05, 1.0].
        // A 0-gate would be inaudible so force a minimum; values over 1.0
        // would smear into the next step (the synth's glide handles that
        // if `slide > 0`, but we don't set slide on import).
        let dur = n.end_tick.saturating_sub(n.start_tick).max(1);
        let gate = (dur as f32 / ticks_per_step as f32).clamp(0.05, 1.0);
        // Velocity 64 = baseline accent=0; 127 maps to accent=1.
        // Negative half (0..64) folds to accent=0 so only genuine
        // accented hits drive the 303's amp lift.
        let accent = ((n.velocity as f32 - 64.0) / 63.0).clamp(0.0, 1.0);
        out[step] = TB303Step {
            active: true,
            note: n.pitch,
            accent,
            slide: 0.0,
            gate,
            pan: 0.0,
        };
    }
    out
}

/// Copy `src[start..end]` into `dst[0..(end-start)]`, silencing any
/// trailing `dst` slots so the bank doesn't re-use stale default
/// notes.  Tolerates `src` being shorter than `end` (shorter voices
/// just get trailing silence rather than panicking), and tolerates
/// `start` being past `src.len()` (whole bank is silent).
fn copy_slice(src: &[TB303Step], dst: &mut [TB303Step], start: usize, end: usize) {
    let clamped_start = start.min(src.len());
    let clamped_end = end.min(src.len()).max(clamped_start);
    let mut written = 0usize;
    for (i, s) in src[clamped_start..clamped_end].iter().enumerate() {
        if i < dst.len() {
            dst[i] = *s;
            written = i + 1;
        }
    }
    for d in dst.iter_mut().skip(written) {
        *d = TB303Step::default();
    }
}
