// ─── ui/llm_log_color.rs ── Huth-colored LayoutJob for the LLM log pane ──────
// Extracted from ui/llm_strip.rs.

use crate::ui::theme;

/// Parse `text` and return a LayoutJob where note references are colored with
/// Huth *Farbige Noten* colors.
///
/// Recognized patterns:
/// • Note+octave: `C4`, `A#3`, `Bb2` etc.  (`[A-G][#b]?\d`)
/// • Plain note name at a word boundary: `C`, `G#`, `Bb` etc.
///   (only when the note letter is NOT surrounded by other letters)
/// • Frequency: `440Hz`, `261.6 Hz` etc. — mapped to nearest chromatic semitone
/// • MIDI number context: `note 60`, `midi 72`, `pitch 48`
pub(super) fn colorize_log(text: &str, _default_color: egui::Color32) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};

    let mut job = LayoutJob::default();
    let font = egui::FontId::monospace(13.0);

    // Per-line base color, prioritizing importance:
    //   agent speak  — CHALK  (near white, most important)
    //   agent think  — HAZE   (bright, slightly dimmer than speak)
    //   user prompt  — FOG    (mid bright)
    //   api / system — SMOKE  (mid)
    let line_color_at = |p: usize, bytes: &[u8], text: &str| -> egui::Color32 {
        let end = bytes[p..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| p + i)
            .unwrap_or(bytes.len());
        let line = &text[p..end];
        if line.contains("(thinking):") {
            theme::HAZE
        } else if line.starts_with("► ") {
            theme::CHALK
        } else if line.starts_with("YOU ") {
            theme::FOG
        } else if line.starts_with('[') && !line.contains("[API]") {
            theme::ASH
        } else if crate::log_fmt::starts_with_persona(line) {
            theme::CHALK
        } else {
            theme::SMOKE
        }
    };

    // Semitone index 0..12 for a note letter + optional accidental.
    let note_semitone = |c: char, acc: Option<char>| -> Option<u8> {
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
        let offset: i8 = match acc {
            Some('#') => 1,
            Some('b') => -1,
            _ => 0,
        };
        Some(((base + offset).rem_euclid(12)) as u8)
    };

    // Hz → chromatic semitone 0..12
    let freq_semitone = |hz: f64| -> u8 {
        let midi = 69.0 + 12.0 * (hz / 440.0_f64).log2();
        (midi.round() as i64).rem_euclid(12) as u8
    };

    // MIDI note number → chromatic semitone 0..12
    let midi_semitone = |n: u8| -> u8 { n % 12 };

    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize; // current byte offset
    let mut seg = 0usize; // start of pending plain segment
    // Base color for the current line (updated on each newline).
    let mut cur_line_color = line_color_at(0, bytes, text);

    // Flush a plain segment from `seg` to `end` using the current line color.
    macro_rules! flush {
        ($end:expr) => {
            if seg < $end {
                job.append(
                    &text[seg..$end],
                    0.0,
                    TextFormat {
                        font_id: font.clone(),
                        color: cur_line_color,
                        ..Default::default()
                    },
                );
                seg = $end;
            }
        };
    }

    // Append a colored span (Huth color), preceded by any pending plain segment.
    // Inlines the flush to avoid a seg write that would be immediately overwritten.
    macro_rules! colored {
        ($start:expr, $end:expr, $semitone:expr) => {
            if seg < $start {
                job.append(
                    &text[seg..$start],
                    0.0,
                    TextFormat {
                        font_id: font.clone(),
                        color: cur_line_color,
                        ..Default::default()
                    },
                );
            }
            job.append(
                &text[$start..$end],
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: theme::NOTE_COLORS[$semitone as usize],
                    ..Default::default()
                },
            );
            seg = $end;
            pos = $end;
        };
    }

    while pos < len {
        let b = bytes[pos];

        // On newline, flush the current line and update color for the next line.
        if b == b'\n' {
            flush!(pos + 1);
            pos += 1;
            if pos < len {
                cur_line_color = line_color_at(pos, bytes, text);
            }
            continue;
        }

        // ── Note name (ASCII A–G) ─────────────────────────────────────────────
        if b.is_ascii_uppercase() && matches!(b, b'A'..=b'G') {
            // Require word-start: pos==0 or previous byte not alphabetic
            let prev_ok = pos == 0 || !bytes[pos - 1].is_ascii_alphabetic();
            if prev_ok {
                let note_char = b as char;
                let mut j = pos + 1;
                // Optional accidental (lowercase b for flat, # for sharp)
                let acc = if j < len && (bytes[j] == b'#' || bytes[j] == b'b') {
                    j += 1;
                    Some(bytes[j - 1] as char)
                } else {
                    None
                };
                // Word-end required: next byte must not be alphabetic (handles CTRL, BPM etc.)
                let next_ok = j >= len || !bytes[j].is_ascii_alphabetic();
                if next_ok && let Some(st) = note_semitone(note_char, acc) {
                    // Optionally consume a trailing octave digit (makes C4, A#3 etc.)
                    let has_octave = j < len && bytes[j].is_ascii_digit();
                    if has_octave {
                        j += 1;
                        // Reject `E4B`, `C4_K_M.gguf` etc. — after the octave
                        // digit we still need a word boundary (non-alpha).
                        if j < len && bytes[j].is_ascii_alphabetic() {
                            pos += 1;
                            continue;
                        }
                    }
                    // For bare notes (no accidental, no octave), require safe punctuation
                    // on both sides — prevents "D" in "D&B", "E" in "E-flat", etc.
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
                        // Ignore-list: if the bare letter is the target of a
                        // non-musical label like "Kit A" / "Kit B", skip the
                        // Huth coloring.  The letter is technically a valid
                        // note name but in this context it's just an ID.
                        //
                        // The preceding character is already known to be a
                        // word-boundary (safe_before), so look back past it
                        // to find the previous word.
                        const IGNORE_PRECEDING_WORDS: &[&[u8]] = &[
                            b"kit",  // "Kit A", "Kit B"
                            b"pad",  // "Pad A", "Pad B" (future-proofing)
                            b"part", // "Part A", "Part B"
                            b"bank", // "Bank A", "Bank B"
                            b"slot", // "Slot A", "Slot B"
                        ];
                        let mut ws = pos.saturating_sub(1); // pos-1 is the word-boundary char
                        while ws > 0 && matches!(bytes[ws], b' ' | b'\t') {
                            ws -= 1;
                        }
                        // ws now points at the last char of the preceding word (or is 0).
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
                        // Extend span to cover "A minor", "G major", etc.
                        if next_char == b' ' && j < len {
                            let rest = &bytes[j + 1..];
                            for q in crate::log_fmt::QUALITIES {
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
                    colored!(pos, j, st);
                    continue;
                }
            }
        }

        // ── Frequency: digits (optional dot+digits) optionally space then Hz ─
        // Word boundary required before the number — otherwise "44100 Hz"
        // would re-match starting at the second '4' as "4100 Hz" (blue).
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
            let num_str = &text[pos..j];
            let mut k = j;
            if k < len && bytes[k] == b' ' {
                k += 1;
            }
            // No upper Hz cap — semitone class wraps cleanly so 44100 Hz
            // colours by its octave-equivalent semitone, not blue at the 4.
            if k + 2 <= len
                && bytes[k..k + 2].eq_ignore_ascii_case(b"Hz")
                && let Ok(hz) = num_str.parse::<f64>()
                && hz >= 20.0
            {
                let st = freq_semitone(hz);
                colored!(pos, k + 2, st);
                continue;
            }
            // ── MIDI number context: "note 60", "midi 72", "pitch 48" ─────────
            let prefix_end = pos;
            let mut prefix_start = pos.saturating_sub(7);
            // Ensure we don't slice inside a multi-byte UTF-8 character
            while prefix_start > 0 && !text.is_char_boundary(prefix_start) {
                prefix_start -= 1;
            }
            let prefix = &text[prefix_start..prefix_end];
            let is_midi_ctx = ["note ", "midi ", "pitch ", "step "]
                .iter()
                .any(|kw| prefix.ends_with(kw));
            if is_midi_ctx && let Ok(n) = num_str.parse::<u8>() {
                let st = midi_semitone(n);
                colored!(pos, j, st);
                continue;
            }
        }

        pos += 1;
    }
    flush!(len);
    let _ = seg; // last flush writes seg but nothing reads it after
    job
}

