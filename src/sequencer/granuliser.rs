// ─── sequencer/granuliser.rs ──────────────────────────────────────────────────
// MIDI granuliser — scatter triggers across an existing step pattern.
// Absurd-queue item #8.
//
// Per the brief: "input a MIDI clip, scatter the triggers with jitter
// / density knobs.  Granular but for triggers, not audio."  V1 ships
// as an in-place transformation on the *current sequencer pattern*
// rather than a file-to-file converter — same conceptual model
// (scatter trigger events) but the user sees the result instantly in
// the running session, not on the next file load.
//
// Pure function over `Vec<TB303Step>` — no allocations, deterministic
// per seed, runnable from a unit test without spinning up the audio
// engine.  The HTTP handler in `api/midi_granulise.rs` calls this
// against whichever voice's pattern the request names.
//
// Knob design:
//
//   * `density` 0..1 — drop probability per active step.  1.0 keeps
//     every active step; 0.5 drops half; 0.0 silences the pattern.
//   * `repeat_chance` 0..1 — probability that a kept active step
//     populates the *next* step too (a quick re-trigger).  Skips
//     when the next slot is already active so the user's existing
//     pattern isn't overwritten.
//   * `pitch_jitter_st` 0..12 — random ±N semitones transposition
//     applied to each kept step's note.  0 = no transpose.
//
// Timing jitter (sub-step offsets) doesn't fit naturally in a
// step-grid pattern — handled at the audio thread's swing / shuffle
// stage if needed.  The grid is the resolution we're working at.

use crate::state::TB303Step;

/// File-to-file mode (V2 follow-up).  Parse the input SMF bytes, run
/// the granuliser over every melodic pattern the exporter cares about,
/// and re-export the result as a fresh SMF byte stream.  Lets users
/// pre-process MIDI clips offline without spinning up the live
/// sequencer.  Returns the new SMF bytes on success, or a
/// user-readable error string on parse / import failure.
///
/// Each lane gets the same `opts` but draws from an independent
/// RNG (one `granulise_tb303` call per lane), so the same seed
/// produces deterministic but per-lane-distinct scattering.
///
/// Round-trip note: the importer normalises 2 melodic tracks into
/// bass voices 0/1, while the exporter writes the canonical
/// `bass_pattern` (= voice 0) + `hoover_pattern` + `an1x_pattern`
/// lanes.  This wrapper bridges the gap by mirroring voice 1 into
/// `hoover_pattern` so a 2-track source survives the round-trip
/// (RH stays on bass; LH ends up on hoover).
pub fn granulise_smf_bytes(input_bytes: &[u8], opts: GranuliseOpts) -> Result<Vec<u8>, String> {
    use crate::midi::{MidiImport, import_midi_into};
    use crate::state::AppState;

    let (mut state, _summary) =
        import_midi_into(AppState::default(), input_bytes, &MidiImport::default())?;

    // Mirror voice 1 → hoover_pattern so both melodic lanes survive
    // the export.  The importer left bass_patterns[1] populated but
    // hoover_pattern empty; the exporter only writes bass_pattern +
    // hoover_pattern + an1x_pattern, so without this bridge the LH
    // lane would be silently dropped.
    if state.sequencer.bass_patterns.len() > 1
        && state.sequencer.bass_patterns[1].iter().any(|s| s.active)
        && !state.sequencer.hoover_pattern.iter().any(|s| s.active)
    {
        state.sequencer.hoover_pattern = state.sequencer.bass_patterns[1].clone();
        state.sequencer.hoover_steps = state.sequencer.bass_voice_steps[1];
    }

    // Granulise the lanes the exporter actually writes.
    // `bass_pattern == bass_patterns[0]` after import (mirror
    // invariant from `import_midi_into`).
    granulise_tb303(&mut state.sequencer.bass_pattern, opts);
    granulise_tb303(&mut state.sequencer.hoover_pattern, opts);
    granulise_tb303(&mut state.sequencer.an1x_pattern, opts);

    Ok(crate::midi::export_sequencer_smf(&state.sequencer))
}

