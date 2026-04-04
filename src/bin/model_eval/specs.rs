// ─── model_eval/specs.rs ──────────────────────────────────────────────────────
// Check helpers and style spec definitions for the model_eval binary.

use impulse_instruct::state::{AppState, DrumVoice};

// ─── Check types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum CheckResult {
    Pass,
    Fail(String),
    Skip(String), // inference produced no valid JSON
}

impl CheckResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, CheckResult::Pass)
    }
    pub fn symbol(&self) -> &'static str {
        match self {
            CheckResult::Pass => "✓",
            CheckResult::Fail(_) => "✗",
            CheckResult::Skip(_) => "–",
        }
    }
    pub fn detail(&self) -> Option<&str> {
        match self {
            CheckResult::Fail(s) | CheckResult::Skip(s) => Some(s),
            CheckResult::Pass => None,
        }
    }
}

pub struct Check {
    pub name: &'static str,
    pub eval: Box<dyn Fn(&AppState) -> CheckResult + Send + Sync>,
}

impl Check {
    pub fn new(
        name: &'static str,
        f: impl Fn(&AppState) -> CheckResult + Send + Sync + 'static,
    ) -> Self {
        Self {
            name,
            eval: Box::new(f),
        }
    }
    pub fn bpm(name: &'static str, lo: f32, hi: f32) -> Self {
        Self::new(name, move |s| {
            let bpm = s.sequencer.bpm;
            if bpm >= lo && bpm <= hi {
                CheckResult::Pass
            } else {
                CheckResult::Fail(format!("bpm={:.0} want {lo:.0}–{hi:.0}", bpm))
            }
        })
    }
    pub fn ge(
        name: &'static str,
        get: impl Fn(&AppState) -> f32 + Send + Sync + 'static,
        threshold: f32,
    ) -> Self {
        Self::new(name, move |s| {
            let v = get(s);
            if v >= threshold {
                CheckResult::Pass
            } else {
                CheckResult::Fail(format!("got {v:.2} want ≥{threshold:.2}"))
            }
        })
    }
    pub fn bool_eq(
        name: &'static str,
        get: impl Fn(&AppState) -> bool + Send + Sync + 'static,
        want: bool,
    ) -> Self {
        Self::new(name, move |s| {
            let v = get(s);
            if v == want {
                CheckResult::Pass
            } else {
                CheckResult::Fail(format!("got {v} want {want}"))
            }
        })
    }
}

// ─── Style spec ──────────────────────────────────────────────────────────────

pub struct StyleSpec {
    pub id: &'static str,
    pub prompt: &'static str,
    pub checks: Vec<Check>,
}

