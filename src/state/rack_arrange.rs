// ─── state/rack_arrange.rs ────────────────────────────────────────────────────
// `RackState::arrange_grid` — canonical sort + 2D bin-pack of every
// module in every zone onto the 12-column rack grid.  Extracted
// from `rack.rs` to keep that file under the 1000-line cap; the
// function is ~320 lines (mostly the `order(kind: ModuleKind) ->
// u8` priority table that grows by one arm per new ModuleKind),
// and was the largest single block in rack.rs.
//
// Same `impl RackState` symmetry as `rack_wiring.rs` — the bin-
// packing reaches into `self.modules` for mutable layout writes,
// so a free function would just be a `&mut self` taker with extra
// ceremony.

use std::collections::HashMap;

use super::module_kind::{GRID_COLS, ModuleKind, Zone};
use super::rack::RackState;

impl RackState {
    /// Bin-pack modules onto the 12-column grid within each zone.
    /// Scans top-to-bottom, left-to-right for the first position where
    /// the module's (w, h) span fits without overlapping existing modules.
    /// After placement, runs a center-bias pass per zone that shifts each
    /// row band toward the centre column when there's free space on the
    /// right edge.
    pub fn arrange_grid(&mut self) {
        fn order(kind: ModuleKind) -> u8 {
            // Within each zone: full-width strips first, then smaller modules pack
            // into the free cells below. AI zone: console above, agents pack under.
            // MAIN AUDIO zone: sequencer, then master.
            match kind {
                ModuleKind::LlmConsole => 0,
                ModuleKind::LlmAgent => 1,
                ModuleKind::StepSequencer => 2,
                ModuleKind::MasterOutput => 3,
                // Canonical voice order in the voice zone: put the bass between
                // the two drum kits so the melodic voice sits centered between
                // low (808) and high (909) drums — also matches its pitch
                // register, which lives above 808 kicks and below 909 hats.
                ModuleKind::DrumKit808 => 10,
                ModuleKind::AcidBass => 11,
                ModuleKind::DrumKit909 => 12,
                // Gabber kick sits next to the drum kits — it's a drum voice.
                ModuleKind::GabberKick => 13,
                ModuleKind::HooverLead => 14,
                // Pluck sits next to Hoover in the voice strip — both
                // are monophonic melodic voices.
                ModuleKind::PluckString => 14,
                // Wavetable next to the other monophonic melodic voices.
                ModuleKind::WavetableVoice => 15,
                // Sample Instrument lives next to Wavetable — both are
                // user-loaded-WAV pitched voices.
                ModuleKind::SampleInstrument => 15,
                ModuleKind::An1xVoice => 15,
                ModuleKind::AmenSampler => 16,
                ModuleKind::NoiseVoice => 17,
                // Theremin sits next to the noise voice — both are
                // "weird sustained drones" tonally.
                ModuleKind::Theremin => 17,
                // Pendulum same family — drone voice with no
                // sequencer trigger, knob-driven beat character.
                ModuleKind::Pendulum => 17,
                // FM operator synth — sequencer-driven voice, sits
                // with the synthesised voices (between AcidBass and
                // HooverLead range).  Bell / FM-bass / E-piano
                // territory complements the AN1X subtractive bank.
                ModuleKind::FmOpsVoice => 13,
                // Additive — sequencer-driven voice; sits next to
                // the FM op synth in the synthesised-voice cluster
                // since both are spectrum-shaping voices that
                // complement the subtractive AN1X.
                ModuleKind::AdditiveVoice => 13,
                // Modal — same cluster as Additive / FM op since
                // it's another spectrum-shaping voice (struck
                // resonator bank instead of partial sum).
                ModuleKind::ModalVoice => 13,
                // Chiptune — synthesised voice cluster with the
                // rest of the spectrum-shaping bank.
                ModuleKind::ChiptuneVoice => 13,
                // Vocal — synthesised voice cluster (formant
                // bank, distinct from NeuTts which lives further
                // along with the sample-style voices).
                ModuleKind::VocalVoice => 13,
                ModuleKind::GranularTexture => 18,
                ModuleKind::NeuTts => 19,
                ModuleKind::FxWaveshaper => 20,
                ModuleKind::FxReverb => 21,
                ModuleKind::FxDelay => 22,
                ModuleKind::FxBitcrush => 23,
                ModuleKind::FxChorus => 24,
                ModuleKind::FxPhaser => 25,
                // Flanger sits adjacent to the phaser in the FX strip — both
                // are LFO-modulated comb-flavour effects, the user reaches
                // for them in the same context.
                ModuleKind::FxFlanger => 25,
                // Comb belongs in the modulation-flavour cluster too —
                // it's a feedback comb tuned to a pitch.
                ModuleKind::FxComb => 26,
                ModuleKind::FxRingMod => 27,
                // Filter / Tilt slot near the EQ family.
                ModuleKind::FxFilter => 28,
                ModuleKind::FxTilt => 29,
                // Transient / Exciter / Limiter — dynamics + mastering
                // tools, near the compressor / tape sat cluster.
                ModuleKind::FxTransient => 32,
                ModuleKind::FxExciter => 33,
                ModuleKind::FxLimiter => 34,
                // Multitap / RevDelay sit next to the regular Delay.
                ModuleKind::FxMultitap => 22,
                ModuleKind::FxRevDelay => 22,
                // Tape stop / Stutter are rhythmic-modulation FX —
                // park them near the bitcrush / drive cluster.
                ModuleKind::FxTapeStop => 23,
                ModuleKind::FxStutter => 23,
                // Freezer parks near the convolution / spectral cluster.
                ModuleKind::FxFreeze => 24,
                ModuleKind::FxEq => 27,
                ModuleKind::FxCompressor => 28,
                // Gate / Vocoder cluster with the dynamics tools — same
                // sidechain idiom as the compressor, users reach for them
                // in the same context.
                ModuleKind::FxGate => 28,
                ModuleKind::FxVocoder => 28,
                ModuleKind::FxTapeSat => 29,
                ModuleKind::FxDrive => 30,
                ModuleKind::FxAutotune => 31,
                ModuleKind::FxPan => 36,
                // Widen sits next to Pan — both stereo master-stage FX.
                ModuleKind::FxWiden => 36,
                // ConvReverb sorts right next to the stock reverb so the two
                // reverbs sit side-by-side in the FX strip.
                ModuleKind::FxConvReverb => 37,
                // ParamEq sorts right after the fixed 3-band EQ so they
                // appear next to each other in the FX strip.
                ModuleKind::FxParamEq => 38,
                // PitchShift next to Autotune (both are pitch-domain FX).
                ModuleKind::FxPitchShift => 39,
                // FreqShift sits next to PitchShift — both pitch-domain.
                ModuleKind::FxFreqShift => 39,
                // Vinyl groups with the saturation / colour cluster
                // (TapeSat / Drive) — same family of analog-character
                // colour effects.
                ModuleKind::FxVinyl => 29,
                // DJ filter sits next to the static Filter — both
                // are LP/HP/BP shaping FX, just with different
                // control surfaces (DJ filter is one-knob morph,
                // FxFilter has cutoff + mode + drive).
                ModuleKind::FxDjFilter => 19,
                // Tremolo lives in the modulation-FX cluster next
                // to Pan / Chorus / Phaser — all internal-LFO-
                // driven movement effects.
                ModuleKind::FxTremolo => 36,
                // Vibrato joins the same cluster — pitch-modulation
                // cousin of Tremolo's amplitude modulation.
                ModuleKind::FxVibrato => 36,
                // ISO EQ groups with the DJ filter — both are
                // performance-oriented filter / band-shaping FX.
                ModuleKind::FxIsoEq => 19,
                // De-esser groups with the dynamics tools — same
                // sidechain idiom as the gate / compressor.
                ModuleKind::FxDeEsser => 28,
                // Resonator bank groups with the comb resonator —
                // same family of pitched-resonance FX, just with
                // six tuned bands instead of one.
                ModuleKind::FxResBank => 9,
                // Tape echo lives next to the stock delay /
                // multitap / revdelay cluster — same delay-line
                // family, distinct character.
                ModuleKind::FxTapeEcho => 11,
                // Multiband compressor sits with the dynamics
                // tools (single-band comp, gate, vocoder).
                ModuleKind::FxMultibandComp => 28,
                // Grain delay groups with the delay-line cluster
                // (delay / multitap / revdelay / tape echo).
                ModuleKind::FxGrainDelay => 11,
                // Spectral gate groups with FxFreeze — both
                // spectral-domain effects, both V1 approximations
                // pending FFT machinery.
                ModuleKind::FxSpectralGate => 24,
                // Plate reverb sorts next to ConvReverb (37) so the
                // three reverb modules cluster (FxReverb=21 separated
                // by other FX, ConvReverb / Plate sit together) — users
                // tend to A/B between them in the same context.
                ModuleKind::FxPlate => 37,
                // Trance gate clusters with FxGate / FxStutter / FxFreeze
                // — the rhythmic / gating family.  23 lands it next to
                // Stutter and TapeStop, both also pattern / time-domain
                // chopping FX.
                ModuleKind::FxTranceGate => 23,
                // Wavefolder clusters with the saturation / drive /
                // waveshaper / vinyl colour family — all distortion-
                // class FX.  Sort key 29 puts it next to FxTapeSat.
                ModuleKind::FxWaveFolder => 29,
                ModuleKind::VoiceMeterStrip => 33,
                // GR history clusters with the LUFS / loudness viz
                // (level / dynamics-domain meters) — sort 33 places it
                // adjacent to the LoudnessMeter for stacked viewing.
                ModuleKind::GrHistory => 33,
                ModuleKind::SpectrumAnalyzer => 32,
                ModuleKind::StereoMeter => 33,
                ModuleKind::ActivityTimeline => 34,
                ModuleKind::LfoModule => 35,
                ModuleKind::CvSequencer => 35,
                ModuleKind::Slew => 35,
                ModuleKind::Quantizer => 35,
                ModuleKind::Comparator => 35,
                ModuleKind::SampleHold => 35,
                ModuleKind::Math => 35,
                ModuleKind::TriggerDiv => 35,
                ModuleKind::LogicGate => 35,
                ModuleKind::FunctionGen => 35,
                ModuleKind::Crossfader => 35,
                // Bar oscilloscope sorts next to the spectrum module —
                // both are global-bus visualisers; users tend to want
                // them adjacent.
                ModuleKind::BarOscilloscope => 40,
                // Goniometer / vectorscope sits next to the bar scope —
                // both are global-bus stereo / waveform visualisers.
                ModuleKind::StereoVectorscope => 41,
                // LFO scope groups with the LFO modules.
                ModuleKind::LfoScope => 42,
                // CV-seq scope sits next to the LFO scope — both
                // are dedicated viz companions for modulation
                // sources.
                ModuleKind::CvSeqScope => 42,
                // Tuner + chord display group with the spectrum cluster.
                ModuleKind::PitchTracker => 43,
                ModuleKind::ChordDisplay => 44,
                // Spectrogram pairs with the bar spectrum — same data,
                // different time-axis treatment.
                ModuleKind::Spectrogram => 46,
                // LUFS sits with the rest of the analysis cluster.
                ModuleKind::LoudnessMeter => 47,
                // Phase wheel pairs with EventStream (transport readout).
                ModuleKind::PhaseWheel => 48,
                // Event stream is melodic / rhythmic activity, parks
                // next to ActivityTimeline.
                ModuleKind::EventStream => 49,
                // Pattern heatmap groups with the activity / event
                // viz cluster — all sequencer-state readouts.
                ModuleKind::PatternHeatmap => 50,
                // Onset grid groups with the heatmap — both
                // sequencer-relative analysis tools.
                ModuleKind::OnsetGrid => 51,
            }
        }
        let cols = GRID_COLS as usize;
        let max_rows = 64usize; // generous upper bound

        for zone in [Zone::Ai, Zone::Global, Zone::Voice, Zone::FxMod] {
            // Collect and sort by canonical order
            let mut ids: Vec<(u32, ModuleKind, bool)> = self
                .modules
                .iter()
                .filter(|m| m.zone == zone)
                .map(|m| (m.id, m.kind, m.pad_expanded))
                .collect();
            ids.sort_by_key(|&(_, k, _)| order(k));

            // 2D occupancy grid
            let mut occ = vec![vec![false; cols]; max_rows];

            for (slot_idx, &(id, kind, pad_expanded)) in ids.iter().enumerate() {
                let (w, h) = kind.grid_size(GRID_COLS);
                let h = if kind.supports_xy_pad() && pad_expanded {
                    h + 1
                } else {
                    h
                };
                let h = self.dyn_height_override(kind).unwrap_or(h);
                let w = w as usize;
                let h = h as usize;

                // Find first free position (top-to-bottom, left-to-right)
                let mut placed = false;
                'scan: for r in 0..max_rows - h {
                    for c in 0..=cols - w {
                        // Check if w×h block is free
                        let fits = (0..h).all(|dr| (0..w).all(|dc| !occ[r + dr][c + dc]));
                        if fits {
                            // Mark occupied
                            for dr in 0..h {
                                for dc in 0..w {
                                    occ[r + dr][c + dc] = true;
                                }
                            }
                            if let Some(m) = self.modules.iter_mut().find(|m| m.id == id) {
                                m.grid_col = c as u8;
                                m.grid_row = r as u8;
                                m.slot = slot_idx as u8;
                            }
                            placed = true;
                            break 'scan;
                        }
                    }
                }
                if !placed {
                    // Shouldn't happen with 64 rows, but fallback to (0, 0)
                    if let Some(m) = self.modules.iter_mut().find(|m| m.id == id) {
                        m.grid_col = 0;
                        m.grid_row = 0;
                        m.slot = slot_idx as u8;
                    }
                }
            }

            // ── Center-bias pass: shift row bands toward the center ──────
            // Find the rightmost occupied column per row, then group rows
            // connected by multi-row modules and apply a uniform shift.
            let zone_mods: Vec<(u32, u8, u8, u8, u8)> = self
                .modules
                .iter()
                .filter(|m| m.zone == zone)
                .map(|m| {
                    let (w, h) = self.effective_grid_size(m);
                    (m.id, m.grid_col, m.grid_row, w, h)
                })
                .collect();
            if zone_mods.is_empty() {
                continue;
            }
            // Union-find to group rows linked by tall modules
            let used_rows = zone_mods
                .iter()
                .map(|&(_, _, r, _, h)| (r + h) as usize)
                .max()
                .unwrap_or(0);
            let mut parent: Vec<usize> = (0..used_rows).collect();
            fn find(p: &mut [usize], x: usize) -> usize {
                if p[x] != x {
                    p[x] = find(p, p[x]);
                }
                p[x]
            }
            fn union(p: &mut [usize], a: usize, b: usize) {
                let ra = find(p, a);
                let rb = find(p, b);
                if ra != rb {
                    p[rb] = ra;
                }
            }
            for &(_, _, r, _, h) in &zone_mods {
                for dr in 1..h {
                    union(&mut parent, r as usize, (r + dr) as usize);
                }
            }
            // Compute max right edge per row-band
            let mut band_right: HashMap<usize, u8> = HashMap::new();
            for &(_, c, r, w, h) in &zone_mods {
                let right = c + w;
                for dr in 0..h {
                    let band = find(&mut parent, (r + dr) as usize);
                    let entry = band_right.entry(band).or_insert(0);
                    *entry = (*entry).max(right);
                }
            }
            // Apply centering shift per module
            for &(id, _, r, _, _) in &zone_mods {
                let band = find(&mut parent, r as usize);
                let right = band_right.get(&band).copied().unwrap_or(cols as u8);
                if right < cols as u8 {
                    let shift = (cols as u8 - right) / 2;
                    if shift > 0
                        && let Some(m) = self.modules.iter_mut().find(|m| m.id == id)
                    {
                        m.grid_col += shift;
                    }
                }
            }
        }
    }
}
