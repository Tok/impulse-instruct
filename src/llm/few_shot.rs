// ─── llm/few_shot.rs ─────────────────────────────────────────────────────────
// Per-lane in-context example bank.  Lets the user steer a lane's style
// without touching the system prompt — drop a JSON file at
// `examples/<lane_slug>.json` and the pipeline injects its
// `{prompt, output}` pairs into that lane's system prompt as concrete
// examples for the model to emulate.
//
// File format (an array):
//   [
//     { "prompt": "make it warmer", "output": "{\"bass\":{\"cutoff\":0.55}}" },
//     { "prompt": "fold the resonance",
//       "output": "{\"bass\":{\"resonance\":0.7,\"env_mod\":0.4}}" }
//   ]
//
// Slugs: "settings", "bass1".."bass4", "kit_a", "kit_b", "amen",
// "hoover", "an1x", "fx", "modulation", "rack".  Files are looked up
// relative to the binary's working directory (typically the repo
// root); missing files are silently skipped.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::lanes::LaneKind;

/// One in-context example: a user prompt the lane saw and the JSON
/// output that worked well for it.  Used as a few-shot reference in
/// the lane's system prompt.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FewShotExample {
    pub prompt: String,
    pub output: String,
}

/// Map a `LaneKind` to its slug used for the example filename.
/// Bass voice 0 → "bass1", voice 1 → "bass2", etc., to match the
/// existing planner labels and keep the file naming intuitive.
pub fn lane_slug(lane: LaneKind) -> &'static str {
    match lane {
        LaneKind::Settings => "settings",
        LaneKind::Bass(0) => "bass1",
        LaneKind::Bass(1) => "bass2",
        LaneKind::Bass(2) => "bass3",
        LaneKind::Bass(3) => "bass4",
        // Bass voices past 3 use the same examples as bass4 — there
        // are only 4 voice slots in the schema, so this is unreachable
        // by design but kept to keep the match exhaustive on `usize`.
        LaneKind::Bass(_) => "bass4",
        LaneKind::KitA => "kit_a",
        LaneKind::KitB => "kit_b",
        LaneKind::Amen => "amen",
        LaneKind::Hoover => "hoover",
        LaneKind::An1x => "an1x",
        LaneKind::Fx => "fx",
        LaneKind::Modulation => "modulation",
        LaneKind::Rack => "rack",
    }
}

/// Default example file path for a lane: `examples/<slug>.json` under
/// the current working directory.  Pure helper so the lookup logic is
/// unit-testable.
pub fn example_path_for(lane: LaneKind) -> PathBuf {
    Path::new("examples").join(format!("{}.json", lane_slug(lane)))
}

/// Load examples for a lane from `examples/<slug>.json`.  Returns an
/// empty list when the file is missing or malformed — the pipeline
/// treats few-shots as best-effort enrichment, never a hard
/// dependency.
pub fn load_examples_for_lane(lane: LaneKind) -> Vec<FewShotExample> {
    let path = example_path_for(lane);
    load_examples_from_path(&path)
}

/// Variant for unit tests / external tooling — loads from an explicit
/// path instead of resolving via `lane_slug`.
pub fn load_examples_from_path(path: &Path) -> Vec<FewShotExample> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<FewShotExample>>(&raw).unwrap_or_else(|e| {
        log::warn!("few_shot: failed to parse {}: {e}", path.display());
        Vec::new()
    })
}

/// Format a list of examples as a prompt section the lane builder
/// can append.  Returns an empty string when `examples` is empty so
/// callers can `format!("{}{}", prompt, render(...))` unconditionally.
///
/// Each example is rendered as a labelled `Prompt → Output` pair on
/// its own block.  Truncates at 5 examples; more would crowd the
/// context window.
pub fn render_examples_section(examples: &[FewShotExample]) -> String {
    if examples.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("\nEXAMPLES (prior outputs that worked — emulate the shape):\n");
    for (i, ex) in examples.iter().take(5).enumerate() {
        out.push_str(&format!(
            "  Example {}:\n    Prompt: {}\n    Output: {}\n",
            i + 1,
            ex.prompt.trim(),
            ex.output.trim(),
        ));
    }
    out
}
