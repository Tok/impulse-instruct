// ─── sync/link.rs ────────────────────────────────────────────────────────────
// Ableton Link integration — bidirectional tempo + bar-phase sync
// over UDP multicast.  Compatible with Ableton Live, AUM, Algoriddim
// djay, MOD devices, the official LinkHut tester, and any other Link-
// aware app on the same LAN.
//
// Two implementations behind a Cargo feature:
//
//   * `link` feature ON  → wraps `rusty_link::AblLink` (which in
//     turn wraps Ableton's official C++ Link library).  Real
//     network participation, peer discovery, beat-time clock.
//   * `link` feature OFF → no-op stub.  Methods compile to nothing
//     so the rest of the codebase doesn't need conditional plumbing
//     at every call site.  UI shows "unavailable" when stub.
//
// API surface is intentionally narrow.  Three operations the rest of
// the app needs:
//   1. `enable(bool)` — opt in / out of network participation.
//   2. `pull(local_bpm) -> Option<f32>` — call once per UI tick;
//      returns `Some(bpm)` when the network tempo differs from
//      `local_bpm` (so the caller can write it back to AppState).
//   3. `push(local_bpm)` — call when the user changes BPM locally,
//      so peers see the new tempo.
//
// V1 stays tempo-only.  Bar-phase alignment (snap our sequencer
// step counter to Link's quantum) is a planned follow-up — it
// requires touching the sequencer clock advance logic, so it ships
// as Link-V2.

use std::sync::atomic::{AtomicBool, Ordering};

/// Link's "quantum" — bars per cycle.  4 = 1 bar at 4/4, the default
/// across Live + LinkHut + most mobile apps.  Used by phase queries
/// in the (future) bar-phase-sync path.
pub const LINK_QUANTUM: f64 = 4.0;

#[cfg(feature = "link")]
mod imp {
    use rusty_link::{AblLink, SessionState};

    pub struct Inner {
        link: AblLink,
        scratch: SessionState,
    }

    impl Inner {
        pub fn new(initial_bpm: f32) -> Self {
            let mut link = AblLink::new(initial_bpm as f64);
            link.enable(false); // wait for the user toggle
            Self {
                link,
                scratch: SessionState::new(),
            }
        }

        pub fn enable(&mut self, on: bool) {
            self.link.enable(on);
        }

        pub fn is_enabled(&self) -> bool {
            self.link.is_enabled()
        }

        pub fn num_peers(&self) -> usize {
            self.link.num_peers() as usize
        }

        /// Read the network-shared tempo.  Returns `None` when the
        /// network value matches `local_bpm` within 0.01 BPM (so the
        /// caller doesn't churn AppState on every tick over float
        /// jitter).
        pub fn pull_tempo(&mut self, local_bpm: f32) -> Option<f32> {
            self.link.capture_app_session_state(&mut self.scratch);
            let net = self.scratch.tempo() as f32;
            if (net - local_bpm).abs() > 0.01 {
                Some(net.clamp(20.0, 999.0))
            } else {
                None
            }
        }

        /// Write `local_bpm` to the network so other peers follow.
        /// Cheap to call repeatedly — Link debounces internally.
        pub fn push_tempo(&mut self, local_bpm: f32) {
            self.link.capture_app_session_state(&mut self.scratch);
            self.scratch
                .set_tempo(local_bpm as f64, self.link.clock_micros());
            self.link.commit_app_session_state(&self.scratch);
        }
    }
}

#[cfg(not(feature = "link"))]
mod imp {
    /// Stub when the `link` feature is off.  Carries no state; every
    /// operation is a no-op.  Lets the rest of the app call
    /// `pull_tempo` / `push_tempo` unconditionally without `cfg`s
    /// peppered through every call site.
    pub struct Inner;

    impl Inner {
        pub fn new(_initial_bpm: f32) -> Self {
            Self
        }
        pub fn enable(&mut self, _on: bool) {}
        pub fn num_peers(&self) -> usize {
            0
        }
        pub fn pull_tempo(&mut self, _local_bpm: f32) -> Option<f32> {
            None
        }
        pub fn push_tempo(&mut self, _local_bpm: f32) {}
    }
}

pub struct LinkSync {
    inner: imp::Inner,
    /// Cached enable state — checked before any inner call so the
    /// stub path stays cheap and the real path skips the C++ FFI
    /// when the user has Link off.
    user_enabled: AtomicBool,
}

impl LinkSync {
    /// Create a new LinkSync.  Disabled until `enable(true)` — gives
    /// the user explicit control over whether the app participates
    /// in the local-network session.
    pub fn new(initial_bpm: f32) -> Self {
        Self {
            inner: imp::Inner::new(initial_bpm),
            user_enabled: AtomicBool::new(false),
        }
    }

    /// True when this build supports real Link (the `link` cargo
    /// feature is on).  UI uses this to gate the "unavailable"
    /// message vs the working toggle.
    pub fn is_supported() -> bool {
        cfg!(feature = "link")
    }

    pub fn enable(&mut self, on: bool) {
        self.user_enabled.store(on, Ordering::Relaxed);
        self.inner.enable(on);
    }

    pub fn is_enabled(&self) -> bool {
        self.user_enabled.load(Ordering::Relaxed)
    }

    pub fn num_peers(&self) -> usize {
        if !self.is_enabled() {
            return 0;
        }
        self.inner.num_peers()
    }

    pub fn pull_tempo(&mut self, local_bpm: f32) -> Option<f32> {
        if !self.is_enabled() {
            return None;
        }
        self.inner.pull_tempo(local_bpm)
    }

    pub fn push_tempo(&mut self, local_bpm: f32) {
        if !self.is_enabled() {
            return;
        }
        self.inner.push_tempo(local_bpm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_or_real_constructs_cleanly() {
        // `LinkSync::new` must work in both build configs without
        // panicking.  The supported flag mirrors the cargo feature.
        let mut link = LinkSync::new(120.0);
        assert!(!link.is_enabled(), "starts disabled");
        link.enable(true);
        assert!(link.is_enabled(), "enable flips the user flag");
        link.enable(false);
        assert!(!link.is_enabled());
    }

    #[test]
    fn pull_tempo_returns_none_when_disabled() {
        // A disabled LinkSync should never bother the inner impl —
        // important for the stub build, where calling pull is fine
        // but for the real build it avoids C++ FFI churn.
        let mut link = LinkSync::new(120.0);
        assert_eq!(link.pull_tempo(120.0), None);
        assert_eq!(link.pull_tempo(80.0), None);
    }

    #[test]
    fn num_peers_zero_when_disabled() {
        let link = LinkSync::new(120.0);
        assert_eq!(link.num_peers(), 0);
    }

    #[test]
    fn is_supported_matches_feature_flag() {
        // True only when built with `--features link`.
        assert_eq!(LinkSync::is_supported(), cfg!(feature = "link"));
    }
}
