// ─── tests/sfz_parser_tests.rs ───────────────────────────────────────────────
// Coverage for the SFZ parser's subset.  Tests stay synthetic — small
// fragments asserted against the parsed `SfzRegion` list — so they
// don't depend on a curated fixture pack.  Sample paths are resolved
// against a synthetic base dir so the assertions are deterministic.

#[cfg(test)]
mod note_parser_tests {
    use crate::state::sfz::parse_note_or_int;

    #[test]
    fn parses_integer_midi() {
        assert_eq!(parse_note_or_int("60"), Some(60));
        assert_eq!(parse_note_or_int("0"), Some(0));
        assert_eq!(parse_note_or_int("127"), Some(127));
    }

    #[test]
    fn parses_natural_note_names() {
        // SFZ convention: c4 = 60.
        assert_eq!(parse_note_or_int("c4"), Some(60));
        assert_eq!(parse_note_or_int("a4"), Some(69));
        assert_eq!(parse_note_or_int("c0"), Some(12));
        assert_eq!(parse_note_or_int("c-1"), Some(0));
        assert_eq!(parse_note_or_int("g9"), Some(127));
    }

    #[test]
    fn parses_sharps_and_flats() {
        // c#4 = 61, db4 = 61.  Accidental is positional only — "bb"
        // root with no accidental ≠ B-flat (same letter).  We test
        // f#, eb, db to be unambiguous.
        assert_eq!(parse_note_or_int("c#4"), Some(61));
        assert_eq!(parse_note_or_int("db4"), Some(61));
        assert_eq!(parse_note_or_int("f#3"), Some(54));
        assert_eq!(parse_note_or_int("eb5"), Some(75));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_note_or_int("zz").is_none());
        assert!(parse_note_or_int("").is_none());
        assert!(parse_note_or_int("c99").is_none()); // out of range
    }
}

#[cfg(test)]
mod parser_tests {
    use std::path::Path;

    use crate::state::sfz::{SfzFilType, SfzLoopMode, parse_sfz};

    #[test]
    fn parses_minimal_single_region() {
        let sfz = "<region> sample=piano_c4.wav lokey=48 hikey=72 pitch_keycenter=60";
        let regions = parse_sfz(sfz, Path::new("/lib"));
        assert_eq!(regions.len(), 1);
        let r = &regions[0];
        assert_eq!(r.sample_path, Path::new("/lib/piano_c4.wav"));
        assert_eq!(r.lokey, 48);
        assert_eq!(r.hikey, 72);
        assert_eq!(r.pitch_keycenter, 60);
    }

    #[test]
    fn parses_note_names_in_keys() {
        let sfz = "<region> sample=a.wav lokey=c3 hikey=c5 pitch_keycenter=a4";
        let regions = parse_sfz(sfz, Path::new("/x"));
        let r = &regions[0];
        assert_eq!(r.lokey, 48);
        assert_eq!(r.hikey, 72);
        assert_eq!(r.pitch_keycenter, 69);
    }

    #[test]
    fn cascades_global_into_groups_into_regions() {
        // Global volume + group lokey + region sample.  Each layer
        // contributes; the region inherits both.
        let sfz = r#"
            <global> volume=-3
            <group> lokey=36 hikey=47
            <region> sample=low.wav pitch_keycenter=42
            <region> sample=mid.wav pitch_keycenter=48 hikey=59
        "#;
        let regions = parse_sfz(sfz, Path::new("/lib"));
        assert_eq!(regions.len(), 2);
        // Both regions inherit the -3 dB global volume.
        assert!((regions[0].volume_db - -3.0).abs() < 1e-5);
        assert!((regions[1].volume_db - -3.0).abs() < 1e-5);
        // Both inherit lokey=36 from the group.
        assert_eq!(regions[0].lokey, 36);
        assert_eq!(regions[1].lokey, 36);
        // Region 0 inherits hikey from group; region 1 overrides it.
        assert_eq!(regions[0].hikey, 47);
        assert_eq!(regions[1].hikey, 59);
    }

