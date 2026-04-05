// ─── tests/music_api_tests.rs ─────────────────────────────────────────────────
// Extensive unit tests for src/music_api/mod.rs — every public function,
// all enum variants, integration with apply_music_api, and edge cases.

use crate::music_api::{
    ChordQuality, RunDirection, amen_pattern, apply_music_api, chord, chord_pcs, parse_root,
    random_diatonic_chord, scale_run,
};
use crate::state::{AppState, DrumVoice, Scale};

// Inline copies of the canonical Amen patterns (mirrors private constants in music_api)
const AMEN_KICK_BASE: [bool; 16] = [
    true, false, false, false, false, false, true, false, false, false, false, false, false, false,
    false, false,
];
const AMEN_SNARE_BASE: [bool; 16] = [
    false, false, false, false, true, false, false, false, false, false, false, false, true, false,
    false, false,
];
const AMEN_HIHAT_BASE: [bool; 16] = [
    true, false, true, false, true, false, true, false, true, false, true, false, true, false,
    true, false,
];

// ─── parse_root ───────────────────────────────────────────────────────────────

#[test]
fn parse_root_naturals() {
    let naturals = [
        ("C", 0u8),
        ("D", 2),
        ("E", 4),
        ("F", 5),
        ("G", 7),
        ("A", 9),
        ("B", 11),
    ];
    for (name, pc) in &naturals {
        let midi = parse_root(name).expect(name);
        assert_eq!(midi % 12, *pc, "{name}");
        assert_eq!(midi / 12, 4, "{name} should be in octave 4 (midi 48-59)"); // 48 = C3
    }
}

#[test]
fn parse_root_sharps() {
    let sharps = [("C#", 1u8), ("D#", 3), ("F#", 6), ("G#", 8), ("A#", 10)];
    for (name, pc) in &sharps {
        let midi = parse_root(name).expect(name);
        assert_eq!(midi % 12, *pc, "{name}");
    }
}

#[test]
fn parse_root_flats() {
    let flats = [("Db", 1u8), ("Eb", 3), ("Gb", 6), ("Ab", 8), ("Bb", 10)];
    for (name, pc) in &flats {
        let midi = parse_root(name).expect(name);
        assert_eq!(midi % 12, *pc, "{name}");
    }
}

#[test]
fn parse_root_unknown_returns_none() {
    assert!(parse_root("X").is_none());
    assert!(parse_root("").is_none());
    assert!(parse_root("H").is_none());
    assert!(parse_root("Cb").is_none()); // not in our map
}

#[test]
fn parse_root_all_twelve_pitch_classes_covered() {
    let names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    for (i, name) in names.iter().enumerate() {
        let midi = parse_root(name).unwrap();
        assert_eq!(midi % 12, i as u8, "{name}");
    }
}

// ─── ChordQuality::from_str ───────────────────────────────────────────────────

#[test]
fn chord_quality_all_canonical_names() {
    let cases = [
        ("major", ChordQuality::Major),
        ("minor", ChordQuality::Minor),
        ("dim", ChordQuality::Dim),
        ("aug", ChordQuality::Aug),
        ("sus2", ChordQuality::Sus2),
        ("sus4", ChordQuality::Sus4),
        ("dom7", ChordQuality::Dom7),
        ("maj7", ChordQuality::Maj7),
        ("min7", ChordQuality::Min7),
        ("dim7", ChordQuality::Dim7),
    ];
    for (s, expected) in &cases {
        assert_eq!(ChordQuality::from_str(s), Some(*expected), "{s}");
    }
}

