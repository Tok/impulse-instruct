// ─── audio/sf2_loader.rs ─────────────────────────────────────────────────────
// SF2 SoundFont parser — extracts preset / instrument / sample mappings
// from a `.sf2` file and converts each playable region into the
// existing `SfzRegionRuntime` shape so the SampleInstrument can ingest
// SoundFont content via the same pipeline as `.sfz` files.
//
// V1 deliberately scoped:
//   * RIFF chunk walk (sfbk → INFO / sdta / pdta).
//   * `phdr` / `pbag` / `pgen` (preset → bag → generator) chain.
//   * `inst` / `ibag` / `igen` (instrument → bag → generator) chain.
//   * `shdr` (sample headers) + `smpl` (raw 16-bit PCM).
//   * Generators we honour: keyRange (43), velRange (44),
//     instrument link (41), sampleID link (53), overridingRootKey
//     (58), coarseTune (51), fineTune (52), pan (17),
//     initialAttenuation (48).
//   * Envelopes, filters, modulators, LFOs, sampleModes, key-on/off
//     transforms — all deferred to V2.  The SoundFont still plays;
//     the per-note shaping just falls back to the SfzRegion defaults.
//   * Loads only the FIRST preset.  SoundFont banks typically pack
//     dozens of presets (drum kit, piano, organ, ...); a future V2
//     adds a preset picker.  V1 = "audition the first preset".
//
// All output is `Vec<SfzRegionRuntime>` — same shape `sfz_loader`
// produces, so the SampleInstrument's existing SFZ trigger path
// handles SF2 content with zero changes.

use std::sync::Arc;

use crate::audio::dsp::sample_instrument::SfzRegionRuntime;
use crate::audio::{SAMPLE_RATE, SAMPLE_RATE_HZ};
use crate::state::sfz::SfzRegion;

/// One entry in the SoundFont's preset table.  Banks ship dozens of
/// these (drum kit, piano, organ, ...); the SampleInstrument UI uses
/// the list to render a preset picker.
#[derive(Clone, Debug)]
pub struct Sf2PresetInfo {
    /// Display name from the `phdr` chunk (trimmed of trailing nulls).
    pub name: String,
    /// MIDI bank number (0 for melodic, 128 for drums by convention).
    pub bank: u16,
    /// MIDI preset number (0..127) within the bank.
    pub preset: u16,
}

/// Parse `path` as an SF2 SoundFont and return the runtime region list
/// for the first preset.  Returns `None` on any I/O or parse error;
/// the SampleInstrument falls back to silence when given an empty or
/// unloadable file.  Empty result vec = valid SF2 with no playable
/// content (rare but possible — soundfonts whose first preset has no
/// linked samples).
pub fn load_sf2_file(path: &str) -> Option<Vec<SfzRegionRuntime>> {
    load_sf2_preset(path, 0)
}

/// Load a specific preset's region list from `path`.  V2 entry point —
/// the UI's preset picker calls this when the user changes selection.
/// Returns `None` for I/O / parse errors; `Some(vec![])` for a valid
/// preset with no playable content.
pub fn load_sf2_preset(path: &str, preset_idx: usize) -> Option<Vec<SfzRegionRuntime>> {
    let bytes = std::fs::read(path).ok()?;
    parse_sf2_preset_regions(&bytes, preset_idx)
}

/// Read the bank's preset list off disk — used by the UI to populate
/// the picker combo box.  Cheap; only walks the `phdr` chunk header.
pub fn load_sf2_presets(path: &str) -> Option<Vec<Sf2PresetInfo>> {
    let bytes = std::fs::read(path).ok()?;
    parse_sf2_presets(&bytes)
}

