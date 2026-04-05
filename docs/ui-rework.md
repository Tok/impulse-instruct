# UI/UX Rework Plan

Tracking the visual and interaction quality-of-life work needed after the v0.5.6 release.
See `assets/screenshots/Screenshot-0.5.6.png` for the baseline.

---

## Critical — blocks basic usability

### 1. Cables must not occlude module controls

**Problem:** Cables are drawn in `egui::Order::Foreground` with full opacity, crossing
directly over the BASS SYNTH, DRUM KIT A, and other voice module panels. You cannot
read knob values or click controls under an active cable.

**Options (pick one or combine):**

- **Separate patch view** (preferred): Tab toggles between a "play view" (panels, no
  cables) and a "patch view" (cables + jack sockets, no panel content). One mode at a
  time. Similar to Reason's rack flip.
- **Clip cables to the port junction strip** only: cables hang from the port row at the
  bottom/top of each module card and never cross the panel interior.
- **Draw cables behind module cards**: change layer order so cables render at
  `egui::Order::Background` and modules always sit on top.

Current: `rack_canvas.rs:181` uses `egui::Order::Foreground`.

### 2. Voice module card width is too narrow

**Problem:** `module_slot_w` assigns `full_w / PHI^2 ≈ 38.2 %` to most voice modules
(`rack_canvas.rs:397-402`). At a 1200 px window this is ~450 px — the BASS SYNTH
panel tries to fit oscillator, filter, envelope, LFO, and FX send knobs in that width,
resulting in very cramped rows. Two modules per row fill only 76 % of the width; three
overflow and wrap to 2+1.

**Fix:** Replace the static golden-ratio tiers with a responsive grid:

- Voice modules: minimum 420 px, expand to fill. Two per row at >= 840 px, three at
  >= 1260 px.
- FX modules: fixed ~220 px (just knobs + label, no sequence), 4-5 per row.
- Complex voice panels (BASS SYNTH, AN1X, HOOVER): may be `wide` (60 % of row) with
  the sequencer lane alongside, rather than standalone full-panel.

---

## Major — meaningfully degrades workflow

### 3. LLM strip takes too much vertical space

**Problem:** The log output area (model responses, status lines) defaults to a tall
fixed height, pushing the module rack below the visible viewport fold at typical
(1080p) screen heights.

**Fix:**

- Default LLM strip to 4 lines (≈ 64 px) collapsed, expand on click.
- The ASK button / prompt input always visible regardless of collapse state.
- Log text area height is already drag-resizable; add a collapse button to reset it
  to minimum.

### 4. Zones lack visual hierarchy

**Problem:** The zone rail labels (`GLOBAL`, `VOICES`, `FX + MOD`) are small dimmed
text strips. Modules in different zones read as an undifferentiated list of dark boxes.

**Fix:**

- Wider zone rail with a subtle left-border accent (grayscale, not color).
- Slightly different card background per zone: `DEEP` for voice, 1-2 counts darker for
  FX — still within the R=G=B rule.
- Zone labels: slightly larger monospace, all-caps, with a rule line across the full
  width.

### 5. Module card inner padding is too tight

**Problem:** Module content area has minimal margin; knob rows crowd the card edges.
The module title bar height is also slightly short, making the port circles appear
clipped.

**Fix:**

- Card content margin: 6 px sides, 8 px top/bottom (currently 4/4).
- Title bar height: 20 px minimum (currently 18 px).
- Port circles: increase `PORT_RADIUS` from 5.5 to 6.5, ensure they don't overlap
  the border stroke.

### 6. No affordance for drag-to-reorder

**Problem:** Modules can be dragged to reorder within their zone (via title bar drag)
but there is no visual hint that this is possible. Users don't discover it.

**Fix:**

- Show a grab-cursor (`CursorIcon::Grab`) when hovering the title bar.
- Draw three small horizontal dots (`···`) at the right side of the title bar as a
  drag handle icon.
- While dragging: highlight the target drop slot with a vertical insertion line.

---

## Quality of life

### 7. Keyboard shortcuts are invisible

The following shortcuts exist but are undocumented in the UI:

| Key | Action |
|-----|--------|
| Tab | Toggle cable visibility |
| Ctrl+Z | Undo |
| Shift+Ctrl+Z | Redo |
| Space | Play/stop |
| Ctrl+S | Save session |

**Fix:** Add a `?` icon in the header that opens a cheat-sheet popup. Or show shortcuts
in button tooltips (`ui.button(...).on_hover_text("Shortcut: Space")`).

### 8. Undo/redo has no visual indicator

`StateHistory` exists but there is no in-UI counter ("12 steps" / "no undo history").
Pressing Ctrl+Z with nothing to undo silently does nothing.

**Fix:** Show undo depth as a small number badge next to the undo button, or in the
header alongside Save.

### 9. Module enable/disable is unclear

Disabled modules show a dimmed card but the enabled/disabled toggle in the title bar
is a tiny circle with no label. Users don't notice it.

**Fix:**

- Label the toggle `ON` / `OFF` when hovered (tooltip).
- When a module is disabled: dim the entire card interior by 50 % (overlay a
  semi-transparent rect rather than just reducing opacity of the title bar).
- Show `[MUTED]` in the title bar text when disabled.

### 10. No per-zone collapse

The FX+MOD zone currently has 10+ module cards which can crowd the view.

**Fix:** Zone rail click collapses / expands the entire zone. Store collapse state
in `UiPrefs`.

### 11. Scrollbar friction

The rack canvas `ScrollArea` uses `drag_to_scroll(false)` (to prevent accidental scroll
during cable drag) but this breaks touchpad two-finger scroll on some platforms.

