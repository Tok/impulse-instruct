// ─── ui/link_handler.rs ──────────────────────────────────────────────────────
// Per-frame Ableton Link tempo sync.  Called once per UI tick from
// `app_update::update`.  Two flows:
//
//   1. **Pull from network**: if Link reports a different BPM than
//      ours, update `state.sequencer.bpm`.  We remember the value
//      we wrote (`last_link_bpm`) so the next iteration knows the
//      change came from us-pulling, not the user.
//
//   2. **Push from user**: if the local BPM differs from
//      `last_link_bpm` AND from the network's current value, the
//      user (or LLM, or MIDI clock) changed it — push our value
//      to the network so peers follow.
//
// Stays cheap when Link is disabled — both paths early-return on
// the `is_enabled()` flag.  When the `link` cargo feature is off
// the underlying calls are no-ops anyway, so this code compiles
// and runs fine without the C++ toolchain.

use super::ImpulseApp;

impl ImpulseApp {
    /// Drive one cycle of the Link bidirectional tempo sync.  Idempotent
    /// — safe to call every frame.
    pub(super) fn tick_link_sync(&mut self) {
        // First ensure the Link participation matches the user's pref.
        // Toggling enable() is cheap; the inner stub is a no-op when
        // the feature is off.
        let pref_enabled = self.state.read().ui_prefs.link_enabled;
        if pref_enabled != self.link_sync.is_enabled() {
            self.link_sync.enable(pref_enabled);
            // Reset the last-pulled BPM so the next pull / push edge
            // doesn't see stale state from a previous session.
            self.last_link_bpm = 0.0;
        }
        if !self.link_sync.is_enabled() {
            return;
        }

        // Snapshot local BPM once — both branches read it.
        let local_bpm = self.state.read().sequencer.bpm;

        // Pull: if the network has a different tempo, write to AppState.
        // The `pull_tempo` helper returns None when the values match
        // within 0.01 BPM, so this only fires when there's a real
        // change.
        if let Some(net_bpm) = self.link_sync.pull_tempo(local_bpm) {
            self.state.write().sequencer.bpm = net_bpm;
            self.last_link_bpm = net_bpm;
            // BPM is part of AudioParams — push so the audio thread
            // sees the new tempo on the next block.
            self.push_audio_params();
            return;
        }

        // Push: the user / LLM / MIDI changed our BPM.  Detect by
        // comparing against `last_link_bpm` (the value we wrote on
        // the last pull).  Tolerance avoids float-jitter loops.
        if (local_bpm - self.last_link_bpm).abs() > 0.05 {
            self.link_sync.push_tempo(local_bpm);
            self.last_link_bpm = local_bpm;
        }
    }
}
