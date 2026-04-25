// ─── ui/panels/mod.rs ─────────────────────────────────────────────────────────
// Re-exports all panel draw functions.

/// Standard PAN slider width shared across voice panels.
pub(super) const PAN_SLIDER_W: f32 = 140.0;

/// Standard horizontal spacing between knobs inside glass groups and FX panels.
pub(super) const KNOB_SPACING: f32 = 8.0;

/// Spacing between adjacent glass panes (used in ui.horizontal layouts containing glass groups).
pub(crate) const GLASS_GAP: f32 = 5.0;

pub mod amen;
mod amen_strips;
pub(crate) mod amen_viz;
pub mod an1x;
pub mod bass;
mod bass_locks;
mod bass_noise;
mod bass_wave;
pub mod drums;
pub mod event_stream_module;
pub mod fx;
pub mod gabber;
pub mod granular;
pub mod hoover;
pub mod lfo;
pub mod noise;
pub mod piano;
pub mod pluck;
pub mod sample_instrument;
pub mod scope_module;
pub mod sequencer;
pub(super) mod sequencer_automation;
pub mod sequencer_chain;
pub(super) mod sequencer_drums;
pub(super) mod sequencer_header;
pub(super) mod sequencer_preecho;
pub mod spectrum;
pub mod stereo_meter;
pub mod timeline;
pub mod tts;
pub mod viz;
pub mod wavetable;

pub use amen::draw_amen;
pub use an1x::draw_an1x;
pub use bass::draw_bass;
pub use drums::{draw_kit_a, draw_kit_b};
pub use event_stream_module::draw_event_stream_module;
pub use fx::draw_fx;
pub use gabber::draw_gabber;
pub use granular::draw_granular;
pub use hoover::draw_hoover;
pub use lfo::{draw_lfo, draw_lfo_slot};
pub use noise::draw_noise;
pub use piano::draw_piano;
pub use pluck::draw_pluck;
pub use sample_instrument::draw_sample_instrument;
pub use scope_module::draw_scope_module;
pub use sequencer::draw_sequencer;
pub use spectrum::draw_spectrum;
pub use stereo_meter::draw_stereo_meter;
pub use timeline::draw_timeline;
pub use tts::draw_tts;
pub use viz::{
    draw_chord_display, draw_lfo_scope, draw_loudness_meter, draw_phase_wheel, draw_pitch_tracker,
    draw_spectrogram, draw_vectorscope,
};
pub use wavetable::draw_wavetable;
