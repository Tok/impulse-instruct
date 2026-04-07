# Farbige Noten - Color Theory Integration

> Ch. A. B. Huth - *Farbige Noten*, Hamburg 1888–1889.
> Three-volume treatise linking the chromatic Western scale to a 12-color
> system derived from the **golden ratio** applied simultaneously to tone
> scale and color scale (*Farbenleiter*).

---

## What the Three Volumes Contain

| Volume | Year | Title | Key Content |
|--------|------|-------|-------------|
| 1 | 1888 | *Farben und Noten* | Foundational theory; piano notation plates (6 Farbtafeln); establishes the 12-color chromatic cycle with international color-name abbreviations |
| 2 | 1889 | *Weiterer Verfolg der neuen Theorie auf das Tonsystem* | Harmonic and compositional theory; TAFEL I - the large spectral "Entwicklung der Tonleiter" chart; TAFEL I (notation) - colored musical examples |
| 3 | - | *Vorschlag eines neuen vereinfachten Notensystems* | Simplified notation proposal; **TAFEL III** - the definitive piano keyboard color chart; TAFEL IV - chord grids for all major/minor keys; TAFEL V - "Farben u. Noten" prism diagram; TAFEL VI - color ring (*Kalte / Warme Töne*) |

---

## Core Theory

Huth observed that applying the **golden section** (φ = 1.618…) to both the
tone scale and the color scale yields the same proportional cut-points. On
the tone scale these intersections produce the harmonic consonances (major
and minor thirds, fifths). On the color scale they produce harmonic color
triads - what Huth calls *harmonische Tricoloren*.

The color wheel he uses is the **traditional RYB wheel** (Red = 0°,
Yellow = 120°, Blue = 240°), which was standard in 19th-century European
color science. This is important for interpreting his color names: his
*"Grün"* / *"Vert"* corresponds to the RYB 180° sector (cyan-green in
modern RGB), and his *"Gelb"* / Yellow maps to the RYB 120° sector
(yellow-green in modern RGB).

**Key structural rule:**
The 12 chromatic semitones each correspond to one 30° step **counter-
clockwise** on the RYB hue wheel, starting from Blue at C. One full octave
= 360° = a complete circuit of the hue wheel. The octave C above returns to
the same blue, confirming cyclic closure.

**Cold / Warm division** (TAFEL VI):
- *Kalte Töne* (cold tones): Blue → Cyan → Green (RYB Green ≈ teal-cyan)
- *Warme Töne* (warm tones): Yellow → Orange → Red
- The transition through Rose → Carmine → Lilac → Indigo bridges warm back
  to cold.

