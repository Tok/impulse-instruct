// ─── midi/export.rs ──────────────────────────────────────────────────────────
// Standard MIDI File (SMF) writer for the sequencer's current pattern.
//
// Hand-rolled so the crate doesn't need to pull a `midi-file`-style dep.
// Produces a Type 1 SMF with:
//   • track 0 — tempo + time-signature meta
//   • track 1 — merged drums on MIDI channel 10 (GM drum map)
//   • track N — bass on ch 1, hoover on ch 2, an1x on ch 3 (one each)
// Tracks are emitted only when their pattern actually has any active
// step, so empty patterns don't clutter the DAW track lane.
//
// Tick grid: every sequencer step is encoded as a 16th note at
// `PPQ = 480` — so 16 steps = 1 bar (4/4), 32 steps = 2 bars, regardless
// of the voice's polyrhythmic `*_steps` value.  Bass gate length
// (`TB303Step.gate`, 0..1) maps onto the step duration; drums get a
// fixed short gate since drum hits don't have a meaningful sustain.

use crate::state::{DrumVoice, SequencerState, Step, TB303Step};

/// Pulses per quarter note.  480 is the de-facto DAW standard.
pub const PPQ: u16 = 480;
/// Ticks per sequencer step (16th at PPQ=480).
pub const TICKS_PER_STEP: u32 = (PPQ as u32) / 4;
/// Fixed short gate for drum hits.  Drum one-shots don't sustain, but the
/// note still needs an explicit Note-Off to keep DAWs happy.
pub const DRUM_GATE_TICKS: u32 = 24;

/// Map a drum voice to its GM-kit MIDI note number on channel 10.
/// Variants without an obvious GM analogue (Amen, GabberKick) reuse the
/// closest kit note so the exported file auditions sensibly in a DAW.
pub fn drum_voice_to_gm_note(voice: DrumVoice) -> u8 {
    match voice {
        DrumVoice::Kick808 | DrumVoice::Kick909 | DrumVoice::GabberKick => 36, // C1
        DrumVoice::Snare808 | DrumVoice::Snare909 => 38,                       // D1
        DrumVoice::HihatClosed808 | DrumVoice::HihatClosed909 => 42,           // F#1
        DrumVoice::HihatOpen808 | DrumVoice::HihatOpen909 => 46,               // A#1
        DrumVoice::TomHi808 => 50,                                             // D2
        DrumVoice::TomMid808 => 47,                                            // B1
        DrumVoice::TomLo808 => 45,                                             // A1
        DrumVoice::Clap909 => 39,                                              // D#1
        DrumVoice::Rim909 => 37,                                               // C#1
        DrumVoice::Amen => 35,                                                 // B0
    }
}

