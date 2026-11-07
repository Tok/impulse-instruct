// ─── log_fmt.rs ──────────────────────────────────────────────────────────────
// ANSI colorization for the shell log — mirrors the in-UI log styling:
//   line-level grayscale by prefix/content (CHALK/HAZE/FOG/SMOKE/ASH) +
//   per-note Huth *Farbige Noten* highlights.
// Only emitted when stderr is a TTY; otherwise the output is plain ASCII.

use std::io::IsTerminal;
use std::sync::OnceLock;

const RESET: &str = "\x1b[0m";

// Grayscale line colors — kept in sync with src/ui/theme.rs.
const CHALK: &str = "\x1b[38;2;235;235;235m";
const HAZE: &str = "\x1b[38;2;210;210;210m";
const FOG: &str = "\x1b[38;2;175;175;175m";
const SMOKE: &str = "\x1b[38;2;130;130;130m";
const ASH: &str = "\x1b[38;2;90;90;90m";

// Huth note colors — kept in sync with src/ui/theme.rs::NOTE_COLORS.
const NOTE_RGB: [(u8, u8, u8); 12] = [
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

fn color_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        std::io::stderr().is_terminal()
    })
}

fn line_color(line: &str) -> &'static str {
    if line.contains("(thinking):") {
        HAZE
    } else if line.starts_with("► ") {
        CHALK
    } else if line.starts_with("YOU ") {
        FOG
    } else if line.starts_with('[') && !line.contains("[API]") {
        ASH
    } else if starts_with_persona(line) {
        CHALK
    } else {
        SMOKE
    }
}

/// Detect a leading persona tag like `PULSE: ...` or `BASS: ...` — first token
/// is ALL CAPS (A–Z, digits, underscore) and ends with a colon followed by a
/// space.
pub fn starts_with_persona(line: &str) -> bool {
    let b = line.as_bytes();
    if b.is_empty() || !b[0].is_ascii_uppercase() {
        return false;
    }
    let mut i = 0;
    while i < b.len() && (b[i].is_ascii_uppercase() || b[i].is_ascii_digit() || b[i] == b'_') {
        i += 1;
    }
    i > 0 && i + 1 < b.len() && b[i] == b':' && b[i + 1] == b' '
}

fn note_semitone(c: char, acc: Option<char>) -> Option<u8> {
    let base: i8 = match c {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let off: i8 = match acc {
        Some('#') => 1,
        Some('b') => -1,
        _ => 0,
    };
    Some(((base + off).rem_euclid(12)) as u8)
}

pub(crate) const IGNORE_PRECEDING_WORDS: &[&[u8]] = &[b"kit", b"pad", b"part", b"bank", b"slot"];
pub(crate) const QUALITIES: &[&[u8]] = &[
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

/// Scan a single line and return `(start, end, semitone)` spans for every
/// recognised note reference (`C4`, `A#3`, `440Hz`, `note 60`, or bare
/// punctuation-bounded letter like `G minor`).
fn note_spans(line: &str) -> Vec<(usize, usize, u8)> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut out: Vec<(usize, usize, u8)> = Vec::new();
    let mut pos = 0usize;
    while pos < len {
        let b = bytes[pos];

        // ── Note letter A–G ────────────────────────────────────────────────
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
                        // Reject `E4B`, `C4_K_M.gguf` etc. — after the octave
                        // digit we still need a word boundary (non-alpha).
                        // Without this check, model filenames colour as notes.
                        if j < len && bytes[j].is_ascii_alphabetic() {
                            pos += 1;
                            continue;
                        }
                    }
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
                                | b'/'
                        );
                        if !safe_before || !safe_after {
                            pos += 1;
                            continue;
                        }
                        // Skip "Kit A", "Bank B", etc.
                        let mut ws = pos.saturating_sub(1);
                        while ws > 0 && matches!(bytes[ws], b' ' | b'\t') {
                            ws -= 1;
                        }
                        let word_end = ws + 1;
                        let mut word_start = word_end;
                        while word_start > 0 && bytes[word_start - 1].is_ascii_alphabetic() {
                            word_start -= 1;
                        }
                        let prev_word = &bytes[word_start..word_end];
                        let skip = IGNORE_PRECEDING_WORDS.iter().any(|ign| {
                            prev_word.len() == ign.len()
                                && prev_word
                                    .iter()
                                    .zip(*ign)
                                    .all(|(a, b)| a.to_ascii_lowercase() == *b)
                        });
                        if skip {
                            pos += 1;
                            continue;
                        }
                        // Extend for "A minor", "G dorian", etc.
                        if next_char == b' ' && j < len {
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
                                        j += 1 + qlen;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    out.push((pos, j, st));
                    pos = j;
                    continue;
                }
            }
        }

        // ── Frequency (N[.N] Hz) ───────────────────────────────────────────
        // Require a non-digit/non-dot word boundary BEFORE the number so we
        // don't match e.g. "4100 Hz" inside "44100 Hz" by re-entering at the
        // second digit.  Also no upper Hz cap — the semitone-class mapping
        // wraps cleanly so 44100 Hz colours as F (its octave-equivalent
        // semitone), not as the embedded 4100 Hz blue.
        if b.is_ascii_digit() {
            let prev_word_break =
                pos == 0 || !(bytes[pos - 1].is_ascii_digit() || bytes[pos - 1] == b'.');
            if !prev_word_break {
                pos += 1;
                continue;
            }
            let mut j = pos;
            while j < len && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                j += 1;
            }
            let num_str = &line[pos..j];
            let mut k = j;
            if k < len && bytes[k] == b' ' {
                k += 1;
            }
            if k + 2 <= len
                && bytes[k..k + 2].eq_ignore_ascii_case(b"Hz")
                && let Ok(hz) = num_str.parse::<f64>()
                && hz >= 20.0
            {
                let midi = crate::audio::dsp::hz_to_midi(hz as f32);
                let st = (midi.round() as i64).rem_euclid(12) as u8;
                out.push((pos, k + 2, st));
                pos = k + 2;
                continue;
            }
            // ── MIDI number context ("note 60", "midi 72", etc.) ───────────
            let mut prefix_start = pos.saturating_sub(7);
            while prefix_start > 0 && !line.is_char_boundary(prefix_start) {
                prefix_start -= 1;
            }
            let prefix = &line[prefix_start..pos];
            let is_midi_ctx = ["note ", "midi ", "pitch ", "step "]
                .iter()
                .any(|kw| prefix.ends_with(kw));
            if is_midi_ctx && let Ok(n) = num_str.parse::<u8>() {
                out.push((pos, j, n % 12));
                pos = j;
                continue;
            }
        }

        pos += 1;
    }
    out
}

