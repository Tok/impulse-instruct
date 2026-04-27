// ─── audio/sf2_generators.rs ─────────────────────────────────────────────────
// SF2 generator opcode constants + the `Generators` accumulator struct.
// Lifted out of `sf2_loader.rs` once that file crossed the 1000-line
// cap during the SF2 LFO ship.  Sibling module — the loader uses these
// types via `use super::sf2_generators::*;`.
//
// Pattern: each generator opcode is a small u16 + an i16 / u8 amount.
// `Generators::absorb()` walks zone gen lists at parse time, layering
// values on top of the SF2 spec defaults until `build_region` reads
// the accumulated state.

// ─── Generator opcode constants ──────────────────────────────────────────────
pub(super) const GEN_PAN: u16 = 17;
pub(super) const GEN_KEYRANGE: u16 = 43;
pub(super) const GEN_VELRANGE: u16 = 44;
pub(super) const GEN_INSTRUMENT: u16 = 41;
pub(super) const GEN_SAMPLE_ID: u16 = 53;
pub(super) const GEN_OVERRIDING_ROOT_KEY: u16 = 58;
pub(super) const GEN_COARSE_TUNE: u16 = 51;
pub(super) const GEN_FINE_TUNE: u16 = 52;
pub(super) const GEN_INITIAL_ATTENUATION: u16 = 48;
// Volume envelope generators (timecents for times,
// centibels of attenuation for sustain).
pub(super) const GEN_ATTACK_VOL_ENV: u16 = 34;
pub(super) const GEN_DECAY_VOL_ENV: u16 = 36;
pub(super) const GEN_SUSTAIN_VOL_ENV: u16 = 37;
pub(super) const GEN_RELEASE_VOL_ENV: u16 = 38;
// Filter generators.  initialFilterFc is in absolute cents
// (hz = 8.176 × 2^(cents/1200)); initialFilterQ is in centibels of
// resonance peak gain (Q_linear = 10^(cB / 200)).
pub(super) const GEN_INITIAL_FILTER_FC: u16 = 8;
pub(super) const GEN_INITIAL_FILTER_Q: u16 = 9;

/// Generator 54 — `sampleModes`.  Bitfield (bits 0–1):
///   0 = no loop, 1 = loop continuously, 2 = no loop (alt encoding),
///   3 = loop while gate held then play release tail.
pub(super) const GEN_SAMPLE_MODES: u16 = 54;

/// Modulation LFO + vibrato LFO opcodes.  V1 wires only the pitch
/// targets; modLfoToFilterFc / modLfoToVolume / modEnvToPitch /
/// modEnvToFilterFc are deferred to a follow-up.
pub(super) const GEN_DELAY_MOD_LFO: u16 = 21; // timecents
pub(super) const GEN_FREQ_MOD_LFO: u16 = 22; // absolute cents (8.176 × 2^(cents/1200))
pub(super) const GEN_DELAY_VIB_LFO: u16 = 23;
pub(super) const GEN_FREQ_VIB_LFO: u16 = 24;
pub(super) const GEN_MOD_LFO_TO_PITCH: u16 = 5; // cents
pub(super) const GEN_VIB_LFO_TO_PITCH: u16 = 6; // cents

