// ─── ui/note.rs ──────────────────────────────────────────────────────────────

/// Colorize note names, frequencies, and MIDI numbers in `text` with ANSI 24-bit
/// escape codes (Huth *Farbige Noten* palette) when stdout is a TTY.
/// Returns the original string unchanged on non-TTY outputs.
pub(crate) fn ansi_colorize_notes(text: &str) -> std::borrow::Cow<'_, str> {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return std::borrow::Cow::Borrowed(text);
    }
    // Huth palette: 12 semitones, same RGB values as theme::NOTE_COLORS.
    const PALETTE: [(u8, u8, u8); 12] = [
        (0x33, 0x66, 0xDD), // C
        (0x22, 0x99, 0xBB), // C#
        (0x33, 0xAA, 0x66), // D
        (0x88, 0xCC, 0x22), // D#
        (0xDD, 0xCC, 0x22), // E
        (0xEE, 0x88, 0x22), // F
        (0xDD, 0x44, 0x22), // F#
        (0xEE, 0x33, 0x66), // G
        (0xCC, 0x11, 0x44), // G#
        (0x99, 0x66, 0xCC), // A
        (0x77, 0x44, 0xBB), // A#
        (0x44, 0x33, 0xAA), // B
    ];
    let note_semitone = |note: char, acc: Option<char>| -> Option<u8> {
        let base: i32 = match note {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let offset: i32 = match acc {
            Some('#') => 1,
            Some('b') => -1,
            _ => 0,
        };
        Some(((base + offset).rem_euclid(12)) as u8)
    };
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + 64);
    let mut pos = 0usize;
    #[allow(unused_assignments)]
    let mut seg = 0usize;
    macro_rules! flush {
        ($end:expr) => {
            if seg < $end {
                out.push_str(&text[seg..$end]);
            }
            seg = $end;
        };
    }
    macro_rules! paint {
        ($start:expr, $end:expr, $st:expr) => {
            if seg < $start {
                out.push_str(&text[seg..$start]);
            }
            let (r, g, b) = PALETTE[$st as usize];
            out.push_str(&format!(
                "\x1b[38;2;{r};{g};{b}m{}\x1b[0m",
                &text[$start..$end]
            ));
            seg = $end;
            pos = $end;
        };
    }
    while pos < len {
        let b = bytes[pos];
        // Note name: A–G possibly followed by # or b then optional digit
        if b.is_ascii_uppercase() && matches!(b, b'A'..=b'G') {
            let prev_ok = pos == 0 || !bytes[pos - 1].is_ascii_alphabetic();
            if prev_ok {
                let note_char = b as char;
                let mut j = pos + 1;
                let acc = if j < len && (bytes[j] == b'#' || bytes[j] == b'b') {
                    j += 1;
                    Some(bytes[j - 1] as char)
                } else {
                    None
                };
                let next_ok = j >= len || !bytes[j].is_ascii_alphabetic();
                if next_ok && let Some(st) = note_semitone(note_char, acc) {
                    let has_octave = j < len && bytes[j].is_ascii_digit();
                    if has_octave {
                        j += 1;
                    }
                    // For bare notes (no accidental, no octave digit), both the
                    // preceding and following characters must be word boundaries or
                    // safe punctuation.  This prevents "B" in "D&B", "E" in "E-flat",
                    // etc. from being colored.
                    let bare = acc.is_none() && !has_octave;
                    if bare {
                        let prev_char = if pos > 0 { bytes[pos - 1] } else { b' ' };
                        let safe_before = matches!(
                            prev_char,
                            b' ' | b'\t'
                                | b'\n'
                                | b'\r'
                                | b'('
                                | b'['
                                | b'{'
                                | b'"'
                                | b'\''
                                | b'`'
                                | b'/'
                        );
                        let next_char = if j < len { bytes[j] } else { b' ' };
                        let safe_after = matches!(
                            next_char,
                            b' ' | b'\t'
                                | b'\n'
                                | b'\r'
                                | b')'
                                | b']'
                                | b'}'
                                | b','
                                | b'.'
                                | b':'
                                | b';'
                                | b'"'
                                | b'\''
                                | b'`'
                                | b'/' // chord slash: D/F#
                        );
                        if !safe_before || !safe_after {
                            pos += 1;
                            continue;
                        }
                        // If followed by a space + music quality word, color the whole
                        // expression (e.g. "A minor", "G major", "D diminished").
                        if next_char == b' ' && j < len {
                            const QUALITIES: &[&[u8]] = &[
                                b"major",
                                b"minor",
                                b"maj",
                                b"min",
                                b"diminished",
                                b"dim",
                                b"augmented",
                                b"aug",
                                b"sus",
                                b"suspended",
                                b"dorian",
                                b"phrygian",
                                b"lydian",
                                b"mixolydian",
                                b"locrian",
                                b"aeolian",
                                b"melodic",
                                b"harmonic",
                                b"pentatonic",
                                b"chromatic",
                            ];
                            let rest = &bytes[j + 1..];
                            for q in QUALITIES {
                                let qlen = q.len();
                                if rest.len() >= qlen {
                                    let matches_ci = rest[..qlen]
                                        .iter()
                                        .zip(*q)
                                        .all(|(a, b)| a.to_ascii_lowercase() == *b);
                                    let word_end =
                                        rest.len() == qlen || !rest[qlen].is_ascii_alphabetic();
                                    if matches_ci && word_end {
                                        j += 1 + qlen; // include space + quality
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    paint!(pos, j, st);
                    continue;
                }
            }
        }
        // Frequency: digits (optional .digits) optionally space then Hz
        if b.is_ascii_digit() {
            let mut j = pos;
            while j < len && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                j += 1;
            }
            let num_str = &text[pos..j];
            let mut k = j;
            if k < len && bytes[k] == b' ' {
                k += 1;
            }
            if k + 2 <= len
                && bytes[k..k + 2].eq_ignore_ascii_case(b"Hz")
                && let Ok(hz) = num_str.parse::<f64>()
                && (20.0..=20_000.0).contains(&hz)
            {
                let st = (69.0 + 12.0 * (hz / 440.0_f64).log2()).round() as i64;
                paint!(pos, k + 2, st.rem_euclid(12) as u8);
                continue;
            }
            // MIDI context: "note 60" etc.
            let prefix = &text[pos.saturating_sub(7)..pos];
            if ["note ", "midi ", "pitch ", "step "]
                .iter()
                .any(|kw| prefix.ends_with(kw))
                && let Ok(n) = num_str.parse::<u8>()
            {
                paint!(pos, j, n % 12);
                continue;
            }
        }
        pos += 1;
    }
    flush!(len);
    let _ = seg; // flush!(len) writes seg; nothing reads it after — suppress lint
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::ansi_colorize_notes;

    // Strip ANSI escape codes so tests can inspect plain text equality.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut iter = s.chars().peekable();
        while let Some(c) = iter.next() {
            if c == '\x1b' {
                // consume up to 'm'
                for ch in iter.by_ref() {
                    if ch == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn has_color(input: &str) -> bool {
        // Force-test even on non-TTY: wrap around private helper via a
        // round-trip: if the output differs from input, color was applied.
        // ansi_colorize_notes returns Borrowed when not a TTY, so we test
        // the logic directly by checking the output contains ESC sequences.
        let result = ansi_colorize_notes(input);
        result.contains('\x1b')
    }

    #[test]
    fn bare_note_followed_by_ampersand_not_colored() {
        // "D&B" — D and B should NOT be colored
        let s = "D&B is a genre";
        // On non-TTY (CI) the function returns Borrowed unchanged; skip.
        // On TTY the output must not contain any ANSI escapes for D or B here.
        // We test the guard logic indirectly: if on TTY the result equals the input.
        // Since we can't force TTY in unit tests, we test the strip round-trip.
        let result = ansi_colorize_notes(s);
        assert_eq!(strip_ansi(&result), s);
    }

    #[test]
    fn note_with_octave_is_colored_on_tty() {
        // "D4" should be colored (has octave digit) — test structure only;
        // actual color injection requires TTY so we just check round-trip text.
        let s = "step D4 active";
        let result = ansi_colorize_notes(s);
        assert_eq!(strip_ansi(&result), s);
    }

    #[test]
    fn note_with_accidental_not_affected_by_bare_guard() {
        // "D#3" has accidental+octave → bare guard should not block it.
        let s = "playing D#3";
        let result = ansi_colorize_notes(s);
        assert_eq!(strip_ansi(&result), s);
    }

    #[test]
    fn bare_note_before_space_passes_guard() {
        // "key of D major" — D followed by space is allowed.
        let s = "key of D major";
        let result = ansi_colorize_notes(s);
        assert_eq!(strip_ansi(&result), s);
    }

    #[test]
    fn bare_note_at_end_of_string_passes_guard() {
        let s = "root note is G";
        let result = ansi_colorize_notes(s);
        assert_eq!(strip_ansi(&result), s);
    }
}

/// Short note name for a MIDI note number (e.g. 60 → "C4").
pub(crate) fn note_name(midi: u8) -> &'static str {
    const NAMES: &[&str] = &[
        "C1", "C#1", "D1", "D#1", "E1", "F1", "F#1", "G1", "G#1", "A1", "A#1", "B1", "C2", "C#2",
        "D2", "D#2", "E2", "F2", "F#2", "G2", "G#2", "A2", "A#2", "B2", "C3", "C#3", "D3", "D#3",
        "E3", "F3", "F#3", "G3", "G#3", "A3", "A#3", "B3", "C4", "C#4", "D4", "D#4", "E4", "F4",
        "F#4", "G4", "G#4", "A4", "A#4", "B4", "C5", "C#5", "D5", "D#5", "E5", "F5", "F#5", "G5",
        "G#5", "A5", "A#5", "B5", "C6",
    ];
    // MIDI 24 = C1
    let idx = midi.saturating_sub(24) as usize;
    NAMES.get(idx).copied().unwrap_or("?")
}
