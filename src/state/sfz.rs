// ─── state/sfz.rs ────────────────────────────────────────────────────────────
// SFZ parser — converts a `.sfz` text file into a list of `SfzRegion`
// records the SampleInstrument voice can consume.  Pure function: takes
// the file's text plus the `.sfz`'s parent directory (used to resolve
// relative `sample=` paths) and returns a `Vec<SfzRegion>`.
//
// Subset (per PLAN.md): `<region>` / `<group>` / `<global>` headers, with
// the opcodes:
//   sample, lokey/hikey/pitch_keycenter, lovel/hivel,
//   loop_mode, loop_start, loop_end,
//   volume, pan,
//   seq_position, seq_length,
//   tune, transpose,
//   ampeg_attack, ampeg_decay, ampeg_sustain, ampeg_release,
//   cutoff, resonance, fil_type.
//
// Anything outside the subset is logged + ignored (the SFZ spec is
// huge — the goal is "load most CC0 piano / orchestral SFZs", not full
// compliance).  Cascading: opcodes inside `<global>` apply to every
// region; `<group>` opcodes apply to every following region until the
// next `<group>` / `<global>`; `<region>` opcodes win where they
// overlap.

use std::path::{Path, PathBuf};

/// One playable zone parsed out of an SFZ file.  All fields are
/// resolved (cascaded from `<global>` → `<group>` → `<region>`); the
/// SampleInstrument voice consumes this list directly without
/// re-walking the cascade.
#[derive(Clone, Debug, PartialEq)]
pub struct SfzRegion {
    /// Resolved absolute path to the audio file referenced by `sample=`.
    /// Empty when the region carries no `sample=` opcode (rare; usually
    /// it means a `<global>` / `<group>` with no playable content).
    pub sample_path: PathBuf,
    /// MIDI note range (inclusive on both ends).  Defaults to 0..=127.
    pub lokey: u8,
    pub hikey: u8,
    /// MIDI note that plays at original pitch.  Defaults to 60 (C4).
    pub pitch_keycenter: u8,
    /// Velocity range (inclusive).  Defaults to 0..=127.
    pub lovel: u8,
    pub hivel: u8,
    /// Loop mode — one of "no_loop", "one_shot", "loop_continuous",
    /// "loop_sustain".  Stored verbatim; the voice maps it.
    pub loop_mode: SfzLoopMode,
    /// Loop start sample index (inclusive).  `None` falls back to 0.
    pub loop_start: Option<u32>,
    /// Loop end sample index (inclusive).  `None` falls back to the
    /// buffer end.
    pub loop_end: Option<u32>,
    /// Volume offset in dB (default 0 dB).  Clamped at consume time.
    pub volume_db: f32,
    /// Pan -100..+100 (centred at 0).
    pub pan: f32,
    /// Round-robin position (1-indexed in SFZ; 0 = inactive).
    pub seq_position: u8,
    /// Round-robin length (number of round-robin variants).  0 = no RR.
    pub seq_length: u8,
    /// Tune in cents (-100..+100).
    pub tune_cents: f32,
    /// Transpose in semitones (-127..+127).
    pub transpose: i8,
    /// Amplitude envelope (seconds for A/D/R, percent for sustain).
    /// Defaults match SFZ defaults: 0 attack/decay, 100% sustain, 0
    /// release.
    pub ampeg_attack_s: f32,
    pub ampeg_decay_s: f32,
    pub ampeg_sustain_pct: f32,
    pub ampeg_release_s: f32,
    /// Filter cutoff in Hz (0 = no filter).
    pub cutoff_hz: f32,
    /// Filter resonance in dB.
    pub resonance_db: f32,
    /// Filter type — one of `Lpf2p` / `Hpf2p` / `Bpf2p`.  `None` for
    /// no filter (cutoff = 0 implies this).
    pub fil_type: Option<SfzFilType>,
    /// CC#1 (mod wheel) crossfade-in lower bound, 0..127.  Below this
    /// CC value the region is fully silent on CC1; between
    /// `xfin_lo_cc1` and `xfin_hi_cc1` it ramps linearly to full
    /// gain.  Default 0 means "fully present at CC1 = 0" (no
    /// fade-in).
    pub xfin_lo_cc1: u8,
    /// CC#1 crossfade-in upper bound.  Above this CC value the
    /// fade-in gain is 1.0.  Default 0 means the fade-in is a
    /// no-op (lo == hi at 0 → gain = 1 for any CC).
    pub xfin_hi_cc1: u8,
    /// CC#1 crossfade-out lower bound.  Below this CC value the
    /// fade-out gain is 1.0.  Default 127 means "no fade-out".
    pub xfout_lo_cc1: u8,
    /// CC#1 crossfade-out upper bound.  Above this CC value the
    /// fade-out gain is 0.  Default 127 = no fade-out at any CC.
    pub xfout_hi_cc1: u8,
    /// Modulation LFO + vibrato LFO.  Mod LFO drives three targets
    /// (pitch, filter cutoff, volume); vib LFO is pitch-only.  All
    /// four LFO timing fields are seconds / Hz; pitch / filter
    /// depths are cents, volume depth is centibels of attenuation.
    /// Defaults (0 depth on every target, 8.176 Hz, 0 s delay)
    /// leave the LFO inert so regions without these generators are
    /// bit-identical to pre-LFO behaviour.
    pub mod_lfo_freq_hz: f32,
    pub mod_lfo_delay_s: f32,
    pub mod_lfo_to_pitch_cents: f32,
    pub mod_lfo_to_filter_fc_cents: f32,
    pub mod_lfo_to_volume_cb: f32,
    pub vib_lfo_freq_hz: f32,
    pub vib_lfo_delay_s: f32,
    pub vib_lfo_to_pitch_cents: f32,
    /// Modulation envelope — five stages plus a sustain level.  Times
    /// in seconds (already converted from SF2 timecents at load
    /// time); sustain is the linear 0..1 level the env decays to.
    /// Defaults (0 s on every stage, sustain 1.0, depths 0) leave
    /// the envelope inert so a region without these generators is
    /// bit-identical to the pre-modenv path.
    pub mod_env_delay_s: f32,
    pub mod_env_attack_s: f32,
    pub mod_env_hold_s: f32,
    pub mod_env_decay_s: f32,
    pub mod_env_sustain_level: f32,
    pub mod_env_release_s: f32,
    /// Cents shift on the read-rate at full envelope value (env = 1).
    pub mod_env_to_pitch_cents: f32,
    /// Cents shift on the filter cutoff knob at full envelope value.
    pub mod_env_to_filter_fc_cents: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SfzLoopMode {
    #[default]
    NoLoop,
    OneShot,
    LoopContinuous,
    LoopSustain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfzFilType {
    Lpf2p,
    Hpf2p,
    Bpf2p,
}

impl Default for SfzRegion {
    /// SFZ-spec defaults for unspecified opcodes.  Also used as the
    /// starting `<global>` accumulator so a bare `<region>` inherits
    /// these (lokey=0, hikey=127, lovel=0, hivel=127, key center C4,
    /// flat envelope at 100% sustain, no filter).
    fn default() -> Self {
        Self {
            sample_path: PathBuf::new(),
            lokey: 0,
            hikey: 127,
            pitch_keycenter: 60,
            lovel: 0,
            hivel: 127,
            loop_mode: SfzLoopMode::NoLoop,
            loop_start: None,
            loop_end: None,
            volume_db: 0.0,
            pan: 0.0,
            seq_position: 0,
            seq_length: 0,
            tune_cents: 0.0,
            transpose: 0,
            ampeg_attack_s: 0.0,
            ampeg_decay_s: 0.0,
            ampeg_sustain_pct: 100.0,
            ampeg_release_s: 0.0,
            cutoff_hz: 0.0,
            resonance_db: 0.0,
            fil_type: None,
            // Crossfade defaults represent "fully present at any CC":
            // xfin range 0..0 → fade-in gain 1.0 from CC=0 onward;
            // xfout range 127..127 → fade-out gain 1.0 up to CC=127.
            // A region without xfin/xfout opcodes therefore reads as
            // gain=1 regardless of CC1, preserving V1 behaviour.
            xfin_lo_cc1: 0,
            xfin_hi_cc1: 0,
            xfout_lo_cc1: 127,
            xfout_hi_cc1: 127,
            // 8.176 Hz / 0 cents = SF2 spec defaults; 0 depth = no
            // audible LFO modulation.
            mod_lfo_freq_hz: 8.176,
            mod_lfo_delay_s: 0.0,
            mod_lfo_to_pitch_cents: 0.0,
            mod_lfo_to_filter_fc_cents: 0.0,
            mod_lfo_to_volume_cb: 0.0,
            vib_lfo_freq_hz: 8.176,
            vib_lfo_delay_s: 0.0,
            vib_lfo_to_pitch_cents: 0.0,
            mod_env_delay_s: 0.0,
            mod_env_attack_s: 0.0,
            mod_env_hold_s: 0.0,
            mod_env_decay_s: 0.0,
            mod_env_sustain_level: 1.0,
            mod_env_release_s: 0.0,
            mod_env_to_pitch_cents: 0.0,
            mod_env_to_filter_fc_cents: 0.0,
        }
    }
}

impl SfzRegion {
    /// Whether this region produced any playable audio file reference
    /// (`<global>` / `<group>` headers without a `sample=` opcode get
    /// dropped from the final list).
    pub fn is_playable(&self) -> bool {
        !self.sample_path.as_os_str().is_empty()
    }

    /// True when `note` falls inside `lokey..=hikey`.
    pub fn matches_note(&self, note: u8) -> bool {
        note >= self.lokey && note <= self.hikey
    }

    /// True when `velocity` falls inside `lovel..=hivel`.
    pub fn matches_velocity(&self, velocity: u8) -> bool {
        velocity >= self.lovel && velocity <= self.hivel
    }

    /// Linear-blend gain for this region given the current CC#1 value
    /// (mod wheel — the standard SFZ convention for multi-mic packs).
    /// Returns 1.0 for regions without crossfade opcodes (defaults
    /// resolve to "fully present at any CC value").  Composed of an
    /// xfin ramp (0 → 1 across `xfin_lo..xfin_hi`) and an xfout ramp
    /// (1 → 0 across `xfout_lo..xfout_hi`); the product is the
    /// region's gain at this CC.
    pub fn cc1_crossfade_gain(&self, cc: u8) -> f32 {
        // The defaults (xfin 0..0, xfout 127..127) represent "no
        // crossfade" — early-return gain=1 in both branches so
        // regions without xfin/xfout opcodes pass through cleanly.
        // A real crossfade always has hi > lo (SFZ semantics), so
        // a parsed region with hi == lo also reads as "no fade"
        // here, matching the convention that a 0-width ramp is a
        // no-op rather than an instant gate.
        let cc = cc.min(127) as f32;

        let g_in = if self.xfin_hi_cc1 <= self.xfin_lo_cc1 {
            1.0
        } else {
            let lo = self.xfin_lo_cc1 as f32;
            let hi = self.xfin_hi_cc1 as f32;
            if cc <= lo {
                0.0
            } else if cc >= hi {
                1.0
            } else {
                (cc - lo) / (hi - lo)
            }
        };

        let g_out = if self.xfout_hi_cc1 <= self.xfout_lo_cc1 {
            1.0
        } else {
            let lo = self.xfout_lo_cc1 as f32;
            let hi = self.xfout_hi_cc1 as f32;
            if cc <= lo {
                1.0
            } else if cc >= hi {
                0.0
            } else {
                1.0 - (cc - lo) / (hi - lo)
            }
        };

        (g_in * g_out).clamp(0.0, 1.0)
    }
}

// ─── Parser ──────────────────────────────────────────────────────────────────

/// Parse an SFZ file's text into a list of playable regions.  `base_dir`
/// is the directory the `.sfz` lives in — relative `sample=` paths
/// resolve against it.  Unknown opcodes / headers are logged + ignored,
/// not errors.
pub fn parse_sfz(text: &str, base_dir: &Path) -> Vec<SfzRegion> {
    let tokens = tokenize(text);
    let mut regions = Vec::new();
    let mut global = SfzRegion::default();
    let mut group = SfzRegion::default();
    let mut region: Option<SfzRegion> = None;
    // Active scope: which struct receives the next opcode.  Starts at
    // global before any header has been seen — most SFZs open with a
    // `<global>` or `<group>`, but a leading bare opcode is treated as
    // global-scope.
    let mut scope = Scope::Global;

    for tok in tokens {
        match tok {
            Token::Header(name) => {
                // Flush any in-progress region before the header changes
                // scope — a `<region>` followed by another header
                // commits the in-progress region.
                if let Some(r) = region.take()
                    && r.is_playable()
                {
                    regions.push(r);
                }
                match name.as_str() {
                    "global" => {
                        scope = Scope::Global;
                    }
                    "group" => {
                        // New group resets to the current global scope.
                        group = global.clone();
                        scope = Scope::Group;
                    }
                    "region" => {
                        // New region inherits from the active group.
                        region = Some(group.clone());
                        scope = Scope::Region;
                    }
                    "control" | "curve" | "effect" | "master" | "midi" | "sample" => {
                        // Recognised SFZ headers we don't model — skip
                        // their opcodes by routing further tokens to a
                        // throwaway buffer.  Easier: just treat them as
                        // a no-op scope; opcode_apply on `Scope::Skip`
                        // discards.
                        scope = Scope::Skip;
                    }
                    other => {
                        log::debug!("sfz: ignoring unknown header <{}>", other);
                        scope = Scope::Skip;
                    }
                }
            }
            Token::Opcode { key, value } => {
                let target: &mut SfzRegion = match scope {
                    Scope::Global => &mut global,
                    Scope::Group => &mut group,
                    Scope::Region => match region.as_mut() {
                        Some(r) => r,
                        None => {
                            // Opcode arrived in `region` scope but no
                            // `<region>` had opened — defensive: fall
                            // back to group.
                            &mut group
                        }
                    },
                    Scope::Skip => continue,
                };
                apply_opcode(target, &key, &value, base_dir);
            }
        }
    }

    // Final flush.
    if let Some(r) = region.take()
        && r.is_playable()
    {
        regions.push(r);
    }

    regions
}

#[derive(Clone, Copy, Debug)]
enum Scope {
    Global,
    Group,
    Region,
    Skip,
}

#[derive(Debug, PartialEq)]
enum Token {
    Header(String),
    Opcode { key: String, value: String },
}

/// Tokenise an SFZ source.  Strips `//` line comments + `/* */` block
/// comments, then emits headers + opcodes.  Sample paths can contain
/// spaces, so opcode values run until the next opcode-key boundary
/// (or the next header / EOF) — we look ahead for `[a-z_][a-z0-9_]*=`
/// before splitting.
fn tokenize(text: &str) -> Vec<Token> {
    // Strip block comments first; the SFZ spec allows them anywhere.
    let no_block = strip_block_comments(text);
    // Split into lines, strip `//` line comments per line.
    let mut joined = String::with_capacity(no_block.len());
    for line in no_block.lines() {
        let stripped = match line.find("//") {
            Some(idx) => &line[..idx],
            None => line,
        };
        joined.push_str(stripped);
        joined.push('\n');
    }
    // Walk the cleaned text, emitting tokens.
    let bytes = joined.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // Whitespace.
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Header: <name>
        if c == b'<' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'>' {
                end += 1;
            }
            if end < bytes.len() {
                let name = std::str::from_utf8(&bytes[start..end])
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                tokens.push(Token::Header(name));
                i = end + 1;
            } else {
                // Unterminated header — bail.
                break;
            }
            continue;
        }
        // Otherwise: opcode `key=value`.
        let key_start = i;
        let mut eq_pos = None;
        while i < bytes.len() {
            let ch = bytes[i];
            if ch == b'=' {
                eq_pos = Some(i);
                i += 1;
                break;
            }
            if ch.is_ascii_whitespace() || ch == b'<' {
                break;
            }
            i += 1;
        }
        let Some(eq) = eq_pos else {
            // Stray non-key text — skip to next whitespace.
            continue;
        };
        let key = std::str::from_utf8(&bytes[key_start..eq])
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        // Value runs until the next key-followed-by-`=` or `<header>`
        // or EOF.  We slide a cursor and check whether `bytes[j..]`
        // starts a new opcode at every whitespace boundary.
        let value_start = i;
        let mut value_end = bytes.len();
        let mut j = i;
        while j < bytes.len() {
            let ch = bytes[j];
            if ch == b'<' {
                value_end = j;
                break;
            }
            if ch.is_ascii_whitespace() {
                // Look ahead to see if the next non-whitespace token is
                // a `key=` form.  If yes, the current value ends here.
                let mut k = j + 1;
                while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < bytes.len() && bytes[k] == b'<' {
                    value_end = j;
                    break;
                }
                if let Some(eq_idx) = scan_key_eq(&bytes[k..]) {
                    // Confirmed next opcode.  Set value end at this
                    // whitespace boundary; advance the outer cursor to
                    // `k` so the next loop iteration picks up the new
                    // key.  `eq_idx` is unused beyond confirmation.
                    let _ = eq_idx;
                    value_end = j;
                    j = k;
                    break;
                }
                // Not a new opcode — continue scanning.
                j = k;
                continue;
            }
            j += 1;
        }
        let value = std::str::from_utf8(&bytes[value_start..value_end])
            .unwrap_or("")
            .trim()
            .to_string();
        if !key.is_empty() && !value.is_empty() {
            tokens.push(Token::Opcode { key, value });
        }
        i = j;
    }
    tokens
}

