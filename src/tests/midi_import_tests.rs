// ─── tests/midi_import_tests.rs ──────────────────────────────────────────────
// Covers the MIDI import pipeline — extraction, voice-picking, auto step
// division, quantisation, bank fill, and the export→import round-trip.
// Fixtures are built in-process so the tests don't depend on any on-disk
// SMF files.  Mirrors the structure of midi_export_tests.rs.

use crate::midi::export::{PPQ, export_sequencer_smf, write_vlq};
use crate::midi::import::{
    ImportSummary, MidiImport, NoteEvent, auto_step_division, extract_bpm, extract_notes,
    import_midi_into, pick_outer_voices, quantise_monophonic,
};
use crate::state::{AppState, MAX_BANKS, MAX_STEPS, SequencerState, Step, TB303Step};
use midly::Smf;

// ─── Synthetic SMF builder ───────────────────────────────────────────────────

/// Append a variable-length quantity (VLQ) — reuses the exporter's writer
/// so we know every test fixture uses canonical deltas.
fn push_vlq(buf: &mut Vec<u8>, v: u32) {
    write_vlq(v, buf);
}

/// Build a minimal Type-1 SMF with the given PPQ and one track per
/// `TrackBlueprint`.  Track 0 is the tempo/meta track.
struct TrackBlueprint {
    name: &'static str,
    notes: Vec<(u32, u32, u8, u8, u8)>, // (on_tick, off_tick, pitch, velocity, channel)
}

fn build_smf(ppq: u16, tempo_bpm: Option<f32>, tracks: &[TrackBlueprint]) -> Vec<u8> {
    let mut file = Vec::new();
    // MThd
    file.extend_from_slice(b"MThd");
    file.extend_from_slice(&6u32.to_be_bytes());
    file.extend_from_slice(&1u16.to_be_bytes()); // format 1
    let ntrks = 1 + tracks.len() as u16; // +1 for the tempo track
    file.extend_from_slice(&ntrks.to_be_bytes());
    file.extend_from_slice(&ppq.to_be_bytes());

    // Tempo track
    let mut tempo_body = Vec::new();
    if let Some(bpm) = tempo_bpm {
        let us_per_q = (60_000_000.0 / bpm) as u32;
        tempo_body.push(0); // delta
        tempo_body.push(0xFF);
        tempo_body.push(0x51);
        tempo_body.push(0x03);
        tempo_body.push(((us_per_q >> 16) & 0xFF) as u8);
        tempo_body.push(((us_per_q >> 8) & 0xFF) as u8);
        tempo_body.push((us_per_q & 0xFF) as u8);
    }
    tempo_body.push(0);
    tempo_body.push(0xFF);
    tempo_body.push(0x2F);
    tempo_body.push(0x00); // EoT

    append_mtrk(&mut file, &tempo_body);

    // Each note track
    for t in tracks {
        let mut body = Vec::new();
        // Track name
        body.push(0);
        body.push(0xFF);
        body.push(0x03);
        push_vlq(&mut body, t.name.len() as u32);
        body.extend_from_slice(t.name.as_bytes());

        // Flatten (on, off) events, sort by tick.
        let mut events: Vec<(u32, Vec<u8>)> = Vec::new();
        for &(on, off, pitch, vel, chan) in &t.notes {
            let chan = chan & 0x0F;
            events.push((on, vec![0x90 | chan, pitch, vel]));
            events.push((off, vec![0x80 | chan, pitch, 0]));
        }
        events.sort_by_key(|(tick, _)| *tick);
        let mut prev = 0u32;
        for (tick, bytes) in &events {
            push_vlq(&mut body, tick - prev);
            body.extend_from_slice(bytes);
            prev = *tick;
        }
        body.push(0);
        body.push(0xFF);
        body.push(0x2F);
        body.push(0x00);
        append_mtrk(&mut file, &body);
    }
    file
}

fn append_mtrk(file: &mut Vec<u8>, body: &[u8]) {
    file.extend_from_slice(b"MTrk");
    file.extend_from_slice(&(body.len() as u32).to_be_bytes());
    file.extend_from_slice(body);
}

// ─── extract_notes / extract_bpm ─────────────────────────────────────────────