pub fn build_style_specs() -> Vec<StyleSpec> {
    vec![
        StyleSpec {
            id: "acid_classic",
            prompt: "FULL RESET to classic acid — TB-303, squelchy resonance, hypnotic pattern",
            checks: vec![
                Check::bpm("bpm 112–130", 112.0, 130.0),
                Check::ge("resonance ≥0.68", |s| s.bass.resonance, 0.68),
                Check::ge("env_mod ≥0.55", |s| s.bass.env_mod, 0.55),
                Check::bool_eq("hoover off", |s| s.hoover.enabled, false),
                Check::new("reverb low (acid is dry)", |s| {
                    if s.fx.reverb_mix <= 0.2 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "reverb={:.2} want ≤0.20 (acid is dry)",
                            s.fx.reverb_mix
                        ))
                    }
                }),
            ],
        },
        StyleSpec {
            id: "acid_techno",
            prompt: "FULL RESET to acid techno — harder and faster than acid house, more industrial edge",
            checks: vec![
                Check::bpm("bpm 128–148", 128.0, 148.0),
                Check::ge("resonance ≥0.72", |s| s.bass.resonance, 0.72),
                Check::ge("env_mod ≥0.60", |s| s.bass.env_mod, 0.60),
            ],
        },
        StyleSpec {
            id: "early_rave",
            prompt: "FULL RESET to early rave dominator style — the Human Resource sound, hoover lead is essential",
            checks: vec![
                Check::bpm("bpm 145–168", 145.0, 168.0),
                Check::bool_eq("hoover enabled", |s| s.hoover.enabled, true),
                Check::ge("hoover resonance ≥0.72", |s| s.hoover.resonance, 0.72),
                Check::ge("hoover filter_start ≥0.72", |s| s.hoover.filter_start, 0.72),
                Check::new("hoover has steps", |s| {
                    let active = s
                        .sequencer
                        .hoover_pattern
                        .iter()
                        .filter(|p| p.active)
                        .count();
                    if active >= 1 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail("no hoover steps active".into())
                    }
                }),
            ],
        },
        StyleSpec {
            id: "trance",
            prompt: "FULL RESET to trance — euphoric, emotional, build and release, hoover or pad lead",
            checks: vec![
                Check::bpm("bpm 130–145", 130.0, 145.0),
                Check::bool_eq(
                    "hoover enabled",
                    |s| s.hoover.enabled || s.an1x.enabled,
                    true,
                ),
                Check::ge("reverb ≥0.20", |s| s.fx.reverb_mix, 0.20),
                Check::ge("delay ≥0.15", |s| s.fx.delay_mix, 0.15),
            ],
        },
        StyleSpec {
            id: "gabber",
            prompt: "FULL RESET to gabber — extreme distortion, 4-on-floor kick at 170+ BPM, nothing subtle",
            checks: vec![
                Check::bpm("bpm ≥160", 160.0, 400.0),
                Check::new("heavy distortion", |s| {
                    if s.fx.distortion_drive >= 0.2 || s.fx.distortion_mix >= 0.3 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "drive={:.2} mix={:.2}, want drive≥0.20 or mix≥0.30",
                            s.fx.distortion_drive, s.fx.distortion_mix
                        ))
                    }
                }),
                Check::new("no reverb (gabber is dry)", |s| {
                    if s.fx.reverb_mix <= 0.15 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!("reverb={:.2} want ≤0.15", s.fx.reverb_mix))
                    }
                }),
            ],
        },
        StyleSpec {
            id: "dub_techno",
            prompt: "FULL RESET to dub techno — reverb and delay ARE the music, deep space, long echo tails",
            checks: vec![
                Check::bpm("bpm 118–130", 118.0, 130.0),
                Check::ge("reverb_mix ≥0.50", |s| s.fx.reverb_mix, 0.50),
                Check::ge("delay_feedback ≥0.55", |s| s.fx.delay_feedback, 0.55),
                Check::ge("delay_mix ≥0.30", |s| s.fx.delay_mix, 0.30),
            ],
        },
        StyleSpec {
            id: "ambient_techno",
            prompt: "FULL RESET to ambient techno — machines dreaming, very long reverb, slow filter sweeps, pad texture",
            checks: vec![
                Check::bpm("bpm ≤125", 0.0, 125.0),
                Check::ge("reverb_mix ≥0.60", |s| s.fx.reverb_mix, 0.60),
                Check::ge("reverb_size ≥0.75", |s| s.fx.reverb_size, 0.75),
                Check::ge("bass.decay ≥0.60", |s| s.bass.decay, 0.60),
                Check::bool_eq("an1x pad enabled", |s| s.an1x.enabled, true),
            ],
        },
        StyleSpec {
            id: "ambient_house",
            prompt: "FULL RESET to ambient house — gentle beat, floaty pads, deep reverb, dreamy warmth",
            checks: vec![
                Check::bpm("bpm 100–118", 100.0, 118.0),
                Check::ge("reverb_mix ≥0.40", |s| s.fx.reverb_mix, 0.40),
                Check::ge("reverb_size ≥0.65", |s| s.fx.reverb_size, 0.65),
                Check::bool_eq(
                    "an1x or hoover pad enabled",
                    |s| s.an1x.enabled || s.hoover.enabled,
                    true,
                ),
                Check::new("kick gentle (≤4 active steps)", |s| {
                    let kicks = s
                        .sequencer
                        .drum_patterns
                        .get(&DrumVoice::Kick808)
                        .map(|p| p.iter().filter(|step| step.active).count())
                        .unwrap_or(0);
                    if kicks <= 4 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "{kicks} kick steps — ambient house should have a gentle, sparse kick"
                        ))
                    }
                }),
            ],
        },
        StyleSpec {
            id: "space_ambient",
            prompt: "FULL RESET to space ambient — vast cosmic space, no drums, slowly evolving pad drones, \
                     maximum reverb, barely any rhythm",
            checks: vec![
                Check::ge("reverb_mix ≥0.75", |s| s.fx.reverb_mix, 0.75),
                Check::ge("reverb_size ≥0.88", |s| s.fx.reverb_size, 0.88),
                Check::ge("bass.decay ≥0.80 (slow drone)", |s| s.bass.decay, 0.80),
                Check::bool_eq("an1x pad enabled", |s| s.an1x.enabled, true),
                Check::new("no kick drums", |s| {
                    let kicks = s
                        .sequencer
                        .drum_patterns
                        .get(&DrumVoice::Kick808)
                        .map(|p| p.iter().filter(|step| step.active).count())
                        .unwrap_or(0)
                        + s.sequencer
                            .drum_patterns
                            .get(&DrumVoice::Kick909)
                            .map(|p| p.iter().filter(|step| step.active).count())
                            .unwrap_or(0);
                    if kicks == 0 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "{kicks} kick steps — space ambient should have no percussion"
                        ))
                    }
                }),
            ],
        },
        StyleSpec {
            id: "dark_ambient",
            prompt: "FULL RESET to dark ambient — no kick, no hi-hat, slow bass drone, maximum reverb, cavernous space",
            checks: vec![
                Check::bpm("bpm ≤80", 0.0, 80.0),
                Check::ge("reverb_mix ≥0.70", |s| s.fx.reverb_mix, 0.70),
                Check::ge("reverb_size ≥0.85", |s| s.fx.reverb_size, 0.85),
                Check::ge("bass.decay ≥0.75", |s| s.bass.decay, 0.75),
                Check::new("no kick or hihat", |s| {
                    let active = |voice: DrumVoice| {
                        s.sequencer
                            .drum_patterns
                            .get(&voice)
                            .map(|p| p.iter().filter(|step| step.active).count())
                            .unwrap_or(0)
                    };
                    let percussion = active(DrumVoice::Kick808)
                        + active(DrumVoice::Kick909)
                        + active(DrumVoice::HihatClosed808)
                        + active(DrumVoice::HihatClosed909);
                    if percussion == 0 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "{percussion} kick/hihat steps — dark ambient should be percussion-free"
                        ))
                    }
                }),
            ],
        },
        StyleSpec {
            id: "lo_fi_hip_hop",
            prompt: "FULL RESET to lo-fi hip-hop — warm dusty chill, kick on 1&3, loose snare on 2&4, \
                     slow BPM, tape warmth, no harsh sounds",
            checks: vec![
                Check::bpm("bpm 70–95", 70.0, 95.0),
                Check::new("kick on 1&3 (steps 0 and 8)", |s| {
                    let p = s.sequencer.drum_patterns.get(&DrumVoice::Kick808);
                    let on_1_and_3 = p.map(|p| p[0].active && p[8].active).unwrap_or(false);
                    if on_1_and_3 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail("kick not on 1&3 (steps 0 and 8)".into())
                    }
                }),
                Check::new("snare on 2&4 (steps 4 and 12)", |s| {
                    let snare_on_2_4 = s
                        .sequencer
                        .drum_patterns
                        .get(&DrumVoice::Snare808)
                        .map(|p| p[4].active && p[12].active)
                        .unwrap_or(false)
                        || s.sequencer
                            .drum_patterns
                            .get(&DrumVoice::Snare909)
                            .map(|p| p[4].active && p[12].active)
                            .unwrap_or(false)
                        || s.sequencer
                            .drum_patterns
                            .get(&DrumVoice::Clap909)
                            .map(|p| p[4].active && p[12].active)
                            .unwrap_or(false);
                    if snare_on_2_4 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail("no snare/clap on 2&4 (steps 4 and 12)".into())
                    }
                }),
                Check::new("warm not harsh (low distortion)", |s| {
                    if s.fx.distortion_drive <= 0.2 && s.fx.distortion_mix <= 0.2 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "distortion drive={:.2} mix={:.2} — lo-fi should be warm, not harsh",
                            s.fx.distortion_drive, s.fx.distortion_mix
                        ))
                    }
                }),
            ],
        },
        StyleSpec {
            id: "drum_and_bass",
            prompt: "FULL RESET to drum and bass — fast two-step breakbeat, big snare on 2&4, \
                     intricate hihats, deep Reese sub bass, no 4-on-floor kick",
            checks: vec![
                Check::bpm("bpm 165–185", 165.0, 185.0),
                Check::ge("bass volume ≥0.75", |s| s.bass.volume, 0.75),
                Check::new("kick NOT 4-on-floor (DnB two-step)", |s| {
                    // Pure 4-on-floor = steps 0,4,8,12 only — DnB should deviate
                    let p = s.sequencer.drum_patterns.get(&DrumVoice::Kick808);
                    let is_4otf = p
                        .map(|p| {
                            p[0].active
                                && p[4].active
                                && p[8].active
                                && p[12].active
                                && !p[1].active
                                && !p[2].active
                                && !p[3].active
                        })
                        .unwrap_or(false);
                    if !is_4otf {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail("pure 4-on-floor kick — DnB needs a two-step".into())
                    }
                }),
                Check::new("snare present (DnB has prominent snare)", |s| {
                    let snares = s
                        .sequencer
                        .drum_patterns
                        .get(&DrumVoice::Snare808)
                        .map(|p| p.iter().filter(|step| step.active).count())
                        .unwrap_or(0)
                        + s.sequencer
                            .drum_patterns
                            .get(&DrumVoice::Snare909)
                            .map(|p| p.iter().filter(|step| step.active).count())
                            .unwrap_or(0)
                        + s.sequencer
                            .drum_patterns
                            .get(&DrumVoice::Clap909)
                            .map(|p| p.iter().filter(|step| step.active).count())
                            .unwrap_or(0);
                    if snares >= 2 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "{snares} snare steps — DnB needs a prominent snare"
                        ))
                    }
                }),
                Check::new("busy hihats (≥6 active, DnB rolls)", |s| {
                    let hats = s
                        .sequencer
                        .drum_patterns
                        .get(&DrumVoice::HihatClosed808)
                        .map(|p| p.iter().filter(|step| step.active).count())
                        .unwrap_or(0)
                        + s.sequencer
                            .drum_patterns
                            .get(&DrumVoice::HihatClosed909)
                            .map(|p| p.iter().filter(|step| step.active).count())
                            .unwrap_or(0);
                    if hats >= 6 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "{hats} hihat steps — DnB needs busy rolling hihats (want ≥6)"
                        ))
                    }
                }),
            ],
        },
        StyleSpec {
            id: "jungle",
            prompt: "FULL RESET to jungle — Amen break energy, complex syncopated kick, ghost snares, \
                     deep sparse sub bass, NOT a house beat",
            checks: vec![
                Check::bpm("bpm 155–175", 155.0, 175.0),
                Check::ge(
                    "bass.decay ≤0.45 (tight, punchy)",
                    |s| 0.45 - s.bass.decay,
                    0.0,
                ),
                Check::new("kick NOT 4-on-floor (jungle is syncopated)", |s| {
                    let p = s.sequencer.drum_patterns.get(&DrumVoice::Kick808);
                    let is_4otf = p
                        .map(|p| {
                            p[0].active
                                && p[4].active
                                && p[8].active
                                && p[12].active
                                && !p[1].active
                                && !p[2].active
                                && !p[3].active
                        })
                        .unwrap_or(false);
                    if !is_4otf {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(
                            "pure 4-on-floor kick — jungle needs Amen syncopation".into(),
                        )
                    }
                }),
                Check::new("complex pattern (≥10 total drum steps)", |s| {
                    let total: usize = [
                        DrumVoice::Kick808,
                        DrumVoice::Snare808,
                        DrumVoice::HihatClosed808,
                        DrumVoice::HihatOpen808,
                        DrumVoice::Kick909,
                        DrumVoice::Snare909,
                        DrumVoice::HihatClosed909,
                        DrumVoice::Clap909,
                    ]
                    .iter()
                    .map(|v| {
                        s.sequencer
                            .drum_patterns
                            .get(v)
                            .map(|p| p.iter().filter(|step| step.active).count())
                            .unwrap_or(0)
                    })
                    .sum();
                    if total >= 10 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "{total} total drum steps — jungle Amen break should be complex (want ≥10)"
                        ))
                    }
                }),
            ],
        },
        StyleSpec {
            id: "breakcore",
            prompt: "FULL RESET to breakcore — extreme BPM, chaotic rhythms, mangled breaks",
            checks: vec![
                Check::bpm("bpm ≥180", 180.0, 400.0),
                Check::new("high distortion or complex pattern", |s| {
                    let has_distortion =
                        s.fx.distortion_drive >= 0.1 || s.fx.distortion_mix >= 0.15;
                    if has_distortion {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail("no distortion — breakcore should be chaotic".into())
                    }
                }),
            ],
        },
        StyleSpec {
            id: "baroque_bach",
            prompt: "FULL RESET to Baroque Bach style — dense stepwise piano melody in D minor, \
                     no drums, no bass, classical counterpoint phrasing",
            checks: vec![
                Check::bpm("bpm 70–130 (baroque range)", 70.0, 130.0),
                Check::bool_eq("an1x enabled (piano voice)", |s| s.an1x.enabled, true),
                Check::new("bass silent (no bass in baroque)", |s| {
                    if s.bass.volume <= 0.05 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "bass.volume={:.2} — baroque piano should have no bass line",
                            s.bass.volume
                        ))
                    }
                }),
                Check::new("no kick drums", |s| {
                    let kicks = s
                        .sequencer
                        .drum_patterns
                        .get(&DrumVoice::Kick808)
                        .map(|p| p.iter().filter(|step| step.active).count())
                        .unwrap_or(0)
                        + s.sequencer
                            .drum_patterns
                            .get(&DrumVoice::Kick909)
                            .map(|p| p.iter().filter(|step| step.active).count())
                            .unwrap_or(0);
                    if kicks == 0 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "{kicks} kick steps — Bach doesn't have a 4-on-floor"
                        ))
                    }
                }),
                Check::new("an1x melody is mostly stepwise", |s| {
                    let notes: Vec<u8> = s
                        .sequencer
                        .an1x_pattern
                        .iter()
                        .filter(|step| step.active)
                        .map(|step| step.note)
                        .collect();
                    if notes.len() < 3 {
                        return CheckResult::Fail(format!(
                            "only {} active an1x steps — need at least 3 for a phrase",
                            notes.len()
                        ));
                    }
                    let stepwise = notes
                        .windows(2)
                        .filter(|w| (w[0] as i16 - w[1] as i16).unsigned_abs() <= 5)
                        .count();
                    let ratio = stepwise as f32 / (notes.len() - 1) as f32;
                    if ratio >= 0.55 {
                        CheckResult::Pass
                    } else {
                        CheckResult::Fail(format!(
                            "{:.0}% stepwise motion — want ≥55% (Bach uses conjunct voice leading)",
                            ratio * 100.0
                        ))
                    }
                }),
            ],
        },
        StyleSpec {
            id: "detroit_techno",
            prompt: "FULL RESET to Detroit techno — raw, soulful, spacious, moderate reverb, no hoover",
            checks: vec![
                Check::bpm("bpm 120–140", 120.0, 140.0),
                Check::ge("reverb ≥0.15", |s| s.fx.reverb_mix, 0.15),
                Check::bool_eq("hoover off", |s| s.hoover.enabled, false),
                Check::ge(
                    "bass.decay ≥0.40 (longer than acid)",
                    |s| s.bass.decay,
                    0.40,
                ),
            ],
        },
    ]
}