/// Strip C-style `/* ... */` block comments.  Nested blocks aren't
/// supported by the SFZ spec, so a single-pass replace works.
fn strip_block_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Skip until matching `*/`.
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Returns the position of `=` if `bytes` starts with `[a-z_][a-z0-9_]*=`.
fn scan_key_eq(bytes: &[u8]) -> Option<usize> {
    let mut i = 0;
    if i >= bytes.len() {
        return None;
    }
    let first = bytes[0];
    let is_first_ok = first.is_ascii_alphabetic() || first == b'_';
    if !is_first_ok {
        return None;
    }
    while i < bytes.len() {
        let ch = bytes[i];
        if ch == b'=' {
            return Some(i);
        }
        let ok = ch.is_ascii_alphanumeric() || ch == b'_';
        if !ok {
            return None;
        }
        i += 1;
    }
    None
}

// ─── Opcode dispatch ─────────────────────────────────────────────────────────

fn apply_opcode(r: &mut SfzRegion, key: &str, value: &str, base_dir: &Path) {
    match key {
        "sample" => {
            let p = Path::new(value);
            r.sample_path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                // SFZ paths often use backslashes on Windows-authored
                // files; normalise to forward slashes so `Path::join`
                // builds a sensible relative path on every platform.
                let normalised = value.replace('\\', "/");
                base_dir.join(normalised)
            };
        }
        "lokey" => {
            if let Some(n) = parse_note_or_int(value) {
                r.lokey = n;
            }
        }
        "hikey" => {
            if let Some(n) = parse_note_or_int(value) {
                r.hikey = n;
            }
        }
        "key" => {
            // Shorthand for setting all three of lokey/hikey/pitch_keycenter.
            if let Some(n) = parse_note_or_int(value) {
                r.lokey = n;
                r.hikey = n;
                r.pitch_keycenter = n;
            }
        }
        "pitch_keycenter" => {
            if let Some(n) = parse_note_or_int(value) {
                r.pitch_keycenter = n;
            }
        }
        "lovel" => {
            if let Ok(n) = value.parse::<u32>() {
                r.lovel = n.min(127) as u8;
            }
        }
        "hivel" => {
            if let Ok(n) = value.parse::<u32>() {
                r.hivel = n.min(127) as u8;
            }
        }
        "loop_mode" => {
            r.loop_mode = match value.to_ascii_lowercase().as_str() {
                "no_loop" => SfzLoopMode::NoLoop,
                "one_shot" => SfzLoopMode::OneShot,
                "loop_continuous" => SfzLoopMode::LoopContinuous,
                "loop_sustain" => SfzLoopMode::LoopSustain,
                _ => r.loop_mode,
            };
        }
        "loop_start" => {
            if let Ok(n) = value.parse::<u32>() {
                r.loop_start = Some(n);
            }
        }
        "loop_end" => {
            if let Ok(n) = value.parse::<u32>() {
                r.loop_end = Some(n);
            }
        }
        "volume" => {
            if let Ok(v) = value.parse::<f32>() {
                r.volume_db = v;
            }
        }
        "pan" => {
            if let Ok(v) = value.parse::<f32>() {
                r.pan = v.clamp(-100.0, 100.0);
            }
        }
        "seq_position" => {
            if let Ok(n) = value.parse::<u32>() {
                r.seq_position = n.min(255) as u8;
            }
        }
        "seq_length" => {
            if let Ok(n) = value.parse::<u32>() {
                r.seq_length = n.min(255) as u8;
            }
        }
        "tune" => {
            if let Ok(v) = value.parse::<f32>() {
                r.tune_cents = v.clamp(-100.0, 100.0);
            }
        }
        "transpose" => {
            if let Ok(v) = value.parse::<i32>() {
                r.transpose = v.clamp(-127, 127) as i8;
            }
        }
        "ampeg_attack" => {
            if let Ok(v) = value.parse::<f32>() {
                r.ampeg_attack_s = v.max(0.0);
            }
        }
        "ampeg_decay" => {
            if let Ok(v) = value.parse::<f32>() {
                r.ampeg_decay_s = v.max(0.0);
            }
        }
        "ampeg_sustain" => {
            if let Ok(v) = value.parse::<f32>() {
                r.ampeg_sustain_pct = v.clamp(0.0, 100.0);
            }
        }
        "ampeg_release" => {
            if let Ok(v) = value.parse::<f32>() {
                r.ampeg_release_s = v.max(0.0);
            }
        }
        "cutoff" => {
            if let Ok(v) = value.parse::<f32>() {
                r.cutoff_hz = v.max(0.0);
            }
        }
        "resonance" => {
            if let Ok(v) = value.parse::<f32>() {
                r.resonance_db = v;
            }
        }
        "fil_type" => {
            r.fil_type = match value.to_ascii_lowercase().as_str() {
                "lpf_2p" => Some(SfzFilType::Lpf2p),
                "hpf_2p" => Some(SfzFilType::Hpf2p),
                "bpf_2p" => Some(SfzFilType::Bpf2p),
                _ => r.fil_type,
            };
        }
        // Multi-mic / multi-position crossfade — CC#1 (mod wheel)
        // is the standard SFZ convention for blending across mic
        // positions (close / room / ambient).  V1 supports CC#1
        // only; full multi-CC support is V2 work.
        "xfin_locc1" => {
            if let Ok(v) = value.parse::<u32>() {
                r.xfin_lo_cc1 = v.min(127) as u8;
            }
        }
        "xfin_hicc1" => {
            if let Ok(v) = value.parse::<u32>() {
                r.xfin_hi_cc1 = v.min(127) as u8;
            }
        }
        "xfout_locc1" => {
            if let Ok(v) = value.parse::<u32>() {
                r.xfout_lo_cc1 = v.min(127) as u8;
            }
        }
        "xfout_hicc1" => {
            if let Ok(v) = value.parse::<u32>() {
                r.xfout_hi_cc1 = v.min(127) as u8;
            }
        }
        _ => {
            // Out-of-subset opcode — common ones we deliberately don't
            // model (offset, end, fil_keytrack, ...).  Logging at trace
            // keeps the noise out of normal runs but lets the user dig
            // when an SFZ doesn't sound right.
            log::trace!("sfz: ignoring opcode {}={}", key, value);
        }
    }
}