#[test]
fn extract_bpm_reads_first_tempo_meta() {
    let smf = build_smf(
        480,
        Some(140.0),
        &[TrackBlueprint {
            name: "rh",
            notes: vec![(0, 120, 60, 100, 0)],
        }],
    );
    let parsed = Smf::parse(&smf).unwrap();
    let bpm = extract_bpm(&parsed).unwrap();
    // 60_000_000 / (60_000_000 / 140) rounded back to bpm — tiny rounding drift.
    assert!((bpm - 140.0).abs() < 0.5, "got {bpm}");
}

#[test]
fn extract_bpm_returns_none_when_no_tempo_meta() {
    // build_smf omits tempo when `None`, so no SetTempo bytes are emitted.
    let smf = build_smf(
        480,
        None,
        &[TrackBlueprint {
            name: "rh",
            notes: vec![(0, 120, 60, 100, 0)],
        }],
    );
    let parsed = Smf::parse(&smf).unwrap();
    assert!(extract_bpm(&parsed).is_none());
}

#[test]
fn extract_notes_pairs_noteon_and_noteoff() {
    let smf = build_smf(
        480,
        Some(120.0),
        &[TrackBlueprint {
            name: "rh",
            notes: vec![(0, 240, 60, 90, 0), (240, 480, 64, 120, 0)],
        }],
    );
    let parsed = Smf::parse(&smf).unwrap();
    let notes = extract_notes(&parsed.tracks[1]);
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].pitch, 60);
    assert_eq!(notes[0].start_tick, 0);
    assert_eq!(notes[0].end_tick, 240);
    assert_eq!(notes[0].velocity, 90);
    assert_eq!(notes[1].pitch, 64);
    assert_eq!(notes[1].start_tick, 240);
    assert_eq!(notes[1].end_tick, 480);
}

// ─── pick_outer_voices ───────────────────────────────────────────────────────

#[test]
fn pick_outer_voices_assigns_higher_pitch_as_rh() {
    let rh: Vec<NoteEvent> = (0..20)
        .map(|i| NoteEvent {
            start_tick: i * 120,
            end_tick: i * 120 + 60,
            pitch: 72,
            velocity: 100,
        })
        .collect();
    let lh: Vec<NoteEvent> = (0..20)
        .map(|i| NoteEvent {
            start_tick: i * 120,
            end_tick: i * 120 + 60,
            pitch: 40,
            velocity: 100,
        })
        .collect();
    // Order the tracks LH-first to prove the pick doesn't just take index 0.
    let tracks = vec![lh, rh];
    let (rh_idx, lh_idx) = pick_outer_voices(&tracks).unwrap();
    assert_eq!(rh_idx, 1, "RH should be the higher-pitch track (index 1)");
    assert_eq!(lh_idx, 0);
}

#[test]
fn pick_outer_voices_none_when_only_one_track_has_notes() {
    let rh = vec![NoteEvent {
        start_tick: 0,
        end_tick: 120,
        pitch: 60,
        velocity: 100,
    }];
    let empty: Vec<NoteEvent> = Vec::new();
    assert!(pick_outer_voices(&[rh, empty]).is_none());
}

// ─── auto_step_division ──────────────────────────────────────────────────────

#[test]
fn auto_step_division_picks_finer_grid_for_faster_onsets() {
    // At PPQ=480, 16th = 120 ticks, 32nd = 60 ticks, 64th = 30 ticks.
    let ev = |tick: u32| NoteEvent {
        start_tick: tick,
        end_tick: tick + 10,
        pitch: 60,
        velocity: 100,
    };
    // 16th grid → div 4
    let sixteenths = vec![ev(0), ev(120), ev(240), ev(360)];
    assert_eq!(auto_step_division(&sixteenths, &[], 480), 4);
    // 32nd grid → div 8
    let thirtyseconds = vec![ev(0), ev(60), ev(120), ev(180)];
    assert_eq!(auto_step_division(&thirtyseconds, &[], 480), 8);
    // 64th grid → div 16
    let sixtyfourths = vec![ev(0), ev(30), ev(60), ev(90)];
    assert_eq!(auto_step_division(&sixtyfourths, &[], 480), 16);
}