    #[test]
    fn second_group_resets_to_global() {
        // The second `<group>` should drop the first group's opcodes
        // and start fresh from the active global state.  Otherwise
        // region 2 would inherit lokey=36 from the previous group.
        let sfz = r#"
            <global> volume=0
            <group> lokey=36 hikey=47
            <region> sample=a.wav
            <group> lokey=60 hikey=72
            <region> sample=b.wav
        "#;
        let regions = parse_sfz(sfz, Path::new("/x"));
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].lokey, 36);
        assert_eq!(regions[1].lokey, 60);
        assert_eq!(regions[1].hikey, 72);
    }

    #[test]
    fn region_overrides_group() {
        let sfz = r#"
            <group> volume=-6 lokey=48 hikey=72
            <region> sample=mid.wav volume=0
        "#;
        let regions = parse_sfz(sfz, Path::new("/x"));
        let r = &regions[0];
        assert_eq!(r.volume_db, 0.0); // region's value wins
        assert_eq!(r.lokey, 48); // inherited
    }

    #[test]
    fn parses_envelope_and_filter_opcodes() {
        let sfz = r#"
            <region> sample=a.wav
            ampeg_attack=0.01 ampeg_decay=0.1 ampeg_sustain=80 ampeg_release=0.4
            cutoff=2000 resonance=3 fil_type=lpf_2p
            tune=15 transpose=-2
        "#;
        let r = &parse_sfz(sfz, Path::new("/x"))[0];
        assert!((r.ampeg_attack_s - 0.01).abs() < 1e-5);
        assert!((r.ampeg_decay_s - 0.1).abs() < 1e-5);
        assert!((r.ampeg_sustain_pct - 80.0).abs() < 1e-5);
        assert!((r.ampeg_release_s - 0.4).abs() < 1e-5);
        assert_eq!(r.cutoff_hz, 2000.0);
        assert_eq!(r.resonance_db, 3.0);
        assert_eq!(r.fil_type, Some(SfzFilType::Lpf2p));
        assert_eq!(r.tune_cents, 15.0);
        assert_eq!(r.transpose, -2);
    }

    #[test]
    fn parses_loop_mode_and_points() {
        let sfz = r#"
            <region> sample=a.wav loop_mode=loop_continuous loop_start=2048 loop_end=88200
        "#;
        let r = &parse_sfz(sfz, Path::new("/x"))[0];
        assert_eq!(r.loop_mode, SfzLoopMode::LoopContinuous);
        assert_eq!(r.loop_start, Some(2048));
        assert_eq!(r.loop_end, Some(88200));
    }

    #[test]
    fn parses_velocity_layers_and_rr() {
        let sfz = r#"
            <region> sample=soft.wav lovel=0 hivel=63
            <region> sample=loud.wav lovel=64 hivel=127
            <region> sample=rr1.wav seq_position=1 seq_length=2
            <region> sample=rr2.wav seq_position=2 seq_length=2
        "#;
        let regions = parse_sfz(sfz, Path::new("/x"));
        assert_eq!(regions.len(), 4);
        assert_eq!(regions[0].lovel, 0);
        assert_eq!(regions[0].hivel, 63);
        assert_eq!(regions[1].lovel, 64);
        assert_eq!(regions[1].hivel, 127);
        assert_eq!(regions[2].seq_position, 1);
        assert_eq!(regions[2].seq_length, 2);
    }

    #[test]
    fn ignores_unknown_opcodes_silently() {
        // Out-of-subset opcodes shouldn't error or drop the region.
        let sfz = r#"
            <region> sample=a.wav offset=100 fil_keytrack=66 random=0.5
        "#;
        let regions = parse_sfz(sfz, Path::new("/x"));
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].sample_path, Path::new("/x/a.wav"));
    }

    #[test]
    fn ignores_unknown_headers_silently() {
        // <effect> / <curve> headers should be skipped — opcodes inside
        // them must not contaminate the next real `<region>`.
        let sfz = r#"
            <effect> bus=main type=apan
            <region> sample=a.wav
        "#;
        let r = &parse_sfz(sfz, Path::new("/x"))[0];
        assert_eq!(r.sample_path, Path::new("/x/a.wav"));
    }

    #[test]
    fn handles_block_and_line_comments() {
        let sfz = r#"
            // header comment
            <region> sample=a.wav // trailing comment
            /* block
               spanning
               multiple lines */
            ampeg_attack=0.05
        "#;
        let r = &parse_sfz(sfz, Path::new("/x"))[0];
        assert_eq!(r.sample_path, Path::new("/x/a.wav"));
        assert!((r.ampeg_attack_s - 0.05).abs() < 1e-5);
    }

    #[test]
    fn sample_paths_with_spaces_round_trip() {
        // SFZ allows spaces in `sample=` values (filename runs until
        // the next opcode key).  Real Salamander packs do this.
        let sfz = "<region> sample=Piano A4.wav lokey=60 hikey=60";
        let r = &parse_sfz(sfz, Path::new("/x"))[0];
        assert_eq!(r.sample_path, Path::new("/x/Piano A4.wav"));
        assert_eq!(r.lokey, 60);
        assert_eq!(r.hikey, 60);
    }

    #[test]
    fn windows_style_backslashes_in_sample_path() {
        // Cross-platform SFZs frequently ship with backslash separators
        // — must normalise so the path resolves on Linux too.
        let sfz = "<region> sample=samples\\piano\\c4.wav";
        let r = &parse_sfz(sfz, Path::new("/lib"))[0];
        // Forward-slash form should match regardless of host platform.
        assert!(
            r.sample_path.ends_with("piano/c4.wav") || r.sample_path.ends_with("piano\\c4.wav")
        );
    }

    #[test]
    fn key_shorthand_sets_lokey_hikey_and_keycenter() {
        let sfz = "<region> sample=a.wav key=c4";
        let r = &parse_sfz(sfz, Path::new("/x"))[0];
        assert_eq!(r.lokey, 60);
        assert_eq!(r.hikey, 60);
        assert_eq!(r.pitch_keycenter, 60);
    }

    #[test]
    fn empty_sfz_yields_no_regions() {
        assert!(parse_sfz("", Path::new("/x")).is_empty());
        assert!(parse_sfz("// only comments", Path::new("/x")).is_empty());
        assert!(parse_sfz("<global> volume=0", Path::new("/x")).is_empty());
    }
}

#[cfg(test)]
mod region_helpers_tests {
    use crate::state::sfz::SfzRegion;

    #[test]
    fn matches_note_respects_inclusive_range() {
        let mut r = SfzRegion {
            lokey: 60,
            hikey: 67,
            ..Default::default()
        };
        r.sample_path = std::path::PathBuf::from("/x/a.wav");
        assert!(r.matches_note(60));
        assert!(r.matches_note(67));
        assert!(!r.matches_note(59));
        assert!(!r.matches_note(68));
    }

    #[test]
    fn matches_velocity_respects_inclusive_range() {
        let mut r = SfzRegion {
            lovel: 64,
            hivel: 127,
            ..Default::default()
        };
        r.sample_path = std::path::PathBuf::from("/x/a.wav");
        assert!(r.matches_velocity(64));
        assert!(r.matches_velocity(127));
        assert!(!r.matches_velocity(63));
    }

    #[test]
    fn is_playable_requires_sample_path() {
        let mut r = SfzRegion::default();
        assert!(!r.is_playable());
        r.sample_path = std::path::PathBuf::from("/x/a.wav");
        assert!(r.is_playable());
    }
}
