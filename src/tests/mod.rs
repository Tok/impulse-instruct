// ─── tests/mod.rs ─────────────────────────────────────────────────────────────
// Unit tests for pure functions — split into submodules by domain.
// Run: ./run-tests.sh
//      ./run-tests.sh --coverage

mod analysis_tests;
mod api_tests;
mod coverage_tests;
mod dsp_fx_tests;
mod dsp_mod_apply_tests;
mod dsp_tests;
mod dsp_voice_extra_tests;
mod dsp_voice_primitives_tests;
mod fx_plan_tests;
mod helpers_tests;
mod jam_tools_tests;
mod lane_scheduler_tests;
mod llm_apply_extra_tests;
mod llm_apply_seq_tests;
mod llm_apply_tests;
mod llm_apply_voice_tests;
mod llm_plumbing_tests;
mod llm_tests;
mod music_api_tests;
mod music_tests;
mod persistence_tests;
mod planner_tests;
mod rack_reach_tests;
mod rack_tests;
mod seq_tests;
mod state_tests;
mod tts_tests;
mod vram_tests;
