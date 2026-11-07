// ─── ui/panels/mod.rs ─────────────────────────────────────────────────────────
// Re-exports all panel draw functions.

/// Standard PAN slider width shared across voice panels.
pub(super) const PAN_SLIDER_W: f32 = 140.0;

/// Standard horizontal spacing between knobs inside glass groups and FX panels.
pub(super) const KNOB_SPACING: f32 = 8.0;

/// Spacing between adjacent glass panes (used in ui.horizontal layouts containing glass groups).
pub(crate) const GLASS_GAP: f32 = 5.0;

pub mod additive;
pub mod amen;
mod amen_strips;
pub(crate) mod amen_viz;
pub mod an1x;
pub mod bass;
mod bass_locks;
mod bass_noise;
mod bass_wave;
pub mod chiptune;
pub mod comparator;
pub mod crossfader;
pub mod cv_seq;
pub mod drums;
pub mod event_stream_module;
pub mod fm_ops;
pub mod function_gen;
pub mod fx;
pub mod gabber;
pub mod gr_history;
pub mod granular;
pub mod hoover;
pub mod lfo;
pub mod logic_gate;
pub mod math_module;
pub mod modal;
pub mod noise;
pub mod onset_grid;
pub mod pattern_heatmap;
pub mod pendulum;
pub mod piano;
pub mod pluck;
pub mod quantizer;
pub mod sample_hold;
pub mod sample_instrument;
pub(crate) mod sample_instrument_viz;
pub mod scope_module;
pub mod sequencer;
pub(super) mod sequencer_automation;
pub mod sequencer_chain;
pub(super) mod sequencer_drums;
pub(super) mod sequencer_header;
pub(super) mod sequencer_preecho;
pub(super) mod sequencer_sample_lane;
pub mod slew;
pub mod spectrum;
pub mod stereo_meter;
pub mod theremin;
pub mod timeline;
pub mod trigger_div;
pub mod tts;
pub mod viz;
pub mod vocal;
pub mod voice_meter_strip;
pub mod wavetable;

pub use additive::draw_additive;
pub use amen::draw_amen;
pub use an1x::draw_an1x;
pub use bass::draw_bass;
pub use chiptune::draw_chiptune;
pub use comparator::draw_comparator;
pub use crossfader::draw_crossfader;
pub use cv_seq::draw_cv_seq;
pub use drums::{draw_kit_a, draw_kit_b};
pub use event_stream_module::draw_event_stream_module;
pub use fm_ops::draw_fm_ops;
pub use function_gen::draw_function_gen;
pub use fx::draw_fx;
pub use gabber::draw_gabber;
pub use gr_history::draw_gr_history;
pub use granular::draw_granular;
pub use hoover::draw_hoover;
pub use lfo::{draw_lfo, draw_lfo_slot};
pub use logic_gate::draw_logic_gate;
pub use math_module::draw_math;
pub use modal::draw_modal;
pub use noise::draw_noise;
pub use onset_grid::draw_onset_grid;
pub use pattern_heatmap::draw_pattern_heatmap;
pub use pendulum::draw_pendulum;
pub use piano::draw_piano;
pub use pluck::draw_pluck;
pub use quantizer::draw_quantizer;
pub use sample_hold::draw_sample_hold;
pub use sample_instrument::draw_sample_instrument;
pub use scope_module::draw_scope_module;
pub use sequencer::draw_sequencer;
pub use slew::draw_slew;
pub use spectrum::draw_spectrum;
pub use stereo_meter::draw_stereo_meter;
pub use theremin::draw_theremin;
pub use timeline::draw_timeline;
pub use trigger_div::draw_trigger_div;
pub use tts::draw_tts;
pub use viz::{
    draw_chord_display, draw_cv_seq_scope, draw_lfo_scope, draw_loudness_meter, draw_phase_wheel,
    draw_pitch_tracker, draw_spectrogram, draw_vectorscope,
};
pub use vocal::draw_vocal;
pub use voice_meter_strip::draw_voice_meter_strip;
pub use wavetable::draw_wavetable;
