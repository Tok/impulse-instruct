// ─── sync/mod.rs ─────────────────────────────────────────────────────────────
// Network sync primitives.  Currently hosts the Ableton Link
// integration (tempo + bar-phase sync over UDP multicast); future
// MIDI clock master / OSC tempo sync would slot in here too.

pub mod link;
pub use link::{LINK_QUANTUM, LinkSync};