**Complementary pairs** (TAFEL VI, *Complementäre Farbtöne*):
Standard RYB complementaries map to the tritone interval (6 semitones apart,
180° on the hue wheel):
- Blue (C) ↔ Orange (F#) - tritone
- Green/Teal (D) ↔ Carmine (G#) - tritone
- Yellow (E) ↔ Violet/Purple (A#) - tritone

---

## The 12-Color Note Mapping

Huth's international abbreviations (Vol. 1, system plate bottom line):

> **BLU. SE. VER. MO. GEL. OR. NER. ROS. CAR. LIL. PEN. IN. BLU.**

13 entries = 12 unique colors + octave repeat of the first (C → C').

| # | Note | Abbrev. | Full Name | English | Hue (RYB) | Hex |
|---|------|---------|-----------|---------|-----------|-----|
| 0 | C   | BLU | Blau / Blue | Blue | 240° | `#3366DD` |
| 1 | C#  | SE  | Seegrün (Sea-green) | Cyan-Blue | 210° | `#2299BB` |
| 2 | D   | VER | Vert (French green) | Green / Teal | 180° | `#33AA66` |
| 3 | D#  | MO  | (Yellow-Green / Mordoré?) | Yellow-Green | 150° | `#88CC22` |
| 4 | E   | GEL | Gelb (German yellow) | Yellow | 120° | `#DDCC22` |
| 5 | F   | OR  | Orange | Orange | 60° | `#EE8822` |
| 6 | F#  | NER | (Vermilion / dark red-orange) | Vermilion | 30° | `#DD4422` |
| 7 | G   | ROS | Rose | Rose-Pink | 350° | `#EE3366` |
| 8 | G#  | CAR | Carmin (Carmine) | Carmine | 320° | `#CC1144` |
| 9 | A   | LIL | Lila (German lilac) | Lilac-Violet | 290° | `#9966CC` |
|10 | A#  | PEN | Pensée (French pansy) | Purple-Violet | 265° | `#7744BB` |
|11 | B   | IN  | Indigo | Indigo | 245° | `#4433AA` |
|12 | C'  | BLU | (octave repeat) | Blue | 240° | `#3366DD` |

> **Note on MO and NER:** These two abbreviations remain uncertain.
> "MO" falls between Green and Yellow; "Mordoré" (dark gold-green, French)
> or "Moos" (German moss) are plausible. "NER" falls between Orange and Rose;
> the most likely referent is a dark vermilion or red-orange ink name.
> The hue positions and hex values above are derived from the RYB 30°-per-
> semitone rule and the surrounding named colors.

---

## Visual Evidence

### TAFEL III (Vol. 3) - Definitive Piano Color Chart
The primary color reference. Shows a piano keyboard with one octave of
colored vertical columns. The 7 natural-note (white-key) columns, left to
right (C → B), display a smooth blue-to-orange-red spectrum. The 5
accidental (black-key) strips show intermediate shades.

As a 135-year-old chromolithograph the printed inks have shifted
significantly. The hex values above are derived from **Huth's text-stated
color names** rather than from direct pipetting of the aged image.

### TAFEL V (Vol. 3) - "Farben u. Noten" Prism Diagram
Spectral prism diagram showing the **physical** correspondence between
light-wave frequency and pitch across the full piano range (not one octave).
Lowest notes (bass) → red-orange; highest notes (treble) → blue-violet.
This is the physical mapping; the cyclic 12-color system in TAFEL III is the
*symbolic* mapping used in the notation.

### TAFEL VI (Vol. 3) - Color Ring
Color wheel showing *Kalte Töne* (cool: upper half) and *Warme Töne* (warm:
lower half), plus complementary color pairs. Confirms the warm/cool grouping
and the tritone-complementary relationship.

### TAFEL I (Vol. 2) - "Entwicklung der Tonleiter"
Large spectral chart showing the harmonic scale derivation with colored
horizontal bands (one per note) across five octaves.

---

## Integration in Impulse Instruct

The Huth palette is defined in `src/ui/theme.rs` as `NOTE_COLORS: [Color32; 12]`
and the helper `note_color(midi_note: u8) -> Color32`.

### Sequencer step cells

Active bass, hoover, and AN1X step buttons show the note name (e.g. "C4") in
its Huth color above the velocity dot.  The color gives instant visual feedback
about the melody shape before you listen — an interval of a fifth apart produces
complementary colors (tritone = exact opposite on the wheel).  Implemented in
`huth_note_cell()` in `src/ui/widgets/step.rs`; only shown when pad size >= 26px.

### Piano display

The piano keyboard at the bottom of the UI uses Huth colors for both white and
black key labels.  Each key's label is rendered in its chromatic color rather than
flat gray.  Modes: Off / Piano / Full (Full shows Huth-colored U-cup notation on
melodic sequencer rows).

### Log output (UI and terminal)

Note names (C4, A#3), frequencies (440 Hz), and MIDI numbers (note 60) in the
LLM response log are highlighted with their Huth color.  This applies both to
the in-UI log (`colorize_log()` in `src/ui/llm_strip.rs`) and to the terminal
via ANSI 24-bit color escape codes (`ansi_colorize_notes()` in
`src/ui/note.rs`).  Word-boundary guards prevent false positives on strings like
"D&B" or "E-flat"; quality-word extensions color "A minor" and "G major" as
single spans.

### Future possibilities

- **Piano roll note editor** — tint each semitone row with its Huth color
- **Waveform display** — map dominant pitch to Huth color via pitch detection
- **Chord grid (TAFEL IV style)** — visualise harmonic color triads
- **MIDI CC output** — send Huth color data as CC values for downstream
  color-coding in Ableton, Max/MSP, etc.

---

## Original Source

Ch. A. B. Huth, *Farbige Noten*, Hamburg 1888–1889.  Three volumes held at
the International Music Score Library Project:

[IMSLP: Farbige Noten (Huth, Ch. A. B.)](https://imslp.org/wiki/Farbige_Noten_(Huth,_Ch._A._B.))
