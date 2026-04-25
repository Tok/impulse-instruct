# Sample-instrument pack

The `SAMPLER+` module (`SampleInstrument`) plays back pitched
recordings — single `.wav` files mapped to a root note, or `.sfz`
multisample banks where each region covers a different key range.

## Single-WAV mode

Drop a `.wav` file into `samples/instruments/` and load it via the
`LOAD` button on the SAMPLER+ card or `/api/sample`.  The module
auto-detects pitch on load (if confidence ≥ 0.5) and sets the root
note accordingly; the played note becomes a ratio-resampled version
of the source.  Loop start / end / enabled in the panel define a
sustain window for held notes.

Best for:
- Single drum hits (kick, snare, hat, fx one-shots)
- Vocal phrases / chops
- Instrument single-shots (piano A4, marimba C5, etc.)
- Breakbeats meant to be played at a single pitch

## SFZ multisample mode

`.sfz` files are text manifests that map several `.wav`s across the
keyboard, often with velocity layers + round-robin variants.  The
parser supports the subset listed in `src/state/sfz.rs` — enough to
load most CC0 piano / orchestral packs.

Drop a `.sfz` file (and its referenced `.wav`s) into
`samples/instruments/`, load via `LOAD`, and the SAMPLER+ panel's
zone-map strip shows which keys cover which regions.

### Free CC-licensed sources

These are good "starter pack" candidates — drop the `.sfz` + samples
into `samples/instruments/<library_name>/` and load.

- **Salamander Grand Piano** (CC-BY 3.0) — the canonical free piano
  SFZ.  Multi-velocity layers, multi-mic.
  https://github.com/sfzinstruments/SalamanderGrandPiano
- **Sonatina Symphonic Orchestra** (CC0) — strings, winds, brass;
  great starting point for orchestral textures.
  https://sso.mattiaswestlund.net/
- **VSCO 2 Community Edition** (CC0) — alternative orchestral pack
  by Versilian Studios.
  https://vis.versilstudios.com/vsco-community.html
- **sfzInstruments collection** — community-maintained index of CC
  SFZ packs.
  https://github.com/sfzinstruments

### Authoring custom SFZs

The parser handles `<global>` / `<group>` / `<region>` headers with
opcode cascading.  Supported opcodes: `sample`,
`lokey`/`hikey`/`pitch_keycenter` (note names like `c4` or integer
MIDI numbers both work), `lovel`/`hivel`, `loop_mode`/`loop_start`/
`loop_end`, `volume`, `pan`, `seq_position`/`seq_length`
(round-robin), `tune`, `transpose`, `ampeg_attack`/`decay`/`sustain`/
`release`, `cutoff`, `resonance`, `fil_type`.

Anything outside that subset is logged + ignored — partial-load
beats hard failure when a real-world pack uses opcodes the parser
doesn't model yet.

Minimal one-region SFZ:

```
<region>
sample=PianoC4.wav
lokey=48 hikey=72 pitch_keycenter=60
ampeg_attack=0.005 ampeg_release=0.4
```

## `starter/` sub-folder

Reserved for a curated CC0 starter pack so the module has audible
content out of the box.  Currently empty — drop your own `.sfz`s or
single-shot `.wav`s in here, or pull one of the linked libraries
above and unzip into `starter/`.
