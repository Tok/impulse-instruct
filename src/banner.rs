// ─── banner.rs ────────────────────────────────────────────────────────────────
// Terminal startup banner: ASCII art title, tagline, Huth-colored keyboard.
// Pure function — only side effect is printing to stdout.

pub fn print_banner() {
    // ── ASCII art (pyfiglet slant font) ──────────────────────────────────────
    const GRAY: &str = "\x1b[38;2;120;120;120m";
    const DIM: &str = "\x1b[38;2;50;50;50m";
    const RESET: &str = "\x1b[0m";

    println!("{GRAY}    ______  _______  __  ____   _____ ______{RESET}");
    println!("{GRAY}   /  _/  |/  / __ \\/ / / / /  / ___// ____/{RESET}");
    println!("{GRAY}   / // /|_/ / /_/ / / / / /   \\__ \\/ __/   {RESET}");
    println!("{GRAY} _/ // /  / / ____/ /_/ / /______/ / /___   {RESET}");
    println!("{GRAY}/___/_/  /_/_/    \\____/_____/____/_____/   {RESET}");
    println!();
    println!("{GRAY}    _____   _________________  __  ______________{RESET}");
    println!("{GRAY}   /  _/ | / / ___/_  __/ __ \\/ / / / ____/_  __/{RESET}");
    println!("{GRAY}   / //  |/ /\\__ \\ / / / /_/ / / / / /     / /   {RESET}");
    println!("{GRAY} _/ // /|  /___/ // / / _, _/ /_/ / /___  / /    {RESET}");
    println!("{GRAY}/___/_/ |_//____//_/ /_/ |_|\\____/\\____/ /_/{RESET}");
    println!();

    // ── Tagline ───────────────────────────────────────────────────────────────
    println!("{DIM}  a synthesizer with a tiny LLM living inside it  ·  rust  ·  llama.cpp{RESET}");

    // ── Huth-colored keyboard (2 octaves C3–C5) ──────────────────────────────
    //
    // Layout: 4 terminal cells per white key, 2 cells per black key.
    // Each cell = 1 space with ANSI 24-bit background color.
    // One octave = 28 cells (3+2+2+2+3 + 3+2+2+2+3 = 7 white keys × 4 = 28).
    //
    // UPPER ROW (black-key zone): shows black key color where a black key sits,
    //   white key color everywhere else — no dark squares between black keys.
    //
    //   pos  0,1,2   → C  (exposed left of C before C# arrives)
    //   pos  3,4     → C# (black key straddles C/D boundary)
    //   pos  5,6     → D  (exposed center of D between C# and D#)
    //   pos  7,8     → D# (black key straddles D/E boundary)
    //   pos  9,10,11 → E  (E is fully exposed — no black key between E and F)
    //   pos 12,13,14 → F  (F exposed before F#)
    //   pos 15,16    → F# (black key straddles F/G boundary)
    //   pos 17,18    → G  (exposed center of G)
    //   pos 19,20    → G# (black key)
    //   pos 21,22    → A  (exposed center of A)
    //   pos 23,24    → A# (black key)
    //   pos 25,26,27 → B  (B fully exposed — no black key between B and C)
    //
    // LOWER ROW (white-key zone): each white key gets all 4 cells.
    //   pos  0..3  → C
    //   pos  4..7  → D
    //   pos  8..11 → E
    //   pos 12..15 → F
    //   pos 16..19 → G
    //   pos 20..23 → A
    //   pos 24..27 → B

    type Rgb = (u8, u8, u8);
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

    // Upper row: 28 colors per octave, index = terminal cell position
    const UPPER: [Rgb; 28] = [
        C, C, C,        // pos  0-2  : C exposed
        CS, CS,         // pos  3-4  : C# black key
        D, D,           // pos  5-6  : D exposed center
        DS, DS,         // pos  7-8  : D# black key
        E, E, E,        // pos  9-11 : E fully exposed
        F, F, F,        // pos 12-14 : F exposed
        FS, FS,         // pos 15-16 : F# black key
        G, G,           // pos 17-18 : G exposed center
        GS, GS,         // pos 19-20 : G# black key
        A, A,           // pos 21-22 : A exposed center
        AS, AS,         // pos 23-24 : A# black key
        B, B, B,        // pos 25-27 : B fully exposed
    ];

    // Lower row: 28 colors per octave
    const LOWER: [Rgb; 28] = [
        C, C, C, C,     // pos  0-3  : C (4 wide)
        D, D, D, D,     // pos  4-7  : D
        E, E, E, E,     // pos  8-11 : E
        F, F, F, F,     // pos 12-15 : F
        G, G, G, G,     // pos 16-19 : G
        A, A, A, A,     // pos 20-23 : A
        B, B, B, B,     // pos 24-27 : B
    ];

    // Build a single keyboard row string from a slice of RGB colors + optional extra C
    let build_row = |row: &[Rgb], extra_c: bool| -> String {
        let mut s = String::with_capacity(512);
        // Two octaves
        for _ in 0..2 {
            for &(r, g, b) in row {
                s.push_str(&format!("\x1b[48;2;{r};{g};{b}m \x1b[0m"));
            }
        }
        // Final C (4 cells for white row, 3 for upper row)
        if extra_c {
            let (r, g, b) = C;
            let n = if row.len() == 28 { 4 } else { 3 };
            for _ in 0..n {
                s.push_str(&format!("\x1b[48;2;{r};{g};{b}m \x1b[0m"));
            }
        }
        s
    };

    println!();
    let upper = build_row(&UPPER, true);
    let lower = build_row(&LOWER, true);
    // Print upper zone 2× for height, lower zone 3× for height
    for _ in 0..2 { println!("  {upper}"); }
    for _ in 0..3 { println!("  {lower}"); }
    println!("{RESET}");
}