/// Walk the `phdr` chunk and return one `Sf2PresetInfo` per real
/// preset (skipping the EOP sentinel).  Pure — drives off bytes so
/// tests can exercise the multi-preset path without disk I/O.
pub fn parse_sf2_presets(bytes: &[u8]) -> Option<Vec<Sf2PresetInfo>> {
    let pdta = locate_pdta(bytes)?;
    let phdr = find_subchunk(pdta, b"phdr")?;
    // phdr is an array of 38-byte records.  The last record is the
    // "EOP" sentinel: its bagNdx bounds the previous preset's bag
    // range but it's not itself a playable preset, so we drop it.
    if phdr.len() < 38 {
        return Some(Vec::new());
    }
    let n_records = phdr.len() / 38;
    let n_presets = n_records.saturating_sub(1); // skip EOP
    let mut out = Vec::with_capacity(n_presets);
    for i in 0..n_presets {
        let off = i * 38;
        let name_bytes = &phdr[off..off + 20];
        let name = preset_name_from_bytes(name_bytes);
        let preset = u16::from_le_bytes(phdr[off + 20..off + 22].try_into().ok()?);
        let bank = u16::from_le_bytes(phdr[off + 22..off + 24].try_into().ok()?);
        out.push(Sf2PresetInfo { name, bank, preset });
    }
    Some(out)
}

/// Decode a 20-byte phdr / inst / shdr name field into a String.
/// SF2 names are zero-padded ASCII; we trim the nulls and any
/// trailing whitespace, fall back to a placeholder if the bytes
/// aren't valid UTF-8 (rare — most banks are ASCII).
fn preset_name_from_bytes(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let slice = &b[..end];
    std::str::from_utf8(slice)
        .map(|s| s.trim_end().to_string())
        .unwrap_or_else(|_| "(non-utf8 name)".to_string())
}

/// Helper — locate the `pdta` LIST body inside the top-level RIFF
/// container.  Pulled out so the preset-list and region-list parsers
/// can both use it.  Returns the inner body (without the 4cc / size
/// header).
fn locate_pdta(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"sfbk" {
        return None;
    }
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let tag = &bytes[pos..pos + 4];
        let chunk_len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        pos += 8;
        if pos + chunk_len > bytes.len() {
            return None;
        }
        if tag == b"LIST" && chunk_len >= 4 && &bytes[pos..pos + 4] == b"pdta" {
            return Some(&bytes[pos + 4..pos + chunk_len]);
        }
        pos += chunk_len;
        if chunk_len & 1 == 1 {
            pos += 1;
        }
    }
    None
}

/// Pure parser — separates the byte-walk from disk I/O so tests can
/// drive synthesised SF2 fixtures.  Returns the regions for preset 0
/// (kept as a thin wrapper for V1 callers; new code should reach for
/// `parse_sf2_preset_regions` directly).
pub fn parse_sf2_bytes(bytes: &[u8]) -> Option<Vec<SfzRegionRuntime>> {
    parse_sf2_preset_regions(bytes, 0)
}