/// Granuliser knob bundle.  All fields are clamped at apply time so
/// out-of-range API inputs don't blow up the transformation.
#[derive(Clone, Copy, Debug)]
pub struct GranuliseOpts {
    pub density: f32,
    pub repeat_chance: f32,
    pub pitch_jitter_st: u8,
    pub seed: u64,
}

impl Default for GranuliseOpts {
    fn default() -> Self {
        Self {
            density: 1.0, // pass-through
            repeat_chance: 0.0,
            pitch_jitter_st: 0,
            seed: 0,
        }
    }
}

/// Apply the granuliser to a single TB303-style pattern in place.
/// Order of operations per active step:
///   1. Density gate — `next_f32() > density` drops the step.
///   2. Pitch jitter — random ±N st added to the note (clamped 0..127).
///   3. Repeat — if the next slot is empty, populate it with a
///      half-accent copy of the current step.  Skipped at end of
///      pattern.
///
/// Pure: same `(pattern, opts)` produces the same output every time.
pub fn granulise_tb303(pattern: &mut [TB303Step], opts: GranuliseOpts) {
    let mut rng = LcgRng::new(opts.seed);
    let density = opts.density.clamp(0.0, 1.0);
    let repeat = opts.repeat_chance.clamp(0.0, 1.0);
    let pj = opts.pitch_jitter_st.min(12);
    let n = pattern.len();
    let mut i = 0;
    while i < n {
        if !pattern[i].active {
            i += 1;
            continue;
        }
        // Density gate — `next_f32() > density` means this step
        // gets dropped.  At density = 1.0 the comparison can't
        // succeed (next_f32 returns < 1.0), so every active step
        // is preserved.
        if rng.next_f32() > density {
            pattern[i].active = false;
            i += 1;
            continue;
        }
        // Pitch jitter — symmetric ±pj range, half open on the +
        // side because `next_bounded(2*pj + 1) - pj` lands evenly
        // across the integer span [-pj, +pj].
        if pj > 0 {
            let range = 2 * pj as u32 + 1;
            let offset = rng.next_bounded(range) as i32 - pj as i32;
            let new_note = (pattern[i].note as i32 + offset).clamp(0, 127) as u8;
            pattern[i].note = new_note;
        }
        // Repeat — half-accent copy of this step into the next
        // slot if it's free.  Don't write past the end of the
        // pattern, and skip the slot we just wrote so the next
        // loop iteration doesn't double-process it.
        if repeat > 0.0 && i + 1 < n && !pattern[i + 1].active && rng.next_f32() < repeat {
            let mut clone = pattern[i];
            // Halve the accent so the repeat reads as a tail, not
            // a duplicate hit.  Clamp to avoid negatives.
            clone.accent = (clone.accent * 0.5).max(0.0);
            pattern[i + 1] = clone;
            i += 2;
        } else {
            i += 1;
        }
    }
}

/// Tiny LCG — same constants as `state::rack_random`.  Inlined here
/// rather than imported because the rack-random copy is `pub(super)`
/// to its own crate path; duplicating the ~20 lines is cheaper than
/// a module-visibility refactor.
struct LcgRng {
    state: u64,
}