#[test]
fn auto_step_division_caps_at_max_step_division() {
    // Tighter-than-64th onsets snap to the 16 ceiling — anything finer
    // eats bank capacity for little audible gain.  At PPQ=480 a
    // min-delta of 8 ticks implies ceil(480/8) = 60 ideal divisions;
    // snapping drops to 16.  A min-delta of 4 would need 120 divs —
    // still capped at 16.
    let ev = |tick: u32| NoteEvent {
        start_tick: tick,
        end_tick: tick + 5,
        pitch: 60,
        velocity: 100,
    };
    let very_fast = vec![ev(0), ev(8), ev(16), ev(24)];
    let very_very_fast = vec![ev(0), ev(4)];
    assert_eq!(auto_step_division(&very_fast, &[], 480), 16);
    assert_eq!(auto_step_division(&very_very_fast, &[], 480), 16);
}

// ─── quantise_monophonic ─────────────────────────────────────────────────────

#[test]
fn quantise_monophonic_keeps_highest_pitch_per_step_when_upper_voice() {
    // Two notes at the same quantised step — upper voice keeps the higher.
    let notes = vec![
        NoteEvent {
            start_tick: 0,
            end_tick: 60,
            pitch: 60,
            velocity: 100,
        },
        NoteEvent {
            start_tick: 10,
            end_tick: 60,
            pitch: 72,
            velocity: 100,
        },
    ];
    let steps = quantise_monophonic(&notes, 120, /*lower_preferred=*/ false);
    assert_eq!(steps[0].note, 72, "upper voice keeps the higher pitch");
}

#[test]
fn quantise_monophonic_keeps_lowest_pitch_per_step_when_lower_voice() {
    let notes = vec![
        NoteEvent {
            start_tick: 0,
            end_tick: 60,
            pitch: 60,
            velocity: 100,
        },
        NoteEvent {
            start_tick: 10,
            end_tick: 60,
            pitch: 48,
            velocity: 100,
        },
    ];
    let steps = quantise_monophonic(&notes, 120, /*lower_preferred=*/ true);
    assert_eq!(steps[0].note, 48, "lower voice keeps the lower pitch");
}

#[test]
fn quantise_monophonic_maps_velocity_to_accent() {
    let notes = vec![
        NoteEvent {
            start_tick: 0,
            end_tick: 60,
            pitch: 60,
            velocity: 127,
        },
        NoteEvent {
            start_tick: 120,
            end_tick: 180,
            pitch: 60,
            velocity: 64,
        },
        NoteEvent {
            start_tick: 240,
            end_tick: 300,
            pitch: 60,
            velocity: 40,
        },
    ];
    let steps = quantise_monophonic(&notes, 120, false);
    assert!(
        steps[0].accent > 0.99,
        "vel 127 → accent ≈ 1.0, got {}",
        steps[0].accent
    );
    assert_eq!(steps[1].accent, 0.0, "vel 64 → accent 0.0");
    assert_eq!(steps[2].accent, 0.0, "vel < 64 floors to accent 0.0");
}

// ─── import_midi_into — end-to-end ───────────────────────────────────────────