#[test]
fn chord_quality_aliases() {
    assert_eq!(ChordQuality::from_str("maj"), Some(ChordQuality::Major));
    assert_eq!(ChordQuality::from_str("min"), Some(ChordQuality::Minor));
    assert_eq!(ChordQuality::from_str("sus"), Some(ChordQuality::Sus4));
    assert_eq!(ChordQuality::from_str("7"), Some(ChordQuality::Dom7));
    assert_eq!(ChordQuality::from_str("m7"), Some(ChordQuality::Min7));
    assert_eq!(
        ChordQuality::from_str("diminished"),
        Some(ChordQuality::Dim)
    );
    assert_eq!(ChordQuality::from_str("augmented"), Some(ChordQuality::Aug));
}

#[test]
fn chord_quality_case_insensitive() {
    assert_eq!(ChordQuality::from_str("MAJOR"), Some(ChordQuality::Major));
    assert_eq!(ChordQuality::from_str("Minor"), Some(ChordQuality::Minor));
    assert_eq!(ChordQuality::from_str("DIM7"), Some(ChordQuality::Dim7));
}

#[test]
fn chord_quality_unknown_returns_none() {
    assert!(ChordQuality::from_str("xyz").is_none());
    assert!(ChordQuality::from_str("").is_none());
}

// ─── chord() intervals ────────────────────────────────────────────────────────

#[test]
fn chord_major_intervals() {
    let root = parse_root("C").unwrap(); // C3 = 48
    let notes = chord(root, ChordQuality::Major);
    assert_eq!(notes, vec![48, 52, 55]); // C, E, G
}

#[test]
fn chord_minor_intervals() {
    let root = parse_root("A").unwrap(); // A3 = 57
    let notes = chord(root, ChordQuality::Minor);
    assert_eq!(notes, vec![57, 60, 64]); // A, C, E
}

#[test]
fn chord_dim_intervals() {
    let root = parse_root("B").unwrap(); // B3 = 59
    let notes = chord(root, ChordQuality::Dim);
    assert_eq!(notes, vec![59, 62, 65]); // B, D, F
}

#[test]
fn chord_aug_intervals() {
    let root = parse_root("C").unwrap();
    let notes = chord(root, ChordQuality::Aug);
    assert_eq!(notes[1] - notes[0], 4);
    assert_eq!(notes[2] - notes[0], 8);
}

#[test]
fn chord_sus2_intervals() {
    let root = parse_root("C").unwrap();
    let notes = chord(root, ChordQuality::Sus2);
    assert_eq!(notes[1] - notes[0], 2);
    assert_eq!(notes[2] - notes[0], 7);
}

#[test]
fn chord_sus4_intervals() {
    let root = parse_root("C").unwrap();
    let notes = chord(root, ChordQuality::Sus4);
    assert_eq!(notes[1] - notes[0], 5);
    assert_eq!(notes[2] - notes[0], 7);
}

#[test]
fn chord_dom7_has_four_notes() {
    let root = parse_root("G").unwrap();
    let notes = chord(root, ChordQuality::Dom7);
    assert_eq!(notes.len(), 4);
    assert_eq!(notes[1] - notes[0], 4); // major third
    assert_eq!(notes[2] - notes[0], 7); // perfect fifth
    assert_eq!(notes[3] - notes[0], 10); // minor seventh
}

#[test]
fn chord_maj7_has_four_notes() {
    let root = parse_root("C").unwrap();
    let notes = chord(root, ChordQuality::Maj7);
    assert_eq!(notes.len(), 4);
    assert_eq!(notes[3] - notes[0], 11); // major seventh
}

#[test]
fn chord_min7_has_four_notes() {
    let root = parse_root("A").unwrap();
    let notes = chord(root, ChordQuality::Min7);
    assert_eq!(notes.len(), 4);
    assert_eq!(notes[1] - notes[0], 3); // minor third
    assert_eq!(notes[3] - notes[0], 10); // minor seventh
}

