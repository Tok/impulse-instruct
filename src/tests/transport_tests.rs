// Regression tests for the "play button turns off after some beats"
// bug.  The LLM writeback paths replace `s.sequencer` wholesale with
// a snapshot computed outside the lock — if the user pressed Play
// during inference, the snapshot's stale `running=false` would
// clobber the live `running=true`.  `preserve_sequencer_transport`
// is the pure helper that keeps transport live across the swap.
use crate::state::{AppState, preserve_sequencer_transport};

#[test]
fn keeps_live_running_when_incoming_is_stopped() {
    let mut live = AppState::default();
    live.sequencer.running = true;
    live.sequencer.current_step = 7;
    let mut incoming = live.sequencer.clone();
    incoming.running = false; // stale snapshot value
    incoming.current_step = 0; // stale snapshot value
    incoming.bpm = 160.0; // ← must still land (LLM-writable)
    preserve_sequencer_transport(&mut live.sequencer, incoming);
    assert!(
        live.sequencer.running,
        "user's Play must survive a stale writeback"
    );
    assert_eq!(live.sequencer.current_step, 7);
    assert!(
        (live.sequencer.bpm - 160.0).abs() < 1e-4,
        "LLM-written bpm should land (only transport is protected)"
    );
}

#[test]
fn keeps_live_stopped_when_incoming_is_running() {
    // Inverse direction: if the user STOPPED the sequencer during
    // inference, a late writeback with running=true must NOT restart
    // playback.  Same helper, symmetric guarantee.
    let mut live = AppState::default();
    live.sequencer.running = false;
    live.sequencer.current_step = 3;
    let mut incoming = live.sequencer.clone();
    incoming.running = true;
    incoming.current_step = 11;
    preserve_sequencer_transport(&mut live.sequencer, incoming);
    assert!(!live.sequencer.running);
    assert_eq!(live.sequencer.current_step, 3);
}

#[test]
fn lets_non_transport_fields_from_incoming_land() {
    // The helper only protects transport.  Everything else in the
    // incoming `SequencerState` (bpm, swing, patterns, etc.) must
    // land, otherwise the LLM can't apply any rewrites.
    let mut live = AppState::default();
    live.sequencer.running = true;
    let mut incoming = live.sequencer.clone();
    incoming.swing = 0.42;
    incoming.bpm = 140.0;
    incoming.running = false;
    preserve_sequencer_transport(&mut live.sequencer, incoming);
    assert!((live.sequencer.swing - 0.42).abs() < 1e-4);
    assert!((live.sequencer.bpm - 140.0).abs() < 1e-4);
    assert!(live.sequencer.running, "transport preserved");
}