/// Encode an unsigned integer as an SMF variable-length quantity.
/// 7 bits per byte, MSB (0x80) set on every byte except the last.
pub fn write_vlq(mut v: u32, out: &mut Vec<u8>) {
    // Collect 7-bit groups least-significant first, then emit
    // most-significant first with the continuation bit on every byte
    // except the last.
    let mut groups = [0u8; 5];
    let mut len = 0;
    loop {
        groups[len] = (v & 0x7F) as u8;
        len += 1;
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    for i in (0..len).rev() {
        if i > 0 {
            out.push(groups[i] | 0x80);
        } else {
            out.push(groups[i]);
        }
    }
}

/// Write an MTrk chunk containing `body`, preceded by the "MTrk" tag and
/// a big-endian u32 length.
fn append_track(file: &mut Vec<u8>, body: &[u8]) {
    file.extend_from_slice(b"MTrk");
    file.extend_from_slice(&(body.len() as u32).to_be_bytes());
    file.extend_from_slice(body);
}

/// Append a "track name" meta event (0xFF 0x03) at delta 0.
fn write_track_name(body: &mut Vec<u8>, name: &str) {
    body.push(0); // delta
    body.push(0xFF);
    body.push(0x03);
    write_vlq(name.len() as u32, body);
    body.extend_from_slice(name.as_bytes());
}

/// Track 0 — tempo + time signature + end-of-track.
fn encode_tempo_track(seq: &SequencerState) -> Vec<u8> {
    let mut body = Vec::new();
    write_track_name(&mut body, "tempo");
    // Set tempo: µs per quarter note.
    let us_per_quarter = (60_000_000.0 / seq.bpm.max(1.0)).round() as u32;
    body.push(0); // delta
    body.push(0xFF);
    body.push(0x51);
    body.push(0x03);
    body.push(((us_per_quarter >> 16) & 0xFF) as u8);
    body.push(((us_per_quarter >> 8) & 0xFF) as u8);
    body.push((us_per_quarter & 0xFF) as u8);
    // Time signature: numerator / 2^dd / clocks-per-click / 32nds-per-quarter.
    body.push(0);
    body.push(0xFF);
    body.push(0x58);
    body.push(0x04);
    body.push(seq.time_sig_num.clamp(1, 9));
    body.push(2); // denominator = 2^2 = 4 (always /4)
    body.push(24); // clocks per click (standard)
    body.push(8); // 32nds per quarter (standard)
    // End of Track
    body.push(0);
    body.push(0xFF);
    body.push(0x2F);
    body.push(0x00);
    body
}

/// Build a list of (absolute_tick, event_bytes) pairs for a drum pattern
/// at `note` on channel 10.  Velocity mapped from `Step.velocity` into
/// the 1..127 MIDI range (0 is reserved for note-off).
fn drum_events(pattern: &[Step], steps: usize, note: u8) -> Vec<(u32, Vec<u8>)> {
    let mut events = Vec::new();
    let n = pattern.len().min(steps);
    for (i, step) in pattern.iter().take(n).enumerate() {
        if !step.active {
            continue;
        }
        let vel = (step.velocity.clamp(0.0, 1.0) * 127.0).round().max(1.0) as u8;
        let tick_on = i as u32 * TICKS_PER_STEP;
        let tick_off = tick_on + DRUM_GATE_TICKS;
        events.push((tick_on, vec![0x99, note, vel])); // NoteOn ch10
        events.push((tick_off, vec![0x89, note, 0])); // NoteOff ch10
    }
    events
}

/// Merge every active drum voice into one channel-10 track.
fn encode_drum_track(seq: &SequencerState) -> Vec<u8> {
    let mut events: Vec<(u32, Vec<u8>)> = Vec::new();
    // Deterministic voice ordering (DrumVoice::ALL) so exports are stable.
    for v in DrumVoice::ALL {
        if let Some(pattern) = seq.drum_patterns.get(v)
            && pattern.iter().any(|s| s.active)
        {
            let steps = seq.drum_steps.get(v).copied().unwrap_or(seq.steps);
            events.extend(drum_events(pattern, steps, drum_voice_to_gm_note(*v)));
        }
    }
    // Stable sort so same-tick events keep their relative order (note-off
    // before note-on is already handled by how we emit — offs for step N
    // precede ons for step N+1 since their absolute ticks differ).
    events.sort_by_key(|(t, _)| *t);

    let mut body = Vec::new();
    write_track_name(&mut body, "drums");
    let mut prev_tick = 0u32;
    for (tick, bytes) in &events {
        write_vlq(tick - prev_tick, &mut body);
        body.extend_from_slice(bytes);
        prev_tick = *tick;
    }
    body.push(0);
    body.push(0xFF);
    body.push(0x2F);
    body.push(0x00);
    body
}

/// Encode a TB303-style melodic pattern to one track on `channel`
/// (0-indexed).  Accent modulates velocity; `gate` scales the note
/// length within a step.
fn encode_melodic_track(pattern: &[TB303Step], steps: usize, channel: u8, name: &str) -> Vec<u8> {
    let chan = channel & 0x0F;
    let note_on = 0x90 | chan;
    let note_off = 0x80 | chan;
    let mut events: Vec<(u32, Vec<u8>)> = Vec::new();
    let n = pattern.len().min(steps);
    for (i, step) in pattern.iter().take(n).enumerate() {
        if !step.active || step.note == 0 {
            continue;
        }
        // Velocity: 64 baseline + accent up to +63.  Matches the "accented
        // steps hit harder" feel without losing the dynamics of unaccented
        // hits (previously accent=0 would floor the export at vel=0).
        let vel = (64.0 + step.accent.clamp(0.0, 1.0) * 63.0)
            .round()
            .clamp(1.0, 127.0) as u8;
        let gate = (step.gate.clamp(0.0, 1.0) * TICKS_PER_STEP as f32).max(1.0) as u32;
        let tick_on = i as u32 * TICKS_PER_STEP;
        let tick_off = tick_on + gate;
        events.push((tick_on, vec![note_on, step.note, vel]));
        events.push((tick_off, vec![note_off, step.note, 0]));
    }
    events.sort_by_key(|(t, _)| *t);

    let mut body = Vec::new();
    write_track_name(&mut body, name);
    let mut prev_tick = 0u32;
    for (tick, bytes) in &events {
        write_vlq(tick - prev_tick, &mut body);
        body.extend_from_slice(bytes);
        prev_tick = *tick;
    }
    body.push(0);
    body.push(0xFF);
    body.push(0x2F);
    body.push(0x00);
    body
}

/// Full SMF export of the sequencer's current pattern state.
pub fn export_sequencer_smf(seq: &SequencerState) -> Vec<u8> {
    let has_drums = seq
        .drum_patterns
        .values()
        .any(|p| p.iter().any(|s| s.active));
    let has_bass = seq.bass_pattern.iter().any(|s| s.active);
    let has_hoover = seq.hoover_pattern.iter().any(|s| s.active);
    let has_an1x = seq.an1x_pattern.iter().any(|s| s.active);

    let ntrks = 1
        + u16::from(has_drums)
        + u16::from(has_bass)
        + u16::from(has_hoover)
        + u16::from(has_an1x);

    let mut file = Vec::new();
    // Header: MThd + 6 bytes: format (u16) / ntrks (u16) / division (u16).
    file.extend_from_slice(b"MThd");
    file.extend_from_slice(&6u32.to_be_bytes());
    file.extend_from_slice(&1u16.to_be_bytes()); // format 1
    file.extend_from_slice(&ntrks.to_be_bytes());
    file.extend_from_slice(&PPQ.to_be_bytes());

    append_track(&mut file, &encode_tempo_track(seq));
    if has_drums {
        append_track(&mut file, &encode_drum_track(seq));
    }
    if has_bass {
        append_track(
            &mut file,
            &encode_melodic_track(&seq.bass_pattern, seq.bass_steps, 0, "bass"),
        );
    }
    if has_hoover {
        append_track(
            &mut file,
            &encode_melodic_track(&seq.hoover_pattern, seq.hoover_steps, 1, "hoover"),
        );
    }
    if has_an1x {
        append_track(
            &mut file,
            &encode_melodic_track(&seq.an1x_pattern, seq.an1x_steps, 2, "an1x"),
        );
    }
    file
}

/// Write the current sequencer pattern to `pattern-<unix_secs>.mid` in
/// the current working directory.  Mirrors `save_project`'s naming.
pub fn save_midi_export(seq: &SequencerState) -> Result<std::path::PathBuf, String> {
    save_midi_export_to(seq, None)
}

/// Like `save_midi_export` but lets the caller pick the output path —
/// used by the API endpoint to let scripted demos pin a filename.
pub fn save_midi_export_to(
    seq: &SequencerState,
    path: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    let bytes = export_sequencer_smf(seq);
    let out = match path {
        Some(p) => p.to_path_buf(),
        None => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            std::path::PathBuf::from(format!("pattern-{ts}.mid"))
        }
    };
    std::fs::write(&out, &bytes).map_err(|e| format!("write failed: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{SequencerState, Step, TB303Step};

    // ── VLQ ──────────────────────────────────────────────────────────────

    #[test]
    fn vlq_single_byte_passes_through() {
        for v in [0u32, 1, 0x3F, 0x7F] {
            let mut out = Vec::new();
            write_vlq(v, &mut out);
            assert_eq!(out, vec![v as u8], "vlq({v}) = {out:?}");
        }
    }

    #[test]
    fn vlq_two_bytes_for_0x80_and_up() {
        let mut out = Vec::new();
        write_vlq(0x80, &mut out);
        assert_eq!(out, vec![0x81, 0x00]);
        out.clear();
        write_vlq(0x3FFF, &mut out);
        assert_eq!(out, vec![0xFF, 0x7F]);
    }

    #[test]
    fn vlq_three_bytes_for_0x4000() {
        let mut out = Vec::new();
        write_vlq(0x4000, &mut out);
        assert_eq!(out, vec![0x81, 0x80, 0x00]);
    }

    // ── GM map ───────────────────────────────────────────────────────────

    #[test]
    fn drum_voice_notes_follow_gm_kit() {
        assert_eq!(drum_voice_to_gm_note(DrumVoice::Kick808), 36);
        assert_eq!(drum_voice_to_gm_note(DrumVoice::Kick909), 36);
        assert_eq!(drum_voice_to_gm_note(DrumVoice::GabberKick), 36);
        assert_eq!(drum_voice_to_gm_note(DrumVoice::Snare808), 38);
        assert_eq!(drum_voice_to_gm_note(DrumVoice::HihatClosed808), 42);
        assert_eq!(drum_voice_to_gm_note(DrumVoice::HihatOpen909), 46);
        assert_eq!(drum_voice_to_gm_note(DrumVoice::Clap909), 39);
        assert_eq!(drum_voice_to_gm_note(DrumVoice::Amen), 35);
    }

    // ── SMF shape ────────────────────────────────────────────────────────

    #[test]
    fn empty_pattern_writes_header_plus_tempo_track_only() {
        let seq = SequencerState::default();
        let smf = export_sequencer_smf(&seq);
        // Header: "MThd" + 4-byte len + 6 bytes body (14 total).
        assert_eq!(&smf[0..4], b"MThd");
        assert_eq!(&smf[4..8], &6u32.to_be_bytes());
        // Format 1.
        assert_eq!(&smf[8..10], &1u16.to_be_bytes());
        // ntrks = 1 (tempo only — default pattern has no active steps).
        assert_eq!(&smf[10..12], &1u16.to_be_bytes());
        // Exactly one MTrk chunk follows.
        assert_eq!(&smf[14..18], b"MTrk");
        // Sanity: the file should end with an End-of-Track meta (0xFF 0x2F 0x00).
        let n = smf.len();
        assert_eq!(&smf[n - 3..], &[0xFF, 0x2F, 0x00]);
    }

    #[test]
    fn populated_drum_pattern_emits_channel10_note_on() {
        let mut seq = SequencerState::default();
        // Put one kick hit on step 0 at max velocity.
        let pattern = seq.drum_patterns.get_mut(&DrumVoice::Kick808).unwrap();
        pattern[0] = Step {
            active: true,
            velocity: 1.0,
            probability: 1.0,
            ratchet: 1,
            slice: 0,
        };
        let smf = export_sequencer_smf(&seq);
        // Expect at least one byte sequence of [0x99, 36, 127] (NoteOn ch10
        // + GM kick + full velocity) somewhere after the header.
        let needle = [0x99u8, 36, 127];
        let found = smf.windows(3).any(|w| w == needle);
        assert!(found, "kick NoteOn not found in SMF output");
    }

    #[test]
    fn populated_bass_pattern_emits_channel1_note_on() {
        let mut seq = SequencerState::default();
        // Single bass hit, MIDI note 43 (G2), accent 0.
        seq.bass_pattern[0] = TB303Step {
            active: true,
            note: 43,
            accent: 0.0,
            slide: 0.0,
            gate: 0.5,
            pan: 0.0,
        };
        let smf = export_sequencer_smf(&seq);
        // Accent 0 → velocity 64.
        let needle = [0x90u8, 43, 64];
        let found = smf.windows(3).any(|w| w == needle);
        assert!(found, "bass NoteOn not found in SMF output");
    }

    #[test]
    fn tempo_track_encodes_bpm_as_us_per_quarter() {
        let mut seq = SequencerState::default();
        seq.bpm = 120.0;
        let smf = export_sequencer_smf(&seq);
        // 120 BPM → 500,000 µs per quarter = 0x07 0xA1 0x20.
        let needle = [0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20];
        let found = smf.windows(6).any(|w| w == needle);
        assert!(found, "set-tempo meta not found or BPM mismatched");
    }
}