/// Pure parser — return the runtime region list for the preset at
/// `preset_idx`.  Out-of-range index returns `None` so the caller
/// distinguishes "no such preset" from "preset has no playable
/// regions" (the latter returns `Some(vec![])`).
pub fn parse_sf2_preset_regions(bytes: &[u8], preset_idx: usize) -> Option<Vec<SfzRegionRuntime>> {
    // Top-level RIFF: "RIFF" <size:u32 LE> "sfbk" <chunks...>
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"sfbk" {
        return None;
    }
    // Walk the inner LIST chunks (INFO / sdta / pdta) one level deep.
    let mut smpl: Option<&[u8]> = None;
    let mut pdta: Option<&[u8]> = None;
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let tag = &bytes[pos..pos + 4];
        let chunk_len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        pos += 8;
        if pos + chunk_len > bytes.len() {
            return None;
        }
        if tag == b"LIST" && chunk_len >= 4 {
            let list_kind = &bytes[pos..pos + 4];
            let list_body = &bytes[pos + 4..pos + chunk_len];
            match list_kind {
                b"sdta" => {
                    smpl = find_subchunk(list_body, b"smpl");
                }
                b"pdta" => {
                    pdta = Some(list_body);
                }
                _ => {}
            }
        }
        pos += chunk_len;
        // RIFF chunks are 2-aligned; skip the pad byte if size was odd.
        if chunk_len & 1 == 1 {
            pos += 1;
        }
    }
    let smpl = smpl?;
    let pdta = pdta?;

    // Parse the pdta sub-chunks we need.
    let phdr = find_subchunk(pdta, b"phdr")?;
    let pbag = find_subchunk(pdta, b"pbag")?;
    let pgen = find_subchunk(pdta, b"pgen")?;
    let inst = find_subchunk(pdta, b"inst")?;
    let ibag = find_subchunk(pdta, b"ibag")?;
    let igen = find_subchunk(pdta, b"igen")?;
    let shdr = find_subchunk(pdta, b"shdr")?;

    // Walk preset_idx's bags.  phdr records are 38 bytes; the last
    // record is a sentinel "EOP" entry whose `presetBagNdx` gives
    // the upper bound for the previous preset's bags, so we need
    // `(preset_idx + 2)` records to bound this preset's range.
    let needed_records = preset_idx.checked_add(2).and_then(|n| n.checked_mul(38))?;
    if phdr.len() < needed_records {
        return None;
    }
    let start_off = preset_idx * 38 + 24;
    let end_off = (preset_idx + 1) * 38 + 24;
    let preset_bag_start =
        u16::from_le_bytes(phdr[start_off..start_off + 2].try_into().ok()?) as usize;
    let preset_bag_end = u16::from_le_bytes(phdr[end_off..end_off + 2].try_into().ok()?) as usize;

    let mut regions = Vec::new();
    let mut sample_cache: std::collections::HashMap<u16, Arc<Vec<f32>>> =
        std::collections::HashMap::new();
    // SF2 has a "global zone" convention: a zone with no instrument
    // generator at the end carries default generators for every
    // subsequent zone in the same preset.  We accumulate into
    // `preset_global` and apply when a zone resolves an instrument.
    let mut preset_global = Generators::default();
    for bag_idx in preset_bag_start..preset_bag_end {
        let bag_off = bag_idx * 4;
        if bag_off + 4 > pbag.len() {
            break;
        }
        let gen_start = u16::from_le_bytes(pbag[bag_off..bag_off + 2].try_into().ok()?) as usize;
        // The next bag's genNdx bounds this zone's generators.
        let next_bag_off = bag_off + 4;
        let gen_end = if next_bag_off + 2 <= pbag.len() {
            u16::from_le_bytes(pbag[next_bag_off..next_bag_off + 2].try_into().ok()?) as usize
        } else {
            pgen.len() / 4
        };
        let mut zone_gens = preset_global.clone();
        let mut instrument_idx: Option<u16> = None;
        for g in gen_start..gen_end {
            let go = g * 4;
            if go + 4 > pgen.len() {
                break;
            }
            let oper = u16::from_le_bytes(pgen[go..go + 2].try_into().ok()?);
            let amount = &pgen[go + 2..go + 4];
            if oper == GEN_INSTRUMENT {
                instrument_idx = Some(u16::from_le_bytes(amount.try_into().ok()?));
            } else {
                zone_gens.absorb(oper, amount);
            }
        }
        match instrument_idx {
            None => {
                // Global preset zone (no instrument link) — its gens
                // become the new defaults for subsequent zones.
                preset_global = zone_gens;
            }
            Some(idx) => {
                walk_instrument(
                    idx,
                    &zone_gens,
                    inst,
                    ibag,
                    igen,
                    shdr,
                    smpl,
                    &mut sample_cache,
                    &mut regions,
                );
            }
        }
    }
    Some(regions)
}

// ─── Generator opcode constants ──────────────────────────────────────────────
const GEN_PAN: u16 = 17;
const GEN_KEYRANGE: u16 = 43;
const GEN_VELRANGE: u16 = 44;
const GEN_INSTRUMENT: u16 = 41;
const GEN_SAMPLE_ID: u16 = 53;
const GEN_OVERRIDING_ROOT_KEY: u16 = 58;
const GEN_COARSE_TUNE: u16 = 51;
const GEN_FINE_TUNE: u16 = 52;
const GEN_INITIAL_ATTENUATION: u16 = 48;

/// Generator accumulator — collects every relevant generator value as
/// we walk a zone, then `materialise` builds the final SfzRegion at
/// instrument-zone time.  Defaults match the SF2 spec defaults so a
/// zone with zero generators still produces a valid region.
#[derive(Clone, Debug)]
struct Generators {
    lokey: u8,
    hikey: u8,
    lovel: u8,
    hivel: u8,
    /// `None` means "use the sample header's originalPitch".
    overriding_root: Option<u8>,
    coarse_tune_st: i16,
    fine_tune_cents: i16,
    pan_per_thousand: i16,       // SF2 pan is -500..+500 = -100..+100 %
    initial_attenuation_cb: i16, // 0.1 dB units; positive = attenuation
}

