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

    // ── Huth-colored keyboard (2 octaves C3–C5) ──────────────────────────────
    //
    // Layout: 5 terminal cells per white key, 3 cells per black key.
    // Each cell = 1 char with ANSI 24-bit background color.
    //
    // UPPER ROW (35 cells per octave, printed ×3 for black-key height):
    //   '|' chars appear at:
    //     pos  0  — C left wall          (aligns with C left wall in lower zone)
    //     pos  5  — C# right wall        (aligns with D left wall in lower zone)
    //     pos 10  — D# right wall        (aligns with E left wall in lower zone)
    //     pos 15  — F left wall          (aligns with F left wall in lower zone)
    //     pos 20  — F# right wall        (aligns with G left wall in lower zone)
    //     pos 25  — G# right wall        (aligns with A left wall in lower zone)
    //     pos 30  — A# right wall        (aligns with B left wall in lower zone)
    //   Black keys: left-wall | space | right-wall (3 cells).
    //   E (pos 11-14) and B (pos 31-34) are plain exposed cells — their
    //   boundaries are marked by adjacent walls (D# right at 10, F left at 15,
    //   A# right at 30, and C left of the next octave at 0).
    //
    // LOWER ROW (35 cells per octave, printed ×2):
    //   Each white key = left-wall '|' + 4 spaces.
    //   Final C5 cap is the same (no trailing '|').

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

    // Upper zone: 35 cells per octave.
    // Alignment: '|' columns in upper zone land on the same horizontal positions
    // as '|' left-walls in the lower zone, creating clean vertical columns.
    #[rustfmt::skip]
    const UPPER: [Cell; 35] = [
        (C,  '|'), (C,  ' '), (C,  ' '),            // C: left wall + 2 exp   (0-2)
        (CS, '|'), (CS, ' '), (CS, '|'),             // C# walls               (3-5)   → col 5 = D left
        (D,  ' '), (D,  ' '),                         // D: exposed             (6-7)
        (DS, '|'), (DS, ' '), (DS, '|'),              // D# walls               (8-10)  → col 10 = E left
        (E,  ' '), (E,  ' '), (E,  ' '), (E,  ' '),  // E: 4 exposed           (11-14)
        (F,  '|'), (F,  ' '), (F,  ' '),              // F: left wall + 2 exp   (15-17) → col 15 = F left
        (FS, '|'), (FS, ' '), (FS, '|'),              // F# walls               (18-20) → col 20 = G left
        (G,  ' '), (G,  ' '),                         // G: exposed             (21-22)
        (GS, '|'), (GS, ' '), (GS, '|'),              // G# walls               (23-25) → col 25 = A left
        (A,  ' '), (A,  ' '),                         // A: exposed             (26-27)
        (AS, '|'), (AS, ' '), (AS, '|'),              // A# walls               (28-30) → col 30 = B left
        (B,  ' '), (B,  ' '), (B,  ' '), (B,  ' '),  // B: 4 exposed           (31-34)
    ];

    // Lower zone: 35 cells per octave.  Each white key = left '|' + 4 spaces.
    #[rustfmt::skip]
    const LOWER: [Cell; 35] = [
        (C, '|'), (C, ' '), (C, ' '), (C, ' '), (C, ' '),   // C  (0-4)
        (D, '|'), (D, ' '), (D, ' '), (D, ' '), (D, ' '),   // D  (5-9)
        (E, '|'), (E, ' '), (E, ' '), (E, ' '), (E, ' '),   // E  (10-14)
        (F, '|'), (F, ' '), (F, ' '), (F, ' '), (F, ' '),   // F  (15-19)
        (G, '|'), (G, ' '), (G, ' '), (G, ' '), (G, ' '),   // G  (20-24)
        (A, '|'), (A, ' '), (A, ' '), (A, ' '), (A, ' '),   // A  (25-29)
        (B, '|'), (B, ' '), (B, ' '), (B, ' '), (B, ' '),   // B  (30-34)
    ];

    // C5 terminators.  Upper: left wall + 2 exposed (same shape as C in UPPER).
    // Lower: left wall + 4 spaces, no trailing '|' — keyboard just ends cleanly.
    const UPPER_C5: [Cell; 3] = [(C, '|'), (C, ' '), (C, ' ')];
    const LOWER_C5: [Cell; 5] = [(C, '|'), (C, ' '), (C, ' '), (C, ' '), (C, ' ')];

    // Render a cell slice: spaces → bg only; '|' → dark fg on key bg.
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

    let upper_row: Vec<Cell> = UPPER.iter()
        .chain(UPPER.iter())
        .chain(UPPER_C5.iter())
        .copied()
        .collect();
    let lower_row: Vec<Cell> = LOWER.iter()
        .chain(LOWER.iter())
        .chain(LOWER_C5.iter())
        .copied()
        .collect();

    let upper = render(&upper_row);
    let lower = render(&lower_row);

    println!();
    for _ in 0..3 { println!("  {upper}"); }   // 3 rows — taller black keys
    for _ in 0..2 { println!("  {lower}"); }   // 2 rows — white key bottom
    println!("{RESET}");
}
