// ─── audio/sfz_loader.rs ─────────────────────────────────────────────────────
// Off-audio-thread helper that turns a `.sfz` file path into a list of
// `SfzRegionRuntime` records ready for `AudioCommand::LoadSampleInstrumentSfz`.
//
// Steps:
//   1. Read the `.sfz` text + parse via `state::sfz::parse_sfz`.
//   2. For each region, load + resample the referenced sample via
//      `audio::load_audio_to_engine`.  V1 only handled .wav; V2 routes
//      through the unified loader so .flac / .aiff regions also resolve.
//      De-duplicate identical paths so the Salamander-style "10 velocity
//      layers point at the same wav for RR slots" case doesn't redo I/O.
//   3. Drop regions whose sample failed to load (logged at warn level so
//      a malformed pack doesn't kill the whole instrument — partial
//      success beats hard failure).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::audio::dsp::sample_instrument::SfzRegionRuntime;
use crate::audio::load_audio_to_engine;
use crate::state::parse_sfz;

/// Parse `path` as an SFZ file and load every referenced sample,
/// returning the runtime region list.  Returns `None` when the SFZ
/// itself can't be read or parsed; an empty vec means a valid file
/// with no playable regions (`<global>` / `<group>` only).  Files
/// where some regions load and some fail return the successful subset
/// — the user gets a partial instrument rather than silence.
pub fn load_sfz_file(path: &str) -> Option<Vec<SfzRegionRuntime>> {
    let text = std::fs::read_to_string(path).ok()?;
    let p = Path::new(path);
    let base_dir = p.parent().unwrap_or_else(|| Path::new("."));
    let regions = parse_sfz(&text, base_dir);
    if regions.is_empty() {
        log::warn!("sfz: {} parsed to zero playable regions", path);
        return Some(Vec::new());
    }
    // De-dupe sample loads — one Arc per unique path.  The key is the
    // resolved absolute path so equivalent relative paths in different
    // regions still cache.
    let mut cache: HashMap<std::path::PathBuf, Arc<Vec<f32>>> = HashMap::new();
    let mut out: Vec<SfzRegionRuntime> = Vec::with_capacity(regions.len());
    for region in regions {
        let arc = if let Some(a) = cache.get(&region.sample_path) {
            a.clone()
        } else {
            let path_str = region.sample_path.to_string_lossy().to_string();
            match load_audio_to_engine(&path_str) {
                Some(a) => {
                    cache.insert(region.sample_path.clone(), a.clone());
                    a
                }
                None => {
                    log::warn!(
                        "sfz: failed to load referenced sample {} — skipping region",
                        path_str
                    );
                    continue;
                }
            }
        };
        out.push(SfzRegionRuntime {
            region,
            samples: arc,
        });
    }
    Some(out)
}
