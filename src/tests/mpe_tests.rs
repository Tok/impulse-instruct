// ─── tests/mpe_tests.rs ───────────────────────────────────────────────────────
// MPE parser + classifier tests.  The full midi_handler routing path
// needs an `ImpulseApp` (rtrb / channels / state), but the parse +
// classify pieces are pure and locked here.

#[cfg(test)]
mod parse {
    use crate::midi::{MidiEvent, parse_midi};

    #[test]
    fn channel_pressure_decodes_one_data_byte() {
        // 0xD2 = ChannelPressure on channel 2 (zero-indexed), pressure=64.
        let evt = parse_midi(&[0xD2, 64]).expect("ChannelPressure should decode");
        match evt {
            MidiEvent::ChannelPressure { channel, value } => {
                assert_eq!(channel, 2);
                assert_eq!(value, 64);
            }
            other => panic!("expected ChannelPressure, got {other:?}"),
        }
    }

    #[test]
    fn pitch_bend_decodes_signed_unit_range() {
        // 14-bit value 8192 (msb=64, lsb=0) → centre, expected 0.0.
        let evt = parse_midi(&[0xE3, 0, 64]).expect("PitchBend should decode");
        match evt {
            MidiEvent::PitchBend { channel, value } => {
                assert_eq!(channel, 3);
                assert!(value.abs() < 1e-3, "centre should be ~0, got {value}");
            }
            other => panic!("expected PitchBend, got {other:?}"),
        }
    }

    #[test]
    fn pitch_bend_full_negative_decodes_to_minus_one() {
        let evt = parse_midi(&[0xE0, 0, 0]).expect("PitchBend should decode");
        match evt {
            MidiEvent::PitchBend { value, .. } => {
                assert!((value - -1.0).abs() < 1e-3, "got {value}");
            }
            other => panic!("expected PitchBend, got {other:?}"),
        }
    }

    #[test]
    fn note_on_unaffected_by_new_arms() {
        // Defensive: adding ChannelPressure to the parser shouldn't
        // perturb existing NoteOn decoding.
        let evt = parse_midi(&[0x91, 60, 100]).expect("NoteOn should decode");
        assert!(matches!(
            evt,
            MidiEvent::NoteOn {
                channel: 1,
                note: 60,
                velocity: 100
            }
        ));
    }
}

#[cfg(test)]
mod classifier {
    use crate::midi::{is_mpe_note_channel, pressure_to_unit};

    #[test]
    fn channel_zero_is_master_not_note_channel() {
        // Ch 1 in 1-indexed MIDI = ch 0 zero-indexed = master.
        assert!(!is_mpe_note_channel(0));
    }

    #[test]
    fn channels_one_through_fifteen_are_note_channels() {
        for ch in 1..=15 {
            assert!(
                is_mpe_note_channel(ch),
                "channel {ch} should be a note channel"
            );
        }
    }

    #[test]
    fn pressure_zero_maps_to_zero() {
        assert!((pressure_to_unit(0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn pressure_max_maps_to_one() {
        assert!((pressure_to_unit(127) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pressure_mid_maps_to_about_half() {
        let v = pressure_to_unit(64);
        assert!((v - 64.0 / 127.0).abs() < 1e-6);
    }

    #[test]
    fn pressure_high_bit_is_masked() {
        // Out-of-range bytes shouldn't push the value past 1.0; the
        // helper masks the 8th bit so 0xFF → 0x7F → 1.0.
        let v = pressure_to_unit(0xFF);
        assert!((v - 1.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod expression_state {
    use crate::state::{AppState, MpeExpression};

    #[test]
    fn default_mpe_state_is_zero() {
        let s = AppState::default();
        assert_eq!(s.mpe, MpeExpression::default());
        assert_eq!(s.mpe.channel, 0);
        assert!((s.mpe.pitch_bend - 0.0).abs() < 1e-6);
        assert!((s.mpe.pressure - 0.0).abs() < 1e-6);
        assert!((s.mpe.timbre - 0.0).abs() < 1e-6);
    }

    #[test]
    fn mpe_struct_is_clone_eq_safe() {
        // Used as a cached field and compared by tests / WS hash —
        // verify the derives are wired.
        let a = MpeExpression {
            channel: 5,
            pitch_bend: 0.25,
            pressure: 0.5,
            timbre: 0.75,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