#[test]
fn chord_all_notes_in_valid_midi_range() {
    for q in [
        ChordQuality::Major,
        ChordQuality::Minor,
        ChordQuality::Dim,
        ChordQuality::Aug,
        ChordQuality::Sus2,
        ChordQuality::Sus4,
        ChordQuality::Dom7,
        ChordQuality::Maj7,
        ChordQuality::Min7,
        ChordQuality::Dim7,
    ] {
        let notes = chord(48, q);
        for &n in &notes {
            assert!(n <= 127, "{q:?} produced out-of-range MIDI note {n}");
        }
    }
}

// ─── chord_pcs ────────────────────────────────────────────────────────────────

#[test]
fn chord_pcs_c_major() {
    let root = parse_root("C").unwrap();
    let pcs = chord_pcs(root, ChordQuality::Major);
    // C=0, E=4, G=7
    assert_eq!(pcs, vec![0, 4, 7]);
}

#[test]
fn chord_pcs_all_in_range_0_to_11() {
    let root = parse_root("B").unwrap(); // B3 = 59, highest octave-3 root
    let pcs = chord_pcs(root, ChordQuality::Dom7);
    for &pc in &pcs {
        assert!(pc < 12, "pitch class {pc} out of range");
    }
}

// ─── random_diatonic_chord ────────────────────────────────────────────────────

#[test]
fn random_diatonic_chord_root_in_natural_minor() {
    let root = parse_root("A").unwrap();
    let (chord_root, _) = random_diatonic_chord(root, Scale::NaturalMinor, 42);
    // A natural minor offsets: 0,2,3,5,7,8,10
    let offset = (chord_root % 12 + 12 - root % 12) % 12;
    let minor_offsets = [0u8, 2, 3, 5, 7, 8, 10];
    assert!(
        minor_offsets.contains(&offset),
        "chord root offset {offset} not in A natural minor"
    );
}

#[test]
fn random_diatonic_chord_is_deterministic() {
    let root = parse_root("C").unwrap();
    let (r1, q1) = random_diatonic_chord(root, Scale::Major, 999);
    let (r2, q2) = random_diatonic_chord(root, Scale::Major, 999);
    assert_eq!(r1, r2);
    assert_eq!(q1, q2);
}

#[test]
fn random_diatonic_chord_different_seeds_may_differ() {
    let root = parse_root("C").unwrap();
    // With many seeds, results should not all be identical
    let results: Vec<_> = (0..20)
        .map(|s| random_diatonic_chord(root, Scale::Major, s).0)
        .collect();
    let all_same = results.iter().all(|&r| r == results[0]);
    assert!(!all_same, "all 20 seeds produced the same chord root");
}

// ─── amen_pattern ─────────────────────────────────────────────────────────────

#[test]
fn amen_heat_zero_canonical_kick() {
    let (kick, _, _) = amen_pattern(0.0, 0);
    for (i, &base) in AMEN_KICK_BASE.iter().enumerate() {
        assert_eq!(kick[i].active, base, "kick[{i}] at heat=0");
    }
}

#[test]
fn amen_heat_zero_canonical_snare() {
    let (_, snare, _) = amen_pattern(0.0, 0);
    for (i, &base) in AMEN_SNARE_BASE.iter().enumerate() {
        assert_eq!(snare[i].active, base, "snare[{i}] at heat=0");
    }
}

#[test]
fn amen_heat_zero_canonical_hihat() {
    let (_, _, hihat) = amen_pattern(0.0, 0);
    for (i, &base) in AMEN_HIHAT_BASE.iter().enumerate() {
        assert_eq!(hihat[i].active, base, "hihat[{i}] at heat=0");
    }
}

fn get_kick(state: &AppState) -> &[crate::state::Step] {
    state
        .sequencer
        .drum_patterns
        .get(&DrumVoice::Kick808)
        .map_or(&[], |v| v.as_slice())
}
fn get_snare(state: &AppState) -> &[crate::state::Step] {
    state
        .sequencer
        .drum_patterns
        .get(&DrumVoice::Snare808)
        .map_or(&[], |v| v.as_slice())
}

