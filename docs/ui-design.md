# Impulse Instruct - UI Design Rules

All UI code lives under `src/ui/`. The visual language is **neumorphic grayscale** -
dark surfaces, soft embossing, and a strict no-color rule for all chrome. Color appears
in exactly one place: Huth *Farbige Noten* note highlights.

---

## The Core Rule: Strict Grayscale Chrome

**Every UI color that is not a musical note highlight must satisfy R = G = B.**

No tints. No accent hues. No blue-grey "cool" backgrounds or warm amber hover states.
All panel backgrounds, borders, widget fills, text, shadows, and interactive states
are drawn from the grayscale palette in `src/ui/theme.rs`.

This is intentional and non-negotiable. The synth is dark studio gear - the palette
mirrors hardware (matte black panels, brushed aluminium knobs, white silk-screened
labels). Color in the UI would compete visually with the note colors, which carry
harmonic meaning. When note colors appear they must be the *only* color on screen.

---

## Palette (`src/ui/theme.rs`)

| Constant | RGB | Role |
|----------|-----|------|
| `VOID` | `(8, 8, 8)` | Near-black - window background |
| `DEEP` | `(18, 18, 18)` | Panel fill - the primary surface |
| `PIT` | `(28, 28, 28)` | Widget background (knob recesses, slider tracks) |
| `SLATE` | `(40, 40, 40)` | Borders, dividers |
| `IRON` | `(60, 60, 60)` | Inactive / disabled state |
| `ASH` | `(90, 90, 90)` | Mid-tone - inactive step buttons, secondary chrome |
| `SMOKE` | `(130, 130, 130)` | Secondary text, dim labels |
| `FOG` | `(175, 175, 175)` | Primary body text (default `override_text_color`) |
| `HAZE` | `(210, 210, 210)` | Bright text, active labels |
| `CHALK` | `(235, 235, 235)` | Highlights - knob specular, button top face |
| `GHOST` | `(255, 255, 255)` | Maximum brightness - active/hot elements, cursor |

`HOT`, `ACTIVE_STEP`, and `CURSOR` are aliases into the same range - keep them
pinned to grayscale values when updating.

**Rule for new colors:** If you need a new palette entry, it must satisfy
`r == g && g == b`. Pick a value that fills a gap in the existing ramp rather than
adding a tint.

---

## The Only Exception: Huth Note Colors

The twelve chromatic semitones each have a named color derived from
Ch. A. B. Huth's *Farbige Noten* (Hamburg 1888). These are the **only
non-grayscale colors permitted** in the UI.

```rust
// src/ui/theme.rs
pub const NOTE_COLORS: [Color32; 12] = [ /* C through B - see table below */ ];
pub fn note_color(midi_note: u8) -> Color32 { NOTE_COLORS[(midi_note % 12) as usize] }
```

| Note | Name | Hex |
|------|------|-----|
| C | Blue (BLU) | `#3366DD` |
| C# | Seegrün (SE) | `#2299BB` |
| D | Vert (VER) | `#33AA66` |
| D# | Yellow-green (MO) | `#88CC22` |
| E | Gelb (GEL) | `#DDCC22` |
| F | Orange (OR) | `#EE8822` |
| F# | Vermilion (NER) | `#DD4422` |
| G | Rose (ROS) | `#EE3366` |
| G# | Carmine (CAR) | `#CC1144` |
| A | Lila (LIL) | `#9966CC` |
| A# | Pensée (PEN) | `#7744BB` |
| B | Indigo (IN) | `#4433AA` |

These values must not be modified without updating `docs/colorful-notes.md`.
See that file for the full color theory and mapping rationale.

