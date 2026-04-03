// ─── ui/panels/mod.rs ─────────────────────────────────────────────────────────
// Re-exports all panel draw functions.

pub mod bass;
pub mod drums;
pub mod fx;
pub mod piano;
pub mod sequencer;

pub use bass::draw_bass;
pub use drums::{draw_kit_a, draw_kit_b};
pub use fx::draw_fx;
pub use piano::draw_piano;
pub use sequencer::draw_sequencer;
