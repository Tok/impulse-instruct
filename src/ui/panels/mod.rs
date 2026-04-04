// ─── ui/panels/mod.rs ─────────────────────────────────────────────────────────
// Re-exports all panel draw functions.

pub mod an1x;
pub mod bass;
pub mod drums;
pub mod fx;
pub mod hoover;
pub mod lfo;
pub mod piano;
pub mod sequencer;

pub use an1x::draw_an1x;
pub use bass::draw_bass;
pub use drums::{draw_kit_a, draw_kit_b};
pub use fx::draw_fx;
pub use hoover::draw_hoover;
pub use lfo::draw_lfo;
pub use piano::draw_piano;
pub use sequencer::draw_sequencer;