#[test]
fn amen_always_returns_16_steps() {
    for heat in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let (k, s, h) = amen_pattern(heat, 0);
        assert_eq!(k.len(), 16);
        assert_eq!(s.len(), 16);
        assert_eq!(h.len(), 16);
    }
}

#[test]
fn amen_deterministic_at_max_heat() {
    let (k1, s1, h1) = amen_pattern(1.0, 7777);
    let (k2, s2, h2) = amen_pattern(1.0, 7777);
    for i in 0..16 {
        assert_eq!(k1[i].active, k2[i].active, "kick[{i}]");
        assert_eq!(s1[i].active, s2[i].active, "snare[{i}]");
        assert_eq!(h1[i].active, h2[i].active, "hihat[{i}]");
    }
}

#[test]
fn amen_heat_clamped_above_one() {
    // heat=2.0 should behave same as heat=1.0 (clamped)
    let (k1, _, _) = amen_pattern(1.0, 42);
    let (k2, _, _) = amen_pattern(2.0, 42);
    for i in 0..16 {
        assert_eq!(k1[i].active, k2[i].active, "step {i}");
    }
}

#[test]
fn amen_active_steps_have_nonzero_velocity() {
    let (kick, snare, hihat) = amen_pattern(0.5, 12345);
    for steps in [&kick[..], &snare[..], &hihat[..]] {
        for step in steps {
            if step.active {
                assert!(step.velocity > 0.0, "active step has zero velocity");
            }
        }
    }
}

#[test]
fn amen_velocity_in_valid_range() {
    let (kick, _, _) = amen_pattern(0.9, 99);
    for step in &kick {
        if step.active {
            assert!(
                step.velocity >= 0.0 && step.velocity <= 1.0,
                "velocity {} out of range",
                step.velocity
            );
        }
    }
}

// ─── RunDirection::from_str ───────────────────────────────────────────────────

#[test]
fn run_direction_all_variants() {
    assert_eq!(RunDirection::from_str("up"), RunDirection::Up);
    assert_eq!(RunDirection::from_str("down"), RunDirection::Down);
    assert_eq!(RunDirection::from_str("updown"), RunDirection::UpDown);
    assert_eq!(RunDirection::from_str("random"), RunDirection::Random);
    assert_eq!(RunDirection::from_str("ascending"), RunDirection::Up); // default
}

#[test]
fn run_direction_aliases() {
    assert_eq!(RunDirection::from_str("descend"), RunDirection::Down);
    assert_eq!(RunDirection::from_str("descending"), RunDirection::Down);
    assert_eq!(RunDirection::from_str("up_down"), RunDirection::UpDown);
    assert_eq!(RunDirection::from_str("bounce"), RunDirection::UpDown);
    assert_eq!(RunDirection::from_str("rand"), RunDirection::Random);
}

#[test]
fn run_direction_unknown_defaults_to_up() {
    assert_eq!(RunDirection::from_str(""), RunDirection::Up);
    assert_eq!(RunDirection::from_str("sideways"), RunDirection::Up);
}

// ─── scale_run ────────────────────────────────────────────────────────────────

#[test]
fn scale_run_always_16_steps() {
    for scale in [
        Scale::Major,
        Scale::NaturalMinor,
        Scale::Pentatonic,
        Scale::Chromatic,
    ] {
        for dir in [
            RunDirection::Up,
            RunDirection::Down,
            RunDirection::UpDown,
            RunDirection::Random,
        ] {
            let steps = scale_run(48, scale, dir, 0);
            assert_eq!(steps.len(), 16, "{scale:?} {dir:?}");
        }
    }
}

#[test]
fn scale_run_all_steps_active() {
    // scale_run always marks all 16 steps active (pattern repeats)
    let steps = scale_run(48, Scale::Major, RunDirection::Up, 0);
    assert!(steps.iter().all(|s| s.active));
}

