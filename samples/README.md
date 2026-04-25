# Samples

Drop-in directory for user-provided audio samples (WAV, 44.1 kHz).

The app scans sub-folders at runtime — nothing is bundled with the
binary, so pull whichever packs you like and unzip them here.

## Amen breaks

The `AMEN` sampler module reads `.wav` files from `samples/amen/`.
Good free sources on the Internet Archive:

- https://archive.org/details/amen-breaks
- https://archive.org/details/amen-breaks-compilation

Download a `.zip`, extract the WAVs into `samples/amen/`, and the
module's file picker will list them automatically.  The module plays
one sample at a time — pitch-shift and loop mode are controlled from
its panel.

## Textures (granular voice)

The `GRAN` granular texture module reads `.wav` files from
`samples/textures/`.  Unlike the amen sampler, the granular voice is
happier with longer, slower material — pads, field recordings, drones,
vowel tones, ambient loops.  Anything with slowly-evolving spectral
content grains well.

Archive.org collections worth poking at:

- https://archive.org/details/opensource_audio — huge mixed-bag of
  speech, field recordings, radio drama, music (varying licenses;
  check each item).
- https://archive.org/details/audio_ambient — ambient and drone
  uploads.

Outside of archive.org:

- https://freesound.org — the canonical source for short one-shots
  and field recordings.  CC-licensed; filter for CC0 if you want the
  least friction.  Grain-friendly search terms: *drone*, *pad*,
  *texture*, *field*, *reverb tail*.

Same workflow as amen: drop `.wav` files into `samples/textures/`
and the module's picker will list them.  Size doesn't matter much —
the voice only reads small random windows at a time.

## Wavetables

The `WAVETABLE` voice reads `.wav` files from `samples/wavetables/`.
Files are interpreted as Serum-style single-cycle frame stacks: the
buffer is split into 2048-sample frames at load time, and the
voice's POSITION knob scans through them.  A 32-frame Serum
wavetable comes out as 32 × 2048 = 65 536 samples in the WAV.

Free wavetable sources:

- https://wavetables.com — large CC0 collection of Serum-format
  tables.
- https://waveedit.online — browse + download single-cycle stacks.
- Adventure Kid's free single-cycles: https://www.adventurekid.se/akrt/

Drop `.wav` files into `samples/wavetables/` and load via the
`LOAD` button on the WAVETABLE card or `/api/wavetable`.  Files
that aren't a multiple of 2048 samples are tolerated — extra
samples at the tail are dropped.

## Impulse responses (convolution reverb)

The `CONV REV` convolution-reverb module reads `.wav` files from
`samples/impulses/`.  An impulse response (IR) captures the tail of a
real space — a cathedral, hall, plate, or hardware spring — and lets
the reverb recreate that space's acoustic signature when you convolve
a dry signal against it.

Archive.org and public-domain IR packs worth checking:

- https://archive.org/details/ir-library — mixed IR library covering
  halls, plates, and outdoor spaces.
- https://openairlib.net — academic IR archive from the University of
  York; rooms, cathedrals, odd spaces.  Filter by CC license.
- https://www.voxengo.com/impulses/ — small Voxengo free pack (mostly
  rooms and plates).  Dual-use for mono/stereo.

Workflow: drop `.wav` files (any sample rate; the loader resamples to
the engine rate) into `samples/impulses/` and use the `LOAD IR` button
on the ConvReverb card, or hit `/api/conv_reverb` with `{ "random":
true }`.  Short IRs (0.5–2 s) work best for musical reverb; longer
tails (3+ s) dominate the mix unless MIX is kept low.

## Pitched samples + multisamples (SAMPLER+)

The `SAMPLER+` module (`SampleInstrument`) reads pitched recordings
from `samples/instruments/`.  Two flavours:

- **Single `.wav`** — one recording mapped across the keyboard via
  ratio resampling.  Auto-detect-root sets the root note on load.
- **`.sfz` multisample** — a text manifest mapping several samples
  to different key + velocity ranges, with optional round-robin.
  The parser supports the opcode subset listed in
  `src/state/sfz.rs` (key range, velocity layers, loop points,
  ADSR, filter, RR).  See `samples/instruments/README.md` for
  authoring notes + free CC-licensed pack sources (Salamander Grand,
  Sonatina, VSCO 2 CE, sfzInstruments).

Drop files into `samples/instruments/` (or a sub-folder per pack)
and load via `LOAD` on the SAMPLER+ card or `/api/sample`.  The
zone-map strip at the bottom of the card visualises SFZ region
coverage; the waveform thumbnail shows the active single-WAV.
