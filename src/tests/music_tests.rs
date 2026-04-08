// ─── tests/music_tests.rs ────────────────────────────────────────────────────
// Tests for pure music theory functions in state/music.rs.

#[cfg(test)]
mod scale_tests {
    use crate::state::{Scale, note_in_scale, scale_degree, scale_notes, snap_to_scale};

    #[test]
    fn note_in_c_minor() {
        // C natural minor: C D Eb F G Ab Bb
        assert!(note_in_scale(60, 0, Scale::NaturalMinor)); // C4
        assert!(note_in_scale(62, 0, Scale::NaturalMinor)); // D4
        assert!(note_in_scale(63, 0, Scale::NaturalMinor)); // Eb4
        assert!(!note_in_scale(64, 0, Scale::NaturalMinor)); // E4 not in C minor
        assert!(note_in_scale(65, 0, Scale::NaturalMinor)); // F4
    }

    #[test]
    fn note_in_scale_wraps_octaves() {
        // C in any octave should be in C minor
        assert!(note_in_scale(0, 0, Scale::NaturalMinor)); // C0
        assert!(note_in_scale(12, 0, Scale::NaturalMinor)); // C1
        assert!(note_in_scale(48, 0, Scale::NaturalMinor)); // C3
        assert!(note_in_scale(120, 0, Scale::NaturalMinor)); // C9
    }

    #[test]
    fn note_in_a_minor_root_offset() {
        // A natural minor: A B C D E F G (root=9)
        assert!(note_in_scale(69, 9, Scale::NaturalMinor)); // A4
        assert!(note_in_scale(60, 9, Scale::NaturalMinor)); // C4 (minor 3rd)
        assert!(!note_in_scale(61, 9, Scale::NaturalMinor)); // C#4 not in A minor
    }

    #[test]
    fn chromatic_includes_all() {
        for note in 0..=127u8 {
            assert!(note_in_scale(note, 0, Scale::Chromatic));
        }
    }

    #[test]
    fn scale_degree_tonic() {
        assert_eq!(scale_degree(60, 0, Scale::NaturalMinor), Some(0)); // C = tonic
        assert_eq!(scale_degree(62, 0, Scale::NaturalMinor), Some(1)); // D = 2nd
        assert_eq!(scale_degree(63, 0, Scale::NaturalMinor), Some(2)); // Eb = 3rd
    }

    #[test]
    fn scale_degree_none_for_outside() {
        assert_eq!(scale_degree(64, 0, Scale::NaturalMinor), None); // E not in C minor
    }

    #[test]
    fn scale_notes_c_major_count() {
        let notes = scale_notes(0, Scale::Major);
        // 7 notes per octave, ~10.7 octaves in 0-127
        assert!(notes.len() >= 70);
        assert!(notes.contains(&60)); // C4
        assert!(notes.contains(&64)); // E4
        assert!(!notes.contains(&61)); // C#4 not in C major
    }

    #[test]
    fn scale_notes_pentatonic_count() {
        let notes = scale_notes(0, Scale::Pentatonic);
        // 5 notes per octave
        assert!(notes.len() >= 50);
    }

    #[test]
    fn snap_to_scale_exact_note_unchanged() {
        // C4 is in C minor → should stay
        assert_eq!(snap_to_scale(60, 0, Scale::NaturalMinor), 60);
    }

    #[test]
    fn snap_to_scale_snaps_to_nearest() {
        // E4 (64) not in C minor → snap to Eb4 (63) or F4 (65)
        let snapped = snap_to_scale(64, 0, Scale::NaturalMinor);
        assert!(
            snapped == 63 || snapped == 65,
            "should snap to Eb or F, got {}",
            snapped
        );
    }

    #[test]
    fn snap_chromatic_is_identity() {
        for note in 0..=127u8 {
            assert_eq!(snap_to_scale(note, 0, Scale::Chromatic), note);
        }
    }

    #[test]
    fn dorian_has_raised_sixth() {
        // C Dorian: C D Eb F G A Bb (A is natural, unlike C minor which has Ab)
        assert!(note_in_scale(69, 0, Scale::Dorian)); // A4 (note 69, offset 9)
        assert!(!note_in_scale(68, 0, Scale::Dorian)); // Ab4 NOT in C Dorian
    }

    #[test]
    fn scale_from_str_round_trip() {
        for scale in &[
            Scale::Major,
            Scale::NaturalMinor,
            Scale::Dorian,
            Scale::Phrygian,
            Scale::Lydian,
            Scale::Mixolydian,
            Scale::Locrian,
            Scale::Pentatonic,
            Scale::Blues,
            Scale::Chromatic,
        ] {
            let name = scale.name();
            let parsed = Scale::from_str(name);
            assert!(
                parsed.is_some(),
                "failed to parse {:?} from name {:?}",
                scale,
                name
            );
        }
    }
}
