// ─── tests/seed_patterns_tests.rs ────────────────────────────────────────────
// Covers `SeedPatterns::is_empty` + `SeedPatterns::to_prompt_lines` — the
// formatter that injects a style's 16-step starter drum+bass patterns
// into the LLM prompt.  Output format is part of the implicit contract
// with the model; subtle format drift will break the LLM's pattern-seed
// comprehension without any compile error.

use crate::llm::styles::SeedPatterns;

#[test]
fn is_empty_when_all_vecs_are_empty() {
    let sp = SeedPatterns::default();
    assert!(sp.is_empty());
}

#[test]
fn is_empty_false_when_any_voice_present() {
    let mut sp = SeedPatterns::default();
    sp.kick = vec![1, 0, 0, 0];
    assert!(!sp.is_empty());
    let mut sp = SeedPatterns::default();
    sp.bass_steps = vec![1];
    assert!(!sp.is_empty());
}

#[test]
fn prompt_lines_omit_absent_voices() {
    // Only `kick` is set — `snare`, `hihat`, `bass_steps`, `bass_notes`
    // are all empty.  The rendered output should have exactly one line
    // (no `snare:` / `hihat:` / etc. stubs).
    let sp = SeedPatterns {
        kick: vec![1, 0, 0, 0],
        ..Default::default()
    };
    let out = sp.to_prompt_lines();
    assert!(out.starts_with("kick:"));
    assert!(!out.contains("snare:"));
    assert!(!out.contains("hihat:"));
    assert!(!out.contains("bass_steps:"));
}

#[test]
fn prompt_lines_format_bits_as_bracketed_csv_of_ones_and_zeros() {
    let sp = SeedPatterns {
        kick: vec![1, 0, 1, 0],
        ..Default::default()
    };
    let out = sp.to_prompt_lines();
    assert!(
        out.contains("[1,0,1,0]"),
        "kick line should render as [1,0,1,0], got {out:?}",
    );
}

#[test]
fn prompt_lines_map_non_one_values_to_zero() {
    // `fmt_bits` treats any value that isn't literally `1u8` as `0` — a
    // pattern of `[2, 3, 1, 0]` must serialise as `[0,0,1,0]`.  This is
    // intentional so the LLM always sees a binary grid.
    let sp = SeedPatterns {
        kick: vec![2, 3, 1, 0],
        ..Default::default()
    };
    let out = sp.to_prompt_lines();
    assert!(
        out.contains("[0,0,1,0]"),
        "non-1 values should collapse to 0, got {out:?}",
    );
}

#[test]
fn prompt_lines_bass_notes_include_both_midi_ints_and_note_names() {
    // bass_notes output is dual-rendered: MIDI ints in brackets, then a
    // parenthesised space-separated list of note-name equivalents.
    // Steps with bass_steps==0 render as ".." placeholders.
    let sp = SeedPatterns {
        bass_steps: vec![1, 0, 1, 0],
        bass_notes: vec![36, 36, 48, 48], // C2, C2, C3, C3
        ..Default::default()
    };
    let out = sp.to_prompt_lines();
    // MIDI int list must appear.
    assert!(out.contains("[36,36,48,48]"), "got: {out}");
    // Note-name list: active steps → names, inactive → "..".
    assert!(out.contains("C2 .. C3 .."), "got: {out}");
}

#[test]
fn prompt_lines_multiple_voices_on_separate_lines() {
    let sp = SeedPatterns {
        kick: vec![1, 0, 0, 0],
        snare: vec![0, 0, 1, 0],
        hihat: vec![1, 1, 1, 1],
        bass_steps: vec![1, 0, 0, 0],
        bass_notes: vec![],
    };
    let out = sp.to_prompt_lines();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "one line per present voice, got {lines:?}",);
    assert!(lines[0].starts_with("kick:"));
    assert!(lines[1].starts_with("snare:"));
    assert!(lines[2].starts_with("hihat:"));
    assert!(lines[3].starts_with("bass_steps:"));
}