#[test]
fn import_fills_bass_voices_0_and_1_with_outer_voices() {
    // Build a simple 2-hand, 4-note-per-hand pattern at 16th resolution.
    // RH: 72, 74, 76, 77 on steps 0,1,2,3 (PPQ=480, 16th=120 ticks).
    // LH: 36, 38, 40, 41 same timing.
    let rh: Vec<(u32, u32, u8, u8, u8)> = [
        (0, 120, 72, 100),
        (120, 240, 74, 100),
        (240, 360, 76, 100),
        (360, 480, 77, 100),
    ]
    .iter()
    .map(|&(on, off, p, v)| (on, off, p, v, 0))
    .collect();
    let lh: Vec<(u32, u32, u8, u8, u8)> = [
        (0, 120, 36, 100),
        (120, 240, 38, 100),
        (240, 360, 40, 100),
        (360, 480, 41, 100),
    ]
    .iter()
    .map(|&(on, off, p, v)| (on, off, p, v, 1))
    .collect();
    let smf = build_smf(
        480,
        Some(120.0),
        &[
            TrackBlueprint {
                name: "rh",
                notes: rh,
            },
            TrackBlueprint {
                name: "lh",
                notes: lh,
            },
        ],
    );

    let state = AppState::default();
    let (new_state, summary) = import_midi_into(state, &smf, &MidiImport::default()).unwrap();

    // Summary: BPM recovered, one bank used, no truncation.
    assert!((summary.bpm - 120.0).abs() < 0.5);
    assert_eq!(summary.banks_used, 1);
    assert!(!summary.was_truncated);
    assert_eq!(summary.notes_voice_0, 4);
    assert_eq!(summary.notes_voice_1, 4);

    // Bank 0 got populated; voices 0 (RH) and 1 (LH) enabled.
    let bank = &new_state.pattern_bank[0];
    assert!(bank.bass_voice_enabled[0]);
    assert!(bank.bass_voice_enabled[1]);
    // Voice 0 carries RH notes.
    assert_eq!(bank.bass_patterns[0][0].note, 72);
    assert_eq!(bank.bass_patterns[0][1].note, 74);
    assert_eq!(bank.bass_patterns[0][2].note, 76);
    assert_eq!(bank.bass_patterns[0][3].note, 77);
    // Voice 1 carries LH notes.
    assert_eq!(bank.bass_patterns[1][0].note, 36);
    assert_eq!(bank.bass_patterns[1][1].note, 38);
    assert_eq!(bank.bass_patterns[1][2].note, 40);
    assert_eq!(bank.bass_patterns[1][3].note, 41);

    // step_division auto-selected to 16ths (4 per beat).
    assert_eq!(bank.step_division, 4);
}

#[test]
fn import_auto_selects_32nd_grid_for_fast_onsets() {
    // Onsets every 60 ticks (32nd at PPQ=480).  The grid should bump to 8.
    let rh: Vec<(u32, u32, u8, u8, u8)> = (0..8)
        .map(|i| (i * 60, i * 60 + 30, 60 + i as u8, 100, 0))
        .collect();
    let lh: Vec<(u32, u32, u8, u8, u8)> = (0..8)
        .map(|i| (i * 60, i * 60 + 30, 40 - i as u8, 100, 1))
        .collect();
    let smf = build_smf(
        480,
        Some(120.0),
        &[
            TrackBlueprint {
                name: "rh",
                notes: rh,
            },
            TrackBlueprint {
                name: "lh",
                notes: lh,
            },
        ],
    );
    let state = AppState::default();
    let (new_state, summary) = import_midi_into(state, &smf, &MidiImport::default()).unwrap();
    assert_eq!(summary.step_division, 8);
    assert_eq!(new_state.pattern_bank[0].step_division, 8);
    // 8 notes in 8 consecutive 32nd steps → fills steps 0..=7 on voice 0.
    for i in 0..8 {
        assert!(
            new_state.pattern_bank[0].bass_patterns[0][i].active,
            "voice 0 step {i} should be active"
        );
    }
}

#[test]
fn import_truncates_beyond_max_banks() {
    // MAX_BANKS × 64 steps is the hard ceiling.  Build one note past the
    // ceiling on a 16th-note grid so the import has to truncate.
    let notes_past_cap = MAX_BANKS * 64 + 1;
    let rh: Vec<(u32, u32, u8, u8, u8)> = (0..notes_past_cap as u32)
        .map(|i| (i * 120, i * 120 + 60, 60, 100, 0))
        .collect();
    let lh: Vec<(u32, u32, u8, u8, u8)> = (0..notes_past_cap as u32)
        .map(|i| (i * 120, i * 120 + 60, 40, 100, 1))
        .collect();
    let smf = build_smf(
        480,
        Some(120.0),
        &[
            TrackBlueprint {
                name: "rh",
                notes: rh,
            },
            TrackBlueprint {
                name: "lh",
                notes: lh,
            },
        ],
    );
    let state = AppState::default();
    let (new_state, summary) = import_midi_into(state, &smf, &MidiImport::default()).unwrap();
    assert!(summary.was_truncated);
    assert_eq!(summary.banks_used, MAX_BANKS);
    // Chain walks all MAX_BANKS slots once.
    assert_eq!(new_state.chain.len(), MAX_BANKS);
    assert!(new_state.chain_enabled);
    for (i, &slot) in new_state.chain.iter().enumerate() {
        assert_eq!(slot, i);
    }
}