impl Default for Generators {
    fn default() -> Self {
        Self {
            lokey: 0,
            hikey: 127,
            lovel: 0,
            hivel: 127,
            overriding_root: None,
            coarse_tune_st: 0,
            fine_tune_cents: 0,
            pan_per_thousand: 0,
            initial_attenuation_cb: 0,
        }
    }
}

impl Generators {
    fn absorb(&mut self, oper: u16, amount: &[u8]) {
        match oper {
            GEN_KEYRANGE if amount.len() == 2 => {
                self.lokey = amount[0];
                self.hikey = amount[1];
            }
            GEN_VELRANGE if amount.len() == 2 => {
                self.lovel = amount[0];
                self.hivel = amount[1];
            }
            GEN_OVERRIDING_ROOT_KEY => {
                if let Ok(b) = amount.try_into() {
                    let v = i16::from_le_bytes(b);
                    // Spec: -1 means "use sample originalPitch".
                    if (0..=127).contains(&v) {
                        self.overriding_root = Some(v as u8);
                    }
                }
            }
            GEN_COARSE_TUNE => {
                if let Ok(b) = amount.try_into() {
                    self.coarse_tune_st = i16::from_le_bytes(b);
                }
            }
            GEN_FINE_TUNE => {
                if let Ok(b) = amount.try_into() {
                    self.fine_tune_cents = i16::from_le_bytes(b);
                }
            }
            GEN_PAN => {
                if let Ok(b) = amount.try_into() {
                    self.pan_per_thousand = i16::from_le_bytes(b);
                }
            }
            GEN_INITIAL_ATTENUATION => {
                if let Ok(b) = amount.try_into() {
                    self.initial_attenuation_cb = i16::from_le_bytes(b);
                }
            }
            _ => {}
        }
    }

    // (No explicit `layer()` helper — the walk uses clone + absorb to
    // accumulate generators per zone.  preset_global → preset zone →
    // walk_instrument(preset_zone) → inst zone is the sequence; each
    // step clones the previous Generators and absorbs new overrides
    // on top.  Cleaner than a separate merge fn would be.)
}

#[allow(clippy::too_many_arguments)]
fn walk_instrument(
    inst_idx: u16,
    preset_zone: &Generators,
    inst: &[u8],
    ibag: &[u8],
    igen: &[u8],
    shdr: &[u8],
    smpl: &[u8],
    sample_cache: &mut std::collections::HashMap<u16, Arc<Vec<f32>>>,
    regions: &mut Vec<SfzRegionRuntime>,
) {
    // inst record: 22 bytes (name[20], instBagNdx u16).
    let off = inst_idx as usize * 22;
    if off + 22 > inst.len() {
        return;
    }
    let bag_start = match inst[off + 20..off + 22].try_into() {
        Ok(b) => u16::from_le_bytes(b) as usize,
        Err(_) => return,
    };
    let next_off = off + 22;
    let bag_end = if next_off + 22 <= inst.len() {
        match inst[next_off + 20..next_off + 22].try_into() {
            Ok(b) => u16::from_le_bytes(b) as usize,
            Err(_) => return,
        }
    } else {
        ibag.len() / 4
    };
    let mut inst_global = preset_zone.clone();
    for bag_idx in bag_start..bag_end {
        let bag_off = bag_idx * 4;
        if bag_off + 4 > ibag.len() {
            break;
        }
        let gen_start = match ibag[bag_off..bag_off + 2].try_into() {
            Ok(b) => u16::from_le_bytes(b) as usize,
            Err(_) => break,
        };
        let next_bag_off = bag_off + 4;
        let gen_end = if next_bag_off + 2 <= ibag.len() {
            match ibag[next_bag_off..next_bag_off + 2].try_into() {
                Ok(b) => u16::from_le_bytes(b) as usize,
                Err(_) => break,
            }
        } else {
            igen.len() / 4
        };
        let mut zone_gens = inst_global.clone();
        let mut sample_idx: Option<u16> = None;
        for g in gen_start..gen_end {
            let go = g * 4;
            if go + 4 > igen.len() {
                break;
            }
            let oper = match igen[go..go + 2].try_into() {
                Ok(b) => u16::from_le_bytes(b),
                Err(_) => break,
            };
            let amount = &igen[go + 2..go + 4];
            if oper == GEN_SAMPLE_ID {
                sample_idx = amount.try_into().ok().map(u16::from_le_bytes);
            } else {
                zone_gens.absorb(oper, amount);
            }
        }
        match sample_idx {
            None => {
                inst_global = zone_gens;
            }
            Some(idx) => {
                if let Some(rt) = build_region(idx, &zone_gens, shdr, smpl, sample_cache) {
                    regions.push(rt);
                }
            }
        }
    }
}

