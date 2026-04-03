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
    // '|' chars use a dark foreground over the key background, marking:
    //   - Left and right walls of each black key
    //   - Left wall of each white key (lower zone)
    //   - E/F and B/C boundaries in the upper zone (no black key there)
    //
    // UPPER ROW breakdown per octave (35 cells total):
    //   C(3)  C#(|_|)  D(2)  D#(|_|)  E+boundary(4)
    //   F(3)  F#(|_|)  G(2)  G#(|_|)  A(2)  A#(|_|)  B+boundary(4)
    //
    // LOWER ROW: each of 7 white keys = '|' + 4 spaces (5 cells)
    //   Keyboard closes with an extra C: '|' + 4 spaces + closing '|'

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
    // Black keys are 3 wide with '|' walls; white exposed regions show bare color.
    // E/F boundary at index 14, B/C boundary at index 34.
    #[rustfmt::skip]
    const UPPER: [Cell; 35] = [
        (C,  ' '), (C,  ' '), (C,  ' '),            // C exposed          (0-2)
        (CS, '|'), (CS, ' '), (CS, '|'),             // C# walls           (3-5)
        (D,  ' '), (D,  ' '),                         // D exposed          (6-7)
        (DS, '|'), (DS, ' '), (DS, '|'),              // D# walls           (8-10)
        (E,  ' '), (E,  ' '), (E,  ' '), (E,  '|'),  // E exposed + E/F |  (11-14)
        (F,  ' '), (F,  ' '), (F,  ' '),              // F exposed          (15-17)
        (FS, '|'), (FS, ' '), (FS, '|'),              // F# walls           (18-20)
        (G,  ' '), (G,  ' '),                         // G exposed          (21-22)
        (GS, '|'), (GS, ' '), (GS, '|'),              // G# walls           (23-25)
        (A,  ' '), (A,  ' '),                         // A exposed          (26-27)
        (AS, '|'), (AS, ' '), (AS, '|'),              // A# walls           (28-30)
        (B,  ' '), (B,  ' '), (B,  ' '), (B,  '|'),  // B exposed + B/C |  (31-34)
    ];

    // Lower zone: 35 cells per octave.
    // Each white key = left '|' + 4 spaces.
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

    // Closing C5 cells appended after two octave repeats.
    const UPPER_C5: [Cell; 3] = [(C, ' '), (C, ' '), (C, ' ')];
    const LOWER_C5: [Cell; 6] = [
        (C, '|'), (C, ' '), (C, ' '), (C, ' '), (C, ' '), (C, '|'),
    ];

    // Render a slice of cells to an ANSI string.
    // Space cells → background-color-only; '|' cells → dark fg over key bg.
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

    // Build two-octave rows by chaining UPPER/LOWER twice then the C5 caps.
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
    for _ in 0..2 { println!("  {upper}"); }
    for _ in 0..3 { println!("  {lower}"); }
    println!("{RESET}");
}
