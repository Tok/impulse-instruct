// ─── banner.rs ────────────────────────────────────────────────────────────────
// Terminal startup banner: ASCII art title, tagline, Huth-colored keyboard.
// Pure function — only side effect is printing to stdout.

pub fn print_banner() {
    // ── ASCII art (pyfiglet slant font) ──────────────────────────────────────
    const GRAY: &str = "\x1b[38;2;120;120;120m";
    const DIM: &str = "\x1b[38;2;50;50;50m";
    const RESET: &str = "\x1b[0m";

    println!(
        "{GRAY}    ______  _______  __  ____   _____ ______{RESET}"
    );
    println!(
        "{GRAY}   /  _/  |/  / __ \\/ / / / /  / ___// ____/{RESET}"
    );
    println!(
        "{GRAY}   / // /|_/ / /_/ / / / / /   \\__ \\/ __/   {RESET}"
    );
    println!(
        "{GRAY} _/ // /  / / ____/ /_/ / /______/ / /___   {RESET}"
    );
    println!(
        "{GRAY}/___/_/  /_/_/    \\____/_____/____/_____/   {RESET}"
    );
    println!();
    println!(
        "{GRAY}    _____   _________________  __  ______________{RESET}"
    );
    println!(
        "{GRAY}   /  _/ | / / ___/_  __/ __ \\/ / / / ____/_  __/{RESET}"
    );
    println!(
        "{GRAY}   / //  |/ /\\__ \\ / / / /_/ / / / / /     / /   {RESET}"
    );
    println!(
        "{GRAY} _/ // /|  /___/ // / / _, _/ /_/ / /___  / /    {RESET}"
    );
    println!(
        "{GRAY}/___/_/ |_//____//_/ /_/ |_|\\____/\\____/ /_/{RESET}"
    );
    println!();

    // ── Tagline ───────────────────────────────────────────────────────────────
    println!(
        "{DIM}  a synthesizer with a tiny LLM living inside it  ·  rust  ·  llama.cpp{RESET}"
    );

    // ── Huth-colored keyboard (2 octaves, C3–C5) ─────────────────────────────
    // Note colors as (R, G, B)
    const C:  (u8, u8, u8) = (0x33, 0x66, 0xDD);
    const CS: (u8, u8, u8) = (0x22, 0x99, 0xBB);
    const D:  (u8, u8, u8) = (0x33, 0xAA, 0x66);
    const DS: (u8, u8, u8) = (0x88, 0xCC, 0x22);
    const E:  (u8, u8, u8) = (0xDD, 0xCC, 0x22);
    const F:  (u8, u8, u8) = (0xEE, 0x88, 0x22);
    const FS: (u8, u8, u8) = (0xDD, 0x44, 0x22);
    const G:  (u8, u8, u8) = (0xEE, 0x33, 0x66);
    const GS: (u8, u8, u8) = (0xCC, 0x11, 0x44);
    const A:  (u8, u8, u8) = (0x99, 0x66, 0xCC);
    const AS: (u8, u8, u8) = (0x77, 0x44, 0xBB);
    const B:  (u8, u8, u8) = (0x44, 0x33, 0xAA);
    const DARK: (u8, u8, u8) = (0x0E, 0x0E, 0x0E);

    // Helper: produce a background-colored 2-space block
    let bg = |c: (u8, u8, u8)| -> String {
        format!("\x1b[48;2;{};{};{}m  \x1b[0m", c.0, c.1, c.2)
    };

    // Row 1: black key tops row (per octave: 21 chars = 21 positions)
    // Pattern per octave (each position = 1 char = 2 terminal cells via 2-space block):
    //  0   : dark (C area)
    //  1   : C# color
    //  2   : dark (D gap)
    //  3   : D# color
    //  4,5 : dark (E area, no black key between E-F)
    //  6   : dark (F gap)
    //  7   : F# color
    //  8   : dark (G gap)
    //  9   : G# color
    //  10  : dark (A gap)
    //  11  : A# color
    //  12,13: dark (B area, no black key between B-C)
    let black_row_octave = || -> String {
        format!(
            "{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
            bg(DARK), bg(CS),  bg(DARK), bg(DS),
            bg(DARK), bg(DARK),
            bg(DARK), bg(FS),  bg(DARK), bg(GS),
            bg(DARK), bg(AS),  bg(DARK), bg(DARK),
        )
    };

    // Row 2: white keys row — C D E F G A B (3 chars per white key)
    // Each white key = 3 terminal "blocks" (3 * 2 = 6 chars wide)
    let white_key = |c: (u8, u8, u8)| -> String {
        format!(
            "{}{}{}",
            bg(c), bg(c), bg(c)
        )
    };
    let white_row_octave = || -> String {
        format!(
            "{}{}{}{}{}{}{}",
            white_key(C), white_key(D), white_key(E),
            white_key(F), white_key(G), white_key(A),
            white_key(B),
        )
    };

    // Final C5 (just one white key)
    let final_c = white_key(C);

    println!();

    // Print black-key row twice (visual height)
    let black_oct1 = black_row_octave();
    let black_oct2 = black_row_octave();
    for _ in 0..2 {
        println!("  {}{}{}", black_oct1, black_oct2, RESET);
    }

    // Print white-key row twice
    let white_oct1 = white_row_octave();
    let white_oct2 = white_row_octave();
    for _ in 0..2 {
        println!("  {}{}{}{}", white_oct1, white_oct2, final_c, RESET);
    }

    println!("{}", RESET);
}