**Current usage:**
- Active bass/hoover/AN1X sequencer steps show note name in Huth color above the velocity dot.
- Piano display keys use their Huth chromatic color for labels.
- LLM log output colorizes note names (C4, A#3), frequencies (440 Hz), and MIDI numbers
  using Huth colors - both in-UI and via ANSI 24-bit terminal escape codes.
- The window icon (`make_window_icon()` in `main.rs`) uses Huth colors for the keyboard keys.
- Drum steps always pass `note_color: None` - no note tint on drum rows.

---

## Widget Style: Neumorphic Grayscale

The widget set (`src/ui/widgets.rs`) uses **soft neumorphism** - the illusion of
physical depth through highlight and shadow rather than flat color blocks.

### Knobs (`chrome_knob`)
- Circular recess drawn in `PIT` with a `SLATE` border
- Arc track: inactive in `IRON`, filled in `HAZE`/`CHALK`
- Pointer: `CHALK` line with a `SMOKE` glow halo
- No colored ring or LED glow - value communicated by arc position only

### Sliders (`glass_slider`)
- Track: `PIT` fill, `SLATE` border - looks recessed
- Thumb: `CHALK` with `SMOKE` shadow beneath, `GHOST` top specular
- No fill color between zero and thumb - the thumb position is sufficient

### Buttons (`embossed_button`)
- Idle: `ASH` face, `IRON` bottom-shadow, `SLATE` top-highlight - raised look
- Pressed: inverted shadows - face drops to `PIT`, looks pushed in
- Active/lit: face moves to `HAZE`, top specular at `CHALK`
- Never use a colored fill for active state - brightness alone signals on/off

### Step buttons (`step_button`)
- Inactive: `IRON` fill, `SLATE` border
- Active: `ACTIVE_STEP` fill, `CHALK` border
- Current (playhead): bright `GHOST` border flash
- Note dot (bass rows only): small filled circle using `note_color(note)`, centered in button - the *only* color allowed here

### LEDs (`led`)
- Off: `IRON` circle
- On: `CHALK`/`GHOST` - brightness only, no hue

### XY Pads
- Background: `PIT`; border: `SLATE`; crosshair: `IRON`
- Cursor: `CHALK` circle with `SMOKE` shadow
- Axis labels in `SMOKE`

### Oscilloscope
- Background: `VOID`; waveform: `FOG` line
- No colored waveform - the scope reads purely as a level/shape indicator

---

## Typography

All text is **monospace** (egui's built-in monospace font). This reinforces the
hardware-console aesthetic and ensures parameter labels align consistently.

| Style | Size | Role |
|-------|------|------|
| `Small` | 9 px | Tiny labels, step count indicators |
| `Body` / `Button` / `Monospace` | 11 px | Primary UI text |
| `Heading` | 13 px | Panel headers, tab labels |

Text colors:
- Panel headers: `HAZE`
- Active labels, knob names: `FOG`
- Secondary / dim labels: `SMOKE`
- Disabled: `IRON`

---

## Layout Principles

- **No rounded corners on panels** - flat-edged sections feel like rack hardware.
- **Widgets use `Rounding::same(3.0)` or `Rounding::same(2.0)`** - subtle, not pill-shaped.
- `item_spacing`: `(4, 4)` - dense, information-rich layout.
- `button_padding`: `(6, 3)` - compact but not cramped.
- `window_margin`: `8 px` uniform.
- All panels are dark (`DEEP`). No light-mode support - this is studio equipment.

---

## Parameter Lock States

Knobs and sliders reflect their lock mode visually, entirely in grayscale:

| Mode | Knob body | Arc / fill | Catch-light / rim | Animation |
|------|-----------|------------|-------------------|-----------|
| **Free** (default) | Normal (`PIT` / `SLATE`) | `FOG` | Normal (100) | None |
| **User-owned (U)** | Darker (body -12) | `IRON` dim | Dimmed (60) | None - feels "locked down" |
| **LLM Focus (F)** | Brighter (body +15) | `CHALK` bright | Shimmering (120-200, 1 Hz) | Slow pulse - "hot" |

The visual contrast tells users at a glance which knobs the LLM will touch (Free),
which are protected (U), and which the LLM is actively targeting (F).

**Interaction:** Alt+click any knob or slider to cycle through Free / U / F.

---

## Footer Mode Indicators

The footer strip shows three modifier indicators on the left:

- **Ctrl** - highlights when held; Ctrl+scroll wheel zooms (global or per-module)
- **Alt** - highlights when held; Alt+click cycles knob/slider lock mode
- **Tab:BACK** - highlights when rack is flipped to back panel view

All indicators have hover tooltips. The indicators light up from `IRON` (idle)
to `CHALK` (active), strictly grayscale.

---

## Agent Status in Header

The header bar shows a compact round-robin display after the HEAT slider: each
enabled agent appears as a dot + persona name. The active (inferring) agent
has a pulsing bright dot and `CHALK` text; idle agents show `IRON`.

---

## What Not to Do

- **No tinted backgrounds.** `Color32::from_rgb(20, 20, 35)` is a blue-tinted dark - use `DEEP` `(18,18,18)` instead.
- **No colored accent borders.** Active states are indicated by brightness, not hue.
- **No gradient fills on widgets** beyond the implicit highlight/shadow emboss.
- **No colored text.** Even error states use `CHALK` or `GHOST` brightness, not red.
- **No hover hue shift.** Hover states increase brightness only - `ASH → SMOKE`, `IRON → ASH`, etc.
- **No note colors on non-note elements.** Don't reuse Huth colors for FX states, LFO indicators, or anything outside the pitch domain.
- **No emojis.** Labels, tooltips, panel headers, and status messages use plain ASCII text only. The aesthetic is studio hardware, not a web app.
- **No em dashes.** Use ` - ` (space-hyphen-space) instead of `—`. This applies to all user-visible strings and all documentation in this repo.
