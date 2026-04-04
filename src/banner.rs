// ─── banner.rs ────────────────────────────────────────────────────────────────
// Terminal startup banner: clean title + tagline + Huth-colored keyboard.
// Pure function — only side effect is printing to stdout.

pub fn print_banner() {
    const GRAY: &str = "\x1b[38;2;160;160;160m";
    const DIM: &str = "\x1b[38;2;50;50;50m";
    const RESET: &str = "\x1b[0m";

    // ── Title & tagline ───────────────────────────────────────────────────────
    println!();
    println!("{GRAY}  I M P U L S E   I N S T R U C T{RESET}");
    println!(
        "{DIM}  a synthesizer with a tiny LLM living inside of it  ·  rust  ·  llama.cpp{RESET}"
    );

    // ── Huth-colored keyboard (2 octaves, 7 rows) ────────────────────────────
    //
    // Layout: W=5 cells per white key, B=3 cells per black key.
    //   White key  = '│' (left wall) + 3 spaces + '│' (right wall) → 5 cells
    //   Black key  = '│' (left wall) + ' ' + '│' (right wall)      → 3 cells
    //   1 octave   = 35 cells total.
    //   2 octaves  = 70 chars + 2 indent = 72 — fits within 80 columns.
    //
    // Height layout (7 rows):
    //   Rows 1–3  UPPER     — black key body (│ │)
    //   Row  4    UPPER_BTM — black key floor (└─┘), white key walls continue
    //   Rows 5–6  LOWER     — white key body (│   │)
    //   Row  7    LOWER_BTM — white key floor (└───┘)

    type Rgb = (u8, u8, u8);
    type Cell = (Rgb, char);

    const C: Rgb = (0x33, 0x66, 0xDD);
    const CS: Rgb = (0x22, 0x99, 0xBB);
    const D: Rgb = (0x33, 0xAA, 0x66);
    const DS: Rgb = (0x88, 0xCC, 0x22);
    const E: Rgb = (0xDD, 0xCC, 0x22);
    const F: Rgb = (0xEE, 0x88, 0x22);
    const FS: Rgb = (0xDD, 0x44, 0x22);
    const G: Rgb = (0xEE, 0x33, 0x66);
    const GS: Rgb = (0xCC, 0x11, 0x44);
    const A: Rgb = (0x99, 0x66, 0xCC);
    const AS: Rgb = (0x77, 0x44, 0xBB);
    const B: Rgb = (0x44, 0x33, 0xAA);

    // Rows 1–3: black key bodies (│ │), white key walls + exposed space.
    // White keys have │ on left only here — right wall comes from adjacent black key.
    #[rustfmt::skip]
    const UPPER: [Cell; 35] = [
        // ── 2-key group (C D E) ──────────────────────────────────────────────
        (C,  '│'), (C,  ' '), (C,  ' '), (C,  ' '),     // C  wall+3  (0-3)
        (CS, '│'), (CS, ' '), (CS, '│'),                  // C# body    (4-6)
        (D,  ' '), (D,  ' '),                             // D  2 exp   (7-8)
        (DS, '│'), (DS, ' '), (DS, '│'),                  // D# body    (9-11)
        (E,  ' '), (E,  ' '), (E,  '│'),                 // E  2 exp + right wall (12-14)
        // ── 3-key group (F G A B) ────────────────────────────────────────────
        (F,  '│'), (F,  ' '), (F,  ' '), (F,  ' '),     // F  wall+3  (15-18)
        (FS, '│'), (FS, ' '), (FS, '│'),                  // F# body    (19-21)
        (G,  ' '), (G,  ' '),                             // G  2 exp   (22-23)
        (GS, '│'), (GS, ' '), (GS, '│'),                  // G# body    (24-26)
        (A,  ' '), (A,  ' '),                             // A  2 exp   (27-28)
        (AS, '│'), (AS, ' '), (AS, '│'),                  // A# body    (29-31)
        (B,  ' '), (B,  ' '), (B,  '│'),                 // B  2 exp + right wall (32-34)
    ];

    // Row 4: black key floor (└─┘ closes each black key), white key walls continue.
    #[rustfmt::skip]
    const UPPER_BTM: [Cell; 35] = [
        (C,  '│'), (C,  ' '), (C,  ' '), (C,  ' '),     // C  wall+3
        (CS, '└'), (CS, '─'), (CS, '┘'),                  // C# floor
        (D,  ' '), (D,  ' '),                             // D  2 exp
        (DS, '└'), (DS, '─'), (DS, '┘'),                  // D# floor
        (E,  ' '), (E,  ' '), (E,  '│'),                 // E  2 exp + right wall
        (F,  '│'), (F,  ' '), (F,  ' '), (F,  ' '),     // F  wall+3
        (FS, '└'), (FS, '─'), (FS, '┘'),                  // F# floor
        (G,  ' '), (G,  ' '),                             // G  2 exp
        (GS, '└'), (GS, '─'), (GS, '┘'),                  // G# floor
        (A,  ' '), (A,  ' '),                             // A  2 exp
        (AS, '└'), (AS, '─'), (AS, '┘'),                  // A# floor
        (B,  ' '), (B,  ' '), (B,  '│'),                 // B  2 exp + right wall
    ];

    // Rows 5–6: white key body — both left '│' and right '│' walls visible.
    #[rustfmt::skip]
    const LOWER: [Cell; 35] = [
        (C, '│'), (C, ' '), (C, ' '), (C, ' '), (C, '│'),
        (D, '│'), (D, ' '), (D, ' '), (D, ' '), (D, '│'),
        (E, '│'), (E, ' '), (E, ' '), (E, ' '), (E, '│'),
        (F, '│'), (F, ' '), (F, ' '), (F, ' '), (F, '│'),
        (G, '│'), (G, ' '), (G, ' '), (G, ' '), (G, '│'),
        (A, '│'), (A, ' '), (A, ' '), (A, ' '), (A, '│'),
        (B, '│'), (B, ' '), (B, ' '), (B, ' '), (B, '│'),
    ];

    // Row 7: white key floor — '└───┘' closes each white key.
    #[rustfmt::skip]
    const LOWER_BTM: [Cell; 35] = [
        (C, '└'), (C, '─'), (C, '─'), (C, '─'), (C, '┘'),
        (D, '└'), (D, '─'), (D, '─'), (D, '─'), (D, '┘'),
        (E, '└'), (E, '─'), (E, '─'), (E, '─'), (E, '┘'),
        (F, '└'), (F, '─'), (F, '─'), (F, '─'), (F, '┘'),
        (G, '└'), (G, '─'), (G, '─'), (G, '─'), (G, '┘'),
        (A, '└'), (A, '─'), (A, '─'), (A, '─'), (A, '┘'),
        (B, '└'), (B, '─'), (B, '─'), (B, '─'), (B, '┘'),
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

    // Two octaves: chain arrays × 2.
    let upper_row: Vec<Cell> = UPPER.iter().chain(UPPER.iter()).copied().collect();
    let upper_btm_row: Vec<Cell> = UPPER_BTM.iter().chain(UPPER_BTM.iter()).copied().collect();
    let lower_row: Vec<Cell> = LOWER.iter().chain(LOWER.iter()).copied().collect();
    let bottom_row: Vec<Cell> = LOWER_BTM.iter().chain(LOWER_BTM.iter()).copied().collect();

    let upper = render(&upper_row);
    let upper_btm = render(&upper_btm_row);
    let lower = render(&lower_row);
    let bottom = render(&bottom_row);

    println!();
    for _ in 0..3 {
        println!("  {upper}");
    } // rows 1–3: black key bodies
    println!("  {upper_btm}"); // row 4:   black key floors
    for _ in 0..2 {
        println!("  {lower}");
    } // rows 5–6: white key bodies
    println!("  {bottom}"); // row 7:   white key floors
    println!("{RESET}");
}