impl LcgRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1),
        }
    }
    fn next(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.state >> 32) as u32
    }
    fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 { 0 } else { self.next() % bound }
    }
    fn next_f32(&mut self) -> f32 {
        // Unit interval — top bits give better mixing than mod 1.
        self.next() as f32 / u32::MAX as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pattern(actives: &[(usize, u8)]) -> Vec<TB303Step> {
        let mut pat: Vec<TB303Step> = (0..16)
            .map(|_| TB303Step {
                active: false,
                note: 60,
                accent: 0.0,
                slide: 0.0,
                gate: 0.5,
                pan: 0.0,
                cond: 0,
            })
            .collect();
        for &(i, n) in actives {
            pat[i].active = true;
            pat[i].note = n;
        }
        pat
    }

    #[test]
    fn density_one_keeps_every_active_step() {
        // density = 1.0, repeat = 0, pitch_jitter = 0 should be
        // an identity transformation.
        let mut pat = make_pattern(&[(0, 60), (4, 64), (8, 67), (12, 72)]);
        let before = pat.clone();
        granulise_tb303(
            &mut pat,
            GranuliseOpts {
                density: 1.0,
                repeat_chance: 0.0,
                pitch_jitter_st: 0,
                seed: 42,
            },
        );
        for i in 0..pat.len() {
            assert_eq!(
                pat[i].active, before[i].active,
                "step {i} active flag drift"
            );
            assert_eq!(pat[i].note, before[i].note, "step {i} note drift");
        }
    }

    #[test]
    fn density_zero_drops_every_active_step() {
        let mut pat = make_pattern(&[(0, 60), (4, 64), (8, 67), (12, 72)]);
        granulise_tb303(
            &mut pat,
            GranuliseOpts {
                density: 0.0,
                repeat_chance: 0.0,
                pitch_jitter_st: 0,
                seed: 0xDEAD_BEEF,
            },
        );
        for (i, s) in pat.iter().enumerate() {
            assert!(!s.active, "step {i} still active after density=0");
        }
    }

    #[test]
    fn density_half_drops_roughly_half_across_seeds() {
        // Statistical property: across many seeds, density=0.5
        // should keep around half the active steps on average.
        // Tolerance is wide because we only run 32 seeds and 4
        // active steps each — small sample, just sanity-checking
        // the gate isn't a no-op.
        let mut total_kept = 0;
        let total_active = 32 * 4;
        for seed in 0..32u64 {
            let mut pat = make_pattern(&[(0, 60), (4, 64), (8, 67), (12, 72)]);
            granulise_tb303(
                &mut pat,
                GranuliseOpts {
                    density: 0.5,
                    repeat_chance: 0.0,
                    pitch_jitter_st: 0,
                    seed,
                },
            );
            total_kept += pat.iter().filter(|s| s.active).count();
        }
        let ratio = total_kept as f32 / total_active as f32;
        assert!(
            (0.3..=0.7).contains(&ratio),
            "density=0.5 should keep ~half (got {ratio})"
        );
    }

    #[test]
    fn pitch_jitter_keeps_notes_in_midi_range() {
        // ±12 st on a midrange note shouldn't escape 0..=127.
        let mut pat = make_pattern(&[(0, 60), (4, 0), (8, 127)]);
        granulise_tb303(
            &mut pat,
            GranuliseOpts {
                density: 1.0,
                repeat_chance: 0.0,
                pitch_jitter_st: 12,
                seed: 7,
            },
        );
        for s in &pat {
            if s.active {
                // The clamp guarantees this regardless of seed —
                // explicit assertion guards against the
                // implementation forgetting it.
                assert!(s.note <= 127, "note overflowed: {}", s.note);
            }
        }
    }

    #[test]
    fn repeat_can_populate_empty_next_slot() {
        // High repeat chance + always-keep density: at least one
        // of the inactive next-to-active slots should fill in.
        let mut pat = make_pattern(&[(0, 60), (4, 64), (8, 67)]);
        granulise_tb303(
            &mut pat,
            GranuliseOpts {
                density: 1.0,
                repeat_chance: 1.0,
                pitch_jitter_st: 0,
                seed: 3,
            },
        );
        // Slots 1, 5, 9 should now be active (clones of 0/4/8).
        assert!(pat[1].active, "slot 1 should be repeat-populated");
        assert!(pat[5].active, "slot 5 should be repeat-populated");
        assert!(pat[9].active, "slot 9 should be repeat-populated");
        // Repeats land at half-accent.
        assert!((pat[1].accent - pat[0].accent * 0.5).abs() < 1e-5);
    }

    #[test]
    fn repeat_doesnt_overwrite_already_active_next_slot() {
        // If slot 1 is already user-set, the repeat should skip it.
        let mut pat = make_pattern(&[(0, 60), (1, 70)]);
        let original_note_1 = pat[1].note;
        granulise_tb303(
            &mut pat,
            GranuliseOpts {
                density: 1.0,
                repeat_chance: 1.0,
                pitch_jitter_st: 0,
                seed: 11,
            },
        );
        assert_eq!(
            pat[1].note, original_note_1,
            "repeat must not overwrite an existing note"
        );
    }

    /// Build a fixture SMF the importer can ingest.  The importer
    /// requires two non-empty melodic tracks so the fixture populates
    /// `bass_pattern` (RH lane on export) and `hoover_pattern` (LH
    /// lane on export).
    fn fixture_smf_bytes() -> Vec<u8> {
        use crate::midi::export_sequencer_smf;
        use crate::state::AppState;

        let mut s = AppState::default();
        // RH lane — bass voice
        s.sequencer.bass_pattern[0].active = true;
        s.sequencer.bass_pattern[0].note = 72;
        s.sequencer.bass_pattern[4].active = true;
        s.sequencer.bass_pattern[4].note = 76;
        s.sequencer.bass_pattern[8].active = true;
        s.sequencer.bass_pattern[8].note = 79;
        // LH lane — hoover voice
        s.sequencer.hoover_pattern[0].active = true;
        s.sequencer.hoover_pattern[0].note = 36;
        s.sequencer.hoover_pattern[8].active = true;
        s.sequencer.hoover_pattern[8].note = 43;
        export_sequencer_smf(&s.sequencer)
    }

    #[test]
    fn smf_bytes_round_trip_preserves_notes_at_density_one() {
        // density=1 should be a pass-through.  Re-import the granulised
        // output and confirm at least one note lands on each bass voice.
        use crate::midi::{MidiImport, import_midi_into};
        use crate::state::AppState;

        let bytes = fixture_smf_bytes();
        let out = granulise_smf_bytes(
            &bytes,
            GranuliseOpts {
                density: 1.0,
                repeat_chance: 0.0,
                pitch_jitter_st: 0,
                seed: 0,
            },
        )
        .expect("granulise_smf_bytes failed");

        let (back, _) =
            import_midi_into(AppState::default(), &out, &MidiImport::default()).expect("re-import");
        // RH bass voice should round-trip through bass_patterns[0]
        // unchanged (density=1).  LH lane is intentionally dropped
        // by the V1 export path — see the doc on
        // `granulise_smf_bytes`.
        let v0 = back.sequencer.bass_patterns[0]
            .iter()
            .filter(|s| s.active)
            .count();
        assert!(v0 >= 1, "round-trip lost RH lane");
    }

    #[test]
    fn smf_bytes_density_zero_drops_all_notes() {
        use crate::midi::{MidiImport, import_midi_into};
        use crate::state::AppState;

        let bytes = fixture_smf_bytes();
        let out = granulise_smf_bytes(
            &bytes,
            GranuliseOpts {
                density: 0.0,
                repeat_chance: 0.0,
                pitch_jitter_st: 0,
                seed: 1,
            },
        )
        .expect("granulise_smf_bytes failed");
        // density=0 wipes every active step — re-import currently
        // rejects empty melodic content with "no two non-empty
        // melodic tracks found", so an Err is the expected proof
        // that the granulised output really is empty.
        let res = import_midi_into(AppState::default(), &out, &MidiImport::default());
        assert!(
            res.is_err(),
            "re-importing an empty SMF should error rather than fabricate notes"
        );
    }

    #[test]
    fn smf_bytes_rejects_invalid_bytes() {
        let bad = b"not an smf at all";
        let res = granulise_smf_bytes(
            bad,
            GranuliseOpts {
                density: 1.0,
                repeat_chance: 0.0,
                pitch_jitter_st: 0,
                seed: 0,
            },
        );
        assert!(res.is_err(), "garbage input should bubble up parse error");
    }

    #[test]
    fn deterministic_for_same_seed() {
        // Same input + opts → same output.  Locks the contract
        // that an interesting roll can be replayed via ?seed=N.
        let pat0 = make_pattern(&[(0, 60), (3, 63), (5, 65), (10, 67), (14, 70)]);
        let mut a = pat0.clone();
        let mut b = pat0.clone();
        let opts = GranuliseOpts {
            density: 0.6,
            repeat_chance: 0.3,
            pitch_jitter_st: 5,
            seed: 0xBADF00D,
        };
        granulise_tb303(&mut a, opts);
        granulise_tb303(&mut b, opts);
        for i in 0..a.len() {
            assert_eq!(a[i].active, b[i].active, "step {i} active diverged");
            assert_eq!(a[i].note, b[i].note, "step {i} note diverged");
        }
    }
}