**Fix:** Only disable drag-to-scroll while a cable drag is in progress (`cable_drag.is_some()`).

### 12. Context menu is bare

Right-click on the rack shows "ADD MODULE" → submenus. No other actions.

**Fix:** Right-click on a specific module card should show:
- Enable / Disable
- Remove
- Duplicate (for FX modules that allow_multiple)
- Move to top / Move to bottom of zone

---

## Skeuomorphic widget pass

The current widget set in `src/ui/widgets/mod.rs` is functional but visually flat.
Every control needs physical depth — the feeling of something you could reach out and touch.

### Knobs (current: flat circle + arc)

`draw_knob()` at `widgets/mod.rs:156` draws:
- Filled circle in `PIT`/`SLATE`
- A thin ring stroke
- An arc track + pointer dot

**Target: machined aluminium cap sitting in a recessed well**

```
Layers (bottom to top):
  1. Well shadow — dark filled circle, slightly larger, offset 1-2 px down-right
  2. Well ring — dark border at the recess edge
  3. Cap body — filled circle with radial gradient:
       top-left quadrant: CHALK (160) — catch-light from above-left
       centre: ASH (90)
       bottom-right: PIT (28) — cast shadow
  4. Indicator line — from centre outward, CHALK, 1.5 px thick
       (replaces the pointer dot — a physical groove/mark)
  5. Top specular ring — partial arc, 12 o'clock to 2 o'clock, GHOST alpha 60
  6. Mode indicator: small dot near rim, not centre-text
```

egui doesn't have radial gradients, so simulate with:
- `circle_filled` in `PIT` (large, offset)
- `circle_filled` in body color (ASH)
- A short `line` from center toward top-left in CHALK (simulates the bright catch-light)
- `circle_stroke` in CHALK alpha-40 for the specular arc

### Step buttons (current: flat rect, active = brighter gray)

`step_button()` at `widgets/step.rs` fills a rect with `IRON` or `ACTIVE_STEP`.

**Target: rubber pad / velocity-sensitive button**

```
Inactive (raised):
  - Background: IRON fill
  - Top/left edge: 1px SLATE (highlight)
  - Bottom/right edge: 1px deep shadow (near-black)
  - Result: looks raised, pressable

Active (pressed):
  - Background: ACTIVE_STEP (brighter)
  - Top/left edge: 1px shadow (depressed — inverted)
  - Bottom/right edge: 1px highlight
  - Optional: small inset shadow rect (2px margin, darker fill)
  - Result: looks punched in

Current step (playhead flash):
  - GHOST border, maybe a 1px bright inner rect on all edges
```

This is a standard neumorphic emboss/deboss pattern — the `emboss.rs` file already
has some of this. Extend it to step buttons.

### XY pad (current: plain rect with crosshair lines)

**Target: recessed rubber playing surface**

```
  1. Outer frame: raised bezel — lighter top/left edge, darker bottom/right
  2. Inner surface: VOID (near-black), slight inset shadow rect
  3. Grid lines: very faint (IRON alpha 60), 4x4 divisions
  4. Cursor: bright CHALK circle, drop shadow below, no fill — ring only
  5. Active drag: pulse the cursor ring (brightness oscillation)
```

### ADSR visualizer (current: plain polygon fill)

**Target: oscilloscope-style inset screen**

```
  1. Outer bezel: raised, same as XY pad frame
  2. Screen background: VOID with a very faint green-tinted (or just dark) fill
     NOTE: must remain R=G=B if tinted. Use 4,4,4 near-black, not green.
  3. ADSR curve: FOG line, 1.5px
  4. Scan-line overlay: horizontal lines every 3px, 5% opacity black
     (simulates CRT phosphor grid without actual color)
  5. Corner vignette: radial dark overlay at all 4 corners
```

### Sliders (current: flat track + thumb)

**Target: machined channel with a sliding cap**

```
Track:
  - VOID fill, recessed (top/left lighter border, bottom/right darker)
  - Track is a 4px tall groove

Thumb:
  - Raised cap shape (rect with rounded ends)
  - Gradient: CHALK top, ASH body, PIT bottom
  - Bottom shadow
  - Top specular line
```

### Panels / group backgrounds

Between widgets, panels currently use flat `DEEP` fill.

**Target: brushed panel surface**

```
  - Section headers (CUT, ENV, etc.): slight top shadow below the label
    (separator line in SLATE, then 1px VOID gap, then content)
  - Control groups: very subtle inset rect (1px darker border on top/left,
    1px lighter on bottom/right) to suggest sub-panels recessed into the face plate
  - No gradients — keep it to 1px rule lines only, all grayscale
```

---

## Bigger layout rethink (post-QoL)

Once the critical and major issues are fixed, consider:

**Horizontal split layout:**
Left panel = voice modules (tabs: BASS / 808 / 909 / HOOVER / AN1X), always visible.
Right panel = sequencer + FX, always visible.
LLM strip: bottom bar, single line by default, expands upward.

This mirrors the classic hardware layout: instruments on the left, effects and
sequencer on the right. No scrolling needed for the main workflow.

**Detachable panels:** Drag any module card out into a floating window (eframe supports
this with `egui::Window`). Power users can arrange a custom layout.

---

## Reference: files to change

| File | Work needed |
|------|-------------|
| `src/ui/rack_canvas.rs` | Cable layer order, zone rail styling, scroll fix |
| `src/ui/module_card.rs` | Card padding, title bar height, port radius, drag affordance |
| `src/ui/panels/bass.rs` | Knob layout at narrower widths |
| `src/ui/llm_strip.rs` | Collapse toggle, default height |
| `src/ui/header.rs` | Shortcut cheat-sheet button |
| `src/state/mod.rs` | `UiPrefs`: zone collapse flags |
