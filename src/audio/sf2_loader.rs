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

use crate::audio::SAMPLE_RATE_HZ;
use crate::audio::audio_load::{i16_pcm_to_f32, resample_mono_linear};
use crate::audio::dsp::sample_instrument::SfzRegionRuntime;
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

// Generator opcodes + the `Generators` accumulator live in
// `sf2_generators.rs` (sibling) since this file crossed the
// 1000-line cap during the SF2 LFO ship.
use super::sf2_generators::*;

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
            let s = i16_pcm_to_f32(i16::from_le_bytes([chunk[0], chunk[1]]));
            mono.push(s);
        }
        let resampled = if sample_rate == SAMPLE_RATE_HZ {
            mono
        } else {
            resample_mono_linear(&mono, sample_rate)
        };
        let arc = Arc::new(resampled);
        sample_cache.insert(sample_idx, arc.clone());
        arc
    };

    // Pick the root note: explicit override > sample's originalPitch.
    let root = gens.overriding_root.unwrap_or(original_pitch).min(127);

    // Volume-envelope conversion.  SF2 stores times as timecents
    // (`secs = 2^(tc / 1200)`) and sustain as centibels of
    // attenuation from peak.  Spec defaults (-12000 tc → ~0.001 s,
    // 0 cB → full sustain) read as "no envelope" so we treat them
    // identically to "generator absent" — leaves SfzRegion at its
    // own defaults (0 / 100 %), which the DSP overrides path
    // interprets as "fall through to global knob".
    let attack_s = sf2_timecents_to_secs(gens.attack_vol_env_tc);
    let decay_s = sf2_timecents_to_secs(gens.decay_vol_env_tc);
    let release_s = sf2_timecents_to_secs(gens.release_vol_env_tc);
    let sustain_pct = match gens.sustain_vol_env_cb {
        // Sustain attenuation in dB = cB / 10.  Amplitude ratio =
        // 10^(-dB / 20) = 10^(-cB / 200).  Convert to 0..100 %.
        Some(cb) if cb != 0 => (10.0_f32.powf(-(cb as f32) / 200.0) * 100.0).clamp(0.0, 100.0),
        _ => 100.0, // generator absent or 0 cB → full sustain
    };
    // Filter generators — `initialFilterFc` in absolute cents,
    // `initialFilterQ` in centibels of resonance peak gain.  SF2
    // spec default is 13500 cents (~20 kHz, "filter off") and
    // 0 cB (Q = 1.0); we collapse those back to "no filter" so
    // the SfzRegion contract matches what an SFZ author would
    // write when they don't want a filter at all.
    let (cutoff_hz, resonance_db, fil_type) = match gens.initial_filter_fc_cents {
        Some(cents) if cents < 13500 => {
            // 8.176 Hz × 2^(cents / 1200) — same formula `state::sfz`
            // would use if SFZ ever supported absolute-cents cutoff.
            let hz = 8.176_f32 * 2.0_f32.powf(cents as f32 / 1200.0);
            // Resonance: SF2 cB → linear Q → dB ≈ cB / 10.
            // SfzRegion stores resonance_db, the DSP override path
            // converts dB → 0..1 knob, so just pass the dB value.
            let resonance_db = (gens.initial_filter_q_cb.unwrap_or(0) as f32) / 10.0;
            (
                hz.clamp(20.0, 20_000.0),
                resonance_db.clamp(0.0, 40.0),
                Some(crate::state::sfz::SfzFilType::Lpf2p),
            )
        }
        _ => (0.0, 0.0, None),
    };

    // sampleModes (gen 54) → SfzLoopMode: 1=Continuous, 3=Sustain,
    // 0/2/absent=NoLoop.  Audio path consumes loop_mode + the
    // SHDR-supplied loop window.
    let loop_mode = match gens.sample_modes {
        Some(1) => crate::state::sfz::SfzLoopMode::LoopContinuous,
        Some(3) => crate::state::sfz::SfzLoopMode::LoopSustain,
        _ => crate::state::sfz::SfzLoopMode::NoLoop,
    };

    // Modulation + vibrato LFO frequencies — absolute cents converted
    // to Hz via 8.176 × 2^(cents / 1200).  Spec defaults (0 cents =
    // 8.176 Hz) apply when the generator is absent.
    let mod_lfo_freq_hz = match gens.freq_mod_lfo_cents {
        Some(c) => 8.176_f32 * 2.0_f32.powf(c as f32 / 1200.0),
        None => 8.176,
    };
    let vib_lfo_freq_hz = match gens.freq_vib_lfo_cents {
        Some(c) => 8.176_f32 * 2.0_f32.powf(c as f32 / 1200.0),
        None => 8.176,
    };
    // Delays — timecents (None = -12000 ≈ "instant" → 0 s).
    let mod_lfo_delay_s = sf2_timecents_to_secs(gens.delay_mod_lfo_tc);
    let vib_lfo_delay_s = sf2_timecents_to_secs(gens.delay_vib_lfo_tc);

    // Modulation envelope — convert timecents to seconds (None /
    // -12000 ≈ "instant" → 0 s) and 0.1 % attenuation units to a
    // linear 0..1 sustain level.  `sustainModEnv` of 0 (or absent)
    // means "full level" = sustain at 1.0; 1000 = full attenuation;
    // values in between map linearly via `1 - n/1000`.
    let mod_env_delay_s = sf2_timecents_to_secs(gens.delay_mod_env_tc);
    let mod_env_attack_s = sf2_timecents_to_secs(gens.attack_mod_env_tc);
    let mod_env_hold_s = sf2_timecents_to_secs(gens.hold_mod_env_tc);
    let mod_env_decay_s = sf2_timecents_to_secs(gens.decay_mod_env_tc);
    let mod_env_release_s = sf2_timecents_to_secs(gens.release_mod_env_tc);
    let mod_env_sustain_level = match gens.sustain_mod_env_thousandths {
        Some(n) if n > 0 => (1.0 - (n as f32) / 1000.0).clamp(0.0, 1.0),
        _ => 1.0,
    };

    let mut region = SfzRegion {
        sample_path: std::path::PathBuf::from(format!("«sf2:sample{sample_idx}»")),
        lokey: gens.lokey,
        hikey: gens.hikey.max(gens.lokey),
        pitch_keycenter: root,
        lovel: gens.lovel,
        hivel: gens.hivel.max(gens.lovel),
        loop_mode,
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
        ampeg_attack_s: attack_s,
        ampeg_decay_s: decay_s,
        ampeg_sustain_pct: sustain_pct,
        ampeg_release_s: release_s,
        cutoff_hz,
        resonance_db,
        fil_type,
        mod_lfo_freq_hz,
        mod_lfo_delay_s,
        mod_lfo_to_pitch_cents: gens.mod_lfo_to_pitch_cents.unwrap_or(0) as f32,
        mod_lfo_to_filter_fc_cents: gens.mod_lfo_to_filter_fc_cents.unwrap_or(0) as f32,
        mod_lfo_to_volume_cb: gens.mod_lfo_to_volume_cb.unwrap_or(0) as f32,
        vib_lfo_freq_hz,
        vib_lfo_delay_s,
        vib_lfo_to_pitch_cents: gens.vib_lfo_to_pitch_cents.unwrap_or(0) as f32,
        mod_env_delay_s,
        mod_env_attack_s,
        mod_env_hold_s,
        mod_env_decay_s,
        mod_env_sustain_level,
        mod_env_release_s,
        mod_env_to_pitch_cents: gens.mod_env_to_pitch_cents.unwrap_or(0) as f32,
        mod_env_to_filter_fc_cents: gens.mod_env_to_filter_fc_cents.unwrap_or(0) as f32,
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

/// Convert an optional SF2 timecents value into seconds for the
/// `ampeg_*_s` fields.  Returns 0.0 when the generator is absent or
/// at its spec-default sentinel (-12000 ≈ 1 ms = "instant"), which
/// the DSP-side override path interprets as "fall through to the
/// global knob".  Real envelope values are always > -12000.
fn sf2_timecents_to_secs(tc: Option<i16>) -> f32 {
    match tc {
        Some(v) if v > -12000 => 2.0_f32.powf(v as f32 / 1200.0),
        _ => 0.0,
    }
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

    #[test]
    fn timecents_default_returns_zero() {
        // Generator absent → 0 (DSP falls through to global knob).
        assert_eq!(sf2_timecents_to_secs(None), 0.0);
        // Spec default sentinel (-12000) → 0 (no envelope).
        assert_eq!(sf2_timecents_to_secs(Some(-12000)), 0.0);
    }

    #[test]
    fn timecents_to_seconds_round_trips_known_values() {
        // 0 timecents = 1 second.
        assert!((sf2_timecents_to_secs(Some(0)) - 1.0).abs() < 1e-4);
        // 1200 timecents = 2 seconds.
        assert!((sf2_timecents_to_secs(Some(1200)) - 2.0).abs() < 1e-4);
        // -1200 timecents = 0.5 seconds.
        assert!((sf2_timecents_to_secs(Some(-1200)) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn generators_absorb_volume_envelope() {
        let mut g = Generators::default();
        // Attack 100 ms ≈ 2^(tc/1200) = 0.1 → tc = 1200 * log2(0.1) ≈ -3986.
        g.absorb(GEN_ATTACK_VOL_ENV, &(-3986_i16).to_le_bytes());
        g.absorb(GEN_DECAY_VOL_ENV, &(-2400_i16).to_le_bytes());
        g.absorb(GEN_SUSTAIN_VOL_ENV, &200_i16.to_le_bytes()); // 20 dB attenuation
        g.absorb(GEN_RELEASE_VOL_ENV, &0_i16.to_le_bytes()); // 1 second
        assert_eq!(g.attack_vol_env_tc, Some(-3986));
        assert_eq!(g.decay_vol_env_tc, Some(-2400));
        assert_eq!(g.sustain_vol_env_cb, Some(200));
        assert_eq!(g.release_vol_env_tc, Some(0));
    }

    #[test]
    fn generators_absorb_filter() {
        let mut g = Generators::default();
        // Cutoff 1 kHz: cents = 1200 × log2(1000 / 8.176) ≈ 8302.
        g.absorb(GEN_INITIAL_FILTER_FC, &8302_i16.to_le_bytes());
        // Q 12 dB peak = 120 cB.
        g.absorb(GEN_INITIAL_FILTER_Q, &120_i16.to_le_bytes());
        assert_eq!(g.initial_filter_fc_cents, Some(8302));
        assert_eq!(g.initial_filter_q_cb, Some(120));
    }

    /// `sampleModes` (gen 54) covers the SF2 loop-mode bit: 0/2 = no
    /// loop, 1 = loop continuously, 3 = loop until release.
    #[test]
    fn generators_absorb_sample_modes() {
        let mut g = Generators::default();
        g.absorb(GEN_SAMPLE_MODES, &1u16.to_le_bytes());
        assert_eq!(g.sample_modes, Some(1));
        g.absorb(GEN_SAMPLE_MODES, &3u16.to_le_bytes());
        assert_eq!(g.sample_modes, Some(3));
    }

    /// LFO timing + pitch-depth generators round-trip through absorb().
    /// Frequency is absolute cents (8.176 × 2^(cents/1200)); delay is
    /// timecents; depth is cents.
    #[test]
    fn generators_absorb_lfo_pitch_depths() {
        let mut g = Generators::default();
        g.absorb(GEN_FREQ_MOD_LFO, &(-700_i16).to_le_bytes()); // ~5 Hz
        g.absorb(GEN_DELAY_MOD_LFO, &(-3863_i16).to_le_bytes()); // ~100 ms
        g.absorb(GEN_MOD_LFO_TO_PITCH, &50_i16.to_le_bytes()); // ±50 cents
        g.absorb(GEN_FREQ_VIB_LFO, &0_i16.to_le_bytes()); // 8.176 Hz default
        g.absorb(GEN_DELAY_VIB_LFO, &(-7973_i16).to_le_bytes()); // ~10 ms
        g.absorb(GEN_VIB_LFO_TO_PITCH, &30_i16.to_le_bytes()); // ±30 cents
        assert_eq!(g.freq_mod_lfo_cents, Some(-700));
        assert_eq!(g.delay_mod_lfo_tc, Some(-3863));
        assert_eq!(g.mod_lfo_to_pitch_cents, Some(50));
        assert_eq!(g.freq_vib_lfo_cents, Some(0));
        assert_eq!(g.delay_vib_lfo_tc, Some(-7973));
        assert_eq!(g.vib_lfo_to_pitch_cents, Some(30));
    }

    /// modLfoToFilterFc (gen 13, cents) + modLfoToVolume (gen 14, cB)
    /// round-trip through absorb().  Confirms the loader plumbs the
    /// non-pitch mod-LFO targets into the Generators accumulator.
    #[test]
    fn generators_absorb_lfo_filter_and_volume_targets() {
        let mut g = Generators::default();
        g.absorb(GEN_MOD_LFO_TO_FILTER_FC, &2400_i16.to_le_bytes()); // ±2 oct cutoff
        g.absorb(GEN_MOD_LFO_TO_VOLUME, &100_i16.to_le_bytes()); // 10 dB swing
        assert_eq!(g.mod_lfo_to_filter_fc_cents, Some(2400));
        assert_eq!(g.mod_lfo_to_volume_cb, Some(100));
    }

    /// Modulation envelope opcodes (gen 7, 11, 25–30) — five timing
    /// fields, sustain attenuation, and two depth targets all
    /// round-trip through absorb().  Sustain is in 0.1 % units
    /// (1000 = full attenuation).
    #[test]
    fn generators_absorb_modulation_envelope() {
        let mut g = Generators::default();
        g.absorb(GEN_DELAY_MOD_ENV, &(-7973_i16).to_le_bytes()); // ~10 ms
        g.absorb(GEN_ATTACK_MOD_ENV, &(-3863_i16).to_le_bytes()); // ~100 ms
        g.absorb(GEN_HOLD_MOD_ENV, &(-2400_i16).to_le_bytes()); // ~250 ms
        g.absorb(GEN_DECAY_MOD_ENV, &(-1200_i16).to_le_bytes()); // ~500 ms
        g.absorb(GEN_SUSTAIN_MOD_ENV, &600_i16.to_le_bytes()); // 60 % attenuation
        g.absorb(GEN_RELEASE_MOD_ENV, &0_i16.to_le_bytes()); // 1 s
        g.absorb(GEN_MOD_ENV_TO_PITCH, &1200_i16.to_le_bytes()); // ±1 oct
        g.absorb(GEN_MOD_ENV_TO_FILTER_FC, &2400_i16.to_le_bytes()); // ±2 oct
        assert_eq!(g.delay_mod_env_tc, Some(-7973));
        assert_eq!(g.attack_mod_env_tc, Some(-3863));
        assert_eq!(g.hold_mod_env_tc, Some(-2400));
        assert_eq!(g.decay_mod_env_tc, Some(-1200));
        assert_eq!(g.sustain_mod_env_thousandths, Some(600));
        assert_eq!(g.release_mod_env_tc, Some(0));
        assert_eq!(g.mod_env_to_pitch_cents, Some(1200));
        assert_eq!(g.mod_env_to_filter_fc_cents, Some(2400));
    }
}