#[test]
fn import_disables_chain_loop_for_one_shot_playback() {
    // Imports of definite-ending pieces should play once and stop — not
    // wrap back to bank 0.  Verified by checking chain_loop = false
    // after any multi-bank import.
    let notes: Vec<(u32, u32, u8, u8, u8)> = (0..200)
        .map(|i| (i * 120, i * 120 + 60, 60 + (i % 12) as u8, 100, 0))
        .collect();
    let lh_notes: Vec<(u32, u32, u8, u8, u8)> = (0..200)
        .map(|i| (i * 120, i * 120 + 60, 40 + (i % 12) as u8, 100, 1))
        .collect();
    let smf = build_smf(
        480,
        Some(120.0),
        &[
            TrackBlueprint { name: "rh", notes },
            TrackBlueprint {
                name: "lh",
                notes: lh_notes,
            },
        ],
    );
    let state = AppState::default();
    // Default AppState has chain_loop=true; import must flip it.
    assert!(
        state.chain_loop,
        "default AppState should have chain_loop=true"
    );
    let (new_state, _) = import_midi_into(state, &smf, &MidiImport::default()).unwrap();
    assert!(
        !new_state.chain_loop,
        "MIDI import must disable chain_loop for one-shot playback"
    );
}

#[test]
fn import_builds_single_slot_chain_without_enabling_chain_mode() {
    // One bank is enough; chain_enabled should stay false so the
    // sequencer just loops the live pattern (not the chain).
    let rh: Vec<(u32, u32, u8, u8, u8)> = (0..4)
        .map(|i| (i * 120, i * 120 + 60, 60, 100, 0))
        .collect();
    let lh: Vec<(u32, u32, u8, u8, u8)> = (0..4)
        .map(|i| (i * 120, i * 120 + 60, 40, 100, 1))
        .collect();
    let smf = build_smf(
        480,
        Some(120.0),
        &[
            TrackBlueprint {
                name: "rh",
                notes: rh,
            },
            TrackBlueprint {
                name: "lh",
                notes: lh,
            },
        ],
    );
    let state = AppState::default();
    let (new_state, summary) = import_midi_into(state, &smf, &MidiImport::default()).unwrap();
    assert_eq!(summary.banks_used, 1);
    assert!(!new_state.chain_enabled);
    assert_eq!(new_state.chain.len(), 1);
}

#[test]
fn import_missing_second_voice_errors() {
    // Only a single non-empty track.
    let smf = build_smf(
        480,
        Some(120.0),
        &[TrackBlueprint {
            name: "rh",
            notes: vec![(0, 120, 60, 100, 0)],
        }],
    );
    let state = AppState::default();
    let err = import_midi_into(state, &smf, &MidiImport::default()).unwrap_err();
    assert!(err.contains("no two"), "got {err}");
}

// ─── round-trip ──────────────────────────────────────────────────────────────

