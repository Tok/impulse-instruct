// ─── banner.rs ────────────────────────────────────────────────────────────────
// Terminal startup banner: clean title + tagline + Huth-colored keyboard.
// Pure function — only side effect is printing to stdout.

pub fn print_banner() {
    const GRAY: &str  = "\x1b[38;2;160;160;160m";
    const DIM:  &str  = "\x1b[38;2;50;50;50m";
    const RESET: &str = "\x1b[0m";

    // ── Title & tagline ───────────────────────────────────────────────────────
    println!();
    println!("{GRAY}  I M P U L S E   I N S T R U C T{RESET}");
    println!("{DIM}  a synthesizer with a tiny LLM living inside of it  ·  rust  ·  llama.cpp{RESET}");

    // ── Huth-colored keyboard (3 octaves) ────────────────────────────────────
    //
    // Layout: W=5 cells per white key, B=3 cells per black key.
    //   White key = '|' (left wall) + 4 spaces  → 5 cells
    //   Black key = '|' (left wall) + ' ' + '|' → 3 cells
    //   1 octave = 35 cells total.
    //   3 octaves = 105 chars + 2 indent = 107 — requires ~110 terminal columns.
    //
    // Black keys are near-centered on white key boundaries.
    // With W=5 and B=3, exact centering is impossible (half-integer boundaries)
    // so each black key is offset 0.5 cells to the right of its boundary:
    //   C/D boundary 4.5 → C# left=4 (center 5, boundary 4.5 — 0.5 right bias)
    //   D/E boundary 9.5 → D# left=9 (center 10)
    //   F/G boundary 19.5 → F# left=19 (center 20)
    //   G/A boundary 24.5 → G# left=24 (center 25)
    //   A/B boundary 29.5 → A# left=29 (center 30)
    //
    // Exposed cells per note in UPPER zone:
    //   C=4  D=2  E=3  |  F=4  G=2  A=2  B=3
    //
    // Height: upper ×4 rows (black key body), lower ×2 + bottom ×1 = 7 rows total.

    type Rgb  = (u8, u8, u8);
    type Cell = (Rgb, char);

    const C:  Rgb = (0x33, 0x66, 0xDD);
    const CS: Rgb = (0x22, 0x99, 0xBB);
    const D:  Rgb = (0x33, 0xAA, 0x66);
    const DS: Rgb = (0x88, 0xCC, 0x22);
    const E:  Rgb = (0xDD, 0xCC, 0x22);
    const F:  Rgb = (0xEE, 0x88, 0x22);
    const FS: Rgb = (0xDD, 0x44, 0x22);
    const G:  Rgb = (0xEE, 0x33, 0x66);
    const GS: Rgb = (0xCC, 0x11, 0x44);
    const A:  Rgb = (0x99, 0x66, 0xCC);
    const AS: Rgb = (0x77, 0x44, 0xBB);
    const B:  Rgb = (0x44, 0x33, 0xAA);

    // Upper zone: 35 cells per octave (W=5 white, B=3 black).
    #[rustfmt::skip]
    const UPPER: [Cell; 35] = [
        // ── 2-key group ──────────────────────────────────────────────────────
        (C,  '|'), (C,  ' '), (C,  ' '), (C,  ' '),     // C  wall+3  (0-3)
        (CS, '|'), (CS, ' '), (CS, '|'),                  // C# (4-6)
        (D,  ' '), (D,  ' '),                             // D  2 exp   (7-8)
        (DS, '|'), (DS, ' '), (DS, '|'),                  // D# (9-11)
        (E,  ' '), (E,  ' '), (E,  ' '),                 // E  3 exp   (12-14)
        // ── 3-key group ──────────────────────────────────────────────────────
        (F,  '|'), (F,  ' '), (F,  ' '), (F,  ' '),     // F  wall+3  (15-18)
        (FS, '|'), (FS, ' '), (FS, '|'),                  // F# (19-21)
        (G,  ' '), (G,  ' '),                             // G  2 exp   (22-23)
        (GS, '|'), (GS, ' '), (GS, '|'),                  // G# (24-26)
        (A,  ' '), (A,  ' '),                             // A  2 exp   (27-28)
        (AS, '|'), (AS, ' '), (AS, '|'),                  // A# (29-31)
        (B,  ' '), (B,  ' '), (B,  ' '),                 // B  3 exp   (32-34)
    ];

    // Lower zone: 35 cells.  Each white key = left '|' + 4 spaces (W=5).
    #[rustfmt::skip]
    const LOWER: [Cell; 35] = [
        (C, '|'), (C, ' '), (C, ' '), (C, ' '), (C, ' '),
        (D, '|'), (D, ' '), (D, ' '), (D, ' '), (D, ' '),
        (E, '|'), (E, ' '), (E, ' '), (E, ' '), (E, ' '),
        (F, '|'), (F, ' '), (F, ' '), (F, ' '), (F, ' '),
        (G, '|'), (G, ' '), (G, ' '), (G, ' '), (G, ' '),
        (A, '|'), (A, ' '), (A, ' '), (A, ' '), (A, ' '),
        (B, '|'), (B, ' '), (B, ' '), (B, ' '), (B, ' '),
    ];

    // Bottom edge row: '|' wall + 4 underscores → visual floor on each white key.
    #[rustfmt::skip]
    const LOWER_BTM: [Cell; 35] = [
        (C, '|'), (C, '_'), (C, '_'), (C, '_'), (C, '_'),
        (D, '|'), (D, '_'), (D, '_'), (D, '_'), (D, '_'),
        (E, '|'), (E, '_'), (E, '_'), (E, '_'), (E, '_'),
        (F, '|'), (F, '_'), (F, '_'), (F, '_'), (F, '_'),
        (G, '|'), (G, '_'), (G, '_'), (G, '_'), (G, '_'),
        (A, '|'), (A, '_'), (A, '_'), (A, '_'), (A, '_'),
        (B, '|'), (B, '_'), (B, '_'), (B, '_'), (B, '_'),
    ];

    // Render a cell slice: spaces → bg only; any other char → dark fg on key bg.
    let render = |cells: &[Cell]| -> String {
        let mut s = String::with_capacity(cells.len() * 24);
        for &((r, g, b), ch) in cells {
            if ch == ' ' {
                s.push_str(&format!("\x1b[48;2;{r};{g};{b}m \x1b[0m"));
            } else {
                s.push_str(&format!(
                    "\x1b[48;2;{r};{g};{b}m\x1b[38;2;14;14;14m{ch}\x1b[0m"
                ));
            }
        }
        s
    };

    // Three octaves: chain arrays × 3.
    // 35 cells × 3 = 105 chars + 2 indent = 107 — fits in 110+ column terminals.
    let upper_row:  Vec<Cell> = UPPER.iter().chain(UPPER.iter()).chain(UPPER.iter()).copied().collect();
    let lower_row:  Vec<Cell> = LOWER.iter().chain(LOWER.iter()).chain(LOWER.iter()).copied().collect();
    let bottom_row: Vec<Cell> = LOWER_BTM.iter().chain(LOWER_BTM.iter()).chain(LOWER_BTM.iter()).copied().collect();

    let upper  = render(&upper_row);
    let lower  = render(&lower_row);
    let bottom = render(&bottom_row);

    println!();
    for _ in 0..4 { println!("  {upper}"); }   // 4 rows — black key zone
    for _ in 0..2 { println!("  {lower}"); }   // 2 rows — white key body
    println!("  {bottom}");                     // 1 row  — white key floor (|____)
    println!("{RESET}");
}