/// Generator accumulator — collects every relevant generator value as
/// the walk traverses a zone, then `build_region` (in `sf2_loader.rs`)
/// builds the final SfzRegion at instrument-zone time.  Defaults
/// match the SF2 spec defaults so a zone with zero generators still
/// produces a valid region.
#[derive(Clone, Debug)]
pub(super) struct Generators {
    pub(super) lokey: u8,
    pub(super) hikey: u8,
    pub(super) lovel: u8,
    pub(super) hivel: u8,
    /// `None` means "use the sample header's originalPitch".
    pub(super) overriding_root: Option<u8>,
    pub(super) coarse_tune_st: i16,
    pub(super) fine_tune_cents: i16,
    pub(super) pan_per_thousand: i16, // SF2 pan is -500..+500 = -100..+100 %
    pub(super) initial_attenuation_cb: i16, // 0.1 dB units; positive = attenuation
    /// Volume envelope generators in their native SF2 units.  `None`
    /// = generator absent (the spec default of -12000 timecents = ~1
    /// ms = "instant" applies).  Times are timecents
    /// (`secs = 2^(tc / 1200)`); sustain is centibels of attenuation
    /// from peak (0 cB = full sustain, 1000 cB = 10 dB attenuation).
    pub(super) attack_vol_env_tc: Option<i16>,
    pub(super) decay_vol_env_tc: Option<i16>,
    pub(super) sustain_vol_env_cb: Option<i16>,
    pub(super) release_vol_env_tc: Option<i16>,
    /// Filter cutoff in absolute cents.  `None` = generator absent;
    /// the spec default 13500 (~20 kHz) reads as "filter off" so we
    /// still surface it but build_region collapses high-cutoff
    /// regions back to "no filter" for the SfzRegion contract.
    pub(super) initial_filter_fc_cents: Option<i16>,
    /// Filter resonance peak gain in centibels of attenuation
    /// (Q_linear = 10^(cB / 200)).  `None` = generator absent.
    pub(super) initial_filter_q_cb: Option<i16>,
    /// `sampleModes` (gen 54) — controls loop behaviour.  `None` =
    /// generator absent, which the SF2 spec treats as "no loop"
    /// regardless of any loop window in the SHDR.  Stored as the
    /// raw u16 so build_region maps it to `SfzLoopMode`.
    pub(super) sample_modes: Option<u16>,
    /// Modulation LFO — delay (timecents), frequency (absolute cents),
    /// pitch depth (cents).  Spec defaults: delay -12000 ≈ 1 ms,
    /// frequency 0 cents = 8.176 Hz, depth 0 cents = no effect.
    pub(super) delay_mod_lfo_tc: Option<i16>,
    pub(super) freq_mod_lfo_cents: Option<i16>,
    pub(super) mod_lfo_to_pitch_cents: Option<i16>,
    /// Vibrato LFO — same shape as the modulation LFO, dedicated to
    /// pitch.  Lets a region carry simultaneous independent
    /// vibrato + modulation wobbles.
    pub(super) delay_vib_lfo_tc: Option<i16>,
    pub(super) freq_vib_lfo_cents: Option<i16>,
    pub(super) vib_lfo_to_pitch_cents: Option<i16>,
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
            attack_vol_env_tc: None,
            decay_vol_env_tc: None,
            sustain_vol_env_cb: None,
            release_vol_env_tc: None,
            initial_filter_fc_cents: None,
            initial_filter_q_cb: None,
            sample_modes: None,
            delay_mod_lfo_tc: None,
            freq_mod_lfo_cents: None,
            mod_lfo_to_pitch_cents: None,
            delay_vib_lfo_tc: None,
            freq_vib_lfo_cents: None,
            vib_lfo_to_pitch_cents: None,
        }
    }
}

impl Generators {
    pub(super) fn absorb(&mut self, oper: u16, amount: &[u8]) {
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
            GEN_ATTACK_VOL_ENV => {
                if let Ok(b) = amount.try_into() {
                    self.attack_vol_env_tc = Some(i16::from_le_bytes(b));
                }
            }
            GEN_DECAY_VOL_ENV => {
                if let Ok(b) = amount.try_into() {
                    self.decay_vol_env_tc = Some(i16::from_le_bytes(b));
                }
            }
            GEN_SUSTAIN_VOL_ENV => {
                if let Ok(b) = amount.try_into() {
                    self.sustain_vol_env_cb = Some(i16::from_le_bytes(b));
                }
            }
            GEN_RELEASE_VOL_ENV => {
                if let Ok(b) = amount.try_into() {
                    self.release_vol_env_tc = Some(i16::from_le_bytes(b));
                }
            }
            GEN_INITIAL_FILTER_FC => {
                if let Ok(b) = amount.try_into() {
                    self.initial_filter_fc_cents = Some(i16::from_le_bytes(b));
                }
            }
            GEN_INITIAL_FILTER_Q => {
                if let Ok(b) = amount.try_into() {
                    self.initial_filter_q_cb = Some(i16::from_le_bytes(b));
                }
            }
            GEN_SAMPLE_MODES => {
                if let Ok(b) = amount.try_into() {
                    // Stored as a u16 — the spec only defines bits 0–1
                    // but the field is a 16-bit word.
                    self.sample_modes = Some(u16::from_le_bytes(b));
                }
            }
            GEN_DELAY_MOD_LFO => {
                if let Ok(b) = amount.try_into() {
                    self.delay_mod_lfo_tc = Some(i16::from_le_bytes(b));
                }
            }
            GEN_FREQ_MOD_LFO => {
                if let Ok(b) = amount.try_into() {
                    self.freq_mod_lfo_cents = Some(i16::from_le_bytes(b));
                }
            }
            GEN_MOD_LFO_TO_PITCH => {
                if let Ok(b) = amount.try_into() {
                    self.mod_lfo_to_pitch_cents = Some(i16::from_le_bytes(b));
                }
            }
            GEN_DELAY_VIB_LFO => {
                if let Ok(b) = amount.try_into() {
                    self.delay_vib_lfo_tc = Some(i16::from_le_bytes(b));
                }
            }
            GEN_FREQ_VIB_LFO => {
                if let Ok(b) = amount.try_into() {
                    self.freq_vib_lfo_cents = Some(i16::from_le_bytes(b));
                }
            }
            GEN_VIB_LFO_TO_PITCH => {
                if let Ok(b) = amount.try_into() {
                    self.vib_lfo_to_pitch_cents = Some(i16::from_le_bytes(b));
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
