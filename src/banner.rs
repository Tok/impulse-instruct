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

    // Upper zone: 49 cells.  Black keys are exactly centered at their boundaries.
    #[rustfmt::skip]
    const UPPER: [Cell; 49] = [
        (C,  '|'), (C,  ' '), (C,  ' '), (C,  ' '), (C,  ' '),  // C  wall+exp  (0-4)
        (CS, '|'), (CS, ' '), (CS, ' '), (CS, '|'),               // C# |  |     (5-8)
        (D,  ' '), (D,  ' '), (D,  ' '),                           // D  exposed  (9-11)
        (DS, '|'), (DS, ' '), (DS, ' '), (DS, '|'),               // D# |  |     (12-15)
        (E,  ' '), (E,  ' '), (E,  ' '), (E,  ' '), (E,  ' '),   // E  exposed  (16-20)
        (F,  '|'), (F,  ' '), (F,  ' '), (F,  ' '), (F,  ' '),   // F  wall+exp (21-25)
        (FS, '|'), (FS, ' '), (FS, ' '), (FS, '|'),               // F# |  |     (26-29)
        (G,  ' '), (G,  ' '), (G,  ' '),                           // G  exposed  (30-32)
        (GS, '|'), (GS, ' '), (GS, ' '), (GS, '|'),               // G# |  |     (33-36)
        (A,  ' '), (A,  ' '), (A,  ' '),                           // A  exposed  (37-39)
        (AS, '|'), (AS, ' '), (AS, ' '), (AS, '|'),               // A# |  |     (40-43)
        (B,  ' '), (B,  ' '), (B,  ' '), (B,  ' '), (B,  ' '),   // B  exposed  (44-48)
    ];

    // Lower zone: 49 cells.  Each white key = left '|' + 6 spaces.
    #[rustfmt::skip]
    const LOWER: [Cell; 49] = [
        (C, '|'), (C, ' '), (C, ' '), (C, ' '), (C, ' '), (C, ' '), (C, ' '),
        (D, '|'), (D, ' '), (D, ' '), (D, ' '), (D, ' '), (D, ' '), (D, ' '),
        (E, '|'), (E, ' '), (E, ' '), (E, ' '), (E, ' '), (E, ' '), (E, ' '),
        (F, '|'), (F, ' '), (F, ' '), (F, ' '), (F, ' '), (F, ' '), (F, ' '),
        (G, '|'), (G, ' '), (G, ' '), (G, ' '), (G, ' '), (G, ' '), (G, ' '),
        (A, '|'), (A, ' '), (A, ' '), (A, ' '), (A, ' '), (A, ' '), (A, ' '),
        (B, '|'), (B, ' '), (B, ' '), (B, ' '), (B, ' '), (B, ' '), (B, ' '),
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
    // 49 cells × 2 = 98 chars + 2 indent = 100 — fits most terminals at 120 cols.
    let upper_row: Vec<Cell> = UPPER.iter().chain(UPPER.iter()).copied().collect();
    let lower_row: Vec<Cell> = LOWER.iter().chain(LOWER.iter()).copied().collect();

    let upper = render(&upper_row);
    let lower = render(&lower_row);

    println!();
    for _ in 0..4 { println!("  {upper}"); }   // 4 rows — black key zone
    for _ in 0..3 { println!("  {lower}"); }   // 3 rows — white key bottom
    println!("{RESET}");
}
