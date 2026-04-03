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
    println!("{DIM}  a synthesizer with a tiny LLM living inside it  ·  rust  ·  llama.cpp{RESET}");

    // ── Huth-colored keyboard (1 octave C3–B3) ───────────────────────────────
    //
    // Layout: 7 terminal cells per white key, 4 cells per black key.
    //   White key = '|' (left wall) + 6 spaces  → 7 cells
    //   Black key = '|' (left wall) + 2 spaces + '|' (right wall) → 4 cells
    //   1 octave = 49 cells total; fits well within 80 columns.
    //
    // Black key centering: with W=7 and B=4, each black key center lands
    // exactly on the white key boundary (left = boundary − 2).
    //   boundary formula: n*W + W − 1  →  e.g. C/D = 6.5
    //   C# left = 6.5 − 2 = 4.5 → pos 5   (center 6.5 ✓)
    //   D# left = 13.5 − 2       → pos 12  (center 13.5 ✓)
    //   F# left = 27.5 − 2       → pos 26  (center 27.5 ✓)
    //   G# left = 34.5 − 2       → pos 33  (center 34.5 ✓)
    //   A# left = 41.5 − 2       → pos 40  (center 41.5 ✓)
    //
    // Exposed regions per note in the UPPER zone:
    //   C=5  D=3  E=5  |  F=5  G=3  A=3  B=5
    //
    // Height: upper zone ×4 rows (black key area), lower zone ×3 rows (white key
    // bottom) = 7 rows total; black keys at 57% of height, close to real piano.

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

    // Upper zone: 42 cells per octave  (W=6 white key, B=4 black key).
    //
    // White key boundaries at half-integers: C/D=5.5, D/E=11.5, F/G=23.5, G/A=29.5, A/B=35.5
    //
    // Real piano offset — black keys are NOT centered on their boundaries:
    //
    //   2-key group (C#, D#): lean outward to widen D's gap
    //     C# lean −1 (toward C): center 4.5,  left = 3  → C# right wall pos 6 = D left ✓
    //     D# lean +1 (toward E): center 12.5, left = 11 → D gets 4 exposed cells
    //
    //   3-key group (F#, G#, A#): F# left, G# centered, A# right
    //     F# lean −1 (toward F): center 22.5, left = 21 → F# right wall pos 24 = G left ✓
    //     G# centered at G/A boundary: center 29.5, left = 28
    //     A# lean +1 (toward B): center 36.5, left = 35
    //     → F, G, A, B each get 3 exposed cells
    //
    // Exposed cells per note: C=3  D=4  E=3 | F=3  G=3  A=3  B=3
    #[rustfmt::skip]
    const UPPER: [Cell; 42] = [
        // ── 2-key group ──────────────────────────────────────────────────────
        (C,  '|'), (C,  ' '), (C,  ' '),                          // C  3 exp  (0-2)
        (CS, '|'), (CS, ' '), (CS, ' '), (CS, '|'),               // C# −1    (3-6)   → col 6 = D left
        (D,  ' '), (D,  ' '), (D,  ' '), (D,  ' '),              // D  4 exp  (7-10)
        (DS, '|'), (DS, ' '), (DS, ' '), (DS, '|'),               // D# +1    (11-14)
        (E,  ' '), (E,  ' '), (E,  ' '),                          // E  3 exp  (15-17)
        // ── 3-key group ──────────────────────────────────────────────────────
        (F,  '|'), (F,  ' '), (F,  ' '),                          // F  3 exp  (18-20)
        (FS, '|'), (FS, ' '), (FS, ' '), (FS, '|'),               // F# −1    (21-24)  → col 24 = G left
        (G,  ' '), (G,  ' '), (G,  ' '),                          // G  3 exp  (25-27)
        (GS, '|'), (GS, ' '), (GS, ' '), (GS, '|'),               // G# ctr   (28-31)
        (A,  ' '), (A,  ' '), (A,  ' '),                          // A  3 exp  (32-34)
        (AS, '|'), (AS, ' '), (AS, ' '), (AS, '|'),               // A# +1    (35-38)
        (B,  ' '), (B,  ' '), (B,  ' '),                          // B  3 exp  (39-41)
    ];

    // Lower zone: 42 cells.  Each white key = left '|' + 5 spaces (W=6).
    #[rustfmt::skip]
    const LOWER: [Cell; 42] = [
        (C, '|'), (C, ' '), (C, ' '), (C, ' '), (C, ' '), (C, ' '),
        (D, '|'), (D, ' '), (D, ' '), (D, ' '), (D, ' '), (D, ' '),
        (E, '|'), (E, ' '), (E, ' '), (E, ' '), (E, ' '), (E, ' '),
        (F, '|'), (F, ' '), (F, ' '), (F, ' '), (F, ' '), (F, ' '),
        (G, '|'), (G, ' '), (G, ' '), (G, ' '), (G, ' '), (G, ' '),
        (A, '|'), (A, ' '), (A, ' '), (A, ' '), (A, ' '), (A, ' '),
        (B, '|'), (B, ' '), (B, ' '), (B, ' '), (B, ' '), (B, ' '),
    ];

    // Bottom edge row: '|' wall + 5 underscores → visual floor on each white key.
    #[rustfmt::skip]
    const LOWER_BTM: [Cell; 42] = [
        (C, '|'), (C, '_'), (C, '_'), (C, '_'), (C, '_'), (C, '_'),
        (D, '|'), (D, '_'), (D, '_'), (D, '_'), (D, '_'), (D, '_'),
        (E, '|'), (E, '_'), (E, '_'), (E, '_'), (E, '_'), (E, '_'),
        (F, '|'), (F, '_'), (F, '_'), (F, '_'), (F, '_'), (F, '_'),
        (G, '|'), (G, '_'), (G, '_'), (G, '_'), (G, '_'), (G, '_'),
        (A, '|'), (A, '_'), (A, '_'), (A, '_'), (A, '_'), (A, '_'),
        (B, '|'), (B, '_'), (B, '_'), (B, '_'), (B, '_'), (B, '_'),
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

    // Two octaves: chain UPPER/LOWER with themselves.
    // 42 cells × 2 = 84 chars + 2 indent = 86 — fits comfortably within 80 cols.
    let upper_row:  Vec<Cell> = UPPER.iter().chain(UPPER.iter()).copied().collect();
    let lower_row:  Vec<Cell> = LOWER.iter().chain(LOWER.iter()).copied().collect();
    let bottom_row: Vec<Cell> = LOWER_BTM.iter().chain(LOWER_BTM.iter()).copied().collect();

    let upper  = render(&upper_row);
    let lower  = render(&lower_row);
    let bottom = render(&bottom_row);

    println!();
    for _ in 0..4 { println!("  {upper}"); }   // 4 rows — black key zone
    for _ in 0..2 { println!("  {lower}"); }   // 2 rows — white key body
    println!("  {bottom}");                     // 1 row  — white key floor (|______)
    println!("{RESET}");
}