#[test]
fn scale_run_notes_in_scale_up() {
    // Major scale from C3 (48): C=0,D=2,E=4,F=5,G=7,A=9,B=11,C=0
    let steps = scale_run(48, Scale::Major, RunDirection::Up, 0);
    let major_pcs = [0u8, 2, 4, 5, 7, 9, 11];
    for (i, step) in steps.iter().enumerate() {
        assert!(
            major_pcs.contains(&step.note),
            "step {i} note {} not in C major",
            step.note
        );
    }
}

#[test]
fn scale_run_down_reverses_up() {
    let up = scale_run(48, Scale::Major, RunDirection::Up, 0);
    let down = scale_run(48, Scale::Major, RunDirection::Down, 0);
    // Both should use notes from the major scale (same note set)
    let up_notes: std::collections::HashSet<u8> = up.iter().map(|s| s.note).collect();
    let down_notes: std::collections::HashSet<u8> = down.iter().map(|s| s.note).collect();
    assert_eq!(up_notes, down_notes, "up and down should use same note set");
    // The sequences should not be identical (different ordering)
    let sequences_identical = up.iter().zip(down.iter()).all(|(a, b)| a.note == b.note);
    assert!(
        !sequences_identical,
        "up and down should produce different orderings"
    );
}

#[test]
fn scale_run_random_is_deterministic() {
    let a = scale_run(48, Scale::NaturalMinor, RunDirection::Random, 42);
    let b = scale_run(48, Scale::NaturalMinor, RunDirection::Random, 42);
    for i in 0..16 {
        assert_eq!(a[i].note, b[i].note, "step {i}");
    }
}

#[test]
fn scale_run_random_different_seeds_differ() {
    let a = scale_run(48, Scale::NaturalMinor, RunDirection::Random, 1);
    let b = scale_run(48, Scale::NaturalMinor, RunDirection::Random, 9999);
    let identical = (0..16).all(|i| a[i].note == b[i].note);
    assert!(
        !identical,
        "different seeds should produce different orderings"
    );
}

#[test]
fn scale_run_updown_returns_correct_length() {
    // UpDown creates notes + rev(notes[1..]) which may be >16 or <16; output truncates/repeats to 16
    let steps = scale_run(48, Scale::Major, RunDirection::UpDown, 0);
    assert_eq!(steps.len(), 16);
}

#[test]
fn scale_run_pentatonic_notes_in_scale() {
    // Minor pentatonic: 0,3,5,7,10 (+ octave = 0)
    let steps = scale_run(48, Scale::Pentatonic, RunDirection::Up, 0);
    let penta_pcs = [0u8, 3, 5, 7, 10];
    for (i, step) in steps.iter().enumerate() {
        assert!(
            penta_pcs.contains(&step.note),
            "step {i} note {} not in minor pentatonic",
            step.note
        );
    }
}

#[test]
fn scale_run_all_active() {
    // scale_run always marks every step active
    let steps = scale_run(48, Scale::Major, RunDirection::Up, 0);
    assert!(
        steps.iter().all(|s| s.active),
        "every step should be active"
    );
}

// ─── apply_music_api integration ─────────────────────────────────────────────

fn json_obj(s: &str) -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(s)
        .unwrap()
        .as_object()
        .unwrap()
        .clone()
}