/// Parse a MIDI note from either an integer ("60") or a note name
/// ("c4", "f#3", "bb-1").  SFZ convention: c4 = 60.
pub fn parse_note_or_int(value: &str) -> Option<u8> {
    if let Ok(n) = value.parse::<u32>() {
        return Some(n.min(127) as u8);
    }
    let lower = value.trim().to_ascii_lowercase();
    let bytes = lower.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut i = 0;
    let letter = bytes[i];
    i += 1;
    let semitone_base: i32 = match letter {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        b'b' => 11,
        _ => return None,
    };
    let mut accidental: i32 = 0;
    if i < bytes.len() {
        match bytes[i] {
            b'#' => {
                accidental = 1;
                i += 1;
            }
            // Disambiguate "bb" (B-flat) from "b" + octave digit: a
            // `b` after a non-`b` letter root counts as flat.
            b'b' if letter != b'b' => {
                accidental = -1;
                i += 1;
            }
            _ => {}
        }
    }
    let octave_str = std::str::from_utf8(&bytes[i..]).ok()?;
    let octave: i32 = octave_str.parse().ok()?;
    // SFZ: c-1 = 0, c0 = 12, c4 = 60.
    let midi = (octave + 1) * 12 + semitone_base + accidental;
    if (0..=127).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
}
