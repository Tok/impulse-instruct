// ─── tests/ws_state_tests.rs ──────────────────────────────────────────────────
// WebSocket state-push helpers — the actual socket plumbing needs a
// running tokio runtime + tower-test, but the change-detection hash
// is a tight pure function we can lock down here.

#[cfg(test)]
mod ws_state_hash {
    use crate::api::ws_state_hash;

    #[test]
    fn empty_input_has_known_hash() {
        // FNV-1a 64-bit's offset basis is fixed by the algorithm —
        // empty input must equal the basis.
        assert_eq!(ws_state_hash(b""), 0xcbf29ce484222325);
    }

    #[test]
    fn identical_bytes_collide() {
        let a = ws_state_hash(b"impulse");
        let b = ws_state_hash(b"impulse");
        assert_eq!(a, b);
    }

    #[test]
    fn one_byte_change_changes_hash() {
        // Tiny mutations must always flip the hash; otherwise the
        // "did anything change" gate would silently drop pushes.
        let a = ws_state_hash(b"impulse");
        let b = ws_state_hash(b"impulsf");
        assert_ne!(a, b);
    }

    #[test]
    fn order_matters() {
        // FNV-1a is sensitive to byte order — assert that so future
        // refactors can't quietly switch to a commutative hash.
        let a = ws_state_hash(b"abc");
        let b = ws_state_hash(b"cba");
        assert_ne!(a, b);
    }

    #[test]
    fn large_input_is_finite() {
        // A real AppState serialises to ~50 KB.  Sanity: hashing a
        // big buffer terminates and produces a non-trivial value.
        let buf = vec![0x42u8; 64 * 1024];
        let h = ws_state_hash(&buf);
        assert_ne!(h, 0);
        assert_ne!(h, ws_state_hash(b""));
    }

    #[test]
    fn full_state_hash_changes_when_bpm_changes() {
        let mut s = crate::state::AppState::default();
        let h0 = ws_state_hash(serde_json::to_string(&s).unwrap().as_bytes());
        s.sequencer.bpm = 142.0;
        let h1 = ws_state_hash(serde_json::to_string(&s).unwrap().as_bytes());
        assert_ne!(
            h0, h1,
            "WS push gate would miss a BPM change with identical hashes"
        );
    }

    #[test]
    fn full_state_hash_stable_when_state_unchanged() {
        let s = crate::state::AppState::default();
        let h0 = ws_state_hash(serde_json::to_string(&s).unwrap().as_bytes());
        let h1 = ws_state_hash(serde_json::to_string(&s).unwrap().as_bytes());
        assert_eq!(
            h0, h1,
            "Same state must hash the same — unchanged-skip gate depends on it"
        );
    }
}
