// ─── tests/midi_export_tests.rs ──────────────────────────────────────────────
// Covers the Standard MIDI File variable-length-quantity encoder.  VLQ is
// the foundation of every delta-time in an SMF track — getting it wrong
// silently corrupts every exported .mid file for downstream DAWs.
// Canonical test vectors are taken from the MIDI spec.

use crate::midi::export::write_vlq;

fn vlq(v: u32) -> Vec<u8> {
    let mut out = Vec::new();
    write_vlq(v, &mut out);
    out
}

#[test]
fn vlq_single_byte_values_have_no_continuation_bit() {
    // 0..127 fit in one byte, MSB clear.
    assert_eq!(vlq(0x00), vec![0x00]);
    assert_eq!(vlq(0x40), vec![0x40]);
    assert_eq!(vlq(0x7F), vec![0x7F]);
}

#[test]
fn vlq_two_byte_boundary_matches_midi_spec() {
    // 0x80 → [0x81, 0x00]  (spec's first worked example).
    assert_eq!(vlq(0x80), vec![0x81, 0x00]);
    assert_eq!(vlq(0x2000), vec![0xC0, 0x00]);
    assert_eq!(vlq(0x3FFF), vec![0xFF, 0x7F]);
}

#[test]
fn vlq_three_byte_boundary_matches_midi_spec() {
    assert_eq!(vlq(0x4000), vec![0x81, 0x80, 0x00]);
    assert_eq!(vlq(0x100000), vec![0xC0, 0x80, 0x00]);
    assert_eq!(vlq(0x1FFFFF), vec![0xFF, 0xFF, 0x7F]);
}

#[test]
fn vlq_four_byte_max_value_matches_midi_spec() {
    // 0x0FFFFFFF is the largest value an SMF VLQ encodes (28 data bits).
    assert_eq!(vlq(0x0FFFFFFF), vec![0xFF, 0xFF, 0xFF, 0x7F]);
}

#[test]
fn vlq_continuation_bit_set_on_all_but_last_byte() {
    // For any multi-byte VLQ, exactly the last byte must have MSB clear
    // and every earlier byte must have MSB set.
    for &v in &[0x80_u32, 0x4000, 0x12345, 0x0FFFFFFF] {
        let bytes = vlq(v);
        assert!(bytes.len() > 1, "test value must be multi-byte: {v:#x}");
        for (i, &b) in bytes.iter().enumerate() {
            if i == bytes.len() - 1 {
                assert_eq!(b & 0x80, 0, "last byte of VLQ must have MSB=0 (v={v:#x})");
            } else {
                assert_eq!(
                    b & 0x80,
                    0x80,
                    "non-last byte of VLQ must have MSB=1 (v={v:#x}, i={i})",
                );
            }
        }
    }
}

#[test]
fn vlq_appends_rather_than_overwrites() {
    // write_vlq takes &mut Vec<u8> — it must APPEND to the buffer, not
    // clear it.  Breaking this would corrupt every track body.
    let mut out = vec![0xAA, 0xBB];
    write_vlq(0x81, &mut out);
    assert_eq!(out, vec![0xAA, 0xBB, 0x81, 0x01]);
}

#[test]
fn vlq_roundtrip_via_midi_spec_decoder() {
    // Confirm against a hand-rolled decoder using the canonical algorithm
    // from the MIDI spec.  Catches any off-by-one / shift direction bugs
    // without relying on a third-party crate.
    fn decode(bytes: &[u8]) -> u32 {
        let mut v: u32 = 0;
        for b in bytes {
            v = (v << 7) | (b & 0x7F) as u32;
            if b & 0x80 == 0 {
                break;
            }
        }
        v
    }
    for &v in &[
        0u32,
        1,
        127,
        128,
        0x2000,
        0x3FFF,
        0x4000,
        0x10_0000,
        0x0FFF_FFFF,
    ] {
        assert_eq!(decode(&vlq(v)), v, "roundtrip failed for {v:#x}");
    }
}