fn build_region(
    sample_idx: u16,
    gens: &Generators,
    shdr: &[u8],
    smpl: &[u8],
    sample_cache: &mut std::collections::HashMap<u16, Arc<Vec<f32>>>,
) -> Option<SfzRegionRuntime> {
    // shdr record: 46 bytes.
    //   name[20] | start u32 | end u32 | loop_start u32 | loop_end u32
    //   sample_rate u32 | original_pitch u8 | pitch_correction i8
    //   sample_link u16 | sample_type u16
    let off = sample_idx as usize * 46;
    if off + 46 > shdr.len() {
        return None;
    }
    let start = u32::from_le_bytes(shdr[off + 20..off + 24].try_into().ok()?);
    let end = u32::from_le_bytes(shdr[off + 24..off + 28].try_into().ok()?);
    let loop_start = u32::from_le_bytes(shdr[off + 28..off + 32].try_into().ok()?);
    let loop_end = u32::from_le_bytes(shdr[off + 32..off + 36].try_into().ok()?);
    let sample_rate = u32::from_le_bytes(shdr[off + 36..off + 40].try_into().ok()?);
    let original_pitch = shdr[off + 40];
    let pitch_correction = shdr[off + 41] as i8;
    if end <= start {
        return None;
    }

    // Slice the smpl chunk: each sample is 16-bit signed PCM.
    let byte_start = start as usize * 2;
    let byte_end = end as usize * 2;
    if byte_end > smpl.len() {
        return None;
    }
    let raw = &smpl[byte_start..byte_end];

    // Cache decoded + resampled audio per sample_idx so multi-zone
    // SF2s (one sample mapped across many key zones) only decode once.
    let arc = if let Some(a) = sample_cache.get(&sample_idx) {
        a.clone()
    } else {
        let mut mono: Vec<f32> = Vec::with_capacity(raw.len() / 2);
        for chunk in raw.chunks_exact(2) {
            let s = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
            mono.push(s);
        }
        let resampled = if sample_rate == SAMPLE_RATE_HZ {
            mono
        } else {
            resample_mono(&mono, sample_rate)
        };
        let arc = Arc::new(resampled);
        sample_cache.insert(sample_idx, arc.clone());
        arc
    };

    // Pick the root note: explicit override > sample's originalPitch.
    let root = gens.overriding_root.unwrap_or(original_pitch).min(127);

    // Build the SfzRegion.  Loop fields are passed through so a
    // future loop-aware DSP path can use them; the current
    // SampleInstrument always loops, which matches the SF2 default
    // for tonal samples.
    let mut region = SfzRegion {
        sample_path: std::path::PathBuf::from(format!("«sf2:sample{sample_idx}»")),
        lokey: gens.lokey,
        hikey: gens.hikey.max(gens.lokey),
        pitch_keycenter: root,
        lovel: gens.lovel,
        hivel: gens.hivel.max(gens.lovel),
        loop_start: Some(loop_start.saturating_sub(start)),
        loop_end: Some(loop_end.saturating_sub(start)),
        // Convert SF2 attenuation (centibels, 0.1 dB) to volume_db.
        // SF2 attenuation is positive = quieter, so we flip sign.
        volume_db: -(gens.initial_attenuation_cb as f32) / 10.0,
        // SF2 pan is -500..+500 (per-thousand, i.e. -50.0..+50.0%).
        // SfzRegion.pan is -100..+100, so scale ×0.2.
        pan: (gens.pan_per_thousand as f32) * 0.2,
        tune_cents: (gens.fine_tune_cents as f32 + pitch_correction as f32).clamp(-100.0, 100.0),
        transpose: (gens.coarse_tune_st as i32).clamp(-127, 127) as i8,
        ..SfzRegion::default()
    };
    // Defensive clamps on the key range — degenerate SF2s sometimes
    // ship hi < lo, which would silently match nothing.
    if region.hikey < region.lokey {
        region.hikey = region.lokey;
    }

    Some(SfzRegionRuntime {
        region,
        samples: arc,
    })
}