/// Produce an ANSI-colored version of `line`. When `stderr` is not a TTY,
/// `NO_COLOR` is set, or the line contains no recognised markup, returns the
/// line unchanged.
pub fn colorize(line: &str) -> String {
    if !color_enabled() {
        return line.to_string();
    }
    let base = line_color(line);
    let spans = note_spans(line);
    if spans.is_empty() {
        return format!("{}{}{}", base, line, RESET);
    }
    let mut out = String::with_capacity(line.len() + 32 + spans.len() * 24);
    out.push_str(base);
    let mut cursor = 0usize;
    for (s, e, st) in spans {
        if cursor < s {
            out.push_str(&line[cursor..s]);
        }
        let (r, g, b) = NOTE_RGB[st as usize];
        out.push_str(&format!("\x1b[38;2;{};{};{}m", r, g, b));
        out.push_str(&line[s..e]);
        out.push_str(base); // restore line base color
        cursor = e;
    }
    if cursor < line.len() {
        out.push_str(&line[cursor..]);
    }
    out.push_str(RESET);
    out
}

#[cfg(test)]
mod tests {
    use super::note_spans;

    fn spans(s: &str) -> Vec<(usize, usize)> {
        note_spans(s).into_iter().map(|(a, b, _)| (a, b)).collect()
    }

    #[test]
    fn model_filename_does_not_match_e4() {
        // E4B sits inside a model filename — no notes should be detected.
        let s = "loading models/gemma-4-E4B-it-Q4_K_M.gguf";
        assert!(spans(s).is_empty(), "unexpected spans: {:?}", spans(s));
    }

    #[test]
    fn freq_44100_matches_full_token_not_embedded() {
        let s = "sample rate 44100 Hz active";
        let v = spans(s);
        assert_eq!(v, vec![(12, 20)], "got {:?}", v); // "44100 Hz" full
    }

    #[test]
    fn freq_4100_still_matches_at_boundary() {
        let s = "tone at 4100 Hz";
        let v = spans(s);
        assert_eq!(v, vec![(8, 15)], "got {:?}", v);
    }

    #[test]
    fn standalone_c4_still_colors() {
        let s = "play C4 next";
        let v = spans(s);
        assert_eq!(v, vec![(5, 7)]);
    }

    #[test]
    fn bare_g_minor_still_colors() {
        let s = "key of G minor";
        let v = spans(s);
        assert_eq!(v, vec![(7, 14)]);
    }
}
