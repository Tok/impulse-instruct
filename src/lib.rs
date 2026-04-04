#![recursion_limit = "256"]
// ─── lib.rs ───────────────────────────────────────────────────────────────���───
// Library crate root — re-exports all modules so both the main binary and
// auxiliary binaries (model_eval, etc.) can share the same code.

pub mod api;
pub mod audio;
pub mod banner;
pub mod config;
pub mod export;
pub mod llm;
#[cfg(all(test, feature = "llm-tests"))]
pub mod llm_suite;
pub mod midi;
pub mod sequencer;
pub mod state;
pub mod sysinfo;
#[cfg(test)]
pub mod tests;
pub mod ui;
