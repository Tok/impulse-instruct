// ─── ui/panels/mod.rs ─────────────────────────────────────────────────────────
// Re-exports all panel draw functions.

pub mod an1x;
pub mod bass;
pub mod drums;
pub mod fx;
pub mod hoover;
pub mod lfo;
pub mod noise;
pub mod piano;
pub mod sequencer;
pub mod sequencer_chain;
pub(super) mod sequencer_drums;
pub mod spectrum;
pub mod stereo_meter;
pub mod timeline;
pub mod tts;

pub use an1x::draw_an1x;
pub use bass::draw_bass;
pub use drums::{draw_amen, draw_kit_a, draw_kit_b};
pub use fx::draw_fx;
pub use hoover::draw_hoover;
pub use lfo::{draw_lfo, draw_lfo_slot};
pub use noise::draw_noise;
pub use piano::draw_piano;
pub use sequencer::draw_sequencer;
pub use spectrum::draw_spectrum;
pub use stereo_meter::draw_stereo_meter;
pub use timeline::draw_timeline;
pub use tts::draw_tts;