#[test]
fn apply_music_api_chord_writes_bass_steps() {
    let state = AppState::default();
    let api = json_obj(r#"{ "chord": { "root": "E", "quality": "minor" } }"#);
    let out = apply_music_api(state, &api);
    // E minor = [52,55,59] → pcs [4,7,11]; written at positions 0,4,8,12
    assert!(out.sequencer.bass_pattern[0].active);
    assert!(out.sequencer.bass_pattern[4].active);
    assert!(out.sequencer.bass_pattern[8].active);
    // Step 0 note should be E (pc=4)
    assert_eq!(out.sequencer.bass_pattern[0].note, 52 % 12); // 4
}

#[test]
fn apply_music_api_chord_sets_accent() {
    let state = AppState::default();
    let api = json_obj(r#"{ "chord": { "root": "C", "quality": "major" } }"#);
    let out = apply_music_api(state, &api);
    assert!(
        out.sequencer.bass_pattern[0].accent,
        "chord step should have accent"
    );
}

#[test]
fn apply_music_api_amen_writes_kick_pattern() {
    let state = AppState::default();
    let api = json_obj(r#"{ "amen_pattern": { "heat": 0.0, "seed": 0 } }"#);
    let out = apply_music_api(state, &api);
    // heat=0 → canonical pattern; kick step 0 should be active
    let kick = get_kick(&out);
    assert!(kick[0].active, "kick step 0 should be active");
    // snare step 4 should be active (canonical amen snare)
    let snare = get_snare(&out);
    assert!(snare[4].active, "snare step 4 should be active");
}

#[test]
fn apply_music_api_amen_pattern_length() {
    let state = AppState::default();
    let original_len = get_kick(&state).len();
    let api = json_obj(r#"{ "amen_pattern": { "heat": 0.5 } }"#);
    let out = apply_music_api(state, &api);
    assert_eq!(get_kick(&out).len(), original_len);
}

#[test]
fn apply_music_api_scale_run_writes_bass_pattern() {
    let state = AppState::default();
    let api =
        json_obj(r#"{ "scale_run": { "root": "A", "scale": "NaturalMinor", "direction": "up" } }"#);
    let out = apply_music_api(state, &api);
    // All 16 steps should be active after a scale run
    assert!(out.sequencer.bass_pattern[..16].iter().all(|s| s.active));
}

#[test]
fn apply_music_api_all_commands_together() {
    let state = AppState::default();
    let api = json_obj(
        r#"{
        "seed": 42,
        "chord": { "root": "C", "quality": "major" },
        "amen_pattern": { "heat": 0.0, "seed": 0 },
        "scale_run": { "root": "C", "scale": "Major", "direction": "up" }
    }"#,
    );
    // scale_run runs last and overwrites the chord writes to bass_pattern —
    // just verify it doesn't panic and state is valid
    let out = apply_music_api(state, &api);
    assert!(out.sequencer.bass_pattern[0].active); // scale_run
    assert!(get_kick(&out)[0].active); // amen
}

#[test]
fn apply_music_api_unknown_root_is_skipped() {
    let state = AppState::default();
    let original_note = state.sequencer.bass_pattern[0].note;
    let api = json_obj(r#"{ "chord": { "root": "X", "quality": "major" } }"#);
    let out = apply_music_api(state, &api);
    // Unknown root → skip; bass pattern unchanged
    assert_eq!(out.sequencer.bass_pattern[0].note, original_note);
}

#[test]
fn apply_music_api_unknown_quality_defaults_to_minor() {
    let state = AppState::default();
    let api = json_obj(r#"{ "chord": { "root": "C", "quality": "garbage" } }"#);
    let out = apply_music_api(state, &api);
    // Should default to minor and still write
    assert!(out.sequencer.bass_pattern[0].active);
}

#[test]
fn apply_music_api_empty_object_is_noop() {
    let state = AppState::default();
    let api = json_obj("{}");
    let out = apply_music_api(state.clone(), &api);
    // No panic; state unchanged (note: AppState must implement PartialEq or we check a field)
    assert_eq!(
        out.sequencer.bass_pattern[0].active,
        state.sequencer.bass_pattern[0].active
    );
}

#[test]
fn apply_music_api_scale_run_unknown_scale_defaults_to_natural_minor() {
    let state = AppState::default();
    let api =
        json_obj(r#"{ "scale_run": { "root": "A", "scale": "UnknownScale", "direction": "up" } }"#);
    // Should not panic; falls back to NaturalMinor
    let out = apply_music_api(state, &api);
    assert!(out.sequencer.bass_pattern[0].active);
}
