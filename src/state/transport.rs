//! Transport preservation helpers for sequencer writebacks.

/// Install `incoming` onto `live`'s sequencer while preserving transport
/// fields — `running` and `current_step`.  Used by every LLM / style /
/// jam-cycle writeback path that swaps the full `SequencerState` with
/// an `incoming` value computed from a stale snapshot.
///
/// Without this, a user Play action that raced with the snapshot gets
/// clobbered on writeback: the snapshot's `running=false` lands after
/// the user's `running=true`.  The reported symptom was "play button
/// turns off after some beats" — the startup one-shot prompt's
/// writeback landing a few seconds into playback.
pub fn preserve_sequencer_transport(
    live: &mut crate::state::SequencerState,
    incoming: crate::state::SequencerState,
) {
    let step = live.current_step;
    let running = live.running;
    *live = incoming;
    live.current_step = step;
    live.running = running;
}