#[test]
fn export_import_round_trip_preserves_voice0_notes() {
    // Build a sequencer state with voice-0 notes, export to SMF, then
    // re-import into a fresh state and check the notes come back.  Needs
    // at least two voices on disk because the importer wants two tracks;
    // we populate the hoover pattern as a stand-in for voice 1.
    let mut seq = SequencerState::default();
    // Voice 0: bass C major triad on steps 0, 4, 8.
    for (step, note) in [(0usize, 60u8), (4, 64), (8, 67)] {
        seq.bass_pattern[step] = TB303Step {
            active: true,
            note,
            accent: 0.0,
            slide: 0.0,
            gate: 0.5,
            pan: 0.0,
        };
    }
    // Hoover: single note to force a second exported track.
    seq.hoover_pattern[0] = TB303Step {
        active: true,
        note: 72,
        accent: 0.0,
        slide: 0.0,
        gate: 0.5,
        pan: 0.0,
    };
    seq.bass_steps = 16;
    seq.hoover_steps = 16;

    let smf = export_sequencer_smf(&seq);
    assert_eq!(&smf[0..4], b"MThd", "export should produce a valid SMF");

    let state = AppState {
        sequencer: seq.clone(),
        ..AppState::default()
    };
    let (new_state, summary) = import_midi_into(state, &smf, &MidiImport::default()).unwrap();
    // PPQ=480 / TICKS_PER_STEP=120 → 16th grid, so step_division=4.
    assert_eq!(summary.step_division, 4);
    // Voice 0 (RH, higher mean pitch) should be the hoover track (single
    // note at pitch 72); voice 1 (LH) should be the bass track (three
    // notes at pitches 60/64/67, all lower than 72).
    let bank = &new_state.pattern_bank[0];
    assert_eq!(bank.bass_patterns[0][0].note, 72, "voice 0 = hoover");
    assert_eq!(bank.bass_patterns[1][0].note, 60, "voice 1 step 0 = C");
    assert_eq!(bank.bass_patterns[1][4].note, 64, "voice 1 step 4 = E");
    assert_eq!(bank.bass_patterns[1][8].note, 67, "voice 1 step 8 = G");
}

// ─── sanity helpers ──────────────────────────────────────────────────────────

#[test]
fn import_summary_fields_are_coherent() {
    // Simple fixture → summary totals should line up with the fixture.
    let rh: Vec<(u32, u32, u8, u8, u8)> = [(0, 120, 60, 100), (120, 240, 62, 100)]
        .iter()
        .map(|&(on, off, p, v)| (on, off, p, v, 0))
        .collect();
    let lh: Vec<(u32, u32, u8, u8, u8)> = [(0, 120, 40, 100), (120, 240, 42, 100)]
        .iter()
        .map(|&(on, off, p, v)| (on, off, p, v, 1))
        .collect();
    let smf = build_smf(
        480,
        Some(100.0),
        &[
            TrackBlueprint {
                name: "rh",
                notes: rh,
            },
            TrackBlueprint {
                name: "lh",
                notes: lh,
            },
        ],
    );
    let state = AppState::default();
    let (_, summary): (AppState, ImportSummary) =
        import_midi_into(state, &smf, &MidiImport::default()).unwrap();
    assert!((summary.bpm - 100.0).abs() < 0.5);
    assert_eq!(summary.picked_tracks, (1, 2)); // track 0 is the tempo track
    assert_eq!(summary.notes_voice_0, 2);
    assert_eq!(summary.notes_voice_1, 2);
    assert!(summary.source_ticks >= 240);
}

// Silence the unused import warning when PPQ is referenced above for
// documentation but not directly inside the test bodies.
#[allow(dead_code)]
const _PPQ_DOC: u16 = PPQ;
#[allow(dead_code)]
const _STEP_DOC: usize = MAX_STEPS;
#[allow(dead_code)]
fn _unused_step(_: Step) {}

// ─── Smoke test against a real Bach SMF (ignored by default) ─────────────────
// Run explicitly with `cargo test -- --ignored bach_piano_smoke_test`.  Skipped
// in the default suite because it depends on files in demo/scenarios/Bach-MIDI
// which may not exist on every checkout.

#[test]
#[ignore]
fn bach_piano_smoke_test() {
    let path = std::path::Path::new("demo/scenarios/bach-italian-3rd.mid");
    if !path.exists() {
        eprintln!("skipping: {} not present", path.display());
        return;
    }
    let bytes = std::fs::read(path).unwrap();
    let state = AppState::default();
    let (new_state, summary) = import_midi_into(state, &bytes, &MidiImport::default()).unwrap();
    eprintln!("{:#?}", summary);
    assert!(summary.banks_used >= 1);
    assert!(summary.notes_voice_0 > 0, "voice 0 should have notes");
    assert!(summary.notes_voice_1 > 0, "voice 1 should have notes");
    assert!(new_state.pattern_bank[0].bass_voice_enabled[0]);
    assert!(new_state.pattern_bank[0].bass_voice_enabled[1]);
    assert!(
        !new_state.chain_loop,
        "MIDI import should disable chain_loop"
    );
}