#[cfg(test)]
mod tests {
    use super::colorize_log;
    use crate::ui::theme;

    /// Collect all distinct colors used in the LayoutJob (excluding the default FOG).
    fn colored_spans(text: &str) -> Vec<(String, egui::Color32)> {
        let job = colorize_log(text, theme::FOG);
        job.sections
            .iter()
            .filter(|s| s.format.color != theme::FOG && s.format.color != theme::SMOKE)
            .map(|s| {
                let range = s.byte_range.clone();
                (text[range].to_string(), s.format.color)
            })
            .collect()
    }

    fn has_note_color(text: &str) -> bool {
        let job = colorize_log(text, theme::FOG);
        job.sections
            .iter()
            .any(|s| theme::NOTE_COLORS.iter().any(|&nc| nc == s.format.color))
    }

    #[test]
    fn dnb_not_colored() {
        assert!(
            !has_note_color("D&B is a genre"),
            "D in D&B must not be colored"
        );
        assert!(
            !has_note_color("listen to D&B"),
            "D in D&B must not be colored"
        );
    }

    #[test]
    fn e_flat_not_colored() {
        assert!(!has_note_color("E-flat"), "E in E-flat must not be colored");
    }

    #[test]
    fn kit_a_b_not_colored() {
        // "Kit A" / "Kit B" are non-musical module IDs — the A/B must
        // stay uncolored even though they're valid note letters.
        assert!(
            !has_note_color("added Kit A to the rack"),
            "Kit A — the A must not be colored"
        );
        assert!(
            !has_note_color("Kit B snare on 4"),
            "Kit B — the B must not be colored"
        );
        // Case insensitivity.
        assert!(
            !has_note_color("kit a hihat rolls"),
            "kit a (lowercase) — a must not be colored"
        );
        // Other ignore-words.
        assert!(!has_note_color("Pad A layered"));
        assert!(!has_note_color("Bank B active"));
        assert!(!has_note_color("Slot F loaded"));
    }