/// Linear-interp resample a mono buffer to the engine rate.  Same
/// algorithm `audio_load::resample_mono_linear` uses; duplicated here
/// rather than re-exported because the call site is internal to the
/// SF2 loader and the helper would otherwise need a `pub(crate)`
/// promotion.
fn resample_mono(mono: &[f32], src_rate: u32) -> Vec<f32> {
    if src_rate == SAMPLE_RATE_HZ {
        return mono.to_vec();
    }
    let ratio = src_rate as f32 / SAMPLE_RATE;
    let new_len = (mono.len() as f32 / ratio) as usize;
    (0..new_len)
        .map(|i| {
            let src = i as f32 * ratio;
            let idx = src as usize;
            let frac = src - idx as f32;
            let a = mono.get(idx).copied().unwrap_or(0.0);
            let b = mono.get(idx + 1).copied().unwrap_or(0.0);
            a + (b - a) * frac
        })
        .collect()
}

/// Find a sub-chunk inside a RIFF list body.  Returns the chunk's
/// payload bytes (without the 8-byte tag/size header) or `None` when
/// the chunk isn't present.  Same shape as the WAV parser's chunk
/// walker — RIFF is RIFF.
fn find_subchunk<'a>(body: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let mut pos = 0;
    while pos + 8 <= body.len() {
        let here = &body[pos..pos + 4];
        let len = u32::from_le_bytes(body[pos + 4..pos + 8].try_into().ok()?) as usize;
        let payload_start = pos + 8;
        if payload_start + len > body.len() {
            return None;
        }
        if here == tag {
            return Some(&body[payload_start..payload_start + len]);
        }
        pos = payload_start + len;
        if len & 1 == 1 {
            pos += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the smallest possible valid SF2 in memory: one preset
    /// pointing at one instrument pointing at one sample (a single
    /// key-mapped zone covering MIDI 60).
    ///
    /// Layout: RIFF header → LIST sdta → smpl payload → LIST pdta →
    /// phdr / pbag / pgen / inst / ibag / igen / shdr.  Indices are
    /// self-consistent so the parser walks a single-region preset
    /// without tripping on the EOP sentinels.
    fn fixture_minimal_sf2() -> Vec<u8> {
        // ── smpl payload — 32 i16 frames at 48 kHz ──
        let mut smpl_payload = Vec::new();
        for i in 0..32_i16 {
            smpl_payload.extend_from_slice(&i.to_le_bytes());
        }
        // ── pdta sub-chunks ──
        // phdr: preset 0 (bagNdx 0) + EOP sentinel (bagNdx 1).
        let mut phdr = Vec::new();
        phdr.extend_from_slice(&[0u8; 20]); // name
        phdr.extend_from_slice(&0u16.to_le_bytes()); // preset
        phdr.extend_from_slice(&0u16.to_le_bytes()); // bank
        phdr.extend_from_slice(&0u16.to_le_bytes()); // bagNdx
        phdr.extend_from_slice(&[0u8; 12]); // library / genre / morph
        // EOP
        phdr.extend_from_slice(&[0u8; 20]);
        phdr.extend_from_slice(&0u16.to_le_bytes());
        phdr.extend_from_slice(&0u16.to_le_bytes());
        phdr.extend_from_slice(&1u16.to_le_bytes()); // bagNdx for EOP
        phdr.extend_from_slice(&[0u8; 12]);

        // pbag: bag 0 → genNdx 0; sentinel → genNdx 1.
        let mut pbag = Vec::new();
        pbag.extend_from_slice(&0u16.to_le_bytes()); // genNdx
        pbag.extend_from_slice(&0u16.to_le_bytes()); // modNdx
        pbag.extend_from_slice(&1u16.to_le_bytes()); // sentinel genNdx
        pbag.extend_from_slice(&0u16.to_le_bytes());

        // pgen: gen 0 = INSTRUMENT(0); sentinel pad.
        let mut pgen = Vec::new();
        pgen.extend_from_slice(&GEN_INSTRUMENT.to_le_bytes());
        pgen.extend_from_slice(&0u16.to_le_bytes());
        // Sentinel (2 bytes oper, 2 bytes amount); zero is a sane padding.
        pgen.extend_from_slice(&[0u8; 4]);

        // inst: instrument 0 (instBagNdx 0); EOP sentinel (instBagNdx 1).
        let mut inst = Vec::new();
        inst.extend_from_slice(&[0u8; 20]); // name
        inst.extend_from_slice(&0u16.to_le_bytes()); // bagNdx
        inst.extend_from_slice(&[0u8; 20]);
        inst.extend_from_slice(&1u16.to_le_bytes()); // EOP

        // ibag: bag 0 → genNdx 0; sentinel → genNdx 2.
        let mut ibag = Vec::new();
        ibag.extend_from_slice(&0u16.to_le_bytes());
        ibag.extend_from_slice(&0u16.to_le_bytes());
        ibag.extend_from_slice(&2u16.to_le_bytes());
        ibag.extend_from_slice(&0u16.to_le_bytes());

        // igen: gen 0 = KEYRANGE(60..72); gen 1 = SAMPLE_ID(0); + sentinel.
        let mut igen = Vec::new();
        igen.extend_from_slice(&GEN_KEYRANGE.to_le_bytes());
        // KEYRANGE amount is two u8 bytes: lo, hi.
        igen.extend_from_slice(&[60, 72]);
        igen.extend_from_slice(&GEN_SAMPLE_ID.to_le_bytes());
        igen.extend_from_slice(&0u16.to_le_bytes());
        // Sentinel.
        igen.extend_from_slice(&[0u8; 4]);

        // shdr: sample 0 covers smpl frames 0..32 at 48 kHz, root C4.
        let mut shdr = Vec::new();
        shdr.extend_from_slice(&[0u8; 20]); // name
        shdr.extend_from_slice(&0u32.to_le_bytes()); // start
        shdr.extend_from_slice(&32u32.to_le_bytes()); // end
        shdr.extend_from_slice(&0u32.to_le_bytes()); // loop_start
        shdr.extend_from_slice(&32u32.to_le_bytes()); // loop_end
        shdr.extend_from_slice(&48000u32.to_le_bytes()); // sample_rate
        shdr.push(60); // original_pitch
        shdr.push(0); // pitch_correction
        shdr.extend_from_slice(&0u16.to_le_bytes()); // sample_link
        shdr.extend_from_slice(&1u16.to_le_bytes()); // sample_type
        // EOP sample header — required by the spec.
        shdr.extend_from_slice(&[0u8; 46]);

        // ── Wrap each sub-chunk with its 4cc + length ──
        fn wrap(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(8 + payload.len());
            out.extend_from_slice(tag);
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(payload);
            if payload.len() & 1 == 1 {
                out.push(0);
            }
            out
        }
        let mut pdta = Vec::new();
        pdta.extend_from_slice(b"pdta");
        pdta.extend_from_slice(&wrap(b"phdr", &phdr));
        pdta.extend_from_slice(&wrap(b"pbag", &pbag));
        pdta.extend_from_slice(&wrap(b"pgen", &pgen));
        pdta.extend_from_slice(&wrap(b"inst", &inst));
        pdta.extend_from_slice(&wrap(b"ibag", &ibag));
        pdta.extend_from_slice(&wrap(b"igen", &igen));
        pdta.extend_from_slice(&wrap(b"shdr", &shdr));

        let mut sdta = Vec::new();
        sdta.extend_from_slice(b"sdta");
        sdta.extend_from_slice(&wrap(b"smpl", &smpl_payload));

        // ── RIFF outer wrapper ──
        let mut body = Vec::new();
        body.extend_from_slice(b"sfbk");
        body.extend_from_slice(b"LIST");
        body.extend_from_slice(&(sdta.len() as u32).to_le_bytes());
        body.extend_from_slice(&sdta);
        body.extend_from_slice(b"LIST");
        body.extend_from_slice(&(pdta.len() as u32).to_le_bytes());
        body.extend_from_slice(&pdta);

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn parse_minimal_sf2_yields_one_region() {
        let bytes = fixture_minimal_sf2();
        let regions = parse_sf2_bytes(&bytes).expect("parse");
        assert_eq!(regions.len(), 1, "minimal SF2 has exactly one region");
        let r = &regions[0].region;
        assert_eq!(r.lokey, 60);
        assert_eq!(r.hikey, 72);
        assert_eq!(r.pitch_keycenter, 60);
        assert_eq!(r.lovel, 0);
        assert_eq!(r.hivel, 127);
        // Sample buffer: 32 frames at 48 kHz → 32 samples at engine rate.
        assert_eq!(regions[0].samples.len(), 32);
        // First sample is i16 0 → 0.0; sample[8] is i16 8 → 8/32768.
        assert!((regions[0].samples[0] - 0.0).abs() < 1e-6);
        assert!((regions[0].samples[8] - 8.0 / 32768.0).abs() < 1e-5);
    }

    #[test]
    fn parse_sf2_rejects_bad_magic() {
        let bytes = b"not an sf2 file at all".to_vec();
        assert!(parse_sf2_bytes(&bytes).is_none());
    }

    #[test]
    fn parse_sf2_rejects_truncated_input() {
        let mut bytes = fixture_minimal_sf2();
        bytes.truncate(20);
        assert!(parse_sf2_bytes(&bytes).is_none());
    }

    #[test]
    fn generators_absorb_keyrange_and_tune() {
        let mut g = Generators::default();
        // KEYRANGE amount is two u8s.
        g.absorb(GEN_KEYRANGE, &[36, 96]);
        assert_eq!(g.lokey, 36);
        assert_eq!(g.hikey, 96);
        // Coarse tune amount is i16 LE.
        g.absorb(GEN_COARSE_TUNE, &(-12_i16).to_le_bytes());
        assert_eq!(g.coarse_tune_st, -12);
        // Overriding root: u8 in spec but stored in the i16 amount field.
        g.absorb(GEN_OVERRIDING_ROOT_KEY, &69_i16.to_le_bytes());
        assert_eq!(g.overriding_root, Some(69));
        // -1 sentinel = "use sample originalPitch" → leaves field as-is.
        g.absorb(GEN_OVERRIDING_ROOT_KEY, &(-1_i16).to_le_bytes());
        assert_eq!(g.overriding_root, Some(69));
    }

    #[test]
    fn parse_sf2_presets_returns_one_entry_for_minimal_fixture() {
        // The minimal fixture has exactly one real preset + the EOP
        // sentinel.  Listing should report one entry, dropping the
        // sentinel.
        let bytes = fixture_minimal_sf2();
        let presets = parse_sf2_presets(&bytes).expect("preset list");
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].preset, 0);
        assert_eq!(presets[0].bank, 0);
    }

    #[test]
    fn parse_sf2_preset_regions_with_idx_zero_matches_legacy_call() {
        // The new preset-indexed entry point at idx=0 should produce
        // the same regions as the legacy `parse_sf2_bytes` wrapper.
        let bytes = fixture_minimal_sf2();
        let by_legacy = parse_sf2_bytes(&bytes).expect("legacy");
        let by_idx = parse_sf2_preset_regions(&bytes, 0).expect("indexed");
        assert_eq!(by_legacy.len(), by_idx.len());
        assert_eq!(by_legacy[0].region.lokey, by_idx[0].region.lokey);
        assert_eq!(by_legacy[0].region.hikey, by_idx[0].region.hikey);
    }

    #[test]
    fn parse_sf2_preset_regions_out_of_range_returns_none() {
        // Asking for preset 99 on a single-preset fixture should
        // bail with None — the caller (UI) treats that as "preset
        // index out of range" and re-clamps the picker.
        let bytes = fixture_minimal_sf2();
        assert!(parse_sf2_preset_regions(&bytes, 99).is_none());
    }

    #[test]
    fn preset_name_decodes_trimmed_ascii() {
        // Standard zero-padded ASCII — trailing nulls trimmed.
        let bytes = b"Grand Piano\0\0\0\0\0\0\0\0\0";
        assert_eq!(preset_name_from_bytes(bytes), "Grand Piano");
        // Empty / all-null name decodes to empty string rather than
        // bubbling up an error.
        let bytes = b"\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
        assert_eq!(preset_name_from_bytes(bytes), "");
    }
}
