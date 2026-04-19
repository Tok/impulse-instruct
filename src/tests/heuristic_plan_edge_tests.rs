// ─── tests/heuristic_plan_edge_tests.rs ──────────────────────────────────────
// Edge cases for `heuristic_plan` beyond the happy paths covered in
// `planner_tests.rs`.  Focus: multi-word bass phrasings (the parser has
// four rows of synonyms per voice), cross-lane-mention punting (the
// heuristic must bail when a prompt names multiple domains), and the
// live-check gate (asking about a disabled voice punts to the LLM
// planner instead of returning a stale lane).

use crate::llm::lanes::LaneKind;
use crate::llm::planner_heuristic::heuristic_plan;
use crate::state::AppState;

fn state_with_bass_voices_enabled(n: usize) -> AppState {
    let mut s = AppState::default();
    for i in 0..n.min(s.bass_voices.len()) {
        s.bass_voices[i].enabled = true;
    }
    s
}

// ─── Multi-word bass phrasings ──────────────────────────────────────────────

#[test]
fn bass_ordinal_word_phrasings_resolve() {
    let s = state_with_bass_voices_enabled(4);
    for (phrase, expected_idx) in [
        ("first bass line", 0),
        ("second bass voice", 1),
        ("third bass is muddy", 2),
        ("fourth bass sub", 3),
    ] {
        let p = heuristic_plan(&s, phrase)
            .unwrap_or_else(|| panic!("phrase {phrase:?} should resolve to a bass voice"));
        assert_eq!(
            p.lanes,
            vec![LaneKind::Bass(expected_idx)],
            "phrase {phrase:?} should resolve to Bass({expected_idx})",
        );
    }
}

#[test]
fn bass_word_number_phrasings_resolve() {
    let s = state_with_bass_voices_enabled(4);
    for (phrase, expected_idx) in [
        ("change bass two", 1),
        ("bass three please", 2),
        ("rework bass four", 3),
    ] {
        let p = heuristic_plan(&s, phrase)
            .unwrap_or_else(|| panic!("phrase {phrase:?} should resolve"));
        assert_eq!(p.lanes, vec![LaneKind::Bass(expected_idx)]);
    }
}

#[test]
fn bass_hash_prefix_phrasings_resolve() {
    let s = state_with_bass_voices_enabled(4);
    let p = heuristic_plan(&s, "fix bass#3").unwrap();
    assert_eq!(p.lanes, vec![LaneKind::Bass(2)]);
    let p = heuristic_plan(&s, "bass #2 louder").unwrap();
    assert_eq!(p.lanes, vec![LaneKind::Bass(1)]);
}

// ─── Live-check gate ────────────────────────────────────────────────────────

#[test]
fn disabled_bass_voice_punts_to_llm_planner() {
    // Only voice 0 is enabled by default.  Asking about voice 2 must
    // punt — returning a stale Bass(2) would silently edit a muted
    // voice behind the user's back.
    let s = AppState::default();
    assert!(
        heuristic_plan(&s, "make bass 3 punchier").is_none(),
        "disabled voice must punt to the LLM planner",
    );
}

// ─── Cross-lane mention punting ─────────────────────────────────────────────

#[test]
fn bass_with_fx_mention_punts() {
    // "bass 2 with more reverb" names both a bass voice and FX — the
    // heuristic is only for narrow single-topic commands, so this must
    // fall through to the real planner.
    let s = state_with_bass_voices_enabled(4);
    assert!(heuristic_plan(&s, "bass 2 with more reverb").is_none());
}

#[test]
fn bass_with_drum_mention_punts() {
    // "bass 1 and 808 kick" mentions two domains.
    let s = state_with_bass_voices_enabled(4);
    assert!(heuristic_plan(&s, "bass 1 and 808 kick louder").is_none());
    assert!(heuristic_plan(&s, "bass 1 + 909 claps").is_none());
}

#[test]
fn fx_with_voice_mention_punts() {
    // "add reverb to the hoover" names both FX and hoover — punt.
    let s = AppState::default();
    assert!(heuristic_plan(&s, "add reverb to the hoover").is_none());
}

#[test]
fn hoover_with_bass_mention_punts() {
    let s = AppState::default();
    assert!(heuristic_plan(&s, "tweak the hoover and the bass").is_none());
}

// ─── FX word list — noun-only detection ─────────────────────────────────────

#[test]
fn fx_keyword_set_covers_expected_nouns() {
    // Each FX noun on its own should route to Fx.  "tape sat" / "tapesat"
    // tests the compound word.  "flutter" tests the tape-flutter family.
    let s = AppState::default();
    for phrase in [
        "chorus please",
        "bitcrush it",
        "more phaser",
        "compressor on the master",
        "tape sat please",
        "tapesat",
        "flutter wobble",
        "distort the bus",
    ] {
        let p = heuristic_plan(&s, phrase)
            .unwrap_or_else(|| panic!("phrase {phrase:?} should resolve to Fx"));
        assert_eq!(
            p.lanes,
            vec![LaneKind::Fx],
            "phrase {phrase:?} should resolve to Fx",
        );
    }
}