    #[test]
    fn note_with_accidental_is_colored() {
        assert!(has_note_color("play D#3"), "D#3 should be colored");
        assert!(has_note_color("root is Gb"), "Gb should be colored");
    }

    #[test]
    fn note_with_octave_is_colored() {
        assert!(has_note_color("note C4"), "C4 should be colored");
    }

    #[test]
    fn bare_note_at_word_boundary_is_colored() {
        assert!(
            has_note_color("root note is G"),
            "bare G at end should be colored"
        );
        assert!(
            has_note_color("key of D major"),
            "D in D major should be colored"
        );
    }

    #[test]
    fn quality_expression_colored_as_one_span() {
        let spans = colored_spans("key of A minor");
        assert_eq!(spans.len(), 1, "A minor should be one colored span");
        assert_eq!(spans[0].0, "A minor");
    }

    #[test]
    fn bare_note_before_punctuation_is_colored() {
        assert!(
            has_note_color("chord: G,"),
            "G before comma should be colored"
        );
        assert!(has_note_color("(G)"), "G in parens should be colored");
    }

    #[test]
    fn hz_is_colored() {
        assert!(has_note_color("440 Hz"), "440 Hz should be colored");
        assert!(has_note_color("440Hz"), "440Hz should be colored");
    }

    #[test]
    fn midi_context_is_colored() {
        assert!(has_note_color("note 60"), "note 60 should be colored");
        assert!(has_note_color("midi 69"), "midi 69 should be colored");
    }
}
